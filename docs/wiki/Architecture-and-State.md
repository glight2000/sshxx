# Architecture, synchronization, and persistence

## Runtime ownership

| Component      | Owns                                                                                           |
| -------------- | ---------------------------------------------------------------------------------------------- |
| `sshxx-daemon` | Shell/SSH processes, filesystem access, workspace and encrypted SSH-profile files, image cache |
| `sshxx-server` | Session coordination, authorization, page-aware collaboration, encrypted message routing       |
| `sshxx-client` | Browser/Tauri rendering, input, local viewport state, temporary full-screen/focus state        |

Terminal processes do not depend on an open browser. Disconnecting or refreshing
a viewer leaves the daemon and its shells running. Restarting the daemon is a
different boundary: workspace metadata is restored, but shell processes are
recreated.

## State boundaries

| State                                                         | Synchronized |        Persisted by daemon |  Browser-local |
| ------------------------------------------------------------- | -----------: | -------------------------: | -------------: |
| Pages and names                                               |          Yes |                        Yes |             No |
| Terminal/note/file-window layout and appearance               |          Yes |                        Yes |             No |
| Note paragraphs and relationships                             |          Yes |                        Yes |             No |
| File tree selection, expansion, scroll, and open editor state |          Yes |                        Yes |             No |
| Terminal stream and input                                     |          Yes | Rolling server buffer only |             No |
| Active page and per-page pan/zoom                             |           No |                         No |            Yes |
| Focus, open popovers, target-selection mode, full-screen      |           No |                         No | Temporary only |
| Viewer display name and application settings                  |           No |                         No |            Yes |

Every synchronized canvas mutation carries a page identity and is validated
against the target item. Page switching itself remains local.

## Local files

The daemon writes these paths relative to its current working directory:

- `.sshx-workspace` — workspace metadata; not encrypted and may include note
  text and filesystem paths.
- `.sshx-connections` — authenticated encrypted SSH profiles.
- `.sshx-connections.key` — owner-only local encryption key.
- `cache/uploads/` — owner-only terminal image cache.

All are ignored by this repository. Treat the daemon working directory as
sensitive application data and include it in an appropriate private backup
policy, never in source control.

## Security boundaries

- WebSocket filesystem requests require session write permission and must target
  a terminal on the same page.
- Request counts, paths, directory entries, editor buffers, file sizes, upload
  sizes, and snapshot sizes are bounded.
- Filesystem roots cannot be renamed, moved, or recursively deleted, including
  paths that resolve to a root through `..`.
- Workspace and SSH-profile writes use temporary replacement files; unreadable
  or future-format data is quarantined for recovery instead of overwritten.
  Encrypted profile data is authenticated before decoding.
- The server coordinates ciphertext but viewers sharing a session key are still
  trusted collaborators for that session. Use separate sessions and network
  access controls for separate trust domains.
