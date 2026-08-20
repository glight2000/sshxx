<script lang="ts">
  import { page } from "$app/state";

  import Session from "$lib/Session.svelte";

  let title: string = "Remote Terminal | sshxx";
</script>

<svelte:head>
  <title>{title}</title>

  <style>
    body {
      overscroll-behavior: none;
    }
  </style>
</svelte:head>

<Session
  id={page.params.id ?? ""}
  on:receiveName={({ detail: sessionName }) => {
    if (sessionName) {
      title = `${sessionName} | sshxx`;
    }
  }}
/>
