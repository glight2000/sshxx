# Engineering

This document owns development dependencies, build and validation procedures,
and the terminal clipboard implementation. Existing state, protocol and trust
boundaries remain in
[Architecture and State](docs/wiki/Architecture-and-State.md). User-visible
clipboard requirements live in [REQUIREMENTS.md](REQUIREMENTS.md).

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
