# Architecture, synchronization, persistence, and security

This page defines the ownership and trust boundaries that new sshxx features
should follow. A state is not “shared” merely because several viewers render it:
every state must have an explicit authority, persistence lifetime, and
synchronization scope.

## Runtime ownership

| Component             | Owns                                                                                                                                                             |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sshxx-terminal-host` | PTY/ConPTY handles, local shell/OpenSSH client processes, and a bounded in-memory terminal-output replay buffer                                                  |
| `sshxx-daemon`        | Host bridging and shell policy, filesystem operations, durable workspace data, encrypted SSH-profile files, per-terminal history, and the image cache            |
| `sshxx-server`        | Authentication and write authorization, live session coordination, page-aware collaboration, encrypted-payload routing, and optional short-lived Redis snapshots |
| `sshxx-client`        | Browser/Tauri rendering and input, viewer preferences, local viewport state, and temporary UI state                                                              |

The suite Release and the runtime components have independent versions. The
version in `release.json` names a downloadable bundle and its GitHub tag;
`sshxx-client`, `sshxx-server`, `sshxx-daemon`, `sshxx-terminal-host`, and the
internal core crate each declare their own implementation version. A Release may
therefore contain different component versions. Only a component whose code or
compatibility contract changed is bumped. Protocol negotiation, not version
string equality, determines whether separately versioned components can
communicate.

This distinction is especially important for terminal-host. Routine client,
server, daemon, Web, packaging, and documentation changes leave its version
unchanged, so installing a new Runtime bundle does not falsely imply that the
process-owning host needs a disruptive restart. The Settings version list
reports the terminal-host version returned by its authenticated local handshake,
independently of the daemon version.

Terminal processes do not depend on an open browser or a continuously running
daemon. The daemon reconnects to stable terminal IDs after restart, replays the
host's bounded output buffer, and continues the same PTY. Stopping, crashing, or
upgrading the terminal host itself closes that boundary and disconnects its
processes. The daemon recreates only windows backed by a saved SSH profile and
reruns that profile's launch configuration; default local windows close. This
does not restore nested connections or application state. Host upgrades are
therefore explicit and never coupled to routine daemon upgrades.

The replay buffer stores raw PTY bytes, not a serialized terminal-emulator
screen. Rebuilding a renderer after older bytes have rolled out can therefore
preserve the process while losing an exact full-screen presentation. This is
most visible during repeated frontend hot reloads or after very high-volume
output; generic input must never be injected automatically to force a redraw.

Workspace restoration may reattach to an existing stable host terminal ID.
Source-derived creation requests—including file-browser “Open terminal
here”—must create a fresh PTY. If an orphaned host entry collides with that new
ID, the daemon replaces the orphan so the request's working directory and SSH
profile cannot be silently discarded.

## Terminal-host lifecycle and upgrades

Outside a service manager, `sshxx-daemon` starts its sibling
`sshxx-terminal-host` executable when no host process is reachable. Under
systemd, the host must run in a separate unit; the daemon intentionally refuses
to spawn it inside the daemon unit's cgroup. If the executable itself is
missing, startup fails with an actionable error instead of falling back to
in-process PTY ownership.

Managed Runtime installation preserves the same three-process boundary on every
platform: separate systemd units on Linux, separate launchd jobs on macOS, and
separate current-user Task Scheduler jobs on Windows. Windows uses login tasks
instead of `LocalSystem` services so interactive shells and filesystem access
retain the installing user's security context. `sshxx-service stop`, `restart`,
and `update` affect server and daemon but never restart terminal-host.

Daemon and host releases negotiate a versioned local protocol. Installing or
restarting a compatible daemon never restarts the currently running host. A new
host executable may be installed on disk while the old process continues to own
its PTYs; activation is deferred until no hosted terminal remains:

```shell
sshxx-daemon terminal-host status
sshxx-daemon terminal-host restart
```

The non-forced restart is rejected while any terminal is active. This protects
local shells, nested SSH connections, full-screen applications, and agents such
as Codex or Claude Code. `restart --force` is an explicit destructive
acknowledgement and disconnects all of them.

Write-capable viewers also have two explicit controls in Settings. **Restart
daemon** recreates the daemon-to-server control channel while leaving the
terminal host and its PTYs untouched. **Restart terminal host** always requires
a browser confirmation, terminates every hosted PTY, and rebuilds the host
runtime on the same authenticated local endpoint. The latter is a recovery
control, not a zero-downtime host-binary upgrade: saved SSH-profile windows can
relaunch, while default local terminals and nested application state are lost.
Read-only viewers cannot invoke either action. The server returns a lifecycle
result only to the requesting WebSocket.

For a systemd system deployment, first run the status command as the service
account. Only when it reports no terminals should an operator restart the
independent host unit and then the daemon unit:

```shell
sudo -u app sshxx-daemon terminal-host status
sudo systemctl restart sshxx-terminal-host.service
sudo systemctl restart sshxx-daemon.service
```

There is currently no live PTY/ConPTY handle transfer between host processes.
Consequently, an active-host upgrade is either deferred or destructive. A future
zero-disruption design would need old and new host generations to run in
parallel, route existing terminals to the old generation, and drain it after
those terminals exit.

## Persistence and synchronization matrix

| Data or behavior                                                                                                                                                                                                                                              | Authority and persistence                                                                                             | Synchronization scope                                                                                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pages; terminal, note, file-window, and custom-component layout/appearance, including minimized state and retained expanded dimensions; note relationships; file-browser state; custom HTML/JavaScript or URL content, content type, and content/preview mode | Durable daemon workspace in `.sshx-workspace`                                                                         | Shared with authenticated viewers in the same session. Custom source, URL, and modes are intentionally visible to the server and every authenticated viewer; they must not contain secrets or bearer tokens. Remote terminals retain only their non-secret SSH profile ID here. |
| Terminal/SSH processes                                                                                                                                                                                                                                        | Terminal-host memory                                                                                                  | Output and permitted input are shared within the session. Processes survive viewer and daemon disconnects. Host/OS loss ends them; saved SSH-profile windows relaunch a new SSH process, while default local windows close.                                                     |
| Per-terminal local shell history                                                                                                                                                                                                                              | `cache/terminal-host/history/<stable-terminal-id>.history` for HISTFILE/PSReadLine plus a per-terminal Fish namespace | Never shared between local terminal windows. Duplication copies a snapshot into a new independent history. Nested SSH shells and programs with their own history require remote/application configuration.                                                                      |
| Reusable SSH profiles                                                                                                                                                                                                                                         | Authenticated encryption in `.sshx-connections`, using the owner-only `.sshx-connections.key`                         | Profile metadata is visible to authenticated viewers in the session; only viewers with write access may change it. Passwords and private-key contents are never stored in a profile.                                                                                            |
| Actual files and directories                                                                                                                                                                                                                                  | Target filesystem, using the daemon OS account or SSH account                                                         | File operations take effect on the target host. Shared file-window state is updated so other viewers can refresh/navigate consistently.                                                                                                                                         |
| Active file-editor buffer                                                                                                                                                                                                                                     | Encrypted bytes in the shared workspace and server snapshot; saved content is the target file                         | Buffer, open path, dirty state, and editing changes are shared within the session and page.                                                                                                                                                                                     |
| Active page and per-page pan/zoom                                                                                                                                                                                                                             | Browser `localStorage`, scoped by server and session                                                                  | Never synchronized. One viewer switching or moving a page does not move another viewer.                                                                                                                                                                                         |
| Display name, application color mode, default terminal theme, scrollback, grid snapping, and canvas mouse-button mapping                                                                                                                                      | Browser `localStorage`                                                                                                | Never synchronized and never persisted by the daemon.                                                                                                                                                                                                                           |
| Focus, canvas group selection, open menus/dialogs, link-target selection, and temporary full-screen state                                                                                                                                                     | Browser memory only                                                                                                   | Never synchronized or persisted. Group selection is page-local; only the resulting page-aware window positions are shared. Full-screen survives page switching in the app instance, but not a refresh.                                                                          |
| Custom-component editor cursor, iframe state, and render generation                                                                                                                                                                                           | Browser memory only                                                                                                   | Never synchronized or persisted. The shared mode determines whether preview is visible, but each viewer independently creates and executes its iframe when entering preview or explicitly rendering again.                                                                      |
| Online users, cursors, terminal focus, note editing ownership, and custom-component click notices                                                                                                                                                             | Server memory                                                                                                         | Transient real-time collaboration state. It is not daemon-persisted; cursor, focus, and click-notice events remain page-aware. Custom-component clicks are announced but never replayed in another iframe.                                                                      |
| Pasted terminal images                                                                                                                                                                                                                                        | Plain completed files under daemon-local `cache/uploads/`, with owner-only permissions                                | The encrypted upload traverses the server; the resulting local path is inserted into the target terminal. Files older than 24 hours are removed on daemon startup.                                                                                                              |
| Server session snapshot                                                                                                                                                                                                                                       | Server memory and, when configured, compressed Redis data                                                             | Continuity/failover aid only. Redis is refreshed at most every 20 seconds (or on requested sync) and expires after 5 minutes; it is not the durable workspace authority or a backup.                                                                                            |

The daemon's `.sshx-workspace` is the durable authority for shared canvas
metadata. Redis is optional and short-lived. Without Redis, server state exists
only in memory. Terminal output has a bounded rolling server buffer; a Redis
snapshot retains at most 32 KiB per terminal.

Normal development and single-server deployments deliberately leave Redis
disabled. The default server build excludes Redis dependencies. The repository's
Compose service belongs to the `multi-server` profile and starts only with
`docker compose --profile multi-server up -d`; each participating test server
must be built with `--features redis-mesh` and explicitly receive `--redis-url`.

If a single server loses its in-memory session, the daemon recognizes the
missing-session response, opens a replacement using its current durable
workspace, preserves the session encryption and write capabilities, and
acknowledges already-running terminal-host processes instead of creating
duplicates. Viewers reconnect to the same URL when the server uses a fixed
session name; a random-name deployment necessarily receives a new URL.

### Deliberately local behavior

- Page deletion is a shared, write-authorized workspace operation. Right-click a
  page and choose **Delete page**, then confirm. Its terminals are terminated,
  its notes/custom components/file windows are removed, and dangling note links
  are cleared. As with closing a terminal, file windows tied to that terminal
  also close even on another page. Unsaved edits are lost; files on disk are not
  deleted. At least one page must remain. Viewers on the deleted page fall back
  to a remaining page; viewers elsewhere keep their current page. Page creation
  and cross-page moves are serialized against deletion at the page registry.
- Application light/dark/system mode controls the shared UI chrome, menus,
  editors, and default backgrounds of notes, file explorers, and custom
  components. It is a viewer preference, not a workspace mutation. Explicit
  component backgrounds remain shared; their text palette follows background
  contrast. "Use theme background" restores the viewer-dependent appearance.
  Legacy default colors (`#3f3f46`, `#111113`, `#18181b`, respectively) are
  interpreted as theme defaults without rewriting saved workspaces. An old
  explicit choice identical to its component's legacy default cannot be
  distinguished from that default. Terminal ANSI palettes and embedded
  third-party pages remain independent; settings menus always follow the
  application mode. Canvas status colors follow application mode, not the custom
  content palette: dark mode uses indigo for focus, neutral gray for note focus,
  and gold for selection; light mode uses saturated blue, cyan, and orange,
  respectively. Linked components use the source component's focus hue. Light
  mode reserves a permanent 2px border on all four component types so changing
  state never shifts their content. Both modes share a solid-color border and a
  glow that fades outward from a solid-color core. Focus uses a steady glow;
  linked components and selection share one breathing animation with different
  durations (1.8s and 1.15s). Breathing changes the glow's extent and strength,
  not window opacity or geometry. Reduced-motion mode keeps the steady glow.
  States recolor the existing border (the custom component uses its permanent
  iframe-safe border layer), never add a second status outline. Selection takes
  precedence over focus/linked highlights. Default-opacity light-mode border
  contrast is covered by `tests/canvasStateColors.test.mjs`.
