<p align="center">
  <img src="static/favicon.svg" width="96" height="96" alt="sshxx icon">
</p>

<h1 align="center">sshxx</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a>
</p>

Self-hosted persistent terminals on a collaborative, multi-page canvas. Use the
same workspace from a browser or the Tauri client while `sshxx-daemon` keeps the
shells alive independently of every viewer.

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

| Area                 | What sshxx provides                                                                        |
| -------------------- | ------------------------------------------------------------------------------------------ |
| Persistent terminals | Local and OpenSSH shells owned by the daemon, not the browser                              |
| Shared canvas        | Page-aware terminals, notes, file windows, layout, links, and live presence                |
| Structured notes     | Multiline paragraphs, linked targets, drag-to-copy, send, and send-and-run actions         |
| Files beside shells  | Synchronized folder navigation, previews, CodeMirror editing, uploads, and file operations |
| Viewer choice        | One Svelte interface for the Web and Tauri-based packaged client                           |
| Local control        | Browser-local page, viewport, full-screen, theme, focus, and undo/redo state               |

The README intentionally stays at project level. See the
**[complete Feature Guide](https://github.com/glight2000/sshxx/wiki/Features)**
for the full capability set and screenshots, or start from the
**[sshxx Wiki](https://github.com/glight2000/sshxx/wiki)**.

## Architecture

| Component      | Source               | Responsibility                                                              |
| -------------- | -------------------- | --------------------------------------------------------------------------- |
| `sshxx-daemon` | `crates/sshx-daemon` | Owns shell/SSH processes, filesystem operations, and durable workspace data |
| `sshxx-server` | `crates/sshx-server` | Authorizes and coordinates encrypted, page-aware sessions                   |
| `sshxx-client` | `src/`, `src-tauri/` | Renders and controls a session in the browser or packaged app               |

Closing or refreshing a viewer does not end its terminals. Restarting the daemon
restores workspace metadata but recreates the shell processes; it does not
resume their prior in-memory state.

## State and trust boundaries

| State                                                      | Owner and lifetime                                                       | Scope                                                                      |
| ---------------------------------------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------- |
| Pages, canvas items, notes/links, file-window/editor state | Daemon `.sshx-workspace`                                                 | Shared within the session; every canvas mutation retains its page ID       |
| Shell and SSH processes                                    | Daemon memory                                                            | Shared stream/input; survives viewer disconnects, not daemon restarts      |
| Reusable SSH profiles                                      | Authenticated-encrypted `.sshx-connections` with an owner-only local key | Session-visible metadata; writer-only mutation; passwords are never stored |
| Active page, per-page pan/zoom, user settings              | Browser `localStorage`                                                   | Local to one browser profile and never synchronized                        |
| Focus, menus, drag state, full-screen, undo/redo           | Browser memory                                                           | Local and temporary                                                        |
| Presence and editing ownership                             | Server memory                                                            | Transient within the session                                               |

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
docker compose up -d
mprocs
```

The default development session is:

```text
http://localhost:5173/s/dev#localdevkey
```

The daemon writes its application data relative to its current working
directory. Start it from the same directory to restore the same workspace.
`.sshx-workspace`, `.sshx-connections`, `.sshx-connections.key`, `cache/`, and
their recovery files are local application data and are intentionally ignored by
Git.

## Build

```shell
cargo build --release -p sshxx-daemon -p sshxx-server
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
- [Keyboard and mouse controls](https://github.com/glight2000/sshxx/wiki/Keyboard-and-Mouse)
- [Architecture, persistence, synchronization, and security](https://github.com/glight2000/sshxx/wiki/Architecture-and-State)
- Versioned Wiki sources: [`docs/wiki`](docs/wiki/Home.md)

<details>
<summary><strong>Roadmap and known limitations</strong></summary>

### TODO

- [ ] Validate signed desktop installers and Android/iOS targets in CI.
- [ ] Publish a maintained production deployment reference with TLS, upgrades,
      backups, and recovery.
- [ ] Add versioned workspace migrations and end-to-end browser coverage for
      pages, notes, snapping, search, terminal input, and concurrent editing.
- [ ] Lazy-load the file editor and reduce the initial Web language registry.
- [ ] Add a capability-checked xterm compatibility adapter around the isolated
      private SGR access used by TypeAhead.
- [ ] Design an explicit daemon-to-client process-status protocol before adding
      AI-agent identification or semantic completion notifications.

### Known limitations

- Daemon restart restores metadata but recreates shell processes.
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
