# Architecture, synchronization, persistence, and security

This page defines the ownership and trust boundaries that new sshxx features
should follow. A state is not “shared” merely because several viewers render it:
every state must have an explicit authority, persistence lifetime, and
synchronization scope.

## Runtime ownership

Release v0.13.3 reports live host versions and Runtime update-job state every 15
seconds (only changes are sent). Probes have bounded timeouts, are cancelled
with their connection, and never resize/terminate terminals. An unavailable host
reports `unknown` rather than indefinitely retaining its old version. The server
sends the latest observation to existing and newly connected viewers. The
Runtime archive is derived from the running daemon executable's version
directory, not the installer's selected-but-not-yet-running pointer. Source
builds are labeled explicitly.

Web-triggered updates are writer-only, opt-in lifecycle actions. The daemon can
start only the locally provisioned `sshxx-update.service` unit in the configured
user/system scope; browsers cannot provide commands, arguments, service names,
paths or release URLs. An independent systemd job owns the actual update, so a
viewer/daemon disconnect does not cancel it. Job result is read back from
systemd after reconnect and is not inferred from WebSocket connectivity. The
administrator owns job code and permissions; root jobs must not execute files or
dependencies writable by the daemon account. Do not grant arbitrary sudo. The
job must preserve the running host; host restart remains separately confirmed
and destructive. A job can update only its explicitly configured deployment, not
an unrelated remote server or packaged desktop app.

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

Host connection readers and writers share one cancellation scope. A failed
writer or failed/panicked output subscription ends that connection and detaches
its subscriptions; other connections and already registered PTYs stay alive.
Partially read protocol frames survive normal subscription completion. A
cancelled connection cannot leave a detached writer task behind. If PTY creation
finishes after its connection was cancelled, the unregistered process is closed
instead of being leaked. These are host-side failure-containment guarantees, not
automatic retries of potentially already-applied terminal input.

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

Write-capable viewers have two confirmed process controls in Settings (client
0.13.1, daemon/server 0.11.2, host 0.10.2 and later):

- **Restart daemon** reloads its executable, retaining the host, PTYs,
  workspace, session URL, encryption identity, and reader/writer capabilities.
  Viewers briefly reconnect. The server validates the existing daemon token
  before replacing that session; its stream state is rebuilt from the host.
- **Restart terminal host** terminates every hosted PTY and reloads the host
  executable on the same authenticated endpoint. Completion requires a new host
  generation, not just a reachable socket. The active host version updates for
  all viewers. Saved SSH-profile windows may relaunch; local terminals, nested
  SSH connections, and application state cannot be recovered automatically.

Neither button downloads updates or restarts server. The executable is validated
before acknowledging a restart; missing/broken replacements are rejected before
tearing down the running runtime. This cannot guarantee recovery from a crash or
an executable changed again after validation. Avoid simultaneous installs and
restart actions. Read-only viewers cannot invoke either action. The result goes
only to the requesting WebSocket; the process change affects every viewer.

For official versioned installations, restart resolves `current-version` again;
for direct executable launches it reloads the same executable path. Unix uses
`exec`, so the PID and service-manager ownership are preserved even though all
daemon/host code is loaded anew. Windows runtime command wrappers supervise an
explicit restart exit code (75), reload the selected executable, and keep the
existing Task Scheduler job alive. Direct Windows executable launches spawn a
successor after releasing runtime handles; the original foreground command
returns. No browser-supplied command/path or OS service-management permission is
introduced.

Daemon-initiated restart temporarily saves `.sshx-restart` alongside the
workspace. It contains a versioned, AES-GCM-authenticated encrypted identity
handoff, reusing the existing `.sshx-connections.key` encryption key. It is
limited to 16 KiB, scoped to the server origin, expires after ten minutes, and
is deleted after the replacement daemon successfully reopens the session. It is
not normal session persistence or a crash-recovery credential store, and is not
synchronized to viewers, written to logs, or inherited by terminal processes.
Unix staging files are owner-only. Invalid/expired handoffs fail explicitly
rather than silently creating another identity; remove `.sshx-restart` to
abandon the handoff and start a new session. Preserve the existing key and
restrict workspace access on all platforms. This handoff and the associated keys
are ignored by Git.

Older Settings controls only reconnected the daemon channel or rebuilt the host
in memory. New clients require the process-restart capability; new daemons
refuse to invoke the old host reset as a binary upgrade. The first upgrade
enabling these controls requires an external service/process restart, after
saving tasks before the host restart. Compatible old hosts can remain running if
this disruptive upgrade is deferred.

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

