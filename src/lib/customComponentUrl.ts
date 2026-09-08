export type CustomComponentUrlResult =
  | { url: string; error: ""; previewError?: string }
  | { url: ""; error: string; previewError?: never };

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
    if (
      viewer.protocol === "https:" &&
      target.protocol === "http:" &&
      target.hostname !== "localhost" &&
      !target.hostname.endsWith(".localhost") &&
      !/^127(?:\.\d{1,3}){3}$/.test(target.hostname) &&
      target.hostname !== "[::1]"
    )
      return {
        url: target.href,
        error: "",
        previewError:
          "This HTTPS workspace cannot embed an HTTP page: browsers block mixed content. Use an HTTPS address for the target page, or open it separately. Direct access working does not make iframe embedding available.",
      };
    return { url: target.href, error: "" };
  } catch {
    return { url: "", error: "Enter a complete, valid URL." };
  }
}
