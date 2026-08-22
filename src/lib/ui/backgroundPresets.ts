export type BackgroundPreset = {
  name: string;
  color: `#${string}`;
};

/**
 * Low-luminance canvas surfaces spanning neutral and chromatic hues. Every
 * preset keeps at least a 10:1 WCAG contrast ratio against the application's
 * primary zinc-200 text color.
 */
export const BACKGROUND_PRESETS: readonly BackgroundPreset[] = [
  { name: "Zinc", color: "#18181b" },
  { name: "Graphite", color: "#1c2028" },
  { name: "Ash", color: "#22252b" },
  { name: "Taupe", color: "#272329" },
  { name: "Midnight", color: "#111827" },
  { name: "Navy", color: "#14213d" },
  { name: "Cobalt", color: "#172554" },
  { name: "Indigo", color: "#1e1b4b" },
  { name: "Violet", color: "#271b3d" },
  { name: "Plum", color: "#321d3a" },
  { name: "Berry", color: "#3a1d2b" },
  { name: "Ember", color: "#3b2020" },
  { name: "Sepia", color: "#3b2518" },
  { name: "Amber", color: "#382d18" },
  { name: "Olive", color: "#303219" },
  { name: "Forest", color: "#1f321f" },
  { name: "Jade", color: "#16322b" },
  { name: "Teal", color: "#123238" },
  { name: "Ocean", color: "#14303d" },
  { name: "Steel", color: "#1b2b36" },
  { name: "Iris", color: "#2d2638" },
  { name: "Wine", color: "#3a2630" },
  { name: "Pine", color: "#193526" },
  { name: "Azure", color: "#173247" },
] as const;
