import assert from "node:assert/strict";
import test from "node:test";
import {
  terminalSubscriptionMessage,
  terminalBatchIsContinuous,
} from "../src/lib/terminalSubscription.ts";

test("resuming requires contiguous chunks and the same output incarnation", () => {
  const epoch = new Uint8Array(16).fill(1);
  assert.equal(terminalBatchIsContinuous(0, 90), true);
  assert.equal(terminalBatchIsContinuous(4, 4, epoch, epoch.slice()), true);
  assert.equal(terminalBatchIsContinuous(4, 90), false);
  assert.equal(terminalBatchIsContinuous(4, 3), false);
  assert.equal(
    terminalBatchIsContinuous(4, 4, epoch, new Uint8Array(16).fill(2)),
    false,
  );
  assert.equal(terminalBatchIsContinuous(4, 4, epoch), false);
  assert.equal(
    terminalBatchIsContinuous(0, Number.MAX_SAFE_INTEGER + 1),
    false,
  );
});

test("recoverable subscriptions carry page, generation, checkpoint and viewer token", () => {
  const capabilities = { recovery: true, generation: true, flowControl: true };
  assert.deepEqual(terminalSubscriptionMessage(7, 2, 3, 12, 8, capabilities), {
    subscribeRecoverable: [7, 2, 3, 8, 12],
  });
  assert.deepEqual(terminalSubscriptionMessage(7, 4, 3, 16, 9, capabilities), {
    subscribeRecoverable: [7, 4, 3, 9, 16],
  });
});

test("new viewers keep speaking the negotiated older subscription protocols", () => {
  const args = [7, 2, 3, 12, 8];
  assert.deepEqual(
    terminalSubscriptionMessage(...args, {
      recovery: false,
      generation: true,
      flowControl: true,
    }),
    { subscribeFlowControlledGeneration: [7, 2, 3, 12] },
  );
  assert.deepEqual(
    terminalSubscriptionMessage(...args, {
      recovery: false,
      generation: false,
      flowControl: false,
    }),
    { subscribe: [7, 2, 12] },
  );
});
