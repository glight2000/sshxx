use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use tokio::sync::{broadcast, mpsc};

use crate::protocol::wire::{CreateTerminal, TerminalSummary};

const OUTPUT_BUFFER_BYTES: usize = 16 << 20;
const COMMAND_QUEUE_CAPACITY: usize = 128;
pub(crate) const OUTPUT_CHUNK_BYTES: usize = 32 << 10;
const MAX_INPUT_BYTES: usize = 256 << 10;

#[derive(Clone, Debug)]
pub(crate) enum SessionEvent {
    Output { sequence: u64, data: Arc<[u8]> },
    Exited { exit_code: u32, signal: String },
}

#[derive(Debug)]
struct OutputBuffer {
    bytes: VecDeque<u8>,
    retained_sequence: u64,
    next_sequence: u64,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            bytes: VecDeque::with_capacity(OUTPUT_BUFFER_BYTES),
            retained_sequence: 0,
            next_sequence: 0,
        }
    }

    fn append(&mut self, bytes: &[u8]) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(bytes.len() as u64);
        self.bytes.extend(bytes);
        if self.bytes.len() > OUTPUT_BUFFER_BYTES {
            let discarded = self.bytes.len() - OUTPUT_BUFFER_BYTES;
            self.bytes.drain(..discarded);
            self.retained_sequence = self.retained_sequence.saturating_add(discarded as u64);
        }
        sequence
    }

    fn snapshot_after(&self, after_sequence: u64) -> BufferSnapshot {
        let start = after_sequence
            .max(self.retained_sequence)
            .min(self.next_sequence);
        let offset = (start - self.retained_sequence) as usize;
        let bytes = self.bytes.iter().skip(offset).copied().collect();
        BufferSnapshot {
            sequence: start,
            next_sequence: self.next_sequence,
            bytes,
        }
    }
}

#[derive(Debug)]
pub(crate) struct BufferSnapshot {
    pub sequence: u64,
    pub next_sequence: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
struct SessionState {
    output: OutputBuffer,
    running: bool,
    exit_code: u32,
    signal: String,
    rows: u16,
    columns: u16,
}

enum SessionCommand {
    Input(Vec<u8>),
    Resize(PtySize),
    Close,
}

pub(crate) struct TerminalSession {
    id: String,
    process_id: u32,
    command_tx: mpsc::Sender<SessionCommand>,
    events: broadcast::Sender<SessionEvent>,
    state: Mutex<SessionState>,
}

impl TerminalSession {
    pub fn spawn(request: CreateTerminal) -> Result<Arc<Self>> {
        validate_terminal_id(&request.terminal_id)?;
        if request.program.trim().is_empty() {
            bail!("terminal program must not be empty");
        }
        if request.program.contains('\0')
            || request.args.iter().any(|argument| argument.contains('\0'))
            || request
                .environment
                .iter()
                .any(|(key, value)| key.contains('\0') || value.contains('\0'))
        {
            bail!("terminal launch configuration contains a null byte");
        }

        let size = validated_size(request.rows, request.columns)?;
        let pair = native_pty_system()
            .openpty(size)
            .context("failed to open pseudo-terminal")?;
        let mut command = CommandBuilder::new(&request.program);
        command.args(&request.args);
        if !request.working_directory.is_empty() {
            command.cwd(PathBuf::from(&request.working_directory));
        }
        for (key, value) in &request.environment {
            command.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("failed to spawn {}", request.program))?;
        let process_id = child.process_id().unwrap_or_default();
        let killer = child.clone_killer();
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take PTY writer")?;

        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        let (events, _) = broadcast::channel(256);
        let session = Arc::new(Self {
            id: request.terminal_id,
            process_id,
            command_tx,
            events,
            state: Mutex::new(SessionState {
                output: OutputBuffer::new(),
                running: true,
                exit_code: 0,
                signal: String::new(),
                rows: size.rows,
                columns: size.cols,
            }),
        });

        spawn_command_thread(session.clone(), pair.master, writer, killer, command_rx);
        spawn_reader_thread(session.clone(), reader);
        spawn_wait_thread(session.clone(), child);
        Ok(session)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.events.subscribe()
    }

    pub fn snapshot_after(&self, after_sequence: u64) -> BufferSnapshot {
        self.state
            .lock()
            .expect("terminal session state poisoned")
            .output
            .snapshot_after(after_sequence)
    }

    pub fn exit_event(&self) -> Option<SessionEvent> {
        let state = self.state.lock().expect("terminal session state poisoned");
        (!state.running).then(|| SessionEvent::Exited {
            exit_code: state.exit_code,
            signal: state.signal.clone(),
        })
    }

    pub fn send_input(&self, data: Vec<u8>) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_INPUT_BYTES {
            bail!("terminal input exceeds {MAX_INPUT_BYTES} bytes");
        }
        self.command_tx
            .try_send(SessionCommand::Input(data))
            .context("terminal input queue is unavailable")
    }

