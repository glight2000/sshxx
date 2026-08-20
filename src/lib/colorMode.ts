export type ColorModePreference = "system" | "light" | "dark";
export type ResolvedColorMode = "light" | "dark";

export function isColorModePreference(
  value: unknown,
): value is ColorModePreference {
  return value === "system" || value === "light" || value === "dark";
}

export function resolveColorMode(
  preference: ColorModePreference,
  systemPrefersDark: boolean,
): ResolvedColorMode {
  return preference === "system"
    ? systemPrefersDark
      ? "dark"
      : "light"
    : preference;
}
