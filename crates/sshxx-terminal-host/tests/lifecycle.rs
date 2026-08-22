#![cfg(unix)]

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use sshxx_terminal_host::client::Client;
use sshxx_terminal_host::protocol::frame::Message;
use sshxx_terminal_host::protocol::wire::{CreateTerminal, Frame};

const TOKEN: [u8; 32] = [0x5a; 32];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_restart_rebinds_the_same_endpoint() -> Result<()> {
    let state = tempfile::tempdir()?;
    let endpoint = state.path().join("restart.sock");
    let endpoint = endpoint.to_string_lossy().into_owned();
    let server_endpoint = endpoint.clone();
    let server = tokio::spawn(async move {
        sshxx_terminal_host::server::serve(&server_endpoint, TOKEN.to_vec()).await
    });

    let mut first = connect_when_ready(&endpoint).await?;
    let restart = first.restart(false).await?;
    expect_ack(&mut first, restart).await?;
    drop(first);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut second = connect_when_ready(&endpoint).await?;
    let list = second.list_terminals().await?;
    assert!(expect_terminal_list(&mut second, list).await?.is_empty());
    let shutdown = second.shutdown(false).await?;
    expect_ack(&mut second, shutdown).await?;
    drop(second);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .context("terminal host did not stop after restart")???;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_survives_client_restart_and_keeps_private_history() -> Result<()> {
    let state = tempfile::tempdir()?;
    let endpoint = state.path().join("host.sock");
    let endpoint = endpoint.to_string_lossy().into_owned();
    let server_endpoint = endpoint.clone();
    let server = tokio::spawn(async move {
        sshxx_terminal_host::server::serve(&server_endpoint, TOKEN.to_vec()).await
    });
    let mut first = connect_when_ready(&endpoint).await?;

    let first_history = state.path().join("history-terminal-a");
    let create = first
        .create_terminal(CreateTerminal {
            terminal_id: "terminal-a".into(),
            program: "/bin/bash".into(),
            args: vec!["--noprofile".into(), "--norc".into()],
            working_directory: state.path().to_string_lossy().into_owned(),
            environment: shell_environment(&first_history),
            rows: 24,
            columns: 80,
        })
        .await?;
    expect_ack(&mut first, create).await?;
    let attach = first.attach_terminal("terminal-a", 0).await?;
    expect_ack(&mut first, attach).await?;
    let disable_echo = first.input("terminal-a", b"stty -echo\r".to_vec()).await?;
    expect_ack(&mut first, disable_echo).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let input = first
        .input(
            "terminal-a",
            b"printf '\\033[31mSSHXX_RAW\\033[0m\\n'; printf 'PID=%s\\n' \"$$\"; history -s terminal-a-only; history -w; printf 'FIRST_DONE\\n'; stty echo\r"
                .to_vec(),
        )
        .await?;
    let (first_output, next_sequence) =
        collect_output_until(&mut first, input, b"FIRST_DONE", Duration::from_secs(5)).await?;
    assert!(
        first_output
            .windows(b"\x1b[31mSSHXX_RAW\x1b[0m".len())
            .any(|window| window == b"\x1b[31mSSHXX_RAW\x1b[0m"),
        "host must preserve raw ANSI bytes: {first_output:?}"
    );
    let first_pid = extract_number_after(&first_output, b"PID=")?;
    drop(first); // Simulates sshxx-daemon exiting without closing its terminals.

    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut second = Client::connect(&endpoint, TOKEN.to_vec(), "lifecycle-test-2").await?;
    let list = second.list_terminals().await?;
    let list = expect_terminal_list(&mut second, list).await?;
    let terminal = list
        .iter()
        .find(|terminal| terminal.terminal_id == "terminal-a")
        .context("terminal was lost when the first client disconnected")?;
    assert!(terminal.running);
    assert_eq!(u64::from(terminal.process_id), first_pid);

    let attach = second.attach_terminal("terminal-a", next_sequence).await?;
    expect_ack(&mut second, attach).await?;
    let resize = second.resize("terminal-a", 37, 111).await?;
    expect_ack(&mut second, resize).await?;
    let disable_echo = second.input("terminal-a", b"stty -echo\r".to_vec()).await?;
    expect_ack(&mut second, disable_echo).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let input = second
        .input(
            "terminal-a",
            b"printf 'PID_AFTER=%s\\n' \"$$\"; printf 'SIZE=%s\\n' \"$(stty size)\"; printf 'SECOND_DONE\\n'; stty echo\r"
                .to_vec(),
        )
        .await?;
    let (second_output, _) =
        collect_output_until(&mut second, input, b"SECOND_DONE", Duration::from_secs(5)).await?;
    assert_eq!(
        extract_number_after(&second_output, b"PID_AFTER=")?,
        first_pid
    );
    assert!(contains_bytes(&second_output, b"SIZE=37 111"));
    let cwd_request = second.get_working_directory("terminal-a").await?;
    let cwd = response_for(&mut second, cwd_request).await?;
    let Some(Message::WorkingDirectory(cwd)) = cwd.message else {
        bail!("expected terminal working directory");
    };
    assert_eq!(Path::new(&cwd.path), state.path());

    let second_history = state.path().join("history-terminal-b");
    let create = second
        .create_terminal(CreateTerminal {
            terminal_id: "terminal-b".into(),
            program: "/bin/bash".into(),
            args: vec!["--noprofile".into(), "--norc".into()],
            working_directory: state.path().to_string_lossy().into_owned(),
            environment: shell_environment(&second_history),
            rows: 24,
            columns: 80,
        })
        .await?;
    expect_ack(&mut second, create).await?;
    let input = second
        .input(
            "terminal-b",
            b"history -s terminal-b-only; history -w\r".to_vec(),
        )
        .await?;
    expect_ack(&mut second, input).await?;
    wait_for_file(&second_history).await?;

    let first_history_contents = tokio::fs::read_to_string(&first_history).await?;
    let second_history_contents = tokio::fs::read_to_string(&second_history).await?;
    assert!(first_history_contents.contains("terminal-a-only"));
    assert!(!first_history_contents.contains("terminal-b-only"));
    assert!(second_history_contents.contains("terminal-b-only"));
    assert!(!second_history_contents.contains("terminal-a-only"));

    let shutdown = second.shutdown(false).await?;
    let error = expect_error(&mut second, shutdown).await?;
    assert_eq!(error.code, "ACTIVE_TERMINALS");
    assert!(error.message.contains("2 active terminal"));

    for terminal_id in ["terminal-a", "terminal-b"] {
        let close = second.close_terminal(terminal_id).await?;
        expect_ack(&mut second, close).await?;
    }
    let shutdown = second.shutdown(false).await?;
    expect_ack(&mut second, shutdown).await?;
    drop(second);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .context("terminal host did not stop")???;
    Ok(())
}

fn shell_environment(history_path: &Path) -> HashMap<String, String> {
    HashMap::from([
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
        (
            "HISTFILE".into(),
            history_path.to_string_lossy().into_owned(),
        ),
        ("HISTCONTROL".into(), String::new()),
    ])
}

async fn connect_when_ready(endpoint: &str) -> Result<Client> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match Client::connect(endpoint, TOKEN.to_vec(), "lifecycle-test-1").await {
            Ok(client) => return Ok(client),
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn expect_ack(client: &mut Client, request_id: u64) -> Result<()> {
    match response_for(client, request_id).await?.message {
        Some(Message::Ack(_)) => Ok(()),
        Some(Message::Error(error)) => bail!("{}: {}", error.code, error.message),
        _ => bail!("expected terminal-host acknowledgement"),
    }
}

async fn expect_error(
    client: &mut Client,
    request_id: u64,
) -> Result<sshxx_terminal_host::protocol::wire::Error> {
    match response_for(client, request_id).await?.message {
        Some(Message::Error(error)) => Ok(error),
        _ => bail!("expected terminal-host error"),
    }
}

async fn expect_terminal_list(
    client: &mut Client,
    request_id: u64,
) -> Result<Vec<sshxx_terminal_host::protocol::wire::TerminalSummary>> {
    match response_for(client, request_id).await?.message {
        Some(Message::TerminalList(list)) => Ok(list.terminals),
        Some(Message::Error(error)) => bail!("{}: {}", error.code, error.message),
        _ => bail!("expected terminal-host list"),
    }
}

async fn response_for(client: &mut Client, request_id: u64) -> Result<Frame> {
    loop {
        let response = client
            .receive()
            .await?
            .context("terminal host disconnected before responding")?;
        if response.request_id == request_id {
            return Ok(response);
        }
    }
}

async fn collect_output_until(
    client: &mut Client,
    acknowledged_request_id: u64,
    marker: &[u8],
    timeout: Duration,
) -> Result<(Vec<u8>, u64)> {
    tokio::time::timeout(timeout, async {
        let mut output = Vec::new();
        let mut next_sequence = 0;
        let mut acknowledged = false;
        loop {
            let frame = client
                .receive()
                .await?
                .context("terminal host disconnected while collecting output")?;
            match frame.message {
                Some(Message::Ack(_)) if frame.request_id == acknowledged_request_id => {
                    acknowledged = true;
                }
                Some(Message::Error(error)) if frame.request_id == acknowledged_request_id => {
                    bail!("{}: {}", error.code, error.message);
                }
                Some(Message::TerminalOutput(data)) => {
                    next_sequence = next_sequence.max(data.sequence + data.data.len() as u64);
                    output.extend_from_slice(&data.data);
                    if acknowledged && contains_bytes(&output, marker) {
                        return Ok((output, next_sequence));
                    }
                }
                _ => {}
            }
        }
    })
    .await
    .context("timed out waiting for terminal output")?
}

async fn wait_for_file(path: &Path) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if tokio::fs::metadata(path).await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("history file was not written: {}", path.display());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn extract_number_after(output: &[u8], prefix: &[u8]) -> Result<u64> {
    let start = output
        .windows(prefix.len())
        .position(|window| window == prefix)
        .context("output prefix was not found")?
        + prefix.len();
    let digits: Vec<u8> = output[start..]
        .iter()
        .copied()
        .take_while(u8::is_ascii_digit)
        .collect();
    let digits = std::str::from_utf8(&digits)?;
    digits.parse().context("output number was invalid")
}
