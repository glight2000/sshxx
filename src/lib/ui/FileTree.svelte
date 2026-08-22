<svelte:options runes={true} />

<script lang="ts">
  import * as tree from "@zag-js/tree-view";
  import { normalizeProps, useMachine } from "@zag-js/svelte";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { ChevronRightIcon, FolderIcon } from "svelte-feather-icons";

  import type { FileTreeEntry } from "$lib/protocol";

  export type FileNode = FileTreeEntry & { children?: FileNode[] };
  type Props = {
    root: FileNode;
    loadDirectory: (path: string) => Promise<FileNode[]>;
    selectDirectory: (node: FileNode) => void;
    openContextMenu: (node: FileNode, event: MouseEvent) => void;
    expandedPaths: string[];
    selectedPath: string;
    scrollTop: number;
    updateExpandedPaths: (paths: string[]) => void;
    updateScrollTop: (scrollTop: number) => void;
    reportError: (message: string) => void;
  };
  let {
    root,
    loadDirectory,
    selectDirectory,
    openContextMenu,
    expandedPaths,
    selectedPath,
    scrollTop,
    updateExpandedPaths,
    updateScrollTop,
    reportError,
  }: Props = $props();

  // svelte-ignore state_referenced_locally -- this component is keyed when its initial tree changes.
  let expandedValue = $state([...expandedPaths]);
  // svelte-ignore state_referenced_locally -- later selectedPath changes are synchronized by the effect below.
  let selectedValue = $state(selectedPath ? [selectedPath] : []);
  let treeElement: HTMLDivElement;
  let applyingSharedScroll = false;
  let scrollTimer: number | undefined;
  let lastRequestedExpansion = "";
  let sharedExpansionTarget: string | null = null;

  function expansionKey(paths: string[]) {
    return [...new Set(paths)].sort().join("\u0000");
  }

  let collection = $state(
    tree.collection<FileNode>({
      // svelte-ignore state_referenced_locally -- FileExplorer keys this component when its root changes.
      rootNode: root,
      nodeToValue: (node) => node.path,
      nodeToString: (node) => node.name,
      nodeToChildren: (node) => node.children ?? [],
      nodeToChildrenCount: (node) =>
        node.kind === "directory"
          ? node.children === undefined
            ? 1
            : node.children.length
          : 0,
    }),
  );

  $effect(() => {
    const next = selectedPath ? [selectedPath] : [];
    if (
      next.length !== selectedValue.length ||
      next.some((value, index) => value !== selectedValue[index])
    ) {
      selectedValue = next;
    }
  });

  function activateNode(node: FileNode) {
    selectedValue = [node.path];
    selectDirectory(node);
  }

  function handleNodePointerDown(event: PointerEvent, node: FileNode) {
    if (event.button !== 0) return;
    event.stopPropagation();
    if (event.currentTarget instanceof HTMLElement) event.currentTarget.focus();
    activateNode(node);
  }

  function handleContextMenu(event: MouseEvent, node: FileNode) {
    event.preventDefault();
    event.stopPropagation();
    activateNode(node);
    openContextMenu(node, event);
  }

  function toggleBranch(
    event: PointerEvent,
    node: FileNode,
    expanded: boolean,
    loading: boolean,
  ) {
    if (event.button !== 0 || loading) return;
    event.preventDefault();
    event.stopPropagation();
    if (expanded) api.collapse([node.path]);
    else api.expand([node.path]);
  }

  onMount(() => {
    void revealInitialSelection();
  });
  onDestroy(() => window.clearTimeout(scrollTimer));

  async function revealInitialSelection() {
    await tick();
    if (scrollTop > 0) {
      applyingSharedScroll = true;
      treeElement.scrollTop = scrollTop;
      requestAnimationFrame(() => (applyingSharedScroll = false));
      return;
    }
    const selected = treeElement.querySelector<HTMLElement>("[data-selected]");
    if (!selected) return;
    const treeBounds = treeElement.getBoundingClientRect();
    const selectedBounds = selected.getBoundingClientRect();
    treeElement.scrollTop +=
      selectedBounds.top -
      treeBounds.top -
      (treeBounds.height - selectedBounds.height) / 2;
  }

  const service = useMachine(tree.machine, () => ({
    id: `file-tree-${root.path}`,
    collection,
    selectionMode: "single" as const,
    expandOnClick: false,
    typeahead: true,
    expandedValue,
    selectedValue,
    onExpandedChange: ({ expandedValue: next }) => {
      expandedValue = next;
      if (sharedExpansionTarget !== null) {
        if (expansionKey(next) === sharedExpansionTarget)
          sharedExpansionTarget = null;
        return;
      }
      updateExpandedPaths(next);
    },
    loadChildren: async ({ node, signal }) => {
      const entries = await loadDirectory(node.path);
      if (signal.aborted) return [];
      return entries;
    },
    onLoadChildrenComplete: ({ collection: next }) => {
      collection = next;
    },
    onLoadChildrenError: ({ nodes }) => {
      const failure = nodes[0];
      reportError(
        failure?.error.message || "Could not load the selected directory.",
      );
    },
    onSelectionChange: ({ selectedValue: next, selectedNodes }) => {
      selectedValue = next;
      const node = selectedNodes[0] as FileNode | undefined;
      if (node) activateNode(node);
    },
  }));
  const api = $derived(tree.connect(service, normalizeProps));

  $effect(() => {
    const requested = [...expandedPaths];
    const requestedKey = expansionKey(requested);
    if (requestedKey === lastRequestedExpansion) return;
    lastRequestedExpansion = requestedKey;

    untrack(() => {
      const current = [...expandedValue];
      const added = requested.filter((path) => !current.includes(path));
      const removed = current.filter((path) => !requested.includes(path));
      if (!added.length && !removed.length) return;

      sharedExpansionTarget = requestedKey;
      if (added.length) api.expand(added);
      if (removed.length) api.collapse(removed);
      queueMicrotask(() => {
        if (sharedExpansionTarget === requestedKey)
          sharedExpansionTarget = null;
      });
    });
  });

  $effect(() => {
    const requested = scrollTop;
    if (!treeElement || Math.abs(treeElement.scrollTop - requested) < 1) return;
    applyingSharedScroll = true;
    treeElement.scrollTop = requested;
    requestAnimationFrame(() => (applyingSharedScroll = false));
  });

  function handleScroll() {
    if (applyingSharedScroll) return;
    window.clearTimeout(scrollTimer);
    scrollTimer = window.setTimeout(
      () => updateScrollTop(Math.max(0, Math.round(treeElement.scrollTop))),
      80,
    );
  }