- Page switching is local, while every shared page mutation includes a page ID.
  After session hydration, every page's terminal, note, file-explorer, and
  custom-component instances remain mounted in that browser. Switching pages
  changes only page-layer visibility, interaction, and the local fade
  transition, so stateful component instances are not recreated.
- Marquee/group selection is local and mutually exclusive with component focus.
  Its membership follows the marquee continuously. Focusing a component,
  clicking empty canvas, or pressing Escape clears the selection. Right-button
  canvas pan does not mutate it; a completed group move sends the normal
  page-aware position update for every affected component.
- Dropping a moving selection on the pager sends one bounded, writer-authorized
  cross-page operation containing IDs and final coordinates. The server
  validates every source item and the destination before mutating any
  collection. The initiating viewer locally centers and, when necessary, zooms
  the destination page to reveal the whole moved group; this viewport change is
  not synchronized. Explicit note/file/terminal relationships may span pages
  after such a move; ordinary input, editing, filesystem messages, and terminal
  output batches use the component's current page ID.
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
  contain note text, custom HTML/JavaScript source, titles, host/path metadata,
  layout, and encrypted active editor bytes.
- `.sshx-connections` — versioned, authenticated-encrypted reusable SSH
  profiles.
- `.sshx-connections.key` — owner-only local key for the profile file.
- `cache/uploads/` — owner-only, temporary, completed image files.
- `cache/terminal-host/host.token` — owner-only local IPC authentication token;
  never sent to the server or viewer.
