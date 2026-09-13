//! An optional, bounded mirror of a terminal's parsed state. Never owns a PTY.
//!
//! A failed/overloaded mirror stops advertising checkpoints, not terminal I/O.
//! The JS runtime exposes no filesystem, process, network, or module loader.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[cfg(test)]
use anyhow::Context as _;
use anyhow::{bail, Result};
use rquickjs::{Context, Function, Runtime};
use tokio::sync::oneshot;

const ENGINE_SOURCE: &str = include_str!(concat!(env!("OUT_DIR"), "/terminal.js"));
const ENGINE_BYTES: usize = 32 << 20;
const AGGREGATE_BYTES: usize = 256 << 20;
const QUEUED_BYTES: usize = 2 << 20;
pub(crate) const MAX_APPEND_BYTES: usize = 16 << 10;
const MAX_RESPONSE_BYTES: usize = 4 << 20;
const EXECUTION_LIMIT: Duration = Duration::from_millis(250);
static MIRROR_BYTES: AtomicUsize = AtomicUsize::new(0);
static MIRROR_COUNT: AtomicUsize = AtomicUsize::new(0);
static RESPONSES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);

const PRELUDE: &str = r#"
var console = {log(){}, debug(){}, warn(){}, error(){}, info(){}};
var self = globalThis;
var navigator = {userAgent: '', platform: 'Linux'};
var process = {title:'sshxx-state', platform:'linux', versions:{node:'24.0.0'}, env:{}};
var performance = {now: () => Date.now()};
var __timers = new Map(); var __timerId = 0;
function setTimeout(fn) { if (__timers.size >= 128) throw Error('timer capacity'); __timers.set(++__timerId, fn); return __timerId; }
function clearTimeout(id) { __timers.delete(id); }
var setInterval = setTimeout, clearInterval = clearTimeout;
// Run one bounded event-loop turn. A rescheduled maintenance task must not
// recursively consume every future idle slice inside a single parse deadline.
function __flushTimers() { for (const [id, fn] of [...__timers.entries()].slice(0,4)) { if (__timers.delete(id)) fn(); } }
"#;

enum Command {
    Stop,
    Output {
        sequence: u64,
        text: String,
    },
    Resize(u16, u16),
    Snapshot {
        history: u32,
        archive: Option<(u64, u64)>,
        reply: oneshot::Sender<Result<Checkpoint>>,
    },
}

/// Snapshot fence counts decoded UTF-8 bytes, not host PTY bytes or chunks.
pub(crate) struct Checkpoint {
    pub sequence: u64,
    pub data: String,
}

pub(crate) struct TerminalMirror {
    sender: mpsc::SyncSender<Command>,
    valid: Arc<AtomicBool>,
    queued: Arc<AtomicUsize>,
}

