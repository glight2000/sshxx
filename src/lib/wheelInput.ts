export type WheelInputKind = "mouse" | "trackpad";
export type WheelInputMode = "auto" | WheelInputKind;
// Slow swipes and momentum can pause longer than a wheel tick. Device and
// destination routing must use the same boundary for these sparse samples.
export const WHEEL_GESTURE_GAP_MS = 350;
const TRACKPAD_MEMORY_MS = 1500;

export function isWheelInputMode(value: unknown): value is WheelInputMode {
  return value === "auto" || value === "mouse" || value === "trackpad";
}

type WheelSample = Pick<
  WheelEvent,
  "deltaMode" | "deltaX" | "deltaY" | "timeStamp"
>;

/** Keep trackpad evidence through acceleration and brief pauses. */
export class WheelInputClassifier {
  #lastTime = -Infinity;
  #lastTrackpadTime = -Infinity;
  #kind: WheelInputKind = "mouse";

  classify(event: WheelSample, mode: WheelInputMode): WheelInputKind {
    if (mode !== "auto") {
      this.#lastTime = -Infinity;
      this.#lastTrackpadTime = -Infinity;
      return mode;
    }
    if (event.deltaX === 0 && event.deltaY === 0) return this.#kind;
    if (event.timeStamp < this.#lastTime) this.#lastTrackpadTime = -Infinity;
    const trackpadEvidence =
      event.deltaMode === 0 &&
      (event.deltaX !== 0 ||
        !Number.isInteger(event.deltaY) ||
        Math.abs(event.deltaY) < 40);
    if (
      event.timeStamp - this.#lastTime > WHEEL_GESTURE_GAP_MS ||
      event.timeStamp < this.#lastTime
    ) {
      if (event.deltaMode !== 0) this.#lastTrackpadTime = -Infinity;
      // A large integer pixel delta is ambiguous, not proof of a mouse wheel.
      // Preserve recent trackpad evidence across a new gesture's first sample.
      this.#kind =
        event.deltaMode === 0 &&
        (trackpadEvidence ||
          event.timeStamp - this.#lastTrackpadTime <= TRACKPAD_MEMORY_MS)
          ? "trackpad"
          : "mouse";
    } else if (trackpadEvidence) {
      // Correct an ambiguous first sample as soon as stronger evidence arrives;
      // never change a recognized pan back into zoom during the same gesture.
      this.#kind = "trackpad";
    }
    if (trackpadEvidence) this.#lastTrackpadTime = event.timeStamp;
    this.#lastTime = event.timeStamp;
    return this.#kind;
  }
}

export function wheelDestination(
  focused: boolean,
  device: WheelInputKind,
  startedOverFocus = true,
) {
  if (device === "trackpad")
    return focused && startedOverFocus ? "component" : "pan";
  return focused ? "component" : "zoom";
}
