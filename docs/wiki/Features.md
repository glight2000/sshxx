# Feature guide

This page is the complete product-level feature reference for sshxx. For exact
shortcuts, see [Keyboard and mouse controls](Keyboard-and-Mouse). For ownership,
persistence, synchronization, encryption, and trust boundaries, see
[Architecture and State](Architecture-and-State).

## Workspace overview

sshxx combines persistent terminals, structured notes, and filesystem windows on
one zoomable canvas. Each window can move, resize from all edges and corners,
snap to the background grid, link to related work, and temporarily fill the
space between the top and bottom bars.

![A complete sshxx release workspace with a terminal, structured note, file editor, pages, and three online collaborators](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-workspace.png)

The toolbar creates terminals and notes, searches the workspace, opens settings,
reports the connection state, and shows up to six online collaborators as
initial avatars. Additional users are available from the overflow menu. The
bottom pager creates and switches independent canvas pages. Double-click a page
name to rename it inline.

## Workspace chat and attachments

Release v0.13.3 adds an **Emoji / 颜文字** picker to chat and notes. Choose a
symbol to insert it at the saved text cursor/selection; it remains ordinary
Unicode text, so existing copy, synchronization, persistence and note undo work
unchanged. No sticker downloads, tracking requests or new media format are used.

Chat opens as a right sidebar on desktop and a full-screen surface on phones.
The room shares its latest 500 messages across all pages and viewers. Names and
timestamps are retained; reconnecting, refreshing, or restarting the daemon can
restore history from the daemon workspace. Closing the chat surface does not
delete sent messages. Unsent drafts are temporary and are discarded on close.

Use **Attach files**, paste an image, or drop files onto the chat or note
content. Images and MP4/WebM videos can be opened with **Preview**; other files
are offered as downloads and are never executed. Media is loaded only when
requested, and videos do not autoplay. This version does not include audio
sending or recording. Notes retain attachments independently from paragraph
editing and window layout. Moving a note or editing its title does not replace
its attachment list.

Limits are 20 MiB per file, 8 attachments per chat message, 32 per note, and 512
MiB in the daemon attachment store. Browser attachment previews/downloads share
a 64 MiB source-data budget and at most four open resources; close a preview to
free space. Browser decoder memory is additional to this source-data budget.
Chat text is limited to 2,000 input characters and 8 KiB on the server.

Authenticated readers can read room history and attachments and send text,
following existing chat permissions. Uploading files or changing note
attachments requires write access. See
[Architecture and State](Architecture-and-State) for encryption and retention
boundaries. Upgrade both server and daemon for durable chat and attachments; a
client-only upgrade is insufficient.

## Persistent terminals

- Terminal processes belong to `sshxx-terminal-host`; closing or refreshing a
  browser, or restarting `sshxx-daemon`, does not stop them.
- In single-server mode, losing the server's in-memory session briefly
  disconnects viewers. The daemon recreates it from the durable workspace and
  reattaches existing hosted terminals; a fixed session name preserves the URL.
- Local shell history uses a stable per-terminal history file/namespace.
  Duplicating a local terminal copies its last persisted history into a new,
  independent history file. A nested remote shell still follows that remote
  account's history policy.
- Local shells and OpenSSH terminals share xterm.js rendering, scrollback,
  selection, WebGL acceleration with DOM fallback, and isolated TypeAhead local
  echo.
- Each terminal has its own title, opacity, color theme, and optional background
  override. New windows begin at 80% opacity and use the viewer's default theme.
- The title-bar actions close, temporarily full-screen, duplicate, and open a
  filesystem window at the current directory. Duplication reuses a saved SSH
  profile (including OpenSSH-configured jump hosts) and, when OSC 7 shows the
  terminal is still on its initially reported SSH host, starts in its current
  remote directory. Local terminals use the terminal host's process working
  directory. A manually entered nested `ssh A` → `ssh B` chain cannot be
  reconstructed safely from the daemon, so a duplicate falls back to the saved
  connection's default directory instead of applying B's path to A.
- A focused terminal remains at the top without discarding its configured
  transparency. Focus highlighting and terminal-attention animation are
  independent visual states; attention animates the title bar until focused.
- Selected terminal text changes `Ctrl+C` into copy and clears the selection.
  Without a selection, the same shortcut is delivered to the process.
