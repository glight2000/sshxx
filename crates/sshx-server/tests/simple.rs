use anyhow::Result;
use sshx_core::proto::*;
use sshx_daemon::encrypt::Encrypt;
use sshx_server::web::protocol::{WsSshAuthMethod, WsSshProfile};
use sshx_server::ServerOptions;

use crate::common::*;

pub mod common;

#[tokio::test]
async fn test_rpc() -> Result<()> {
    let server = TestServer::new().await;
    let mut client = server.grpc_client().await;

    let req = OpenRequest {
        origin: "sshxx.example".into(),
        encrypted_zeros: Encrypt::new("").zeros().into(),
        name: String::new(),
        write_password_hash: None,
        daemon_version: "test-daemon".into(),
        terminal_host_version: "test-host".into(),
        workspace: None,
        ssh_profiles: None,
        capabilities: Vec::new(),
    };
    let resp = client.open(req).await?;
    assert!(!resp.into_inner().name.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_web_get() -> Result<()> {
    let server = TestServer::new().await;

    let resp = reqwest::get(server.endpoint()).await?;
    assert!(!resp.status().is_server_error());
    assert_eq!(
        resp.headers()
            .get(reqwest::header::CONTENT_SECURITY_POLICY)
            .and_then(|value| value.to_str().ok()),
        Some("frame-ancestors 'none'")
    );
    assert_eq!(
        resp.headers()
            .get(reqwest::header::X_FRAME_OPTIONS)
            .and_then(|value| value.to_str().ok()),
        Some("DENY")
    );

    let missing_chunk = reqwest::get(format!(
        "{}/_app/immutable/chunks/missing-deployment-chunk.js",
        server.endpoint()
    ))
    .await?;
    assert_eq!(missing_chunk.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(
        missing_chunk
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );

    Ok(())
}

#[tokio::test]
async fn test_fixed_session_name() -> Result<()> {
    let mut options = ServerOptions::default();
    options.session_name = Some("dev".into());
    let server = TestServer::new_with_options(options).await;
    let mut client = server.grpc_client().await;
    let resp = client
        .open(OpenRequest {
            origin: "http://localhost:5173".into(),
            encrypted_zeros: Encrypt::new("localdevkey").zeros().into(),
            name: String::new(),
            write_password_hash: None,
            daemon_version: "test-daemon".into(),
            terminal_host_version: "test-host".into(),
            workspace: None,
            ssh_profiles: None,
            capabilities: Vec::new(),
        })
        .await?
        .into_inner();

    assert_eq!(resp.name, "dev");
    assert_eq!(resp.url, "http://localhost:5173/s/dev");

    let original_session = server.state().lookup("dev").unwrap();
    client
        .open(OpenRequest {
            origin: "http://localhost:5173".into(),
            encrypted_zeros: Encrypt::new("localdevkey").zeros().into(),
            name: String::new(),
            write_password_hash: None,
            daemon_version: "restarted-daemon".into(),
            terminal_host_version: "restarted-host".into(),
            workspace: None,
            ssh_profiles: None,
            capabilities: Vec::new(),
        })
        .await?;
    let replacement_session = server.state().lookup("dev").unwrap();
    assert!(!std::sync::Arc::ptr_eq(
        &original_session,
        &replacement_session
    ));
    let mut socket = ClientSocket::connect(&server.ws_endpoint("dev"), "localdevkey", None).await?;
    socket.flush().await;
    assert_eq!(socket.daemon_version, "restarted-daemon");
    assert_eq!(socket.terminal_host_version, "restarted-host");
    Ok(())
}

#[tokio::test]
async fn test_restore_daemon_workspace() -> Result<()> {
    let server = TestServer::new().await;
    let mut client = server.grpc_client().await;
    let workspace = WorkspaceState {
        format_version: sshx_core::WORKSPACE_FORMAT_VERSION,
        shells: vec![WorkspaceShell {
            id: 7,
            x: 120,
            y: 240,
            rows: 30,
            cols: 100,
            width: 714,
            height: 518,
            title: "Build".into(),
            background: "#112233".into(),
            opacity: 70,
            page_id: 2,
            theme: "Dracula".into(),
            ssh_profile_id: String::new(),
            minimized: true,
        }],
        notes: vec![WorkspaceNote {
            id: 8,
            x: 360,
            y: 480,
            width: 400,
            height: 240,
            text: "Deploy after tests".into(),
            paragraphs: vec!["Deploy after tests".into()],
            linked_shell_ids: vec![7],
            linked_note_ids: Vec::new(),
            linked_file_window_ids: vec![9],
            title: "Checklist".into(),
            background: "#445566".into(),
            opacity: 75,
            page_id: 2,
            minimized: true,
        }],
        file_windows: vec![WorkspaceFileWindow {
            id: 9,
            shell_id: 7,
            page_id: 2,
            path: "/tmp".into(),
            title: "Build".into(),
            background: "#111827".into(),
            x: 480,
            y: 600,
            width: 1040,
            height: 680,
            current_path: "/tmp/project".into(),
            expanded_paths: vec!["/".into(), "/tmp".into(), "/tmp/project".into()],
            selected_path: "/tmp/project/config.toml".into(),
            selected_kind: "file".into(),
            tree_scroll_top: 96,
            editor_path: "/tmp/project/config.toml".into(),
            editor_stream: 1 << 63,
            editor_data: b"encrypted editor".as_slice().into(),
            editor_dirty: true,
            sidebar_width: 360,
            tree_revision: 4,
            minimized: true,
        }],
        custom_windows: Vec::new(),
        pages: vec![
            WorkspacePage {
                id: 1,
                name: "Page 1".into(),
            },
            WorkspacePage {
                id: 2,
                name: "Work".into(),
            },
        ],
    };
    let response = client
        .open(OpenRequest {
            origin: "http://localhost:5173".into(),
            encrypted_zeros: Encrypt::new("localdevkey").zeros().into(),
            name: String::new(),
            write_password_hash: None,
            daemon_version: "test-daemon".into(),
            terminal_host_version: "test-host".into(),
            workspace: Some(workspace.clone()),
            ssh_profiles: None,
            capabilities: Vec::new(),
        })
        .await?
        .into_inner();

    let session = server.state().lookup(&response.name).unwrap();
    assert_eq!(session.workspace_state(), workspace);
    assert!(session.sequence_numbers().map.is_empty());
    let restored = session.add_shell(
        sshx_core::Sid(7),
        (120, 240),
        2,
        (30, 100),
        (714, 518),
        ("Dracula".into(), String::new(), String::new()),
    )?;
    assert_eq!(restored.unwrap().rows, 30);
    assert_eq!(session.workspace_state().shells[0].page_id, 2);
    assert_eq!(session.workspace_state().shells[0].theme, "Dracula");
    assert_eq!(session.workspace_state().shells[0].width, 714);
    assert_eq!(session.workspace_state().shells[0].height, 518);
    assert_eq!(session.workspace_state().notes[0].page_id, 2);
    assert_eq!(session.workspace_state().file_windows[0].shell_id, 7);
    assert!(session.sequence_numbers().map.contains_key(&7));
    assert_eq!(session.counter().next_sid(), sshx_core::Sid(10));

    session.move_canvas_items(
        2,
        1,
        vec![(sshx_core::Sid(7), 120, 240)],
        vec![(sshx_core::Sid(8), 360, 480)],
        vec![(sshx_core::Sid(9), 480, 600)],
        Vec::new(),
    )?;
    let moved = session.workspace_state();
    assert_eq!(moved.shells[0].page_id, 1);
    assert_eq!(moved.notes[0].page_id, 1);
    assert_eq!(moved.file_windows[0].page_id, 1);
    assert_eq!(moved.notes[0].linked_shell_ids, vec![7]);
    assert_eq!(moved.notes[0].linked_file_window_ids, vec![9]);
    assert!(session
        .move_canvas_items(
            1,
            99,
            vec![(sshx_core::Sid(7), 0, 0)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .is_err());
    assert_eq!(session.workspace_state().shells[0].page_id, 1);
    Ok(())
}

#[tokio::test]
async fn test_restore_and_validate_ssh_profiles() -> Result<()> {
    let server = TestServer::new().await;
    let mut client = server.grpc_client().await;
    let profile = SshProfile {
        id: "office".into(),
        name: "Office".into(),
        host: "office.example.test".into(),
        port: 22,
        username: "dev".into(),
        auth_method: SshAuthMethod::SshAuthAgent.into(),
        key_path: String::new(),
        accept_new_host_key: true,
        theme: "Dracula".into(),
        background_enabled: true,
        background: "#101010".into(),
    };
    let response = client
        .open(OpenRequest {
            origin: "http://localhost:5173".into(),
            encrypted_zeros: Encrypt::new("localdevkey").zeros().into(),
            name: String::new(),
            write_password_hash: None,
            daemon_version: "test-daemon".into(),
            terminal_host_version: "test-host".into(),
            workspace: None,
            ssh_profiles: Some(SshProfileCollection {
                format_version: sshx_core::SSH_PROFILE_FORMAT_VERSION,
                profiles: vec![profile.clone()],
            }),
            capabilities: Vec::new(),
        })
        .await?
        .into_inner();
    let session = server.state().lookup(&response.name).unwrap();
    assert_eq!(
        session.ssh_profile_collection().profiles,
        vec![profile.clone()]
    );
    session.add_shell(
        sshx_core::Sid(1),
        (0, 0),
        1,
        (24, 80),
        (640, 400),
        (String::new(), String::new(), "office".into()),
    )?;
    assert_eq!(
        session.shell_ssh_profile_id(sshx_core::Sid(1)).as_deref(),
        Some("office")
    );
    assert_eq!(session.workspace_state().shells[0].ssh_profile_id, "office");

    let restored_response = client
        .open(OpenRequest {
            origin: "http://localhost:5173".into(),
            encrypted_zeros: Encrypt::new("localdevkey").zeros().into(),
            name: String::new(),
            write_password_hash: None,
            daemon_version: "restored-daemon".into(),
            terminal_host_version: "restored-host".into(),
            workspace: Some(session.workspace_state()),
            ssh_profiles: Some(SshProfileCollection {
                format_version: sshx_core::SSH_PROFILE_FORMAT_VERSION,
                profiles: vec![profile],
            }),
            capabilities: Vec::new(),
        })
        .await?
        .into_inner();
    let restored_session = server.state().lookup(&restored_response.name).unwrap();
    assert_eq!(
        restored_session
            .shell_ssh_profile_id(sshx_core::Sid(1))
            .as_deref(),
        Some("office")
    );

    let duplicate_name = WsSshProfile {
        id: "other".into(),
        name: "office".into(),
        host: "other.example.test".into(),
        port: 22,
        username: String::new(),
        auth_method: WsSshAuthMethod::Default,
        key_path: String::new(),
        accept_new_host_key: false,
        theme: String::new(),
        background_enabled: false,
        background: String::new(),
    };
    assert!(session.upsert_ssh_profile(duplicate_name).is_err());
    Ok(())
}
