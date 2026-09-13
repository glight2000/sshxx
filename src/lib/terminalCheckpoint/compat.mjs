/**
 * Version-locked xterm 6.0.0 checkpoint boundary. No terminal UI code may use
 * these internals. Shared by the embedded headless engine and the Web reader.
 * This is a data format, not executable terminal output or a generic object
 * deserializer. Add a conformance test before extending the field allowlists.
 */
export const CHECKPOINT_FORMAT = "sshxx-xterm-6.0.0/1";
export const CHECKPOINT_MAX_CELLS = 300_000;
const BUFFER_FIELDS = [
  "x",
  "y",
  "ybase",
  "savedX",
  "savedY",
  "scrollTop",
  "scrollBottom",
];
const PARAM_FIELDS = [
  "length",
  "_subParamsLength",
  "_rejectDigits",
  "_rejectSubDigits",
  "_digitIsSub",
];
const INPUT_FIELDS = [
  "_windowTitle",
  "_iconName",
  "_windowTitleStack",
  "_iconNameStack",
];
const CORE_FIELDS = [
  "isCursorInitialized",
  "isCursorHidden",
  "modes",
  "decPrivateModes",
];
const DECODER_FIELDS = ["_interim"];

function requireState(ok) {
  if (!ok) throw new Error("Unsupported or invalid terminal checkpoint");
}

function integer(value, min, max) {
  requireState(Number.isSafeInteger(value) && value >= min && value <= max);
  return value;
}

function copyFields(source, keys) {
  return Object.fromEntries(keys.map((key) => [key, source[key]]));
}

function setFields(target, source, keys) {
  for (const key of keys)
    if (Object.hasOwn(source, key)) target[key] = source[key];
}

function coreOf(term) {
  const core = term._core;
  requireState(
    core?._inputHandler?._parser?._params?.params instanceof Int32Array,
  );
  requireState(
    core._bufferService?.buffers?.normal?.getBlankLine instanceof Function,
  );
  requireState(
    core.coreMouseService && core._oscLinkService && core._charsetService,
  );
  return core;
}

function attrData(attr, links, core) {
  if (attr.extended.urlId) {
    const link = core._oscLinkService.getLinkData(attr.extended.urlId);
    if (link) links[attr.extended.urlId] = link;
  }
  return [attr.fg, attr.bg, attr.extended._ext, attr.extended.urlId];
}

function applyAttr(attr, value, linkIds) {
  attr.fg = value[0];
  attr.bg = value[1];
  const extended = attr.extended.clone();
  extended.ext = value[2];
  extended.urlId = linkIds.get(value[3]) ?? 0;
  attr.extended = extended;
}

function saveLine(line, links, core) {
  const extended = {};
  for (const [column, attr] of Object.entries(line._extendedAttrs)) {
    if (attr) {
      extended[column] = [attr._ext, attr.urlId];
      if (attr.urlId) {
        const link = core._oscLinkService.getLinkData(attr.urlId);
        if (link) links[attr.urlId] = link;
      }
    }
  }
  return [
    line.isWrapped,
    Array.from(line._data.subarray(0, line.length * 3)),
    { ...line._combined },
    extended,
  ];
}

function loadLine(buffer, row, cols, linkIds) {
  const line = buffer.getBlankLine(buffer.savedCurAttrData, row[0]);
  line._data = Uint32Array.from(row[1]);
  line.length = cols;
  line._combined = { ...row[2] };
  line._extendedAttrs = {};
  for (const [column, data] of Object.entries(row[3])) {
    const attr = buffer.savedCurAttrData.extended.clone();
    attr.ext = data[0];
    attr.urlId = linkIds.get(data[1]) ?? 0;
    line._extendedAttrs[column] = attr;
  }
  return line;
}

function saveBuffer(buffer, history, core, links) {
  const first = Math.max(0, buffer.ybase - history);
  const state = copyFields(buffer, BUFFER_FIELDS);
  state.ybase -= first;
  state.savedY -= first; // Saved Y is absolute, including scrollback.
  return {
    ...state,
    tabs: { ...buffer.tabs },
    savedCharset: buffer.savedCharset,
    attr: attrData(buffer.savedCurAttrData, links, core),
    lines: Array.from({ length: buffer.lines.length - first }, (_, i) =>
      saveLine(buffer.lines.get(first + i), links, core),
    ),
    first,
  };
}

