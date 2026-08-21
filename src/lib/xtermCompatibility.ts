import type { IBufferCell, Terminal } from "@xterm/xterm";

type XtermPrivateSurface = {
  _core?: {
    _inputHandler?: {
      getAttrData?: () => IBufferCell;
    };
  };
};

function attributeReader(terminal: Terminal) {
  const handler = (terminal as unknown as XtermPrivateSurface)._core
    ?._inputHandler;
  return handler && typeof handler.getAttrData === "function"
    ? handler.getAttrData.bind(handler)
    : null;
}

/** Capability check for the only xterm private API required by TypeAhead. */
export function supportsTypeAheadAttributes(terminal: Terminal) {
  return attributeReader(terminal) !== null;
}

/**
 * Read the current SGR attributes used to roll local predictions back.
 *
 * xterm has no public equivalent. Keep this access isolated so an xterm
 * upgrade either passes the capability check or disables TypeAhead cleanly.
 */
export function currentTypeAheadAttributes(terminal: Terminal): IBufferCell {
  const read = attributeReader(terminal);
  if (!read)
    throw new Error("This xterm version does not expose TypeAhead attributes.");
  return read();
}
