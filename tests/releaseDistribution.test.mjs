import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const releaseWorkflow = readFileSync(".github/workflows/release.yaml", "utf8");
const unixInstaller = readFileSync("scripts/install.sh", "utf8");
const windowsInstaller = readFileSync("scripts/install.ps1", "utf8");
const unixServiceManager = readFileSync("scripts/service.sh", "utf8");
const windowsServiceManager = readFileSync("scripts/service.ps1", "utf8");

const runtimeTargets = [
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-pc-windows-msvc",
];

const runtimeComponents = [
  "sshxx-daemon",
  "sshxx-server",
  "sshxx-terminal-host",
];

test("release workflow builds complete runtime archives for supported targets", () => {
  assert.match(
    releaseWorkflow,
    /uses: arduino\/setup-protoc@v3\n {8}with:\n {10}repo-token:/,
  );
  for (const target of runtimeTargets) {
    assert.match(releaseWorkflow, new RegExp(`target: ${target}`));
  }
  for (const component of runtimeComponents) {
    assert.match(releaseWorkflow, new RegExp(`-p ${component}`));
    assert.match(releaseWorkflow, new RegExp(`release/${component}`));
  }
  assert.match(releaseWorkflow, /cp -R build/);
  assert.match(releaseWorkflow, /Copy-Item -Recurse "build"/);
  assert.match(releaseWorkflow, /scripts\/install\.sh scripts\/service\.sh/);
  assert.match(
    releaseWorkflow,
    /"scripts\/install\.ps1", "scripts\/service\.ps1"/,
  );
});

test("release stays draft until checksums and attestations are complete", () => {
  assert.match(releaseWorkflow, /uses: \.\/\.github\/workflows\/ci\.yaml/);
  assert.match(releaseWorkflow, /prepare:\n(?:.*\n){0,3} {4}needs: quality/);
  assert.match(releaseWorkflow, /--draft/);
  assert.match(releaseWorkflow, /needs:\n {6}- runtime\n {6}- desktop/);
  assert.match(releaseWorkflow, /sha256sum > \.\.\/SHA256SUMS/);
  assert.match(releaseWorkflow, /uses: actions\/attest@v4/);
  assert.match(releaseWorkflow, /--draft=false/);
  assert.doesNotMatch(releaseWorkflow, /matrix\.args#/);
});

test("installers verify checksums and require every runtime component", () => {
  for (const installer of [unixInstaller, windowsInstaller]) {
    assert.match(installer, /SHA256SUMS/);
    assert.match(installer, /SHA256/);
    for (const component of runtimeComponents) {
      assert.match(installer, new RegExp(component));
    }
  }
  assert.match(unixInstaller, /while \[ -L "\$SCRIPT_PATH" \]/);
  assert.match(unixInstaller, /ln -sfn sshxx-launcher/);
  assert.match(unixInstaller, /sshxx-service/);
  assert.match(windowsInstaller, /sshxx-service\.cmd/);
});

test("managed installers preserve the independent terminal-host lifecycle", () => {
  assert.match(unixServiceManager, /systemctl --user/);
  assert.match(unixServiceManager, /Library\/LaunchAgents/);
  assert.match(unixServiceManager, /Library\/LaunchDaemons/);
  assert.match(unixServiceManager, /terminal-host remains running/);
  assert.match(unixServiceManager, /terminal-host was not restarted/);
  assert.doesNotMatch(
    unixServiceManager,
    /linux_control restart[^\n]*terminal-host/,
  );

  assert.match(windowsServiceManager, /Register-ScheduledTask/);
  assert.match(windowsServiceManager, /terminal-host remains running/);
  assert.match(windowsServiceManager, /-PurgeData/);
  assert.match(windowsServiceManager, /Unregister-ScheduledTask/);
  assert.match(
    windowsServiceManager,
    /"daemon" \{\s+Wait-Host \$Configuration\s+Wait-Web \$Configuration/,
  );
});