impl TerminalMirror {
    pub fn new(rows: u16, cols: u16) -> Self {
        let (sender, receiver) = mpsc::sync_channel(512);
        let valid = Arc::new(AtomicBool::new(true));
        let queued = Arc::new(AtomicUsize::new(0));
        let worker_valid = valid.clone();
        let worker_queued = queued.clone();
        if MIRROR_COUNT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 128).then_some(count + 1)
            })
            .is_err()
        {
            valid.store(false, Ordering::Release);
            return Self {
                sender,
                valid,
                queued,
            };
        }
        let count = WorkerCount;
        let spawned = std::thread::Builder::new()
            .name("terminal-state".into())
            .spawn(move || {
                let _count = count;
                let result = run(receiver, &worker_valid, &worker_queued, rows, cols);
                worker_valid.store(false, Ordering::Release);
                if let Err(error) = result {
                    // Engine exceptions can contain terminal content. Do not log
                    // their message, source, stack, or snapshot payload.
                    tracing::warn!(
                        phase = error
                            .chain()
                            .find_map(|cause| cause.downcast_ref::<MirrorPhase>())
                            .map(|phase| phase.0)
                            .unwrap_or("initialization-or-capacity"),
                        "terminal checkpoint mirror unavailable; raw output remains active"
                    );
                }
            });
        if spawned.is_err() {
            valid.store(false, Ordering::Release);
        }
        Self {
            sender,
            valid,
            queued,
        }
    }

    pub fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
        // Wake an idle worker as well; a full queue is already a wake-up.
        let _ = self.sender.try_send(Command::Stop);
    }

    pub fn append(&self, sequence: u64, text: &str) -> bool {
        if text.is_empty() || !self.valid.load(Ordering::Acquire) {
            return false;
        }
        let length = text.len();
        let previous = self.queued.fetch_add(length, Ordering::AcqRel);
        if length > MAX_APPEND_BYTES || previous.saturating_add(length) > QUEUED_BYTES {
            self.queued.fetch_sub(length, Ordering::AcqRel);
            if length > MAX_APPEND_BYTES {
                self.invalidate();
            }
            return false;
        }
        if self
            .sender
            .try_send(Command::Output {
                sequence,
                text: text.into(),
            })
            .is_err()
        {
            self.queued.fetch_sub(length, Ordering::AcqRel);
            return false;
        }
        true
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        if self.sender.try_send(Command::Resize(rows, cols)).is_err() {
            self.invalidate();
        }
    }

    pub fn request(
        &self,
        history: u32,
        archive: Option<(u64, u64)>,
    ) -> Result<oneshot::Receiver<Result<Checkpoint>>> {
        if !self.valid.load(Ordering::Acquire) {
            bail!("terminal checkpoint unavailable");
        }
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(Command::Snapshot {
                history,
                archive,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("terminal checkpoint busy"))?;
        Ok(receiver)
    }

    #[cfg(test)]
    pub async fn snapshot(&self, history: u32) -> Result<Checkpoint> {
        let receiver = self.request(history, None)?;
        tokio::time::timeout(Duration::from_secs(2), receiver)
            .await
            .context("terminal checkpoint timed out")?
            .context("terminal checkpoint worker stopped")?
    }
}

impl Drop for TerminalMirror {
    fn drop(&mut self) {
        self.invalidate();
    }
}

/// Run response work outside the PTY forwarding loop, with no terminal content
/// in errors or logs. A fresh 96-bit nonce authenticates each standalone value.
pub(crate) fn respond(
    request: sshx_core::proto::TerminalCheckpointRequest,
    receiver: Option<oneshot::Receiver<Result<Checkpoint>>>,
    encrypt: crate::encrypt::Encrypt,
    output: tokio::sync::mpsc::Sender<sshx_core::proto::client_update::ClientMessage>,
) {
    let Ok(permit) = RESPONSES.try_acquire() else {
        let _ = output.try_send(
            sshx_core::proto::client_update::ClientMessage::TerminalCheckpoint(
                sshx_core::proto::TerminalCheckpointResponse {
                    id: request.id,
                    request_id: request.request_id,
                    generation: request.generation,
                    ..Default::default()
                },
            ),
        );
        return;
    };
    tokio::spawn(async move {
        let _permit = permit;
        use aes_gcm::aead::{rand_core::RngCore, OsRng};
        use sshx_core::proto::{client_update::ClientMessage, TerminalCheckpointResponse};
        let checkpoint = match receiver {
            Some(receiver) => tokio::time::timeout(Duration::from_secs(2), receiver)
                .await
                .ok()
                .and_then(Result::ok)
                .and_then(Result::ok),
            None => None,
        };
        let mut response = TerminalCheckpointResponse {
            id: request.id,
            request_id: request.request_id.clone(),
            generation: request.generation,
            ..Default::default()
        };
        if let Some(checkpoint) = checkpoint {
            let payload = serde_json::json!({
                "id": request.id, "requestId": request.request_id, "generation": request.generation,
                "sequence": checkpoint.sequence,
                "state": serde_json::from_str::<serde_json::Value>(&checkpoint.data).ok(),
            });
            let mut nonce = [0u8; 12];
            OsRng.fill_bytes(&mut nonce);
            if let Ok(data) = encrypt.seal_checkpoint(&nonce, payload.to_string().as_bytes()) {
                response.sequence = checkpoint.sequence;
                response.data = data.into();
                response.nonce = nonce.to_vec().into();
            }
        }
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            output.send(ClientMessage::TerminalCheckpoint(response)),
        )
        .await;
    });
}

