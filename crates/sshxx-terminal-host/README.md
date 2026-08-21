# sshxx-terminal-host

`sshxx-terminal-host` is the long-lived local PTY owner for
[sshxx](https://github.com/glight2000/sshxx). It keeps terminal processes alive
while `sshxx-daemon` disconnects, crashes, restarts, or is upgraded.

This is deliberately a small, low-frequency component. It does not render a
terminal, expose a network service, understand canvas state, or contain shell
policy. It owns PTY/ConPTY handles and transports raw terminal bytes over an
authenticated local connection.

## Lifecycle contract

| Event                                    | Terminal process result                                |
| ---------------------------------------- | ------------------------------------------------------ |
| Browser refresh/disconnect               | Unaffected                                             |
| `sshxx-daemon` exit/restart/upgrade      | Unaffected; the daemon reconnects                      |
| Explicit terminal close                  | The host closes that PTY and process                   |
| `sshxx-terminal-host` stop/crash/upgrade | All hosted terminal processes disconnect               |
| Operating-system restart                 | Processes stop; application-level recovery is required |

The daemon may discover and start the host, but the host is an independent OS
process and never exits merely because daemon connections disappear. Under an OS
service manager, use a separate host service/unit. Do not place the host in a
daemon unit whose cgroup is killed during daemon restart.

## Disruptive upgrades

Host upgrades are intentionally never automatic:

- Protocol negotiation allows compatible daemon releases to keep using the
  already-running host binary.
- `start` reports a version mismatch and leaves the running host untouched.
- `stop` is rejected while terminal processes are active.
- `stop --force` is the explicit destructive operation. It warns in its help and
  disconnects every remaining hosted process.

Before upgrading the host, users must inspect `status`, recover or close their
terminal processes, and acknowledge that application-specific state (for example
a Codex or nested SSH session) may require manual recovery.

## Protocol and security boundary

- Protocol: length-prefixed protobuf with explicit version negotiation.
- Transport: Unix domain socket on Unix; named pipe on Windows.
- Socket directory mode on Unix: `0700`; socket and token mode: `0600`.
- Authentication: a random local token of at least 256 bits.
- Terminal output: raw PTY bytes with absolute byte sequence numbers.
- Replay: bounded 16 MiB rolling buffer per terminal.
- No TCP listener and no remote API.
- A daemon disconnect only removes its subscriptions; it does not close PTYs.

The default local state directory is `cache/terminal-host` relative to the
working directory. The host owns the local socket and authentication token; the
daemon adds its stable instance ID and per-terminal shell-history files.
Terminal transcripts remain in memory and are not persisted to disk by the host.

## Per-terminal history

Shell-history policy belongs to `sshxx-daemon`, not this host. The daemon gives
each stable terminal UUID its own launch environment, for example a unique
`HISTFILE` for Bash/Zsh or history identifier for Fish. The host passes that
environment to the process unchanged. This keeps the host protocol stable when
shell-specific policy evolves.

Manually nested remote SSH shells still require corresponding remote-shell
configuration if their command history must also be isolated.

## Development

The project uses the repository lockfile and the machine's mise-managed Rust
toolchain.

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Manage the host through the daemon CLI (recommended outside an OS service
manager):

```bash
cargo run -p sshxx-daemon -- terminal-host start
cargo run -p sshxx-daemon -- terminal-host status
cargo run -p sshxx-daemon -- terminal-host stop
cargo run -p sshxx-daemon -- terminal-host restart
```

`stop` and `restart` refuse while terminals are active. Adding `--force` is an
explicit destructive acknowledgement. A systemd deployment uses a separate
`sshxx-terminal-host.service`; routine daemon restarts must not restart it.

`portable-pty` provides the Unix PTY and Windows ConPTY implementations. Linux
is covered by lifecycle integration tests. Windows named-pipe and ConPTY
validation must be run on a Windows test machine before declaring that target
production-ready.
