const PRELOAD_RELOAD_KEY = "sshxx-preload-reload";
const RELOAD_COOLDOWN_MS = 30_000;

type StorageLike = Pick<Storage, "getItem" | "setItem">;

/** Prevent a missing deployment chunk from trapping the app in a reload loop. */
export function shouldReloadAfterPreloadError(
  storage: StorageLike,
  url: string,
  now = Date.now(),
) {
  try {
    const previous = JSON.parse(storage.getItem(PRELOAD_RELOAD_KEY) || "null");
    if (
      previous?.url === url &&
      Number.isFinite(previous?.time) &&
      now - previous.time < RELOAD_COOLDOWN_MS
    )
      return false;
    storage.setItem(PRELOAD_RELOAD_KEY, JSON.stringify({ url, time: now }));
  } catch {
    // A blocked sessionStorage must not prevent recovery.
  }
  return true;
}
