<script lang="ts">
  import { browser } from "$app/environment";
  import { onMount } from "svelte";
  import "@fontsource-variable/inter";

  import "@xterm/xterm/css/xterm.css";
  import "../app.css";

  import ToastContainer from "$lib/ui/ToastContainer.svelte";
  import { resolveColorMode } from "$lib/colorMode";
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
    return () => media.removeEventListener("change", update);
  });
</script>

<ToastContainer />

<slot />
