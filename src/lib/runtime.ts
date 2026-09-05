/** Runtime helpers shared by the browser and packaged application. */

const SERVER_QUERY_PARAM = "server";
const UPSTREAM_SSHX_HOST = "sshx.io";

/** Returns whether a URL targets the upstream project's public service. */
export function isUpstreamSshxUrl(url: URL): boolean {
  const hostname = url.hostname.toLowerCase().replace(/\.$/, "");
  return (
    hostname === UPSTREAM_SSHX_HOST ||
    hostname.endsWith(`.${UPSTREAM_SSHX_HOST}`)
  );
}

/** Returns whether the frontend is running inside a packaged desktop shell. */
export function isNativeApp(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "sshxxDesktop" in window)
  );
}

/**
 * Converts a relative sshxx API path into an absolute WebSocket URL.
 *
 * Browser sessions use the page origin. Packaged applications carry the
 * selected sshxx server origin in the `server` query parameter because their
 * own origin belongs to Tauri rather than to an sshxx server.
 */
export function resolveWebSocketUrl(path: string): string {
  const params = new URLSearchParams(window.location.search);
  const configuredOrigin = isNativeApp()
    ? params.get(SERVER_QUERY_PARAM)
    : null;
  const base = configuredOrigin
    ? new URL(configuredOrigin)
    : new URL(window.location.href);

  if (base.protocol !== "http:" && base.protocol !== "https:") {
    throw new Error("an sshxx server origin is required in the packaged app");
  }

  const url = new URL(path, base);
  url.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}

/** Converts a normal sshxx share link into an internal viewer route. */
export function viewerRouteFromShareUrl(value: string): string {
  const url = new URL(value.trim());
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("the session link must use http or https");
  }

  const match = /^\/s\/([^/]+)\/?$/.exec(url.pathname);
  if (!match) {
    throw new Error("the session link must contain /s/<session-id>");
  }

  const id = decodeURIComponent(match[1]);
  const server = encodeURIComponent(url.origin);
  return `/s/${encodeURIComponent(id)}?${SERVER_QUERY_PARAM}=${server}${
    url.hash
  }`;
}
