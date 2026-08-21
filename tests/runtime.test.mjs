import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { URL } from "node:url";

import {
  isUpstreamSshxUrl,
  resolveWebSocketUrl,
  viewerRouteFromShareUrl,
} from "../src/lib/runtime.ts";

const originalWindow = globalThis.window;

afterEach(() => {
  if (originalWindow === undefined) {
    delete globalThis.window;
  } else {
    globalThis.window = originalWindow;
  }
});

test("converts LAN share links without requiring TLS", () => {
  assert.equal(
    viewerRouteFromShareUrl("http://192.168.1.25:8051/s/demo#secret"),
    "/s/demo?server=http%3A%2F%2F192.168.1.25%3A8051#secret",
  );
  assert.equal(
    viewerRouteFromShareUrl("http://terminal.local/s/demo#secret"),
    "/s/demo?server=http%3A%2F%2Fterminal.local#secret",
  );
  assert.equal(
    viewerRouteFromShareUrl("http://[fd00::25]:8051/s/demo#secret"),
    "/s/demo?server=http%3A%2F%2F%5Bfd00%3A%3A25%5D%3A8051#secret",
  );
});

test("uses the selected server only inside the packaged app", () => {
  globalThis.window = {
    __TAURI_INTERNALS__: {},
    location: {
      href: "http://tauri.localhost/s/demo",
      search: "?server=http%3A%2F%2F192.168.1.25%3A8051",
    },
  };

  assert.equal(
    resolveWebSocketUrl("/api/s/demo"),
    "ws://192.168.1.25:8051/api/s/demo",
  );
});

test("maps secure server links to secure WebSockets", () => {
  globalThis.window = {
    __TAURI_INTERNALS__: {},
    location: {
      href: "http://tauri.localhost/s/demo",
      search: "?server=https%3A%2F%2Fsshxx.example",
    },
  };

  assert.equal(
    resolveWebSocketUrl("/api/s/demo"),
    "wss://sshxx.example/api/s/demo",
  );
});

test("browser sessions ignore cross-origin server overrides", () => {
  globalThis.window = {
    location: {
      href: "https://trusted.example/s/demo?server=http://attacker.test",
      search: "?server=http%3A%2F%2Fattacker.test",
    },
  };

  assert.equal(
    resolveWebSocketUrl("/api/s/demo"),
    "wss://trusted.example/api/s/demo",
  );
});

test("rejects non-HTTP links and malformed session paths", () => {
  assert.throws(
    () => viewerRouteFromShareUrl("ssh://terminal.local/s/demo#secret"),
    /must use http or https/,
  );
  assert.throws(
    () => viewerRouteFromShareUrl("http://terminal.local/demo#secret"),
    /must contain \/s\/<session-id>/,
  );
});

test("identifies upstream sshx public-service links for explicit consent", () => {
  assert.equal(isUpstreamSshxUrl(new URL("https://sshx.io/s/demo")), true);
  assert.equal(isUpstreamSshxUrl(new URL("https://SSHX.IO./s/demo")), true);
  assert.equal(isUpstreamSshxUrl(new URL("https://edge.sshx.io/s/demo")), true);
  assert.equal(
    isUpstreamSshxUrl(new URL("https://sshx.io.example/s/demo")),
    false,
  );
});

test("allows an explicitly selected upstream service in the packaged app", () => {
  assert.equal(
    viewerRouteFromShareUrl("https://sshx.io/s/demo#secret"),
    "/s/demo?server=https%3A%2F%2Fsshx.io#secret",
  );

  globalThis.window = {
    __TAURI_INTERNALS__: {},
    location: {
      href: "http://tauri.localhost/s/demo",
      search: "?server=https%3A%2F%2Fsshx.io",
    },
  };

  assert.equal(resolveWebSocketUrl("/api/s/demo"), "wss://sshx.io/api/s/demo");
});
