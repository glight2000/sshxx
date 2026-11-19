import assert from "node:assert/strict";
import { test } from "node:test";

import { parseOsc7Location } from "../src/lib/terminalLocation.ts";
import { TerminalDirectoryFollow } from "../src/lib/terminalDirectoryFollow.ts";

test("parses an OSC 7 host and decoded working directory", () => {
  assert.deepEqual(
    parseOsc7Location("file://build.example.test/work/release%20candidate"),
    {
      workingDirectory: "/work/release candidate",
      workingDirectoryHost: "build.example.test",
    },
  );
});

test("rejects malformed and non-file OSC locations", () => {
  assert.equal(parseOsc7Location("https://example.test/work"), null);
  assert.equal(parseOsc7Location("not a URI"), null);
  assert.equal(parseOsc7Location("file://example.test/%zz"), null);
});

test("preserves encoded filename punctuation and normalizes the reported host", () => {
  assert.deepEqual(
    parseOsc7Location("file://BUILD.example.test./work/a%23b%3Fc%25"),
    {
      workingDirectory: "/work/a#b?c%",
      workingDirectoryHost: "build.example.test",
    },
  );
});

test("rejects ambiguous, oversized and control-bearing directory hints", () => {
  for (const path of [
    "work?file",
    "dir#fragment",
    "%00",
    "%1B",
    "x".repeat(8193),
  ])
    assert.equal(parseOsc7Location("file://host/" + path), null);
});

function followFixture(t, probe, navigate) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const paths = [],
    messages = [];
  const follow = new TerminalDirectoryFollow({
    probe,
    reveal:
      navigate ??
      (async (path) => {
        paths.push(path);
        return true;
      }),
    status: (message) => messages.push(message),
  });
  t.after(() => follow.dispose());
  return {
    follow,
    paths,
    messages,
    async tick(ms) {
      t.mock.timers.tick(ms);
      await new Promise((resolve) => setImmediate(resolve));
    },
  };
}

const sshReport = (path, host = "server.example") => ({
  workingDirectoryHost: host,
  workingDirectory: path,
});
const sshContext = async () => ({
  operation: "terminalDirectory",
  ok: true,
  directoryHost: "Server.Example.",
  path: "",
});

test("remote follow coalesces live directories, pauses on a different host, and catches up after editing", async (t) => {
  const { follow, paths, messages, tick } = followFixture(t, sshContext);
  follow.update("shell1/page2", false, null);
  await tick(150);
  assert.match(messages.at(-1), /Waiting for the SSH shell/);
  follow.update("shell1/page2", false, sshReport("/one"));
  follow.update("shell1/page2", false, sshReport("/two"));
  await tick(150);
  follow.update("shell1/page2", false, sshReport("/private", "nested.example"));
  await tick(150);
  assert.match(messages.at(-1), /differs/);
  follow.update("shell1/page2", true, sshReport("/three"));
  await tick(10000);
  assert.deepEqual(paths, ["/two"]);
  follow.update("shell1/page2", false, sshReport("/three"));
  await tick(150);
  follow.update("shell1/page2", false, sshReport("/three"));
  await tick(5000);
  assert.deepEqual(paths, ["/two", "/three"]);
});

test("local follow polls the process cwd and ignores SSH reports from a manually nested shell", async (t) => {
  let cwd = "/initial",
    calls = 0;
  const { follow, paths, tick } = followFixture(t, async () => {
    calls++;
    return { ok: true, operation: "terminalDirectory", path: cwd };
  });
  follow.update("local", false, sshReport("/other-host"));
  await tick(150);
  await tick(1500);
  cwd = "/new";
  await tick(1500);
  follow.dispose();
  await tick(10000);
  assert.deepEqual(paths, ["/initial", "/new"]);
  assert.equal(calls, 3);
});

test("an in-flight remote listing cannot publish after a newer report, disconnect, or rebind", async (t) => {
  let resolve;
  const pending = [],
    applied = [];
  const { follow, tick } = followFixture(
    t,
    sshContext,
    async (path, current) => {
      pending.push(path);
      await new Promise((r) => {
        resolve = r;
      });
      if (current()) applied.push(path);
      return current();
    },
  );
  follow.update("one", false, sshReport("/stale"));
  await tick(150);
  follow.update("one", false, sshReport("/new"));
  await tick(300);
  assert.deepEqual(pending, ["/stale"]);
  resolve();
  await tick(0);
  await tick(150);
  follow.update(null, false, null);
  follow.update("two", false, sshReport("/latest"));
  resolve();
  await tick(0);
  await tick(150);
  resolve();
  await tick(0);
  assert.deepEqual(applied, ["/latest"]);
});
