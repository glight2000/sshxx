<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import {
    Edit2Icon,
    FilePlusIcon,
    FileIcon,
    FolderIcon,
    FolderPlusIcon,
    MoveIcon,
    RefreshCwIcon,
    SaveIcon,
    SettingsIcon,
    TerminalIcon,
    Trash2Icon,
    UploadCloudIcon,
  } from "svelte-feather-icons";

  import type {
    FileOperationRequest,
    FileOperationResponse,
    FileTreeEntry,
  } from "$lib/protocol";
  import { makeToast } from "$lib/toast";
  import CanvasRelations, {
    type CanvasRelationItem,
  } from "./CanvasRelations.svelte";
  import CodeEditor, {
    type TextInsertPosition,
    type TextInsertResult,
  } from "./CodeEditor.svelte";
  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";
  import FileEntryDialog from "./FileEntryDialog.svelte";
  import FileMoveDialog from "./FileMoveDialog.svelte";
  import FileTree, { type FileNode } from "./FileTree.svelte";
  import FileUploadDialog, { type UploadItem } from "./FileUploadDialog.svelte";

  export let title: string;
  export let initialPath: string;
  export let currentPath: string;
  export let expandedPaths: string[];
  export let selectedPath: string;
  export let selectedKind: "" | FileTreeEntry["kind"];
  export let treeScrollTop: number;
  export let editorPath: string;
  export let sharedEditorContent: string | null;
  export let sharedEditorDirty: boolean;
  export let width: number;
  export let height: number;
  export let sidebarWidth: number;
  export let treeRevision: number;
  export let hasWriteAccess: boolean | undefined;
  export let linkedNotes: CanvasRelationItem[] = [];
  export let linkedHighlight = false;
  export let paragraphDropState: "none" | "ready" | "blocked" = "none";
  export let fullscreen = false;
  export let insertText: (
    text: string,
    position?: TextInsertPosition,
  ) => TextInsertResult;
  export let previewTextDrop: (position: TextInsertPosition) => boolean;
  export let cancelTextDropPreview: () => void;
  export let request: (
    request: FileOperationRequest,
  ) => Promise<FileOperationResponse>;
  export let updateSharedState: (update: {
    currentPath?: string;
    expandedPaths?: string[];
    selectedPath?: string;
    selectedKind?: "" | FileTreeEntry["kind"];
    treeScrollTop?: number;
    editorPath?: string;
    editorContent?: string;
    editorDirty?: boolean;
    sidebarWidth?: number;
    treeRevision?: number;
  }) => void;

  const dispatch = createEventDispatcher<{
    close: void;
    toggleFullscreen: void;
    bringToFront: void;
    startMove: MouseEvent;
    openTerminal: string;
    focus: void;
    blur: void;
    navigateNote: CanvasRelationItem;
    unlinkNote: CanvasRelationItem;
  }>();
  let sectionElement: HTMLElement;
  let pathInput: HTMLInputElement;
  let settingsButton: HTMLButtonElement;
  let settingsPanel: HTMLDivElement;
  let contextMenuElement: HTMLDivElement;
  let loading = false;
  let error = "";
  let root: FileNode | null = null;
  let treeVersion = 0;
  let selected: FileTreeEntry | null = null;
  let directoryEntries: FileTreeEntry[] = [];
  let content = "";
  let originalContent = "";
  let encoding: "utf8" | "base64" | null = null;
  let previewUrl = "";
  let handledDirectoryPath = "";
  let handledEditorPath = "";
  let editorSyncTimer: number | undefined;
  let previewLoadVersion = 0;
  let observedTreeRevision = treeRevision;
  let sidebarWidthValue = clampSidebarWidth(sidebarWidth);
  let resizingSidebar = false;
  let pathEditing = false;
  let pathDraft = "";
  let pathEditOriginal = "";
  let pathPreviewPath = "";
  let pathPreviewValid = false;
  let previewExpandedPaths: string[] = [];
  let pathEditTimer: number | undefined;
  let pathPreviewVersion = 0;
  let uploadOpen = false;
  let uploadTarget = "";
  let createKind: "file" | "directory" | null = null;
  let entryDestination = "";
  let renameTarget: FileTreeEntry | null = null;
  let moveTarget: FileTreeEntry | null = null;
  let settingsOpen = false;
  let contextMenu: {
    entry: FileTreeEntry;
    source: "tree" | "grid" | "background";
    x: number;
    y: number;
  } | null = null;
  let mutationBusy = false;
  let editorInsertText: (
    text: string,
    position?: TextInsertPosition,
  ) => TextInsertResult = () => ({
    ok: false,
    message: "Open a text file before inserting text.",
  });
  let editorPreviewTextDrop: (position: TextInsertPosition) => boolean = () =>
    false;
  let editorCancelTextDropPreview: () => void = () => {};

  $: dirty = encoding === "utf8" && content !== originalContent;
  $: previewKind = selected ? previewType(selected.name) : "none";
  $: displayedExpandedPaths = pathEditing
    ? previewExpandedPaths
    : expandedPaths;
  $: rightSelection = findDirectoryEntry(selectedPath, selectedKind);
  $: currentDirectory = directoryEntry(currentPath);
  $: actionTarget = rightSelection ?? currentDirectory;
  $: selectedDestination =
    actionTarget?.kind === "directory"
      ? actionTarget.path
      : actionTarget
        ? parentPath(actionTarget.path)
        : currentPath;
  $: uploadDestination = uploadTarget || selectedDestination;
  $: if (!resizingSidebar)
    sidebarWidthValue = clampSidebarWidth(sidebarWidth, width);
  $: if (treeRevision !== observedTreeRevision) {
    observedTreeRevision = treeRevision;
    void loadRoot();
  }

  function unavailableEditorMessage() {
    if (!selected) return "No file is open in this file editor.";
    if (encoding !== "utf8")
      return `“${selected.name}” is not an editable text file.`;
    if (!hasWriteAccess) return "The file editor is read-only.";
    return "The text editor is not ready.";
  }

  function insertIntoOpenEditor(
    text: string,
    position?: TextInsertPosition,
  ): TextInsertResult {
    if (!selected || encoding !== "utf8" || !hasWriteAccess)
      return { ok: false, message: unavailableEditorMessage() };
    return editorInsertText(text, position);
  }

  function previewOpenEditorDrop(position: TextInsertPosition) {
    return Boolean(
      selected &&
      encoding === "utf8" &&
      hasWriteAccess &&
      editorPreviewTextDrop(position),
    );
  }

  function cancelOpenEditorDropPreview() {
    editorCancelTextDropPreview();
  }

  function clearPreviewUrl() {
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    previewUrl = "";
  }
  onDestroy(() => {
    clearPreviewUrl();
    window.clearTimeout(editorSyncTimer);
    window.clearTimeout(pathEditTimer);
    stopSidebarResize();
    insertText = () => ({
      ok: false,
      message: "The file editor is closed.",
    });
    previewTextDrop = () => false;
    cancelTextDropPreview = () => {};
  });
  onMount(() => {
    insertText = insertIntoOpenEditor;
    previewTextDrop = previewOpenEditorDrop;
    cancelTextDropPreview = cancelOpenEditorDropPreview;
    void loadRoot();
  });

  function closeSettingsOnOutsideClick(event: MouseEvent) {
    if (!(event.target instanceof Node)) return;
    if (
      settingsOpen &&
      !settingsButton?.contains(event.target) &&
      !settingsPanel?.contains(event.target)
    ) {
      settingsOpen = false;
    }
    if (contextMenu && !contextMenuElement?.contains(event.target)) {
      contextMenu = null;
    }
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") contextMenu = null;
  }

  // Both synchronizers write the locally mirrored prop after resolving a
  // daemon path. Their handled-path guards make the reactive feedback finite.
  /* eslint-disable svelte/infinite-reactive-loop */
  $: synchronizeDirectory(currentPath);
  $: synchronizeOpenedFile(editorPath);
  $: applySharedEditor(editorPath, sharedEditorContent, sharedEditorDirty);

  async function buildTree(
    targetPath: string,
    requestedExpandedPaths: string[],
  ) {
    const initial = await listDirectory(targetPath || ".");
    const rootPath = filesystemRoot(initial.path);
    const chain = ancestorPaths(rootPath, initial.path);
    const rootListing = samePath(rootPath, initial.path)
      ? initial
      : await listDirectory(rootPath);
    const filesystem: FileNode = directoryNode(
      rootListing.path,
      rootListing.entries,
    );
    const listings = new Map<string, Awaited<ReturnType<typeof listDirectory>>>(
      [
        [normalizedPath(initial.path), initial],
        [normalizedPath(rootListing.path), rootListing],
      ],
    );
    const requestedPaths = Array.from(
      new Set([
        ...chain,
        ...requestedExpandedPaths.filter((path) =>
          isPathInside(rootPath, path),
        ),
      ]),
    ).sort((left, right) => pathDepth(left) - pathDepth(right));
    const requiredPaths = new Set(chain.map(normalizedPath));
    const unavailableExpandedPaths: string[] = [];

    for (const requestedPath of requestedPaths) {
      let parent = filesystem;
      for (const ancestor of ancestorPaths(rootPath, requestedPath).slice(1)) {
        let child = parent.children?.find(
          (node) => node.kind === "directory" && samePath(node.path, ancestor),
        );
        if (!child) {
          child = directoryNode(ancestor, []);
          parent.children = [...(parent.children ?? []), child];
        }
        let listing = listings.get(normalizedPath(ancestor));
        if (!listing) {
          try {
            listing = await listDirectory(ancestor);
          } catch (cause) {
            if (requiredPaths.has(normalizedPath(ancestor))) throw cause;
            unavailableExpandedPaths.push(requestedPath);
            break;
          }
          listings.set(normalizedPath(listing.path), listing);
        }
        child.path = listing.path;
        child.children = listing.entries
          .filter((entry) => entry.kind === "directory")
          .map(toNode);
        parent = child;
      }
    }

    return {
      root: {
        name: "Filesystem",
        path: "sshxx://filesystem-root",
        kind: "directory" as const,
        size: 0,
        children: [filesystem],
      },
      resolvedPath: initial.path,
      chain,
      unavailableExpandedPaths,
    };
  }

  async function loadRoot() {
    loading = true;
    error = "";
    clearPreviewUrl();
    try {
      const built = await buildTree(currentPath || ".", expandedPaths);
      root = built.root;
      if (!samePath(currentPath, built.resolvedPath)) {
        currentPath = built.resolvedPath;
        updateSharedState({ currentPath });
      }
      if (expandedPaths.length === 0) {
        expandedPaths = built.chain;
        updateSharedState({ expandedPaths });
      } else if (built.unavailableExpandedPaths.length) {
        expandedPaths = expandedPaths.filter(
          (path) =>
            !built.unavailableExpandedPaths.some(
              (unavailable) =>
                samePath(path, unavailable) || isPathInside(unavailable, path),
            ),
        );
        updateSharedState({ expandedPaths });
      }
      treeVersion += 1;
      handledDirectoryPath = "";
      const directory: FileTreeEntry = {
        name: pathName(currentPath),
        path: currentPath,
        kind: "directory",
        size: 0,
      };
      const openedEditorPath = editorPath;
      const openedEditorKind = selectedKind;
      await selectDirectory(directory, false, false);
      if (openedEditorPath) {
        await selectFile(
          {
            name: pathName(openedEditorPath),
            path: openedEditorPath,
            kind:
              openedEditorKind === "file" || openedEditorKind === "symlink"
                ? openedEditorKind
                : "file",
            size: 0,
          },
          false,
        );
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  async function beginPathEdit() {
    pathEditOriginal = currentPath;
    pathDraft = currentPath;
    pathPreviewPath = currentPath;
    pathPreviewValid = true;
    previewExpandedPaths = [...expandedPaths];
    pathEditing = true;
    await tick();
    pathInput?.focus();
    pathInput?.select();
  }

  function schedulePathPreview() {
    window.clearTimeout(pathEditTimer);
    const version = ++pathPreviewVersion;
    const target = pathDraft.trim();
    if (!target) {
      root = null;
      pathPreviewValid = false;
      return;
    }
    pathEditTimer = window.setTimeout(async () => {
      try {
        const built = await buildTree(target, []);
        if (version !== pathPreviewVersion || !pathEditing) return;
        root = built.root;
        pathPreviewPath = built.resolvedPath;
        pathPreviewValid = true;
        previewExpandedPaths = built.chain;
        treeVersion += 1;
      } catch {
        if (version !== pathPreviewVersion || !pathEditing) return;
        root = null;
        pathPreviewPath = "";
        pathPreviewValid = false;
      }
    }, 220);
  }

  function commitPathEdit() {
    if (!pathEditing || !pathPreviewValid || !pathPreviewPath) return;
    pathEditing = false;
    currentPath = pathPreviewPath;
    expandedPaths = previewExpandedPaths;
    selectedPath = "";
    selectedKind = "";
    selected = null;
    encoding = null;
    content = "";
    originalContent = "";
    clearPreviewUrl();
    updateSharedState({
      currentPath,
      expandedPaths,
      selectedPath,
      selectedKind,
      treeScrollTop: 0,
      editorPath: "",
      editorContent: "",
      editorDirty: false,
    });
  }

  function cancelPathEdit() {
    if (!pathEditing) return;
    pathEditing = false;
    pathDraft = pathEditOriginal;
    pathPreviewVersion += 1;
    void loadRoot();
  }

  const toNode = (entry: FileTreeEntry): FileNode => ({ ...entry });

  async function listDirectory(path: string) {
    const response = await request({ operation: "list", path });
    if (!response.ok)
      throw new Error(response.error || "Could not open directory.");
    return {
      path: response.path,
      entries: response.entries ?? [],
    };
  }

  function directoryNode(path: string, entries: FileTreeEntry[]): FileNode {
    return {
      name: pathName(path),
      path,
      kind: "directory",
      size: 0,
      children: entries
        .filter((entry) => entry.kind === "directory")
        .map(toNode),
    };
  }

  function pathSeparator(path: string) {
    return path.includes("\\") ? "\\" : "/";
  }

  function trimTrailingSeparators(path: string) {
    if (path === "/" || /^[A-Za-z]:[\\/]$/.test(path)) return path;
    return path.replace(/[\\/]+$/, "");
  }

  function normalizedPath(path: string) {
    const normalized = trimTrailingSeparators(path).replace(/\\/g, "/");
    return /^[A-Za-z]:\//.test(normalized)
      ? normalized.toLowerCase()
      : normalized;
  }

  function samePath(left: string, right: string) {
    return normalizedPath(left) === normalizedPath(right);
  }

  function filesystemRoot(path: string) {
    const extendedDrive = path.match(/^\\\\\?\\([A-Za-z]:)\\/);
    if (extendedDrive) return `\\\\?\\${extendedDrive[1]}\\`;
    const drive = path.match(/^([A-Za-z]:)[\\/]/);
    if (drive) return `${drive[1]}${pathSeparator(path)}`;
    const unc = path.match(/^(\\\\[^\\]+\\[^\\]+)[\\]?/);
    if (unc) return `${unc[1]}\\`;
    return "/";
  }

  function ancestorPaths(rootPath: string, targetPath: string) {
    const separator = pathSeparator(targetPath);
    const remainder = targetPath
      .slice(rootPath.length)
      .split(/[\\/]+/)
      .filter(Boolean);
    const paths = [rootPath];
    let cursor = rootPath;
    for (const part of remainder) {
      cursor = `${cursor.replace(/[\\/]+$/, "")}${separator}${part}`;
      paths.push(cursor);
    }
    return paths;
  }

  function isPathInside(rootPath: string, path: string) {
    const root = normalizedPath(rootPath);
    const candidate = normalizedPath(path);
    return (
      candidate === root ||
      candidate.startsWith(root === "/" ? root : `${root}/`)
    );
  }

  function pathDepth(path: string) {
    return normalizedPath(path).split("/").filter(Boolean).length;
  }

  function pathName(path: string) {
    return path.split(/[\\/]/).filter(Boolean).at(-1) || path;
  }

  function findNode(node: FileNode, path: string): FileNode | null {
    if (samePath(node.path, path)) return node;
    for (const child of node.children ?? []) {
      const found = findNode(child, path);
      if (found) return found;
    }
    return null;
  }

  async function loadDirectory(path: string) {
    const response = await listDirectory(path);
    const children = response.entries
      .filter((entry) => entry.kind === "directory")
      .map(toNode);
    const directory = root ? findNode(root, path) : null;
    if (directory) directory.children = children;
    return children;
  }

  async function selectDirectory(
    directory: FileTreeEntry,
    publish = true,
    clearRightSelection = true,
  ) {
    const loadVersion = ++previewLoadVersion;
    handledDirectoryPath = normalizedPath(directory.path);
    handledEditorPath = "";
    currentPath = directory.path;
    editorPath = "";
    if (clearRightSelection) {
      selectedPath = "";
      selectedKind = "";
    }
    selected = directory;
    directoryEntries = [];
    encoding = null;
    content = "";
    originalContent = "";
    loading = true;
    error = "";
    clearPreviewUrl();
    if (publish) {
      editorPath = "";
      updateSharedState({
        currentPath,
        ...(clearRightSelection
          ? { selectedPath: "", selectedKind: "" as const }
          : {}),
        editorPath: "",
        editorContent: "",
        editorDirty: false,
      });
    }
    try {
      const listing = await listDirectory(directory.path);
      if (loadVersion !== previewLoadVersion) return;
      selected = { ...directory, path: listing.path };
      currentPath = listing.path;
      directoryEntries = listing.entries;
      const node = root ? findNode(root, directory.path) : null;
      if (node)
        node.children = listing.entries
          .filter((entry) => entry.kind === "directory")
          .map(toNode);
      if (publish && !samePath(directory.path, listing.path)) {
        updateSharedState({
          currentPath: listing.path,
        });
      }
    } catch (cause) {
      if (loadVersion !== previewLoadVersion) return;
      error = cause instanceof Error ? cause.message : String(cause);
      if (publish) {
        handledEditorPath = "";
        editorPath = "";
        updateSharedState({
          editorPath: "",
          editorContent: "",
          editorDirty: false,
        });
      }
    } finally {
      if (loadVersion === previewLoadVersion) loading = false;
    }
  }

  async function selectFile(file: FileTreeEntry, publish = true) {
    const loadVersion = ++previewLoadVersion;
    handledEditorPath = normalizedPath(file.path);
    selected = file;
    loading = true;
    error = "";
    clearPreviewUrl();
    if (publish) {
      updateSharedState({
        selectedPath: file.path,
        selectedKind: file.kind,
      });
    }
    try {
      const response = await request({ operation: "read", path: file.path });
      if (!response.ok)
        throw new Error(response.error || "Could not open file.");
      if (loadVersion !== previewLoadVersion) return;
      encoding = response.encoding ?? null;
      content = response.content ?? "";
      originalContent = content;
      if (publish) {
        editorPath = response.path;
        updateSharedState({
          editorPath: response.path,
          editorContent: encoding === "utf8" ? content : "",
          editorDirty: false,
        });
      }
      if (encoding === "base64" && previewType(file.name) !== "binary") {
        const bytes = Uint8Array.from(atob(content), (character) =>
          character.charCodeAt(0),
        );
        previewUrl = URL.createObjectURL(
          new Blob([bytes], { type: mimeType(file.name) }),
        );
      }
    } catch (cause) {
      if (loadVersion !== previewLoadVersion) return;
      error = cause instanceof Error ? cause.message : String(cause);
      if (publish) {
        handledEditorPath = "";
        editorPath = "";
        updateSharedState({
          editorPath: "",
          editorContent: "",
          editorDirty: false,
        });
      }
    } finally {
      if (loadVersion === previewLoadVersion) loading = false;
    }
  }

  function synchronizeDirectory(path: string) {
    if (!path || normalizedPath(path) === handledDirectoryPath || !root) return;
    void selectDirectory(directoryEntry(path), false, false);
  }

  function synchronizeOpenedFile(path: string) {
    if (!path) {
      if (handledEditorPath && selected?.kind !== "directory") {
        handledEditorPath = "";
        void selectDirectory(directoryEntry(currentPath), false, false);
      }
      return;
    }
    const normalized = normalizedPath(path);
    if (normalized === handledEditorPath || !root) return;
    const entry = findDirectoryEntry(path, selectedKind) ?? {
      name: pathName(path),
      path,
      kind:
        selectedKind === "file" || selectedKind === "symlink"
          ? selectedKind
          : "file",
      size: 0,
    };
    if (entry.kind !== "directory") void selectFile(entry, false);
  }

  function directoryEntry(path: string): FileTreeEntry {
    return {
      name: pathName(path),
      path,
      kind: "directory",
      size: 0,
    };
  }

  function findDirectoryEntry(path: string, kind: FileTreeEntry["kind"] | "") {
    if (!path || !kind) return null;
    return (
      directoryEntries.find(
        (entry) => entry.kind === kind && samePath(entry.path, path),
      ) ?? null
    );
  }
  /* eslint-enable svelte/infinite-reactive-loop */

  function selectGridEntry(entry: FileTreeEntry) {
    selectedPath = entry.path;
    selectedKind = entry.kind;
    updateSharedState({ selectedPath, selectedKind });
  }

  function openGridEntry(entry: FileTreeEntry) {
    if (entry.kind === "directory") void selectDirectory(entry);
    else void selectFile(entry);
  }

  function openEntryContextMenu(
    entry: FileTreeEntry,
    source: "tree" | "grid" | "background",
    event: MouseEvent,
  ) {
    event.preventDefault();
    event.stopPropagation();
    if (source === "grid") selectGridEntry(entry);
    if (!sectionElement) return;
    const bounds = sectionElement.getBoundingClientRect();
    const scaleX = bounds.width / sectionElement.offsetWidth || 1;
    const scaleY = bounds.height / sectionElement.offsetHeight || 1;
    const menuWidth = 210;
    const menuHeight = entry.kind === "directory" ? 390 : 230;
    contextMenu = {
      entry,
      source,
      x: Math.max(
        8,
        Math.min(
          (event.clientX - bounds.left) / scaleX,
          sectionElement.offsetWidth - menuWidth - 8,
        ),
      ),
      y: Math.max(
        8,
        Math.min(
          (event.clientY - bounds.top) / scaleY,
          sectionElement.offsetHeight - menuHeight - 8,
        ),
      ),
    };
  }

  function runContextAction(action: () => void) {
    action();
    contextMenu = null;
  }

  function applySharedEditor(
    sharedPath: string,
    sharedContent: string | null,
    sharedDirty: boolean,
  ) {
    if (
      sharedContent === null ||
      encoding !== "utf8" ||
      !selected ||
      !samePath(sharedPath, selected.path)
    )
      return;
    if (content !== sharedContent) content = sharedContent;
    if (!sharedDirty) originalContent = sharedContent;
  }

  function updateEditor(value: string) {
    content = value;
    window.clearTimeout(editorSyncTimer);
    editorSyncTimer = window.setTimeout(() => {
      if (!selected || encoding !== "utf8") return;
      updateSharedState({
        editorPath: selected.path,
        editorContent: value,
        editorDirty: value !== originalContent,
      });
    }, 100);
  }

  async function save() {
    if (!selected || !dirty || !hasWriteAccess) return;
    loading = true;
    try {
      const response = await request({
        operation: "write",
        path: selected.path,
        content,
      });
      if (!response.ok)
        throw new Error(response.error || "Could not save file.");
      originalContent = content;
      window.clearTimeout(editorSyncTimer);
      updateSharedState({
        editorPath: selected.path,
        editorContent: content,
        editorDirty: false,
      });
      makeToast({ kind: "success", message: `Saved ${selected.name}.` });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      loading = false;
    }
  }

  function childPath(directory: string, name: string) {
    const separator = directory.includes("\\") ? "\\" : "/";
    return `${directory.replace(/[\\/]+$/, "")}${separator}${name}`;
  }

  function parentPath(path: string) {
    const separator = pathSeparator(path);
    const trimmed = trimTrailingSeparators(path);
    if (/^[A-Za-z]:$/.test(trimmed)) return `${trimmed}${separator}`;
    const boundary = trimmed.lastIndexOf(separator);
    if (boundary === 2 && trimmed[1] === ":") return trimmed.slice(0, 3);
    if (boundary > 0) return trimmed.slice(0, boundary);
    return separator;
  }

  function clampSidebarWidth(value: number, windowWidth = width) {
    const fallback = Math.round(windowWidth * 0.32);
    return Math.round(
      Math.min(
        Math.max(Number.isFinite(value) && value > 0 ? value : fallback, 200),
        windowWidth - 320,
      ),
    );
  }

  function startSidebarResize(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    resizingSidebar = true;
    window.addEventListener("pointermove", resizeSidebar);
    window.addEventListener("pointerup", finishSidebarResize, { once: true });
    window.addEventListener("pointercancel", finishSidebarResize, {
      once: true,
    });
  }

  function resizeSidebar(event: PointerEvent) {
    if (!resizingSidebar || !sectionElement) return;
    event.preventDefault();
    const bounds = sectionElement.getBoundingClientRect();
    const scale = bounds.width / width || 1;
    sidebarWidthValue = clampSidebarWidth(
      (event.clientX - bounds.left) / scale,
    );
  }

  function finishSidebarResize() {
    if (!resizingSidebar) return;
    resizingSidebar = false;
    stopSidebarResize();
    updateSharedState({ sidebarWidth: sidebarWidthValue });
  }

  function stopSidebarResize() {
    window.removeEventListener("pointermove", resizeSidebar);
    window.removeEventListener("pointerup", finishSidebarResize);
    window.removeEventListener("pointercancel", finishSidebarResize);
  }

  function encodeBase64(bytes: Uint8Array) {
    let binary = "";
    const chunkSize = 32 << 10;
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      binary += String.fromCharCode(
        ...bytes.subarray(offset, offset + chunkSize),
      );
    }
    return btoa(binary);
  }

  function safeUploadPath(path: string) {
    const parts = path.split(/[\\/]+/).filter(Boolean);
    if (
      parts.length === 0 ||
      parts.some(
        (part) =>
          part === "." ||
          part === ".." ||
          part.length > 255 ||
          /[\0\u0000-\u001f]/.test(part),
      )
    )
      throw new Error(`Upload path “${path}” is invalid.`);
    return parts;
  }

  async function uploadItems(items: UploadItem[]) {
    if (!hasWriteAccess || mutationBusy || items.length === 0) return;
    if (items.length > 500) {
      makeToast({
        kind: "error",
        message: "An upload can contain at most 500 files.",
      });
      return;
    }
    const totalSize = items.reduce((total, item) => total + item.file.size, 0);
    if (
      items.some((item) => item.file.size > 8 << 20) ||
      totalSize > 64 << 20
    ) {
      makeToast({
        kind: "error",
        message:
          "Each file must be at most 8 MiB and the upload at most 64 MiB.",
      });
      return;
    }
    const destination = uploadDestination;
    mutationBusy = true;
    try {
      const prepared = items.map((item) => ({
        item,
        parts: safeUploadPath(item.relativePath),
      }));
      const directories = new Set<string>();
      for (const { parts } of prepared) {
        let directory = destination;
        for (const part of parts.slice(0, -1)) {
          directory = childPath(directory, part);
          directories.add(directory);
        }
      }
      for (const directory of [...directories].sort(
        (left, right) => pathDepth(left) - pathDepth(right),
      )) {
        const response = await request({
          operation: "createDirectory",
          path: directory,
          recursive: true,
        });
        if (!response.ok)
          throw new Error(response.error || `Could not create ${directory}.`);
      }
      for (const { item, parts } of prepared) {
        const bytes = new Uint8Array(await item.file.arrayBuffer());
        const response = await request({
          operation: "write",
          path: parts.reduce(childPath, destination),
          content: encodeBase64(bytes),
          encoding: "base64",
        });
        if (!response.ok)
          throw new Error(
            response.error || `Could not upload ${item.relativePath}.`,
          );
      }
      uploadOpen = false;
      uploadTarget = "";
      makeToast({
        kind: "success",
        message: `Uploaded ${items.length} ${items.length === 1 ? "item" : "items"}.`,
      });
      publishMutationRefresh({
        currentPath: destination,
        selectedPath: "",
        selectedKind: "",
      });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      mutationBusy = false;
    }
  }

  async function createEntry(name: string) {
    if (!createKind || !hasWriteAccess || mutationBusy) return;
    const destination = entryDestination || selectedDestination;
    const path = childPath(destination, name);
    mutationBusy = true;
    try {
      const response = await request({
        operation: createKind === "file" ? "createFile" : "createDirectory",
        path,
      });
      if (!response.ok)
        throw new Error(response.error || `Could not create ${name}.`);
      const selectedKind = createKind;
      createKind = null;
      entryDestination = "";
      publishMutationRefresh({
        currentPath: destination,
        selectedPath: path,
        selectedKind,
      });
      makeToast({ kind: "success", message: `Created ${name}.` });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      mutationBusy = false;
    }
  }

  function beginCreate(kind: "file" | "directory", directory: string) {
    if (!hasWriteAccess || mutationBusy || !directory) return;
    entryDestination = directory;
    createKind = kind;
  }

  function beginUpload(directory: FileTreeEntry) {
    if (directory.kind !== "directory" || !hasWriteAccess || mutationBusy)
      return;
    uploadTarget = directory.path;
    uploadOpen = true;
  }

  function beginRename(entry: FileTreeEntry) {
    if (!canMutateEntry(entry) || !hasWriteAccess || mutationBusy) return;
    renameTarget = entry;
  }

  function beginMove(entry: FileTreeEntry) {
    if (!canMutateEntry(entry) || !hasWriteAccess || mutationBusy) return;
    moveTarget = entry;
  }

  function canMutateEntry(entry: FileTreeEntry) {
    return !samePath(parentPath(entry.path), entry.path);
  }

  function relocatedExpandedPaths(source: string, destination: string) {
    const sourceNormalized = normalizedPath(source);
    return expandedPaths.map((path) => {
      const candidate = normalizedPath(path);
      if (candidate === sourceNormalized) return destination;
      if (!candidate.startsWith(`${sourceNormalized}/`)) return path;
      return `${trimTrailingSeparators(destination)}${pathSeparator(destination)}${path
        .slice(source.length)
        .replace(/^[\\/]+/, "")}`;
    });
  }

  async function renameEntry(name: string) {
    if (!renameTarget || !hasWriteAccess || mutationBusy) return;
    const source = renameTarget;
    const destinationDirectory = parentPath(source.path);
    const destination = childPath(destinationDirectory, name);
    if (samePath(source.path, destination)) {
      renameTarget = null;
      return;
    }
    mutationBusy = true;
    try {
      const response = await request({
        operation: "rename",
        path: source.path,
        destination,
      });
      if (!response.ok)
        throw new Error(
          response.error || "Could not rename the selected item.",
        );
      renameTarget = null;
      const renamingCurrentDirectory =
        source.kind === "directory" && samePath(source.path, currentPath);
      publishMutationRefresh({
        currentPath: renamingCurrentDirectory ? destination : currentPath,
        selectedPath: renamingCurrentDirectory ? "" : destination,
        selectedKind: renamingCurrentDirectory ? "" : source.kind,
        expandedPaths: relocatedExpandedPaths(source.path, destination),
        editorPath: "",
        editorContent: "",
        editorDirty: false,
      });
      makeToast({ kind: "success", message: `Renamed to ${name}.` });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      mutationBusy = false;
    }
  }

  async function moveEntry(destinationDirectory: string) {
    if (!moveTarget || !hasWriteAccess || mutationBusy) return;
    const source = moveTarget;
    const destination = childPath(destinationDirectory, source.name);
    if (samePath(source.path, destination)) {
      makeToast({
        kind: "error",
        message: "The item is already in that folder.",
      });
      return;
    }
    mutationBusy = true;
    try {
      const response = await request({
        operation: "move",
        path: source.path,
        destination,
      });
      if (!response.ok)
        throw new Error(response.error || "Could not move the selected item.");
      moveTarget = null;
      const movingCurrentDirectory =
        source.kind === "directory" && samePath(source.path, currentPath);
      publishMutationRefresh({
        currentPath: movingCurrentDirectory ? destination : currentPath,
        selectedPath: movingCurrentDirectory ? "" : destination,
        selectedKind: movingCurrentDirectory ? "" : source.kind,
        expandedPaths: relocatedExpandedPaths(source.path, destination),
        editorPath: "",
        editorContent: "",
        editorDirty: false,
      });
      makeToast({ kind: "success", message: `Moved ${source.name}.` });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      mutationBusy = false;
    }
  }

  async function deleteEntry(entry: FileTreeEntry) {
    if (!canMutateEntry(entry) || !hasWriteAccess || mutationBusy) return;
    const deletingCurrentDirectory =
      entry.kind === "directory" && samePath(entry.path, currentPath);
    const destination = deletingCurrentDirectory
      ? parentPath(entry.path)
      : currentPath;
    if (!window.confirm(`Permanently delete “${entry.path}”?`)) return;
    mutationBusy = true;
    try {
      const response = await request({
        operation: "delete",
        path: entry.path,
      });
      if (!response.ok)
        throw new Error(
          response.error || "Could not delete the selected item.",
        );
      selected = null;
      encoding = null;
      content = "";
      originalContent = "";
      clearPreviewUrl();
      publishMutationRefresh({
        currentPath: destination,
        selectedPath: "",
        selectedKind: "",
        editorPath: "",
        editorContent: "",
        editorDirty: false,
      });
      makeToast({ kind: "success", message: "Deleted the selected item." });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      mutationBusy = false;
    }
  }

  function publishMutationRefresh(
    update: Parameters<typeof updateSharedState>[0],
  ) {
    const refreshToken = crypto.getRandomValues(new Uint32Array(1))[0];
    updateSharedState({
      ...update,
      // Independent clients must not publish the same increment after
      // concurrent filesystem mutations.
      treeRevision:
        refreshToken === treeRevision ? (refreshToken + 1) >>> 0 : refreshToken,
    });
  }

  function openTerminalAtEntry(entry: FileTreeEntry) {
    const location =
      entry.kind === "directory" ? entry.path : parentPath(entry.path);
    if (location) dispatch("openTerminal", location);
  }

  function previewType(filename: string) {
    const extension = filename.split(".").at(-1)?.toLowerCase();
    if (
      ["png", "jpg", "jpeg", "gif", "webp", "svg", "ico"].includes(
        extension ?? "",
      )
    )
      return "image";
    if (["mp3", "wav", "ogg", "flac", "m4a"].includes(extension ?? ""))
      return "audio";
    if (["mp4", "webm", "mov"].includes(extension ?? "")) return "video";
    if (extension === "pdf") return "pdf";
    return "binary";
  }

  function mimeType(filename: string) {
    const extension = filename.split(".").at(-1)?.toLowerCase();
    return (
      (
        {
          png: "image/png",
          jpg: "image/jpeg",
          jpeg: "image/jpeg",
          gif: "image/gif",
          webp: "image/webp",
          svg: "image/svg+xml",
          ico: "image/x-icon",
          mp3: "audio/mpeg",
          wav: "audio/wav",
          ogg: "audio/ogg",
          flac: "audio/flac",
          m4a: "audio/mp4",
          mp4: "video/mp4",
          webm: "video/webm",
          mov: "video/quicktime",
          pdf: "application/pdf",
        } as Record<string, string>
      )[extension ?? ""] ?? "application/octet-stream"
    );
  }
</script>

<svelte:window
  on:mousedown|capture={closeSettingsOnOutsideClick}
  on:keydown={handleWindowKeydown}
/>

<section
  bind:this={sectionElement}
  class="file-window relative flex flex-col overflow-hidden rounded-xl border border-zinc-700 bg-zinc-950 shadow-lg shadow-black/45"
  class:linked-highlight={linkedHighlight}
  class:fullscreen
  style:width={fullscreen ? "100%" : `${width}px`}
  style:height={fullscreen ? "100%" : `${height}px`}
  role="presentation"
  aria-label="File explorer"
  on:mousedown={() => {
    dispatch("bringToFront");
    dispatch("focus");
  }}
  on:focusin={() => dispatch("focus")}
  on:focusout={(event) => {
    if (
      event.relatedTarget instanceof Node &&
      sectionElement.contains(event.relatedTarget)
    )
      return;
    dispatch("blur");
  }}
  on:pointerdown|stopPropagation
  on:wheel={(event) => {
    if (!event.ctrlKey) event.stopPropagation();
  }}
>
  <header
    role="presentation"
    class="relative flex h-9 shrink-0 cursor-move select-none items-center border-b border-zinc-800"
    class:cursor-default={fullscreen}
    on:mousedown={(event) => {
      if (event.button === 0 && !fullscreen) dispatch("startMove", event);
    }}
  >
    <div class="flex h-full flex-1 items-center px-3">
      <CircleButtons>
        <CircleButton
          kind="red"
          disabled={!hasWriteAccess}
          ariaLabel="Close file explorer"
          on:mousedown={(event) => event.button === 0 && dispatch("close")}
        />
        <CircleButton
          kind="purple"
          active={fullscreen}
          ariaLabel={fullscreen ? "Exit full screen" : "Full screen"}
          on:mousedown={(event) =>
            event.button === 0 && dispatch("toggleFullscreen")}
        />
      </CircleButtons>
    </div>
    <div
      class="flex h-full w-0 flex-grow-[4] items-center justify-center overflow-hidden whitespace-nowrap px-2 text-center text-sm font-medium text-zinc-300"
    >
      <span class="truncate">{title} · Files</span>
    </div>
    <div
      class="relative flex h-full flex-1 items-center justify-end gap-0.5 pr-2"
    >
      <button
        class="header-button"
        title="Reload filesystem"
        aria-label="Reload filesystem"
        on:mousedown|stopPropagation
        on:click={loadRoot}><RefreshCwIcon /></button
      >
      <button
        bind:this={settingsButton}
        class="header-button"
        title="File explorer settings"
        aria-label="File explorer settings"
        on:mousedown|stopPropagation
        on:click={() => (settingsOpen = !settingsOpen)}><SettingsIcon /></button
      >
      {#if settingsOpen}
        <div
          bind:this={settingsPanel}
          role="presentation"
          class="panel absolute right-2 top-8 z-30 w-52 p-1.5 text-left text-sm"
          on:mousedown|stopPropagation
        >
          <button
            type="button"
            class="settings-row"
            on:click={() => {
              sidebarWidthValue = clampSidebarWidth(Math.round(width * 0.32));
              updateSharedState({ sidebarWidth: sidebarWidthValue });
              settingsOpen = false;
            }}>Reset split layout</button
          >
        </div>
      {/if}
    </div>
  </header>
  <div
    class="grid min-h-0 flex-1"
    style:grid-template-columns={`${sidebarWidthValue}px 5px minmax(0, 1fr)`}
  >
    <aside
      class="flex min-h-0 flex-col overflow-hidden border-r border-zinc-800 bg-zinc-900/45"
    >
      <div
        class="relative flex h-9 shrink-0 items-center border-b border-zinc-800 bg-indigo-500/10 px-2"
      >
        {#if pathEditing}
          <input
            bind:this={pathInput}
            bind:value={pathDraft}
            class:path-invalid={!pathPreviewValid}
            class="path-input"
            aria-label="Filesystem path"
            autocomplete="off"
            spellcheck="false"
            on:input={schedulePathPreview}
            on:keydown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                cancelPathEdit();
              } else if (event.key === "Enter") {
                event.preventDefault();
                commitPathEdit();
              }
            }}
            on:blur={cancelPathEdit}
          />
        {:else}
          <button
            class="flex h-full min-w-0 flex-1 items-center gap-2 rounded px-1 text-left text-sm text-indigo-100 hover:bg-indigo-500/10"
            title={currentPath || initialPath}
            on:click={beginPathEdit}
          >
            <FolderIcon class="h-4 w-4 shrink-0 text-amber-300" />
            <span class="truncate">{currentPath || initialPath}</span>
          </button>
        {/if}
      </div>
      {#if root}
        <div class="min-h-0 flex-1" class:pointer-events-none={pathEditing}>
          {#key treeVersion}
            <FileTree
              {root}
              {loadDirectory}
              {selectDirectory}
              expandedPaths={displayedExpandedPaths}
              selectedPath={currentPath}
              scrollTop={treeScrollTop}
              updateExpandedPaths={(paths) =>
                updateSharedState({ expandedPaths: paths })}
              updateScrollTop={(scrollTop) =>
                updateSharedState({ treeScrollTop: scrollTop })}
              reportError={(message) => (error = message)}
              openContextMenu={(entry, event) =>
                openEntryContextMenu(entry, "tree", event)}
            />
          {/key}
        </div>
      {:else if !loading && !pathEditing}
        <p class="p-4 text-sm text-zinc-500">No directory loaded.</p>
      {/if}
    </aside>
    <button
      type="button"
      class="sidebar-divider"
      class:active={resizingSidebar}
      aria-label="Resize file tree"
      title="Drag to resize the file tree"
      on:pointerdown={startSidebarResize}
    ></button>
    <main
      class="relative flex min-h-0 min-w-0 flex-col bg-[#111113]"
      data-canvas-file-editor
    >
      <div
        class="flex h-10 shrink-0 items-center gap-2 border-b border-zinc-800 px-3"
      >
        <span class="min-w-0 flex-1 truncate text-xs text-zinc-400"
          >{actionTarget?.path ?? currentPath}</span
        >
        {#if actionTarget?.kind === "directory"}
          <span class="text-[11px] text-zinc-600"
            >{samePath(actionTarget.path, currentPath)
              ? `${directoryEntries.length} items`
              : "Folder selected"}</span
          >
          <button
            class="content-action"
            disabled={!hasWriteAccess || mutationBusy}
            title="Upload files or folders here"
            aria-label="Upload files or folders here"
            on:click={() => beginUpload(actionTarget!)}
            ><UploadCloudIcon /></button
          >
          <button
            class="content-action"
            disabled={!hasWriteAccess || mutationBusy}
            title="Create folder here"
            aria-label="Create folder here"
            on:click={() => beginCreate("directory", actionTarget!.path)}
            ><FolderPlusIcon /></button
          >
          <button
            class="content-action"
            disabled={!hasWriteAccess || mutationBusy}
            title="Create file here"
            aria-label="Create file here"
            on:click={() => beginCreate("file", actionTarget!.path)}
            ><FilePlusIcon /></button
          >
          <button
            class="content-action"
            title="Open terminal here"
            aria-label="Open terminal here"
            on:click={() => openTerminalAtEntry(actionTarget!)}
            ><TerminalIcon /></button
          >
          <button
            class="content-action danger"
            disabled={!hasWriteAccess ||
              mutationBusy ||
              !canMutateEntry(actionTarget!)}
            title="Delete folder"
            aria-label="Delete folder"
            on:click={() => void deleteEntry(actionTarget!)}
            ><Trash2Icon /></button
          >
        {:else if actionTarget}
          <button
            class="content-action"
            title="Open or edit file"
            aria-label="Open or edit file"
            on:click={() => openGridEntry(actionTarget!)}><Edit2Icon /></button
          >
          <button
            class="content-action"
            title="Open terminal in containing folder"
            aria-label="Open terminal in containing folder"
            on:click={() => openTerminalAtEntry(actionTarget!)}
            ><TerminalIcon /></button
          >
          <button
            class="content-action danger"
            disabled={!hasWriteAccess || mutationBusy}
            title="Delete file"
            aria-label="Delete file"
            on:click={() => void deleteEntry(actionTarget!)}
            ><Trash2Icon /></button
          >
        {/if}
        {#if dirty}
          <span class="text-xs text-amber-300">Unsaved</span>
          <button
            class="save-button"
            disabled={!hasWriteAccess || loading}
            on:click={save}><SaveIcon />Save</button
          >
        {/if}
      </div>
      <div
        class="relative min-h-0 flex-1 overflow-auto"
        on:wheel={(event) => {
          if (!event.ctrlKey) event.stopPropagation();
        }}
      >
        {#if selected?.kind === "directory"}
          <div
            class="directory-list"
            aria-label={`Contents of ${selected.path}`}
            role="presentation"
            on:mousedown={(event) => {
              if (event.target !== event.currentTarget || event.button !== 0)
                return;
              selectedPath = "";
              selectedKind = "";
              updateSharedState({ selectedPath, selectedKind });
            }}
            on:contextmenu={(event) =>
              event.target === event.currentTarget &&
              openEntryContextMenu(currentDirectory, "background", event)}
          >
            {#if directoryEntries.length}
              {#each directoryEntries as entry (entry.path)}
                <button
                  type="button"
                  class="directory-entry"
                  class:selected={selectedKind === entry.kind &&
                    samePath(selectedPath, entry.path)}
                  title={entry.path}
                  on:click={() => selectGridEntry(entry)}
                  on:dblclick={() => openGridEntry(entry)}
                  on:contextmenu={(event) =>
                    openEntryContextMenu(entry, "grid", event)}
                >
                  {#if entry.kind === "directory"}
                    <FolderIcon class="text-amber-300/90" />
                  {:else}
                    <FileIcon class="text-zinc-400" />
                  {/if}
                  <span class="directory-entry-name">{entry.name}</span>
                </button>
              {/each}
            {:else if !loading}
              <div class="empty-directory">Empty</div>
            {/if}
          </div>
        {:else if selected && encoding === "utf8"}
          {#key selected.path}
            <CodeEditor
              value={content}
              filename={selected.name}
              readOnly={!hasWriteAccess}
              onChange={updateEditor}
              bind:insertText={editorInsertText}
              bind:previewTextDrop={editorPreviewTextDrop}
              bind:cancelTextDropPreview={editorCancelTextDropPreview}
            />
          {/key}
        {:else if previewUrl && previewKind === "image"}
          <div class="flex min-h-full items-center justify-center p-6">
            <img
              class="max-h-full max-w-full object-contain"
              src={previewUrl}
              alt={selected?.name}
            />
          </div>
        {:else if previewUrl && previewKind === "audio"}
          <div class="flex min-h-full items-center justify-center p-6">
            <audio controls src={previewUrl}></audio>
          </div>
        {:else if previewUrl && previewKind === "video"}
          <!-- svelte-ignore a11y_media_has_caption -->
          <div class="flex min-h-full items-center justify-center p-6">
            <video class="max-h-full max-w-full" controls src={previewUrl}
            ></video>
          </div>
        {:else if previewUrl && previewKind === "pdf"}
          <iframe
            class="h-full w-full border-0"
            src={previewUrl}
            title={selected?.name}
          ></iframe>
        {:else if selected && encoding === "base64"}
          <div
            class="flex h-full items-center justify-center p-8 text-center text-sm text-zinc-500"
          >
            Binary preview is not available for this file type.
          </div>
        {:else}
          <div
            class="flex h-full items-center justify-center p-8 text-center text-sm text-zinc-600"
          >
            Select a folder on the left, then double-click an item to open it.
          </div>
        {/if}
        {#if loading}<div
            class="absolute inset-0 flex items-center justify-center bg-zinc-950/65 text-sm text-zinc-300 backdrop-blur-[1px]"
          >
            Loading…
          </div>{/if}
        {#if paragraphDropState === "blocked"}<div
            class="pointer-events-none absolute inset-0 z-20 flex items-center justify-center border-2 border-dashed border-amber-300/65 bg-amber-950/35 p-8 text-center text-sm font-medium text-amber-100 backdrop-blur-[1px]"
          >
            Open an editable text file before dropping this paragraph.
          </div>{/if}
      </div>
      {#if error}<div
          class="shrink-0 border-t border-red-900/60 bg-red-950/50 px-3 py-2 text-xs text-red-300"
          role="alert"
        >
          {error}
        </div>{/if}
    </main>
  </div>
  {#if linkedNotes.length}<div class="file-relations">
      <CanvasRelations
        items={linkedNotes}
        disabled={!hasWriteAccess}
        on:navigate={(event) => dispatch("navigateNote", event.detail)}
        on:remove={(event) => dispatch("unlinkNote", event.detail)}
      />
    </div>{/if}
  {#if contextMenu}
    <div
      bind:this={contextMenuElement}
      class="context-menu"
      role="menu"
      tabindex="-1"
      aria-label={`Actions for ${contextMenu.entry.name}`}
      style:left={`${contextMenu.x}px`}
      style:top={`${contextMenu.y}px`}
      on:mousedown|stopPropagation
      on:contextmenu|preventDefault|stopPropagation
    >
      <div class="context-title" title={contextMenu.entry.path}>
        {contextMenu.entry.name}
      </div>
      {#if contextMenu.entry.kind === "directory"}
        <button
          class="context-action"
          role="menuitem"
          on:click={() =>
            runContextAction(() => void selectDirectory(contextMenu!.entry))}
          ><FolderIcon />Open folder</button
        >
        <button
          class="context-action"
          role="menuitem"
          on:click={() =>
            runContextAction(() => openTerminalAtEntry(contextMenu!.entry))}
          ><TerminalIcon />Open terminal here</button
        >
        <div class="context-divider"></div>
        <button
          class="context-action"
          role="menuitem"
          disabled={!hasWriteAccess || mutationBusy}
          on:click={() =>
            runContextAction(() => beginUpload(contextMenu!.entry))}
          ><UploadCloudIcon />Upload here</button
        >
        <button
          class="context-action"
          role="menuitem"
          disabled={!hasWriteAccess || mutationBusy}
          on:click={() =>
            runContextAction(() =>
              beginCreate("directory", contextMenu!.entry.path),
            )}><FolderPlusIcon />New folder</button
        >
        <button
          class="context-action"
          role="menuitem"
          disabled={!hasWriteAccess || mutationBusy}
          on:click={() =>
            runContextAction(() =>
              beginCreate("file", contextMenu!.entry.path),
            )}><FilePlusIcon />New file</button
        >
      {:else}
        <button
          class="context-action"
          role="menuitem"
          on:click={() =>
            runContextAction(() => openGridEntry(contextMenu!.entry))}
          ><Edit2Icon />Open / edit</button
        >
        <button
          class="context-action"
          role="menuitem"
          on:click={() =>
            runContextAction(() => openTerminalAtEntry(contextMenu!.entry))}
          ><TerminalIcon />Open terminal here</button
        >
      {/if}
      <div class="context-divider"></div>
      <button
        class="context-action"
        role="menuitem"
        disabled={!hasWriteAccess ||
          mutationBusy ||
          !canMutateEntry(contextMenu.entry)}
        on:click={() => runContextAction(() => beginRename(contextMenu!.entry))}
        ><Edit2Icon />Rename</button
      >
      <button
        class="context-action"
        role="menuitem"
        disabled={!hasWriteAccess ||
          mutationBusy ||
          !canMutateEntry(contextMenu.entry)}
        on:click={() => runContextAction(() => beginMove(contextMenu!.entry))}
        ><MoveIcon />Move</button
      >
      <button
        class="context-action danger"
        role="menuitem"
        disabled={!hasWriteAccess ||
          mutationBusy ||
          !canMutateEntry(contextMenu.entry)}
        on:click={() =>
          runContextAction(() => void deleteEntry(contextMenu!.entry))}
        ><Trash2Icon />Delete</button
      >
    </div>
  {/if}
</section>

<FileUploadDialog
  open={uploadOpen}
  destination={uploadDestination}
  busy={mutationBusy}
  on:close={() => {
    uploadOpen = false;
    uploadTarget = "";
  }}
  on:upload={(event) => void uploadItems(event.detail)}
/>
<FileEntryDialog
  open={createKind !== null}
  kind={createKind ?? "file"}
  destination={entryDestination || selectedDestination}
  busy={mutationBusy}
  on:close={() => {
    createKind = null;
    entryDestination = "";
  }}
  on:create={(event) => void createEntry(event.detail)}
/>
<FileEntryDialog
  open={renameTarget !== null}
  kind={renameTarget?.kind === "directory" ? "directory" : "file"}
  mode="rename"
  initialName={renameTarget?.name ?? ""}
  destination={renameTarget ? parentPath(renameTarget.path) : ""}
  busy={mutationBusy}
  on:close={() => (renameTarget = null)}
  on:rename={(event) => void renameEntry(event.detail)}
/>
<FileMoveDialog
  open={moveTarget !== null}
  source={moveTarget?.path ?? ""}
  initialDestination={moveTarget ? parentPath(moveTarget.path) : ""}
  busy={mutationBusy}
  on:close={() => (moveTarget = null)}
  on:move={(event) => void moveEntry(event.detail)}
/>

<style lang="postcss">
  @reference "../../app.css";
  .header-button {
    @apply inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .header-button :global(svg) {
    @apply h-4 w-4;
  }
  .settings-row {
    @apply block w-full rounded px-2 py-1.5 text-left text-xs text-zinc-300 hover:bg-zinc-700 hover:text-white;
  }
  .path-input {
    @apply h-7 w-full rounded border border-indigo-500/40 bg-zinc-950/85 px-2 font-mono text-xs text-indigo-100 outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/25;
  }
  .path-input.path-invalid {
    @apply border-red-500/55 text-red-200 focus:border-red-400 focus:ring-red-500/20;
  }
  .sidebar-divider {
    @apply relative cursor-col-resize border-0 bg-zinc-800/60 p-0 outline-none transition-colors hover:bg-indigo-500/65 focus-visible:bg-indigo-500/65;
  }
  .sidebar-divider::after {
    content: "";
    @apply absolute inset-y-0 -left-1 -right-1;
  }
  .sidebar-divider.active {
    @apply bg-indigo-400;
  }
  .save-button {
    @apply inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1 text-xs text-white hover:bg-indigo-500 disabled:opacity-40;
  }
  .save-button :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .content-action {
    @apply inline-flex h-7 w-7 shrink-0 items-center justify-center rounded text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .content-action :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .content-action.danger:hover:not(:disabled) {
    @apply bg-red-950/70 text-red-300;
  }
  .directory-list {
    @apply relative grid min-h-full auto-rows-min grid-cols-[repeat(auto-fill,minmax(96px,112px))] content-start gap-2 p-3;
  }
  .directory-entry {
    @apply flex h-24 w-full flex-col items-center justify-center gap-2 rounded-lg border border-transparent px-2 py-2 text-sm text-zinc-300 outline-none hover:border-zinc-700/70 hover:bg-zinc-800/80 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-indigo-500/70;
  }
  .directory-entry.selected {
    @apply bg-indigo-500/20 text-indigo-100 ring-1 ring-inset ring-indigo-400/45;
  }
  .directory-entry :global(svg) {
    @apply h-9 w-9 shrink-0;
    stroke-width: 1.5;
  }
  .directory-entry-name {
    @apply max-h-8 w-full overflow-hidden break-all text-center text-xs leading-4;
  }
  .empty-directory {
    @apply pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-zinc-600;
  }
  .context-menu {
    @apply absolute z-50 w-[210px] rounded-lg border border-zinc-700 bg-zinc-900/98 p-1.5 text-left shadow-xl shadow-black/60 backdrop-blur-md;
  }
  .context-title {
    @apply truncate px-2 py-1.5 text-[11px] font-medium text-zinc-500;
  }
  .context-action {
    @apply flex h-8 w-full items-center gap-2 rounded-md px-2 text-xs text-zinc-300 outline-none hover:bg-zinc-700 hover:text-white focus-visible:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .context-action :global(svg) {
    @apply h-3.5 w-3.5 shrink-0;
  }
  .context-action.danger:not(:disabled) {
    @apply text-red-300;
  }
  .context-action.danger:hover:not(:disabled) {
    @apply bg-red-950/70;
  }
  .context-divider {
    @apply my-1 border-t border-zinc-700/80;
  }
  .file-window.fullscreen {
    display: flex;
    flex-direction: column;
  }
  .file-window.linked-highlight {
    outline: 2px solid rgb(212 212 216 / 75%);
    outline-offset: 1px;
    animation: linked-file-pulse 1.8s ease-in-out infinite;
  }
  .file-relations {
    @apply pointer-events-auto absolute bottom-2 right-2 z-20 max-w-[65%];
  }
  @keyframes linked-file-pulse {
    50% {
      box-shadow: 0 0 10px rgb(212 212 216 / 34%);
    }
  }
</style>
