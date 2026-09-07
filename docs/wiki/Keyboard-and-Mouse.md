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
- With a focused component, mouse-wheel and two-finger trackpad scrolling both
  scroll that component, regardless of pointer position. File explorers use the
  actively edited or last-clicked pane, with horizontal and vertical scrolling.
- Without component focus, a mouse wheel zooms and a trackpad pans the canvas,
  even when the pointer is over a window. Neither action changes focus.
- **Scroll input device** in Settings defaults to Auto. Because browsers do not
  expose the physical device, choose Mouse wheel or Trackpad if Auto guesses
  incorrectly. The choice is local to this browser. Menus/dialogs scroll
  themselves; cross-origin embedded pages handle their own input.
- When a component is full-screen, canvas pan/zoom is disabled. Clicking the
  visible space outside the component exits full-screen.
- Double-click a page name in the bottom pager to rename it inline; there is no
  separate edit action.

## Desktop workspace shortcuts

| Default shortcut           | Action                                      |
| -------------------------- | ------------------------------------------- |
| `Ctrl+1` … `Ctrl+9`        | Open page 1 … 9 in the bottom pager's order |
| `Ctrl+0`                   | Open page 10                                |
| `Ctrl+Left` / `Ctrl+Right` | Previous / next page                        |
| `Ctrl+Up` / `Ctrl+Down`    | First / last page                           |
| `Ctrl+Space`               | Open the toolbar search and focus its input |

Missing page numbers do nothing; adjacent-page navigation stops at the ends
without wrapping. Switching pages preserves their local camera positions and
keeps component instances mounted. It does not switch another viewer's page.

In **Settings → Keyboard shortcuts**, click a binding and press its replacement.
Changes save immediately. Escape cancels recording; **Disable** removes one
binding and **Restore defaults** resets the complete map. Duplicate bindings are
rejected. Use Ctrl, Alt, or Meta with a supported key, or F1–F12. Preferences
live in this browser profile's local storage for the current origin, including
its other tabs; they are not sent to the server or persisted by the daemon.

