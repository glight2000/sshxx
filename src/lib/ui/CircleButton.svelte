<script lang="ts">
  import { CopyIcon, MinusIcon, PlusIcon, XIcon } from "svelte-feather-icons";

  export let kind: keyof typeof details;
  export let disabled = false;
  export let ariaLabel: string;

  const details = {
    red: {
      cls: "bg-red-500 active:bg-red-700",
      icon: XIcon,
    },
    yellow: {
      cls: "bg-yellow-500 active:bg-yellow-700",
      icon: MinusIcon,
    },
    green: {
      cls: "bg-green-500 active:bg-green-700",
      icon: PlusIcon,
    },
    blue: {
      cls: "bg-blue-500 active:bg-blue-700",
      icon: CopyIcon,
    },
  };
</script>

<button
  class="inline-flex h-3 w-3 shrink-0 items-center justify-center rounded-full p-[1px] {details[
    kind
  ].cls}"
  {disabled}
  aria-label={ariaLabel}
  on:mousedown|stopPropagation
  on:click
>
  <svelte:component
    this={details[kind].icon}
    class="w-full h-full"
    strokeWidth={3}
  />
</button>
