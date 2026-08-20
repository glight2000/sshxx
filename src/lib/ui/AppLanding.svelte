<script lang="ts">
  import { goto } from "$app/navigation";

  import logo from "$lib/assets/logo.svg";
  import { viewerRouteFromShareUrl } from "$lib/runtime";

  let sessionUrl = "";
  let error = "";

  async function connect() {
    error = "";
    try {
      await goto(viewerRouteFromShareUrl(sessionUrl));
    } catch (cause) {
      error = cause instanceof Error ? cause.message : "invalid session link";
    }
  }
</script>

<main
  class="min-h-screen grid place-items-center px-6 text-zinc-100 bg-zinc-950"
>
  <section class="panel w-full max-w-xl p-8 sm:p-10">
    <div class="mb-10 flex items-center gap-3">
      <img class="h-12" src={logo} alt="sshxx logo" />
      <span class="text-3xl font-semibold tracking-tight">sshxx</span>
    </div>

    <h1 class="text-2xl font-medium mb-3">Connect to a terminal</h1>
    <p class="text-zinc-400 mb-8">
      Paste a session link from any sshxx server. The terminal continues running
      on its host when this application is closed.
    </p>

    <form on:submit|preventDefault={connect}>
      <label class="block text-sm text-zinc-300 mb-2" for="session-url">
        Session link
      </label>
      <input
        id="session-url"
        bind:value={sessionUrl}
        type="url"
        required
        spellcheck="false"
        autocomplete="off"
        placeholder="http://192.168.1.25:8051/s/session#key"
        class="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-4 py-3 outline-none focus:border-fuchsia-500"
      />

      <p class="mt-3 text-sm text-zinc-500">
        LAN addresses and unencrypted HTTP links are supported.
      </p>

      {#if error}
        <p class="mt-3 text-sm text-red-400" role="alert">{error}</p>
      {/if}

      <button
        type="submit"
        class="mt-6 bg-pink-700 hover:bg-pink-600 active:ring-4 active:ring-pink-500/50 font-medium px-6 py-2.5 rounded-full"
      >
        Connect
      </button>
    </form>

    <p
      class="mt-8 border-t border-zinc-800 pt-5 text-xs leading-5 text-zinc-500"
    >
      sshxx is derived from
      <a
        class="text-zinc-300 underline hover:text-white"
        href="https://github.com/ekzhang/sshx"
        target="_blank"
        rel="noreferrer">sshx by Eric Zhang</a
      > and remains available under the MIT License.
    </p>
  </section>
</main>