- `cache/terminal-host/instance.id` — stable non-secret namespace used to
  reconnect workspace terminal IDs after daemon restart.
- `cache/terminal-host/history/` — per-terminal history files for shells
  honoring `HISTFILE` and PowerShell PSReadLine. Fish uses a stable independent
  history namespace in its normal user-data location.
- `cache/terminal-host/host.sock` on Unix — owner-only local IPC socket. On
  Windows the equivalent endpoint is a state-directory-specific named pipe.

These paths are ignored by this repository. Treat the entire daemon working
directory as sensitive application data, keep it out of source control, and
apply an appropriate private backup and file-permission policy. Unreadable or
future-format workspace/profile files are quarantined with an `.invalid-*`
suffix rather than silently overwritten.

## Communication and data visibility

Custom component preview is an explicit untrusted-code boundary inside each
viewer. Source or a remote URL runs in an opaque-origin sandboxed iframe without
`allow-same-origin`, parent/top navigation, nested frames, object embedding,
referrer transmission, or device permissions. Scripts and CORS-permitted network
calls remain available by design. This limits access to sshxx state but does not
make external side effects atomic: every viewer opening preview runs a separate
copy. HTML/JavaScript previews also emit bounded pointer coordinates to their
parent. The server validates the component, page, and coordinates before
broadcasting a transient click notice. This metadata is neither persisted nor
used to execute an event remotely. Cross-origin URL content is opaque unless it
voluntarily emits the same versioned pointer message. Shared URL navigation
follows the same explicit bridge boundary. User-authored HTML calls
`window.sshxx.setUrl`, while a cooperating URL page may emit
`sshxx-custom-set-url-v1`. The parent requires write access and validates the
URL before updating the existing shared custom-window state; implicit iframe
navigation is never inferred or persisted. The Web server sends
`frame-ancestors 'none'` and `X-Frame-Options: DENY`; the client also refuses to
mount when it detects an embedding frame. Together these rules prevent custom
code from recursively rendering an authenticated sshxx workspace in production,
development, and the packaged client. URL mode also rejects the current viewer
origin before navigation. Server validation bounds each source to 256 KiB, each
URL to 4 KiB, the source session total to 4 MiB, and the component count to 100.

