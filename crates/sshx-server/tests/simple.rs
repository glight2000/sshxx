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
        workspace: None,
        ssh_profiles: None,
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
            workspace: None,
            ssh_profiles: None,
        })
        .await?
        .into_inner();

    assert_eq!(resp.name, "dev");
    assert_eq!(resp.url, "http://localhost:5173/s/dev");
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
        }],
        notes: vec![WorkspaceNote {
            id: 8,
            x: 360,
            y: 480,
            width: 400,
            height: 240,
            text: "Deploy after tests".into(),
            background: "#445566".into(),
            opacity: 75,
            page_id: 2,
        }],
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
            workspace: Some(workspace.clone()),
            ssh_profiles: None,
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
        "Dracula".into(),
    )?;
    assert_eq!(restored.unwrap().rows, 30);
    assert_eq!(session.workspace_state().shells[0].page_id, 2);
    assert_eq!(session.workspace_state().shells[0].theme, "Dracula");
    assert_eq!(session.workspace_state().shells[0].width, 714);
    assert_eq!(session.workspace_state().shells[0].height, 518);
    assert_eq!(session.workspace_state().notes[0].page_id, 2);
    assert!(session.sequence_numbers().map.contains_key(&7));
    assert_eq!(session.counter().next_sid(), sshx_core::Sid(9));
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
    };
    let response = client
        .open(OpenRequest {
            origin: "http://localhost:5173".into(),
            encrypted_zeros: Encrypt::new("localdevkey").zeros().into(),
            name: String::new(),
            write_password_hash: None,
            daemon_version: "test-daemon".into(),
            workspace: None,
            ssh_profiles: Some(SshProfileCollection {
                format_version: sshx_core::SSH_PROFILE_FORMAT_VERSION,
                profiles: vec![profile.clone()],
            }),
        })
        .await?
        .into_inner();
    let session = server.state().lookup(&response.name).unwrap();
    assert_eq!(session.ssh_profile_collection().profiles, vec![profile]);

    let duplicate_name = WsSshProfile {
        id: "other".into(),
        name: "office".into(),
        host: "other.example.test".into(),
        port: 22,
        username: String::new(),
        auth_method: WsSshAuthMethod::Default,
        key_path: String::new(),
        accept_new_host_key: false,
    };
    assert!(session.upsert_ssh_profile(duplicate_name).is_err());
    Ok(())
}
