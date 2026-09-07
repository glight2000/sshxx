import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const note = readFileSync("src/lib/ui/Note.svelte", "utf8");

test("note text selection remains native until a drag crosses paragraphs", () => {
  const range = note.slice(
    note.indexOf("function updateRangeSelection("),
    note.indexOf("function finishRangeSelection("),
  );
  assert.match(
    range,
    /!rangeSelection.active &&\s*paragraphIndexAt\(event.clientY\) === rangeSelection.anchor\s*\)\s*return;/,
  );
  assert.ok(
    range.indexOf("paragraphIndexAt(event.clientY) ===") <
      range.indexOf("event.preventDefault()"),
  );
  assert.match(note, /\.paragraph-input \{[^}]*user-select: text;/);
  assert.match(
    note,
    /target instanceof HTMLTextAreaElement && !rangeSelection\?\.active/,
  );
});

test("reading selected text neither requests editing nor replaces native copy", () => {
  const begin = note.slice(
    note.indexOf("async function beginEditing("),
    note.indexOf("function portal("),
  );
  assert.match(begin, /if \(selectionStart !== selectionEnd\) return;/);
  assert.ok(
    begin.indexOf("if (selectionStart !== selectionEnd)") <
      begin.indexOf("await ensureEditing()"),
  );
  const copy = note.slice(
    note.indexOf("function handleCopy("),
    note.indexOf("async function pasteStructuredParagraphs("),
  );
  assert.match(copy, /active.selectionStart !== active.selectionEnd/);
  assert.ok(
    copy.indexOf("active.selectionStart") <
      copy.indexOf("event.preventDefault()"),
  );
});
