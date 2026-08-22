<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import { FolderIcon } from "svelte-feather-icons";

  import type {
    FileOperationRequest,
    FileOperationResponse,
    FileTreeEntry,
  } from "$lib/protocol";
  import {
    type FilePreviewKind,
    ancestorPaths,
    childPath,
    encodeBase64,
    filesystemRoot,
    isPathInside,
    mimeType,
    normalizedPath,
    parentPath,
    pathDepth,
    pathName,
    pathSeparator,
    previewType,
    revealDirectoryPaths,
    safeUploadPath,
    samePath,
    trimTrailingSeparators,
  } from "$lib/filesystem";
  import {
    decodeDownloadContent,
    FILE_DOWNLOAD_LIMIT_BYTES,
    startBrowserDownload,
  } from "$lib/fileDownload";
  import { makeToast } from "$lib/toast";
  import CanvasRelations, {
    type CanvasRelationItem,
  } from "./CanvasRelations.svelte";
  import type {
    TextInsertPosition,
    TextInsertResult,
  } from "./CodeEditor.svelte";
  import FileEntryDialog from "./FileEntryDialog.svelte";
  import FileContextMenu from "./FileContextMenu.svelte";
  import FileDirectoryGrid from "./FileDirectoryGrid.svelte";
  import FileExplorerActions from "./FileExplorerActions.svelte";
  import FileExplorerHeader from "./FileExplorerHeader.svelte";
  import FileMoveDialog from "./FileMoveDialog.svelte";
  import FilePreview from "./FilePreview.svelte";
  import FileTree, { type FileNode } from "./FileTree.svelte";
  import FileUploadDialog, { type UploadItem } from "./FileUploadDialog.svelte";

  export let title: string;
  export let background: string;
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
  export let online: boolean;
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
    title?: string;
    background?: string;
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
    floatingChange: boolean;
  }>();
  let sectionElement: HTMLElement;
  let pathInput: HTMLInputElement;
  let loading = false;
  let error = "";
  let root: FileNode | null = null;
  let treeVersion = 0;
  let selected: FileTreeEntry | null = null;
  let directoryEntries: FileTreeEntry[] = [];
  let content = "";
  let originalContent = "";
  let encoding: "utf8" | "utf16le" | "utf16be" | "base64" | null = null;
  let previewUrl = "";
  let previewKind: FilePreviewKind;
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
  let contextMenu: {
    entry: FileTreeEntry;
    x: number;
    y: number;
  } | null = null;
  let mutationBusy = false;
  let downloadBusy = false;
  let headerFloating = false;
  let reportedFloating = false;
  let mounted = false;
  let observedOnline = online;
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

  $: dirty = isTextEncoding(encoding) && content !== originalContent;
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
  $: fileOverlayOpen =
    headerFloating ||
    contextMenu !== null ||
    uploadOpen ||
    createKind !== null ||
    renameTarget !== null ||
    moveTarget !== null;
  $: if (fileOverlayOpen !== reportedFloating) {
    reportedFloating = fileOverlayOpen;
    dispatch("floatingChange", fileOverlayOpen);
  }
  $: if (!resizingSidebar)
    sidebarWidthValue = clampSidebarWidth(sidebarWidth, width);
  $: if (treeRevision !== observedTreeRevision) {
    observedTreeRevision = treeRevision;
    if (online) void loadRoot();
  }
  $: {
    const reconnected = mounted && online && !observedOnline;
    observedOnline = online;
    if (reconnected) void loadRoot();
  }

  function unavailableEditorMessage() {
    if (!selected) return "No file is open in this file editor.";
    if (!isTextEncoding(encoding))
      return `“${selected.name}” is not an editable text file.`;
    if (!hasWriteAccess) return "The file editor is read-only.";
    return "The text editor is not ready.";
  }

  function insertIntoOpenEditor(
    text: string,
    position?: TextInsertPosition,
  ): TextInsertResult {
    if (!selected || !isTextEncoding(encoding) || !hasWriteAccess)
      return { ok: false, message: unavailableEditorMessage() };
    return editorInsertText(text, position);
  }

  function previewOpenEditorDrop(position: TextInsertPosition) {
    return Boolean(
      selected &&
      isTextEncoding(encoding) &&
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

  function isTextEncoding(
    value: typeof encoding,
  ): value is "utf8" | "utf16le" | "utf16be" {
    return value !== null && value !== "base64";
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
    mounted = true;
    insertText = insertIntoOpenEditor;
    previewTextDrop = previewOpenEditorDrop;
    cancelTextDropPreview = cancelOpenEditorDropPreview;
    if (online) void loadRoot();
  });

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
          editorContent: isTextEncoding(encoding) ? content : "",
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

  async function openGridEntry(entry: FileTreeEntry) {
    if (entry.kind === "directory") {
      expandedPaths = revealDirectoryPaths(expandedPaths, entry.path);
      updateSharedState({ expandedPaths });
      await selectDirectory(entry);
    } else {
      await selectFile(entry);
    }
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

  function applySharedEditor(
    sharedPath: string,
    sharedContent: string | null,
    sharedDirty: boolean,
  ) {
    if (
      sharedContent === null ||
      !isTextEncoding(encoding) ||
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
      if (!selected || !isTextEncoding(encoding)) return;
      updateSharedState({
        editorPath: selected.path,
        editorContent: value,
        editorDirty: value !== originalContent,
      });
    }, 100);
  }

  async function save() {
    if (!selected || !isTextEncoding(encoding) || !dirty || !hasWriteAccess)
      return;
    loading = true;
    try {
      const response = await request({
        operation: "write",
        path: selected.path,
        content,
        encoding,
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

  async function downloadFile(file: FileTreeEntry) {
    if (file.kind !== "file" || downloadBusy || mutationBusy) return;
    if (file.size > FILE_DOWNLOAD_LIMIT_BYTES) {
      makeToast({
        kind: "error",
        message: "Files larger than 8 MiB cannot be downloaded yet.",
      });
      return;
    }
    downloadBusy = true;
    try {
      const response = await request({ operation: "read", path: file.path });
      if (!response.ok)
        throw new Error(response.error || "Could not download file.");
      if (!response.encoding)
        throw new Error("The downloaded file has no content encoding.");
      const bytes = decodeDownloadContent(
        response.content ?? "",
        response.encoding,
      );
      if (response.size !== undefined && response.size !== bytes.byteLength)
        throw new Error("The downloaded file did not pass its size check.");
      startBrowserDownload(bytes, file.name, mimeType(file.name));
      makeToast({ kind: "success", message: `Downloaded ${file.name}.` });
    } catch (cause) {
      makeToast({
        kind: "error",
        message: cause instanceof Error ? cause.message : String(cause),
      });
    } finally {
      downloadBusy = false;
    }
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
</script>

<section
  bind:this={sectionElement}
  class="file-window relative flex flex-col overflow-hidden rounded-xl border border-zinc-700 bg-zinc-950 shadow-sm shadow-black/20"
  class:linked-highlight={linkedHighlight}
  class:fullscreen
  style:--file-window-background={background || "#111113"}
  style:width={fullscreen ? "100%" : `${width}px`}
  style:height={fullscreen ? "100%" : `${height}px`}
  role="presentation"
  tabindex="-1"
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
  <FileExplorerHeader
    {title}
    {background}
    {fullscreen}
    {hasWriteAccess}
    on:close={() => dispatch("close")}
    on:toggleFullscreen={() => dispatch("toggleFullscreen")}
    on:bringToFront={() => dispatch("bringToFront")}
    on:startMove={(event) => dispatch("startMove", event.detail)}
    on:reload={loadRoot}
    on:floatingChange={(event) => (headerFloating = event.detail)}
    on:title={(event) => updateSharedState({ title: event.detail })}
    on:background={(event) => updateSharedState({ background: event.detail })}
    on:resetSplit={() => {
      sidebarWidthValue = clampSidebarWidth(Math.round(width * 0.32));
      updateSharedState({ sidebarWidth: sidebarWidthValue });
    }}
  />
  <div
    class="grid min-h-0 flex-1"
    style:grid-template-columns={`${sidebarWidthValue}px 5px minmax(0, 1fr)`}
  >
    <aside
      class="file-tree-pane flex min-h-0 flex-col overflow-hidden border-r border-zinc-800"
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
      class="file-content-pane relative flex min-h-0 min-w-0 flex-col"
      data-canvas-file-editor
    >
      <FileExplorerActions
        target={actionTarget}
        {currentPath}
        directoryCount={directoryEntries.length}
        {dirty}
        {loading}
        {downloadBusy}
        {mutationBusy}
        {hasWriteAccess}
        canMutate={actionTarget ? canMutateEntry(actionTarget) : false}
        on:upload={(event) => beginUpload(event.detail)}
        on:create={(event) =>
          beginCreate(event.detail.kind, event.detail.directory)}
        on:openTerminal={(event) => openTerminalAtEntry(event.detail)}
        on:delete={(event) => void deleteEntry(event.detail)}
        on:openFile={(event) => openGridEntry(event.detail)}
        on:download={(event) => void downloadFile(event.detail)}
        on:save={save}
      />
      <div
        class="relative min-h-0 flex-1 overflow-auto"
        on:wheel={(event) => {
          if (!event.ctrlKey) event.stopPropagation();
        }}
      >
        <FilePreview
          {selected}
          {encoding}
          {content}
          {previewUrl}
          {previewKind}
          readOnly={!hasWriteAccess}
          {loading}
          paragraphDropBlocked={paragraphDropState === "blocked"}
          onChange={updateEditor}
          bind:insertText={editorInsertText}
          bind:previewTextDrop={editorPreviewTextDrop}
          bind:cancelTextDropPreview={editorCancelTextDropPreview}
        >
          <svelte:fragment slot="directory">
            <FileDirectoryGrid
              directory={selected ?? currentDirectory}
              entries={directoryEntries}
              {selectedPath}
              {selectedKind}
              {loading}
              on:clearSelection={() => {
                selectedPath = "";
                selectedKind = "";
                updateSharedState({ selectedPath, selectedKind });
              }}
              on:select={(event) => selectGridEntry(event.detail)}
              on:open={(event) => openGridEntry(event.detail)}
              on:context={(event) =>
                openEntryContextMenu(
                  event.detail.entry,
                  event.detail.source,
                  event.detail.event,
                )}
            />
          </svelte:fragment>
        </FilePreview>
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
    <FileContextMenu
      entry={contextMenu.entry}
      x={contextMenu.x}
      y={contextMenu.y}
      {hasWriteAccess}
      {mutationBusy}
      {downloadBusy}
      canMutate={canMutateEntry(contextMenu.entry)}
      on:close={() => (contextMenu = null)}
      on:openDirectory={(event) => void selectDirectory(event.detail)}
      on:openFile={(event) => openGridEntry(event.detail)}
      on:download={(event) => void downloadFile(event.detail)}
      on:openTerminal={(event) => openTerminalAtEntry(event.detail)}
      on:upload={(event) => beginUpload(event.detail)}
      on:create={(event) =>
        beginCreate(event.detail.kind, event.detail.directory)}
      on:rename={(event) => beginRename(event.detail)}
      on:move={(event) => beginMove(event.detail)}
      on:delete={(event) => void deleteEntry(event.detail)}
    />
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
  .file-window.fullscreen {
    display: flex;
    flex-direction: column;
  }
  .file-window {
    background: var(--file-window-background);
  }
  .file-tree-pane {
    background: color-mix(in srgb, var(--file-window-background) 82%, black);
  }
  .file-content-pane {
    background: color-mix(in srgb, var(--file-window-background) 90%, black);
  }
  .file-window.linked-highlight {
    border-color: rgb(212 212 216 / 75%);
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