- Desktop page/search shortcut bindings use the existing browser-local settings
  store (`sshx-settings-store`). They survive refresh and are shared only by
  tabs in the same browser profile and origin, not by other clients through the
  session protocol. The daemon never persists them. Shortcut navigation uses the
  existing local page/search actions; ordinary cursor/focus presence can still
  change, but no shared page, geometry, or PTY-size mutation is emitted. Phone
  mode, modal dialogs, and IME composition do not use these bindings.
- Scroll input routing is viewer-local and follows component focus, never
  pointer hover. Both mouse wheels and trackpads scroll the focused component;
  without focus, wheels zoom and trackpads pan the canvas. The Auto / Mouse
  wheel / Trackpad preference is browser-persisted. The device classifier's
  current gesture and each file window's last-clicked scroll pane are ephemeral
  browser memory; they are not daemon state or synchronized input. Existing
  page-aware file-tree scroll synchronization is unchanged. Menus/dialogs keep
  their native scrolling, and cross-origin iframe input stays inside the frame.
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
  transition, so stateful component instances are not recreated. Hidden
  terminals skip the extra zoom-triggered atlas clear/repaint and cancel pending
  zoom refreshes. Revealing a terminal refreshes only if its last painted zoom
  differs; output parsing and subscriptions continue while hidden.
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
- `MobileWorkspaceControls` owns phone focus/fullscreen controls and the
  existing same-page association picker. Pages and search reuse the standard
  workspace chrome; tapping a component only focuses it. `mobileCanvas` owns
  phone touch gestures, leaving desktop input handlers unchanged. Touch movement
  previews only the world transform and childless grid once per animation frame,
  without changing camera variables inherited by every component. Releasing the
  gesture commits the final local view, avoiding session-wide reactive updates
  for every movement frame. The focus and phone full-screen target live only in
  browser memory. Opening a minimized window fullscreen locally reveals its
  content without changing shared minimized state, geometry, or PTY size.
  Normal-canvas two-finger gestures own zoom and midpoint panning even over
  focused content; native single-finger editing/scrolling remains available
  there. Iframes are pointer-shielded in normal canvas because cross-origin
  events do not bubble to the parent; fullscreen restores iframe interaction.
  Fullscreen disables canvas gestures and outer browser zoom, not single-finger
  scrolling. Deleting or moving the viewed component off the current page exits
  fullscreen. `mobilePage` owns local visible-viewport sizing and follows
  keyboard/orientation changes. Detail pages reuse the existing portal (no new
  route, session, or terminal instance); Restore reveals the unchanged canvas.
  Phone fullscreen hides the top toolbar, bottom pager and component titlebar
  controls, retaining Restore. Navigation remains mounted and reappears on
  restore; only the compact top-left Restore control reserves space above
  fullscreen content, following the visible viewport and safe-area insets.
  `MobileTerminalKeypad` supplies both the floating canvas keys and the reader's
  keys; only the active view renders a keypad, not two concurrent input panels.
  `MobileTerminalReader` owns the phone-only text/input surface. The existing
  xterm painter is hidden, but its parser, write queue, subscriptions and
  renderer ACK processing remain live. `mobileTerminalText` extracts a current
  snapshot through public cell/buffer APIs, joining soft wraps without rewriting
  the PTY. It is not a historical transcript: alternate-screen redraws replace
  the view, and cleared/trimmed data is not retained separately. One active
  detail reader retains at most 128K UTF-16 units, scans at most 2,000 rows /
  128K cells (with a 4,096-column guard), and uses one coalescing 250ms timer
  only while dirty. Cell reads are bounded before concatenation, including
  oversized combining sequences. Public cell attributes add basic ANSI/256/RGB
  styling without a second parser or raw HTML. Theme CSS variables reuse the
  terminal's effective theme/background, including preview updates; no
  independent reader theme is persisted. Attribute ranges store offsets into the
  same text, merging adjacent styles and capping styled ranges at 1,024 (2,049
  total spans). On overflow, older styling is dropped with a notice, not
  accumulated. Concealed cells remain blank even in that fallback. Blink, images
  and exact terminal grid layout are intentionally not rendered in this
  text-reading surface. Selection, reading earlier output, and hidden tabs pause
  projection without accumulating snapshots or blocking output ACKs; resuming
  reads the newest buffer. Unmounting removes subscriptions,
  selection/visibility/pointer listeners, resize observers and pending timers.
  The bounded 16K-unit input draft is memory-only and discarded on exit; send
  mode and special-key panel state are also local and reset on exit. Explicit
  Send uses the existing paste/optional-Enter path or xterm's public `input()`
  for direct keys. Special keys use `input()` without paste markers;
  arrow/Home/End sequences follow public application-cursor mode. No shared
  protocol or synthetic browser keyboard events are added. Read-only, connection
  and replay guards apply to every send. Phone focus does not lock input or
  change shared layout/PTY dimensions. Desktop resizing can still change the
  shared terminal's output, which the reader then reflects. Desktop Escape
  clears local focus/selection; existing terminal focus presence is updated, but
  no terminal input is sent.
