import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

test("Electron sandbox preload exposes only the runtime marker through CommonJS", () => {
  const root = new URL("../clients/electron/", import.meta.url);
  const manifest = JSON.parse(
    readFileSync(new URL("package.json", root), "utf8"),
  );
  assert.ok(manifest.build.files.includes("preload.cjs"));
  assert.match(
    readFileSync(new URL("main.mjs", root), "utf8"),
    /"preload\.cjs"/,
  );
  const exposed = {};
  runInNewContext(readFileSync(new URL("preload.cjs", root), "utf8"), {
    require(name) {
      assert.equal(name, "electron");
      return {
        contextBridge: {
          exposeInMainWorld(key, value) {
            exposed[key] = value;
          },
        },
      };
    },
  });
  assert.equal(exposed.sshxxDesktop.runtime, "electron");
  assert.deepEqual(Object.keys(exposed.sshxxDesktop), ["runtime"]);
  assert.ok(Object.isFrozen(exposed.sshxxDesktop));
});
