import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const packageLock = JSON.parse(readFileSync("package-lock.json", "utf8"));
const cargoToml = readFileSync("Cargo.toml", "utf8");
const cargoLock = readFileSync("Cargo.lock", "utf8");
const tauriConfig = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
);
const viteConfig = readFileSync("vite.config.ts", "utf8");

function requiredMatch(source, pattern, label) {
  const match = source.match(pattern);
  assert.ok(match, `Could not find ${label}`);
  return match[1];
}

test("client and workspace release versions stay aligned", () => {
  const releaseVersion = packageJson.version;
  const versions = {
    packageLock: packageLock.packages[""].version,
    workspace: requiredMatch(
      cargoToml,
      /\[workspace\.package\][\s\S]*?version = "([^"]+)"/,
      "Cargo workspace version",
    ),
    tauri: tauriConfig.version,
    vite: requiredMatch(
      viteConfig,
      /__APP_VERSION__:\s*JSON\.stringify\("([^"]+)-"/,
      "Vite client version",
    ),
  };

  for (const [source, version] of Object.entries(versions)) {
    assert.equal(version, releaseVersion, `${source} version drifted`);
  }
});

test("all published Rust modules use the workspace release version", () => {
  const releaseVersion = packageJson.version;
  const moduleNames = [
    "sshxx-client",
    "sshxx-core",
    "sshxx-daemon",
    "sshxx-server",
    "sshxx-terminal-host",
  ];

  for (const moduleName of moduleNames) {
    const escapedName = moduleName.replaceAll("-", "\\-");
    const version = requiredMatch(
      cargoLock,
      new RegExp(`name = "${escapedName}"\\nversion = "([^"]+)"`),
      `${moduleName} lockfile version`,
    );
    assert.equal(version, releaseVersion, `${moduleName} version drifted`);
  }

  for (const dependencyName of ["sshx-core", "sshxx-terminal-host"]) {
    const version = requiredMatch(
      cargoToml,
      new RegExp(`^${dependencyName} = \\{[^\\n]*version = "([^"]+)"`, "m"),
      `${dependencyName} workspace dependency version`,
    );
    assert.equal(version, releaseVersion, `${dependencyName} version drifted`);
  }
});
