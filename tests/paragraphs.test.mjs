import assert from "node:assert/strict";
import { test } from "node:test";

import {
  deleteParagraphs,
  deserializeParagraphs,
  normalizeParagraphIndexes,
  paragraphPlainText,
  reorderParagraphs,
  selectedParagraphs,
  serializeParagraphs,
} from "../src/lib/paragraphs.ts";

test("moves multiple paragraphs as one stable block", () => {
  assert.deepEqual(reorderParagraphs(["a", "b", "c", "d"], [1, 2], 4), {
    paragraphs: ["a", "d", "b", "c"],
    selectedIndexes: [2, 3],
  });
});

test("preserves non-contiguous selection order and clamps drop targets", () => {
  assert.deepEqual(
    reorderParagraphs(["a", "b", "c", "d", "e"], [3, 1, 3], -10),
    {
      paragraphs: ["b", "d", "a", "c", "e"],
      selectedIndexes: [0, 1],
    },
  );
});

test("dropping inside the selected block is a no-op", () => {
  assert.deepEqual(reorderParagraphs(["a", "b", "c", "d"], [1, 2], 3), {
    paragraphs: ["a", "b", "c", "d"],
    selectedIndexes: [1, 2],
  });
});

test("normalizes a paragraph selection and preserves its structure", () => {
  const paragraphs = ["one\ncontinued", "two", "three"];
  const indexes = normalizeParagraphIndexes([2, 0, 2, 9], paragraphs.length);
  assert.deepEqual(indexes, [0, 2]);
  assert.deepEqual(selectedParagraphs(paragraphs, indexes), [
    "one\ncontinued",
    "three",
  ]);
  assert.equal(
    paragraphPlainText(selectedParagraphs(paragraphs, indexes)),
    "one\ncontinued\nthree",
  );
});

test("round-trips structured paragraph clipboard data", () => {
  const paragraphs = ["one\ncontinued", "two"];
  assert.deepEqual(
    deserializeParagraphs(serializeParagraphs(paragraphs)),
    paragraphs,
  );
  assert.equal(
    deserializeParagraphs('{"version":2,"paragraphs":["no"]}'),
    null,
  );
  assert.equal(deserializeParagraphs('{"version":1,"paragraphs":[]}'), null);
});

test("deletes a non-contiguous paragraph selection as one operation", () => {
  assert.deepEqual(deleteParagraphs(["a", "b", "c", "d"], [3, 1]), {
    paragraphs: ["a", "c"],
    selectedIndex: 1,
  });
  assert.deepEqual(deleteParagraphs(["only"], [0]), {
    paragraphs: [""],
    selectedIndex: 0,
  });
});
