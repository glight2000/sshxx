import assert from "node:assert/strict";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import test from "node:test";

const source = readFileSync("scripts/service.sh", "utf8");

function executable(path, content) {
  writeFileSync(path, `#!/bin/sh\nset -eu\n${content}\n`);
  chmodSync(path, 0o755);
}

function fixture({ hostStopFails = false, platform = "Linux" } = {}) {
  const root = mkdtempSync(join(tmpdir(), "sshxx-service-test-"));
  const installRoot = join(root, "runtime");
  const versionRoot = join(installRoot, "versions", "test");
  const bin = join(installRoot, "bin");
  const fakeBin = join(root, "fake-bin");
  const home = join(root, "home");
  const workspace = join(root, "workspace");
  mkdirSync(join(versionRoot, "scripts"), { recursive: true });
  mkdirSync(bin, { recursive: true });
  mkdirSync(fakeBin, { recursive: true });
  mkdirSync(home, { recursive: true });
  mkdirSync(workspace, { recursive: true });
  writeFileSync(join(installRoot, "current-version"), "test\n");
  writeFileSync(join(versionRoot, "scripts", "service.sh"), source);
  chmodSync(join(versionRoot, "scripts", "service.sh"), 0o755);

  executable(join(bin, "sshxx-server"), "exit 0");
  executable(join(bin, "sshxx-daemon"), "exit 0");
  executable(
    join(bin, "sshxx-terminal-host"),
    `if [ "\${1:-}" = stop ] && [ "${hostStopFails}" = true ]; then exit 1; fi\nexit 0`,
  );
  symlinkSync(
    join(versionRoot, "scripts", "service.sh"),
    join(bin, "sshxx-service"),
  );
  executable(
    join(fakeBin, "systemctl"),
    `printf '%s\\n' "$*" >> "${join(root, "systemctl.log")}"`,
  );
  executable(join(fakeBin, "uname"), `printf '%s\\n' "${platform}"`);
  executable(
    join(fakeBin, "launchctl"),
    `printf '%s\\n' "$*" >> "${join(root, "launchctl.log")}"\nif [ "\${1:-}" = print ]; then exit 1; fi`,
  );
  executable(join(fakeBin, "curl"), "exit 0");

  return {
    root,
    installRoot,
    home,
    workspace,
    command: join(versionRoot, "scripts", "service.sh"),
    env: {
      ...process.env,
      HOME: home,
      XDG_CONFIG_HOME: join(home, ".config"),
      PATH: `${fakeBin}:${process.env.PATH}`,
      SSHXX_INSTALL_ROOT: installRoot,
      SSHXX_BIN_DIR: bin,
    },
  };
}

function run(fixtureValue, args) {
  return spawnSync(fixtureValue.command, args, {
    encoding: "utf8",
    env: fixtureValue.env,
  });
}

test("Linux managed install emits three independent user units", () => {
  const value = fixture();
  try {
    const result = run(value, [
      "install",
      "--workspace",
      value.workspace,
      "--scope",
      "user",
    ]);
    assert.equal(result.status, 0, result.stderr);
    const unitRoot = join(value.home, ".config", "systemd", "user");
    const daemon = readFileSync(join(unitRoot, "sshxx-daemon.service"), "utf8");
    const host = readFileSync(
      join(unitRoot, "sshxx-terminal-host.service"),
      "utf8",
    );
    assert.match(daemon, /WorkingDirectory="[^"]+\/workspace"/);
    assert.match(daemon, /Wants=.*sshxx-terminal-host\.service/);
    assert.match(host, /sshxx-terminal-host" serve/);
    assert.doesNotMatch(host, /PartOf=sshxx-daemon/);
    const calls = readFileSync(join(value.root, "systemctl.log"), "utf8");
    assert.match(calls, /restart sshxx-server\.service/);
    assert.match(calls, /start sshxx-terminal-host\.service/);
    assert.doesNotMatch(calls, /restart[^\n]*terminal-host/);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("uninstall refuses to remove a Runtime with active terminals", () => {
  const value = fixture({ hostStopFails: true });
  try {
    assert.equal(
      run(value, ["install", "--workspace", value.workspace]).status,
      0,
    );
    const result = run(value, ["uninstall"]);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /active terminals/);
    assert.equal(existsSync(value.installRoot), true);
    assert.equal(existsSync(value.workspace), true);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("macOS managed update renders separate jobs without unloading terminal-host", () => {
  const value = fixture({ platform: "Darwin" });
  try {
    const result = run(value, [
      "install",
      "--workspace",
      value.workspace,
      "--scope",
      "user",
    ]);
    assert.equal(result.status, 0, result.stderr);
    const agentRoot = join(value.home, "Library", "LaunchAgents");
    const daemon = readFileSync(
      join(agentRoot, "io.sshxx.daemon.plist"),
      "utf8",
    );
    const host = readFileSync(
      join(agentRoot, "io.sshxx.terminal-host.plist"),
      "utf8",
    );
    assert.match(daemon, /<string>--server<\/string>/);
    assert.match(host, /<string>serve<\/string>/);
    const calls = readFileSync(join(value.root, "launchctl.log"), "utf8");
    assert.match(calls, /bootout gui\/\d+\/io\.sshxx\.daemon/);
    assert.doesNotMatch(calls, /bootout gui\/\d+\/io\.sshxx\.terminal-host/);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});

test("normal uninstall removes services and Runtime but preserves workspace", () => {
  const value = fixture();
  try {
    assert.equal(
      run(value, ["install", "--workspace", value.workspace]).status,
      0,
    );
    const result = run(value, ["uninstall"]);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(existsSync(value.installRoot), false);
    assert.equal(existsSync(value.workspace), true);
  } finally {
    rmSync(value.root, { recursive: true, force: true });
  }
});
