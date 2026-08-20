# sshxx

sshxx is a self-hosted, persistent, collaborative terminal with browser and
cross-platform app viewers. Terminal processes are owned by a daemon rather than
by the browser, so closing or refreshing a viewer does not end the shell.

This project is derived from [sshx](https://github.com/ekzhang/sshx), created by
[Eric Zhang](https://github.com/ekzhang). Thank you to Eric and every upstream
contributor for the original architecture and implementation. sshxx keeps that
foundation while adding features and interaction changes for a different set of
personal workflows.

## Upstream relationship

sshxx is published as an independent repository rather than a GitHub fork, so
GitHub may not display a “forked from” badge. The repository retains the
upstream Git history and MIT license. Add the original project as an `upstream`
remote when suitable upstream changes need to be reviewed or merged:

```shell
git remote add upstream https://github.com/ekzhang/sshx.git
```

## What changed

- Browser and Tauri-based desktop/mobile viewers share one Svelte frontend.
- The daemon persists pages, terminal layout, appearance, and notes in the
  current working directory.
- Multiple independent canvas pages, global search, editable notes, terminal
  duplication, appearance controls, and optional grid snapping.
- Character-level note synchronization and page-aware collaboration events.
- Updated terminal rendering and frontend/backend dependencies.
- Product binaries are named `sshxx-daemon`, `sshxx-server`, and `sshxx-client`
  to distinguish them from upstream sshx.

## Architecture

| Component      | Source               | Responsibility                                        |
| -------------- | -------------------- | ----------------------------------------------------- |
| `sshxx-daemon` | `crates/sshx-daemon` | Owns shell processes and local workspace persistence. |
| `sshxx-server` | `crates/sshx-server` | Coordinates sessions and serves the Web client/API.   |
| `sshxx-client` | `src/`, `src-tauri/` | Displays sessions in a browser or packaged app.       |

The server coordinates encrypted terminal data but does not own the shell
process. The daemon continues running terminals when all viewers disconnect.

## Development

The repository follows its lockfiles and is intended to use the runtimes managed
by `mise`. Install the declared JavaScript dependencies and start the
development Redis service:

```shell
mise install
npm ci
docker compose up -d
```

Run the server, daemon, and Web viewer together:

```shell
mprocs
```

The default development session is available at:

```text
http://localhost:5173/s/dev#localdevkey
```

The daemon stores workspace metadata in `.sshx-workspace` in its current
directory. This compatibility filename is intentionally retained from sshx so
existing workspaces continue to load. Start the daemon from the same directory
to restore the saved pages, notes, layout, and terminal configuration. Shell
processes themselves are recreated.

## Build

Build the daemon and server:

```shell
cargo build --release -p sshxx-daemon -p sshxx-server
```

Build the static Web client:

```shell
npm run build
```

Build the packaged client after installing the platform-specific Tauri system
dependencies:

```shell
npm run app:build
```

On Ubuntu, the native dependencies are:

```shell
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

LAN HTTP/WebSocket endpoints are supported by the packaged app. Use HTTPS and
WSS on untrusted networks.

## Run separately

Start a local server:

```shell
./target/release/sshxx-server \
  --listen :: \
  --secret replace-this-secret
```

Start the daemon from the directory in which its workspace should be saved:

```shell
./target/release/sshxx-daemon --server http://localhost:8051
```

For production, place the server behind an appropriate reverse proxy, configure
TLS, use a strong secret, and provide Redis when running multiple server
instances.

## Validation

```shell
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
npm run lint
npm run check
npm run test:runtime
npm run build
```

## License

sshxx inherits the upstream [MIT License](LICENSE). The original copyright and
license notice are retained unchanged. See the upstream
[sshx repository](https://github.com/ekzhang/sshx) for the original project and
its history.
