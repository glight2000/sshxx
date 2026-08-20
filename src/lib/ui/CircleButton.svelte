<script lang="ts">
  import { CopyIcon, MinusIcon, PlusIcon, XIcon } from "svelte-feather-icons";

  export let kind: keyof typeof details;
  export let disabled = false;
  export let ariaLabel: string;

  const details = {
    red: {
      cls: "border-rose-300/50 bg-rose-400 text-rose-950 group-hover:bg-rose-300 group-active:bg-rose-500",
      icon: XIcon,
    },
    yellow: {
      cls: "border-amber-300/50 bg-amber-400 text-amber-950 group-hover:bg-amber-300 group-active:bg-amber-500",
      icon: MinusIcon,
    },
    green: {
      cls: "border-emerald-300/50 bg-emerald-400 text-emerald-950 group-hover:bg-emerald-300 group-active:bg-emerald-500",
      icon: PlusIcon,
    },
    blue: {
      cls: "border-sky-300/50 bg-sky-400 text-sky-950 group-hover:bg-sky-300 group-active:bg-sky-500",
      icon: CopyIcon,
    },
  };
</script>

<button
  class="group inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md p-0 disabled:opacity-40"
  {disabled}
  aria-label={ariaLabel}
  on:mousedown|stopPropagation
  on:click
>
  <span
    class="inline-flex h-3.5 w-3.5 items-center justify-center rounded-full border shadow-sm shadow-black/30 transition-colors {details[
      kind
    ].cls}"
  >
    <svelte:component
      this={details[kind].icon}
      class="block h-2.5 w-2.5"
      strokeWidth={2.5}
    />
  </span>
</button>
