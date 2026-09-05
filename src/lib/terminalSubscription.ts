import type { WsClient } from "./protocol";

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