| Link                           | Protocol and protection                                                                                                         | Boundary                                                                                                                                                                                                                                                                                |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Viewer ↔ server                | HTTP/WebSocket; use HTTPS/WSS outside localhost or a trusted isolated LAN                                                       | The URL fragment carries the session key and optional write credential. Fragments are not sent in HTTP requests, but remain bearer secrets visible to the browser, history, screenshots, and extensions. Authorized lifecycle action names and request IDs are server-visible metadata. |
| Viewer ↔ daemon through server | Session-key encryption for terminal streams, filesystem request/response payloads, image chunks, and active editor content      | The server routes these payloads as ciphertext. The stream format uses AES-CTR and does not replace TLS transport authentication/integrity.                                                                                                                                             |
| Daemon ↔ server                | gRPC over the configured HTTP/HTTPS endpoint                                                                                    | Use TLS across untrusted networks. `SSHXX_SECRET` signs server session tokens; it does not encrypt all application metadata.                                                                                                                                                            |
| Server ↔ Redis                 | Redis protocol using the configured URL                                                                                         | Redis contains compressed session snapshots and coordination keys. Keep it private; use authentication and TLS when it crosses a trusted host/network boundary.                                                                                                                         |
| Daemon ↔ SSH host              | System OpenSSH and SFTP                                                                                                         | OpenSSH host-key, agent, key-file, and authentication policies apply. Filesystem access has the SSH account's privileges.                                                                                                                                                               |
| Daemon ↔ terminal host         | Versioned length-prefixed protobuf over an owner-only Unix socket or Windows named pipe, authenticated by a 256-bit local token | Local-only bridge for raw PTY bytes and control operations. Dropping the connection never closes a terminal; an explicit close operation does.                                                                                                                                          |

