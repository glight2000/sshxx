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

Godot, Electron and Pocket experiments remain outside this maintenance scope.
Target-specific deployment addresses, credentials and runbooks stay private.

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
before enabling the updated client's ordering control.

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
