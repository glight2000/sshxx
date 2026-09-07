import type { WsPage, WsNote } from "./protocol";
import type { CanvasSearchItem } from "./ui/TerminalSearch.svelte";

// A narrow desktop window is not a phone. Also allow touch-screen landscape.
export const MOBILE_NAVIGATION_QUERY =
  "(pointer: coarse) and (max-width: 1024px)";

export function groupNavigationItems(
  pages: WsPage[],
  items: CanvasSearchItem[],
) {
  const groups = pages.map((page) => ({
    page,
    items: [] as CanvasSearchItem[],
  }));
  const byPage = new Map(groups.map((group) => [group.page.id, group]));
  for (const item of items) byPage.get(item.pageId)?.items.push(item);
  return groups;
}

export function navigationItemKey(item: Pick<CanvasSearchItem, "kind" | "id">) {
  return `${item.kind}:${item.id}`;
}

/** Same-page association rules, not a new phone-specific relationship model. */
export function mobileAssociationTargets(
  items: CanvasSearchItem[],
  noteId: number,
  note: WsNote | undefined,
  associatedNotes: number[],
) {
  if (!note) return [];
  return items.filter((item) => {
    if (item.pageId !== note.pageId) return false;
    if (item.kind === "terminal") return !note.linkedShellIds.includes(item.id);
    if (item.kind === "file")
      return !note.linkedFileWindowIds.includes(item.id);
    return (
      item.kind === "note" &&
      item.id !== noteId &&
      !associatedNotes.includes(item.id)
    );
  });
}