- `Shift+Enter` sends LF for multiline-capable foreground applications. The
  application, rather than sshxx, ultimately decides whether LF submits input.

### Reusable SSH connections

The main half of the split terminal button creates a default local terminal. Its
arrow opens saved SSH profiles and profile management.

![SSH profile editor with authentication and terminal appearance defaults](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-ssh-profile.png)

Profiles support OpenSSH config/default behavior, SSH Agent, private-key files,
and an interactive password prompt. A profile can preselect the new terminal's
color theme and optionally override its background. Profile names are unique;
existing entries can be opened, edited, or deleted from the menu.

Passwords and private-key contents are never stored. Profile metadata is stored
by the daemon in an authenticated-encrypted, versioned file with an owner-only
local key. Unsupported or damaged versions are quarantined rather than blocking
daemon startup.

## Pages, canvas navigation, and search

Right-click a page tab and choose **Delete page** to remove it after
confirmation. This closes its terminals and removes its components for every
viewer; unsaved edits are lost, but files on disk are not deleted. File browsers
attached to a closed terminal also close. The last page cannot be deleted.
Viewers currently on the removed page switch to a remaining page.

Every page owns an independent set of terminals, notes, file windows, and custom
components. Window geometry, content, and relationships are page-aware and
persisted by the daemon. A viewer's active page and pan/zoom for each page stay
browser-local, so one collaborator can navigate without moving everyone else.

![All-page terminal and note search with two canvas pages and online collaborators](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-search-pages.png)

Global search indexes terminals, notes, file windows, and custom components
across every page. The list supports filtering, pointer selection, arrow-key
navigation, and Enter. Choosing a result switches only the current viewer to the
target page and centers the selected window. Search queries and page switching
are not synchronized.

