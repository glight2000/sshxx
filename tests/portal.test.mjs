import assert from "node:assert/strict";
import test from "node:test";

import { portal } from "../src/lib/action/portal.ts";

function createParent() {
  return {
    children: [],
    appendChild(node) {
      node.remove();
      this.children.push(node);
      node.parentNode = this;
    },
  };
}

function createNode(parent) {
  const node = {
    parentNode: null,
    remove() {
      if (!this.parentNode) return;
      const index = this.parentNode.children.indexOf(this);
      if (index !== -1) this.parentNode.children.splice(index, 1);
      this.parentNode = null;
    },
  };
  parent.appendChild(node);
  return node;
}

test("portal moves one existing node without adding list siblings", () => {
  const slot = createParent();
  const layer = createParent();
  const node = createNode(slot);
  const action = portal(node, { active: false, target: layer });

  assert.deepEqual(slot.children, [node]);
  assert.deepEqual(layer.children, []);

  action.update({ active: true, target: layer });
  assert.deepEqual(slot.children, []);
  assert.deepEqual(layer.children, [node]);

  action.update({ active: false, target: layer });
  assert.deepEqual(slot.children, [node]);
  assert.deepEqual(layer.children, []);
});

test("destroying a portaled item removes it instead of resurrecting it", () => {
  const slot = createParent();
  const layer = createParent();
  const node = createNode(slot);
  const action = portal(node, { active: true, target: layer });

  action.destroy();

  assert.deepEqual(slot.children, []);
  assert.deepEqual(layer.children, []);
  assert.equal(node.parentNode, null);
});