</script>

<div {...api.getRootProps()} class="h-full min-h-0">
  <div
    bind:this={treeElement}
    {...api.getTreeProps()}
    class="file-tree h-full overflow-y-auto py-1"
    aria-label="Folders"
    onselectstart={(event) => event.preventDefault()}
    onscroll={handleScroll}
  >
    {#each api.getVisibleNodes() as { node, indexPath } (node.path)}
      {@const nodeProps = { node, indexPath }}
      {@const state = api.getNodeState(nodeProps)}
      <div {...api.getBranchProps(nodeProps)}>
        <div
          {...api.getBranchControlProps(nodeProps)}
          class="tree-row relative"
          style:padding-left={`${Math.max(0, state.depth - 1) * 16 + 8}px`}
          onpointerdown={(event) => handleNodePointerDown(event, node)}
          oncontextmenu={(event) => handleContextMenu(event, node)}
          onclick={(event) => event.stopPropagation()}
        >
          <span
            {...api.getBranchIndicatorProps(nodeProps)}
            class="branch-indicator"
            role="button"
            tabindex="-1"
            aria-label={state.expanded
              ? `Collapse ${node.name}`
              : `Expand ${node.name}`}
            onpointerdown={(event) =>
              toggleBranch(event, node, state.expanded, state.loading)}
          >
            <ChevronRightIcon class="h-3.5 w-3.5" />
          </span>
          <FolderIcon class="h-4 w-4 shrink-0 text-amber-300/85" />
          <span {...api.getBranchTextProps(nodeProps)} class="truncate"
            >{node.name}</span
          >
          {#if state.loading}<span class="ml-auto text-[10px] text-zinc-500"
              >Loading…</span
            >{/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<style lang="postcss">
  @reference "../../app.css";
  .tree-row {
    @apply flex h-8 items-center gap-2 rounded px-2 text-sm text-zinc-300 outline-none hover:bg-zinc-800 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-indigo-500/70 data-[selected]:bg-indigo-500/15 data-[selected]:text-indigo-100;
  }
  .branch-indicator {
    @apply inline-flex shrink-0 cursor-pointer rounded p-0.5 text-zinc-500 transition-transform hover:bg-zinc-700 hover:text-zinc-200 data-[state=open]:rotate-90;
  }
  .file-tree {
    -webkit-user-select: none;
    user-select: none;
    scrollbar-width: thin;
    scrollbar-color: rgb(113 113 122 / 65%) transparent;
  }
</style>
