import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  MOBILE_TERMINAL_KEYS,
  mobileTerminalKey,
} from "../src/lib/mobileTerminalKeys.ts";

test("phone keys follow cursor mode and never append Enter or paste markers", () => {
  for (const [key, sequence] of Object.entries(MOBILE_TERMINAL_KEYS)) {
    assert.equal(mobileTerminalKey(key, false), sequence);
    assert.equal(
      mobileTerminalKey(key, true),
      /^\x1b\[[ABCDHF]$/.test(sequence) ? sequence.replace("[", "O") : sequence,
    );
    if (key !== "Enter") assert.ok(!sequence.includes("\r"));
    assert.ok(!sequence.includes("200~"));
  }
  assert.equal(mobileTerminalKey("↑", false), "\x1b[A");
  assert.equal(mobileTerminalKey("↑", true), "\x1bOA");
  assert.equal(mobileTerminalKey("Ctrl+C", false), "\x03");
  assert.equal(mobileTerminalKey("toString", true), null);
  const reader = readFileSync("src/lib/ui/MobileTerminalReader.svelte", "utf8");
  assert.match(reader, /send\(draft, sendMode\)/);
  assert.match(reader, /send\(sequence, "keys"\)/);
  assert.match(reader, /invalidKeys/);
  const keypad = readFileSync("src/lib/ui/MobileTerminalKeypad.svelte", "utf8");
  assert.match(keypad, /on:pointerdown\|capture/);
  assert.match(keypad, /aria-label="Terminal arrow keys"/);
  assert.match(reader, /disabled=\{!writable \|\| !!blocked\}/);
  const controls = readFileSync(
    "src/lib/ui/MobileWorkspaceControls.svelte",
    "utf8",
  );
  assert.match(
    controls,
    /\{#if current.kind === "terminal" && !fullscreen\}\s*<MobileTerminalKeypad/,
  );
  assert.match(reader, /<MobileTerminalKeypad/);
  for (const surface of [controls, reader]) {
    assert.doesNotMatch(
      surface,
      /Object.keys\(MOBILE_TERMINAL_KEYS\)/,
      "key layout stays in the shared keypad, not each surface",
    );
  }
});
