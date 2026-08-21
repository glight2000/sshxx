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
bottom pager creates, renames, and switches independent canvas pages.

## Persistent terminals

- Terminal processes belong to `sshxx-terminal-host`; closing or refreshing a
  browser, or restarting `sshxx-daemon`, does not stop them.
- Local shell history uses a stable per-terminal history file/namespace. A
  nested remote shell still follows that remote account's history policy.
- Local shells and OpenSSH terminals share xterm.js rendering, scrollback,
  selection, WebGL acceleration with DOM fallback, and isolated TypeAhead local
  echo.
- Each terminal has its own title, opacity, color theme, and optional background
  override. New windows begin at 80% opacity and use the viewer's default theme.
- The title-bar actions close, temporarily full-screen, duplicate from the same
  working directory/environment, and open a filesystem window at the current
  directory.
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

Every page owns an independent set of terminals, notes, and file windows. Window
geometry, content, and relationships are page-aware and persisted by the daemon.
A viewer's active page and pan/zoom for each page stay browser-local, so one
collaborator can navigate without moving everyone else.

![All-page terminal and note search with two canvas pages and online collaborators](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-search-pages.png)

Global search indexes terminals and notes across every page. The list supports
filtering, pointer selection, arrow-key navigation, and Enter. Choosing a result
switches only the current viewer to the target page and centers the selected
window. Search queries and page switching are not synchronized.

Canvas navigation supports empty-space drag, unconditional middle-button pan,
and `Ctrl` + wheel zoom that suppresses browser zoom. Plain wheel input is
routed to the hovered terminal, note, menu, file tree, directory grid, or
editor. Pan and zoom are disabled while a component is full-screen; clicking its
visible outside margin exits full-screen.

With snapping enabled, moving and eight-direction resizing use the same grid
anchors and one-tenth-grid visual inset. New terminal, note, and file windows
are created with matching aligned geometry.

## Structured notes and connected workflows

Notes are visually distinct neutral-gray canvas windows. Focus uses a light-gray
border, while active editing uses its own state so “selected note” and “editing
a paragraph” never become ambiguous.

![Structured note with all paragraph delivery actions and linked canvas targets](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-notes.png)

- A click enters paragraph editing without selecting placeholder text; Escape or
  an outside click ends editing.
- Enter inserts a line break inside the current paragraph. `Ctrl`/`Cmd` + Enter
  adds a separate paragraph.
- Four-dot handles keep paragraph boundaries visible and expose delete, copy,
  insert, and delivery actions. `Ctrl`/`Cmd`-click toggles paragraphs and
  Shift-click selects a range. A pointer drag across paragraph bodies paints a
  contiguous block selection; it never becomes a cross-paragraph browser text
  selection.
- Delivery can target every linked item, linked notes, linked terminals, linked
  terminals followed by execution input, or linked writable file editors.
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
full-screen in the same way as terminals and notes.

![Full-width sshxx filesystem browser with folder tree and CodeMirror editor](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-file-explorer.png)

### Navigation and presentation

- The editable path bar resolves an absolute directory in real time. Escape or
  an outside click restores the last selected valid path.
- The left tree shows folders only. Clicking a row selects it; only its arrow
  expands or collapses it. Standard tree keyboard navigation is supported.
- A draggable divider controls tree width.
- The right side shows the selected folder as a large-icon grid. Double-click
  enters a folder or opens a file. Empty folders show a centered `Empty` state.
- Common file types receive recognizable icons. Supported non-text files are
  previewed when possible.
- Text files up to 8 MiB open in CodeMirror with filename-based language
  support, local undo/redo, dirty state, explicit save, and shared cursor/editor
  state.

### File operations

Selection-aware title-bar actions and custom right-click menus provide upload,
create file/folder, rename, move, delete, refresh, save, and “open terminal
here”. Upload accepts mixed multi-selection of files and folders. Names and
destinations are validated, and destructive actions confirm where appropriate.
Selecting a file opens a terminal in its containing folder.

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
- Page switching, canvas viewport, hover, menus, and local full-screen never
  force another viewer to follow.
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

| Capability                              | Durable at daemon      | Shared with session                  | Viewer-local only               |
| --------------------------------------- | ---------------------- | ------------------------------------ | ------------------------------- |
| Pages and canvas window content/layout  | Yes                    | Yes                                  | Active page and viewport        |
| Terminal process                        | Until host/OS restart  | Stream/input                         | Focus and selection             |
| Note paragraphs and relationships       | Yes                    | Character-level updates              | Undo stack and active editor    |
| File tree/editor navigation and content | Yes                    | Yes                                  | Menus, hover, undo, full-screen |
| SSH profiles                            | Encrypted file         | Metadata; writer mutation            | Password prompt input           |
| User preferences                        | No                     | No                                   | Browser `localStorage`          |
| Online users and editing ownership      | No                     | Transient                            | Hover/overflow menu             |
| Uploaded images                         | Temporary daemon cache | Encrypted transfer and inserted path | Drag/paste UI state             |
