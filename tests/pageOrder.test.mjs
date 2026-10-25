import assert from "node:assert/strict";
import test from "node:test";
import {
  addedPageId,
  movePage,
  pageMoveAtX,
  pageMoveForOrder,
  pageScrollSpeed,
} from "../src/lib/pageOrder.ts";

test("page creation selection distinguishes new identities from reordered snapshots", () => {
  const previous = [{ id: 7 }, { id: 2 }, { id: 19 }];
  assert.equal(addedPageId(previous, [{ id: 19 }, { id: 7 }, { id: 2 }]), null);
  assert.equal(addedPageId(previous, [{ id: 2 }, { id: 7 }]), null);
  assert.equal(
    addedPageId(previous, [{ id: 7 }, { id: 2 }, { id: 19 }, { id: 20 }]),
    20,
  );
  assert.equal(
    addedPageId(previous, [{ id: 20 }, { id: 19 }, { id: 7 }, { id: 2 }]),
    20,
  );
  assert.equal(addedPageId(previous, [{ id: 7 }, { id: 20 }, { id: 19 }]), 20);
});

test("page moves preserve identity and the relative order of other pages", () => {
  const original = [7, 2, 19, 5];
  assert.deepEqual(movePage(original, { id: 19, beforeId: 7 }), [19, 7, 2, 5]);
  assert.deepEqual(
    movePage(original, { id: 7, beforeId: null }),
    [2, 19, 5, 7],
  );
  assert.deepEqual(movePage(original, { id: 7, beforeId: 19 }), [2, 7, 19, 5]);
  for (const move of [
    { id: 99, beforeId: 7 },
    { id: 7, beforeId: 99 },
    { id: 7, beforeId: 7 },
  ])
    assert.deepEqual(movePage(original, move), original);
  assert.deepEqual(original, [7, 2, 19, 5]);
  assert.equal(pageMoveForOrder(original, original, 7), null);
  assert.deepEqual(pageMoveForOrder(original, [2, 19, 5, 7], 7), {
    id: 7,
    beforeId: null,
  });
  assert.deepEqual(pageMoveForOrder(original, [19, 7, 2, 5], 19), {
    id: 19,
    beforeId: 7,
  });
});

test("unequal tab widths use neighbor centers and end slots", () => {
  const boxes = [
    { id: 7, center: 20 },
    { id: 2, center: 130 },
    { id: 19, center: 240 },
  ];
  assert.deepEqual(pageMoveAtX(boxes, 7, 129), { id: 7, beforeId: 2 });
  assert.deepEqual(pageMoveAtX(boxes, 7, 131), { id: 7, beforeId: 19 });
  assert.deepEqual(pageMoveAtX(boxes, 7, 241), { id: 7, beforeId: null });
  assert.deepEqual(pageMoveAtX(boxes, 19, -10), { id: 19, beforeId: 7 });
});

test("overflow scroll accelerates at either edge and stops outside the pager", () => {
  assert.equal(pageScrollSpeed(50, 0, 100), 0);
  assert.equal(pageScrollSpeed(-1, 0, 100), 0);
  assert.equal(pageScrollSpeed(101, 0, 100), 0);
  assert.equal(pageScrollSpeed(0, 0, 100), -480);
  assert.equal(pageScrollSpeed(100, 0, 100), 480);
  assert.equal(pageScrollSpeed(16, 0, 100), -240);
  assert.equal(pageScrollSpeed(84, 0, 100), 240);
});
