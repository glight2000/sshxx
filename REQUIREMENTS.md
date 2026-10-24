# Requirements

This document owns the terminal clipboard requirements maintained with the
current dependency upgrade. Existing requirements for unaffected features remain
in the [versioned feature guide](docs/wiki/Features.md) and its linked controls.
Implementation and validation procedures live in
[ENGINEERING.md](ENGINEERING.md).

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
