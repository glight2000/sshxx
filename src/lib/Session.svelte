<script lang="ts">
  import {
    onDestroy,
    onMount,
    tick,
    beforeUpdate,
    afterUpdate,
    createEventDispatcher,
  } from "svelte";
  import { fade } from "svelte/transition";
  import { debounce, throttle } from "lodash-es";

  import { Encrypt } from "./encrypt";
  import {
    FileRequestClient,
    randomEncryptedStream,
    randomHex,
  } from "./fileRequests";
  import { createLock } from "./lock";
  import { Srocket } from "./srocket";
  import { isNativeApp } from "./runtime";
  import type {
    WsClient,
    FileTreeEntry,
    WsFileWindow,
    WsNote,
    WsPage,
    WsServer,
    WsSshProfile,
    WsUser,
    WsWinsize,
  } from "./protocol";
  import { makeToast } from "./toast";
  import { TerminalHistory } from "./terminalHistory";
  import { constrainTerminalResize } from "./terminalGeometry";
  import type { ChatMessage } from "./ui/Chat.svelte";
  import CanvasContextMenu from "./ui/CanvasContextMenu.svelte";
  import Note from "./ui/Note.svelte";
  import ResizeHandles, {
    type ResizeDirection,
  } from "./ui/ResizeHandles.svelte";
  import type { CanvasSearchItem } from "./ui/TerminalSearch.svelte";
  import SessionChrome from "./ui/SessionChrome.svelte";
  import XTerm from "./ui/XTerm.svelte";
  import type { CanvasRelationItem } from "./ui/CanvasRelations.svelte";
  import type {
    TextInsertPosition,
    TextInsertResult,
  } from "./ui/CodeEditor.svelte";
  import Avatars from "./ui/Avatars.svelte";
  import LiveCursor from "./ui/LiveCursor.svelte";
  import { slide } from "./action/slide";
  import { TouchZoom, INITIAL_ZOOM } from "./action/touchZoom";
  import {
    arrangeNewCanvasItem,
    arrangeNewCanvasItemNear,
    type CanvasItemRect,
  } from "./arrange";
  import {
    canvasItemKey,
    marqueeRect,
    parseCanvasItemKey,
    rectsIntersect,
    type CanvasItemKey,
    type CanvasItemKind,
  } from "./canvasSelection";
  import { canvasPanButton, canvasSelectionButton } from "./canvasMouseButtons";
  import {
    GRID_SIZE,
    gridAlignedRect,
    gridLeadingEdge,
    gridTrailingEdge,
  } from "./grid";
  import { settings } from "./settings";
  import {
    LOCAL_VIEW_STATE_VERSION,
    localViewStateKey,
    parseLocalViewState,
  } from "./viewState";

  export let id: string;

  const dispatch = createEventDispatcher<{ receiveName: string }>();

  // The magic numbers "left" and "top" are used to approximately center the
  // terminal at the time that it is first created.
  const CONSTANT_OFFSET_LEFT = 378;
  const CONSTANT_OFFSET_TOP = 240;

  const OFFSET_LEFT_CSS = `calc(50vw - ${CONSTANT_OFFSET_LEFT}px)`;
  const OFFSET_TOP_CSS = `calc(50vh - ${CONSTANT_OFFSET_TOP}px)`;
  const OFFSET_TRANSFORM_ORIGIN_CSS = `calc(-1 * ${OFFSET_LEFT_CSS}) calc(-1 * ${OFFSET_TOP_CSS})`;

  // Terminal width and height limits.
  const TERM_INITIAL_ROWS = 26;
  const TERM_INITIAL_COLS = 79;
  const TERM_INITIAL_WIDTH = 715;
  const TERM_INITIAL_HEIGHT = 523;
  const NOTE_INITIAL_WIDTH = 384;
  const NOTE_INITIAL_HEIGHT = 224;
  let fileExplorerModulePromise: Promise<
    typeof import("./ui/FileExplorer.svelte")
  > | null = null;

  function loadFileExplorer() {
    return (fileExplorerModulePromise ??= import("./ui/FileExplorer.svelte"));
  }
  const snapLeadingEdge = (value: number) =>
    $settings.snapToGrid ? gridLeadingEdge(value) : value;
  const snapTrailingEdge = (value: number) =>
    $settings.snapToGrid ? gridTrailingEdge(value) : value;
  const resizesWest = (direction: ResizeDirection) => direction.endsWith("w");
  const resizesEast = (direction: ResizeDirection) => direction.endsWith("e");
  const resizesNorth = (direction: ResizeDirection) =>
    direction.startsWith("n");
  const resizesSouth = (direction: ResizeDirection) =>
    direction.startsWith("s");
  const resizeCursor = (direction: ResizeDirection) =>
    direction === "n" || direction === "s"
      ? "ns-resize"
      : direction === "e" || direction === "w"
        ? "ew-resize"
        : direction === "ne" || direction === "sw"
          ? "nesw-resize"
          : "nwse-resize";

  function getConstantOffset() {
    return [
      0.5 * window.innerWidth - CONSTANT_OFFSET_LEFT,
      0.5 * window.innerHeight - CONSTANT_OFFSET_TOP,
    ];
  }

  let fabricEl: HTMLElement;
  let touchZoom: TouchZoom;
  let center = [0, 0];
  let zoom = INITIAL_ZOOM;
  let localViewStorageKey = "";
  let localViewSaveTimer: number | null = null;

  function saveLocalViewState() {
    if (!localViewStorageKey) return;
    if (localViewSaveTimer !== null) {
      window.clearTimeout(localViewSaveTimer);
      localViewSaveTimer = null;
    }
    pageViews[activePageId] = { center: [...center], zoom };
    try {
      window.localStorage.setItem(
        localViewStorageKey,
        JSON.stringify({
          version: LOCAL_VIEW_STATE_VERSION,
          activePageId: preferredPageId,
          pages: pageViews,
        }),
      );
      window.localStorage.removeItem(`sshx.activePage.${id}`);
    } catch (error) {
      console.warn("Could not persist local canvas view state.", error);
    }
  }

  function scheduleLocalViewSave() {
    if (!localViewStorageKey) return;
    if (localViewSaveTimer !== null) {
      window.clearTimeout(localViewSaveTimer);
    }
    localViewSaveTimer = window.setTimeout(saveLocalViewState, 250);
  }

  let showChat = false; // @hmr:keep
  let settingsOpen = false; // @hmr:keep
  let showNetworkInfo = false; // @hmr:keep
  let searchOpen = false;
  let canvasContextMenuOpen = false;
  let canvasContextMenuX = 0;
  let canvasContextMenuY = 0;
  let canvasContextPosition: [number, number] = [0, 0];
  let pendingCanvasContextMenu: {
    x: number;
    y: number;
    position: [number, number];
  } | null = null;
  let selectedCanvasItems: CanvasItemKey[] = [];
  let pendingCanvasSelection: CanvasItemKey | null = null;
  let pendingCanvasTitleFocus: CanvasItemKey | null = null;
  let selectionMarquee: {
    startX: number;
    startY: number;
    currentX: number;
    currentY: number;
    canvasLeft: number;
    canvasTop: number;
    moved: boolean;
  } | null = null;
  let canvasDropPageId: number | null = null;
  let suppressMarqueeContextMenu = false;
  let canvasDropPreviewOffsets: Partial<
    Record<CanvasItemKey, [number, number]>
  > = {};
  let serverVersion = "unknown";
  let daemonVersion = "unknown";

  function canvasItemFromTarget(target: EventTarget | null) {
    if (!(target instanceof Element)) return null;
    const element = target.closest<HTMLElement>(
      "[data-canvas-terminal], [data-canvas-note-wrapper], [data-canvas-file-window]",
    );
    if (!element) return null;
    if (element.dataset.canvasTerminal)
      return canvasItemKey("terminal", Number(element.dataset.canvasTerminal));
    if (element.dataset.canvasNoteWrapper)
      return canvasItemKey("note", Number(element.dataset.canvasNoteWrapper));
    if (element.dataset.canvasFileWindow)
      return canvasItemKey("file", Number(element.dataset.canvasFileWindow));
    return null;
  }

  function canvasItemExists(key: CanvasItemKey) {
    const { kind, id } = parseCanvasItemKey(key);
    if (kind === "terminal")
      return shells.some(
        ([shellId, shell]) => shellId === id && shell.pageId === activePageId,
      );
    if (kind === "note")
      return notes.some(
        ([noteId, note]) => noteId === id && note.pageId === activePageId,
      );
    return fileWindows.some(
      ([windowId, window]) => windowId === id && window.pageId === activePageId,
    );
  }

  function clearCanvasSelection() {
    selectedCanvasItems = [];
  }

  function canvasItemWrapper(key: CanvasItemKey) {
    const { kind, id } = parseCanvasItemKey(key);
    if (kind === "terminal") return termWrappers[id];
    if (kind === "note") return noteWrappers[id];
    return fileWrappers[id];
  }

  function beginMarqueeSelection(event: MouseEvent) {
    const selectionButton = canvasSelectionButton(
      $settings.swapCanvasMouseButtons,
    );
    if (
      event.button !== selectionButton ||
      event.target !== fabricEl ||
      activeFullscreenKey() !== null
    )
      return false;
    if (selectionButton === 2) suppressMarqueeContextMenu = false;
    const rect = fabricEl.getBoundingClientRect();
    clearCanvasSelection();
    if (document.activeElement instanceof HTMLElement)
      document.activeElement.blur();
    focusedTerminalId = null;
    focusedNoteId = null;
    focusedFileWindowId = null;
    focused = [];
    selectionMarquee = {
      startX: event.clientX,
      startY: event.clientY,
      currentX: event.clientX,
      currentY: event.clientY,
      canvasLeft: rect.left,
      canvasTop: rect.top,
      moved: false,
    };
    pendingCanvasSelection = null;
    pendingCanvasTitleFocus = null;
    canvasContextMenuOpen = false;
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
    return true;
  }

  function updateMarqueeSelection(event: MouseEvent) {
    if (!selectionMarquee) return;
    event.preventDefault();
    const distance = Math.hypot(
      event.clientX - selectionMarquee.startX,
      event.clientY - selectionMarquee.startY,
    );
    const moved = selectionMarquee.moved || distance >= 3;
    selectionMarquee = {
      ...selectionMarquee,
      currentX: event.clientX,
      currentY: event.clientY,
      moved,
    };
    if (!moved) return;
    if (canvasSelectionButton($settings.swapCanvasMouseButtons) === 2)
      suppressMarqueeContextMenu = true;
    window.getSelection()?.removeAllRanges();
    const marquee = marqueeRect(
      selectionMarquee.startX,
      selectionMarquee.startY,
      selectionMarquee.currentX,
      selectionMarquee.currentY,
    );
    const candidates: CanvasItemKey[] = [
      ...shells
        .filter(([, shell]) => shell.pageId === activePageId)
        .map(([shellId]) => canvasItemKey("terminal", shellId)),
      ...notes
        .filter(([, note]) => note.pageId === activePageId)
        .map(([noteId]) => canvasItemKey("note", noteId)),
      ...fileWindows
        .filter(([, window]) => window.pageId === activePageId)
        .map(([windowId]) => canvasItemKey("file", windowId)),
    ];
    selectedCanvasItems = candidates.filter((key) => {
      const wrapper = canvasItemWrapper(key);
      return wrapper
        ? rectsIntersect(marquee, wrapper.getBoundingClientRect())
        : false;
    });
  }

  function finishCanvasSelection() {
    if (selectionMarquee) {
      if (!selectionMarquee.moved) clearCanvasSelection();
      selectionMarquee = null;
      pendingCanvasSelection = null;
      return;
    }
    if (pendingCanvasSelection) {
      clearCanvasSelection();
      pendingCanvasSelection = null;
    }
    if (pendingCanvasTitleFocus) {
      focusCanvasItem(pendingCanvasTitleFocus);
      pendingCanvasTitleFocus = null;
    }
  }

  function hasActiveCanvasItem() {
    const activeElement = document.activeElement;
    return (
      activeElement instanceof HTMLElement &&
      (activeElement.classList.contains("xterm-helper-textarea") ||
        activeElement.closest(".term-container") !== null ||
        activeElement.closest("[data-canvas-note]") !== null)
    );
  }

  function activePageFullscreenKeys() {
    return [
      ...shells
        .filter(([, shell]) => shell.pageId === activePageId)
        .map(([shellId]) => `terminal:${shellId}`),
      ...notes
        .filter(([, note]) => note.pageId === activePageId)
        .map(([noteId]) => `note:${noteId}`),
      ...fileWindows
        .filter(([, fileWindow]) => fileWindow.pageId === activePageId)
        .map(([windowId]) => `file:${windowId}`),
    ].filter((key) => fullscreenItems[key]);
  }

  function activeFullscreenKey() {
    return activePageFullscreenKeys()[0] ?? null;
  }

  function exitActivePageFullscreen() {
    const activeKeys = activePageFullscreenKeys();
    if (!activeKeys.length) return;
    fullscreenItems = {
      ...fullscreenItems,
      ...Object.fromEntries(activeKeys.map((key) => [key, false])),
    };
  }

  function handleWindowMouseDownCapture(event: MouseEvent) {
    if (
      event.button === 2 &&
      canvasSelectionButton($settings.swapCanvasMouseButtons) === 2
    )
      suppressMarqueeContextMenu = false;
    const fullscreenKey = activeFullscreenKey();
    const target = event.target instanceof Element ? event.target : null;
    if (fullscreenKey && !target?.closest(".canvas-fullscreen"))
      exitActivePageFullscreen();
    const wasLinking = linkingNoteId !== null;
    handleCanvasLinkSelection(event);
    if (wasLinking) return;
    if (beginMarqueeSelection(event)) return;
    if (event.button === 0) {
      pendingCanvasSelection = canvasItemFromTarget(event.target);
      const target = event.target instanceof Element ? event.target : null;
      pendingCanvasTitleFocus =
        pendingCanvasSelection &&
        target?.closest("[data-canvas-titlebar]") &&
        !target.closest("button, input, textarea, select, a")
          ? pendingCanvasSelection
          : null;
    }
  }

  onMount(() => {
    const configuredServer = isNativeApp()
      ? new URLSearchParams(window.location.search).get("server")
      : null;
    localViewStorageKey = localViewStateKey(
      id,
      window.location.origin,
      configuredServer,
    );
    let storedViewState = null;
    try {
      storedViewState = parseLocalViewState(
        window.localStorage.getItem(localViewStorageKey),
      );
    } catch (error) {
      console.warn("Could not load local canvas view state.", error);
    }
    if (storedViewState) {
      preferredPageId = storedViewState.activePageId;
      for (const [pageId, view] of Object.entries(storedViewState.pages)) {
        pageViews[Number(pageId)] = {
          center: [...view.center],
          zoom: view.zoom,
        };
      }
    } else {
      // One-time migration from the original active-page-only local setting.
      const storedPageId = Number(
        window.localStorage.getItem(`sshx.activePage.${id}`),
      );
      if (Number.isSafeInteger(storedPageId) && storedPageId > 0) {
        preferredPageId = storedPageId;
      }
    }

    touchZoom = new TouchZoom(
      fabricEl,
      () => !hasActiveCanvasItem(),
      () => activeFullscreenKey() === null,
      () => canvasPanButton($settings.swapCanvasMouseButtons),
    );
    const initialView = pageViews[activePageId];
    center = [...initialView.center];
    zoom = initialView.zoom;
    touchZoom.setView(center, zoom);
    const unsubscribe = touchZoom.onMove(() => {
      center = touchZoom.center;
      zoom = touchZoom.zoom;
      pageViews[activePageId] = { center: [...center], zoom };
      scheduleLocalViewSave();

      // Blur if the user is currently focused on a terminal.
      //
      // This makes it so that panning does not stop when the cursor happens to
      // intersect with the textarea, which absorbs wheel and touch events.
      if (document.activeElement) {
        const classList = [...document.activeElement.classList];
        if (classList.includes("xterm-helper-textarea")) {
          (document.activeElement as HTMLElement).blur();
        }
      }

      showNetworkInfo = false;
      canvasContextMenuOpen = false;
      pendingCanvasContextMenu = null;
    });
    return () => {
      unsubscribe();
      saveLocalViewState();
    };
  });

  /** Returns the mouse position in infinite grid coordinates, offset transformations and zoom. */
  function normalizePosition(event: MouseEvent): [number, number] {
    const [ox, oy] = getConstantOffset();
    return [
      Math.round(center[0] + event.pageX / zoom - ox),
      Math.round(center[1] + event.pageY / zoom - oy),
    ];
  }

  function handlePageContextMenu(event: MouseEvent) {
    if (touchZoom?.consumeContextMenuSuppression()) {
      canvasContextMenuOpen = false;
      return;
    }
    if (
      canvasSelectionButton($settings.swapCanvasMouseButtons) === 2 &&
      (selectionMarquee?.moved || suppressMarqueeContextMenu)
    ) {
      suppressMarqueeContextMenu = false;
      canvasContextMenuOpen = false;
      pendingCanvasContextMenu = null;
      return;
    }
    const target = event.target;
    if (target === fabricEl) {
      const menu = {
        x: event.clientX,
        y: event.clientY,
        position: normalizePosition(event),
      };
      if (
        canvasSelectionButton($settings.swapCanvasMouseButtons) === 2 &&
        selectionMarquee !== null
      ) {
        pendingCanvasContextMenu = menu;
        canvasContextMenuOpen = false;
        return;
      }
      // Chromium can dispatch `contextmenu` before the secondary pointer is
      // released. Defer opening until mouseup so a right-drag never flashes the
      // action menu before it becomes a canvas pan.
      if (touchZoom?.isSecondaryPointerActive()) {
        pendingCanvasContextMenu = menu;
        canvasContextMenuOpen = false;
        return;
      }
      canvasContextMenuX = menu.x;
      canvasContextMenuY = menu.y;
      canvasContextPosition = menu.position;
      canvasContextMenuOpen = true;
      return;
    }
    canvasContextMenuOpen = false;
  }

  let encrypt: Encrypt;
  let fileRequests: FileRequestClient | null = null;
  let srocket: Srocket<WsServer, WsClient> | null = null;

  let connected = false;
  let sessionReady = false;
  let exitReason: string | null = null;
  let failureStage: "server" | "session" | null = null;
  let readinessTimer: number | null = null;
  let lastNotifiedConnectionIssue = "";

  function clearReadinessTimer() {
    if (readinessTimer !== null) {
      window.clearTimeout(readinessTimer);
      readinessTimer = null;
    }
  }

  function reportConnectionIssue(message: string, stage: "server" | "session") {
    exitReason = message;
    failureStage = stage;
    if (lastNotifiedConnectionIssue !== message) {
      lastNotifiedConnectionIssue = message;
      makeToast({ kind: "error", message }, 7000);
    }
  }

  function scheduleReadinessWarning() {
    if (readinessTimer !== null) return;
    readinessTimer = window.setTimeout(() => {
      readinessTimer = null;
      if (sessionReady || exitReason) return;
      if (connected) {
        reportConnectionIssue(
          "Connected to sshxx-server, but the session handshake timed out. Check that sshxx-daemon is running; retrying automatically.",
          "session",
        );
      } else {
        reportConnectionIssue(
          "Unable to reach sshxx-server through the WebSocket endpoint. Check the server and reverse proxy; retrying automatically.",
          "server",
        );
      }
    }, 5000);
  }

  /** Bound "write" method for each terminal. */
  const writers: Record<number, (data: string, replay?: boolean) => void> = {};
  const terminalTextSenders: Record<
    number,
    (data: string, execute?: boolean) => void
  > = {};
  const fileTextSenders: Record<
    number,
    (data: string, position?: TextInsertPosition) => TextInsertResult
  > = {};
  const fileDropPreviewers: Record<
    number,
    (position: TextInsertPosition) => boolean
  > = {};
  const fileDropPreviewCancelers: Record<number, () => void> = {};
  const termWrappers: Record<number, HTMLDivElement> = {};
  const termElements: Record<number, HTMLDivElement> = {};
  const noteWrappers: Record<number, HTMLElement> = {};
  const fileWrappers: Record<number, HTMLDivElement> = {};
  const chunknums: Record<number, number> = {};
  const locks: Record<number, any> = {};
  const terminalHistory = new TerminalHistory(2 * 1024 * 1024);
  const replayedWriters: Record<
    number,
    (data: string, replay?: boolean) => void
  > = {};
  // Transient collaboration state: synchronized, but never persisted.
  let userId = 0;
  let users: [number, WsUser][] = [];
  let noteEditors: Record<number, { pageId: number; userId: number }> = {};

  // Browser-memory-only derived state: neither synchronized nor persisted.
  let terminalTitles: Record<number, string> = {};
  let fullscreenItems: Record<string, boolean> = {};
  // Shared workspace state: synchronized by server and persisted by daemon.
  let shells: [number, WsWinsize][] = [];
  let notes: [number, WsNote][] = [];
  let fileWindows: [number, WsFileWindow][] = [];
  let fileEditorBuffers: Record<
    number,
    { path: string; stream: bigint; content: string }
  > = {};
  const fileEditorUpdateVersions: Record<number, number> = {};
  const pendingFileEditorUpdates = new Set<number>();
  let pages: WsPage[] = [{ id: 1, name: "Page 1" }];
  let sshProfiles: WsSshProfile[] = [];

  // Browser-local view state: never sent to server or persisted by daemon.
  let activePageId = 1;
  let preferredPageId = 1;
  let selectCreatedPage = false;
  const pageViews: Record<number, { center: number[]; zoom: number }> = {
    1: { center: [0, 0], zoom: INITIAL_ZOOM },
  };
  let subscriptions = new Set<number>();
  let linkingNoteId: number | null = null;
  let focusedTerminalId: number | null = null;
  let focusedNoteId: number | null = null;
  let focusedFileWindowId: number | null = null;
  type ParagraphDropTarget = {
    kind: "terminal" | "note" | "file";
    id: number;
    noteInsertIndex?: number;
    fileReady?: boolean;
  };
  let paragraphDrag: {
    paragraphs: string[];
    text: string;
    sourceNoteId: number;
    paragraphIndexes: number[];
  } | null = null;
  let paragraphDropTarget: ParagraphDropTarget | null = null;
  $: if (
    linkingNoteId !== null &&
    !notes.some(([noteId]) => noteId === linkingNoteId)
  )
    linkingNoteId = null;

  function appendTerminalHistory(id: number, data: string) {
    terminalHistory.append(id, data);
  }

  function readTerminalHistory(id: number) {
    return terminalHistory.read(id);
  }

  function writeTerminalData(id: number, data: string, replay: boolean) {
    appendTerminalHistory(id, data);
    const writer = writers[id];
    if (!writer) return;
    if (replayedWriters[id] !== writer) {
      replayedWriters[id] = writer;
      writer(readTerminalHistory(id), true);
    } else {
      writer(data, replay);
    }
  }

  // May be undefined before `users` is first populated.
  $: hasWriteAccess = users.find(([uid]) => uid === userId)?.[1]?.canWrite;

  let moving = -1; // Terminal ID that is being dragged.
  let movingOrigin = [0, 0]; // Coordinates of mouse at origin when drag started.
  let movingStartClient = [0, 0];
  let movingDidMove = false;
  let movingStartSize: WsWinsize;
  let movingSize: WsWinsize; // New [x, y] position of the dragged terminal.
  let movingIsDone = false; // Moving finished but hasn't been acknowledged.

  let resizing = -1; // Terminal ID that is being resized.
  let resizingStartPointer = [0, 0];
  let resizingStartEdges = [0, 0, 0, 0];
  let resizingStartPixels = [0, 0];
  let resizingStartSize: WsWinsize;
  let resizingCanvasCell = [0, 0];
  let resizingSize: WsWinsize; // Last resize message sent.
  let resizingDirection: ResizeDirection = "se";

  let movingNote = -1;
  let movingNoteOrigin = [0, 0];
  let movingNoteStartClient = [0, 0];
  let movingNoteDidMove = false;
  let movingNoteStartState: WsNote;
  let movingNoteState: WsNote;
  let resizingNote = -1;
  let resizingNoteStartPointer = [0, 0];
  let resizingNoteStartState: WsNote;
  let resizingNoteState: WsNote;
  let resizingNoteDirection: ResizeDirection = "se";

  let movingFile = -1;
  let movingFileOrigin = [0, 0];
  let movingFileStartClient = [0, 0];
  let movingFileDidMove = false;
  let movingFileStartState: WsFileWindow;
  let movingFileState: WsFileWindow;
  let resizingFile = -1;
  let resizingFileStartPointer = [0, 0];
  let resizingFileStartState: WsFileWindow;
  let resizingFileState: WsFileWindow;
  let resizingFileDirection: ResizeDirection = "se";
  let terminalFloating: Record<number, boolean> = {};
  let noteFloating: Record<number, boolean> = {};
  let fileFloating: Record<number, boolean> = {};

  type CanvasGroupMove = {
    leadKey: CanvasItemKey;
    selectedKeys: CanvasItemKey[];
    startPointer: [number, number];
    startClient: [number, number];
    leadPosition: [number, number];
    offset: [number, number];
    moved: boolean;
  };
  let canvasGroupMove: CanvasGroupMove | null = null;
  let groupTerminalStates: Record<number, WsWinsize> = {};
  let groupNoteStates: Record<number, WsNote> = {};
  let groupFileStates: Record<number, WsFileWindow> = {};
  let groupTerminalStartStates: Record<number, WsWinsize> = {};
  let groupNoteStartStates: Record<number, WsNote> = {};
  let groupFileStartStates: Record<number, WsFileWindow> = {};

  function startCanvasGroupMove(
    kind: CanvasItemKind,
    id: number,
    event: MouseEvent,
  ) {
    if (event.button !== 0) return false;
    const leadKey = canvasItemKey(kind, id);
    pendingCanvasSelection = null;
    if (!selectedCanvasItems.includes(leadKey)) {
      clearCanvasSelection();
      return false;
    }

    const selection = selectedCanvasItems.filter(canvasItemExists);
    selectedCanvasItems = selection;
    if (selection.length < 2) return false;

    // Save only the selection keys and lead geometry here. The remaining
    // states are projected lazily after the pointer crosses the drag threshold.
    const lead =
      kind === "terminal"
        ? shells.find(([itemId]) => itemId === id)?.[1]
        : kind === "note"
          ? notes.find(([itemId]) => itemId === id)?.[1]
          : fileWindows.find(([itemId]) => itemId === id)?.[1];
    if (!lead) return false;

    canvasGroupMove = {
      leadKey,
      selectedKeys: selection,
      startPointer: normalizePosition(event),
      startClient: [event.clientX, event.clientY],
      leadPosition: [lead.x, lead.y],
      offset: [0, 0],
      moved: false,
    };
    return true;
  }

  function updateCanvasPageDropTarget(event: MouseEvent, dragging: boolean) {
    if (!dragging) {
      canvasDropPageId = null;
      canvasDropPreviewOffsets = {};
      return;
    }
    const pageElement = document
      .elementFromPoint(event.clientX, event.clientY)
      ?.closest<HTMLElement>("[data-canvas-page-id]");
    const pageId = Number(pageElement?.dataset.canvasPageId);
    canvasDropPageId =
      Number.isSafeInteger(pageId) &&
      pageId !== activePageId &&
      pages.some((page) => page.id === pageId)
        ? pageId
        : null;
    if (canvasDropPageId === null || !pageElement) {
      canvasDropPreviewOffsets = {};
      return;
    }

    const keys = canvasGroupMove?.moved
      ? canvasGroupMove.selectedKeys
      : moving !== -1 && movingDidMove
        ? [canvasItemKey("terminal", moving)]
        : movingNote !== -1 && movingNoteDidMove
          ? [canvasItemKey("note", movingNote)]
          : movingFile !== -1 && movingFileDidMove
            ? [canvasItemKey("file", movingFile)]
            : [];
    const target = pageElement.getBoundingClientRect();
    const targetCenter = [
      target.left + target.width / 2,
      target.top + target.height / 2,
    ];
    const offsets: Partial<Record<CanvasItemKey, [number, number]>> = {};
    for (const key of keys) {
      const wrapper = canvasItemWrapper(key);
      if (!wrapper) continue;
      const rect = wrapper.getBoundingClientRect();
      offsets[key] = [
        (targetCenter[0] - (rect.left + rect.width / 2)) / zoom,
        (targetCenter[1] - (rect.top + rect.height / 2)) / zoom,
      ];
    }
    canvasDropPreviewOffsets = offsets;
  }

  function focusCanvasItem(key: CanvasItemKey) {
    clearCanvasSelection();
    const { kind, id } = parseCanvasItemKey(key);
    const focusTarget =
      kind === "terminal"
        ? termElements[id]?.querySelector<HTMLElement>(".xterm-helper-textarea")
        : kind === "note"
          ? noteWrappers[id]?.querySelector<HTMLElement>("[data-canvas-note]")
          : fileWrappers[id]?.querySelector<HTMLElement>(".file-window");
    focusTarget?.focus({ preventScroll: true });
  }

  function moveCanvasItemsToPage(
    keys: CanvasItemKey[],
    targetPageId: number,
    positionOverrides = new Map<CanvasItemKey, [number, number]>(),
  ) {
    const sourcePageId = activePageId;
    const selected = keys.filter(canvasItemExists);
    const keySet = new Set(selected);
    const terminalMoves = shells
      .filter(
        ([id, state]) =>
          state.pageId === sourcePageId &&
          keySet.has(canvasItemKey("terminal", id)),
      )
      .map(([id, state]): [number, number, number] => {
        const [x, y] = positionOverrides.get(canvasItemKey("terminal", id)) ?? [
          state.x,
          state.y,
        ];
        return [id, x, y];
      });
    const noteMoves = notes
      .filter(
        ([id, state]) =>
          state.pageId === sourcePageId &&
          keySet.has(canvasItemKey("note", id)),
      )
      .map(([id, state]): [number, number, number] => {
        const [x, y] = positionOverrides.get(canvasItemKey("note", id)) ?? [
          state.x,
          state.y,
        ];
        return [id, x, y];
      });
    const fileMoves = fileWindows
      .filter(
        ([id, state]) =>
          state.pageId === sourcePageId &&
          keySet.has(canvasItemKey("file", id)),
      )
      .map(([id, state]): [number, number, number] => {
        const [x, y] = positionOverrides.get(canvasItemKey("file", id)) ?? [
          state.x,
          state.y,
        ];
        return [id, x, y];
      });
    if (!terminalMoves.length && !noteMoves.length && !fileMoves.length)
      return false;

    if (document.activeElement instanceof HTMLElement)
      document.activeElement.blur();
    srocket?.send({
      moveCanvasItems: [
        sourcePageId,
        targetPageId,
        terminalMoves,
        noteMoves,
        fileMoves,
      ],
    });
    const terminalPositions = new Map(
      terminalMoves.map(([id, x, y]) => [id, [x, y] as const]),
    );
    const notePositions = new Map(
      noteMoves.map(([id, x, y]) => [id, [x, y] as const]),
    );
    const filePositions = new Map(
      fileMoves.map(([id, x, y]) => [id, [x, y] as const]),
    );
    shells = shells.map(([id, state]) => {
      const position = terminalPositions.get(id);
      return position
        ? [
            id,
            { ...state, x: position[0], y: position[1], pageId: targetPageId },
          ]
        : [id, state];
    });
    notes = notes.map(([id, state]) => {
      const position = notePositions.get(id);
      return position
        ? [
            id,
            { ...state, x: position[0], y: position[1], pageId: targetPageId },
          ]
        : [id, state];
    });
    fileWindows = fileWindows.map(([id, state]) => {
      const position = filePositions.get(id);
      return position
        ? [
            id,
            { ...state, x: position[0], y: position[1], pageId: targetPageId },
          ]
        : [id, state];
    });
    selectedCanvasItems = selected;
    canvasDropPageId = null;
    canvasDropPreviewOffsets = {};
    switchPage(targetPageId, true);
    return true;
  }

  function updateCanvasGroupMove(event: MouseEvent) {
    if (!canvasGroupMove) return;
    const clientDistance = Math.hypot(
      event.clientX - canvasGroupMove.startClient[0],
      event.clientY - canvasGroupMove.startClient[1],
    );
    if (!canvasGroupMove.moved && clientDistance < 3) return;
    pendingCanvasTitleFocus = null;

    const [pointerX, pointerY] = normalizePosition(event);
    const rawDx = pointerX - canvasGroupMove.startPointer[0];
    const rawDy = pointerY - canvasGroupMove.startPointer[1];
    const leadX = snapLeadingEdge(
      Math.round(canvasGroupMove.leadPosition[0] + rawDx),
    );
    const leadY = snapLeadingEdge(
      Math.round(canvasGroupMove.leadPosition[1] + rawDy),
    );
    const dx = leadX - canvasGroupMove.leadPosition[0];
    const dy = leadY - canvasGroupMove.leadPosition[1];
    const selected = new Set(canvasGroupMove.selectedKeys);
    if (!canvasGroupMove.moved) {
      groupTerminalStartStates = Object.fromEntries(
        shells.filter(([id]) => selected.has(canvasItemKey("terminal", id))),
      );
      groupNoteStartStates = Object.fromEntries(
        notes.filter(([id]) => selected.has(canvasItemKey("note", id))),
      );
      groupFileStartStates = Object.fromEntries(
        fileWindows.filter(([id]) => selected.has(canvasItemKey("file", id))),
      );
    }
    canvasGroupMove = { ...canvasGroupMove, offset: [dx, dy], moved: true };
    groupTerminalStates = Object.fromEntries(
      Object.entries(groupTerminalStartStates).map(([itemId, state]) => [
        Number(itemId),
        { ...state, x: state.x + dx, y: state.y + dy },
      ]),
    );
    groupNoteStates = Object.fromEntries(
      Object.entries(groupNoteStartStates).map(([itemId, state]) => [
        Number(itemId),
        { ...state, x: state.x + dx, y: state.y + dy },
      ]),
    );
    groupFileStates = Object.fromEntries(
      Object.entries(groupFileStartStates).map(([itemId, state]) => [
        Number(itemId),
        { ...state, x: state.x + dx, y: state.y + dy },
      ]),
    );
    updateCanvasPageDropTarget(event, true);
  }

  function finishCanvasGroupMove() {
    if (!canvasGroupMove) return;
    const move = canvasGroupMove;
    if (!move.moved) {
      clearCanvasSelection();
      focusCanvasItem(move.leadKey);
    } else if (canvasDropPageId !== null) {
      const overrides = new Map<CanvasItemKey, [number, number]>();
      for (const [id, state] of Object.entries(groupTerminalStartStates))
        overrides.set(canvasItemKey("terminal", Number(id)), [
          state.x,
          state.y,
        ]);
      for (const [id, state] of Object.entries(groupNoteStartStates))
        overrides.set(canvasItemKey("note", Number(id)), [state.x, state.y]);
      for (const [id, state] of Object.entries(groupFileStartStates))
        overrides.set(canvasItemKey("file", Number(id)), [state.x, state.y]);
      moveCanvasItemsToPage(move.selectedKeys, canvasDropPageId, overrides);
      // The cross-page operation has already committed the original positions.
    } else {
      shells = shells.map(([id, state]) => [
        id,
        groupTerminalStates[id] ?? state,
      ]);
      notes = notes.map(([id, state]) => [id, groupNoteStates[id] ?? state]);
      fileWindows = fileWindows.map(([id, state]) => [
        id,
        groupFileStates[id] ?? state,
      ]);
      for (const [id, state] of Object.entries(groupTerminalStates)) {
        srocket?.send({ move: [Number(id), state.pageId, state] });
      }
      for (const [id, state] of Object.entries(groupNoteStates)) {
        srocket?.send({ updateNote: [Number(id), state.pageId, state] });
      }
      for (const [id, state] of Object.entries(groupFileStates)) {
        srocket?.send({
          updateFileWindow: [Number(id), state.pageId, state],
        });
      }
    }
    canvasGroupMove = null;
    groupTerminalStates = {};
    groupNoteStates = {};
    groupFileStates = {};
    groupTerminalStartStates = {};
    groupNoteStartStates = {};
    groupFileStartStates = {};
    canvasDropPageId = null;
    canvasDropPreviewOffsets = {};
  }

  let chatMessages: ChatMessage[] = [];
  let newMessages = false;

  let serverLatencies: number[] = [];
  let shellLatencies: number[] = [];

  onMount(async () => {
    if (!window.isSecureContext || !crypto.subtle) {
      exitReason =
        "End-to-end encryption requires HTTPS (or localhost). Open this session through the LAN HTTPS endpoint.";
      return;
    }

    // The page hash sets the end-to-end encryption key.
    const key = window.location.hash?.slice(1).split(",")[0] ?? "";
    const writePassword = window.location.hash?.slice(1).split(",")[1] ?? null;

    encrypt = await Encrypt.new(key);
    const encryptedZeros = await encrypt.zeros();

    const writeEncryptedZeros = writePassword
      ? await (await Encrypt.new(writePassword)).zeros()
      : null;

    fileRequests = new FileRequestClient(
      encrypt,
      () => Boolean(srocket?.connected),
      (message) => srocket?.send(message),
    );
    scheduleReadinessWarning();
    srocket = new Srocket<WsServer, WsClient>(`/api/s/${id}`, {
      onMessage(message) {
        if (message.hello) {
          userId = message.hello[0];
          dispatch("receiveName", message.hello[1]);
          serverVersion = message.hello[2] || "unknown";
          daemonVersion = message.hello[3] || "unknown";
          makeToast({
            kind: "success",
            message: `Connected to the server.`,
          });
          exitReason = null;
          failureStage = null;
        } else if (message.invalidAuth) {
          reportConnectionIssue(
            "The URL is not correct: the end-to-end encryption key is invalid.",
            "session",
          );
          srocket?.dispose();
        } else if (message.chunks) {
          let [id, pageId, replay, seqnum, chunks] = message.chunks;
          if (
            !shells.some(
              ([shellId, shell]) => shellId === id && shell.pageId === pageId,
            )
          ) {
            return;
          }
          locks[id](async () => {
            await tick();
            chunknums[id] += chunks.length;
            const plaintextChunks: string[] = [];
            const decoder = new TextDecoder();
            for (const data of chunks) {
              const buf = await encrypt.segment(
                0x100000000n | BigInt(id),
                BigInt(seqnum),
                data,
              );
              seqnum += data.length;
              plaintextChunks.push(decoder.decode(buf));
            }
            writeTerminalData(id, plaintextChunks.join(""), replay);
          });
        } else if (message.users) {
          sessionReady = true;
          clearReadinessTimer();
          exitReason = null;
          failureStage = null;
          lastNotifiedConnectionIssue = "";
          users = message.users.map(([id, user]) => [
            id,
            { ...user, pageId: user.pageId ?? 1 },
          ]);
        } else if (message.userDiff) {
          const [id, update] = message.userDiff;
          users = users.filter(([uid]) => uid !== id);
          if (update !== null) {
            users = [...users, [id, { ...update, pageId: update.pageId ?? 1 }]];
          }
        } else if (message.shells) {
          const liveShellIds = new Set(
            message.shells.map(([shellId]) => shellId),
          );
          terminalHistory.retain(liveShellIds);
          for (const shellId of subscriptions) {
            if (liveShellIds.has(shellId)) continue;
            subscriptions.delete(shellId);
            delete chunknums[shellId];
            delete locks[shellId];
            delete replayedWriters[shellId];
            delete terminalTitles[shellId];
          }
          shells = message.shells.map(([shellId, winsize]) => [
            shellId,
            {
              ...winsize,
              width: winsize.width ?? 0,
              height: winsize.height ?? 0,
              title: winsize.title ?? "",
              background: winsize.background ?? "",
              opacity: winsize.opacity ?? 80,
              pageId: winsize.pageId ?? 1,
              theme: winsize.theme ?? "",
            },
          ]);
          if (movingIsDone) {
            moving = -1;
          }
          for (const [id, winsize] of message.shells) {
            if (!subscriptions.has(id)) {
              chunknums[id] ??= 0;
              locks[id] ??= createLock();
              subscriptions.add(id);
              srocket?.send({
                subscribe: [id, winsize.pageId ?? 1, chunknums[id]],
              });
            }
          }
        } else if (message.notes) {
          notes = message.notes.map(([noteId, note]) => [
            noteId,
            {
              ...note,
              paragraphs: note.paragraphs?.length
                ? note.paragraphs
                : note.text.split("\n"),
              linkedShellIds: note.linkedShellIds ?? [],
              linkedNoteIds: note.linkedNoteIds ?? [],
              linkedFileWindowIds: note.linkedFileWindowIds ?? [],
              title: note.title ?? "",
              width: note.width ?? 384,
              height: note.height ?? 224,
              pageId: note.pageId ?? 1,
            },
          ]);
        } else if (message.fileWindows) {
          fileWindows = message.fileWindows.map(([windowId, window]) => [
            windowId,
            {
              ...window,
              pageId: window.pageId ?? 1,
              path: window.path || ".",
              title: window.title || `Terminal ${window.shellId}`,
              background: window.background || "#111113",
              width: window.width || 1_040,
              height: window.height || 680,
              currentPath: window.currentPath || window.path || ".",
              expandedPaths: window.expandedPaths ?? [],
              selectedPath: window.selectedPath ?? "",
              selectedKind: window.selectedKind ?? "",
              treeScrollTop: window.treeScrollTop ?? 0,
              editorPath: window.editorPath ?? "",
              editorStream:
                typeof window.editorStream === "bigint"
                  ? window.editorStream
                  : BigInt(window.editorStream ?? 0),
              editorData:
                window.editorData instanceof Uint8Array
                  ? window.editorData
                  : new Uint8Array(),
              editorDirty: window.editorDirty ?? false,
            },
          ]);
          for (const [windowId, window] of fileWindows) {
            void synchronizeFileEditorBuffer(windowId, window);
          }
        } else if (message.pages) {
          pages = message.pages.length
            ? message.pages
            : [{ id: 1, name: "Page 1" }];
          if (selectCreatedPage) {
            selectCreatedPage = false;
            switchPage(pages.at(-1)?.id ?? pages[0].id);
          } else if (pages.some((page) => page.id === preferredPageId)) {
            switchPage(preferredPageId);
          } else if (!pages.some((page) => page.id === activePageId)) {
            switchPage(pages[0].id);
          }
        } else if (message.sshProfiles) {
          sshProfiles = message.sshProfiles.map((profile) => ({
            ...profile,
            theme: profile.theme || $settings.theme,
            backgroundEnabled: profile.backgroundEnabled ?? false,
            background: profile.background || "#181818",
          }));
        } else if (message.noteEditing) {
          const [noteId, pageId, editor] = message.noteEditing;
          noteEditors = { ...noteEditors };
          if (editor === null) delete noteEditors[noteId];
          else noteEditors[noteId] = { pageId, userId: editor };
        } else if (message.noteText) {
          const [noteId, pageId, text] = message.noteText;
          notes = notes.map(([id, note]) =>
            id === noteId && note.pageId === pageId
              ? [id, { ...note, text, paragraphs: text.split("\n") }]
              : [id, note],
          );
        } else if (message.noteParagraphs) {
          const [noteId, pageId, paragraphs] = message.noteParagraphs;
          notes = notes.map(([id, note]) =>
            id === noteId && note.pageId === pageId
              ? [id, { ...note, paragraphs, text: paragraphs.join("\n") }]
              : [id, note],
          );
        } else if (message.hear) {
          const [uid, name, msg] = message.hear;
          chatMessages.push({ uid, name, msg, sentAt: new Date() });
          chatMessages = chatMessages;
          if (!showChat) newMessages = true;
        } else if (message.shellLatency !== undefined) {
          const shellLatency = Number(message.shellLatency);
          shellLatencies = [...shellLatencies, shellLatency].slice(-10);
        } else if (message.fileResponse) {
          const [requestId, stream, data] = message.fileResponse;
          fileRequests?.handleResponse(requestId, BigInt(stream), data);
        } else if (message.pong !== undefined) {
          const serverLatency = Date.now() - Number(message.pong);
          serverLatencies = [...serverLatencies, serverLatency].slice(-10);
        } else if (message.error) {
          console.warn("Server error: " + message.error);
          makeToast({ kind: "error", message: message.error });
        }
      },

      onConnect() {
        exitReason = null;
        failureStage = null;
        scheduleReadinessWarning();
        srocket?.send({ authenticate: [encryptedZeros, writeEncryptedZeros] });
        if ($settings.name) {
          srocket?.send({ setName: $settings.name });
        }
        connected = true;
      },

      onDisconnect() {
        connected = false;
        sessionReady = false;
        userId = 0;
        subscriptions.clear();
        users = [];
        noteEditors = {};
        serverLatencies = [];
        shellLatencies = [];
        fileRequests?.rejectAll(
          "Connection closed before the filesystem request completed.",
        );
      },

      onClose(event) {
        if (event.code === 4404) {
          reportConnectionIssue(
            `Session is unavailable: ${event.reason || "sshxx-daemon is not connected"}. Retrying automatically.`,
            "session",
          );
        } else if (event.code === 4500) {
          reportConnectionIssue(
            `sshxx-server reported an internal error${event.reason ? `: ${event.reason}` : ""}. Retrying automatically.`,
            "session",
          );
        } else {
          reportConnectionIssue(
            event.reason
              ? `WebSocket closed (${event.code}): ${event.reason}. Retrying automatically.`
              : "Unable to reach sshxx-server through the WebSocket endpoint. Check the server and reverse proxy; retrying automatically.",
            "server",
          );
        }
        scheduleReadinessWarning();
      },
    });
  });

  onDestroy(() => {
    clearReadinessTimer();
    fileRequests?.dispose();
    srocket?.dispose();
  });

  // Send periodic ping messages for latency estimation.
  onMount(() => {
    const pingIntervalId = window.setInterval(() => {
      if (srocket?.connected) {
        srocket.send({ ping: BigInt(Date.now()) });
      }
    }, 2000);
    return () => window.clearInterval(pingIntervalId);
  });

  function integerMedian(values: number[]) {
    if (values.length === 0) {
      return null;
    }
    const sorted = values.toSorted();
    const mid = Math.floor(sorted.length / 2);
    return sorted.length % 2 !== 0
      ? sorted[mid]
      : Math.round((sorted[mid - 1] + sorted[mid]) / 2);
  }

  $: if ($settings.name) {
    srocket?.send({ setName: $settings.name });
  }

  let counter = 0n;
  let connectionStatus: "connected" | "connecting" | "unavailable";

  $: connectionStatus =
    connected && sessionReady
      ? "connected"
      : exitReason
        ? "unavailable"
        : "connecting";

  function switchPage(pageId: number, preserveCanvasSelection = false) {
    if (!pages.some((page) => page.id === pageId)) return;
    preferredPageId = pageId;
    if (pageId === activePageId) {
      scheduleLocalViewSave();
      return;
    }
    pageViews[activePageId] = { center: [...center], zoom };
    terminalFloating = {};
    noteFloating = {};
    fileFloating = {};
    if (!preserveCanvasSelection) selectedCanvasItems = [];
    pendingCanvasSelection = null;
    pendingCanvasTitleFocus = null;
    selectionMarquee = null;
    activePageId = pageId;
    const view = pageViews[pageId] ?? {
      center: [0, 0],
      zoom: INITIAL_ZOOM,
    };
    pageViews[pageId] = view;
    center = [...view.center];
    zoom = view.zoom;
    touchZoom?.setView(center, zoom);
    scheduleLocalViewSave();
    srocket?.send({ setCursor: [pageId, null] });
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  }

  function pageName(pageId: number) {
    return pages.find((page) => page.id === pageId)?.name ?? "Unknown page";
  }

  function noteTitle(noteId: number, note: WsNote) {
    if (note.title.trim()) return note.title.trim();
    const firstLine = (
      note.paragraphs?.length ? note.paragraphs : note.text.split("\n")
    )
      .flatMap((paragraph) => paragraph.split("\n"))
      .find((line) => line.trim());
    return firstLine?.trim() || `Note #${noteId}`;
  }

  function terminalTitle(shellId: number, winsize: WsWinsize) {
    return winsize.title || terminalTitles[shellId] || `Terminal #${shellId}`;
  }

  function fileWindowTitle(windowId: number, window: WsFileWindow) {
    const path = window.editorPath || window.currentPath || window.path;
    return path ? `${window.title} · ${path}` : `File editor #${windowId}`;
  }

  function updateNoteAssociations(
    noteId: number,
    update: Partial<
      Pick<WsNote, "linkedShellIds" | "linkedNoteIds" | "linkedFileWindowIds">
    >,
  ) {
    if (!hasWriteAccess) return;
    const entry = notes.find(([id]) => id === noteId);
    if (!entry) return;
    const [id, note] = entry;
    const next = { ...note, ...update };
    notes = notes.map(([candidateId, candidate]) =>
      candidateId === id ? [candidateId, next] : [candidateId, candidate],
    );
    srocket?.send({ updateNote: [id, note.pageId, next] });
  }

  function associatedNoteIds(noteId: number) {
    const note = notes.find(([id]) => id === noteId)?.[1];
    if (!note) return [];
    return Array.from(
      new Set([
        ...note.linkedNoteIds,
        ...notes
          .filter(
            ([candidateId, candidate]) =>
              candidateId !== noteId &&
              candidate.linkedNoteIds.includes(noteId),
          )
          .map(([candidateId]) => candidateId),
      ]),
    );
  }

  function toggleCanvasLinkSelection(noteId: number) {
    if (!hasWriteAccess) return;
    linkingNoteId = linkingNoteId === noteId ? null : noteId;
  }

  function removeCanvasRelation(noteId: number, item: CanvasRelationItem) {
    const note = notes.find(([id]) => id === noteId)?.[1];
    if (!note) return;
    if (item.kind === "terminal") {
      updateNoteAssociations(noteId, {
        linkedShellIds: note.linkedShellIds.filter((id) => id !== item.id),
      });
    } else if (item.kind === "file") {
      updateNoteAssociations(noteId, {
        linkedFileWindowIds: note.linkedFileWindowIds.filter(
          (id) => id !== item.id,
        ),
      });
    } else if (note.linkedNoteIds.includes(item.id)) {
      updateNoteAssociations(noteId, {
        linkedNoteIds: note.linkedNoteIds.filter((id) => id !== item.id),
      });
    } else {
      const incoming = notes.find(([id]) => id === item.id);
      if (incoming?.[1].linkedNoteIds.includes(noteId)) {
        updateNoteAssociations(item.id, {
          linkedNoteIds: incoming[1].linkedNoteIds.filter(
            (id) => id !== noteId,
          ),
        });
      }
    }
  }

  function handleCanvasLinkSelection(event: MouseEvent) {
    if (linkingNoteId === null || !(event.target instanceof Element)) return;
    if (event.target.closest("[data-link-toggle]")) return;
    const terminal = event.target.closest<HTMLElement>(
      "[data-canvas-terminal]",
    );
    const targetNote = event.target.closest<HTMLElement>(
      "[data-canvas-note-id]",
    );
    const fileEditor = event.target.closest<HTMLElement>(
      "[data-canvas-file-editor]",
    );
    const fileWindow = fileEditor?.closest<HTMLElement>(
      "[data-canvas-file-window]",
    );
    if (event.button !== 0) {
      linkingNoteId = null;
      return;
    }
    if (
      event.target.closest(
        "button, input, select, a, [role=dialog], [role=menu]",
      ) ||
      (event.target.closest("textarea") &&
        (!targetNote ||
          Number(targetNote.dataset.canvasNoteId) === linkingNoteId))
    ) {
      linkingNoteId = null;
      return;
    }
    if (!terminal && !targetNote && !fileWindow) {
      linkingNoteId = null;
      return;
    }
    const noteId = linkingNoteId;
    const note = notes.find(([id]) => id === noteId)?.[1];
    linkingNoteId = null;
    if (!note) return;
    event.preventDefault();
    event.stopPropagation();

    if (terminal) {
      const shellId = Number(terminal.dataset.canvasTerminal);
      const shell = shells.find(([id]) => id === shellId)?.[1];
      if (
        shell?.pageId === note.pageId &&
        !note.linkedShellIds.includes(shellId)
      ) {
        updateNoteAssociations(noteId, {
          linkedShellIds: [...note.linkedShellIds, shellId],
        });
      }
    } else if (targetNote) {
      const targetId = Number(targetNote.dataset.canvasNoteId);
      const target = notes.find(([id]) => id === targetId)?.[1];
      if (targetId === noteId) {
        makeToast({ kind: "info", message: "A note cannot link to itself." });
      } else if (
        target?.pageId === note.pageId &&
        !associatedNoteIds(noteId).includes(targetId)
      ) {
        updateNoteAssociations(noteId, {
          linkedNoteIds: [...note.linkedNoteIds, targetId],
        });
      }
    } else if (fileWindow) {
      const windowId = Number(fileWindow.dataset.canvasFileWindow);
      const target = fileWindows.find(([id]) => id === windowId)?.[1];
      if (
        target?.pageId === note.pageId &&
        !note.linkedFileWindowIds.includes(windowId)
      ) {
        updateNoteAssociations(noteId, {
          linkedFileWindowIds: [...note.linkedFileWindowIds, windowId],
        });
      }
    }
  }

  function handleRelationshipKeydown(event: KeyboardEvent) {
    let handled = false;
    if (event.key === "Escape" && linkingNoteId !== null) {
      linkingNoteId = null;
      handled = true;
    }
    if (
      event.key === "Escape" &&
      (selectedCanvasItems.length > 0 || selectionMarquee !== null)
    ) {
      clearCanvasSelection();
      pendingCanvasSelection = null;
      pendingCanvasTitleFocus = null;
      selectionMarquee = null;
      handled = true;
    }
    if (handled) event.preventDefault();
  }

  function existingCanvasItems(pageId = activePageId) {
    return [
      ...shells
        .filter(([, winsize]) => winsize.pageId === pageId)
        .flatMap(([shellId, winsize]) => {
          const wrapper = termWrappers[shellId];
          return wrapper
            ? [
                {
                  x: winsize.x,
                  y: winsize.y,
                  width: winsize.width || wrapper.clientWidth / zoom,
                  height: winsize.height || wrapper.clientHeight / zoom,
                },
              ]
            : [];
        }),
      ...notes
        .filter(([, note]) => note.pageId === pageId)
        .map(([, note]) => ({
          x: note.x,
          y: note.y,
          width: note.width,
          height: note.height,
        })),
      ...fileWindows
        .filter(([, window]) => window.pageId === pageId)
        .map(([, { x, y, width, height }]) => ({ x, y, width, height })),
    ];
  }

  function nextCanvasRect(width: number, height: number) {
    const position = arrangeNewCanvasItem(existingCanvasItems(), width, height);
    return gridAlignedRect({ ...position, width, height });
  }

  function nextCanvasRectNear(
    source: CanvasItemRect,
    width: number,
    height: number,
    pageId = activePageId,
  ) {
    const position = arrangeNewCanvasItemNear(
      existingCanvasItems(pageId),
      width,
      height,
      source,
    );
    return gridAlignedRect({ ...position, width, height });
  }

  function terminalCanvasRect(id: number): CanvasItemRect | null {
    const terminal = shells.find(([shellId]) => shellId === id)?.[1];
    if (!terminal) return null;
    const wrapper = termWrappers[id];
    return {
      x: terminal.x,
      y: terminal.y,
      width:
        terminal.width || wrapper?.clientWidth / zoom || TERM_INITIAL_WIDTH,
      height:
        terminal.height || wrapper?.clientHeight / zoom || TERM_INITIAL_HEIGHT,
    };
  }

  function canvasRectAt(
    position: [number, number] | undefined,
    width: number,
    height: number,
  ) {
    return position
      ? gridAlignedRect({ x: position[0], y: position[1], width, height })
      : nextCanvasRect(width, height);
  }

  async function handleCreate(position?: [number, number]) {
    if (hasWriteAccess === false) {
      makeToast({
        kind: "info",
        message: "You are in read-only mode and cannot create new terminals.",
      });
      return;
    }
    if (shells.length >= 100) {
      makeToast({
        kind: "error",
        message: "You can only create up to 100 terminals.",
      });
      return;
    }
    const { x, y, width, height } = canvasRectAt(
      position,
      TERM_INITIAL_WIDTH,
      TERM_INITIAL_HEIGHT,
    );
    srocket?.send({
      createWindowed: [
        x,
        y,
        width,
        height,
        TERM_INITIAL_ROWS,
        TERM_INITIAL_COLS,
        activePageId,
        $settings.theme,
      ],
    });
    if (!position) touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function handleCreateSsh(profileId: string, position?: [number, number]) {
    if (!hasWriteAccess || shells.length >= 100) return;
    const { x, y, width, height } = canvasRectAt(
      position,
      TERM_INITIAL_WIDTH,
      TERM_INITIAL_HEIGHT,
    );
    const profile = sshProfiles.find((item) => item.id === profileId);
    srocket?.send({
      createSshWindowed: [
        profileId,
        x,
        y,
        width,
        height,
        TERM_INITIAL_ROWS,
        TERM_INITIAL_COLS,
        activePageId,
        profile?.theme || $settings.theme,
      ],
    });
    if (!position) touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function handleCreateNote(position?: [number, number]) {
    if (hasWriteAccess === false || notes.length >= 100) return;
    const { x, y, width, height } = canvasRectAt(
      position,
      NOTE_INITIAL_WIDTH,
      NOTE_INITIAL_HEIGHT,
    );
    srocket?.send({ createNoteSized: [x, y, width, height, activePageId] });
    if (!position) touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  async function requestFileOperation(
    shellId: number,
    pageId: number,
    request: Parameters<FileRequestClient["request"]>[2],
  ) {
    if (!fileRequests) throw new Error("Filesystem requests are not ready.");
    return fileRequests.request(shellId, pageId, request);
  }

  function toggleFullscreen(
    kind: "terminal" | "note" | "file",
    itemId: number,
  ) {
    const key = `${kind}:${itemId}`;
    const entering = !fullscreenItems[key];
    const activeKeys = entering ? activePageFullscreenKeys() : [];
    fullscreenItems = {
      ...fullscreenItems,
      ...Object.fromEntries(activeKeys.map((activeKey) => [activeKey, false])),
      [key]: entering,
    };
  }

  type FileWindowSharedUpdate = {
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
  };

  async function synchronizeFileEditorBuffer(id: number, window: WsFileWindow) {
    if (pendingFileEditorUpdates.has(id)) return;
    if (!window.editorPath || window.editorStream === 0n) {
      if (fileEditorBuffers[id]) {
        fileEditorBuffers = { ...fileEditorBuffers };
        delete fileEditorBuffers[id];
      }
      return;
    }
    const existing = fileEditorBuffers[id];
    if (
      existing?.path === window.editorPath &&
      existing.stream === window.editorStream
    )
      return;
    try {
      const plaintext = await encrypt.segment(
        window.editorStream,
        0n,
        window.editorData,
      );
      const content = new TextDecoder("utf-8", { fatal: true }).decode(
        plaintext,
      );
      const current = fileWindows.find(([windowId]) => windowId === id)?.[1];
      if (
        !current ||
        current.editorPath !== window.editorPath ||
        current.editorStream !== window.editorStream ||
        pendingFileEditorUpdates.has(id)
      )
        return;
      fileEditorBuffers = {
        ...fileEditorBuffers,
        [id]: { path: window.editorPath, stream: window.editorStream, content },
      };
    } catch (cause) {
      console.warn("Could not decrypt a shared file editor buffer.", cause);
    }
  }

  function sameSharedValue(left: unknown, right: unknown) {
    if (Array.isArray(left) && Array.isArray(right)) {
      return (
        left.length === right.length &&
        left.every((value, index) => value === right[index])
      );
    }
    return left === right;
  }

  function updateFileWindowSharedState(
    id: number,
    pageId: number,
    update: FileWindowSharedUpdate,
  ) {
    if (!hasWriteAccess) return;
    const entry = fileWindows.find(([windowId]) => windowId === id);
    if (!entry || entry[1].pageId !== pageId) return;
    const current = entry[1];
    const { editorContent, ...stateUpdate } = update;
    const changed = Object.entries(stateUpdate).some(
      ([key, value]) =>
        !sameSharedValue(current[key as keyof WsFileWindow], value),
    );
    if (editorContent === undefined && !changed) return;

    let next: WsFileWindow = { ...current, ...stateUpdate };
    if (editorContent === undefined) {
      fileWindows = fileWindows.map(([windowId, window]) =>
        windowId === id ? [windowId, next] : [windowId, window],
      );
      srocket?.send({ updateFileWindow: [id, pageId, next] });
      return;
    }

    const nextEditorPath = stateUpdate.editorPath ?? next.editorPath;
    if (
      !changed &&
      fileEditorBuffers[id]?.path === nextEditorPath &&
      fileEditorBuffers[id]?.content === editorContent
    )
      return;

    const version = (fileEditorUpdateVersions[id] ?? 0) + 1;
    fileEditorUpdateVersions[id] = version;
    pendingFileEditorUpdates.add(id);
    next = {
      ...next,
      editorPath: nextEditorPath,
      editorStream: 0n,
      editorData: new Uint8Array(),
    };
    if (nextEditorPath) {
      fileEditorBuffers = {
        ...fileEditorBuffers,
        [id]: { path: nextEditorPath, stream: 0n, content: editorContent },
      };
    } else {
      fileEditorBuffers = { ...fileEditorBuffers };
      delete fileEditorBuffers[id];
    }
    fileWindows = fileWindows.map(([windowId, window]) =>
      windowId === id ? [windowId, next] : [windowId, window],
    );

    if (!nextEditorPath) {
      pendingFileEditorUpdates.delete(id);
      srocket?.send({ updateFileWindow: [id, pageId, next] });
      return;
    }

    void (async () => {
      const stream = randomEncryptedStream();
      const data = await encrypt.segment(
        stream,
        0n,
        new TextEncoder().encode(editorContent),
      );
      if (fileEditorUpdateVersions[id] !== version) return;
      const latest = fileWindows.find(([windowId]) => windowId === id)?.[1];
      if (
        !latest ||
        latest.pageId !== pageId ||
        latest.editorPath !== nextEditorPath
      )
        return;
      const encrypted: WsFileWindow = {
        ...latest,
        editorStream: stream,
        editorData: data,
      };
      pendingFileEditorUpdates.delete(id);
      fileEditorBuffers = {
        ...fileEditorBuffers,
        [id]: { path: nextEditorPath, stream, content: editorContent },
      };
      fileWindows = fileWindows.map(([windowId, window]) =>
        windowId === id ? [windowId, encrypted] : [windowId, window],
      );
      srocket?.send({ updateFileWindow: [id, pageId, encrypted] });
    })().catch((cause) => {
      pendingFileEditorUpdates.delete(id);
      console.warn("Could not encrypt a shared file editor buffer.", cause);
    });
  }

  function bringFileWindowToFront(id: number, pageId: number) {
    if (fileWindows.at(-1)?.[0] === id) return;
    srocket?.send({ updateFileWindow: [id, pageId, null] });
  }

  function openFileWindow(
    shellId: number,
    pageId: number,
    path: string,
    title: string,
  ) {
    const existing = fileWindows.find(
      ([, window]) => window.shellId === shellId && window.pageId === pageId,
    );
    if (existing) {
      bringFileWindowToFront(existing[0], pageId);
      return;
    }
    const source = terminalCanvasRect(shellId);
    const rect = source
      ? nextCanvasRectNear(source, 1_040, 680, pageId)
      : nextCanvasRect(1_040, 680);
    srocket?.send({
      createFileWindow: [
        shellId,
        pageId,
        path || ".",
        title,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
      ],
    });
  }

  function handleDuplicate(
    sourceId: number,
    location: {
      workingDirectory: string;
      workingDirectoryHost: string;
      initialWorkingDirectoryHost: string;
    },
  ) {
    if (!hasWriteAccess || shells.length >= 100) return;
    const source = shells.find(([id]) => id === sourceId)?.[1];
    if (!source) return;
    const wrapper = termWrappers[sourceId];
    const width =
      source.width || wrapper?.clientWidth / zoom || TERM_INITIAL_WIDTH;
    const height =
      source.height || wrapper?.clientHeight / zoom || TERM_INITIAL_HEIGHT;
    const sourceRect = terminalCanvasRect(sourceId);
    const rect = sourceRect
      ? nextCanvasRectNear(sourceRect, width, height, source.pageId)
      : nextCanvasRect(width, height);
    srocket?.send({
      cloneWindowedAt: [
        sourceId,
        location.workingDirectory,
        location.workingDirectoryHost,
        location.initialWorkingDirectoryHost,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        source.rows,
        source.cols,
        source.pageId,
        source.theme || $settings.theme,
      ],
    });
  }

  function handleCreateAt(
    sourceId: number,
    pageId: number,
    path: string,
    sourceRect: CanvasItemRect,
  ) {
    if (!hasWriteAccess || shells.length >= 100 || !path) return;
    const rect = nextCanvasRectNear(
      sourceRect,
      TERM_INITIAL_WIDTH,
      TERM_INITIAL_HEIGHT,
      pageId,
    );
    srocket?.send({
      createAt: [
        sourceId,
        path,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        TERM_INITIAL_ROWS,
        TERM_INITIAL_COLS,
        pageId,
        $settings.theme,
      ],
    });
  }

  $: canvasSearchItems = [
    ...shells.map(([shellId, winsize]): CanvasSearchItem => ({
      id: shellId,
      kind: "terminal",
      pageId: winsize.pageId,
      pageName: pageName(winsize.pageId),
      title: winsize.title || terminalTitles[shellId] || "Remote Terminal",
      content: terminalTitles[shellId] || "",
    })),
    ...notes.map(([noteId, note]): CanvasSearchItem => ({
      id: noteId,
      kind: "note",
      pageId: note.pageId,
      pageName: pageName(note.pageId),
      title: noteTitle(noteId, note),
      content: note.text,
    })),
  ];

  async function selectCanvasItem(item: CanvasSearchItem) {
    const entry =
      item.kind === "terminal"
        ? shells.find(([id]) => id === item.id)
        : notes.find(([id]) => id === item.id);
    if (!entry) return;
    searchOpen = false;
    switchPage(item.pageId);
    await tick();
    const state = entry[1];
    await touchZoom.moveTo([state.x, state.y], INITIAL_ZOOM);
    if (item.kind === "terminal") {
      srocket?.send({ move: [item.id, item.pageId, null] });
    } else {
      srocket?.send({ updateNote: [item.id, item.pageId, null] });
    }
    const textarea =
      item.kind === "terminal"
        ? termElements[item.id]?.querySelector("textarea")
        : noteWrappers[item.id]?.querySelector("textarea");
    if (textarea instanceof HTMLTextAreaElement) textarea.focus();
  }

  function navigateToTerminal(shellId: number) {
    const shell = shells.find(([id]) => id === shellId)?.[1];
    if (!shell) return;
    void selectCanvasItem({
      id: shellId,
      kind: "terminal",
      pageId: shell.pageId,
      pageName: pageName(shell.pageId),
      title: terminalTitle(shellId, shell),
      content: terminalTitles[shellId] || "",
    });
  }

  function navigateToNote(noteId: number) {
    const note = notes.find(([id]) => id === noteId)?.[1];
    if (!note) return;
    void selectCanvasItem({
      id: noteId,
      kind: "note",
      pageId: note.pageId,
      pageName: pageName(note.pageId),
      title: noteTitle(noteId, note),
      content: note.text,
    });
  }

  async function navigateToFileWindow(windowId: number) {
    const fileWindow = fileWindows.find(([id]) => id === windowId)?.[1];
    if (!fileWindow) return;
    switchPage(fileWindow.pageId);
    await tick();
    await touchZoom.moveTo([fileWindow.x, fileWindow.y], INITIAL_ZOOM);
    bringFileWindowToFront(windowId, fileWindow.pageId);
    fileWrappers[windowId]
      ?.querySelector<HTMLElement>(".cm-content, [data-canvas-file-editor]")
      ?.focus({ preventScroll: true });
  }

  function navigateCanvasRelation(item: CanvasRelationItem) {
    if (item.kind === "terminal") navigateToTerminal(item.id);
    else if (item.kind === "note") navigateToNote(item.id);
    else void navigateToFileWindow(item.id);
  }

  function insertParagraphsIntoNote(
    targetNoteId: number,
    insertedParagraphs: readonly string[],
    insertIndex?: number,
  ): TextInsertResult {
    const entry = notes.find(([id]) => id === targetNoteId);
    if (!entry)
      return { ok: false, message: "The target note no longer exists." };
    if (noteEditors[targetNoteId]) {
      return {
        ok: false,
        message: `${noteTitle(targetNoteId, entry[1])} is currently being edited.`,
      };
    }
    const paragraphs = [...entry[1].paragraphs];
    const index = Math.max(
      0,
      Math.min(insertIndex ?? paragraphs.length, paragraphs.length),
    );
    paragraphs.splice(index, 0, ...insertedParagraphs);
    const projectedText = paragraphs.join("\n");
    if (paragraphs.length > 500 || projectedText.length > 10_000) {
      return { ok: false, message: "The target note is too large." };
    }
    const next = { ...entry[1], paragraphs, text: projectedText };
    notes = notes.map(([id, note]) =>
      id === targetNoteId ? [id, next] : [id, note],
    );
    srocket?.send({
      updateNote: [targetNoteId, entry[1].pageId, next],
    });
    return { ok: true };
  }

  function sendNoteParagraph(
    noteId: number,
    detail: {
      paragraphs: string[];
      text: string;
      target: "all" | "notes" | "terminals" | "terminals-execute" | "files";
    },
  ) {
    if (!hasWriteAccess) return;
    const note = notes.find(([id]) => id === noteId)?.[1];
    if (!note) return;
    if (detail.paragraphs.every((paragraph) => !paragraph)) {
      makeToast({
        kind: "info",
        message: "The selected paragraphs are empty.",
      });
      return;
    }
    const sendToNotes = detail.target === "all" || detail.target === "notes";
    const sendToTerminals =
      detail.target === "all" ||
      detail.target === "terminals" ||
      detail.target === "terminals-execute";
    const sendToFiles = detail.target === "all" || detail.target === "files";
    const targetNoteIds = sendToNotes ? associatedNoteIds(noteId) : [];
    const shellIds = sendToTerminals ? note.linkedShellIds : [];
    const fileWindowIds = sendToFiles ? note.linkedFileWindowIds : [];
    const targetCount =
      targetNoteIds.length + shellIds.length + fileWindowIds.length;
    if (targetCount === 0) {
      const targetName =
        detail.target === "notes"
          ? "notes"
          : detail.target === "files"
            ? "file editors"
            : detail.target.startsWith("terminals")
              ? "terminals"
              : "canvas items";
      makeToast({
        kind: "info",
        message: `This note has no linked ${targetName}.`,
      });
      return;
    }

    let sent = 0;
    const failures: string[] = [];
    for (const targetNoteId of targetNoteIds) {
      const result = insertParagraphsIntoNote(targetNoteId, detail.paragraphs);
      if (result.ok) sent += 1;
      else failures.push(result.message || "A linked note was unavailable.");
    }
    for (const shellId of shellIds) {
      const sender = terminalTextSenders[shellId];
      if (sender) {
        sender(detail.text, detail.target === "terminals-execute");
        sent += 1;
      } else {
        failures.push(`Terminal #${shellId} is unavailable.`);
      }
    }
    for (const windowId of fileWindowIds) {
      const result = fileTextSenders[windowId]?.(detail.text) ?? {
        ok: false,
        message: `File editor #${windowId} is unavailable.`,
      };
      if (result.ok) sent += 1;
      else failures.push(result.message || "A file editor was unavailable.");
    }

    if (failures.length) {
      makeToast({
        kind: "error",
        message: `${sent ? `Sent to ${sent} target${sent === 1 ? "" : "s"}; ` : ""}${failures[0]}${failures.length > 1 ? ` (+${failures.length - 1} more)` : ""}`,
      });
    } else {
      makeToast({
        kind: "success",
        message: `Sent to ${sent} linked target${sent === 1 ? "" : "s"}.`,
      });
    }
  }

  function noteParagraphDropIndex(noteElement: HTMLElement, clientY: number) {
    const rows = Array.from(
      noteElement.querySelectorAll<HTMLElement>("[data-note-paragraph-index]"),
    );
    for (const row of rows) {
      const bounds = row.getBoundingClientRect();
      const index = Number(row.dataset.noteParagraphIndex);
      if (clientY < bounds.top + bounds.height / 2) return index;
    }
    return rows.length;
  }

  function cancelParagraphDropPreview(target = paragraphDropTarget) {
    if (target?.kind === "file") fileDropPreviewCancelers[target.id]?.();
  }

  function resolveParagraphDropTarget(event: DragEvent) {
    if (!paragraphDrag) return null;
    const element = document.elementFromPoint(event.clientX, event.clientY);
    if (!(element instanceof Element)) return null;

    const terminal = element.closest<HTMLElement>("[data-canvas-terminal]");
    if (terminal) {
      return {
        kind: "terminal" as const,
        id: Number(terminal.dataset.canvasTerminal),
      };
    }

    const fileEditor = element.closest<HTMLElement>(
      "[data-canvas-file-editor]",
    );
    const fileWindow = fileEditor?.closest<HTMLElement>(
      "[data-canvas-file-window]",
    );
    if (fileWindow) {
      const id = Number(fileWindow.dataset.canvasFileWindow);
      return {
        kind: "file" as const,
        id,
        fileReady:
          fileDropPreviewers[id]?.({ x: event.clientX, y: event.clientY }) ??
          false,
      };
    }

    const noteElement = element.closest<HTMLElement>("[data-canvas-note-id]");
    if (noteElement) {
      const id = Number(noteElement.dataset.canvasNoteId);
      return {
        kind: "note" as const,
        id,
        noteInsertIndex: noteParagraphDropIndex(noteElement, event.clientY),
      };
    }
    return null;
  }

  function handleParagraphDragOver(event: DragEvent) {
    if (!paragraphDrag) return;
    const next = resolveParagraphDropTarget(event);
    if (
      paragraphDropTarget?.kind === "file" &&
      (next?.kind !== "file" || next.id !== paragraphDropTarget.id)
    ) {
      cancelParagraphDropPreview();
    }
    paragraphDropTarget = next;
    if (!next) return;
    event.preventDefault();
    const reordering =
      next.kind === "note" && next.id === paragraphDrag.sourceNoteId;
    if (!reordering) event.stopPropagation();
    if (event.dataTransfer)
      event.dataTransfer.dropEffect = reordering ? "move" : "copy";
  }

  function finishParagraphDrag() {
    cancelParagraphDropPreview();
    paragraphDropTarget = null;
    paragraphDrag = null;
  }

  function handleParagraphDrop(event: DragEvent) {
    if (!paragraphDrag) return;
    const target = resolveParagraphDropTarget(event) ?? paragraphDropTarget;
    const text = paragraphDrag.text;
    const paragraphCount = paragraphDrag.paragraphs.length;
    if (!target) {
      finishParagraphDrag();
      return;
    }
    if (target.kind === "note" && target.id === paragraphDrag.sourceNoteId) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    let result: TextInsertResult;
    if (target.kind === "terminal") {
      const sender = terminalTextSenders[target.id];
      if (sender) {
        sender(text, false);
        result = { ok: true };
      } else {
        result = { ok: false, message: "The target terminal is unavailable." };
      }
    } else if (target.kind === "note") {
      result = insertParagraphsIntoNote(
        target.id,
        paragraphDrag.paragraphs,
        target.noteInsertIndex,
      );
    } else {
      result = fileTextSenders[target.id]?.(text, {
        x: event.clientX,
        y: event.clientY,
      }) ?? {
        ok: false,
        message: "The target file editor is unavailable.",
      };
    }
    finishParagraphDrag();
    makeToast({
      kind: result.ok ? "success" : "error",
      message: result.ok
        ? `${paragraphCount === 1 ? "Paragraph" : "Paragraphs"} copied to the target.`
        : result.message || "Could not copy the paragraph.",
    });
  }

  async function handleInput(id: number, pageId: number, data: Uint8Array) {
    if (counter === 0n) {
      // On the first call, initialize the counter to a random 64-bit integer.
      const array = new Uint8Array(8);
      crypto.getRandomValues(array);
      counter = new DataView(array.buffer).getBigUint64(0);
    }
    const offset = counter;
    counter += BigInt(data.length); // Must increment before the `await`.
    const encrypted = await encrypt.segment(0x200000000n, offset, data);
    srocket?.send({ data: [id, pageId, encrypted, offset] });
  }

  let terminalInputQueue = Promise.resolve();
  function queueTerminalInput(id: number, pageId: number, data: Uint8Array) {
    terminalInputQueue = terminalInputQueue
      .then(() => handleInput(id, pageId, data))
      .catch((error) => {
        makeToast({
          kind: "error",
          message:
            error instanceof Error
              ? error.message
              : "Could not send terminal input.",
        });
      });
  }

  const imageUploadChunkBytes = 64 << 10;
  let imageUploadQueue = Promise.resolve();

  async function uploadImage(id: number, pageId: number, file: File) {
    if (!srocket?.connected) {
      throw new Error("Connect to the daemon before uploading an image.");
    }
    const uploadId = randomHex(16);
    const streamNum = randomEncryptedStream();
    const totalSize = BigInt(file.size);

    for (let offset = 0; offset < file.size; offset += imageUploadChunkBytes) {
      if (!srocket?.connected) {
        throw new Error("Image upload stopped because the connection closed.");
      }
      const plaintext = new Uint8Array(
        await file.slice(offset, offset + imageUploadChunkBytes).arrayBuffer(),
      );
      const encrypted = await encrypt.segment(
        streamNum,
        BigInt(offset),
        plaintext,
      );
      const complete = offset + plaintext.length === file.size;
      srocket.send({
        uploadImage: [
          id,
          pageId,
          uploadId,
          file.type,
          totalSize,
          streamNum,
          BigInt(offset),
          encrypted,
          complete,
        ],
      });
    }
    makeToast({
      kind: "success",
      message: "Image sent. Its daemon cache path will be inserted here.",
    });
  }

  function queueImageUpload(id: number, pageId: number, file: File) {
    imageUploadQueue = imageUploadQueue
      .then(() => uploadImage(id, pageId, file))
      .catch((error) => {
        const message =
          error instanceof Error ? error.message : "Image upload failed.";
        makeToast({ kind: "error", message });
      });
  }

  // Stupid hack to preserve input focus when terminals are reordered.
  // See: https://github.com/sveltejs/svelte/issues/3973
  let activeElement: Element | null = null;

  beforeUpdate(() => {
    activeElement = document.activeElement;
  });

  afterUpdate(() => {
    if (activeElement instanceof HTMLElement)
      activeElement.focus({ preventScroll: true });
    for (const [id, shell] of shells) {
      if (shell.pageId !== activePageId) continue;
      const writer = writers[id];
      if (writer && replayedWriters[id] !== writer) {
        replayedWriters[id] = writer;
        writer(readTerminalHistory(id), true);
      }
    }
  });

  // Global mouse handler logic follows, attached to the window element for smoothness.
  onMount(() => {
    // 50 milliseconds between successive terminal move updates.
    const sendMove = throttle((message: WsClient) => {
      srocket?.send(message);
    }, 50);

    // 80 milliseconds between successive cursor updates.
    const sendCursor = throttle((message: WsClient) => {
      srocket?.send(message);
    }, 80);

    function handleMouse(event: MouseEvent) {
      updateMarqueeSelection(event);
      updateCanvasGroupMove(event);

      if (moving !== -1 && !movingIsDone) {
        const distance = Math.hypot(
          event.clientX - movingStartClient[0],
          event.clientY - movingStartClient[1],
        );
        if (!movingDidMove && distance < 3) return;
        movingDidMove = true;
        pendingCanvasTitleFocus = null;
        const [x, y] = normalizePosition(event);
        movingSize = {
          ...movingSize,
          x: snapLeadingEdge(Math.round(x - movingOrigin[0])),
          y: snapLeadingEdge(Math.round(y - movingOrigin[1])),
        };
        sendMove({ move: [moving, movingSize.pageId, movingSize] });
        updateCanvasPageDropTarget(event, true);
      }

      if (movingNote !== -1) {
        const distance = Math.hypot(
          event.clientX - movingNoteStartClient[0],
          event.clientY - movingNoteStartClient[1],
        );
        if (!movingNoteDidMove && distance < 3) return;
        movingNoteDidMove = true;
        pendingCanvasTitleFocus = null;
        const [x, y] = normalizePosition(event);
        movingNoteState = {
          ...movingNoteState,
          x: snapLeadingEdge(Math.round(x - movingNoteOrigin[0])),
          y: snapLeadingEdge(Math.round(y - movingNoteOrigin[1])),
        };
        updateCanvasPageDropTarget(event, true);
      }

      if (movingFile !== -1) {
        const distance = Math.hypot(
          event.clientX - movingFileStartClient[0],
          event.clientY - movingFileStartClient[1],
        );
        if (!movingFileDidMove && distance < 3) return;
        movingFileDidMove = true;
        pendingCanvasTitleFocus = null;
        const [x, y] = normalizePosition(event);
        movingFileState = {
          ...movingFileState,
          x: snapLeadingEdge(Math.round(x - movingFileOrigin[0])),
          y: snapLeadingEdge(Math.round(y - movingFileOrigin[1])),
        };
        updateCanvasPageDropTarget(event, true);
      }

      if (resizing !== -1) {
        const [x, y] = normalizePosition(event);
        const dx = x - resizingStartPointer[0];
        const dy = y - resizingStartPointer[1];
        const [startLeft, startTop, startRight, startBottom] =
          resizingStartEdges;
        const left = resizesWest(resizingDirection)
          ? snapLeadingEdge(startLeft + dx)
          : startLeft;
        const top = resizesNorth(resizingDirection)
          ? snapLeadingEdge(startTop + dy)
          : startTop;
        const right = resizesEast(resizingDirection)
          ? snapTrailingEdge(startRight + dx)
          : startRight;
        const bottom = resizesSouth(resizingDirection)
          ? snapTrailingEdge(startBottom + dy)
          : startBottom;
        const resized = constrainTerminalResize({
          direction: resizingDirection,
          left,
          top,
          right,
          bottom,
          startWidth: resizingStartPixels[0],
          startHeight: resizingStartPixels[1],
          startRows: resizingStartSize.rows,
          startCols: resizingStartSize.cols,
          cellWidth: resizingCanvasCell[0],
          cellHeight: resizingCanvasCell[1],
        });
        if (
          resized.rows !== resizingSize.rows ||
          resized.cols !== resizingSize.cols ||
          resized.width !== resizingSize.width ||
          resized.height !== resizingSize.height ||
          resized.x !== resizingSize.x ||
          resized.y !== resizingSize.y
        ) {
          resizingSize = {
            ...resizingSize,
            x: resized.x,
            y: resized.y,
            width: resized.width,
            height: resized.height,
            rows: resized.rows,
            cols: resized.cols,
          };
          srocket?.send({
            move: [resizing, resizingSize.pageId, resizingSize],
          });
        }
      }

      if (resizingNote !== -1) {
        const [x, y] = normalizePosition(event);
        const dx = x - resizingNoteStartPointer[0];
        const dy = y - resizingNoteStartPointer[1];
        const startRight =
          resizingNoteStartState.x + resizingNoteStartState.width;
        const startBottom =
          resizingNoteStartState.y + resizingNoteStartState.height;
        let left = resizesWest(resizingNoteDirection)
          ? snapLeadingEdge(resizingNoteStartState.x + dx)
          : resizingNoteStartState.x;
        let top = resizesNorth(resizingNoteDirection)
          ? snapLeadingEdge(resizingNoteStartState.y + dy)
          : resizingNoteStartState.y;
        let right = resizesEast(resizingNoteDirection)
          ? snapTrailingEdge(startRight + dx)
          : startRight;
        let bottom = resizesSouth(resizingNoteDirection)
          ? snapTrailingEdge(startBottom + dy)
          : startBottom;
        if (right - left < 240) {
          if (resizesWest(resizingNoteDirection)) left = right - 240;
          else right = left + 240;
        }
        if (bottom - top < 160) {
          if (resizesNorth(resizingNoteDirection)) top = bottom - 160;
          else bottom = top + 160;
        }
        resizingNoteState = {
          ...resizingNoteState,
          x: Math.round(left),
          y: Math.round(top),
          width: Math.round(right - left),
          height: Math.round(bottom - top),
        };
      }

      if (resizingFile !== -1) {
        const [x, y] = normalizePosition(event);
        const dx = x - resizingFileStartPointer[0];
        const dy = y - resizingFileStartPointer[1];
        const startRight =
          resizingFileStartState.x + resizingFileStartState.width;
        const startBottom =
          resizingFileStartState.y + resizingFileStartState.height;
        let left = resizesWest(resizingFileDirection)
          ? snapLeadingEdge(resizingFileStartState.x + dx)
          : resizingFileStartState.x;
        let top = resizesNorth(resizingFileDirection)
          ? snapLeadingEdge(resizingFileStartState.y + dy)
          : resizingFileStartState.y;
        let right = resizesEast(resizingFileDirection)
          ? snapTrailingEdge(startRight + dx)
          : startRight;
        let bottom = resizesSouth(resizingFileDirection)
          ? snapTrailingEdge(startBottom + dy)
          : startBottom;
        if (right - left < 600) {
          if (resizesWest(resizingFileDirection)) left = right - 600;
          else right = left + 600;
        }
        if (bottom - top < 360) {
          if (resizesNorth(resizingFileDirection)) top = bottom - 360;
          else bottom = top + 360;
        }
        if (right - left > 4_000) {
          if (resizesWest(resizingFileDirection)) left = right - 4_000;
          else right = left + 4_000;
        }
        if (bottom - top > 4_000) {
          if (resizesNorth(resizingFileDirection)) top = bottom - 4_000;
          else bottom = top + 4_000;
        }
        const fileWidth = Math.round(right - left);
        resizingFileState = {
          ...resizingFileState,
          x: Math.round(left),
          y: Math.round(top),
          width: fileWidth,
          height: Math.round(bottom - top),
          sidebarWidth: Math.min(
            resizingFileState.sidebarWidth || Math.round(fileWidth * 0.32),
            Math.max(200, fileWidth - 320),
          ),
        };
      }

      const [cursorX, cursorY] = normalizePosition(event);
      sendCursor({ setCursor: [activePageId, [cursorX, cursorY]] });
    }

    function handleMouseEnd(event: MouseEvent) {
      finishCanvasGroupMove();

      if (moving !== -1) {
        const movedId = moving;
        sendMove.cancel();
        if (!movingDidMove) {
          moving = -1;
          focusCanvasItem(canvasItemKey("terminal", movedId));
        } else if (canvasDropPageId !== null) {
          const key = canvasItemKey("terminal", movedId);
          const overrides = new Map<CanvasItemKey, [number, number]>([
            [key, [movingStartSize.x, movingStartSize.y]],
          ]);
          moving = -1;
          movingIsDone = false;
          moveCanvasItemsToPage([key], canvasDropPageId, overrides);
        } else {
          movingIsDone = true;
          srocket?.send({ move: [movedId, movingSize.pageId, movingSize] });
        }
        movingDidMove = false;
      }

      if (resizing !== -1) {
        const resizedId = resizing;
        srocket?.send({
          move: [resizedId, resizingSize.pageId, resizingSize],
        });
        shells = shells.map(([id, winsize]) =>
          id === resizedId ? [id, resizingSize] : [id, winsize],
        );
        resizing = -1;
      }

      if (movingNote !== -1) {
        const movedId = movingNote;
        if (!movingNoteDidMove) {
          movingNote = -1;
          focusCanvasItem(canvasItemKey("note", movedId));
        } else if (canvasDropPageId !== null) {
          const key = canvasItemKey("note", movedId);
          const overrides = new Map<CanvasItemKey, [number, number]>([
            [key, [movingNoteStartState.x, movingNoteStartState.y]],
          ]);
          movingNote = -1;
          moveCanvasItemsToPage([key], canvasDropPageId, overrides);
        } else {
          notes = notes.map(([id, note]) =>
            id === movedId ? [id, movingNoteState] : [id, note],
          );
          srocket?.send({
            updateNote: [movedId, movingNoteState.pageId, movingNoteState],
          });
          void tick().then(() => {
            if (movingNote === movedId) movingNote = -1;
          });
        }
        movingNoteDidMove = false;
      }

      if (resizingNote !== -1) {
        const resizedId = resizingNote;
        notes = notes.map(([id, note]) =>
          id === resizedId ? [id, resizingNoteState] : [id, note],
        );
        srocket?.send({
          updateNote: [resizedId, resizingNoteState.pageId, resizingNoteState],
        });
        void tick().then(() => {
          if (resizingNote === resizedId) resizingNote = -1;
        });
      }

      if (movingFile !== -1) {
        const movedId = movingFile;
        if (!movingFileDidMove) {
          movingFile = -1;
          focusCanvasItem(canvasItemKey("file", movedId));
        } else if (canvasDropPageId !== null) {
          const key = canvasItemKey("file", movedId);
          const overrides = new Map<CanvasItemKey, [number, number]>([
            [key, [movingFileStartState.x, movingFileStartState.y]],
          ]);
          movingFile = -1;
          moveCanvasItemsToPage([key], canvasDropPageId, overrides);
        } else {
          fileWindows = fileWindows.map(([id, window]) =>
            id === movedId ? [id, movingFileState] : [id, window],
          );
          srocket?.send({
            updateFileWindow: [
              movedId,
              movingFileState.pageId,
              movingFileState,
            ],
          });
          void tick().then(() => {
            if (movingFile === movedId) movingFile = -1;
          });
        }
        movingFileDidMove = false;
      }

      if (resizingFile !== -1) {
        const resizedId = resizingFile;
        fileWindows = fileWindows.map(([id, window]) =>
          id === resizedId ? [id, resizingFileState] : [id, window],
        );
        srocket?.send({
          updateFileWindow: [
            resizedId,
            resizingFileState.pageId,
            resizingFileState,
          ],
        });
        void tick().then(() => {
          if (resizingFile === resizedId) resizingFile = -1;
        });
      }

      if (event.type === "mouseleave") {
        sendCursor.cancel();
        srocket?.send({ setCursor: [activePageId, null] });
      }
      finishCanvasSelection();
      if (
        canvasGroupMove === null &&
        moving === -1 &&
        movingNote === -1 &&
        movingFile === -1
      ) {
        canvasDropPageId = null;
        canvasDropPreviewOffsets = {};
      }
      if (event.button === 2 && pendingCanvasContextMenu) {
        if (suppressMarqueeContextMenu) {
          suppressMarqueeContextMenu = false;
        } else if (!touchZoom?.consumeContextMenuSuppression()) {
          canvasContextMenuX = pendingCanvasContextMenu.x;
          canvasContextMenuY = pendingCanvasContextMenu.y;
          canvasContextPosition = pendingCanvasContextMenu.position;
          canvasContextMenuOpen = true;
        }
        pendingCanvasContextMenu = null;
      }
    }

    window.addEventListener("mousemove", handleMouse);
    window.addEventListener("mouseup", handleMouseEnd);
    document.body.addEventListener("mouseleave", handleMouseEnd);
    return () => {
      window.removeEventListener("mousemove", handleMouse);
      window.removeEventListener("mouseup", handleMouseEnd);
      document.body.removeEventListener("mouseleave", handleMouseEnd);
    };
  });

  let focused: [number, number][] = [];
  $: setFocus(focused);

  // Wait a small amount of time, since blur events happen before focus events.
  const setFocus = debounce((focused: [number, number][]) => {
    srocket?.send({ setFocus: focused[0] ?? null });
  }, 20);
</script>

<svelte:window
  on:mousedown|capture={handleWindowMouseDownCapture}
  on:contextmenu|preventDefault={handlePageContextMenu}
  on:keydown={handleRelationshipKeydown}
  on:dragover|capture={handleParagraphDragOver}
  on:drop|capture={handleParagraphDrop}
  on:dragend={finishParagraphDrag}
/>

<!-- Wheel handler stops native macOS Chrome zooming on pinch. -->
<main
  class="p-8"
  style:cursor={linkingNoteId !== null
    ? "crosshair"
    : resizing !== -1
      ? resizeCursor(resizingDirection)
      : resizingNote !== -1
        ? resizeCursor(resizingNoteDirection)
        : resizingFile !== -1
          ? resizeCursor(resizingFileDirection)
          : undefined}
  on:wheel={(event) => event.preventDefault()}
>
  <SessionChrome
    {connected}
    {connectionStatus}
    connectionDetail={exitReason}
    {failureStage}
    {newMessages}
    {hasWriteAccess}
    profiles={sshProfiles}
    {users}
    {searchOpen}
    searchItems={canvasSearchItems}
    {showNetworkInfo}
    serverLatency={integerMedian(serverLatencies)}
    shellLatency={integerMedian(shellLatencies)}
    {showChat}
    {userId}
    {chatMessages}
    {settingsOpen}
    {serverVersion}
    {daemonVersion}
    {pages}
    {activePageId}
    {canvasDropPageId}
    on:create={() => handleCreate()}
    on:createSsh={(event) => handleCreateSsh(event.detail)}
    on:saveSshProfile={(event) =>
      srocket?.send({ upsertSshProfile: event.detail })}
    on:deleteSshProfile={(event) =>
      srocket?.send({ deleteSshProfile: event.detail })}
    on:createNote={() => handleCreateNote()}
    on:toggleChat={() => {
      showChat = !showChat;
      newMessages = false;
    }}
    on:openSettings={() => (settingsOpen = true)}
    on:toggleSearch={() => (searchOpen = !searchOpen)}
    on:selectSearch={(event) => selectCanvasItem(event.detail)}
    on:toggleNetwork={() => (showNetworkInfo = !showNetworkInfo)}
    on:chat={(event) => srocket?.send({ chat: event.detail })}
    on:closeChat={() => (showChat = false)}
    on:closeSettings={() => (settingsOpen = false)}
    on:selectPage={(event) => switchPage(event.detail)}
    on:createPage={() => {
      selectCreatedPage = true;
      srocket?.send({ createPage: "" });
    }}
    on:renamePage={(event) =>
      srocket?.send({
        renamePage: [event.detail.id, event.detail.name],
      })}
  />

  <CanvasContextMenu
    open={canvasContextMenuOpen}
    x={canvasContextMenuX}
    y={canvasContextMenuY}
    {connected}
    {hasWriteAccess}
    profiles={sshProfiles}
    on:close={() => (canvasContextMenuOpen = false)}
    on:create={() => handleCreate(canvasContextPosition)}
    on:createSsh={(event) =>
      handleCreateSsh(event.detail, canvasContextPosition)}
    on:saveSshProfile={(event) =>
      srocket?.send({ upsertSshProfile: event.detail })}
    on:deleteSshProfile={(event) =>
      srocket?.send({ deleteSshProfile: event.detail })}
    on:createNote={() => handleCreateNote(canvasContextPosition)}
    on:search={() => {
      settingsOpen = false;
      searchOpen = true;
    }}
    on:settings={() => {
      searchOpen = false;
      settingsOpen = true;
    }}
  />

  <!--
    Dotted circle background appears underneath the rest of the elements, but
    moves and zooms with the fabric of the canvas.
  -->
  <div
    class="absolute inset-0 -z-10"
    style:background-image="radial-gradient(var(--canvas-grid-dot) {zoom}px,
    transparent 0)"
    style:background-size="{GRID_SIZE * zoom}px {GRID_SIZE * zoom}px"
    style:background-position="calc({zoom * 50}vw - {zoom *
      (CONSTANT_OFFSET_LEFT + center[0])}px) calc({zoom * 50}vh - {zoom *
      (CONSTANT_OFFSET_TOP + center[1])}px)"
  ></div>

  <div class="absolute inset-0 overflow-hidden touch-none" bind:this={fabricEl}>
    {#if selectionMarquee?.moved}
      {@const selectionRect = marqueeRect(
        selectionMarquee.startX,
        selectionMarquee.startY,
        selectionMarquee.currentX,
        selectionMarquee.currentY,
      )}
      <div
        class="selection-marquee pointer-events-none absolute z-20"
        style:left={`${selectionRect.left - selectionMarquee.canvasLeft}px`}
        style:top={`${selectionRect.top - selectionMarquee.canvasTop}px`}
        style:width={`${selectionRect.right - selectionRect.left}px`}
        style:height={`${selectionRect.bottom - selectionRect.top}px`}
        aria-hidden="true"
      ></div>
    {/if}

    {#each shells.filter(([, winsize]) => winsize.pageId === activePageId) as [id, winsize] (id)}
      {@const ws =
        groupTerminalStates[id] ??
        (id === moving ? movingSize : id === resizing ? resizingSize : winsize)}
      {@const terminalKey = canvasItemKey("terminal", id)}
      {@const terminalDropOffset = canvasDropPreviewOffsets[terminalKey]}
      <div
        class="absolute"
        data-canvas-terminal={id}
        class:canvas-active={focusedTerminalId === id}
        class:canvas-selected={selectedCanvasItems.includes(terminalKey)}
        class:canvas-page-drop-preview={terminalDropOffset !== undefined}
        class:canvas-interacting={groupTerminalStates[id] !== undefined ||
          moving === id ||
          resizing === id}
        class:canvas-fullscreen={fullscreenItems[`terminal:${id}`]}
        class:canvas-floating={terminalFloating[id]}
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        style:--canvas-drop-x={terminalDropOffset
          ? `${terminalDropOffset[0]}px`
          : "0px"}
        style:--canvas-drop-y={terminalDropOffset
          ? `${terminalDropOffset[1]}px`
          : "0px"}
        transition:fade|local
        use:slide={{
          x: ws.x,
          y: ws.y,
          center,
          zoom,
          immediate:
            groupTerminalStates[id] !== undefined ||
            id === moving ||
            id === resizing,
        }}
        bind:this={termWrappers[id]}
      >
        <XTerm
          rows={ws.rows}
          cols={ws.cols}
          windowWidth={ws.width}
          windowHeight={ws.height}
          canvasZoom={zoom}
          title={ws.title}
          background={ws.background}
          colorTheme={ws.theme}
          opacity={ws.opacity}
          fullscreen={fullscreenItems[`terminal:${id}`] ?? false}
          linkedNotes={notes
            .filter(([, note]) => note.linkedShellIds.includes(id))
            .map(([noteId, note]) => ({
              id: noteId,
              label: noteTitle(noteId, note),
              kind: "note" as const,
            }))}
          linkedHighlight={focusedNoteId !== null &&
            (notes
              .find(([noteId]) => noteId === focusedNoteId)?.[1]
              .linkedShellIds.includes(id) ??
              false)}
          paragraphDropActive={paragraphDropTarget?.kind === "terminal" &&
            paragraphDropTarget.id === id}
          {hasWriteAccess}
          bind:write={writers[id]}
          bind:sendText={terminalTextSenders[id]}
          bind:termEl={termElements[id]}
          on:data={({ detail: data }) =>
            hasWriteAccess && queueTerminalInput(id, ws.pageId, data)}
          on:uploadImage={({ detail: file }) =>
            hasWriteAccess && queueImageUpload(id, ws.pageId, file)}
          on:close={() => srocket?.send({ close: [id, ws.pageId] })}
          on:duplicate={(event) => handleDuplicate(id, event.detail)}
          on:toggleFullscreen={() => toggleFullscreen("terminal", id)}
          on:navigateNote={(event) => navigateCanvasRelation(event.detail)}
          on:unlinkNote={(event) =>
            removeCanvasRelation(event.detail.id, {
              id,
              kind: "terminal",
              label: terminalTitle(id, ws),
            })}
          on:openFiles={(event) => {
            openFileWindow(
              id,
              ws.pageId,
              event.detail,
              ws.title || terminalTitles[id] || `Terminal ${id}`,
            );
          }}
          on:appearance={(event) =>
            srocket?.send({
              move: [id, ws.pageId, { ...ws, ...event.detail }],
            })}
          on:floatingChange={(event) =>
            (terminalFloating = {
              ...terminalFloating,
              [id]: event.detail,
            })}
          on:title={(event) => {
            terminalTitles = { ...terminalTitles, [id]: event.detail };
          }}
          on:bringToFront={() => {
            if (!hasWriteAccess) return;
            showNetworkInfo = false;
            srocket?.send({ move: [id, ws.pageId, null] });
          }}
          on:startMove={({ detail: event }) => {
            if (
              event.button !== 0 ||
              !hasWriteAccess ||
              fullscreenItems[`terminal:${id}`]
            )
              return;
            if (startCanvasGroupMove("terminal", id, event)) return;
            const startingSize = ws;
            const [x, y] = normalizePosition(event);
            movingOrigin = [x - startingSize.x, y - startingSize.y];
            movingStartClient = [event.clientX, event.clientY];
            movingDidMove = false;
            movingStartSize = startingSize;
            movingSize = startingSize;
            movingIsDone = false;
            moving = id;
          }}
          on:focus={() => {
            clearCanvasSelection();
            if (!hasWriteAccess) return;
            focusedTerminalId = id;
            focusedNoteId = null;
            focusedFileWindowId = null;
            focused = [...focused, [id, ws.pageId]];
          }}
          on:blur={() => {
            if (focusedTerminalId === id) focusedTerminalId = null;
            focused = focused.filter(([focusedId]) => focusedId !== id);
          }}
        />

        <!-- User avatars -->
        <div class="absolute bottom-2.5 right-2.5 pointer-events-none">
          <Avatars
            users={users.filter(
              ([uid, user]) =>
                uid !== userId &&
                user.pageId === ws.pageId &&
                user.focus === id,
            )}
          />
        </div>

        <ResizeHandles
          disabled={!hasWriteAccess || fullscreenItems[`terminal:${id}`]}
          on:start={({ detail }) => {
            pendingCanvasSelection = null;
            clearCanvasSelection();
            const canvasEl = termElements[id].querySelector(".xterm-screen");
            if (canvasEl) {
              const screenRect = canvasEl.getBoundingClientRect();
              const wrapperRect = termWrappers[id].getBoundingClientRect();
              const canvasWidth = ws.width || wrapperRect.width / zoom;
              const canvasHeight = ws.height || wrapperRect.height / zoom;
              resizingStartPointer = normalizePosition(detail.event);
              resizingStartEdges = [
                ws.x,
                ws.y,
                ws.x + canvasWidth,
                ws.y + canvasHeight,
              ];
              resizingStartPixels = [canvasWidth, canvasHeight];
              resizingStartSize = ws;
              resizingCanvasCell = [
                screenRect.width / zoom / ws.cols,
                screenRect.height / zoom / ws.rows,
              ];
              resizingSize = ws;
              resizingDirection = detail.direction;
              resizing = id;
            }
          }}
        />
      </div>
    {/each}

    {#each notes.filter(([, note]) => note.pageId === activePageId) as [id, note] (id)}
      {@const displayNote =
        groupNoteStates[id] ??
        (id === movingNote
          ? movingNoteState
          : id === resizingNote
            ? resizingNoteState
            : note)}
      {@const noteKey = canvasItemKey("note", id)}
      {@const noteDropOffset = canvasDropPreviewOffsets[noteKey]}
      <div
        class="absolute"
        data-canvas-note-wrapper={id}
        class:canvas-active={focusedNoteId === id}
        class:canvas-selected={selectedCanvasItems.includes(noteKey)}
        class:canvas-page-drop-preview={noteDropOffset !== undefined}
        class:canvas-interacting={groupNoteStates[id] !== undefined ||
          movingNote === id ||
          resizingNote === id}
        class:canvas-fullscreen={fullscreenItems[`note:${id}`]}
        class:canvas-floating={noteFloating[id]}
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        style:--canvas-drop-x={noteDropOffset
          ? `${noteDropOffset[0]}px`
          : "0px"}
        style:--canvas-drop-y={noteDropOffset
          ? `${noteDropOffset[1]}px`
          : "0px"}
        style:opacity={displayNote.opacity / 100}
        transition:fade|local
        use:slide={{
          x: displayNote.x,
          y: displayNote.y,
          center,
          zoom,
          immediate:
            groupNoteStates[id] !== undefined ||
            id === movingNote ||
            id === resizingNote,
        }}
        bind:this={noteWrappers[id]}
      >
        <Note
          noteId={id}
          note={displayNote}
          {hasWriteAccess}
          {userId}
          editingBy={noteEditors[id]?.pageId === note.pageId
            ? noteEditors[id].userId
            : null}
          editingName={users.find(
            ([uid]) => uid === noteEditors[id]?.userId,
          )?.[1].name ?? ""}
          fullscreen={fullscreenItems[`note:${id}`] ?? false}
          linkedItems={[
            ...displayNote.linkedShellIds.flatMap((shellId) => {
              const shell = shells.find(([id]) => id === shellId)?.[1];
              return shell
                ? [
                    {
                      id: shellId,
                      label: terminalTitle(shellId, shell),
                      kind: "terminal" as const,
                    },
                  ]
                : [];
            }),
            ...associatedNoteIds(id).flatMap((linkedNoteId) => {
              const linkedNote = notes.find(
                ([noteId]) => noteId === linkedNoteId,
              )?.[1];
              return linkedNote
                ? [
                    {
                      id: linkedNoteId,
                      label: noteTitle(linkedNoteId, linkedNote),
                      kind: "note" as const,
                    },
                  ]
                : [];
            }),
            ...displayNote.linkedFileWindowIds.flatMap((windowId) => {
              const fileWindow = fileWindows.find(
                ([id]) => id === windowId,
              )?.[1];
              return fileWindow
                ? [
                    {
                      id: windowId,
                      label: fileWindowTitle(windowId, fileWindow),
                      kind: "file" as const,
                    },
                  ]
                : [];
            }),
          ]}
          linkSelecting={linkingNoteId === id}
          linkedHighlight={(focusedTerminalId !== null &&
            displayNote.linkedShellIds.includes(focusedTerminalId)) ||
            (focusedFileWindowId !== null &&
              displayNote.linkedFileWindowIds.includes(focusedFileWindowId)) ||
            (focusedNoteId !== null &&
              focusedNoteId !== id &&
              associatedNoteIds(id).includes(focusedNoteId))}
          linkedHighlightSource={focusedTerminalId !== null &&
          displayNote.linkedShellIds.includes(focusedTerminalId)
            ? "terminal"
            : focusedFileWindowId !== null &&
                displayNote.linkedFileWindowIds.includes(focusedFileWindowId)
              ? "file"
              : focusedNoteId !== null &&
                  focusedNoteId !== id &&
                  associatedNoteIds(id).includes(focusedNoteId)
                ? "note"
                : null}
          paragraphDropIndex={paragraphDropTarget?.kind === "note" &&
          paragraphDropTarget.id === id
            ? (paragraphDropTarget.noteInsertIndex ?? null)
            : null}
          on:toggleFullscreen={() => toggleFullscreen("note", id)}
          on:floatingChange={(event) =>
            (noteFloating = { ...noteFloating, [id]: event.detail })}
          on:toggleLink={() => toggleCanvasLinkSelection(id)}
          on:navigateRelation={(event) => navigateCanvasRelation(event.detail)}
          on:unlinkRelation={(event) => removeCanvasRelation(id, event.detail)}
          on:sendParagraph={(event) => sendNoteParagraph(id, event.detail)}
          on:paragraphDragStart={(event) => {
            linkingNoteId = null;
            paragraphDrag = event.detail;
          }}
          on:paragraphDragEnd={finishParagraphDrag}
          on:close={() => srocket?.send({ closeNote: [id, note.pageId] })}
          on:update={(event) =>
            srocket?.send({
              updateNote: [id, note.pageId, event.detail],
            })}
          on:editing={(event) =>
            srocket?.send({
              setNoteEditing: [id, note.pageId, event.detail],
            })}
          on:paragraphs={(event) => {
            const paragraphs = event.detail;
            notes = notes.map(([noteId, note]) =>
              noteId === id
                ? [noteId, { ...note, paragraphs, text: paragraphs.join("\n") }]
                : [noteId, note],
            );
            srocket?.send({
              updateNoteParagraphs: [id, note.pageId, paragraphs],
            });
          }}
          on:focus={() => {
            clearCanvasSelection();
            focusedNoteId = id;
            focusedTerminalId = null;
            focusedFileWindowId = null;
          }}
          on:blur={() => {
            if (focusedNoteId === id) focusedNoteId = null;
          }}
          on:bringToFront={() =>
            srocket?.send({ updateNote: [id, note.pageId, null] })}
          on:startMove={({ detail: event }) => {
            if (fullscreenItems[`note:${id}`]) return;
            if (startCanvasGroupMove("note", id, event)) return;
            const startingNote = displayNote;
            const [x, y] = normalizePosition(event);
            movingNoteOrigin = [x - startingNote.x, y - startingNote.y];
            movingNoteStartClient = [event.clientX, event.clientY];
            movingNoteDidMove = false;
            movingNoteStartState = startingNote;
            movingNoteState = startingNote;
            movingNote = id;
          }}
          on:startResize={({ detail }) => {
            if (fullscreenItems[`note:${id}`]) return;
            pendingCanvasSelection = null;
            clearCanvasSelection();
            const startingNote = displayNote;
            resizingNoteStartPointer = normalizePosition(detail.event);
            resizingNoteStartState = startingNote;
            resizingNoteState = startingNote;
            resizingNoteDirection = detail.direction;
            srocket?.send({ updateNote: [id, note.pageId, null] });
            resizingNote = id;
          }}
        />
      </div>
    {/each}

    {#each fileWindows.filter(([, window]) => window.pageId === activePageId) as [id, fileWindow] (id)}
      {@const displayFileWindow =
        groupFileStates[id] ??
        (id === movingFile
          ? movingFileState
          : id === resizingFile
            ? resizingFileState
            : fileWindow)}
      {@const fileKey = canvasItemKey("file", id)}
      {@const fileDropOffset = canvasDropPreviewOffsets[fileKey]}
      <div
        class="absolute"
        data-canvas-file-window={id}
        class:canvas-active={focusedFileWindowId === id}
        class:canvas-selected={selectedCanvasItems.includes(fileKey)}
        class:canvas-page-drop-preview={fileDropOffset !== undefined}
        class:canvas-interacting={groupFileStates[id] !== undefined ||
          movingFile === id ||
          resizingFile === id}
        class:canvas-fullscreen={fullscreenItems[`file:${id}`]}
        class:canvas-floating={fileFloating[id]}
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        style:--canvas-drop-x={fileDropOffset
          ? `${fileDropOffset[0]}px`
          : "0px"}
        style:--canvas-drop-y={fileDropOffset
          ? `${fileDropOffset[1]}px`
          : "0px"}
        transition:fade|local
        use:slide={{
          x: displayFileWindow.x,
          y: displayFileWindow.y,
          center,
          zoom,
          immediate:
            groupFileStates[id] !== undefined ||
            id === movingFile ||
            id === resizingFile,
        }}
        bind:this={fileWrappers[id]}
      >
        {#await loadFileExplorer()}
          <div
            class="flex h-full w-full items-center justify-center rounded-xl border border-zinc-700 bg-zinc-950 text-sm text-zinc-500 shadow-sm shadow-black/20"
            style:width={`${displayFileWindow.width}px`}
            style:height={`${displayFileWindow.height}px`}
          >
            Loading files…
          </div>
        {:then fileExplorerModule}
          <svelte:component
            this={fileExplorerModule.default}
            title={displayFileWindow.title}
            background={displayFileWindow.background}
            initialPath={displayFileWindow.path}
            currentPath={displayFileWindow.currentPath}
            expandedPaths={displayFileWindow.expandedPaths}
            selectedPath={displayFileWindow.selectedPath}
            selectedKind={displayFileWindow.selectedKind}
            treeScrollTop={displayFileWindow.treeScrollTop}
            editorPath={displayFileWindow.editorPath}
            sharedEditorContent={fileEditorBuffers[id]?.path ===
            displayFileWindow.editorPath
              ? fileEditorBuffers[id].content
              : null}
            sharedEditorDirty={displayFileWindow.editorDirty}
            width={displayFileWindow.width}
            height={displayFileWindow.height}
            sidebarWidth={displayFileWindow.sidebarWidth}
            treeRevision={displayFileWindow.treeRevision}
            online={sessionReady}
            linkedNotes={notes
              .filter(([, note]) => note.linkedFileWindowIds.includes(id))
              .map(([noteId, note]) => ({
                id: noteId,
                label: noteTitle(noteId, note),
                kind: "note" as const,
              }))}
            linkedHighlight={focusedNoteId !== null &&
              (notes
                .find(([noteId]) => noteId === focusedNoteId)?.[1]
                .linkedFileWindowIds.includes(id) ??
                false)}
            paragraphDropState={paragraphDropTarget?.kind === "file" &&
            paragraphDropTarget.id === id
              ? paragraphDropTarget.fileReady
                ? "ready"
                : "blocked"
              : "none"}
            fullscreen={fullscreenItems[`file:${id}`] ?? false}
            {hasWriteAccess}
            bind:insertText={fileTextSenders[id]}
            bind:previewTextDrop={fileDropPreviewers[id]}
            bind:cancelTextDropPreview={fileDropPreviewCancelers[id]}
            updateSharedState={(update) =>
              updateFileWindowSharedState(id, displayFileWindow.pageId, update)}
            request={(request) =>
              requestFileOperation(
                displayFileWindow.shellId,
                shells.find(
                  ([shellId]) => shellId === displayFileWindow.shellId,
                )?.[1].pageId ?? displayFileWindow.pageId,
                request,
              )}
            on:close={() =>
              hasWriteAccess &&
              srocket?.send({
                closeFileWindow: [id, fileWindow.pageId],
              })}
            on:toggleFullscreen={() => toggleFullscreen("file", id)}
            on:floatingChange={(event) =>
              (fileFloating = { ...fileFloating, [id]: event.detail })}
            on:openTerminal={(event) =>
              handleCreateAt(
                displayFileWindow.shellId,
                displayFileWindow.pageId,
                event.detail,
                displayFileWindow,
              )}
            on:navigateNote={(event) => navigateCanvasRelation(event.detail)}
            on:unlinkNote={(event) =>
              removeCanvasRelation(event.detail.id, {
                id,
                kind: "file",
                label: fileWindowTitle(id, displayFileWindow),
              })}
            on:focus={() => {
              clearCanvasSelection();
              focusedFileWindowId = id;
              focusedTerminalId = null;
              focusedNoteId = null;
            }}
            on:blur={() => {
              if (focusedFileWindowId === id) focusedFileWindowId = null;
            }}
            on:bringToFront={() =>
              bringFileWindowToFront(id, fileWindow.pageId)}
            on:startMove={({ detail: event }) => {
              if (!hasWriteAccess || fullscreenItems[`file:${id}`]) return;
              if (startCanvasGroupMove("file", id, event)) return;
              const [x, y] = normalizePosition(event);
              movingFileOrigin = [
                x - displayFileWindow.x,
                y - displayFileWindow.y,
              ];
              movingFileStartClient = [event.clientX, event.clientY];
              movingFileDidMove = false;
              movingFileStartState = displayFileWindow;
              movingFileState = displayFileWindow;
              movingFile = id;
            }}
          />
        {:catch error}
          <div
            class="flex h-full w-full items-center justify-center rounded-xl border border-red-900/70 bg-zinc-950 p-6 text-center text-sm text-red-300 shadow-sm shadow-black/20"
            style:width={`${displayFileWindow.width}px`}
            style:height={`${displayFileWindow.height}px`}
            role="alert"
          >
            Could not load the file explorer: {error instanceof Error
              ? error.message
              : String(error)}
          </div>
        {/await}
        <ResizeHandles
          disabled={!hasWriteAccess || fullscreenItems[`file:${id}`]}
          on:start={({ detail }) => {
            pendingCanvasSelection = null;
            clearCanvasSelection();
            resizingFileStartPointer = normalizePosition(detail.event);
            resizingFileStartState = displayFileWindow;
            resizingFileState = displayFileWindow;
            resizingFileDirection = detail.direction;
            bringFileWindowToFront(id, fileWindow.pageId);
            resizingFile = id;
          }}
        />
      </div>
    {/each}

    {#each users.filter(([id, user]) => id !== userId && user.cursor !== null && user.pageId === activePageId) as [id, user] (id)}
      <div
        class="absolute"
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        transition:fade|local={{ duration: 200 }}
        use:slide={{
          x: user.cursor?.[0] ?? 0,
          y: user.cursor?.[1] ?? 0,
          center,
          zoom,
        }}
      >
        <LiveCursor userId={id} {user} />
      </div>
    {/each}
  </div>
</main>

<style>
  :global(.canvas-active) {
    z-index: 1;
  }
  :global(.canvas-interacting) {
    z-index: 2;
    will-change: transform;
  }
  :global([data-canvas-terminal].canvas-selected > .term-container),
  :global([data-canvas-note-wrapper].canvas-selected > .note-container),
  :global([data-canvas-file-window].canvas-selected > .file-window) {
    border-color: rgb(254 240 138 / 1);
    animation: canvas-selection-pulse 1.15s ease-in-out infinite;
  }
  :global([data-canvas-terminal] > .term-container),
  :global([data-canvas-note-wrapper] > .note-container),
  :global([data-canvas-file-window] > .file-window) {
    transition:
      transform 190ms ease-out,
      opacity 150ms ease-out;
  }
  :global([data-canvas-terminal].canvas-page-drop-preview > .term-container),
  :global(
    [data-canvas-note-wrapper].canvas-page-drop-preview > .note-container
  ),
  :global([data-canvas-file-window].canvas-page-drop-preview > .file-window) {
    pointer-events: none;
    opacity: 0.06;
    transform: translate(var(--canvas-drop-x, 0px), var(--canvas-drop-y, 0px))
      scale(0.08);
    transform-origin: center;
    transition:
      transform 190ms ease-in,
      opacity 150ms ease-in;
    will-change: transform, opacity;
  }
  .selection-marquee {
    border: 1px solid rgb(212 212 216 / 0.72);
    border-radius: 0;
    background: rgb(113 113 122 / 0.12);
  }
  :global(.canvas-fullscreen) {
    position: fixed !important;
    left: 24px !important;
    right: 24px !important;
    top: 88px !important;
    bottom: 68px !important;
    z-index: 35 !important;
    transform: none !important;
  }
  :global(.canvas-floating) {
    z-index: 45 !important;
  }

  @keyframes canvas-selection-pulse {
    0%,
    100% {
      border-color: rgb(253 224 71 / 0.8);
      box-shadow: 0 0 4px rgb(253 224 71 / 0.3);
    }
    50% {
      border-color: rgb(254 249 195 / 1);
      box-shadow: 0 0 8px rgb(254 240 138 / 0.72);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global([data-canvas-terminal].canvas-selected > .term-container),
    :global([data-canvas-note-wrapper].canvas-selected > .note-container),
    :global([data-canvas-file-window].canvas-selected > .file-window) {
      animation: none;
    }
    :global([data-canvas-terminal].canvas-page-drop-preview > .term-container),
    :global(
      [data-canvas-note-wrapper].canvas-page-drop-preview > .note-container
    ),
    :global([data-canvas-file-window].canvas-page-drop-preview > .file-window) {
      opacity: 0.3;
      transform: none;
      transition: none;
    }
  }
</style>
