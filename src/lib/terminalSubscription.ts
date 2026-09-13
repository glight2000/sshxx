import type { WsClient } from "./protocol";

/** Zero is a fresh renderer; resumed renderers require an exact next chunk. */
export function terminalBatchIsContinuous(
  expected: number,
  start: number,
  previousEpoch?: Uint8Array,
  epoch?: Uint8Array,
) {
  return (
    Number.isSafeInteger(start) &&
    start >= 0 &&
    (expected === 0 || expected === start) &&
    (!previousEpoch ||
      (previousEpoch.length === (epoch?.length ?? 0) &&
        previousEpoch.every((byte, index) => byte === epoch?.[index])))
  );
}

/** Viewer-local subscription identity; never persisted or shared with viewers. */
export function terminalSubscriptionMessage(
  id: number,
  page: number,
  generation: number,
  chunk: number,
  token: number,
  capabilities: {
    recovery: boolean;
    generation: boolean;
    flowControl: boolean;
  },
): WsClient {
  if (capabilities.recovery)
    return { subscribeRecoverable: [id, page, generation, token, chunk] };
  if (capabilities.generation)
    return capabilities.flowControl
      ? { subscribeFlowControlledGeneration: [id, page, generation, chunk] }
      : { subscribeGeneration: [id, page, generation, chunk] };
  return capabilities.flowControl
    ? { subscribeFlowControlled: [id, page, chunk] }
    : { subscribe: [id, page, chunk] };
}
