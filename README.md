<p align="center">
  <img src="static/favicon.svg" width="96" height="96" alt="sshxx icon">
</p>

<h1 align="center">sshxx</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a>
</p>

Self-hosted persistent terminals on a collaborative, multi-page canvas. Use the
same workspace from a browser or the Tauri client while the local terminal host
keeps shells alive independently of every viewer and daemon restart.

![sshxx note actions connected to a persistent terminal and file editor](docs/images/sshxx-notes.png)

sshxx is derived from [ekzhang/sshx](https://github.com/ekzhang/sshx). Thank you
to Eric Zhang and all upstream contributors for the original architecture and
implementation. This repository preserves the upstream Git history and MIT
license, then extends the project for a different set of personal workflows.

> **Self-hosted by default:** sshxx never selects or recommends the upstream
> public service at `sshx.io`, and this project does not guarantee compatibility
> with or provide support for it. A user may still connect intentionally after
> explicit acknowledgement: pass `--allow-upstream-service` to `sshxx-daemon` or
> confirm the warning shown by the packaged client. For supported operation,
> deploy and connect to your own `sshxx-server`.

## At a glance

| Area                 | What sshxx provides                                                                          |
| -------------------- | -------------------------------------------------------------------------------------------- |
| Persistent terminals | Local and OpenSSH shells owned by an independent local host, not a browser or daemon process |
| Shared canvas        | Page-aware terminals, notes, file windows, layout, links, and live presence                  |
| Structured notes     | Multiline paragraphs, block selection/reordering, structured copy, links, and delivery       |
| Files beside shells  | Synchronized folder navigation, previews, CodeMirror editing, uploads, and file operations   |
| Viewer choice        | One Svelte interface for the Web and Tauri-based packaged client                             |
| Local control        | Browser-local page, viewport, full-screen, theme, focus, and undo/redo state                 |

The README intentionally stays at project level. See the
**[complete Feature Guide](https://github.com/glight2000/sshxx/wiki/Features)**
for the full capability set and screenshots, or start from the
**[sshxx Wiki](https://github.com/glight2000/sshxx/wiki)**.

## Architecture

| Component             | Source                       | Responsibility                                                               |
| --------------------- | ---------------------------- | ---------------------------------------------------------------------------- |
| `sshxx-terminal-host` | `crates/sshxx-terminal-host` | Owns PTY/ConPTY handles and shell/SSH processes across daemon restarts       |
| `sshxx-daemon`        | `crates/sshx-daemon`         | Bridges hosted terminals, performs filesystem operations, and persists state |
| `sshxx-server`        | `crates/sshx-server`         | Authorizes and coordinates encrypted, page-aware sessions                    |
| `sshxx-client`        | `src/`, `src-tauri/`         | Renders and controls a session in the browser or packaged app                |

The supported connection path is `viewer ↔ server ↔ daemon ↔ terminal-host`. The
daemon has no browser-facing listener and cannot be used alone. A minimal
self-hosted workspace therefore needs the server, daemon, terminal host, and the
Web build shipped with the server. The Tauri client is an optional viewer, not a
replacement for those runtime services.

Closing or refreshing a viewer does not end its terminals. Restarting or
upgrading the daemon reconnects to the same hosted PTYs and processes.
Restarting `sshxx-terminal-host` remains destructive and is deliberately never
automatic.

In the default single-server mode, a server restart briefly disconnects viewers;
the daemon automatically recreates a missing server session from its durable
workspace and reattaches the hosted terminals. A configured fixed session name
preserves the URL. Without one, the replacement receives a new random URL.

## Install and run

The release installer downloads the matching Linux/macOS runtime archive,
verifies its SHA-256 checksum, installs all three required executables plus the
Web client, and starts a local server and daemon:

```shell
curl -fsSL https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.sh | sh -s -- --run
```

Windows PowerShell (x64):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/glight2000/sshxx/main/scripts/install.ps1))) -Run
```

The daemon prints the session URL and stores its local state in the directory
where the command was started. Omit `--run`/`-Run` to install without starting
the local stack. Desktop installers are optional downloads on the
[Releases page](https://github.com/glight2000/sshxx/releases); complete package,
platform, signing, and operator details are in
**[Installation and Releases](https://github.com/glight2000/sshxx/wiki/Installation-and-Releases)**.

## State and trust boundaries

| State                                                      | Owner and lifetime                                                       | Scope                                                                                           |
| ---------------------------------------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| Pages, canvas items, notes/links, file-window/editor state | Daemon `.sshx-workspace`                                                 | Shared within the session; includes the non-secret SSH profile ID used by each remote terminal  |
| Shell and SSH processes                                    | `sshxx-terminal-host` memory                                             | Shared stream/input; survives viewer and daemon restarts, not host/OS restarts                  |
| Per-terminal shell history                                 | Daemon launch policy and owner-only local history data                   | One history namespace/file per local terminal; nested remote shells follow remote configuration |
| Reusable SSH profiles                                      | Authenticated-encrypted `.sshx-connections` with an owner-only local key | Session-visible metadata; writer-only mutation; passwords are never stored                      |
| Active page, per-page pan/zoom, user settings              | Browser `localStorage`                                                   | Local to one browser profile and never synchronized                                             |
| Focus, menus, drag state, full-screen, undo/redo           | Browser memory                                                           | Local and temporary                                                                             |
| Presence and editing ownership                             | Server memory                                                            | Transient within the session                                                                    |

Terminal streams, filesystem payloads, image chunks, and active editor content
are encrypted through the server. Coordination metadata remains visible to the
server. A write-capable participant can exercise the daemon or SSH account's
terminal and filesystem permissions: sshxx is not a filesystem sandbox.

Use HTTPS/WSS outside localhost or a trusted isolated LAN, and treat URL
fragments as bearer secrets. The complete persistence, synchronization,
communication, Redis, authorization, and data-visibility contract is documented
in
**[Architecture and State](https://github.com/glight2000/sshxx/wiki/Architecture-and-State)**.

## Development

The repository follows its lockfiles and is intended to use `mise`-managed
runtimes:

```shell
mise install
npm ci
mprocs
```

Normal development and single-server deployments do not use Redis. The
repository keeps an opt-in service only for multi-server coordination tests:

```shell
docker compose --profile multi-server up -d
cargo run -p sshxx-server --features redis-mesh -- \
  --redis-url redis://localhost:12601 # add to each test server
