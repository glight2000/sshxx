<script lang="ts">
  import "@fontsource-variable/inter";

  import "@xterm/xterm/css/xterm.css";
  import "../app.css";

  import ToastContainer from "$lib/ui/ToastContainer.svelte";
</script>

<ToastContainer />

<slot />
