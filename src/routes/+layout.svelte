<script lang="ts">
  import { browser } from "$app/environment";
  import { onMount } from "svelte";
  import "@fontsource-variable/inter";

  import "@xterm/xterm/css/xterm.css";
  import "../app.css";

  import ToastContainer from "$lib/ui/ToastContainer.svelte";
  import { resolveColorMode } from "$lib/colorMode";
  import { shouldReloadAfterPreloadError } from "$lib/preloadRecovery";
  import { settings } from "$lib/settings";

  let systemPrefersDark = browser
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : true;
  $: resolvedColorMode = resolveColorMode(
    $settings.colorMode,
    systemPrefersDark,
  );
  $: if (browser) {
    document.documentElement.dataset.colorMode = resolvedColorMode;
  }

  onMount(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => (systemPrefersDark = media.matches);
    update();
    media.addEventListener("change", update);
    const recoverPreload = (event: Event) => {
      event.preventDefault();
      if (
        shouldReloadAfterPreloadError(
          window.sessionStorage,
          window.location.href,
        )
      )
        window.location.reload();
    };
    window.addEventListener("vite:preloadError", recoverPreload);
    return () => {
      media.removeEventListener("change", update);
      window.removeEventListener("vite:preloadError", recoverPreload);
    };
  });
</script>

<ToastContainer />

<slot />
