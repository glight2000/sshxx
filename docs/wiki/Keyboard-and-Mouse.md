# Keyboard and mouse controls

## Canvas

- Left-drag empty canvas space to draw a selection marquee. Any terminal, note,
  or file window touched by the changing marquee becomes selected immediately;
  leaving the marquee removes it from the selection immediately.
- Right-drag empty canvas space to pan. A right-click without movement opens the
  canvas action menu and does not change the current selection.
- The browser-local **Canvas mouse buttons** setting can exchange the two
  blank-canvas drag gestures: right-drag selects and left-drag pans. A
  stationary right-click still opens the canvas action menu; a completed
  right-drag does not.
- Middle-button drag always pans, even when the pointer is over a window.
- `Ctrl` + wheel always zooms the canvas and suppresses browser zoom. Wheel zoom
  uses the faster canvas step; it does not change browser page scale.
- Plain wheel is routed to a hovered terminal, note, menu, tree, directory grid,
  or editor. Outside windows, it zooms when no canvas item is active.
- When a component is full-screen, canvas pan/zoom is disabled. Clicking the
  visible space outside the component exits full-screen.
- Double-click a page name in the bottom pager to rename it inline; there is no
  separate edit action.

## Windows

- Window title bars keep the normal pointer cursor. A click focuses the window,
  a double-click edits its title inline, and movement beyond the drag threshold
  moves it without changing focus.
- Selection and input/editing focus are mutually exclusive. Clicking a window
  focuses only that window and clears the complete selection; clicking empty
  canvas or pressing Escape also clears it. Dragging one of several selected
  windows moves the complete group with one shared offset. Selected windows use
  a pulsing yellow border; the selection is local to the current viewer and
  page.
- While moving one window or a selected group, hover another page in the bottom
  pager. Only the page under the pointer receives target feedback; the moving
  windows shrink and fade toward it, then reverse that preview when the pointer
  leaves. Releasing moves the exact selection to that page and opens it locally.
  The windows keep their original coordinates and relative layout.
- Drag any edge or corner to resize. Every direction uses the same terminal
  minimum of 32 columns by 8 rows; the effective visual minimum also retains
  title-bar/chrome space and never goes below 240 by 160 canvas pixels.
- With snapping enabled, the leading and trailing anchors use the same one-tenth
  grid inset. New items are created with matching aligned geometry.
- Close and full-screen controls are at the left of the title bar. Contextual
  actions and appearance settings are at the right.

## Terminal

- If terminal text is selected, `Ctrl+C` copies it and clears the selection.
  Otherwise `Ctrl+C` is sent to the foreground process.
- Enter sends normal terminal input. `Shift+Enter` sends LF for applications
  that support multiline input. The foreground program ultimately decides how LF
  is interpreted.
- Paste or drop supported images into a local terminal to upload and insert a
  daemon-local path.

## Note

- Click a paragraph to edit it. `Escape` or an outside click ends editing.
- Enter inserts a line break; `Ctrl`/`Cmd` + Enter creates a new paragraph.
- `Ctrl`/`Cmd` + Z and redo variants apply to the active note editor only.
- Click a four-dot handle to select its paragraph. `Ctrl`/`Cmd`-click toggles
  individual paragraphs, while Shift-click selects a range. Drag across the
  paragraph bodies to select a visual range without creating a browser text
  selection. Then drag any selected handle to move the group inside the note or
  copy it to another target.
- The handle menu contains paragraph-local delete, copy, and insertion actions.
  Hover a paragraph and use its right-side send button to choose a linked
  target.
- `Ctrl`/`Cmd` + C and V preserve paragraph boundaries between notes. Terminals
  and file editors receive the same selection as multiline plain text.
- Delete/Backspace outside paragraph editing, or Delete in the handle menu,
  removes the complete selected paragraph group.

## File explorer

- The folder tree follows standard arrow-key tree navigation. Only its arrow
  toggles expansion; clicking the row selects the folder.
- The directory grid opens files/folders on double-click.
- Right-click opens sshxx actions instead of the browser context menu.
- `Ctrl`/`Cmd` + Z and redo variants remain local to the active text editor.
