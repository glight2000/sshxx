# Architecture, synchronization, persistence, and security

This page defines the ownership and trust boundaries that new sshxx features
should follow. A state is not “shared” merely because several viewers render it:
every state must have an explicit authority, persistence lifetime, and
synchronization scope.

## Runtime ownership

| Component      | Owns                                                                                                                                                             |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sshxx-daemon` | Shell/SSH processes, filesystem operations, durable workspace data, encrypted SSH-profile files, and the image cache                                             |
| `sshxx-server` | Authentication and write authorization, live session coordination, page-aware collaboration, encrypted-payload routing, and optional short-lived Redis snapshots |
| `sshxx-client` | Browser/Tauri rendering and input, viewer preferences, local viewport state, and temporary UI state                                                              |

Terminal processes do not depend on an open browser. Disconnecting or refreshing
a viewer leaves the daemon and its shells running. Restarting the daemon is a
different boundary: workspace metadata is restored, but shell processes are
recreated rather than resumed.

## Persistence and synchronization matrix

| Data or behavior                                                                                                                  | Authority and persistence                                                                     | Synchronization scope                                                                                                                                                                |
| --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Pages; terminal, note, and file-window layout/appearance; note paragraphs and relationships; file-browser navigation/editor state | Durable daemon workspace in `.sshx-workspace`                                                 | Shared with authenticated viewers in the same session. Every canvas mutation carries its page identity.                                                                              |
| Terminal/SSH processes                                                                                                            | Daemon memory                                                                                 | Output and permitted input are shared within the session. Processes survive viewer disconnects, but not daemon restarts.                                                             |
| Reusable SSH profiles                                                                                                             | Authenticated encryption in `.sshx-connections`, using the owner-only `.sshx-connections.key` | Profile metadata is visible to authenticated viewers in the session; only viewers with write access may change it. Passwords and private-key contents are never stored in a profile. |
| Actual files and directories                                                                                                      | Target filesystem, using the daemon OS account or SSH account                                 | File operations take effect on the target host. Shared file-window state is updated so other viewers can refresh/navigate consistently.                                              |
| Active file-editor buffer                                                                                                         | Encrypted bytes in the shared workspace and server snapshot; saved content is the target file | Buffer, open path, dirty state, and editing changes are shared within the session and page.                                                                                          |
| Active page and per-page pan/zoom                                                                                                 | Browser `localStorage`, scoped by server and session                                          | Never synchronized. One viewer switching or moving a page does not move another viewer.                                                                                              |
| Display name, application color mode, default terminal theme, scrollback, and grid snapping                                       | Browser `localStorage`                                                                        | Never synchronized and never persisted by the daemon.                                                                                                                                |
| Focus, open menus/dialogs, link-target selection, and temporary full-screen state                                                 | Browser memory only                                                                           | Never synchronized or persisted. Full-screen state survives page switching in the current app instance, but not a refresh.                                                           |
| Online users, cursors, terminal focus, and note editing ownership                                                                 | Server memory                                                                                 | Transient real-time collaboration state. It is not daemon-persisted; cursor and focus events remain page-aware.                                                                      |
| Pasted terminal images                                                                                                            | Plain completed files under daemon-local `cache/uploads/`, with owner-only permissions        | The encrypted upload traverses the server; the resulting local path is inserted into the target terminal. Files older than 24 hours are removed on daemon startup.                   |
| Server session snapshot                                                                                                           | Server memory and, when configured, compressed Redis data                                     | Continuity/failover aid only. Redis is refreshed at most every 20 seconds (or on requested sync) and expires after 5 minutes; it is not the durable workspace authority or a backup. |

The daemon's `.sshx-workspace` is the durable authority for shared canvas
metadata. Redis is optional and short-lived. Without Redis, server state exists
only in memory. Terminal output has a bounded rolling server buffer; a Redis
snapshot retains at most 32 KiB per terminal.

### Deliberately local behavior

- Page switching is local, while every shared page mutation includes a page ID.
- Global search runs locally over the shared all-page snapshot. The query is not
  synchronized; choosing a result changes only that viewer's page and viewport.
- Undo/redo stacks for note and file editing belong to the active viewer. The
  edits they produce are shared, but the history stack itself is not.
- Notifications, hover previews, drag state, focus styling, and open popovers
  are presentation state and must remain local unless a feature explicitly
  establishes a collaboration contract.

## Daemon-local files

The daemon writes these paths relative to its current working directory:

- `.sshx-workspace` — versioned workspace data. It is **not encrypted** and may
  contain note text, titles, host/path metadata, layout, and encrypted active
  editor bytes.
- `.sshx-connections` — versioned, authenticated-encrypted reusable SSH
  profiles.
- `.sshx-connections.key` — owner-only local key for the profile file.
- `cache/uploads/` — owner-only, temporary, completed image files.

These paths are ignored by this repository. Treat the entire daemon working
directory as sensitive application data, keep it out of source control, and
apply an appropriate private backup and file-permission policy. Unreadable or
future-format workspace/profile files are quarantined with an `.invalid-*`
suffix rather than silently overwritten.

## Communication and data visibility

| Link                           | Protocol and protection                                                                                                    | Boundary                                                                                                                                                                                                 |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Viewer ↔ server                | HTTP/WebSocket; use HTTPS/WSS outside localhost or a trusted isolated LAN                                                  | The URL fragment carries the session key and optional write credential. Fragments are not sent in HTTP requests, but remain bearer secrets visible to the browser, history, screenshots, and extensions. |
| Viewer ↔ daemon through server | Session-key encryption for terminal streams, filesystem request/response payloads, image chunks, and active editor content | The server routes these payloads as ciphertext. The stream format uses AES-CTR and does not replace TLS transport authentication/integrity.                                                              |
| Daemon ↔ server                | gRPC over the configured HTTP/HTTPS endpoint                                                                               | Use TLS across untrusted networks. `SSHXX_SECRET` signs server session tokens; it does not encrypt all application metadata.                                                                             |
| Server ↔ Redis                 | Redis protocol using the configured URL                                                                                    | Redis contains compressed session snapshots and coordination keys. Keep it private; use authentication and TLS when it crosses a trusted host/network boundary.                                          |
| Daemon ↔ SSH host              | System OpenSSH and SFTP                                                                                                    | OpenSSH host-key, agent, key-file, and authentication policies apply. Filesystem access has the SSH account's privileges.                                                                                |

The server cannot decrypt terminal contents, filesystem payloads, pasted image
chunks, or active editor contents without the URL-fragment session key. It can
see collaboration metadata required for coordination, including page/layout
state, note text, display names/cursors, titles, filesystem paths, and SSH
profile metadata such as host, user, authentication mode, and key path. Redis
snapshots inherit this split: encrypted payloads remain encrypted, while
coordination metadata is readable to the server/Redis trust domain.

## Authorization and trust model

- Read-only and write-capable URLs are separate bearer capabilities. Mutations,
  terminal input, uploads, and filesystem requests require server-side write
  authorization. Filesystem requests must also target an existing terminal on
  the stated page.
- A write-capable participant is a trusted collaborator. Terminal and file
  operations run with the daemon OS account or remote SSH account permissions;
  sshxx does not provide a filesystem sandbox. Share write URLs only with users
  who may exercise those privileges.
- All participants holding the session key can decrypt the session's encrypted
  content. Use separate sessions plus network/access controls for separate trust
  domains.
- Paths, request counts, pending operations, directory entries, editor/file
  sizes, image uploads, canvas entities, and snapshots are bounded. Filesystem
  roots cannot be renamed, moved, or recursively deleted, including paths that
  resolve to a root through `..`.
- HTTPS/WSS remains required on untrusted networks. Session encryption protects
  selected content from the routing server; it does not authenticate the web
  origin, hide server-visible metadata, or make an exposed write URL safe.

## Rules for future features

Before adding a stateful feature, document and implement all four decisions:

1. Which component is authoritative: daemon, server, client, or target host?
2. Is it durable, short-lived, or browser-memory-only, and how is old/corrupt
   data handled?
3. Is it synchronized, to which session/page/users, and which events carry the
   page/object identity?
4. Is its content encrypted in transit/at rest, who can read it, and which
   authorization check protects each mutation?

Prefer existing page-aware messages and versioned workspace fields. Do not turn
viewer-local presentation state into shared state without an explicit product
requirement.
