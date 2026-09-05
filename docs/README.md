# Documentation map

This directory is the single index for versioned sshxx documentation. Product
behavior is documented once in the Wiki sources; deployment and standalone
component material stay with the code they operate.

## Canonical locations

| Audience                       | Location                                                                                                                                                   | Content                                                                                 |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| New users                      | [`README.md`](../README.md), [`README.zh-CN.md`](../README.zh-CN.md)                                                                                       | Project purpose, boundaries, quick development/build entry points, and links onward     |
| Users and operators            | [`docs/wiki/`](wiki/Home.md)                                                                                                                               | Installation/releases, features, controls, architecture/state/security, and limitations |
| Terminal-host maintainers      | [`crates/sshxx-terminal-host/README.md`](../crates/sshxx-terminal-host/README.md) and [`docs/PROTOCOL.md`](../crates/sshxx-terminal-host/docs/PROTOCOL.md) | Independently operated host lifecycle and its local protocol                            |
| Contributors and coding agents | [`AGENTS.md`](../AGENTS.md)                                                                                                                                | Repository architecture, modularity, security, validation, and documentation rules      |

## Maintenance rules

Experimental native client build notes live in
[`clients/electron/README.md`](../clients/electron/README.md) and
[`clients/godot/README.md`](../clients/godot/README.md). They describe
prototypes, not supported replacements for the established Web client.

- Keep the English and Chinese READMEs structurally equivalent. They summarize;
  they do not duplicate the full feature guide or architecture contract.
- GitHub Wiki pages use descriptive `Title-Case.md` names. Add every page to
  `docs/wiki/Home.md` and `docs/wiki/_Sidebar.md`; links inside the repository
  must continue to work before the Wiki is published.
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

The complete code-maintenance contract, including module-size review thresholds
and required validation commands, is in [`AGENTS.md`](../AGENTS.md).