function saveParams(params) {
  return {
    ...copyFields(params, PARAM_FIELDS),
    params: Array.from(params.params),
    sub: Array.from(params._subParams),
    indices: Array.from(params._subParamsIdx),
  };
}

function loadParams(params, value) {
  setFields(params, value, PARAM_FIELDS);
  params.params.set(value.params);
  params._subParams.set(value.sub);
  params._subParamsIdx.set(value.indices);
}

function saveParser(input) {
  const parser = input._parser;
  requireState(!input._parseStack.paused && !parser._parseStack.state);
  const osc = parser._oscParser;
  const dcs = parser._dcsParser;
  requireState(!osc._stack.paused && !dcs._stack.paused);
  const handlers = (active) =>
    active.map((handler) => {
      requireState(
        typeof handler._data === "string" && handler._data.length <= 65_536,
      );
      return { data: handler._data, hitLimit: handler._hitLimit };
    });
  return {
    state: parser.currentState,
    collect: parser._collect,
    preceding: parser.precedingJoinState,
    params: saveParams(parser._params),
    decoder: copyFields(input._stringDecoder, DECODER_FIELDS),
    osc: { state: osc._state, id: osc._id, active: handlers(osc._active) },
    dcs: {
      id: dcs._ident,
      active: handlers(dcs._active),
      params: dcs._active.length ? saveParams(dcs._active[0]._params) : null,
    },
  };
}

function loadParser(input, state) {
  const parser = input._parser;
  parser.reset();
  loadParams(parser._params, state.params);
  parser.currentState = state.state;
  parser._collect = state.collect;
  parser.precedingJoinState = state.preceding;
  setFields(input._stringDecoder, state.decoder, DECODER_FIELDS);
  const osc = parser._oscParser;
  osc._id = state.osc.id;
  osc._state = state.osc.state;
  if (state.osc.state === 2) osc._start();
  // Built-in OSC handlers and public parser registrations use the same bounded
  // string handler. Browser-only handlers must receive the partial payload too.
  for (const handler of osc._active) {
    requireState(typeof handler._data === "string");
    handler._data = state.osc.active[0]?.data ?? "";
    handler._hitLimit = state.osc.active[0]?.hitLimit ?? false;
  }
  if (state.dcs.active.length) {
    const params = parser._params.clone();
    loadParams(params, state.dcs.params);
    parser._dcsParser.hook(state.dcs.id, params);
    for (const handler of parser._dcsParser._active) {
      requireState(typeof handler._data === "string");
      handler._data = state.dcs.active[0].data;
      handler._hitLimit = state.dcs.active[0].hitLimit;
    }
  }
}

/** Capture only parsed state. The caller supplies the exact UTF-8 stream fence. */
export function captureCheckpoint(term, history) {
  const core = coreOf(term);
  const input = core._inputHandler;
  const buffers = core._bufferService.buffers;
  const links = {};
  const state = {
    format: CHECKPOINT_FORMAT,
    cols: term.cols,
    rows: term.rows,
    alternate: buffers.active === buffers.alt,
    normal: saveBuffer(buffers.normal, history, core, links),
    alt: saveBuffer(buffers.alt, 0, core, links),
    attr: attrData(input._curAttrData, links, core),
    eraseAttr: attrData(input._eraseAttrDataInternal, links, core),
    input: copyFields(input, INPUT_FIELDS),
    core: copyFields(core.coreService, CORE_FIELDS),
    charset: copyFields(core._charsetService, [
      "charset",
      "glevel",
      "_charsets",
    ]),
    mouse: [
      core.coreMouseService.activeProtocol,
      core.coreMouseService.activeEncoding,
    ],
    parser: saveParser(input),
    links,
  };
  // Return detached data: later parsing must not mutate a pending response.
  return JSON.parse(JSON.stringify(state));
}