- `CanvasRelations` selects the phone-only `MobileCanvasRelations` surface using
  the same phone media query as navigation. Touch tap/hold/drag routing and
  native action dialogs are local; `MobileWorkspaceControls` owns the target
  picker. Add and remove operations use the existing page-scoped association
  mutations, permission checks and daemon persistence. Desktop
  mouse/context-menu behavior is unchanged. Phone focus/navigation never sends
  desktop window-raising mutations; its full-screen frame uses only the local
  visual viewport, not shared canvas bounds.
- Undo/redo stacks for note and file editing belong to the active viewer. The
  edits they produce are shared, but the history stack itself is not.
- Notifications, hover previews, drag state, focus styling, and open popovers
  are presentation state and must remain local unless a feature explicitly
  establishes a collaboration contract.

## Daemon-local files

### Chat history and note attachments (unreleased source)

The server orders room chat and retains its latest 500 records. It includes them
in the existing workspace snapshots written by the daemon to `.sshx-workspace`.
History is session-wide, not page-scoped. Notes store attachment references in
their own metadata; attachment changes carry note ID and page ID and are
independent of paragraph editing and layout mutations. Existing geometry
updates, including updates from older clients, cannot replace an attachment
list. Old protobuf workspaces without these fields load with empty
history/attachments.

Attachment bodies live in private daemon-local `cache/attachments/`, separate
from the expiring terminal-image paste cache. Bodies cross the existing
encrypted file request/response channel in 64 KiB chunks. The attachment route
accepts only random IDs and read/write chunk operations, never arbitrary
filesystem paths or SSH terminal IDs. Viewer reads are marked read-only by the
server and enforced again by the daemon; uploads and note attachment changes
require write permission. Responses go only to the requesting browser. Other
clients receive metadata and load bodies on demand, not through workspace
broadcasts.

Chat text, names, timestamps, and attachment names/types/sizes are workspace
metadata visible to the server, like existing note text. Attachment body files
are plaintext at rest with owner-only permissions, like terminal image uploads;
transport encryption does not encrypt daemon backups or protect against a daemon
machine administrator. Do not present chat as end-to-end encrypted messaging.

Storage is capped at 512 MiB with eight in-progress uploads and 20 MiB per file.
When another upload begins, files unreferenced by the saved chat/notes and older
than one hour are collected. The grace period protects in-flight metadata saves.
Referenced note attachments are not subject to the terminal-image 24-hour
expiry. Abandoned partial uploads become eligible after one hour. Hitting a
limit returns an error without deleting referenced attachments. Browser source
buffers/object URLs are capped at 64 MiB/four open attachments, with
cancellation and URL cleanup on close or teardown; browser image/video decoding
has additional memory costs.

Chat/sidebar visibility, pending drafts, preview selection, playback, download,
and scroll position remain local and temporary. Sending text preserves the prior
chat permission rule (authenticated readers can chat). A daemon/server upgrade
is required for persistence and attachments; terminal-host needs no changes for
this feature. No recording or audio sending is provided.

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

Daemon-to-server output has a separate byte-sequence recovery boundary. Both
hosted and embedded runners retain a bounded UTF-8 replay tail (pruned from over
12 MiB to approximately 8 MiB); queued sends are not acknowledgements. Three
Sync reports without acknowledged progress trigger replay, including while the
PTY continues producing output. Each chunk identifies the earliest encrypted
byte offset still retained. If the server's missing bytes precede that offset,
only a chunk beginning at this declared boundary may discard the unrecoverable
gap. The server keeps the original absolute encryption offsets, clears that
terminal's old replay cache, and advances its display generation so viewers
rebuild only that renderer and subscribe again. Other terminals, the PTY, nested
SSH connections, and terminal host are untouched. Viewers are notified that
earlier output was lost; retained bytes are not a complete terminal-emulator
screen snapshot, so exact TUI screen restoration is not guaranteed. Recovery
never injects input to force a redraw.