Desktop shortcuts open pages by pager order (`Ctrl+1…9`, `Ctrl+0` for page 10),
move to adjacent pages (`Ctrl+Left/Right`), jump to first/last (`Ctrl+Up/Down`),
and open/focus search (`Ctrl+Space`). **Settings → Keyboard shortcuts** supports
rebinding, disabling, and restoring defaults for this browser only. See
[Keyboard and mouse controls](Keyboard-and-Mouse#desktop-workspace-shortcuts)
for input priority and browser/system shortcut limitations.

Canvas navigation uses left-drag on empty space for a local selection marquee,
right-drag on empty space for pan, unconditional middle-button pan, and a faster
`Ctrl` + wheel zoom that suppresses browser zoom. Marquee selection is distinct
from component focus: membership updates continuously with the marquee, and
focusing a window, clicking empty canvas, or pressing Escape clears the
selection. Dragging any selected window moves the selected group with one common
offset, while right-button pan leaves the selection unchanged.

Mouse wheels follow **component focus**, even over blank canvas or another
window; without focus they zoom the canvas. Two-finger trackpad gestures scroll
content only when they start inside the focused window; otherwise they pan the
canvas. The starting window remains the destination for that gesture. File
explorers retain both scroll axes and use the actively edited or last-clicked
pane. Scroll input does not change focus or chain into browser-page scrolling.
Ctrl + wheel (including browser-reported trackpad pinch) remains forced canvas
zoom. Pan and zoom are disabled while a component is full-screen; clicking its
visible outside margin exits full-screen.

Settings → **Scroll input device** offers Auto, Mouse wheel, and Trackpad. The
[standard WheelEvent API](https://developer.mozilla.org/en-US/docs/Web/API/WheelEvent)
does not identify the physical input device. Auto therefore estimates from delta
units, size, and horizontal movement, keeping one classification throughout a
gesture. Precision wheels and some touchpad drivers can be misidentified; choose
an explicit mode on those devices. This preference stays in browser-local
storage and never changes another viewer's behavior. Menus and dialogs retain
their own scrolling. Events originating inside a cross-origin custom iframe do
not reach the parent client and remain controlled by the embedded page; the
client cannot remotely scroll that page from outside its iframe.

A browser-local setting exchanges the left and right blank-canvas drag roles for
users who prefer left-button panning. It does not alter component controls,
middle-button panning, or the stationary right-click action menu. A right-button
selection drag suppresses that menu for the completed gesture.

On phones, the standard top search and bottom page tabs provide navigation.
Tapping a component only focuses it without moving the canvas. A floating
**Fullscreen / Restore** button explicitly switches the focused component's
view. Fullscreen hides the top toolbar, bottom page tabs and component titlebar
controls, giving more space to the content while keeping **Restore** accessible.
Terminal pages show an adaptive, selectable text view above a multiline input
box. Hold text to copy; Enter adds an input line and **Send ↵** submits the text
followed by Enter to the foreground program. Selection temporarily freezes the
text display, not output reception; **Latest** resumes following updates. The
page tracks the visible viewport when the phone keyboard opens. In the overview,
two-finger zoom and midpoint pan work even over focused windows. Fullscreen
disables canvas gestures and outer browser zoom, while preserving single-finger
content scrolling. Embedded websites are touch-shielded on the canvas and
interactive in fullscreen. These are separate touch controls, not changes to
desktop mouse behavior. The same terminal parser and component instances remain
mounted. No lock, shared geometry change or PTY resize is introduced. On
desktop, Shift+Escape clears focus as well as selection. Plain Escape remains
available to the terminal program or active editor/menu.

The phone composer also offers **Paste only** (no extra Enter) and **Direct
keys** (plain terminal input, without paste markers or Enter). Direct keys
rejects draft line breaks; use Paste only for multiline text, whose line breaks
are still interpreted by the running application. **Keys** expands a bounded,
scrollable keypad for arrows, Enter, Tab, Esc, Backspace/Delete, Home/End,
PageUp/PageDown and common Ctrl combinations. Each tap sends immediately,
without submitting or clearing the draft. Ctrl+C interrupts the foreground
program rather than copying text. Arrow/Home/End sequences follow the terminal's
application cursor mode. Send mode and keypad visibility are local to this
detail view and reset on exit; all sends honor write access and connection /
replay availability.

The text view reads the existing buffer, not a separately archived chat log. It
wraps text locally and follows the terminal's chosen theme and background
override, including its ANSI palette, selection colors and basic text styles.
Indexed 256-color and truecolor output are supported. Images and fixed-column
application layouts are not reproduced. Cleared/overwritten content cannot be
reconstructed, and alternate-screen applications replace their current screen
rather than accumulate fake history. Extraction is bounded to 2,000 rows, 128K
UTF-16 units and 128K scanned cells, at most once per 250ms while dirty.
Truncation is indicated. Adjacent styled characters are merged, with at most
1,024 colored/styled ranges (2,049 total text spans). Excess older styling falls
back to the terminal's default text color, with a notice; the text itself
retains the same limits. Selection, reading earlier output, and background tabs
pause projection only; returning to Latest refreshes from the current buffer.
Input is limited to 16K UTF-16 units. Leaving the detail page releases its
snapshot, draft, subscriptions and timers; none is persisted or sent to other
viewers. Only explicitly submitted input affects the shared PTY.

Tap an association icon to locate and focus the target on the canvas. Hold it to
see the full name and explicit **Open component** / **Remove association**
actions; dragging the strip scrolls it without navigating. A note's **+** opens
a target list, following the existing same-page rules and excluding existing
links. Dismiss the list to cancel. Association changes remain shared and
persisted; menus and navigation remain local. Real-device selection and keyboard
behavior still need broader validation across phone browsers.

Dragging a single window or selected group over a non-active page in the bottom
pager highlights only the page currently under the pointer. The moving windows
shrink and fade toward that target; leaving it reverses the preview. Releasing
performs one validated cross-page mutation, switches the current viewer to that
page, and preserves the local selection. The moved windows retain their
coordinates and relative layout from the start of the drag; linked notes,
terminals, and file windows keep their explicit relationships even when they now
span pages.

The session replaces the browser's native context menu. Right-clicking empty
canvas space opens the same default-terminal, saved-SSH, note, custom-component,
search, and settings actions as the toolbar; windows created there use the
clicked canvas position. Existing component-specific context menus remain in
place. The menu and its anchor position are viewer-local and never synchronized
or persisted.

With snapping enabled, moving and eight-direction resizing use the same grid
anchors and one-tenth-grid visual inset. New terminal, note, file, and custom
windows are created with matching aligned geometry.

Source-derived windows use bounded nearby placement. Duplicated terminals, file
browsers opened from a terminal, and terminals opened from any file-browser
action first try the closest free positions around their source. A crowded
region falls back to a small cascaded overlap instead of placing the new window
far across the canvas; these actions do not recenter the viewer.

All canvas window types use the same title and surface-color workflow.
Double-click a title to edit it in place; Enter or an outside click saves, while
Escape cancels. A title-bar click focuses the component, while an actual drag
moves it without changing focus. Titles are no longer duplicated in appearance
menus. The yellow title-bar control minimizes or restores a window for every
viewer. A minimized window keeps its width and saved expanded height, displays
only its title bar at exactly one background-grid unit high, and cannot be
resized or opened full-screen until restored. This state is synchronized and
daemon-persisted; temporary full-screen remains viewer-local. Each background
picker provides 24 low-luminance neutral and chromatic presets whose contrast
against the primary text color is at least 10:1, plus a custom color input. A
terminal's first swatch restores its theme background, so it does not need a
separate enable/disable control. Saved titles and backgrounds are shared with
the session and persisted in the daemon workspace; title-edit focus itself
remains viewer-local.

## Custom HTML, JavaScript, and URL components

The toolbar and blank-canvas context menu can create a custom component with the
same title, full-screen, move, resize, snapping, background, selection, and
cross-page behavior as other canvas windows. Its content view switches between
HTML/JavaScript and URL modes. HTML uses the existing CodeMirror syntax editor
and an on-demand Format action; URL mode accepts an absolute HTTP(S) page. The
adjacent title-bar control switches between content and preview; every switch to
preview rebuilds the iframe, and the refresh action rebuilds it again without
leaving preview. A component can be reduced to a two-column by three-row
background-grid footprint; the usual one-tenth-grid edge insets remain part of
that geometry.

Custom source or URL, content type, content/preview mode, title, background,
page, and geometry are synchronized and persisted in the daemon workspace. A
mode change is reflected by every viewer; each viewer entering preview rebuilds
its own iframe at least once. The editor cursor and running iframe remain
browser-local. Pointer movement from an HTML/JavaScript preview is bridged into
the normal page-aware live cursor. A click is deliberately never replayed in
another browser; other viewers on that page instead see the transient message
`自定义组件不同步点击事件` at the corresponding component position. A remote URL
remains cross-origin and cannot be instrumented by sshxx; it participates only
if that page explicitly emits the documented `sshxx-custom-pointer-v1`
`postMessage` shape itself. Preview uses a sandboxed, opaque-origin iframe:
scripts, forms, media, downloads, and CORS-permitted API calls are available,
while access to the sshxx parent document, same-origin storage, top-level
navigation, nested frames, objects, referrer data, and browser device
permissions is not granted. URL mode rejects the viewer's own origin before
loading; the server's anti-frame headers and client embedding guard provide a
second recursion boundary. A target site can still refuse embedding with
`X-Frame-Options` or `frame-ancestors`, and sites requiring same-origin cookies
or storage may not work in the opaque sandbox.

An HTTPS workspace cannot embed ordinary HTTP pages even if they open directly
in a browser tab. The component displays a mixed-content explanation instead of
a blank preview; serve the target over HTTPS or open it separately. This is a
viewer-local preview restriction, not a change to the saved/shared URL.

An HTML/JavaScript preview may explicitly navigate the shared component from a
user action:

```html
<button onclick="window.sshxx.setUrl('https://example.test/next')">
  Open next view
</button>
```

The parent validates an absolute HTTP(S) URL, rejects recursive sshxx origins,
switches the component to URL preview, and sends the normal shared component
update. Every viewer then loads the new URL, and the daemon persists it with the
workspace. A cross-origin URL page cannot be inspected automatically; a page
that intentionally integrates with sshxx may instead send
`{ type: "sshxx-custom-set-url-v1", url: "https://…" }` to its parent. Normal
iframe navigation without one of these explicit requests remains browser-local.

This is intentionally client-side rendering. Every viewer that opens preview
runs the component independently, and switching back to preview runs it again.
The source view displays this warning permanently. Code that causes external
side effects must therefore be idempotent or perform its own coordination, and
shared component source and URLs must never contain credentials, session
fragment keys, bearer tokens, or other secrets.

## Structured notes and connected workflows

Notes are visually distinct neutral-gray canvas windows. Focus uses a light-gray
border, while active editing uses its own state so “selected note” and “editing
a paragraph” never become ambiguous. A custom inline title and selected note
background follow the same shared persistence rules as the note contents.

![Structured note with all paragraph delivery actions and linked canvas targets](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-notes.png)

- A click enters paragraph editing without selecting placeholder text; Escape or
  an outside click ends editing.
- Notes use plain text with browser spellchecking disabled. The shared UI font,
  comfortable line spacing and subtle paragraph boundaries support longer
  reading without changing the stored text or paragraph structure.
- Enter inserts a line break inside the current paragraph. `Ctrl`/`Cmd` + Enter
  adds a separate paragraph.
- Four-dot handles keep paragraph boundaries visible and expose only
  paragraph-local delete, copy, and insertion actions. `Ctrl`/`Cmd`-click
  toggles paragraphs and Shift-click selects a range. A pointer drag across
  paragraph bodies paints a contiguous block selection; it never becomes a
  cross-paragraph browser text selection.
- Hovering a paragraph reveals a send icon on its right. Its separate menu can
  target every linked item, linked notes, linked terminals, linked terminals
  followed by execution input, or linked writable file editors.
- Dragging any selected handle moves the selected group as a stable block inside
  its note. Dropping elsewhere preserves separate paragraphs in another note;
  terminals and file editors receive a multiline plain-text projection.
- Copy and paste use a versioned structured clipboard payload between notes,
  with a plain-text clipboard representation for other applications and canvas
  targets.
- Undo/redo is available while the local note editor is active. The resulting
  text changes synchronize, but the undo stack itself remains local.

Paragraph selection and clipboard fallback state remain local to the viewer;
only the resulting paragraph changes synchronize. The plus button in a note's
relationship strip starts target selection. A note can link to terminals, other
notes, and file editors on the same page. Selecting anything incompatible,
pressing Escape, or using another canvas action cancels selection. Relationship
icons navigate to a target; right-click removes the link. Focusing a terminal
highlights its related notes with the terminal focus color at 50% opacity;
focusing a note applies the reciprocal treatment to related terminals. The
stronger pulse does not change either window's saved appearance.

Note text uses character-level collaborative updates. Paragraphs, relationships,
and edits always carry their page and object identity to prevent cross-page
application.

## Filesystem browser and editor

The filesystem browser is a first-class synchronized canvas window rather than a
modal. It moves, resizes, snaps, links, persists, and supports viewer-local
full-screen in the same way as terminals and notes. Its inline title and surface
background are synchronized workspace appearance state; editor syntax colors
remain optimized independently for readable source display.

![Full-width sshxx filesystem browser with folder tree and CodeMirror editor](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-file-explorer.png)

### Navigation and presentation

- The editable path bar resolves an absolute directory in real time. Escape or
  an outside click restores the last selected valid path.
- The left tree shows folders only. Clicking a row selects it; only its arrow
  expands or collapses it. Standard tree keyboard navigation is supported.
- Entering a folder from the right-side grid expands its ancestry and highlights
  the same folder in the left tree. This navigation state is shared with other
  viewers of the file window.
- A draggable divider controls tree width.
- The right side shows the selected folder as a large-icon grid. Double-click
  enters a folder or opens a file. Empty folders show a centered `Empty` state.
- Common file types receive recognizable icons. Browser-native previews cover
  common image, audio, video, and PDF formats; unsupported codecs show an
  explicit message instead of an empty preview.
- UTF-8 and BOM-marked UTF-16 text files up to 8 MiB open in CodeMirror with
  filename-based language support, local undo/redo, dirty state, explicit save,
  and shared cursor/editor state. UTF-16 files retain their original byte order
  when saved.
- A deployment invalidating a lazy-loaded editor chunk triggers one guarded
  application refresh. HTML is revalidated, hashed assets are immutable, and a
  missing hashed asset returns 404 instead of the SPA fallback; the editor error
  state also offers explicit retry and reload actions.

### File operations

Selection-aware title-bar actions and custom right-click menus provide upload,
download, create file/folder, rename, move, delete, refresh, save, and “open
terminal here”. Upload accepts mixed multi-selection of files and folders. Files
up to 8 MiB can be downloaded from either action surface. Downloads use the
existing encrypted filesystem request channel, remain local to the viewer, and
preserve UTF-8, BOM-marked UTF-16, and arbitrary binary bytes. Names and
destinations are validated, and destructive actions confirm where appropriate.
Selecting a file opens a terminal in its containing folder.

“Open terminal here” retains the source terminal's SSH connection and opens the
selected remote directory. The terminal-to-profile association survives daemon
and server restarts; only the profile ID is stored in the ordinary workspace.
The requested directory is applied at process launch and reasserted after local
Bash, Fish, or PowerShell initialization; SSH Bash sessions receive the same
one-time correction. This prevents common startup scripts that change directory
from silently overriding the action.

Local filesystem operations run with the daemon OS account. SSH browsing uses
the OpenSSH SFTP subsystem and therefore requires a config, Agent, or key-based
profile; an interactive password cannot be safely reused for the separate
filesystem channel. A write-capable session has the target account's actual
permissions—sshxx is not a filesystem sandbox.

File-window geometry, path, selection, tree expansion/scroll, divider position,
open editor, content changes, dirty state, and operations synchronize to other
viewers and persist with the workspace. Hover, context menus, local undo
history, and temporary full-screen do not.

## Collaboration and presence

- Successful participants appear as colored initial avatars; hover reveals the
  full display name. The color differentiates users rather than representing a
  connection state.
- Cursor, focus, terminal activity, and note editing ownership are transient
  session presence. These signals are page-aware and are not daemon-persisted.
- Note editing ownership prevents simultaneous local editors from presenting an
  ambiguous state. Character updates remain synchronized to every viewer.
- Page switching, canvas viewport, marquee/group selection, hover, menus, and
  local full-screen never force another viewer to follow. A completed group move
  still publishes page-aware layout data. An explicit pager drop is the
  exception for page ownership: it atomically moves the selected shared items,
  while only the initiating viewer follows to the destination page.
- Shared mutations are writer-authorized and validate page/object relationships
  before application.

## Appearance and viewer preferences

![Viewer settings for grid snapping, light/dark/system appearance, identity, default terminal palette, and scrollback](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-settings.png)

Viewer settings include optional grid snapping, light/dark/system application
appearance, display name, the default palette for new terminals, and terminal
scrollback. Application appearance deliberately does not recolor existing notes
or terminals. These preferences stay in the current browser profile and are not
daemon-persisted or synchronized.

The toolbar Wi-Fi icon alone represents connection state. Its color and
animation distinguish connecting, connected, reconnecting, and failed states.
Connection failures surface an explanatory message instead of leaving the viewer
at an indefinite `Connecting…` state.

## Image paste and drop

Pasting or dropping PNG, JPEG, WebP, or GIF data into a local terminal encrypts
the upload in the viewer, stores it under the daemon working directory's
`cache/uploads/`, and inserts the resulting absolute path at the terminal input
position. The maximum image size is 20 MiB. Completed files are owner-only;
files older than 24 hours are removed at daemon startup, and abandoned active
uploads are reclaimed after a short timeout.

Remote SSH terminals reject this workflow because the remote host cannot read a
daemon-local path. Supporting them requires a separate SFTP/SCP forwarding
design.

## Feature and state summary

| Capability                                                        | Durable at daemon      | Shared with session                  | Viewer-local only                |
| ----------------------------------------------------------------- | ---------------------- | ------------------------------------ | -------------------------------- |
| Pages and canvas window content/layout, including minimized state | Yes                    | Yes                                  | Active page, viewport, selection |
| Terminal process                                                  | Until host/OS restart  | Stream/input                         | Focus and selection              |
| Note paragraphs and relationships                                 | Yes                    | Character-level updates              | Undo stack and active editor     |
| File tree/editor navigation and content                           | Yes                    | Yes                                  | Menus, hover, undo, full-screen  |
| SSH profiles                                                      | Encrypted file         | Metadata; writer mutation            | Password prompt input            |
| User preferences                                                  | No                     | No                                   | Browser `localStorage`           |
| Online users and editing ownership                                | No                     | Transient                            | Hover/overflow menu              |
| Uploaded images                                                   | Temporary daemon cache | Encrypted transfer and inserted path | Drag/paste UI state              |
