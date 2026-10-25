# Requirements

This document owns the maintained terminal clipboard, page ordering and canvas
input requirements. Existing requirements for unaffected features remain in the
[versioned feature guide](docs/wiki/Features.md) and its linked controls.
Implementation and validation procedures live in
[ENGINEERING.md](ENGINEERING.md).

## Canvas wheel and trackpad input

- Mouse wheels scroll the focused component regardless of pointer position;
  without focus they zoom the canvas. Two-finger trackpad gestures scroll a
  component only when starting inside its focused window; otherwise they pan the
  canvas. Neither action changes focus. File explorers retain both scroll axes,
  using the active editor or last-clicked pane.
- Short pauses, acceleration and integer-valued trackpad samples must not turn
  an identified pan into zoom or retarget it to a window moving under the
  pointer. A deliberate click or a new gesture can choose a new destination.
  Empty events must not move the camera, scroll content or choose a destination.
- Canvas events must cancel native outer-page scrolling. Menus and dialogs keep
  their own scrolling; embedded cross-origin pages handle input inside their
  frames. Fullscreen disables canvas pan/zoom.
- `Ctrl` + wheel explicitly zooms the canvas and suppresses browser zoom, using
  the existing faster wheel zoom step. It remains distinct from ordinary
  two-finger movement.
- **Scroll input device** defaults to Auto. Event-based identification remains
  approximate; Mouse wheel and Trackpad are explicit browser-local overrides.
  Routing and camera changes are not synchronized workspace mutations.

## Page ordering

- Writers can drag bottom pager items to reorder pages. Moving one item inserts
  it at the indicated position; other items retain their relative order. The
  order is shared with all viewers and survives workspace and server restores.
- Desktop dragging begins after a small movement threshold. A raised copy
  follows the pointer, a marked placeholder shows the destination, and adjacent
  items slide aside. Overflowing pagers scroll when dragging near either edge.
  Reduced-motion preferences disable the sliding animation.
- On touch screens, hold a page for 450 ms before dragging. A normal tap
  switches pages; moving before the hold scrolls the pager normally. Multitouch
  cancels sorting. With a page button focused, `Alt+Shift+Left/Right` moves it
  one place.
- Touch users can open the current page's actions with its ellipsis button;
  sorting must not remove access to the existing page deletion menu. That button
  does not initiate a drag.
- Dropping within the pager commits one move. Escape, pointer cancellation,
  leaving the pager before dropping, window blur/resize, loss of write access or
  connection, and a new authoritative page list cancel a drag. Readers and
  disconnected clients cannot reorder pages. Dragging must not also click a
  different page or trigger inline renaming.
- Reordering preserves the current page, each page's mounted components, their
  content and geometry, and browser-local cameras. Ordinary clicks still switch
  pages; double-clicking a name still renames it inline. Dragging canvas items
  onto a page retains its separate cross-page move behavior.

## Terminal clipboard

- With terminal text selected, `Ctrl+C` on Windows/Linux or `Cmd+C` on macOS
  copies the current selection. The Shift variants also copy. Without a
  selection, `Ctrl+C` retains its terminal interrupt behavior.
- Each copy shortcut must initiate one clipboard write and suppress the
  browser's competing default copy. The clipboard receives the selection
  captured at the time of that shortcut, including Unicode and multiline text.
- Show success only after the write succeeds. Clear the copied selection only if
  it is still current. A denied or failed write shows an error and preserves the
  selection so the user can retry. Late completion must not change a closed
  terminal or erase a selection made while the write was pending.
- Browsers without the async Clipboard API may use the native copy command; a
  rejected command must never produce a success notification.
- Copy remains local to the browser and operating system. It never sends input
  to the terminal process, changes shared window state or persists copied text
  in the workspace. Native copy from note editors, title fields and the phone
  text reader keeps its existing behavior.
