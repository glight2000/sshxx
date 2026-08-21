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
  import { createLock } from "./lock";
  import { Srocket } from "./srocket";
  import { isNativeApp } from "./runtime";
  import type {
    WsClient,
    FileTreeEntry,
    FileOperationRequest,
    FileOperationResponse,
    WsFileWindow,
    WsNote,
    WsPage,
    WsServer,
    WsSshProfile,
    WsUser,
    WsWinsize,
  } from "./protocol";
  import { makeToast } from "./toast";
  import Chat, { type ChatMessage } from "./ui/Chat.svelte";
  import ChooseName from "./ui/ChooseName.svelte";
  import NetworkInfo from "./ui/NetworkInfo.svelte";
  import Note from "./ui/Note.svelte";
  import PagePager from "./ui/PagePager.svelte";
  import ResizeHandles, {
    type ResizeDirection,
  } from "./ui/ResizeHandles.svelte";
  import Settings from "./ui/Settings.svelte";
  import Toolbar from "./ui/Toolbar.svelte";
  import TerminalSearch, {
    type CanvasSearchItem,
  } from "./ui/TerminalSearch.svelte";
  import XTerm from "./ui/XTerm.svelte";
  import FileExplorer from "./ui/FileExplorer.svelte";
  import type { CanvasRelationItem } from "./ui/CanvasRelations.svelte";
  import type {
    TextInsertPosition,
    TextInsertResult,
  } from "./ui/CodeEditor.svelte";
  import Avatars from "./ui/Avatars.svelte";
  import LiveCursor from "./ui/LiveCursor.svelte";
  import { slide } from "./action/slide";
  import { TouchZoom, INITIAL_ZOOM } from "./action/touchZoom";
  import { arrangeNewCanvasItem } from "./arrange";
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
  import { EyeIcon } from "svelte-feather-icons";

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
  const TERM_MIN_ROWS = 8;
  const TERM_MIN_COLS = 32;
  const TERM_INITIAL_ROWS = 26;
  const TERM_INITIAL_COLS = 79;
  const TERM_INITIAL_WIDTH = 715;
  const TERM_INITIAL_HEIGHT = 523;
  const NOTE_INITIAL_WIDTH = 384;
  const NOTE_INITIAL_HEIGHT = 224;
  const MAX_TERMINAL_HISTORY = 2 * 1024 * 1024;
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
  let serverVersion = "unknown";
  let daemonVersion = "unknown";

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
    const fullscreenKey = activeFullscreenKey();
    const target = event.target instanceof Element ? event.target : null;
    if (fullscreenKey && !target?.closest(".canvas-fullscreen"))
      exitActivePageFullscreen();
    handleCanvasLinkSelection(event);
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

  let encrypt: Encrypt;
  let srocket: Srocket<WsServer, WsClient> | null = null;

  let connected = false;
  let sessionReady = false;
  let exitReason: string | null = null;
  let failureStage: "server" | "session" | null = null;
  let readinessTimer: number | null = null;
  let lastNotifiedConnectionIssue = "";
  const incompatibleServerMessage =
    "This server is not sshxx-server. The upstream public sshx service is intentionally unsupported; use a self-hosted sshxx-server.";

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
  type TerminalHistoryBuffer = {
    chunks: string[];
    start: number;
    length: number;
  };
  const terminalHistory: Record<number, TerminalHistoryBuffer> = {};
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
  type PendingFileRequest = {
    stream: bigint;
    resolve: (response: FileOperationResponse) => void;
    reject: (error: Error) => void;
    timer: number;
  };
  const pendingFileRequests = new Map<string, PendingFileRequest>();

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
  let paragraphDrag: { text: string; sourceNoteId: number } | null = null;
  let paragraphDropTarget: ParagraphDropTarget | null = null;
  $: if (
    linkingNoteId !== null &&
    !notes.some(([noteId]) => noteId === linkingNoteId)
  )
    linkingNoteId = null;

  function appendTerminalHistory(id: number, data: string) {
    const history = (terminalHistory[id] ??= {
      chunks: [],
      start: 0,
      length: 0,
    });
    history.chunks.push(data);
    history.length += data.length;

    while (history.length > MAX_TERMINAL_HISTORY) {
      const first = history.chunks[history.start];
      const overflow = history.length - MAX_TERMINAL_HISTORY;
      if (overflow >= first.length) {
        history.length -= first.length;
        history.start += 1;
      } else {
        history.chunks[history.start] = first.slice(overflow);
        history.length -= overflow;
      }
    }

    if (history.start > 256 && history.start * 2 > history.chunks.length) {
      history.chunks = history.chunks.slice(history.start);
      history.start = 0;
    }
  }

  function readTerminalHistory(id: number) {
    const history = terminalHistory[id];
    return history ? history.chunks.slice(history.start).join("") : "";
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
  let movingNoteState: WsNote;
  let resizingNote = -1;
  let resizingNoteStartPointer = [0, 0];
  let resizingNoteStartState: WsNote;
  let resizingNoteState: WsNote;
  let resizingNoteDirection: ResizeDirection = "se";

  let movingFile = -1;
  let movingFileOrigin = [0, 0];
  let movingFileState: WsFileWindow;
  let resizingFile = -1;
  let resizingFileStartPointer = [0, 0];
  let resizingFileStartState: WsFileWindow;
  let resizingFileState: WsFileWindow;
  let resizingFileDirection: ResizeDirection = "se";

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

    scheduleReadinessWarning();
    srocket = new Srocket<WsServer, WsClient>(`/api/s/${id}`, {
      onMessage(message) {
        if (message.hello) {
          if (message.hello[4] !== "sshxx") {
            clearReadinessTimer();
            reportConnectionIssue(incompatibleServerMessage, "server");
            srocket?.dispose();
            return;
          }
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
          const pending = pendingFileRequests.get(requestId);
          if (!pending || BigInt(stream) !== pending.stream) return;
          pendingFileRequests.delete(requestId);
          window.clearTimeout(pending.timer);
          void encrypt
            .segment(pending.stream, 0n, data)
            .then((plaintext) => {
              pending.resolve(
                JSON.parse(
                  new TextDecoder().decode(plaintext),
                ) as FileOperationResponse,
              );
            })
            .catch((error) =>
              pending.reject(
                error instanceof Error ? error : new Error(String(error)),
              ),
            );
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
        for (const pending of pendingFileRequests.values()) {
          window.clearTimeout(pending.timer);
          pending.reject(
            new Error(
              "Connection closed before the filesystem request completed.",
            ),
          );
        }
        pendingFileRequests.clear();
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

  function switchPage(pageId: number) {
    if (!pages.some((page) => page.id === pageId)) return;
    preferredPageId = pageId;
    if (pageId === activePageId) {
      scheduleLocalViewSave();
      return;
    }
    pageViews[activePageId] = { center: [...center], zoom };
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
              candidate.pageId === note.pageId &&
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
    if (event.key === "Escape" && linkingNoteId !== null) {
      event.preventDefault();
      linkingNoteId = null;
    }
  }

  function existingCanvasItems() {
    return [
      ...shells
        .filter(([, winsize]) => winsize.pageId === activePageId)
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
        .filter(([, note]) => note.pageId === activePageId)
        .map(([, note]) => ({
          x: note.x,
          y: note.y,
          width: note.width,
          height: note.height,
        })),
      ...fileWindows
        .filter(([, window]) => window.pageId === activePageId)
        .map(([, { x, y, width, height }]) => ({ x, y, width, height })),
    ];
  }

  function nextCanvasRect(width: number, height: number) {
    const position = arrangeNewCanvasItem(existingCanvasItems(), width, height);
    return gridAlignedRect({ ...position, width, height });
  }

  async function handleCreate() {
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
    const { x, y, width, height } = nextCanvasRect(
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
    touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function handleCreateSsh(profileId: string) {
    if (!hasWriteAccess || shells.length >= 100) return;
    const { x, y, width, height } = nextCanvasRect(
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
    touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function handleCreateNote() {
    if (hasWriteAccess === false || notes.length >= 100) return;
    const { x, y, width, height } = nextCanvasRect(
      NOTE_INITIAL_WIDTH,
      NOTE_INITIAL_HEIGHT,
    );
    srocket?.send({ createNoteSized: [x, y, width, height, activePageId] });
    touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function randomEncryptedStream() {
    const bytes = crypto.getRandomValues(new Uint8Array(8));
    bytes[0] |= 0x80;
    return bytes.reduce((value, byte) => (value << 8n) | BigInt(byte), 0n);
  }

  async function requestFileOperation(
    shellId: number,
    pageId: number,
    request: FileOperationRequest,
  ): Promise<FileOperationResponse> {
    if (!srocket?.connected) throw new Error("The daemon is not connected.");
    const requestId = randomHex(16);
    const requestStream = randomEncryptedStream();
    let responseStream = randomEncryptedStream();
    while (responseStream === requestStream)
      responseStream = randomEncryptedStream();
    const plaintext = new TextEncoder().encode(JSON.stringify(request));
    const data = await encrypt.segment(requestStream, 0n, plaintext);
    const response = new Promise<FileOperationResponse>((resolve, reject) => {
      const timer = window.setTimeout(() => {
        pendingFileRequests.delete(requestId);
        reject(new Error("Filesystem request timed out."));
      }, 35_000);
      pendingFileRequests.set(requestId, {
        stream: responseStream,
        resolve,
        reject,
        timer,
      });
    });
    srocket.send({
      fileRequest: [
        shellId,
        pageId,
        requestId,
        requestStream,
        responseStream,
        data,
      ],
    });
    return response;
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
    const rect = nextCanvasRect(1_040, 680);
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
    touchZoom.moveTo([rect.x, rect.y], INITIAL_ZOOM);
  }

  function handleDuplicate(sourceId: number) {
    if (!hasWriteAccess || shells.length >= 100) return;
    const source = shells.find(([id]) => id === sourceId)?.[1];
    if (!source) return;
    const wrapper = termWrappers[sourceId];
    const width =
      source.width || wrapper?.clientWidth / zoom || TERM_INITIAL_WIDTH;
    const height =
      source.height || wrapper?.clientHeight / zoom || TERM_INITIAL_HEIGHT;
    const rect = nextCanvasRect(width, height);
    srocket?.send({
      cloneWindowed: [
        sourceId,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        source.rows,
        source.cols,
        activePageId,
        source.theme || $settings.theme,
      ],
    });
    touchZoom.moveTo([rect.x, rect.y], INITIAL_ZOOM);
  }

  function handleCreateAt(sourceId: number, pageId: number, path: string) {
    if (!hasWriteAccess || shells.length >= 100 || !path) return;
    const rect = nextCanvasRect(TERM_INITIAL_WIDTH, TERM_INITIAL_HEIGHT);
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
    touchZoom.moveTo([rect.x, rect.y], INITIAL_ZOOM);
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

  function insertParagraphIntoNote(
    targetNoteId: number,
    text: string,
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
    paragraphs.splice(index, 0, text);
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
      text: string;
      target: "all" | "notes" | "terminals" | "terminals-execute" | "files";
    },
  ) {
    if (!hasWriteAccess) return;
    const note = notes.find(([id]) => id === noteId)?.[1];
    if (!note) return;
    if (!detail.text) {
      makeToast({ kind: "info", message: "This paragraph is empty." });
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
      const result = insertParagraphIntoNote(targetNoteId, detail.text);
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
      if (id === paragraphDrag.sourceNoteId) return null;
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
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
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
    if (!target) {
      finishParagraphDrag();
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
      result = insertParagraphIntoNote(target.id, text, target.noteInsertIndex);
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
        ? "Paragraph copied to the target."
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

  function randomHex(bytes: number) {
    const data = new Uint8Array(bytes);
    crypto.getRandomValues(data);
    return Array.from(data, (byte) => byte.toString(16).padStart(2, "0")).join(
      "",
    );
  }

  function randomUploadStream() {
    const data = new Uint8Array(8);
    crypto.getRandomValues(data);
    return new DataView(data.buffer).getBigUint64(0) | 0x8000000000000000n;
  }

  async function uploadImage(id: number, pageId: number, file: File) {
    if (!srocket?.connected) {
      throw new Error("Connect to the daemon before uploading an image.");
    }
    const uploadId = randomHex(16);
    const streamNum = randomUploadStream();
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
    if (activeElement instanceof HTMLElement) activeElement.focus();
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
      if (moving !== -1 && !movingIsDone) {
        const [x, y] = normalizePosition(event);
        movingSize = {
          ...movingSize,
          x: snapLeadingEdge(Math.round(x - movingOrigin[0])),
          y: snapLeadingEdge(Math.round(y - movingOrigin[1])),
        };
        sendMove({ move: [moving, movingSize.pageId, movingSize] });
      }

      if (movingNote !== -1) {
        const [x, y] = normalizePosition(event);
        movingNoteState = {
          ...movingNoteState,
          x: snapLeadingEdge(Math.round(x - movingNoteOrigin[0])),
          y: snapLeadingEdge(Math.round(y - movingNoteOrigin[1])),
        };
      }

      if (movingFile !== -1) {
        const [x, y] = normalizePosition(event);
        movingFileState = {
          ...movingFileState,
          x: snapLeadingEdge(Math.round(x - movingFileOrigin[0])),
          y: snapLeadingEdge(Math.round(y - movingFileOrigin[1])),
        };
      }

      if (resizing !== -1) {
        const [x, y] = normalizePosition(event);
        const dx = x - resizingStartPointer[0];
        const dy = y - resizingStartPointer[1];
        const [startLeft, startTop, startRight, startBottom] =
          resizingStartEdges;
        let left = resizesWest(resizingDirection)
          ? snapLeadingEdge(startLeft + dx)
          : startLeft;
        let top = resizesNorth(resizingDirection)
          ? snapLeadingEdge(startTop + dy)
          : startTop;
        const right = resizesEast(resizingDirection)
          ? snapTrailingEdge(startRight + dx)
          : startRight;
        const bottom = resizesSouth(resizingDirection)
          ? snapTrailingEdge(startBottom + dy)
          : startBottom;
        const changesWidth =
          resizesWest(resizingDirection) || resizesEast(resizingDirection);
        const changesHeight =
          resizesNorth(resizingDirection) || resizesSouth(resizingDirection);
        const cols = changesWidth
          ? Math.max(
              resizingStartSize.cols +
                Math.floor(
                  (right - left - resizingStartPixels[0]) /
                    resizingCanvasCell[0],
                ),
              TERM_MIN_COLS,
            )
          : resizingStartSize.cols;
        const rows = changesHeight
          ? Math.max(
              resizingStartSize.rows +
                Math.floor(
                  (bottom - top - resizingStartPixels[1]) /
                    resizingCanvasCell[1],
                ),
              TERM_MIN_ROWS,
            )
          : resizingStartSize.rows;
        if (resizesWest(resizingDirection) && cols === TERM_MIN_COLS) {
          left = Math.round(
            startRight -
              (resizingStartPixels[0] +
                (cols - resizingStartSize.cols) * resizingCanvasCell[0]),
          );
        }
        if (resizesNorth(resizingDirection) && rows === TERM_MIN_ROWS) {
          top = Math.round(
            startBottom -
              (resizingStartPixels[1] +
                (rows - resizingStartSize.rows) * resizingCanvasCell[1]),
          );
        }
        const width = Math.round(right - left);
        const height = Math.round(bottom - top);
        if (
          rows !== resizingSize.rows ||
          cols !== resizingSize.cols ||
          width !== resizingSize.width ||
          height !== resizingSize.height ||
          left !== resizingSize.x ||
          top !== resizingSize.y
        ) {
          resizingSize = {
            ...resizingSize,
            x: left,
            y: top,
            width,
            height,
            rows,
            cols,
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
      if (moving !== -1) {
        movingIsDone = true;
        sendMove.cancel();
        srocket?.send({ move: [moving, movingSize.pageId, movingSize] });
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
        fileWindows = fileWindows.map(([id, window]) =>
          id === movedId ? [id, movingFileState] : [id, window],
        );
        srocket?.send({
          updateFileWindow: [movedId, movingFileState.pageId, movingFileState],
        });
        void tick().then(() => {
          if (movingFile === movedId) movingFile = -1;
        });
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
  <div
    class="absolute top-8 inset-x-0 flex justify-center pointer-events-none z-10"
  >
    <Toolbar
      {connected}
      {connectionStatus}
      connectionDetail={exitReason}
      {newMessages}
      {hasWriteAccess}
      profiles={sshProfiles}
      {users}
      on:create={handleCreate}
      on:createSsh={(event) => handleCreateSsh(event.detail)}
      on:saveSshProfile={(event) =>
        srocket?.send({ upsertSshProfile: event.detail })}
      on:deleteSshProfile={(event) =>
        srocket?.send({ deleteSshProfile: event.detail })}
      on:createNote={handleCreateNote}
      on:chat={() => {
        showChat = !showChat;
        newMessages = false;
      }}
      on:settings={() => {
        settingsOpen = true;
      }}
      on:search={() => (searchOpen = !searchOpen)}
      on:networkInfo={() => {
        showNetworkInfo = !showNetworkInfo;
      }}
    />

    <TerminalSearch
      open={searchOpen}
      items={canvasSearchItems}
      on:close={() => (searchOpen = false)}
      on:select={(event) => selectCanvasItem(event.detail)}
    />

    {#if showNetworkInfo}
      <div class="absolute top-20 translate-x-[116.5px]">
        <NetworkInfo
          status={connectionStatus === "connected"
            ? "connected"
            : exitReason
              ? failureStage === "session"
                ? "no-shell"
                : "no-server"
              : "no-server"}
          serverLatency={integerMedian(serverLatencies)}
          shellLatency={integerMedian(shellLatencies)}
          detail={exitReason}
        />
      </div>
    {/if}
  </div>

  {#if showChat}
    <div
      class="absolute flex flex-col justify-end inset-y-4 right-4 w-80 pointer-events-none z-10"
    >
      <Chat
        {userId}
        messages={chatMessages}
        on:chat={(event) => srocket?.send({ chat: event.detail })}
        on:close={() => (showChat = false)}
      />
    </div>
  {/if}

  <Settings
    open={settingsOpen}
    {serverVersion}
    {daemonVersion}
    on:close={() => (settingsOpen = false)}
  />

  <ChooseName />

  <PagePager
    {pages}
    {activePageId}
    {hasWriteAccess}
    on:select={(event) => switchPage(event.detail)}
    on:create={() => {
      selectCreatedPage = true;
      srocket?.send({ createPage: "" });
    }}
    on:rename={(event) =>
      srocket?.send({
        renamePage: [event.detail.id, event.detail.name],
      })}
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

  <div class="py-2">
    {#if userId && hasWriteAccess === false}
      <div
        class="bg-yellow-900 text-yellow-200 px-1 py-0.5 rounded inline-flex items-center gap-1"
      >
        <EyeIcon size="14" />
        <span class="text-xs">Read-only</span>
      </div>
    {/if}
  </div>

  <div class="absolute inset-0 overflow-hidden touch-none" bind:this={fabricEl}>
    {#each shells.filter(([, winsize]) => winsize.pageId === activePageId) as [id, winsize] (id)}
      {@const ws =
        id === moving ? movingSize : id === resizing ? resizingSize : winsize}
      <div
        class="absolute"
        data-canvas-terminal={id}
        class:canvas-fullscreen={fullscreenItems[`terminal:${id}`]}
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        transition:fade|local
        use:slide={{
          x: ws.x,
          y: ws.y,
          center,
          zoom,
          immediate: id === moving || id === resizing,
        }}
        bind:this={termWrappers[id]}
      >
        <XTerm
          rows={ws.rows}
          cols={ws.cols}
          windowWidth={ws.width}
          windowHeight={ws.height}
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
          on:duplicate={() => handleDuplicate(id)}
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
          on:title={(event) => {
            terminalTitles = { ...terminalTitles, [id]: event.detail };
          }}
          on:bringToFront={() => {
            if (!hasWriteAccess) return;
            showNetworkInfo = false;
            srocket?.send({ move: [id, ws.pageId, null] });
          }}
          on:startMove={({ detail: event }) => {
            if (!hasWriteAccess || fullscreenItems[`terminal:${id}`]) return;
            const startingSize = ws;
            const [x, y] = normalizePosition(event);
            movingOrigin = [x - startingSize.x, y - startingSize.y];
            movingSize = startingSize;
            movingIsDone = false;
            moving = id;
          }}
          on:focus={() => {
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
        id === movingNote
          ? movingNoteState
          : id === resizingNote
            ? resizingNoteState
            : note}
      <div
        class="absolute"
        class:canvas-fullscreen={fullscreenItems[`note:${id}`]}
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        style:opacity={displayNote.opacity / 100}
        transition:fade|local
        use:slide={{
          x: displayNote.x,
          y: displayNote.y,
          center,
          zoom,
          immediate: id === movingNote || id === resizingNote,
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
          paragraphDropIndex={paragraphDropTarget?.kind === "note" &&
          paragraphDropTarget.id === id
            ? (paragraphDropTarget.noteInsertIndex ?? null)
            : null}
          on:toggleFullscreen={() => toggleFullscreen("note", id)}
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
            const startingNote = displayNote;
            const [x, y] = normalizePosition(event);
            movingNoteOrigin = [x - startingNote.x, y - startingNote.y];
            movingNoteState = startingNote;
            movingNote = id;
          }}
          on:startResize={({ detail }) => {
            if (fullscreenItems[`note:${id}`]) return;
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
        id === movingFile
          ? movingFileState
          : id === resizingFile
            ? resizingFileState
            : fileWindow}
      <div
        class="absolute"
        data-canvas-file-window={id}
        class:canvas-fullscreen={fullscreenItems[`file:${id}`]}
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        transition:fade|local
        use:slide={{
          x: displayFileWindow.x,
          y: displayFileWindow.y,
          center,
          zoom,
          immediate: id === movingFile || id === resizingFile,
        }}
        bind:this={fileWrappers[id]}
      >
        <FileExplorer
          title={displayFileWindow.title}
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
              displayFileWindow.pageId,
              request,
            )}
          on:close={() =>
            hasWriteAccess &&
            srocket?.send({
              closeFileWindow: [id, fileWindow.pageId],
            })}
          on:toggleFullscreen={() => toggleFullscreen("file", id)}
          on:openTerminal={(event) =>
            handleCreateAt(
              displayFileWindow.shellId,
              displayFileWindow.pageId,
              event.detail,
            )}
          on:navigateNote={(event) => navigateCanvasRelation(event.detail)}
          on:unlinkNote={(event) =>
            removeCanvasRelation(event.detail.id, {
              id,
              kind: "file",
              label: fileWindowTitle(id, displayFileWindow),
            })}
          on:focus={() => {
            focusedFileWindowId = id;
            focusedTerminalId = null;
            focusedNoteId = null;
          }}
          on:blur={() => {
            if (focusedFileWindowId === id) focusedFileWindowId = null;
          }}
          on:bringToFront={() => bringFileWindowToFront(id, fileWindow.pageId)}
          on:startMove={({ detail: event }) => {
            if (!hasWriteAccess || fullscreenItems[`file:${id}`]) return;
            const [x, y] = normalizePosition(event);
            movingFileOrigin = [
              x - displayFileWindow.x,
              y - displayFileWindow.y,
            ];
            movingFileState = displayFileWindow;
            bringFileWindowToFront(id, fileWindow.pageId);
            movingFile = id;
          }}
        />
        <ResizeHandles
          disabled={!hasWriteAccess || fullscreenItems[`file:${id}`]}
          on:start={({ detail }) => {
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
  :global(.canvas-fullscreen) {
    position: fixed !important;
    left: 24px !important;
    right: 24px !important;
    top: 88px !important;
    bottom: 68px !important;
    z-index: 35 !important;
    transform: none !important;
  }
</style>
