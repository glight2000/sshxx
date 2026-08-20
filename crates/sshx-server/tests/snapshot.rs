use std::sync::Arc;

use anyhow::Result;
use sshx_core::{Sid, Uid};
use sshx_daemon::{controller::Controller, runner::Runner};
use sshx_server::{
    session::Session,
    web::protocol::{WsClient, WsNote, WsWinsize},
};

use crate::common::*;

pub mod common;

#[tokio::test]
async fn test_basic_restore() -> Result<()> {
    let server = TestServer::new().await;

    let mut controller = Controller::new(&server.endpoint(), "", Runner::Echo, false).await?;
    let name = controller.name().to_owned();
    let key = controller.encryption_key().to_owned();
    tokio::spawn(async move { controller.run().await });

    let mut s = ClientSocket::connect(&server.ws_endpoint(&name), &key, None).await?;
    s.flush().await;
    assert_eq!(s.user_id, Uid(1));
    assert_eq!(s.server_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(s.daemon_version, env!("CARGO_PKG_VERSION"));

    s.send(WsClient::CreatePage("Work".into())).await;
    s.flush().await;
    let page_id = s.pages.last().unwrap().id;
    s.send(WsClient::Create(0, 0, page_id)).await;
    s.flush().await;

    let new_size = WsWinsize {
        x: 42,
        y: 105,
        rows: 200,
        cols: 20,
        width: 714,
        height: 518,
        title: "Build logs".into(),
        background: "#123456".into(),
        opacity: 72,
        page_id,
        theme: "Tokyo Night".into(),
    };
    let note = WsNote {
        x: 120,
        y: 240,
        width: 512,
        height: 320,
        text: "Remember to deploy".into(),
        background: "#654321".into(),
        opacity: 65,
        page_id,
    };

    s.send_input(Sid(1), b"hello there!").await;
    s.send_input(Sid(1), b" - another message").await;
    s.send(WsClient::Move(Sid(1), page_id, Some(new_size.clone())))
        .await;
    s.send(WsClient::CreateNote(note.x, note.y, page_id)).await;
    s.flush().await;
    s.send(WsClient::UpdateNote(Sid(2), page_id, Some(note.clone())))
        .await;
    s.flush().await;
    assert!(s.shells.contains_key(&Sid(1)));

    // Replace the shell with its snapshot.
    let data = server.state().lookup(&name).unwrap().snapshot()?;
    server
        .state()
        .insert(&name, Arc::new(Session::restore(&data)?));

    let mut s = ClientSocket::connect(&server.ws_endpoint(&name), &key, None).await?;
    s.send(WsClient::Subscribe(Sid(1), page_id, 0)).await;
    s.flush().await;

    assert_eq!(s.read(Sid(1)), "hello there! - another message");
    assert_eq!(s.shells.get(&Sid(1)).unwrap(), &new_size);
    assert_eq!(s.notes.get(&Sid(2)).unwrap(), &note);
    assert_eq!(s.pages.last().unwrap().name, "Work");
    assert_eq!(s.daemon_version, env!("CARGO_PKG_VERSION"));

    Ok(())
}
