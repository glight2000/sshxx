export type CustomComponentUrlResult =
  { url: string; error: "" } | { url: ""; error: string };

/** Resolve a URL preview while refusing unsupported and recursive targets. */
export function resolveCustomComponentUrl(
  value: string,
  viewerHref: string,
): CustomComponentUrlResult {
  const input = value.trim();
  if (!input) return { url: "", error: "Enter a URL to render." };
  try {
    const viewer = new URL(viewerHref);
    const target = new URL(input);
    if (target.protocol !== "http:" && target.protocol !== "https:")
      return { url: "", error: "Only HTTP and HTTPS URLs can be rendered." };
    if (target.origin === viewer.origin)
      return {
        url: "",
        error: "sshxx refuses to render its own origin recursively.",
      };
    return { url: target.href, error: "" };
  } catch {
    return { url: "", error: "Enter a complete, valid URL." };
  }
}