This requires an updated daemon and server, but no terminal-host update. The
server advertises `output_recovery` in Sync; protobuf defaults preserve older
peers. An updated daemon connected to an older server reports an unrecoverable
gap and suppresses futile full-tail retransmissions rather than silently
looping; upgrading only the browser cannot repair this condition. Older daemons
still receive normal sequence acknowledgements but cannot authorize gap
truncation. Diagnostics log terminal IDs, expected/retained byte positions, and
rejected chunk counts, never terminal contents or credentials.

Browser output delivery uses capability-negotiated renderer backpressure.
Compatible viewers receive at most 256 KiB per terminal batch; the server does
not send that terminal's next batch until xterm's public write callback confirms
that the current batch was parsed. The viewer further writes in 64 KiB chunks
with a timer-based event-loop yield between chunks, independent of animation
frames that pause in hidden browser tabs. Inactive-page terminals keep parsing
and acknowledging output through their mounted xterm instances. If a writer is
not yet available, bounded browser history owns the batch for later replay.
Evicting a whole history chunk immediately clears its text reference, without
waiting for array compaction; garbage collection timing remains browser-owned.
Older viewers retain the legacy subscription behavior, while batch size and
current-page labeling remain safe on the server side.

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

Terminal initialization also has a 15-second deadline. Import/font completion
checks that the instance is still alive and its parent is connected before
opening xterm; hidden pages remain mounted. A failed initialization shows a
local Retry action instead of repeatedly remounting; this retry replays bounded
browser history rather than discarding output received before initialization.
Teardown releases observers, queued writes, and timers. A pending write lasting
two seconds shows “Catching up”, even if the batch is too small to reach the
queue-size threshold. Browser timers cannot run while the browser/OS completely
suspends the page; they resume when scheduling resumes, while the server has its
independent deadline.

Text clipboard events inside xterm are consumed once through public `paste()`.
xterm owns newline normalization and negotiated bracketed-paste markers; the
browser's default insertion into its hidden textarea is canceled. Title fields,
settings, and the phone's independent text composer retain native text editing.

#### Output diagnostics

Diagnostics are local, transient metadata, not synchronized workspace state or
terminal text. Enable the affected process's Rust module at `debug` using
`RUST_LOG` during investigation (for example `sshx_daemon::runner=debug`,
`sshx_server::session=debug,sshx_server::web::socket=debug`, or
`sshxx_terminal_host=debug`). Do not restart a running host merely to enable
logging; new host instrumentation only becomes available after a separately
planned host upgrade, which can interrupt its processes.

| Layer               | Recorded metadata                                                                                                                                                                                                                                                   |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Host                | Per-subscription 10-second progress: numeric terminal suffix, process ID, raw retention start/end, last PTY output Unix timestamp, last queued forwarding position. Sampling continues when that subscriber is backpressured.                                       |
| Daemon              | On Sync: terminal ID, raw host read position (hosted terminals), decoded/encrypted retention range, last queued send position, requested Sync position, last read/send age.                                                                                         |
| Server              | On Sync: terminal ID, expected and last received sequence, retention start, rejected-gap count, last accepted-data age. Gap/rebase warnings remain available without debug logging.                                                                                 |
| Viewer subscription | WebSocket viewer ID, terminal ID/display generation and subscription protocol/token; last batch end, last ACK batch, wait duration/status. Pending waits report every 10 seconds and are canceled with the viewer connection.                                       |
| Browser renderer    | `data-terminal-diagnostics` on its `.term-container`: terminal ID/generation, pending characters/chunks, pending-since and last-completed Unix timestamps, failed state. Console initialization/failure/rebuild and delayed/recovered events contain metadata only. |

IDs are scoped to a workspace/connection; a renderer generation is not a host
process generation. Host raw PTY byte offsets and daemon/server UTF-8 encrypted
stream offsets are different counters: compare continuity within each link, not
numerical equality across those links. “Sent” means queued locally, not accepted
by the next layer. A lack of PTY output can simply mean an idle shell; compare
output and acknowledgement progress before declaring a stall. Debug logging is
opt-in and metadata is bounded by live terminals/subscriptions; use the service
manager's existing log retention policy, not an unbounded trace file.

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
