export function mobileViewport(node: HTMLElement, active: boolean) {
  const viewport = window.visualViewport;
  const measure = () => {
    if (!active) return;
    node.style.setProperty(
      "--mobile-viewport-top",
      `${viewport?.offsetTop ?? 0}px`,
    );
    node.style.setProperty(
      "--mobile-viewport-height",
      `${viewport?.height ?? window.innerHeight}px`,
    );
    node.style.setProperty(
      "--mobile-viewport-left",
      `${viewport?.offsetLeft ?? 0}px`,
    );
    node.style.setProperty(
      "--mobile-viewport-width",
      `${viewport?.width ?? window.innerWidth}px`,
    );
  };
  viewport?.addEventListener("resize", measure);
  viewport?.addEventListener("scroll", measure);
  window.addEventListener("resize", measure);
  measure();
  return {
    update(next: boolean) {
      active = next;
      measure();
    },
    destroy() {
      viewport?.removeEventListener("resize", measure);
      viewport?.removeEventListener("scroll", measure);
      window.removeEventListener("resize", measure);
      node.style.removeProperty("--mobile-viewport-top");
      node.style.removeProperty("--mobile-viewport-height");
      node.style.removeProperty("--mobile-viewport-left");
      node.style.removeProperty("--mobile-viewport-width");
    },
  };
}
