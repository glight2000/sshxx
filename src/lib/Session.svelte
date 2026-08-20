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
  import type {
    WsClient,
    WsNote,
    WsPage,
    WsServer,
    WsUser,
    WsWinsize,
  } from "./protocol";
  import { makeToast } from "./toast";
  import Chat, { type ChatMessage } from "./ui/Chat.svelte";
  import ChooseName from "./ui/ChooseName.svelte";
  import NameList from "./ui/NameList.svelte";
  import NetworkInfo from "./ui/NetworkInfo.svelte";
  import Note from "./ui/Note.svelte";
  import PagePager from "./ui/PagePager.svelte";
  import Settings from "./ui/Settings.svelte";
  import Toolbar from "./ui/Toolbar.svelte";
  import TerminalSearch, {
    type CanvasSearchItem,
  } from "./ui/TerminalSearch.svelte";
  import XTerm from "./ui/XTerm.svelte";
  import Avatars from "./ui/Avatars.svelte";
  import LiveCursor from "./ui/LiveCursor.svelte";
  import { slide } from "./action/slide";
  import { TouchZoom, INITIAL_ZOOM } from "./action/touchZoom";
  import { arrangeNewTerminal } from "./arrange";
  import { settings } from "./settings";
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
  const GRID_SIZE = 24;
  const GRID_EDGE_GAP = GRID_SIZE / 10;
  const MAX_TERMINAL_HISTORY = 2 * 1024 * 1024;

  const nearestGridPoint = (value: number) =>
    Math.round(value / GRID_SIZE) * GRID_SIZE;
  const snapLeadingEdge = (value: number, canvasOffset: number) =>
    $settings.snapToGrid
      ? Math.round(
          nearestGridPoint(value + canvasOffset) + GRID_EDGE_GAP - canvasOffset,
        )
      : value;
  const snapTrailingEdge = (value: number, canvasOffset: number) =>
    $settings.snapToGrid
      ? Math.round(
          nearestGridPoint(value + canvasOffset) - GRID_EDGE_GAP - canvasOffset,
        )
      : value;

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

  onMount(() => {
    const storedPageId = Number(
      window.localStorage.getItem(`sshx.activePage.${id}`),
    );
    if (Number.isSafeInteger(storedPageId) && storedPageId > 0) {
      preferredPageId = storedPageId;
    }
    touchZoom = new TouchZoom(fabricEl, () => !hasActiveCanvasItem());
    touchZoom.onMove(() => {
      center = touchZoom.center;
      zoom = touchZoom.zoom;
      pageViews[activePageId] = { center: [...center], zoom };

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
  let exitReason: string | null = null;

  /** Bound "write" method for each terminal. */
  const writers: Record<number, (data: string, replay?: boolean) => void> = {};
  const termWrappers: Record<number, HTMLDivElement> = {};
  const termElements: Record<number, HTMLDivElement> = {};
  const noteWrappers: Record<number, HTMLElement> = {};
  const chunknums: Record<number, number> = {};
  const locks: Record<number, any> = {};
  const terminalHistory: Record<number, string> = {};
  const replayedWriters: Record<
    number,
    (data: string, replay?: boolean) => void
  > = {};
  let userId = 0;
  let users: [number, WsUser][] = [];
  let shells: [number, WsWinsize][] = [];
  let notes: [number, WsNote][] = [];
  let terminalTitles: Record<number, string> = {};
  let noteEditors: Record<number, { pageId: number; userId: number }> = {};
  let pages: WsPage[] = [{ id: 1, name: "Page 1" }];
  let activePageId = 1;
  let preferredPageId = 1;
  let selectCreatedPage = false;
  const pageViews: Record<number, { center: number[]; zoom: number }> = {
    1: { center: [0, 0], zoom: INITIAL_ZOOM },
  };
  let subscriptions = new Set<number>();

  function writeTerminalData(id: number, data: string, replay: boolean) {
    const history = (terminalHistory[id] ?? "") + data;
    terminalHistory[id] = history.slice(-MAX_TERMINAL_HISTORY);
    const writer = writers[id];
    if (!writer) return;
    if (replayedWriters[id] !== writer) {
      replayedWriters[id] = writer;
      writer(terminalHistory[id], true);
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
  let resizingStartBottomRight = [0, 0];
  let resizingStartSize: WsWinsize;
  let resizingCanvasCell = [0, 0];
  let resizingSize: WsWinsize; // Last resize message sent.

  let movingNote = -1;
  let movingNoteOrigin = [0, 0];
  let movingNoteState: WsNote;
  let resizingNote = -1;
  let resizingNoteStartPointer = [0, 0];
  let resizingNoteStartBottomRight = [0, 0];
  let resizingNoteStartState: WsNote;
  let resizingNoteState: WsNote;

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
        } else if (message.invalidAuth) {
          exitReason =
            "The URL is not correct, invalid end-to-end encryption key.";
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
            for (const data of chunks) {
              const buf = await encrypt.segment(
                0x100000000n | BigInt(id),
                BigInt(seqnum),
                data,
              );
              seqnum += data.length;
              writeTerminalData(id, new TextDecoder().decode(buf), replay);
            }
          });
        } else if (message.users) {
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
              title: winsize.title ?? "",
              background: winsize.background ?? "",
              opacity: winsize.opacity ?? 80,
              pageId: winsize.pageId ?? 1,
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
              width: note.width ?? 384,
              height: note.height ?? 224,
              pageId: note.pageId ?? 1,
            },
          ]);
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
        } else if (message.noteEditing) {
          const [noteId, pageId, editor] = message.noteEditing;
          noteEditors = { ...noteEditors };
          if (editor === null) delete noteEditors[noteId];
          else noteEditors[noteId] = { pageId, userId: editor };
        } else if (message.noteText) {
          const [noteId, pageId, text] = message.noteText;
          notes = notes.map(([id, note]) =>
            id === noteId && note.pageId === pageId
              ? [id, { ...note, text }]
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
        } else if (message.pong !== undefined) {
          const serverLatency = Date.now() - Number(message.pong);
          serverLatencies = [...serverLatencies, serverLatency].slice(-10);
        } else if (message.error) {
          console.warn("Server error: " + message.error);
        }
      },

      onConnect() {
        srocket?.send({ authenticate: [encryptedZeros, writeEncryptedZeros] });
        if ($settings.name) {
          srocket?.send({ setName: $settings.name });
        }
        connected = true;
      },

      onDisconnect() {
        connected = false;
        subscriptions.clear();
        users = [];
        noteEditors = {};
        serverLatencies = [];
        shellLatencies = [];
      },

      onClose(event) {
        if (event.code === 4404) {
          exitReason = "Failed to connect: " + event.reason;
        } else if (event.code === 4500) {
          exitReason = "Internal server error: " + event.reason;
        }
      },
    });
  });

  onDestroy(() => srocket?.dispose());

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

  function switchPage(pageId: number) {
    if (!pages.some((page) => page.id === pageId)) return;
    preferredPageId = pageId;
    window.localStorage.setItem(`sshx.activePage.${id}`, String(pageId));
    if (pageId === activePageId) return;
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
    srocket?.send({ setCursor: [pageId, null] });
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  }

  function pageName(pageId: number) {
    return pages.find((page) => page.id === pageId)?.name ?? "Unknown page";
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
                  width: wrapper.clientWidth / zoom,
                  height: wrapper.clientHeight / zoom,
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
    ];
  }

  function nextCanvasPosition() {
    const position = arrangeNewTerminal(existingCanvasItems());
    const [offsetX, offsetY] = getConstantOffset();
    return {
      x: snapLeadingEdge(position.x, offsetX),
      y: snapLeadingEdge(position.y, offsetY),
    };
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
    const { x, y } = nextCanvasPosition();
    srocket?.send({ create: [x, y, activePageId] });
    touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function handleCreateNote() {
    if (hasWriteAccess === false || notes.length >= 100) return;
    const { x, y } = nextCanvasPosition();
    srocket?.send({ createNote: [x, y, activePageId] });
    touchZoom.moveTo([x, y], INITIAL_ZOOM);
  }

  function handleDuplicate(sourceId: number) {
    if (!hasWriteAccess || shells.length >= 100) return;
    const { x, y } = nextCanvasPosition();
    srocket?.send({ clone: [sourceId, x, y, activePageId] });
    touchZoom.moveTo([x, y], INITIAL_ZOOM);
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
      title:
        note.text.split("\n").find((line) => line.trim()) || `Note #${noteId}`,
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
        writer(terminalHistory[id] ?? "", true);
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
        const [offsetX, offsetY] = getConstantOffset();
        movingSize = {
          ...movingSize,
          x: snapLeadingEdge(Math.round(x - movingOrigin[0]), offsetX),
          y: snapLeadingEdge(Math.round(y - movingOrigin[1]), offsetY),
        };
        sendMove({ move: [moving, movingSize.pageId, movingSize] });
      }

      if (movingNote !== -1) {
        const [x, y] = normalizePosition(event);
        const [offsetX, offsetY] = getConstantOffset();
        movingNoteState = {
          ...movingNoteState,
          x: snapLeadingEdge(Math.round(x - movingNoteOrigin[0]), offsetX),
          y: snapLeadingEdge(Math.round(y - movingNoteOrigin[1]), offsetY),
        };
      }

      if (resizing !== -1) {
        const [x, y] = normalizePosition(event);
        const [offsetX, offsetY] = getConstantOffset();
        const right = snapTrailingEdge(
          resizingStartBottomRight[0] + x - resizingStartPointer[0],
          offsetX,
        );
        const bottom = snapTrailingEdge(
          resizingStartBottomRight[1] + y - resizingStartPointer[1],
          offsetY,
        );
        const cols = Math.max(
          resizingStartSize.cols +
            Math.round(
              (right - resizingStartBottomRight[0]) / resizingCanvasCell[0],
            ),
          TERM_MIN_COLS,
        );
        const rows = Math.max(
          resizingStartSize.rows +
            Math.round(
              (bottom - resizingStartBottomRight[1]) / resizingCanvasCell[1],
            ),
          TERM_MIN_ROWS,
        );
        if (rows !== resizingSize.rows || cols !== resizingSize.cols) {
          resizingSize = { ...resizingSize, rows, cols };
          srocket?.send({
            move: [resizing, resizingSize.pageId, resizingSize],
          });
        }
      }

      if (resizingNote !== -1) {
        const [x, y] = normalizePosition(event);
        const [offsetX, offsetY] = getConstantOffset();
        const right = snapTrailingEdge(
          resizingNoteStartBottomRight[0] + x - resizingNoteStartPointer[0],
          offsetX,
        );
        const bottom = snapTrailingEdge(
          resizingNoteStartBottomRight[1] + y - resizingNoteStartPointer[1],
          offsetY,
        );
        resizingNoteState = {
          ...resizingNoteState,
          width: Math.max(240, Math.round(right - resizingNoteStartState.x)),
          height: Math.max(160, Math.round(bottom - resizingNoteStartState.y)),
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

<!-- Wheel handler stops native macOS Chrome zooming on pinch. -->
<main
  class="p-8"
  class:cursor-nwse-resize={resizing !== -1 || resizingNote !== -1}
  on:wheel={(event) => event.preventDefault()}
>
  <div
    class="absolute top-8 inset-x-0 flex justify-center pointer-events-none z-10"
  >
    <Toolbar
      {connected}
      {newMessages}
      {hasWriteAccess}
      on:create={handleCreate}
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
          status={connected
            ? "connected"
            : exitReason
              ? "no-shell"
              : "no-server"}
          serverLatency={integerMedian(serverLatencies)}
          shellLatency={integerMedian(shellLatencies)}
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
    style:background-image="radial-gradient(#333 {zoom}px, transparent 0)"
    style:background-size="{24 * zoom}px {24 * zoom}px"
    style:background-position="{-zoom * center[0]}px {-zoom * center[1]}px"
  ></div>

  <div class="py-2">
    {#if exitReason !== null}
      <div class="text-red-400">{exitReason}</div>
    {:else if connected}
      <div class="flex items-center">
        <div class="text-green-400">You are connected!</div>
        {#if userId && hasWriteAccess === false}
          <div
            class="bg-yellow-900 text-yellow-200 px-1 py-0.5 rounded ml-3 inline-flex items-center gap-1"
          >
            <EyeIcon size="14" />
            <span class="text-xs">Read-only</span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="text-yellow-400">Connecting…</div>
    {/if}

    <div class="mt-4">
      <NameList {users} />
    </div>
  </div>

  <div class="absolute inset-0 overflow-hidden touch-none" bind:this={fabricEl}>
    {#each shells.filter(([, winsize]) => winsize.pageId === activePageId) as [id, winsize] (id)}
      {@const ws =
        id === moving ? movingSize : id === resizing ? resizingSize : winsize}
      <div
        class="absolute"
        style:left={OFFSET_LEFT_CSS}
        style:top={OFFSET_TOP_CSS}
        style:transform-origin={OFFSET_TRANSFORM_ORIGIN_CSS}
        transition:fade|local
        use:slide={{ x: ws.x, y: ws.y, center, zoom, immediate: id === moving }}
        bind:this={termWrappers[id]}
      >
        <XTerm
          rows={ws.rows}
          cols={ws.cols}
          title={ws.title}
          background={ws.background}
          opacity={ws.opacity}
          {hasWriteAccess}
          bind:write={writers[id]}
          bind:termEl={termElements[id]}
          on:data={({ detail: data }) =>
            hasWriteAccess && handleInput(id, ws.pageId, data)}
          on:close={() => srocket?.send({ close: [id, ws.pageId] })}
          on:duplicate={() => handleDuplicate(id)}
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
            if (!hasWriteAccess) return;
            const startingSize = ws;
            const [x, y] = normalizePosition(event);
            movingOrigin = [x - startingSize.x, y - startingSize.y];
            movingSize = startingSize;
            movingIsDone = false;
            moving = id;
          }}
          on:focus={() => {
            if (!hasWriteAccess) return;
            focused = [...focused, [id, ws.pageId]];
          }}
          on:blur={() => {
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

        <!-- Interactable element for resizing -->
        <button
          type="button"
          aria-label="Resize terminal"
          class="absolute w-5 h-5 -bottom-1 -right-1 cursor-nwse-resize"
          on:mousedown={(event) => {
            const canvasEl = termElements[id].querySelector(".xterm-screen");
            if (canvasEl) {
              const screenRect = canvasEl.getBoundingClientRect();
              const wrapperRect = termWrappers[id].getBoundingClientRect();
              resizingStartPointer = normalizePosition(event);
              resizingStartBottomRight = [
                ws.x + wrapperRect.width / zoom,
                ws.y + wrapperRect.height / zoom,
              ];
              resizingStartSize = ws;
              resizingCanvasCell = [
                screenRect.width / zoom / ws.cols,
                screenRect.height / zoom / ws.rows,
              ];
              resizingSize = ws;
              resizing = id;
            }
          }}
          on:pointerdown={(event) => event.stopPropagation()}
        ></button>
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
          note={displayNote}
          {hasWriteAccess}
          {userId}
          editingBy={noteEditors[id]?.pageId === note.pageId
            ? noteEditors[id].userId
            : null}
          editingName={users.find(
            ([uid]) => uid === noteEditors[id]?.userId,
          )?.[1].name ?? ""}
          on:close={() => srocket?.send({ closeNote: [id, note.pageId] })}
          on:update={(event) =>
            srocket?.send({
              updateNote: [id, note.pageId, event.detail],
            })}
          on:editing={(event) =>
            srocket?.send({
              setNoteEditing: [id, note.pageId, event.detail],
            })}
          on:text={(event) => {
            notes = notes.map(([noteId, note]) =>
              noteId === id
                ? [noteId, { ...note, text: event.detail }]
                : [noteId, note],
            );
            srocket?.send({
              updateNoteText: [id, note.pageId, event.detail],
            });
          }}
          on:bringToFront={() =>
            srocket?.send({ updateNote: [id, note.pageId, null] })}
          on:startMove={({ detail: event }) => {
            const startingNote = displayNote;
            const [x, y] = normalizePosition(event);
            movingNoteOrigin = [x - startingNote.x, y - startingNote.y];
            movingNoteState = startingNote;
            movingNote = id;
          }}
          on:startResize={({ detail: event }) => {
            const startingNote = displayNote;
            resizingNoteStartPointer = normalizePosition(event);
            resizingNoteStartBottomRight = [
              startingNote.x + startingNote.width,
              startingNote.y + startingNote.height,
            ];
            resizingNoteStartState = startingNote;
            resizingNoteState = startingNote;
            srocket?.send({ updateNote: [id, note.pageId, null] });
            resizingNote = id;
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
        <LiveCursor {user} />
      </div>
    {/each}
  </div>
</main>
