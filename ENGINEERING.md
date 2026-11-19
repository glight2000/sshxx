# Engineering

This document owns development dependencies, build and validation procedures,
and the terminal clipboard, page ordering and canvas input implementations.
Existing state, protocol and trust boundaries remain in
[Architecture and State](docs/wiki/Architecture-and-State.md). User-visible
requirements live in [REQUIREMENTS.md](REQUIREMENTS.md).

## Toolchain and dependencies

`mise.toml` is the local toolchain source of truth. Use `mise install` and
`mise exec -- <command>`; keep the CI/release workflows and Docker build stages
aligned with those versions. Node stays on the 24 LTS line, with matching
`@types/node`. npm follows `packageManager` and `package-lock.json`; Rust
follows `Cargo.lock`. Update lockfiles and run the affected validation gates
together. The optional Redis image is pinned in `compose.yaml`; updating it does
not start Redis or migrate an existing deployment.

### 2026-10-24 dependency migration

- SvelteKit 3 configuration lives in `vite.config.ts`. Package subpath imports
  use `#lib/*` with explicit extensions, environment imports use `$app/env`, and
  `tsconfig.json` extends `$app/tsconfig`. The static SPA fallback remains
  `spa.html`; deployment polling remains disabled.
- TypeScript 7 supplies `tsc`, run after Svelte diagnostics by `npm run check`.
  The `typescript` package name aliases the official TypeScript 6 compatibility
  package for Svelte checking, ESLint and test code that still require the
  JavaScript compiler API. Its transitive compiler stays on TypeScript 6. This
  follows the
  [upstream compatibility guidance](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/).
- npm 12 requires an install-script allowlist. `allowScripts` in `package.json`
  permits the pinned `cbor-extract` native build, which checks bundled platform
  binaries before attempting a local build. Review new script-bearing packages
  or versions before extending that list.
