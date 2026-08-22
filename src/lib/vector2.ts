/** A two-dimensional value; callers may keep it in an ordinary number array. */
export type Vector2 = readonly number[];

export function addVector(a: Vector2, b: Vector2): [number, number] {
  return [a[0] + b[0], a[1] + b[1]];
}

export function subtractVector(a: Vector2, b: Vector2): [number, number] {
  return [a[0] - b[0], a[1] - b[1]];
}

export function multiplyVector(
  vector: Vector2,
  scalar: number,
): [number, number] {
  return [vector[0] * scalar, vector[1] * scalar];
}

export function divideVector(
  vector: Vector2,
  scalar: number,
): [number, number] {
  return [vector[0] / scalar, vector[1] / scalar];
}

export function lerpVector(
  from: Vector2,
  to: Vector2,
  amount: number,
): [number, number] {
  return [
    from[0] + (to[0] - from[0]) * amount,
    from[1] + (to[1] - from[1]) * amount,
  ];
}

export function vectorsEqual(a: Vector2, b: Vector2): boolean {
  return a[0] === b[0] && a[1] === b[1];
}

export function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
