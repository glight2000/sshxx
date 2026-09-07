import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

test("terminal initialization guards delayed work and releases owned resources", () => {
  const source = readFileSync(
    new URL("../src/lib/ui/XTerm.svelte", import.meta.url),
    "utf8",
  );
  const initialize = source.slice(
    source.indexOf("async function initializeTerminal()"),
    source.indexOf("onMount(() =>"),
  );
  assert.ok(
    initialize.indexOf("await waitForFonts()") <
      initialize.indexOf("!termEl?.isConnected"),
  );
  assert.ok(
    initialize.indexOf("!termEl?.isConnected") <
      initialize.indexOf("new Terminal("),
  );
  assert.match(initialize, /if \(destroyed \|\| initializationError/);
  assert.match(
    source,
    /onDestroy\(\(\) => \{\s*destroyed = true;\s*writeQueue.dispose\(\)/,
  );
  assert.match(source, /focusObserver\?\.disconnect\(\)/);
  assert.match(source, /clearTimeout\(initializationTimer\)/);
  assert.match(source, /clearTimeout\(diagnosticTimer\)/);
  assert.match(source, /Date.now\(\) - state.pendingSince >= 2000/);
  assert.match(source, /Retry\s*<\/button\s*>/);
  assert.match(source, /dispatch\("retryInitialization"\)/);
  const session = readFileSync(
    new URL("../src/lib/Session.svelte", import.meta.url),
    "utf8",
  );
  assert.match(session, /if \(!preserveHistory\) terminalHistory.delete\(id\)/);
});