These shortcuts take priority over terminal/editor bindings, but pause while a
modal dialog or Settings is open, during IME composition, and in phone mode.
Keys inside embedded website frames do not reach the workspace. Browser and OS
shortcuts may intercept a combination before the page receives it—for example
tab switching or an input-method toggle. Rebind such combinations in Settings;
see the
[Firefox shortcut reference](https://support.mozilla.org/en-US/kb/keyboard-shortcuts-perform-firefox-tasks-quickly)
for platform-specific browser bindings.

## Windows

- On desktop, `Escape` clears component focus and canvas selection without
  closing the component. In a terminal it is a browser focus command, not PTY
  input. Editor/menu Escape handling still finishes normally.

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
- Close, minimize/restore, and full-screen controls are at the left of the title
  bar. Minimize/restore is shared and persisted; it preserves the expanded width
  and height while showing a title bar exactly one grid unit high. Contextual
  actions and appearance settings are at the right.

## Phone navigation

- Touch-primary screens up to 1024 CSS pixels use dedicated touch controls with
  the same top toolbar, search and bottom page tabs as desktop. Tapping a window
  only focuses it, without recentering or resizing. **Fullscreen** on the
  focused window's floating controls opens its adaptive view; **Restore**
  returns to the unchanged canvas. There is no separate Pages list or Back
  toolbar.
- In the normal canvas, one finger pans over blank space or unfocused windows;
  focused content keeps its own scrolling/editing. Two fingers always control
  canvas scale and midpoint movement, regardless of focus. Drags never select or
  move shared windows. In fullscreen, canvas gestures and outer browser zoom are
  disabled; one-finger content scrolling/editing remains available.
- Embedded pages receive interaction in fullscreen. On the normal canvas their
  iframe is touch-shielded so gestures reach the canvas. Cross-origin page
  scripts and their internal gesture handling remain outside the parent's
  control.
- Viewing a minimized component temporarily reveals its content locally. It does
  not restore the shared window or resize the terminal PTY. Terminal detail
  pages show a selectable, wrapping text view above a multiline input box. Hold
  output text to select/copy; text updates pause during selection or while
  reading earlier output without pausing terminal reception. **Latest** clears
  selection and resumes following output. Enter adds an input line; **Send ↵**
  pastes the text then sends Enter to the foreground program. Drafts stay local
  and are cleared when leaving the detail page. Read-only viewers cannot send;
  disconnected/replaying viewers keep their draft and see why sending is
  unavailable.
- The text view is a bounded projection of the current terminal buffer, not an
  archived conversation. Overwritten/cleared text is not recovered, and
  full-screen applications replace their current screen rather than append each
  redraw. The reader follows this terminal's theme/background and preserves ANSI
  colors, bold, italic, underline, dim and reverse-video text. Images and exact
  screen layout are not reproduced. Excessively fragmented older styling falls
  back to default text with a notice. A `Recent output only` notice marks
  truncation. Only the current detail view is projected: at most 2,000 physical
  rows, 128K UTF-16 units and 128K scanned cells, refreshed at most every 250ms
  while output changes. Input is limited to 16K UTF-16 units. No new terminal
  instance, subscription, lock, shared layout change or PTY resize is created.
- The detail page follows the visible viewport when the keyboard opens or the
  phone rotates. It scrolls locally and keeps the input box available; the
  original xterm painter remains mounted but hidden in terminal detail mode.
- Tap association icons to locate and focus their target on the canvas. Hold for
  the full name and **Open component** / **Remove association** actions; dismiss
  with an outside tap, Close, or Escape. Horizontal dragging scrolls the icon
  strip. The note's **+** opens a list of available same-page targets; Cancel or
  an outside tap leaves associations unchanged. Only add/remove operations sync.
- Phone fullscreen hides the top toolbar, bottom page tabs and the component's
  own titlebar controls. The content uses the freed space; **Restore** remains
  available at the top left, above the content, and brings the normal navigation
  back. Desktop controls are unchanged.

## Terminal

- The phone composer defaults to **Text + Enter**. **Paste only** appends no
  Enter; **Direct keys** sends draft text without paste markers or Enter and
  requires a single line. Arrow buttons are always visible in the focused
  terminal's controls and the fullscreen reader; both views reuse one keypad
  component and display it mutually exclusively. **Keys** exposes Enter, Tab,
  Esc, editing/navigation keys and common Ctrl combinations. These buttons act
  on the remote terminal immediately, not the local input box, and leave any
  draft intact. Ctrl+C means interrupt, Ctrl+D may end input or exit the shell,
  and Ctrl+Z may suspend the foreground job. Close the panel using Keys, an
  outside tap, or Escape. Disconnected, read-only or replay-blocked terminals
  cannot send text or special keys.

- If terminal text is selected, `Ctrl+C` copies it and clears the selection.
  Otherwise `Ctrl+C` is sent to the foreground process.
- Enter sends normal terminal input. `Shift+Enter` sends LF for applications
  that support multiline input. The foreground program ultimately decides how LF
  is interpreted.
- Paste or drop supported images into a local terminal to upload and insert a
  daemon-local path.

## Note

- Click a paragraph to edit it. `Escape` or an outside click ends editing.
- Drag within one paragraph to select native text, even if the note was not
  focused. The note gains focus without acquiring an editing lock, and the
  selection remains after release for copying. This also works in read-only mode
  or while another viewer edits the note.
- Enter inserts a line break; `Ctrl`/`Cmd` + Enter creates a new paragraph.
- `Ctrl`/`Cmd` + Z and redo variants apply to the active note editor only.
- Click a four-dot handle to select its paragraph. `Ctrl`/`Cmd`-click toggles
  individual paragraphs, while Shift-click selects a range. Starting in a
  non-editing paragraph and dragging into another paragraph switches to a visual
  block range instead of a cross-paragraph browser text selection. Then drag any
  selected handle to move the group inside the note or copy it to another
  target.
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
