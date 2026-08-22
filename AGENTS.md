# sshxx maintenance contract

This file is the repository-level instruction source for human and AI-assisted
maintenance. It applies to the entire repository. Keep it concise and update it
when an architectural boundary changes; do not duplicate product documentation
here.

## Architecture and compatibility

- Preserve the four runtime ownership boundaries documented in
  `docs/wiki/Architecture-and-State.md`: terminal host, daemon, server, and
  client. A feature must name its authority, persistence lifetime, and
  synchronization scope before adding state.
- Keep page identity on every shared canvas mutation. Browser-local view state,
  focus, menus, temporary full-screen state, and undo/redo must not leak into
  synchronized or daemon-persisted state.
- Extend existing protocol messages and versioned persistence formats
  compatibly. Any incompatible format change requires an explicit migration,
  tests for old data, and documentation in the same change.
- Preserve the repository's current Svelte, Rust, protocol, styling, error, and
  test conventions. Do not introduce a parallel state system, UI framework, or
  service boundary without an explicit architecture decision.

## Module boundaries and loading

- Svelte components render one coherent surface. Move reusable algorithms,
  validation, transport, persistence, and state machines into typed modules;
  split independently testable visual regions into child components.
- Treat 500 lines as a review threshold for a Svelte/TypeScript module and 800
  lines for a Rust module. These are design signals, not mechanical limits. A
  change to an existing oversized file should normally extract a coherent
  responsibility or at least avoid increasing its scope.
- Do not create pass-through wrappers merely to reduce line counts. A new module
  must own a recognizable responsibility and expose a smaller typed interface.
- Lazy-load optional heavy surfaces at the user interaction boundary. Every
  dynamic import must have a stable loading state, an actionable failure state,
  and no effect on protocol or persistence semantics.
- Keep compatibility shims narrow and named. In particular, xterm private API
  access belongs behind the existing TypeAhead compatibility boundary; do not
  spread `_core` access through terminal UI code.

## Security, robustness, and performance

- Validate untrusted data at the first authoritative boundary and retain
  explicit size/count limits. Client validation is for usability and never
  replaces server or daemon validation.
- Never log or commit URL fragment keys, write passwords, SSH secrets, private
  keys, encrypted local data keys, workspace files, terminal history, caches,
  deployment secrets, hostnames, or local-only test data.
- Use cryptographically secure randomness for identifiers or stream numbers
  involved in authentication/encryption. Preserve authenticated encryption and
  constant-time secret comparisons.
- Bound retained output, editor buffers, uploads, queues, and pending requests;
  cancel timers and reject pending work on disconnect or component teardown.
- Prefer pure, tested transformations over duplicated component-local logic.
  Avoid recomputing large registries or importing optional editors/media tools
  on the initial session path.

## Documentation ownership

- `README.md` and `README.zh-CN.md` are mirrored project overviews. Keep their
  structure and claims equivalent; do not turn them into full manuals.
- `docs/wiki/` is the canonical, versioned user and architecture manual mirrored
  to GitHub Wiki. `Home.md` and `_Sidebar.md` must link every user-facing page.
- `docs/wiki/Architecture-and-State.md` is normative for ownership, persistence,
  synchronization, communication, and security boundaries.
- Target-specific operational material belongs under `deploy/<target>/` and must
  not be mixed into the general Wiki. Component-local documentation is allowed
  only for a separately operated component or protocol, such as the terminal
  host.
- `docs/README.md` is the documentation map and naming policy. Do not add an ad
  hoc root Markdown file or a new documentation directory without adding it to
  that map and identifying its owner and audience.
- Update documentation in the same change when behavior, user workflow,
  protocol, persistence, synchronization, security, deployment, or a known
  limitation changes. Pure refactors should update only maintainer-facing
  structure documentation.

## Validation

- Use the installed `mise` toolchain and repository lockfiles. Do not add a
  runtime, version manager, package manager, global tool, or project dependency
  without first exhausting existing facilities and discussing a material new
  dependency.
- Frontend changes must pass `npm run lint`, `npm run check`,
  `npm run test:runtime`, and `npm run build`.
- Rust changes must pass `cargo fmt --all -- --check`,
  `cargo test --workspace --all-targets`, and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- Changes touching server state or dependencies must also compile and test the
  opt-in Redis path with
  `cargo test -p sshxx-server --all-targets --features redis-mesh`; the default
  build must remain Redis-free.
- Put tests beside the closest existing test style and cover compatibility,
  invalid input, cleanup, and synchronization boundaries relevant to the change.