function validateRows(lines, cols, remaining) {
  requireState(Array.isArray(lines) && lines.length * cols <= remaining);
  for (const row of lines) {
    requireState(
      Array.isArray(row) && row.length === 4 && typeof row[0] === "boolean",
    );
    requireState(Array.isArray(row[1]) && row[1].length === cols * 3);
    for (const word of row[1]) integer(word, 0, 0xffffffff);
    for (const [column, text] of Object.entries(row[2])) {
      integer(Number(column), 0, cols - 1);
      requireState(typeof text === "string" && text.length <= 1024);
    }
    for (const [column, attr] of Object.entries(row[3])) {
      integer(Number(column), 0, cols - 1);
      requireState(Array.isArray(attr) && attr.length === 2);
      integer(attr[0], -0x80000000, 0xffffffff);
      integer(attr[1], 0, Number.MAX_SAFE_INTEGER);
    }
  }
}

function validateAttr(attr) {
  requireState(Array.isArray(attr) && attr.length === 4);
  for (const value of attr.slice(0, 3)) integer(value, -0x80000000, 0xffffffff);
  integer(attr[3], 0, Number.MAX_SAFE_INTEGER);
}

function validateParams(params) {
  integer(params.length, 0, 32);
  integer(params._subParamsLength, 0, 32);
  for (const field of ["_rejectDigits", "_rejectSubDigits", "_digitIsSub"])
    requireState(typeof params[field] === "boolean");
  for (const [field, maximum] of [
    ["params", 0x7fffffff],
    ["sub", 0x7fffffff],
    ["indices", 65535],
  ]) {
    requireState(Array.isArray(params[field]) && params[field].length === 32);
    for (const value of params[field])
      integer(value, field === "indices" ? 0 : -1, maximum);
  }
}

function validateLinks(links) {
  requireState(Object.keys(links).length <= CHECKPOINT_MAX_CELLS);
  for (const [id, link] of Object.entries(links)) {
    integer(Number(id), 1, Number.MAX_SAFE_INTEGER);
    requireState(typeof link.uri === "string" && link.uri.length <= 8192);
    requireState(
      link.id === undefined ||
        (typeof link.id === "string" && link.id.length <= 8192),
    );
  }
}

function validateCharset(charset) {
  if (charset == null) return;
  requireState(
    typeof charset === "object" && Object.keys(charset).length <= 256,
  );
  for (const [key, value] of Object.entries(charset))
    requireState(
      key.length === 1 && typeof value === "string" && value.length <= 2,
    );
}