```

The default server build excludes Redis support. Without the explicit build
feature, Compose profile, and `--redis-url`, Redis remains disabled.

The default development session is:

```text
http://localhost:5173/s/dev#localdevkey
```

The daemon writes its application data relative to its current working
directory. Start it from the same directory to restore the same workspace.
`.sshx-workspace`, `.sshx-connections`, `.sshx-connections.key`, `cache/`, and
their recovery files are local application data and are intentionally ignored by
Git.

The daemon normally discovers or starts its sibling host outside a service
manager. Manual lifecycle commands are:

```shell
sshxx-daemon terminal-host start
sshxx-daemon terminal-host status
sshxx-daemon terminal-host stop
sshxx-daemon terminal-host restart
```

`stop` and `restart` reject active terminals unless `--force` explicitly
acknowledges process loss. Production service managers must run the host in a
separate unit; restarting the daemon unit must not restart the host unit.

Routine daemon releases leave a compatible running host untouched. A host
upgrade is deliberately deferred: install the new binary, inspect
`terminal-host status`, and restart it only after the active-terminal list is
empty. `restart --force` is destructive and should be reserved for cases where
losing every hosted shell, nested SSH connection, and foreground application is
acceptable. Under systemd, check status first and then restart the independent
`sshxx-terminal-host.service`, followed by `sshxx-daemon.service`. See
**[Architecture and State](https://github.com/glight2000/sshxx/wiki/Architecture-and-State#terminal-host-lifecycle-and-upgrades)**
for the complete upgrade contract.

## Build

```shell
cargo build --release -p sshxx-daemon -p sshxx-server -p sshxx-terminal-host
npm run build
npm run app:build
```

The packaged client requires the platform-specific Tauri system dependencies. On
Ubuntu:

```shell
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

For production, run the server behind TLS with a strong secret and use Redis
when coordinating multiple server instances.

## Documentation

- [Complete feature guide and screenshots](https://github.com/glight2000/sshxx/wiki/Features)
- [Installation, runtime packages, and releases](https://github.com/glight2000/sshxx/wiki/Installation-and-Releases)
- [Keyboard and mouse controls](https://github.com/glight2000/sshxx/wiki/Keyboard-and-Mouse)
- [Architecture, persistence, synchronization, and security](https://github.com/glight2000/sshxx/wiki/Architecture-and-State)
- Versioned Wiki sources: [`docs/wiki`](docs/wiki/Home.md)
- [Documentation ownership and maintenance map](docs/README.md)

<details>
<summary><strong>Roadmap and known limitations</strong></summary>

### TODO

- [ ] Add trusted desktop code signing/notarization and validate Android/iOS
      targets.
- [ ] Publish a maintained production deployment reference with TLS, upgrades,
      backups, and recovery.
- [ ] Add versioned workspace migrations and end-to-end browser coverage for
      pages, notes, snapping, search, terminal input, and concurrent editing.
- [ ] Design an explicit daemon-to-client process-status protocol before adding
      AI-agent identification or semantic completion notifications.

### Known limitations

- A terminal-host or operating-system restart disconnects hosted processes;
  application-specific recovery such as Codex resume remains manual.
- The terminal host retains bounded raw PTY output rather than an emulator
  screen snapshot. After high-volume output rolls over that buffer, a renderer
  rebuild may not reproduce the exact previous screen even though the process
  remains alive.
- Windows ConPTY cannot currently report a child process working directory, so
  terminal duplication may fall back to the daemon directory.
- Image paste currently targets local daemon shells; remote SSH forwarding still
  needs an SFTP/SCP flow.
- `Shift+Enter` sends LF, but the foreground application decides whether that is
  a line break or submission.
- Attention effects require a terminal bell or supported OSC notification; they
  do not infer AI-agent state.
- WebGL falls back to DOM terminal rendering when unavailable, with lower
  performance for large terminals.
- Plain HTTP/WebSocket is only suitable for trusted local networks.

</details>

## Validation

```shell
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
npm run lint
npm run check
npm run test:runtime
npm run build
```

## Upstream and license

sshxx is published as an independent repository, so GitHub may not show a
“forked from” badge. Add the original project as an `upstream` remote when
needed:

```shell
git remote add upstream https://github.com/ekzhang/sshx.git
```

sshxx inherits the upstream [MIT License](LICENSE), including its original
copyright and license notice.