- dekit replaces the renamed mprocs tool. `start.sh` uses `dekit mprocs` with
  the existing `mprocs.yaml`; setup, logs and shutdown are documented in the
  [README](README.md#development).
- Web and headless xterm remain paired at version 6.0.0. The checkpoint adapter
  and embedded parser depend on that exact pair; do not adopt prereleases or
  update only one side. QuickJS and cryptographic library upgrades must pass
  existing parser-continuation, cross-language encryption and persistence tests.

The Electron shell has its own npm lockfile under `clients/electron`. Audit its
builder dependencies separately from the root client. The Svelte formatter
plugin stays at 4.1.1 to preserve the repository's existing markup formatting.
Target-specific deployment addresses, credentials and runbooks stay private.

## Source deployment on Linux

Use `./start.sh build-runtime /absolute/path/to/new-build` for an existing
single-server systemd test instance. Run it as the service account with the
pinned mise tools, a native C toolchain, Make, protoc, and GNU coreutils and
flock installed. It installs the locked npm dependencies, builds the Web client
and the two release-mode binaries, and publishes a new directory after all three
succeed. Use a local disk readable by the service account. Build from a separate
checkout if a Vite development server also needs to run during builds.

For activation, point a local `current` symlink at the completed directory. Use
machine-local systemd drop-ins for the server and daemon. Choose filenames that
sort after existing site overrides. After reloading systemd, verify that each
unit's effective `ExecStart` selects the source binary before restarting.
Preserve each unit's existing arguments, user, and environment files when
overriding its `ExecStart` to use `current/bin/sshxx-server` or
`current/bin/sshxx-daemon`. Set the server's `WorkingDirectory` to `current` so
it serves the matching `build/`. Keep the daemon's durable working directory and
existing terminal-host service. Set `UnsetEnvironment=SSHXX_WEB_UPDATE` on the
daemon and suspend any site-specific GitHub update jobs for this instance while
testing source builds.

Reload systemd after changing drop-ins. To apply subsequent edits, build a fresh
directory, switch the symlink atomically, then restart `sshxx-server.service`
and `sshxx-daemon.service` together. Browsers reconnect and can reload the new
client. Verify both executables and the server's working directory via the
running PIDs before retiring a superseded build. Keep the previous build for
rollback. To return to GitHub releases, remove only the source drop-ins, reload
systemd, restart the same two units and restore the site's updater.

## SSH directory reporting

The terminal consumes OSC 7 file URIs from the live output stream and the
encrypted terminal checkpoint. Its folder button uses the decoded directory with
the file browser attached to the same terminal. OpenSSH SFTP uses the daemon's
saved connection, so the reporting hostname never selects a destination.

For an interactive remote Bash account that does not already emit OSC 7, add the
following to the remote `~/.bashrc`. It appends to existing prompt commands,
preserves the previous exit status and percent-encodes path bytes, including
spaces, Unicode and shell punctuation.

```bash
sshxx_report_directory() {
  local status=$? LC_ALL=C path= byte escaped i
  for ((i = 0; i < ${#PWD}; i++)); do
    byte=${PWD:i:1}
    case $byte in
      [a-zA-Z0-9/._~-]) path+=$byte ;;
      *) printf -v escaped '%%%02X' "'$byte"; path+=$escaped ;;
    esac
  done
  printf '\033]7;file://%s%s\033\\' "$HOSTNAME" "$path"
  return "$status"
}
if [[ $- == *i* && " ${PROMPT_COMMAND[*]} " != *" sshxx_report_directory "* ]]; then
  PROMPT_COMMAND+=(sshxx_report_directory)
fi
```

The next prompt reports the new directory. SFTP requires a profile that can
authenticate non-interactively, such as a key or SSH agent.

### Live directory following

An enabled file window queries `terminalDirectory` through the existing
encrypted, page-checked file request channel. A local reply contains the live
shell process directory. If the terminal host cannot supply it, the UI shows the
error. The viewer checks again 1.5 seconds after a successful local probe. A
saved SSH profile runs the fixed command `hostname` in batch mode at the
configured destination, once per active binding. Its bounded result identifies
which live OSC 7 reports to use, including when the profile uses an IP address
or a config alias. Reports must match that hostname after case and trailing-dot
normalization. Terminal output never supplies an SSH destination.

Each viewer owns its follow switch and transient hostname, report and timer. One
follow request runs at a time per enabled window. A newer target, disabled
switch, disconnect or changed binding invalidates an outstanding result.
Historical replay and checkpoints do not drive automatic navigation. The server
applies a page- and generation-checked navigation update to the latest file
window. It preserves geometry, stacking order and any open editor. The existing
file-window snapshot carries the resulting directory to viewers. Following does
not create file windows, persist a subscription or change SSH credentials. The
server advertises the follow capability only when the daemon supports the query.
Older deployments retain ordinary file browsing.

OpenSSH starts an SFTP subsystem for file operations. If the daemon user's
OpenSSH config enables connection multiplexing for the same destination, OpenSSH
can share the underlying transport through its control socket. An SSH client
started on a jump host has its control socket on that host. The daemon cannot
attach to it through the interactive terminal. Password-only profiles are
excluded from file browsing.

## Keeping terminal state through a daemon restart

The terminal host owns the live PTY. A daemon restart reattaches using the
workspace terminal ID. This retains the foreground process, working directory
and in-memory shell history while the original PTY lives. After an interrupted
host subscription, local and saved SSH windows first reattach to their own PTYs.
A local window closes if its original PTY is gone. For source-derived windows,
orphan replacement applies only to the first launch. Both process replacement
and graceful system-service shutdown drain the workspace writer before the final
save. A remote disconnect or a terminal-host restart can end the process itself.

Saved SSH windows can isolate their **remote Bash history** and recover their
last prompt directory if the SSH process has to be recreated. To enable this on
a remote account:

1. Have the remote OpenSSH server accept `SSHXX_HISTORY_ID` through its
   `AcceptEnv` configuration. The daemon requests this variable only for saved
   interactive SSH profiles. SFTP continues to use the original profile.
2. Install [the Bash integration](scripts/remote-bash-state.bash) on that
   account, for example as `~/.config/sshxx/remote-bash-state.bash`. Source it
   at the end of the interactive Bash startup file, after normal history and
   prompt configuration:

   ```bash
   [[ -r ~/.config/sshxx/remote-bash-state.bash ]] &&
     source ~/.config/sshxx/remote-bash-state.bash
   ```

3. Open a new saved SSH window. The shell writes its own history (up to 10,000
   entries, respecting smaller limits and history exclusions) and its last
   prompt directory under `${XDG_STATE_HOME:-$HOME/.local/state}/sshxx/` on the
   remote host. The directory is private to that account. A supplied “Open
   terminal here” directory takes precedence. The hook leaves other logins
   alone.

The daemon retains only the opaque history namespace beside its per-terminal
local history. Closing the window retires that namespace; a duplicate gets a
separate one. Remote files remain under the remote account's control. An
already-running SSH login keeps its current environment on reattach. Shells
started through a manually typed nested SSH command use the remote account's
normal shell configuration. sshxx does not record the PTY input stream or
re-execute old commands to recreate a program.

## Build

```shell
mise install
mise exec -- npm ci
mise exec -- cargo build --release -p sshxx-daemon -p sshxx-server -p sshxx-terminal-host
mise exec -- npm run build
mise exec -- npm run app:build
```

The daemon embeds its pinned headless terminal parser during compilation, so
`npm ci` must precede Cargo builds. Installed Runtime packages need neither
Node.js nor a separate parser service. The packaged client also requires Tauri's
platform dependencies; on Ubuntu these are:

```shell
sudo apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

Production servers require TLS and a strong secret; Redis is only needed for
multiple coordinated servers. Building or updating dependencies does not restart
an existing terminal host, deploy a service, or publish a release. Keep the
independent component versions and host lifecycle described in
[Architecture and State](docs/wiki/Architecture-and-State.md).

## Terminal clipboard

`src/lib/terminalClipboard.ts` owns terminal clipboard transport. On a copy
shortcut, cancel the browser default action synchronously before invoking
`Clipboard.writeText()`: returning `false` from xterm's custom key callback only
stops xterm processing, not the browser's native copy. Running both allows a
later native copy to replace the text after an asynchronous success
notification. Without the async API, the synchronous `copy` command uses xterm's
own selection handler and must report success before the UI clears the
selection.

Only a successful write produces a success toast. Rejections preserve the
selection; completion after a selection change does not clear the newer text,
and completion after disposal does not update the UI. The captured selection is
temporary browser-local state: it is neither persisted nor synchronized and is
never sent to the server or PTY.

Text paste events inside xterm are consumed once through public `paste()`. xterm
owns newline normalization and negotiated bracketed-paste markers; the browser's
default insertion into its hidden textarea is canceled. Title fields, settings,
and the phone's independent text composer retain native text editing. The
existing encrypted paste-mode checkpoint contract remains documented in
[Architecture and State](docs/wiki/Architecture-and-State.md).

## Page ordering

The server owns the live ordered page registry; its existing ordered
`WorkspaceState.pages` vector is persisted by the daemon and included in server
snapshots. The browser owns only the temporary drag preview and its active page.
No new persistence fields or format version are needed, and no terminal-host
operation is involved. Existing workspace/snapshot readers preserve vector
order. Canvas layers remain keyed by stable page IDs.

The additive client message `reorderPage: [pageId, beforePageId | null]` moves a
page before an existing anchor, or appends it when the anchor is null. The
server requires write permission and validates both IDs under the page registry
write lock before mutating. Missing IDs fail without modifying state;
same-position moves do not publish. Applying a move to the current registry
preserves concurrent creates, renames and unrelated pages instead of replacing
the list with a stale client permutation. Successful changes use the existing
Pages broadcast and workspace persistence pipeline. Deploy the supporting server
before enabling the updated client's ordering control. In local development,
rebuilding the server binary does not replace the running process: restart the
server entry in the existing process manager after server protocol changes. Vite
hot reload updates only the Web client.

`pageOrder.ts` owns the pure ordering and edge-scroll calculations.
`action/pageReorder.ts` owns desktop pointer tracking, layout-based hit testing,
preview/cancel/drop lifecycle and bounded optimistic feedback;
`action/mobilePageReorder.ts` keeps phone hold-versus-scroll handling separate.
`PagePager.svelte` renders the keyed preview, FLIP animation, placeholder and
pointer-transparent floating item. A drop holds its preview until the next
authoritative page list, or at most two seconds; an unacknowledged move is never
automatically retried. New shared page data cancels a current drag so deleted or
renamed targets cannot leave stale local state. The existing error notification
handles rejected moves.

Pending page creation selects only a newly observed page ID, wherever it appears
in the next list. Rename/reorder-only broadcasts cannot complete that intent;
errors and disconnects cancel it. Phone sorting also watches touch starts at
window capture so a second finger outside the pager cancels both a pending hold
and an active drag before other canvas handlers consume the event.

Regression tests cover drag thresholds, click suppression, cancellation,
readonly/disconnected clients, touch scrolling, timer/listener cleanup,
ordering, edge-scroll calculations, shared delivery, server validation,
concurrent page creation/rename and both workspace/snapshot restoration. Browser
display checks can use an isolated pager with synthetic pages, without
connecting to a real session or executing App business flows.

## Canvas wheel and trackpad input

Scroll routing is viewer-local. `wheelInput.ts` classifies device samples;
`action/containWheel.ts` chooses the gesture's starting destination before child
widgets consume wheel events. `TouchZoom` moves the local camera, while focused
components retain their existing scroll paths. File-pane selection and current
gesture evidence live only in browser memory; the Auto / Mouse wheel / Trackpad
preference uses the browser settings store. Existing page-aware file-tree scroll
synchronization is unchanged. User behavior is specified in
[Requirements](REQUIREMENTS.md#canvas-wheel-and-trackpad-input).

The classifier and router share a 350 ms gesture gap, so a slow swipe does not
lose its destination after a brief pause. A pointer press explicitly resets
routing. Auto retains recognized trackpad evidence for up to 1.5 seconds across
gesture boundaries: a large integer pixel delta alone cannot overturn that
recent evidence. Horizontal, fractional or small pixel samples can correct an
initial mouse guess within a gesture; a recognized trackpad gesture never
changes back to mouse zoom mid-stream. Line/page events at the start of a new
gesture, explicit device overrides, clock resets and sufficiently long idle
periods allow fresh classification. Zero-delta samples neither choose a target
nor prolong gesture/device memory.

These are bounded heuristics, not device identification. The first large integer
pixel sample after idle can be ambiguous; an explicit Trackpad or Mouse wheel
setting bypasses guessing. The
[WheelEvent contract](https://w3c.github.io/pointerevents/#events-wheelevents)
allows platform-dependent deltas and transaction timing. Do not infer physical
finger contact or hardware identity from an idle timeout. Regression coverage in
`tests/wheelInput.test.mjs` and `tests/containWheel.test.mjs` exercises sparse
samples, acceleration, recovery from an ambiguous first sample, routing
continuity, explicit clicks/overrides, idle/clock resets, native-scroll
cancellation and existing focused-content scrolling.

`TouchZoom` deduplicates only identical wheel event objects using a `WeakSet`.
Do not deduplicate by timestamp: separate horizontal and vertical events can
share one timestamp, and discarding the second event loses an axis. Both deltas
accumulate before the existing animation-frame notification. Camera regression
tests in `tests/touchZoom.test.mjs` cover either starting axis, equal
timestamps, diagonal movement, reversals and duplicate listener delivery.

## Validation

Run these gates before handing off changes across runtime modules, protocols or
dependencies, and before committing or releasing executable changes:

```shell
mise exec -- npm run lint
mise exec -- npm run check
mise exec -- npm run test:runtime
mise exec -- npm run build
mise exec -- cargo fmt --all -- --check
mise exec -- cargo test --workspace --all-targets
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
```

Server state or dependency changes also require
`mise exec -- cargo test -p sshxx-server --all-targets --features redis-mesh`.
This code check does not authorize starting Redis or changing production
services. Use `npm audit` and `npm outdated` to review the resulting dependency
graph; an npm audit result does not assess Rust dependencies or deployed
environments.

Clipboard regression checks belong in `tests/terminalClipboard.test.mjs`,
covering browser default cancellation, captured text, failure and
unavailable-API behavior. Keep terminal recovery tests in the existing
[coverage map](docs/README.md#recovery-and-cross-client-verification). Report
code checks separately from real browser/OS clipboard verification and App
business-flow E2E tests; the latter require explicit authorization.
