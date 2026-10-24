import type { CanvasItemKind } from "./canvasSelection";

/** Browser-local search projection shared by desktop and phone navigation. */
export type CanvasSearchItem = {
  id: number;
  kind: CanvasItemKind;
  pageId: number;
  pageName: string;
  title: string;
  content: string;
};