/** Reject before mutating an existing renderer. Authentication precedes this. */
export function validateCheckpoint(state) {
  requireState(state?.format === CHECKPOINT_FORMAT);
  integer(state.cols, 2, 1000);
  integer(state.rows, 1, 1000);
  requireState(typeof state.alternate === "boolean");
  let cells = CHECKPOINT_MAX_CELLS;
  for (const buffer of [state.normal, state.alt]) {
    validateRows(buffer.lines, state.cols, cells);
    cells -= buffer.lines.length * state.cols;
    integer(buffer.x, 0, state.cols);
    integer(buffer.y, 0, state.rows - 1);
    integer(buffer.ybase, 0, buffer.lines.length);
    integer(buffer.scrollTop, 0, state.rows - 1);
    integer(buffer.scrollBottom, buffer.scrollTop, state.rows - 1);
    integer(buffer.savedX, 0, state.cols);
    integer(buffer.savedY, -CHECKPOINT_MAX_CELLS, CHECKPOINT_MAX_CELLS);
    validateAttr(buffer.attr);
    validateCharset(buffer.savedCharset);
    for (const [key, value] of Object.entries(buffer.tabs)) {
      integer(Number(key), 0, 1000);
      requireState(typeof value === "boolean");
    }
    requireState(
      !buffer.lines.length || buffer.ybase + state.rows === buffer.lines.length,
    );
  }
  requireState(state.normal.lines.length >= state.rows);
  requireState(!state.alternate || state.alt.lines.length >= state.rows);
  integer(state.parser.state, 0, 13);
  validateAttr(state.attr);
  validateAttr(state.eraseAttr);
  validateParams(state.parser.params);
  integer(state.parser.collect, -0x80000000, 0xffffffff);
  integer(state.parser.preceding, -0x80000000, 0xffffffff);
  integer(state.parser.decoder._interim, 0, 65535);
  integer(state.parser.osc.state, 0, 3);
  integer(state.parser.osc.id, -1, Number.MAX_SAFE_INTEGER);
  integer(state.parser.dcs.id, 0, 0xffffffff);
  for (const parser of [state.parser.osc, state.parser.dcs]) {
    requireState(Array.isArray(parser.active) && parser.active.length <= 32);
    for (const handler of parser.active)
      requireState(
        typeof handler.data === "string" &&
          handler.data.length <= 65536 &&
          typeof handler.hitLimit === "boolean",
      );
  }
  if (state.parser.dcs.active.length) validateParams(state.parser.dcs.params);
  for (const field of ["_windowTitle", "_iconName"])
    requireState(
      typeof state.input[field] === "string" &&
        state.input[field].length <= 65536,
    );
  for (const field of ["_windowTitleStack", "_iconNameStack"])
    requireState(
      Array.isArray(state.input[field]) &&
        state.input[field].length <= 10 &&
        state.input[field].every(
          (value) => typeof value === "string" && value.length <= 65536,
        ),
    );
  requireState(
    typeof state.core.isCursorInitialized === "boolean" &&
      typeof state.core.isCursorHidden === "boolean",
  );
  for (const modes of [state.core.modes, state.core.decPrivateModes])
    requireState(
      Object.values(modes).every((value) => typeof value === "boolean"),
    );
  integer(state.charset.glevel, 0, 3);
  validateCharset(state.charset.charset);
  requireState(
    Array.isArray(state.charset._charsets) &&
      state.charset._charsets.length <= 4,
  );
  state.charset._charsets.forEach(validateCharset);
  requireState(
    ["NONE", "X10", "VT200", "DRAG", "ANY"].includes(state.mouse[0]),
  );
  requireState(["DEFAULT", "SGR", "SGR_PIXELS"].includes(state.mouse[1]));
  validateLinks(state.links);
  requireState(
    state.location === undefined ||
      (typeof state.location === "string" && state.location.length <= 8192),
  );
  requireState(
    state.colors === undefined ||
      (Array.isArray(state.colors) && state.colors.length <= 259),
  );
  for (const color of state.colors ?? []) {
    requireState(
      color.type === 1 &&
        Array.isArray(color.color) &&
        color.color.length === 3,
    );
    integer(color.index, 0, 258);
    color.color.forEach((value) => integer(value, 0, 255));
  }
  return state;
}

function installLinks(core, links) {
  const ids = new Map();
  for (const [id, link] of Object.entries(links)) {
    requireState(typeof link.uri === "string" && link.uri.length <= 8192);
    ids.set(
      Number(id),
      core._oscLinkService.registerLink({
        uri: link.uri,
        ...(typeof link.id === "string" ? { id: link.id } : {}),
      }),
    );
  }
  return ids;
}

function attachLineLinks(core, buffer, lines, start = 0) {
  for (let index = 0; index < lines.length; index++) {
    const y = start + index;
    for (const attr of Object.values(lines[index]._extendedAttrs)) {
      const entry = core._oscLinkService._dataByLinkId.get(attr.urlId);
      if (
        entry &&
        !entry.lines.some(
          (marker) => marker.line === y && marker.__sshxxBuffer === buffer,
        )
      ) {
        const marker = buffer.addMarker(y);
        marker.__sshxxBuffer = buffer;
        entry.lines.push(marker);
        marker.onDispose(() =>
          core._oscLinkService._removeMarkerFromLink(entry, marker),
        );
      }
    }
  }
}

function refreshCheckpointView(term, core) {
  const viewport = core._viewport;
  if (viewport) {
    requireState(typeof viewport.queueSync === "function");
    // xterm 6 no longer listens to onRequestSyncScrollBar. Its Viewport also
    // caches the last scroll target separately from buffer.ydisp: queueSync()
    // alone can reuse that old target (often zero after scrolling to the top).
    // Invalidate it so the next paint syncs both dimensions and scroll position
    // from the final buffer, including output/scrolls arriving before the paint.
    // Do not pass ydisp to queueSync: that marks it already synced and skips the
    // actual scrollbar movement. Headless/unopened terminals have no viewport.
    viewport._latestYDisp = undefined;
    viewport.queueSync();
  }
  term.refresh?.(0, term.rows - 1);
  core._onWriteParsed.fire();
}

