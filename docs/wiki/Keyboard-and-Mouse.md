# Keyboard and mouse controls

## Canvas

- Drag empty canvas space to pan.
- Middle-button drag always pans, even when the pointer is over a window.
- `Ctrl` + wheel always zooms the canvas and suppresses browser zoom.
- Plain wheel is routed to a hovered terminal, note, menu, tree, directory grid,
  or editor. Outside windows, it zooms when no canvas item is active.
- When a component is full-screen, canvas pan/zoom is disabled. Clicking the
  visible space outside the component exits full-screen.

## Windows

- Drag the title bar to move a window.
- Drag any edge or corner to resize.
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
- Click a four-dot paragraph handle for actions; drag it to copy the paragraph
  to a compatible target.

## File explorer

- The folder tree follows standard arrow-key tree navigation. Only its arrow
  toggles expansion; clicking the row selects the folder.
- The directory grid opens files/folders on double-click.
- Right-click opens sshxx actions instead of the browser context menu.
- `Ctrl`/`Cmd` + Z and redo variants remain local to the active text editor.
