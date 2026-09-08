export type WheelInputKind = "mouse" | "trackpad";
export type WheelInputMode = "auto" | WheelInputKind;
export const WHEEL_GESTURE_GAP_MS = 180;

export function isWheelInputMode(value: unknown): value is WheelInputMode {
  return value === "auto" || value === "mouse" || value === "trackpad";
}

type WheelSample = Pick<
  WheelEvent,
  "deltaMode" | "deltaX" | "deltaY" | "timeStamp"
>;

/** WheelEvent has no device identity. Keep one guess throughout a gesture. */
export class WheelInputClassifier {
  #lastTime = -Infinity;
  #kind: WheelInputKind = "mouse";

  classify(event: WheelSample, mode: WheelInputMode): WheelInputKind {
    if (mode !== "auto") {
      this.#lastTime = -Infinity;
      return mode;
    }
    if (
      event.timeStamp - this.#lastTime > WHEEL_GESTURE_GAP_MS ||
      event.timeStamp < this.#lastTime
    ) {
      // ponytail: a high-resolution wheel can look exactly like a touchpad.
      // The viewer-local device override is the fallback, not a UA/device list.
      this.#kind =
        event.deltaMode === 0 &&
        (event.deltaX !== 0 ||
          !Number.isInteger(event.deltaY) ||
          Math.abs(event.deltaY) < 40)
          ? "trackpad"
          : "mouse";
    }
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