/** Must run between write-queue batches, with replay replies suppressed. */
export function restoreCheckpoint(term, value, scrollback) {
  const state = validateCheckpoint(value);
  const previous = coreOf(term)._bufferService.buffers;
  previous.normal.clearAllMarkers();
  previous.alt.clearAllMarkers();
  term.reset();
  term.options.scrollback = Math.max(scrollback, state.normal.ybase);
  term.resize(state.cols, state.rows); // Renderer only. Never a PTY resize.
  const core = coreOf(term);
  const buffers = core._bufferService.buffers;
  if (state.alternate) buffers.activateAltBuffer();
  for (const [buffer, saved] of [
    [buffers.normal, state.normal],
    [buffers.alt, state.alt],
  ]) {
    buffer.clearAllMarkers();
    buffer.lines.length = 0;
    // Link markers must refer to installed rows. Install the rows first, then
    // attach lifetime markers after both buffers' contents have been populated.
    for (const row of saved.lines)
      buffer.lines.push(loadLine(buffer, row, state.cols, new Map()));
    setFields(buffer, saved, BUFFER_FIELDS);
    buffer.ydisp = buffer.ybase;
    buffer.tabs = { ...saved.tabs };
    buffer.savedCharset = saved.savedCharset;
  }
  const linkIds = installLinks(core, state.links);
  for (const [buffer, saved] of [
    [buffers.normal, state.normal],
    [buffers.alt, state.alt],
  ]) {
    applyAttr(buffer.savedCurAttrData, saved.attr, linkIds);
    for (let y = 0; y < saved.lines.length; y++) {
      const line = loadLine(buffer, saved.lines[y], state.cols, linkIds);
      buffer.lines.set(y, line);
      attachLineLinks(core, buffer, [line], y);
    }
  }
  applyAttr(core._inputHandler._curAttrData, state.attr, linkIds);
  applyAttr(
    core._inputHandler._eraseAttrDataInternal,
    state.eraseAttr,
    linkIds,
  );
  setFields(core._inputHandler, state.input, INPUT_FIELDS);
  setFields(core.coreService, state.core, CORE_FIELDS);
  setFields(core._charsetService, state.charset, [
    "charset",
    "glevel",
    "_charsets",
  ]);
  core.coreMouseService.activeProtocol = state.mouse[0];
  core.coreMouseService.activeEncoding = state.mouse[1];
  loadParser(core._inputHandler, state.parser);
  core._inputHandler._onColor.fire([{ type: 2 }, ...(state.colors ?? [])]);
  core._bufferService.isUserScrolling = false;
  refreshCheckpointView(term, core);
}

/** Embedded engine only: synchronous parsing with no timers or PTY responses. */
export function parseCheckpointOutput(term, text) {
  const result = coreOf(term)._inputHandler.parse(text);
  requireState(!result || typeof result.then !== "function");
}

/** Historical cells are never fed into the live escape-sequence parser. */
export function captureHistoryRows(term, start, end) {
  const core = coreOf(term);
  const buffer = core._bufferService.buffers.normal;
  integer(start, 0, buffer.ybase);
  integer(end, start, Math.min(buffer.ybase, start + 1000));
  const links = {};
  return {
    format: CHECKPOINT_FORMAT,
    cols: term.cols,
    lines: Array.from({ length: end - start }, (_, i) =>
      saveLine(buffer.lines.get(start + i), links, core),
    ),
    links,
  };
}

export function prependHistoryRows(term, page, scrollback) {
  requireState(page.format === CHECKPOINT_FORMAT && page.cols === term.cols);
  validateRows(page.lines, term.cols, CHECKPOINT_MAX_CELLS);
  validateLinks(page.links);
  const core = coreOf(term);
  const buffer = core._bufferService.buffers.normal;
  const count = page.lines.length;
  requireState(buffer.ybase + count <= scrollback);
  term.options.scrollback = scrollback;
  const ids = installLinks(core, page.links);
  const lines = page.lines.map((row) => loadLine(buffer, row, term.cols, ids));
  buffer.lines.splice(0, 0, ...lines);
  buffer.ybase += count;
  buffer.ydisp += count;
  buffer.savedY += count;
  attachLineLinks(core, buffer, lines);
  refreshCheckpointView(term, core);
}
