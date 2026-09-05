/** Legacy defaults follow the viewer's theme without rewriting shared data. */
export function surfaceBackground(
  value: string,
  legacyDefault: string,
): string {
  return value.toLowerCase() === legacyDefault ? "" : value;
}

/** Choose the higher-contrast text palette for an explicit background. */
export function surfaceTone(value: string): "dark" | "light" | undefined {
  if (!/^#[0-9a-f]{6}$/i.test(value)) return undefined;
  const channels = [1, 3, 5].map((offset) => {
    const channel = parseInt(value.slice(offset, offset + 2), 16) / 255;
    return channel <= 0.04045
      ? channel / 12.92
      : ((channel + 0.055) / 1.055) ** 2.4;
  });
  const luminance =
    0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
  return luminance > 0.179 ? "light" : "dark";
}