Terminal output delivery uses capability-negotiated renderer backpressure.
Compatible viewers receive at most 256 KiB per terminal batch; the server does
not send that terminal's next batch until xterm's public write callback confirms
that the current batch was rendered. The viewer further writes in 64 KiB chunks
with a timer-based event-loop yield between chunks, independent of animation
frames that pause in hidden browser tabs. Inactive-page terminals acknowledge
after their bounded browser history accepts the data. Older viewers retain the
legacy subscription behavior, while batch size and current-page labeling remain
safe on the server side.

The browser bounds each xterm write-callback wait to 15 seconds. A timeout
quarantines that renderer, releases replay/input suppression, clears only that
viewer's bounded terminal scrollback, and remounts the xterm instance; it never
restarts or closes the PTY, SSH connection, or terminal-host process. The server
independently bounds a renderer-acknowledgement wait to 75 seconds. With the
`terminal-recovery-v1` capability, it removes only that viewer's stalled
terminal subscription and sends `terminalStalled`. The viewer waits for its
bounded write queue to settle and subscribes again from the chunk checkpoint
accepted into its browser history, using the terminal's current page and a fresh
subscription token. Batch acknowledgements include the PTY generation, token,
and exact next chunk index; old or duplicate acknowledgements cannot release a
newer batch. Other terminals, file requests, viewers, and the underlying
processes are not restarted. Terminal output follows the stable ID/generation
across page moves; page identity still scopes shared layout mutations. Old
clients that cannot negotiate recovery use connection-level reconnect after an
ACK timeout; new clients fall back to the older subscription protocol when
connected to an older server. While retained output is replaying, keyboard,
paste, and component-driven input are blocked with a visible retry message
rather than being silently discarded.

### Filesystem save safety

File saves and uploads stage complete contents in a randomly named sibling
`.sshxx-save-*` file, then publish it without truncating the destination first.
Local staging uses the existing `tempfile` dependency and is removed on ordinary
failure or cancellation. Preparing a local save in a blocking worker cannot
publish a file after its requesting future has been cancelled. Symbolic links
are followed to their targets; local files with multiple hard links are rejected
because replacing one name would silently leave the other names unchanged.

Local Linux saves preserve owner/group, mode, and exposed extended attributes
(including POSIX ACLs); macOS uses native metadata copying; Windows uses native
[`ReplaceFileW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)
attribute/security merging without ignore-error flags. Its failure path retains
recovery files and reports their locations, because a failed native replacement
can already have renamed the original. Successful replacement removes the
backup. An attribute copy failure aborts before publication. New local files are
published exclusively and default to private permissions. Atomic replacement
requires permission to write the containing directory as well as access to the
original file. Detected concurrent changes abort the save, but this is not a
filesystem transaction or a cross-process compare-and-swap guarantee.

Remote saves preserve the owner/group and permissions exposed by SFTP and
require OpenSSH atomic-replacement support for existing files, or exclusive
hard-link publication for new files. Unsupported servers fail explicitly instead
of falling back to truncation. SFTP v3 does not expose all ACLs, extended
attributes, or hard-link counts: remote preservation of those properties is
**not guaranteed**. Use native tools for remote files that depend on them.
Extended-attribute merging on macOS and Windows still needs native-platform
acceptance testing.

Normal remote failures attempt staging-file cleanup. A daemon/SSH crash can
leave a `.sshxx-save-*` sibling behind; no automatic sweep deletes such files. A
network failure during the final rename can also lose the success response after
the save committed: reload and verify before retrying. Atomic replacement
prevents partially written destination contents; it does not promise universal
power-loss durability on every remote filesystem. Changes to file contents
retain the existing daemon authority and file-editor collaboration scope.

The server cannot decrypt terminal contents, filesystem payloads, pasted image
chunks, or active editor contents without the URL-fragment session key. It can
see collaboration metadata required for coordination, including page/layout
state, note text, display names/cursors, titles, filesystem paths, and SSH
profile metadata such as host, user, authentication mode, and key path. Redis
snapshots inherit this split: encrypted payloads remain encrypted, while
coordination metadata is readable to the server/Redis trust domain.

## Authorization and trust model

- Read-only and write-capable URLs are separate bearer capabilities. Mutations,
  terminal input, uploads, filesystem requests, and runtime lifecycle actions
  require server-side write authorization. Filesystem requests must also target
  an existing terminal on the stated page. Terminal-host restart remains an
  explicitly confirmed destructive operation.
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
