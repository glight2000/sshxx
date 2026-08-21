# Feature guide

## Persistent multi-page canvas

Each page owns an independent set of terminals, notes, and file explorer
windows. Their shared layout is stored by the daemon. The active page, canvas
pan/zoom, and temporary full-screen state are local to each viewer, so one user
can navigate without moving another user's viewport.

Global search spans all pages and all three canvas item types. Choosing a result
switches to its page and centers the target.

## Terminals and SSH profiles

The main half of the terminal split button opens a default local terminal. The
arrow opens reusable SSH profiles and profile management.

![SSH profile editor](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-ssh-profile.png)

Profiles support OpenSSH config/default behavior, SSH Agent, private-key files,
and interactive password prompts. Passwords are never stored. Profiles may also
choose the new terminal's color theme and an optional custom background.

Each terminal window can independently change its title, opacity, theme, and
background. It can be duplicated with the same working directory/environment,
opened full-screen locally, or used as the source for a file explorer.

## Notes, relationships, and paragraph delivery

Notes contain visible paragraph blocks. Enter inserts a line break inside a
paragraph; `Ctrl`/`Cmd` + Enter splits it into a new paragraph. Local undo/redo
works while a note editor is active.

![Note relationships and paragraph actions](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-notes.png)

The plus button at the bottom of a note starts target selection. A note can link
to terminals, other notes, and file editors on the same page. Relationship icons
navigate to targets; right-click removes a relationship. Focus highlights linked
items using the target type's distinct visual treatment.

Paragraph actions can send text to all links or only one target category. The
terminal “send and run” action appends execution input. Dragging a paragraph
copies it at the indicated insertion point in another note, terminal, or
writable file editor.

## File explorer and editor

The file explorer is a normal canvas window: it moves, resizes, snaps, links,
synchronizes, persists, and supports local full-screen just like terminals and
notes.

![File explorer and editor](https://raw.githubusercontent.com/glight2000/sshxx/main/docs/images/sshxx-file-explorer.png)

The left side shows folders only and supports keyboard navigation. The path bar
can locate an absolute directory. The right side uses a large-icon directory
grid; double-click enters folders or opens files. Text files up to 8 MiB open in
a CodeMirror editor with filename-based language support. Other supported files
are previewed when possible.

Available operations include upload, create file/folder, rename, move, delete,
save, and open terminal at the selected location. Context menus replace the
browser menu inside the file explorer. Destructive operations require explicit
confirmation where appropriate.

Local filesystem operations run beside the daemon. SSH browsing uses the OpenSSH
SFTP subsystem and requires a key/config/Agent profile; an interactive password
cannot be safely reused by the filesystem channel.

## Image paste

Pasting or dropping PNG, JPEG, WebP, or GIF data into a local terminal encrypts
the upload in the browser, stores it under the daemon working directory's
`cache/uploads/`, and inserts the resulting absolute path. The maximum image
size is 20 MiB. Completed cache files older than 24 hours are removed at daemon
startup, and abandoned active uploads are reclaimed after a short timeout.
