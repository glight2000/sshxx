<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { SmileIcon } from "svelte-feather-icons";

  export let disabled = false;
  const dispatch = createEventDispatcher<{ open: void; select: string }>();
  const emoji = [
    "😀",
    "😊",
    "😂",
    "🥰",
    "😎",
    "🤔",
    "😭",
    "🥳",
    "👍",
    "👏",
    "🙏",
    "❤️",
    "🎉",
    "🔥",
    "✨",
    "✅",
    "❌",
    "👀",
    "🚀",
    "💡",
    "🐛",
    "📌",
    "☕",
    "💪",
  ];
  const faces = [
    "(＾▽＾)",
    "(≧▽≦)",
    "(๑•̀ㅂ•́)و✧",
    "(づ｡◕‿‿◕｡)づ",
    "(ง •̀_•́)ง",
    "(￣▽￣)ノ",
    "(╥﹏╥)",
    "(・_・;)",
    "¯\\_(ツ)_/¯",
    "( ˘ω˘ )",
  ];
  let open = false;
  let root: HTMLDivElement;
  function outside(event: PointerEvent) {
    if (open && !event.composedPath().includes(root)) open = false;
  }
  function choose(value: string) {
    dispatch("select", value);
    open = false;
  }
</script>

<svelte:window on:pointerdown={outside} />
<!-- Keyboard events belong to the contained buttons; Escape closes their picker. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={root}
  class="expressions"
  on:keydown={(event) => {
    if (event.key === "Escape" && open) {
      event.stopPropagation();
      open = false;
    }
  }}
  role="group"
  aria-label="Emoji and kaomoji"
>
  <button
    type="button"
    class="toggle"
    {disabled}
    aria-expanded={open}
    on:mousedown|preventDefault|stopPropagation
    on:click|stopPropagation={() => {
      if (!open) dispatch("open");
      open = !open;
    }}
  >
    <SmileIcon size="16" /> Emoji / 颜文字
  </button>
  {#if open && !disabled}
    <div class="choices" aria-label="Emoji">
      {#each emoji as value}
        <button
          type="button"
          aria-label={value}
          on:mousedown|preventDefault|stopPropagation
          on:click|stopPropagation={() => choose(value)}>{value}</button
        >
      {/each}
    </div>
    <div class="choices faces" aria-label="Kaomoji">
      {#each faces as value}
        <button
          type="button"
          aria-label={value}
          on:mousedown|preventDefault|stopPropagation
          on:click|stopPropagation={() => choose(value)}>{value}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .expressions {
    color: var(--app-text);
    font-size: 12px;
  }
  button {
    border-radius: 6px;
    padding: 5px 7px;
  }
  button:hover {
    background: var(--surface-hover);
  }
  button:focus-visible {
    outline: 2px solid currentColor;
    outline-offset: -2px;
  }
  button:disabled {
    opacity: 0.4;
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 5px;
    border: 1px solid var(--surface-border);
    border-radius: 6px;
    background: var(--surface-bg);
  }
  .choices button {
    font-size: 21px;
    line-height: 1.4;
  }
  .faces {
    margin-top: 4px;
  }
  .faces button {
    font-size: 12px;
  }
</style>
