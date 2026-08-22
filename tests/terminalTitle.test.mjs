import assert from "node:assert/strict";
import test from "node:test";

import { splitTerminalTitle } from "../src/lib/terminalTitle.ts";

test("keeps Codex title spinner frames separate from the editable title", () => {
  assert.deepEqual(splitTerminalTitle("⠹ sshxx"), {
    activity: "⠹",
    title: "sshxx",
  });
  assert.deepEqual(splitTerminalTitle("⠋   release workspace"), {
    activity: "⠋",
    title: "release workspace",
  });
});

test("does not strip ordinary leading title characters", () => {
  assert.deepEqual(splitTerminalTitle("[ ! ] Action Required · sshxx"), {
    activity: "",
    title: "[ ! ] Action Required · sshxx",
  });
  assert.deepEqual(splitTerminalTitle("Remote Terminal"), {
    activity: "",
    title: "Remote Terminal",
  });
});