struct WorkerCount;
impl Drop for WorkerCount {
    fn drop(&mut self) {
        MIRROR_COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
struct MirrorPhase(&'static str);
impl std::fmt::Display for MirrorPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for MirrorPhase {}

struct MemoryUsage(usize);
impl MemoryUsage {
    fn update(&mut self, bytes: usize) -> Result<()> {
        let mut total = MIRROR_BYTES.load(Ordering::Acquire);
        loop {
            let next = total - self.0 + bytes;
            if next > AGGREGATE_BYTES {
                bail!("terminal checkpoint aggregate capacity");
            }
            match MIRROR_BYTES.compare_exchange_weak(
                total,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => total = current,
            }
        }
        self.0 = bytes;
        Ok(())
    }

    fn reserve(&mut self, valid: &AtomicBool) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.update(ENGINE_BYTES).is_err() {
            if !valid.load(Ordering::Acquire) || Instant::now() >= deadline {
                bail!("terminal checkpoint aggregate capacity");
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
        Ok(())
    }
}
impl Drop for MemoryUsage {
    fn drop(&mut self) {
        MIRROR_BYTES.fetch_sub(self.0, Ordering::AcqRel);
    }
}

fn run(
    receiver: mpsc::Receiver<Command>,
    valid: &AtomicBool,
    queued: &AtomicUsize,
    rows: u16,
    cols: u16,
) -> Result<()> {
    // Reserve the *maximum* heap before executing, then return unused budget.
    // Concurrent engines cannot all overshoot the aggregate limit first.
    let mut memory = MemoryUsage(0);
    memory.reserve(valid)?;
    let runtime = Runtime::new()?;
    runtime.set_memory_limit(ENGINE_BYTES);
    runtime.set_max_stack_size(512 << 10);
    let deadline = Arc::new(Mutex::new(Instant::now() + Duration::from_secs(2)));
    let interrupt_deadline = deadline.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        Instant::now() > *interrupt_deadline.lock().unwrap()
    })));
    let context = Context::full(&runtime)?;
    context.with(|ctx| -> rquickjs::Result<()> {
        ctx.eval::<(), _>(PRELUDE)?;
        ctx.eval::<(), _>(ENGINE_SOURCE)?;
        ctx.eval::<(), _>(format!(
            "var __term = new exports.Terminal({{rows:{rows},cols:{cols},scrollback:Math.min(10000,Math.floor(200000/{cols})),allowProposedApi:true}});"
        ))?;
        ctx.eval::<(), _>(r#"
            var __unsupported = false;
            var __archive = new CheckpointArchive(__term);
            for (const id of [7, 133, 633]) __term.parser.registerOscHandler(id, () => false);
            __term.parser.registerDcsHandler({final:'q'}, () => { __unsupported = true; return false; });
            __term.parser.registerOscHandler(1337, () => { __unsupported = true; return false; });
            function __append(text) { parseCheckpointOutput(__term, text); __flushTimers(); }
            function __resize(rows, cols) { __term.options.scrollback = Math.min(10000,Math.floor(200000/cols)); __term.resize(cols, rows); __archive.resized(); __flushTimers(); }
            function __snapshot(history) {
                const parser = __term._core._inputHandler._parser;
                if (__unsupported || parser._dcsParser._ident === 113 || parser._oscParser._id === 1337) throw Error('image checkpoint unavailable');
                const state = __archive.snapshot(Math.min(history, 10000));
                return JSON.stringify(state);
            }
            function __history(epoch, before, count) { return JSON.stringify(__archive.history(epoch, before, count)); }
        "#)?;
        Ok(())
    })?;
    memory.update(runtime.memory_usage().memory_used_size.max(0) as usize)?;
    let mut sequence = 0;
    while let Ok(command) = receiver.recv() {
        if !valid.load(Ordering::Acquire) {
            break;
        }
        memory.reserve(valid)?;
        *deadline.lock().unwrap() = Instant::now() + EXECUTION_LIMIT;
        match command {
            Command::Stop => break,
            Command::Output {
                sequence: start,
                text,
            } => {
                queued.fetch_sub(text.len(), Ordering::AcqRel);
                if start != sequence {
                    bail!("terminal checkpoint sequence gap");
                }
                context
                    .with(|ctx| {
                        ctx.globals()
                            .get::<_, Function>("__append")?
                            .call::<_, ()>((text.as_str(),))
                    })
                    .map_err(|_| MirrorPhase("output-parse"))?;
                sequence += text.len() as u64;
            }
            Command::Resize(rows, cols) => {
                context
                    .with(|ctx| {
                        ctx.globals()
                            .get::<_, Function>("__resize")?
                            .call::<_, ()>((rows, cols))
                    })
                    .map_err(|_| MirrorPhase("resize"))?;
            }
            Command::Snapshot {
                history,
                archive,
                reply,
            } => {
                if reply.is_closed() {
                    memory.update(runtime.memory_usage().memory_used_size.max(0) as usize)?;
                    continue;
                }
                let data = context.with(|ctx| match archive {
                    Some((epoch, before)) => ctx
                        .globals()
                        .get::<_, Function>("__history")?
                        .call::<_, String>((epoch, before, history)),
                    None => ctx
                        .globals()
                        .get::<_, Function>("__snapshot")?
                        .call::<_, String>((history,)),
                });
                let response = match data {
                    Ok(data) if data.len() <= MAX_RESPONSE_BYTES => {
                        Ok(Checkpoint { sequence, data })
                    }
                    _ => Err(anyhow::anyhow!(
                        "terminal checkpoint unavailable for this state"
                    )),
                };
                reply.send(response).ok();
            }
        }
        memory.update(runtime.memory_usage().memory_used_size.max(0) as usize)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn embedded_engine_restores_latest_with_exact_utf8_fence() -> Result<()> {
        let mirror = TerminalMirror::new(3, 20);
        let output = "old\r\nolder\r\n中文\r\nlatest\x1b[3";
        mirror.append(0, output);
        let state = mirror.snapshot(1).await?;
        assert_eq!(state.sequence, output.len() as u64);
        let value: serde_json::Value = serde_json::from_str(&state.data)?;
        assert_eq!(value["normal"]["lines"].as_array().unwrap().len(), 4);
        assert_eq!(value["parser"]["state"], 4);
        Ok(())
    }

    #[tokio::test]
    async fn gap_or_capacity_failure_does_not_wait_forever() {
        let mirror = TerminalMirror::new(3, 20);
        mirror.append(10, "gap");
        assert!(mirror.snapshot(1).await.is_err());
        let mirror = TerminalMirror::new(3, 20);
        mirror.append(0, &"x".repeat(65_537));
        assert!(mirror.snapshot(1).await.is_err());
    }

    #[tokio::test]
    async fn unsupported_graphics_falls_back_without_stopping_other_mirrors() -> Result<()> {
        let bad = TerminalMirror::new(3, 20);
        bad.append(0, "\x1bPq~\x1b\\");
        assert!(bad.snapshot(1).await.is_err());
        let good = TerminalMirror::new(3, 20);
        good.append(0, "prompt");
        assert_eq!(good.snapshot(1).await?.sequence, 6);
        Ok(())
    }

    #[tokio::test]
    async fn large_retained_replay_remains_bounded_and_reaches_the_latest_fence() -> Result<()> {
        let mirror = TerminalMirror::new(24, 89);
        let mut sequence = 0;
        let text = "\x1b[32mworking\x1b[0m\r\n".repeat(500);
        for _ in 0..100 {
            let until = Instant::now() + Duration::from_secs(5);
            while !mirror.append(sequence, &text) {
                if !mirror.valid.load(Ordering::Acquire) || Instant::now() >= until {
                    bail!("mirror could not catch up");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            sequence += text.len() as u64;
        }
        // Match viewer retries: one deadline may expire behind a replay burst,
        // especially alongside the suite's concurrent Argon2/retention tests.
        let until = Instant::now() + Duration::from_secs(10);
        let snapshot = loop {
            match mirror.snapshot(12).await {
                Ok(snapshot) => break snapshot,
                Err(_) if mirror.valid.load(Ordering::Acquire) && Instant::now() < until => {}
                Err(error) => return Err(error),
            }
        };
        assert_eq!(snapshot.sequence, sequence);
        let state: serde_json::Value = serde_json::from_str(&snapshot.data)?;
        assert_eq!(state["normal"]["lines"].as_array().unwrap().len(), 36);
        Ok(())
    }
}
