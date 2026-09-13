# sshxx maintenance contract

Repository-wide maintenance contract for humans and coding agents. Keep project
invariants here; consult the linked manuals only when their subject is involved.

## Architecture and compatibility

- Preserve the four runtime ownership boundaries documented in
  `docs/wiki/Architecture-and-State.md`: terminal host, daemon, server, and
  client. A feature must name its authority, persistence lifetime, and
  synchronization scope before adding state.
- Keep the suite Release in `release.json` separate from client, server, daemon,
  terminal-host and internal core versions. Bump only components whose code or
  compatibility changed, not unchanged components to match a bundle. Do not bump
  terminal-host for unrelated changes: activating a new host can disconnect all
  hosted processes.
- Keep page identity on every shared canvas mutation. Browser-local view state,
  focus, menus, temporary full-screen state, and undo/redo must not leak into
  synchronized or daemon-persisted state.
- Keep every page's canvas component instances mounted after session hydration.
  Page switching is a browser-local visibility and transition change; it must
  not recreate terminals, notes, file explorers, or custom components.
- Keep phone gestures in dedicated modules, separate from desktop interaction,
  with tap-to-focus separate from explicit fullscreen/restore. Reuse standard
  search/page chrome in canvas view; phone fullscreen hides both and retains
  Restore, while disabling canvas gestures and outer browser zoom. Phone
  full-screen viewing must not mutate shared window geometry, minimized state,
  or PTY size.
- Extend existing protocol messages and versioned persistence formats
  compatibly. Any incompatible format change requires an explicit migration,
  tests for old data, and documentation in the same change.
- Preserve the repository's current Svelte, Rust, protocol, styling, error, and
  test conventions. Do not introduce a parallel state system, UI framework, or
  service boundary without an explicit architecture decision.
- Treat the root Svelte application as the established client behavior and
  protocol baseline. `src-tauri/`, `clients/electron/`, and `clients/godot/` are
  alternative shells or native clients within the same client authority; they
  must not silently change persistence, synchronization, or server trust
  boundaries.
- Keep proprietary or machine-local editor integrations out of Git and export
  artifacts. In particular, Godot MCP Pro is a local development aid, not an
  sshxx source or runtime dependency.
- An explicitly requested contract change is already authorized within its
  stated scope. Before making an additional lifecycle, ownership,
  synchronization, persistence, architecture or interaction change, explain its
  impact and obtain confirmation. A bug fix does not implicitly authorize a
  product redesign.

## Module boundaries and loading

- Fix existing installation, update, and verification scripts in place. Do not
  leave parallel repair scripts or alternate maintenance entry points; discuss
  any necessary new entry point with the user first. Integrate regression checks
  into existing tests and remove task-local repair artifacts after use.
- Svelte components render one coherent surface. Move reusable algorithms,
  validation, transport, persistence, and state machines into typed modules;
  split independently testable visual regions into child components.
- Review responsibilities around 500 lines in Svelte/TypeScript or 800 in Rust.
  These are signals, not splitting requirements or permission for unrelated
  refactoring. Extract only when it improves the requested change coherently.
- Do not create pass-through wrappers merely to reduce line counts. A new module
  must own a recognizable responsibility and expose a smaller typed interface.
- Lazy-load optional heavy surfaces at the user interaction boundary. Every
  dynamic import must have a stable loading state, an actionable failure state,
  and no effect on protocol or persistence semantics.
- Keep compatibility shims narrow and named. xterm private API access belongs
  behind the TypeAhead and version-locked `terminalCheckpoint` boundaries; do
  not spread `_core` access through terminal UI code. Checkpoint adapter changes
  must pass headless-to-Web continuation tests with the exact pinned versions.

## Security, robustness, and performance

- Validate untrusted data at the first authoritative boundary and retain
  explicit size/count limits. Client validation is for usability and never
  replaces server or daemon validation.
- Never log or commit credentials (including URL fragment keys, write passwords,
  SSH/private keys and local encryption keys), private user workspace contents,
  terminal history, runtime caches, private deployment addresses or local-only
  test data. Project source files, public example domains and synthetic fixtures
  are not private runtime data; fixtures must not copy real user/session
  content.
- Use cryptographically secure randomness for identifiers or stream numbers
  involved in authentication/encryption. Preserve authenticated encryption and
  constant-time secret comparisons.
- Bound retained output, editor buffers, uploads, queues, and pending requests;
  cancel timers and reject pending work on disconnect or component teardown.
- Distinguish received/retained output, successful parsing, and
  acknowledgements. Settling cancelled or failed work must not imply successful
  processing. Classify reconnects as resumable, retention-gap, or new-generation
  recovery; never silently append across a known gap or retry input/mutations of
  unknown outcome.
- Contain output recovery to the affected subscription or renderer. Never inject
  input or resize a PTY merely to repaint a viewer. A raw output tail is not a
  complete terminal-state snapshot: do not skip/reorder arbitrary ANSI bytes or
  feed historical bytes into a live parser as pagination. Checkpoint/archive
  changes require an authority, encryption, retention, and compatibility design.
- Prefer pure, tested transformations over duplicated component-local logic.
  Avoid recomputing large registries or importing optional editors/media tools
  on the initial session path.

## Documentation ownership

- Use `docs/README.md` for documentation locations, naming, audience and
  ownership. The two READMEs remain equivalent overviews; `docs/wiki/` is the
  canonical versioned manual mirrored to GitHub Wiki, with
  `Architecture-and-State.md` normative for state and trust boundaries.
  Deployment runbooks stay with their targets, not in the general Wiki.
- Update the existing relevant docs in the same change as behavior, workflow,
  protocol, persistence, synchronization, security, deployment or limitations.
  Pure refactors need only affected maintainer docs. Do not add ad hoc root docs
  or new documentation directories without an owner, audience and map entry.

## Validation

- Use installed `mise` tools and repository lockfiles; retain the global
  tool/dependency approval policy.
- During iteration, choose existing checks for changed modules and their
  callers, proportionate to risk. Prose-only edits need content, reference and
  formatting checks, not unrelated runtime builds.
- Before handing off cross-module runtime, protocol or dependency changes, or
  committing/releasing executable changes, pass the full gates for affected
  stacks below. Reuse results only while their code, dependencies and relevant
  configuration remain unchanged; do not repeat every gate after every edit.
  - Frontend: `npm run lint`, `npm run check`, `npm run test:runtime`,
    `npm run build`.
  - Rust: `cargo fmt --all -- --check`, `cargo test --workspace --all-targets`,
    `cargo clippy --workspace --all-targets -- -D warnings`.
- For server state/dependency changes also verify the opt-in path with
  `cargo test -p sshxx-server --all-targets --features redis-mesh`. The default
  build remains Redis-free; this gate is not authority to deploy Redis or change
  production services. Report blocked checks instead of claiming they passed.
- Extend nearby tests rather than adding a parallel runner. For recovery work,
  use the existing coverage map in `docs/README.md`: synthetic streams,
  controlled clocks, stale generations, retention rollover, cleanup, bounded
  references and unaffected viewers. Share protocol/parser fixtures; keep
  platform tests local.
- App business-flow E2E tests require explicit user authorization; ordinary
  development or verification requests do not grant it. For display-only UI
  changes, inspect visuals without triggering business actions. Report code
  tests, visual checks and business E2E separately; parser/unit checks do not
  prove browser/device performance or sustained-load behavior.
