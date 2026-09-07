/** Phone relation buttons: tap to visit, hold for actions, drag to scroll. */
export function mobileRelationPress(
  node: HTMLElement,
  actions: { tap(): void; hold(): void },
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pointer: { id: number; x: number; y: number } | null = null;
  let suppressClick = false;
  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
  };
  const down = (event: PointerEvent) => {
    if (event.pointerType !== "touch") return;
    cancel();
    suppressClick = false;
    pointer = { id: event.pointerId, x: event.clientX, y: event.clientY };
    timer = setTimeout(() => {
      timer = undefined;
      suppressClick = true;
      actions.hold();
    }, 500);
  };
  const move = (event: PointerEvent) => {
    if (!pointer || pointer.id !== event.pointerId) return;
    if (Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) > 8) {
      suppressClick = true;
      cancel();
    }
  };
  const end = (event: PointerEvent) => {
    if (pointer?.id !== event.pointerId) return;
    if (event.type === "pointercancel") suppressClick = true;
    pointer = null;
    cancel();
  };
  const click = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    if (!suppressClick || event.detail === 0) actions.tap();
    suppressClick = false;
  };
  const context = (event: Event) => {
    event.preventDefault();
    event.stopPropagation();
    // Some phone browsers emit contextmenu before the hold timer expires.
    cancel();
    suppressClick = true;
    actions.hold();
  };
  node.addEventListener("pointerdown", down);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
  node.addEventListener("click", click);
  node.addEventListener("contextmenu", context);
  return {
    update(next: typeof actions) {
      actions = next;
    },
    destroy() {
      cancel();
      node.removeEventListener("pointerdown", down);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      node.removeEventListener("click", click);
      node.removeEventListener("contextmenu", context);
    },
  };
}
