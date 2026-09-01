import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const releaseJson = JSON.parse(readFileSync("release.json", "utf8"));
const releaseWorkflow = readFileSync(".github/workflows/release.yaml", "utf8");
const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const packageLock = JSON.parse(readFileSync("package-lock.json", "utf8"));
const cargoToml = readFileSync("Cargo.toml", "utf8");
const cargoLock = readFileSync("Cargo.lock", "utf8");
const tauriConfig = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
);
const viteConfig = readFileSync("vite.config.ts", "utf8");

const rustManifests = new Map([
  ["sshxx-client", "src-tauri/Cargo.toml"],
  ["sshxx-core", "crates/sshx-core/Cargo.toml"],
  ["sshxx-daemon", "crates/sshx-daemon/Cargo.toml"],
  ["sshxx-server", "crates/sshx-server/Cargo.toml"],
  ["sshxx-terminal-host", "crates/sshxx-terminal-host/Cargo.toml"],
]);

function requiredMatch(source, pattern, label) {
  const match = source.match(pattern);
  assert.ok(match, `Could not find ${label}`);
  return match[1];
}

function manifestVersion(path) {
  return requiredMatch(
    readFileSync(path, "utf8"),
    /\[package\][\s\S]*?^version = "([^"]+)"/m,
    `${path} package version`,
  );
}

function lockfileVersion(packageName) {
  const escapedName = packageName.replaceAll("-", "\\-");
  return requiredMatch(
    cargoLock,
    new RegExp(`name = "${escapedName}"\\nversion = "([^"]+)"`),
    `${packageName} lockfile version`,
  );
}

test("suite release version is independent and drives the release tag", () => {
  assert.match(releaseJson.version, /^\d+\.\d+\.\d+$/);
  assert.match(releaseWorkflow, /require\("\.\/release\.json"\)\.version/);
  const workspacePackage = requiredMatch(
    cargoToml,
    /\[workspace\.package\]\n([\s\S]*?)(?=\n\[|$)/,
    "workspace package defaults",
  );
  assert.doesNotMatch(
    workspacePackage,
    /^version\s*=/m,
    "suite releases must not impose a shared component version",
  );
});

test("Web and packaged clients share the client component version", () => {
  const clientVersion = packageJson.version;
  const versions = {
    packageLock: packageLock.packages[""].version,
    tauriManifest: manifestVersion("src-tauri/Cargo.toml"),
    tauriConfig: tauriConfig.version,
    vite: requiredMatch(
      viteConfig,
      /__APP_VERSION__:\s*JSON\.stringify\("([^"]+)-"/,
      "Vite client version",
    ),
    cargoLock: lockfileVersion("sshxx-client"),
  };

  for (const [source, version] of Object.entries(versions)) {
    assert.equal(version, clientVersion, `${source} client version drifted`);
  }
});

test("each Rust component owns its package version", () => {
  for (const [packageName, manifest] of rustManifests) {
    assert.equal(
      lockfileVersion(packageName),
      manifestVersion(manifest),
      `${packageName} manifest and lockfile versions drifted`,
    );
  }

  const workspaceDependencies = new Map([
    ["sshx-core", "sshxx-core"],
    ["sshxx-terminal-host", "sshxx-terminal-host"],
  ]);
  for (const [dependencyName, packageName] of workspaceDependencies) {
    const dependencyVersion = requiredMatch(
      cargoToml,
      new RegExp(`^${dependencyName} = \\{[^\\n]*version = "([^"]+)"`, "m"),
      `${dependencyName} workspace dependency version`,
    );
    assert.equal(
      dependencyVersion,
      manifestVersion(rustManifests.get(packageName)),
      `${dependencyName} dependency constraint drifted`,
    );
  }
});
