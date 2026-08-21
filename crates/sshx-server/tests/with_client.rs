use anyhow::{Context, Result};
use sshx_core::{
    proto::{server_update::ServerMessage, NewShell, TerminalInput},
    Sid, Uid,
};
use sshx_daemon::{controller::Controller, encrypt::Encrypt, runner::Runner};
use sshx_server::web::protocol::{WsClient, WsWinsize};
use tokio::time::{self, Duration};

use crate::common::*;

pub mod common;

#[tokio::test]
async fn test_handshake() -> Result<()> {
    let server = TestServer::new().await;
    let controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    controller.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_fixed_encryption_key() -> Result<()> {
    let server = TestServer::new().await;
    let controller = Controller::new_with_encryption_key(
        &server.endpoint(),
        "",
        Runner::Echo,
        false,
        Some("localdevkey"),
    )
    .await?;

    assert_eq!(controller.encryption_key(), "localdevkey");
    assert!(controller.url().ends_with("#localdevkey"));
    controller.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_command() -> Result<()> {
    let server = TestServer::new().await;
    let runner = Runner::Shell("/bin/bash".into());
    let mut controller = Controller::new(&server.endpoint(), "", runner, false).await?;

    let session = server
        .state()
        .lookup(controller.name())
        .context("couldn't find session in server state")?;

    let updates = session.update_tx();
    let new_shell = NewShell {
        id: 1,
        x: 0,
        y: 0,
        source_id: None,
        page_id: 1,
        rows: 24,
        cols: 80,
        width: 0,
        height: 0,
        background: String::new(),
        ssh_profile: None,
        theme: String::new(),
        working_directory: String::new(),
    };
    updates.send(ServerMessage::CreateShell(new_shell)).await?;

    let key = controller.encryption_key();
    let encrypt = Encrypt::new(key);
    let offset = 4242;
    let data = TerminalInput {
        id: 1,
        data: encrypt.segment(0x200000000, offset, b"ls\r\n").into(),
        offset,
    };
    updates.send(ServerMessage::Input(data)).await?;

    tokio::select! {
        _ = controller.run() => (),
        _ = time::sleep(Duration::from_millis(1000)) => (),
    };
    controller.close().await?;
    Ok(())
}

#[tokio::test]
async fn test_ws_missing() -> Result<()> {
    let server = TestServer::new().await;

    let bad_endpoint = format!("ws://{}/not/an/endpoint", server.local_addr());
    assert!(ClientSocket::connect(&bad_endpoint, "", None)
        .await
        .is_err());

    let mut s = ClientSocket::connect(&server.ws_endpoint("foobar"), "", None).await?;
    s.expect_close(4404).await;

    Ok(())
}

#[tokio::test]
async fn test_ws_basic() -> Result<()> {
    let server = TestServer::new().await;

    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let mut s = ClientSocket::connect(&server.ws_endpoint(&name), &key, None).await?;
    s.flush().await;
    assert_eq!(s.user_id, Uid(1));

    s.send(WsClient::CreateWindowed(
        2,
        2,
        714,
        518,
        24,
        80,
        1,
        "Dracula".into(),
    ))
    .await;
    s.flush().await;
    assert_eq!(s.shells.len(), 1);
    assert!(s.shells.contains_key(&Sid(1)));
    assert_eq!(s.shells.get(&Sid(1)).unwrap().theme, "Dracula");
    assert_eq!(s.shells.get(&Sid(1)).unwrap().width, 714);
    assert_eq!(s.shells.get(&Sid(1)).unwrap().height, 518);

    s.send(WsClient::Subscribe(Sid(1), 1, 0)).await;
    assert_eq!(s.read(Sid(1)), "");

    s.send_input(Sid(1), b"hello!").await;
    s.flush().await;
    assert_eq!(s.read(Sid(1)), "hello!");
    assert_eq!(s.chunk_replays, [(Sid(1), false)]);

    s.send_input(Sid(1), b" 123").await;
    s.flush().await;
    assert_eq!(s.read(Sid(1)), "hello! 123");
    assert_eq!(s.chunk_replays.last(), Some(&(Sid(1), false)));

    let mut viewer = ClientSocket::connect(&server.ws_endpoint(&name), &key, None).await?;
    viewer.flush().await;
    viewer.send(WsClient::Subscribe(Sid(1), 1, 0)).await;
    viewer.flush().await;
    assert_eq!(viewer.read(Sid(1)), "hello! 123");
    assert_eq!(viewer.chunk_replays, [(Sid(1), true)]);

    s.send(WsClient::CloneWindowed(
        Sid(1),
        42,
        62,
        714,
        518,
        24,
        80,
        1,
        "Tokyo Night".into(),
    ))
    .await;
    s.flush().await;
    assert_eq!(s.shells.len(), 2);
    assert_eq!(s.shells.get(&Sid(2)).unwrap().x, 42);
    assert_eq!(s.shells.get(&Sid(2)).unwrap().y, 62);
    assert_eq!(s.shells.get(&Sid(2)).unwrap().width, 714);
    assert_eq!(s.shells.get(&Sid(2)).unwrap().height, 518);
    assert_eq!(s.shells.get(&Sid(2)).unwrap().theme, "Tokyo Night");

    Ok(())
}

#[tokio::test]
async fn test_pages_and_live_note_editing() -> Result<()> {
    let server = TestServer::new().await;
    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let endpoint = server.ws_endpoint(&name);
    let mut writer = ClientSocket::connect(&endpoint, &key, None).await?;
    let mut viewer = ClientSocket::connect(&endpoint, &key, None).await?;
    writer.flush().await;
    viewer.flush().await;

    writer.send(WsClient::CreatePage(String::new())).await;
    writer.flush().await;
    viewer.flush().await;
    let page_id = writer.pages.last().unwrap().id;
    assert_eq!(writer.pages.len(), 2);
    assert_eq!(viewer.pages, writer.pages);

    writer
        .send(WsClient::RenamePage(page_id, "Review".into()))
        .await;
    writer.send(WsClient::Create(20, 40, page_id)).await;
    writer.send(WsClient::CreateNote(60, 80, page_id)).await;
    writer.flush().await;
    viewer.flush().await;
    assert_eq!(viewer.pages.last().unwrap().name, "Review");
    assert_eq!(viewer.shells.get(&Sid(1)).unwrap().page_id, page_id);
    assert_eq!(viewer.notes.get(&Sid(2)).unwrap().page_id, page_id);

    writer
        .send(WsClient::CreateFileWindow(
            Sid(1),
            page_id,
            "/tmp".into(),
            "Review shell".into(),
            120,
            160,
            1040,
            680,
        ))
        .await;
    writer.flush().await;
    viewer.flush().await;
    let mut file_window = writer.file_windows.get(&Sid(3)).unwrap().clone();
    assert_eq!(viewer.file_windows, writer.file_windows);
    file_window.x = 240;
    file_window.width = 1200;
    file_window.current_path = "/tmp/project".into();
    file_window.expanded_paths = vec!["/".into(), "/tmp".into(), "/tmp/project".into()];
    file_window.selected_path = "/tmp/project/Cargo.toml".into();
    file_window.selected_kind = "file".into();
    file_window.tree_scroll_top = 128;
    file_window.editor_path = file_window.selected_path.clone();
    file_window.editor_stream = 1 << 63;
    file_window.editor_data = b"encrypted editor buffer".as_slice().into();
    file_window.editor_dirty = true;
    writer
        .send(WsClient::UpdateFileWindow(
            Sid(3),
            page_id,
            Some(file_window.clone()),
        ))
        .await;
    writer.flush().await;
    viewer.flush().await;
    assert_eq!(viewer.file_windows.get(&Sid(3)), Some(&file_window));
    writer.send(WsClient::CreateNote(720, 80, page_id)).await;
    writer.flush().await;
    viewer.flush().await;
    assert!(viewer.notes.contains_key(&Sid(4)));

    let previous_errors = writer.errors.len();
    writer.send(WsClient::Move(Sid(1), 1, None)).await;
    writer.send(WsClient::UpdateNote(Sid(2), 1, None)).await;
    writer.send(WsClient::CloseNote(Sid(2), 1)).await;
    writer.send(WsClient::SetFocus(Some((Sid(1), 1)))).await;
    writer.send(WsClient::Subscribe(Sid(1), 1, 0)).await;
    writer.flush().await;
    assert_eq!(writer.errors.len(), previous_errors + 5);
    assert_eq!(writer.shells.get(&Sid(1)).unwrap().page_id, page_id);
    assert_eq!(writer.notes.get(&Sid(2)).unwrap().page_id, page_id);

    writer
        .send(WsClient::SetNoteEditing(Sid(2), page_id, true))
        .await;
    writer
        .send(WsClient::UpdateNoteText(Sid(2), page_id, "a".into()))
        .await;
    writer
        .send(WsClient::UpdateNoteText(Sid(2), page_id, "ab".into()))
        .await;
    writer
        .send(WsClient::UpdateNoteParagraphs(
            Sid(2),
            page_id,
            vec!["first line\nsecond line".into(), "next block".into()],
        ))
        .await;
    writer.flush().await;
    viewer.flush().await;
    assert_eq!(
        viewer.note_editors.get(&Sid(2)),
        Some(&(page_id, writer.user_id))
    );
    let mut linked_note = writer.notes.get(&Sid(2)).unwrap().clone();
    assert_eq!(
        linked_note.paragraphs,
        ["first line\nsecond line", "next block"]
    );
    linked_note.linked_shell_ids = vec![Sid(1)];
    linked_note.linked_note_ids = vec![Sid(4)];
    linked_note.linked_file_window_ids = vec![Sid(3)];
    writer
        .send(WsClient::UpdateNote(
            Sid(2),
            page_id,
            Some(linked_note.clone()),
        ))
        .await;
    writer.flush().await;
    viewer.flush().await;
    assert_eq!(viewer.notes.get(&Sid(2)), Some(&linked_note));
    let workspace = server.state().lookup(&name).unwrap().workspace_state();
    assert_eq!(workspace.pages.last().unwrap().name, "Review");
    assert_eq!(workspace.shells[0].page_id, page_id);
    let workspace_note = workspace.notes.iter().find(|note| note.id == 2).unwrap();
    assert_eq!(workspace_note.page_id, page_id);
    assert_eq!(workspace_note.paragraphs, linked_note.paragraphs);
    assert_eq!(workspace_note.linked_shell_ids, [1]);
    assert_eq!(workspace_note.linked_note_ids, [4]);
    assert_eq!(workspace_note.linked_file_window_ids, [3]);
    assert_eq!(workspace.file_windows[0].shell_id, 1);
    assert_eq!(workspace.file_windows[0].x, 240);
    assert_eq!(workspace.file_windows[0].current_path, "/tmp/project");
    assert_eq!(workspace.file_windows[0].tree_scroll_top, 128);
    assert!(workspace.file_windows[0].editor_dirty);

    writer
        .send(WsClient::SetNoteEditing(Sid(2), page_id, false))
        .await;
    viewer.flush().await;
    assert!(!viewer.note_editors.contains_key(&Sid(2)));

    writer
        .send(WsClient::CloseFileWindow(Sid(3), page_id))
        .await;
    writer.flush().await;
    viewer.flush().await;
    assert!(writer.file_windows.is_empty());
    assert!(viewer.file_windows.is_empty());
    assert!(viewer
        .notes
        .get(&Sid(2))
        .unwrap()
        .linked_file_window_ids
        .is_empty());
    assert!(server
        .state()
        .lookup(&name)
        .unwrap()
        .workspace_state()
        .file_windows
        .is_empty());

    writer.send(WsClient::CloseNote(Sid(4), page_id)).await;
    writer.flush().await;
    viewer.flush().await;
    assert!(viewer
        .notes
        .get(&Sid(2))
        .unwrap()
        .linked_note_ids
        .is_empty());

    writer.send(WsClient::Close(Sid(1), page_id)).await;
    writer.flush().await;
    viewer.flush().await;
    assert!(viewer
        .notes
        .get(&Sid(2))
        .unwrap()
        .linked_shell_ids
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn test_ws_resize() -> Result<()> {
    let server = TestServer::new().await;

    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let mut s = ClientSocket::connect(&server.ws_endpoint(&name), &key, None).await?;

    s.send(WsClient::Move(Sid(1), 1, None)).await; // error: does not exist yet!
    s.flush().await;
    assert_eq!(s.errors.len(), 1);

    s.send(WsClient::Create(0, 0, 1)).await;
    s.flush().await;
    assert_eq!(s.shells.len(), 1);
    assert_eq!(*s.shells.get(&Sid(1)).unwrap(), WsWinsize::default());

    let new_size = WsWinsize {
        x: 42,
        y: 105,
        rows: 200,
        cols: 20,
        ..Default::default()
    };
    s.send(WsClient::Move(Sid(1), 1, Some(new_size.clone())))
        .await;
    s.send(WsClient::Move(Sid(2), 1, Some(new_size.clone())))
        .await; // error: does not exist
    s.flush().await;
    assert_eq!(s.shells.len(), 1);
    assert_eq!(*s.shells.get(&Sid(1)).unwrap(), new_size);
    assert_eq!(s.errors.len(), 2);

    s.send(WsClient::Close(Sid(1), 1)).await;
    s.flush().await;
    assert_eq!(s.shells.len(), 0);

    s.send(WsClient::Move(Sid(1), 1, None)).await; // error: shell was closed
    s.flush().await;
    assert_eq!(s.errors.len(), 3);

    Ok(())
}

#[tokio::test]
async fn test_users_join() -> Result<()> {
    let server = TestServer::new().await;

    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let endpoint = server.ws_endpoint(&name);
    let mut s1 = ClientSocket::connect(&endpoint, &key, None).await?;
    s1.flush().await;
    assert_eq!(s1.users.len(), 1);

    let mut s2 = ClientSocket::connect(&endpoint, &key, None).await?;
    s2.flush().await;
    assert_eq!(s2.users.len(), 2);

    drop(s2);
    let mut s3 = ClientSocket::connect(&endpoint, &key, None).await?;
    s3.flush().await;
    assert_eq!(s3.users.len(), 2);

    s1.flush().await;
    assert_eq!(s1.users.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_users_metadata() -> Result<()> {
    let server = TestServer::new().await;

    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let endpoint = server.ws_endpoint(&name);
    let mut s = ClientSocket::connect(&endpoint, &key, None).await?;
    s.flush().await;
    assert_eq!(s.users.len(), 1);
    assert_eq!(s.users.get(&s.user_id).unwrap().cursor, None);

    s.send(WsClient::SetName("mr. foo".into())).await;
    s.send(WsClient::SetCursor(1, Some((40, 524)))).await;
    s.flush().await;
    let user = s.users.get(&s.user_id).unwrap();
    assert_eq!(user.name, "mr. foo");
    assert_eq!(user.cursor, Some((40, 524)));

    Ok(())
}

#[tokio::test]
async fn test_chat_messages() -> Result<()> {
    let server = TestServer::new().await;

    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let endpoint = server.ws_endpoint(&name);
    let mut s1 = ClientSocket::connect(&endpoint, &key, None).await?;
    let mut s2 = ClientSocket::connect(&endpoint, &key, None).await?;

    s1.send(WsClient::SetName("billy".into())).await;
    s1.send(WsClient::Chat("hello there!".into())).await;
    s1.flush().await;

    s2.flush().await;
    assert_eq!(s2.messages.len(), 1);
    assert_eq!(
        s2.messages[0],
        (s1.user_id, "billy".into(), "hello there!".into())
    );

    let mut s3 = ClientSocket::connect(&endpoint, &key, None).await?;
    s3.flush().await;
    assert_eq!(s1.messages.len(), 1);
    assert_eq!(s3.messages.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_read_write_permissions() -> Result<()> {
    let server = TestServer::new().await;

    // create controller with read-only mode enabled
    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, true).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    let write_url = controller
        .write_url()
        .expect("Should have write URL when enable_readers is true")
        .to_string();

    tokio::spawn(async move { controller.run().await });

    let write_password = write_url
        .split(',')
        .nth(1)
        .expect("Write URL should contain password");

    // connect with write access
    let mut writer =
        ClientSocket::connect(&server.ws_endpoint(&name), &key, Some(write_password)).await?;
    writer.flush().await;

    // test write permissions
    writer.send(WsClient::Create(0, 0, 1)).await;
    writer.flush().await;
    assert_eq!(
        writer.shells.len(),
        1,
        "Writer should be able to create a shell"
    );
    assert!(writer.errors.is_empty(), "Writer should not receive errors");

    // connect with read-only access
    let mut reader = ClientSocket::connect(&server.ws_endpoint(&name), &key, None).await?;
    reader.flush().await;

    // test read-only restrictions
    reader.send(WsClient::Create(0, 0, 1)).await;
    reader.flush().await;
    assert!(
        !reader.errors.is_empty(),
        "Reader should receive an error when attempting to create shell"
    );
    assert_eq!(
        reader.shells.len(),
        1,
        "Reader should still see the existing shell"
    );

    Ok(())
}