    pub fn resize(&self, rows: u32, columns: u32) -> Result<()> {
        let size = validated_size(rows, columns)?;
        self.command_tx
            .try_send(SessionCommand::Resize(size))
            .context("terminal command queue is unavailable")
    }

    pub fn close(&self) -> Result<()> {
        self.command_tx
            .try_send(SessionCommand::Close)
            .context("terminal command queue is unavailable")
    }

    pub fn is_running(&self) -> bool {
        self.state
            .lock()
            .expect("terminal session state poisoned")
            .running
    }

    pub fn working_directory(&self) -> Option<PathBuf> {
        working_directory_for_process(self.process_id)
    }

    pub fn summary(&self) -> TerminalSummary {
        let state = self.state.lock().expect("terminal session state poisoned");
        TerminalSummary {
            terminal_id: self.id.clone(),
            retained_sequence: state.output.retained_sequence,
            next_sequence: state.output.next_sequence,
            running: state.running,
            process_id: self.process_id,
            rows: state.rows.into(),
            columns: state.columns.into(),
        }
    }

    fn append_output(&self, data: &[u8]) {
        let sequence = {
            let mut state = self.state.lock().expect("terminal session state poisoned");
            state.output.append(data)
        };
        self.events
            .send(SessionEvent::Output {
                sequence,
                data: Arc::from(data),
            })
            .ok();
    }

    fn mark_exited(&self, exit_code: u32, signal: String) {
        {
            let mut state = self.state.lock().expect("terminal session state poisoned");
            state.running = false;
            state.exit_code = exit_code;
            state.signal.clone_from(&signal);
        }
        self.events
            .send(SessionEvent::Exited { exit_code, signal })
            .ok();
        self.command_tx.try_send(SessionCommand::Close).ok();
    }

    fn record_size(&self, size: PtySize) {
        let mut state = self.state.lock().expect("terminal session state poisoned");
        state.rows = size.rows;
        state.columns = size.cols;
    }
}

fn spawn_command_thread(
    session: Arc<TerminalSession>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    mut writer: Box<dyn Write + Send>,
    mut killer: Box<dyn ChildKiller + Send + Sync>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
) {
    std::thread::Builder::new()
        .name(format!("sshxx-host-command-{}", session.id()))
        .spawn(move || {
            while let Some(command) = command_rx.blocking_recv() {
                match command {
                    SessionCommand::Input(data) => {
                        if writer.write_all(&data).is_err() || writer.flush().is_err() {
                            break;
                        }
                    }
                    SessionCommand::Resize(size) => {
                        if master.resize(size).is_ok() {
                            session.record_size(size);
                        }
                    }
                    SessionCommand::Close => {
                        killer.kill().ok();
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn terminal command thread");
}

fn spawn_reader_thread(session: Arc<TerminalSession>, mut reader: Box<dyn Read + Send>) {
    std::thread::Builder::new()
        .name(format!("sshxx-host-output-{}", session.id()))
        .spawn(move || {
            let mut buffer = [0u8; OUTPUT_CHUNK_BYTES];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(length) => session.append_output(&buffer[..length]),
                }
            }
        })
        .expect("failed to spawn terminal output thread");
}

fn spawn_wait_thread(
    session: Arc<TerminalSession>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
) {
    std::thread::Builder::new()
        .name(format!("sshxx-host-wait-{}", session.id()))
        .spawn(move || match child.wait() {
            Ok(status) => session.mark_exited(
                status.exit_code(),
                status.signal().unwrap_or_default().to_owned(),
            ),
            Err(error) => session.mark_exited(1, error.to_string()),
        })
        .expect("failed to spawn terminal wait thread");
}

fn validated_size(rows: u32, columns: u32) -> Result<PtySize> {
    let rows = u16::try_from(rows)
        .ok()
        .filter(|rows| (2..=1_000).contains(rows))
        .context("terminal rows must be between 2 and 1000")?;
    let columns = u16::try_from(columns)
        .ok()
        .filter(|columns| (2..=1_000).contains(columns))
        .context("terminal columns must be between 2 and 1000")?;
    Ok(PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    })
}

fn validate_terminal_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("terminal ID is invalid");
    }
    Ok(())
}

pub(crate) type SessionMap = Arc<tokio::sync::RwLock<HashMap<String, Arc<TerminalSession>>>>;

#[cfg(target_os = "linux")]
fn working_directory_for_process(process_id: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{process_id}/cwd")).ok()
}

#[cfg(not(target_os = "linux"))]
fn working_directory_for_process(_process_id: u32) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::OutputBuffer;

    #[test]
    fn output_snapshot_uses_absolute_byte_sequences() {
        let mut output = OutputBuffer::new();
        assert_eq!(output.append(b"hello"), 0);
        assert_eq!(output.append(b" world"), 5);
        let snapshot = output.snapshot_after(6);
        assert_eq!(snapshot.sequence, 6);
        assert_eq!(snapshot.next_sequence, 11);
        assert_eq!(snapshot.bytes, b"world");
    }
}
