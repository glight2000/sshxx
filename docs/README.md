# Documentation map

This index maps root documentation and remaining legacy manuals. Maintained
topics migrate to the root documents as they are touched; the Wiki keeps links
instead of duplicate instructions for those topics. Deployment and standalone
component material stay with their owners.

## Canonical locations

| Audience                       | Location                                                                                                                                                   | Content                                                                                                  |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| New users                      | [`README.md`](../README.md), [`README.zh-CN.md`](../README.zh-CN.md)                                                                                       | Project purpose, boundaries, quick development/build entry points, and links onward                      |
| Users and maintainers          | [`REQUIREMENTS.md`](../REQUIREMENTS.md)                                                                                                                    | Migrated functional behavior and acceptance criteria; clipboard, page ordering and canvas input          |
| Contributors                   | [`ENGINEERING.md`](../ENGINEERING.md)                                                                                                                      | Toolchains, dependencies, build and validation, clipboard, page ordering and canvas input implementation |
| Users and operators            | [`docs/wiki/`](wiki/Home.md)                                                                                                                               | Installation/releases, features, controls, architecture/state/security, and limitations                  |
| Terminal-host maintainers      | [`crates/sshxx-terminal-host/README.md`](../crates/sshxx-terminal-host/README.md) and [`docs/PROTOCOL.md`](../crates/sshxx-terminal-host/docs/PROTOCOL.md) | Independently operated host lifecycle and its local protocol                                             |
| Contributors and coding agents | [`AGENTS.md`](../AGENTS.md)                                                                                                                                | Repository architecture, modularity, security, validation, and documentation rules                       |

## Maintenance rules

Experimental native client build notes live in
[`clients/electron/README.md`](../clients/electron/README.md) and
[`clients/godot/README.md`](../clients/godot/README.md). They describe
prototypes, not supported replacements for the established Web client.

- Keep the English and Chinese READMEs structurally equivalent. They summarize;
  they do not duplicate the full feature guide or architecture contract.
- Remaining GitHub Wiki pages use descriptive `Title-Case.md` names. Preserve
  working repository and Wiki links when moving affected topics to root docs;
  new canonical topics belong in the corresponding root document.
- Put target-specific runbooks, scripts, units, Compose files, and verification
  steps together under `deploy/<target>/`. Do not copy them into the Wiki.
  Machine-private deployment material must stay outside the repository or in an
  explicitly ignored target directory.
- A separately executable component may own a local README and protocol docs
  when those documents are required to build, operate, or evolve it in
  isolation. Link them from this map.
- Do not create dated status reports, duplicate TODO files, meeting notes, or
  alternate architecture summaries in the repository. Track unfinished product
  work in the README roadmap or an issue; keep normative boundaries in
  `Architecture-and-State.md`.
- Update docs in the same commit as any user-visible behavior, protocol,
  persistence/synchronization rule, security boundary, deployment procedure, or
  known limitation change.

The code-maintenance contract and module-size review thresholds are in
[`AGENTS.md`](../AGENTS.md). Required validation commands are maintained in
[`ENGINEERING.md`](../ENGINEERING.md#validation).

## Recovery and cross-client verification

Extend the existing `npm run test:runtime` suite and component-local tests
rather than adding a parallel runner or dated verification report. Synthetic
terminal streams in `tests/fixtures/terminal-replay.json` define expected
screen, cursor, and mode state for Web and native parser checks. Review a
semantic difference before changing a fixture to match a port; platform-specific
rendering and input checks remain in the relevant client.

| Boundary                              | Existing coverage to extend                                                                                                                                                                                           |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Renderer queue and browser history    | `tests/terminalWriteQueue.test.mjs`, `tests/terminalHistory.test.mjs`: ordered replay/live delivery, missing callbacks, capacity, failure/disposal, late completion, retention rollover and isolation                 |
| Parsed checkpoints and pagination     | `tests/terminalCheckpoint.test.mjs` and `crates/sshx-server/tests/with_client.rs`: pinned headless/Web continuation, small cold state, live fence, historical row anchors, rollover, encryption and request isolation |
| Subscriptions and component lifecycle | Existing terminal subscription, output-flow and initialization tests: page identity, generation/token barriers, bounded retry, teardown and background progress                                                       |
| Host/daemon/server continuity         | Tests alongside Rust stream, runner, output, session and socket implementations: retained ranges, gaps, snapshot/replay ordering, cancellation and per-subscriber ACK deadlines                                       |

Controlled clocks and repeated rollover tests cover state transitions without
waiting days. Bounded references are not a measurement of process RSS or GPU
memory; parser conformance is not browser/device performance or input-method
validation. Report real multi-viewer, background-tab and sustained-load checks
separately, including gaps. Never restart a production terminal host as a test.
