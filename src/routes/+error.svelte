<script lang="ts">
  import { page } from "$app/state";

  import logo from "$lib/assets/logo.svg";
</script>

<main class="p-4 max-w-xl mx-auto my-6 md:my-12 lg:my-24">
  <div class="flex items-center gap-3">
    <img class="h-14" src={logo} alt="sshxx logo" />
    <span class="text-3xl font-semibold tracking-tight text-white">sshxx</span>
  </div>

  <div class="space-y-4 mt-6 mb-8 text-zinc-300">
    <p>
      {#if page.status === 404}
        <b class="text-white">404 Not Found.</b> We couldn't find this page, sorry!
      {:else}
        <b class="text-white">Error {page.status}.</b> An unexpected error occurred.
      {/if}
    </p>
    {#if page.status !== 404}
      <pre class="whitespace-pre-wrap break-all p-3 rounded bg-zinc-900">
{JSON.stringify(page.error, null, 2)}
</pre>
    {/if}
    <p>Please try again later or return to the home page.</p>
  </div>

  <a
    href="/"
    class="inline-block font-medium px-6 py-2 rounded-full bg-indigo-900 hover:bg-indigo-700"
    >Return home</a
  >
</main>
