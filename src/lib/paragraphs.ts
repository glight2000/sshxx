export type ParagraphReorderResult = {
  paragraphs: string[];
  selectedIndexes: number[];
};

export const PARAGRAPH_CLIPBOARD_TYPE =
  "application/x-sshxx-note-paragraphs+json";

export function normalizeParagraphIndexes(
  indexes: readonly number[],
  paragraphCount: number,
) {
  return [...new Set(indexes)]
    .filter((index) => index >= 0 && index < paragraphCount)
    .sort((left, right) => left - right);
}

export function selectedParagraphs(
  paragraphs: readonly string[],
  indexes: readonly number[],
) {
  return normalizeParagraphIndexes(indexes, paragraphs.length).map(
    (index) => paragraphs[index],
  );
}

/** Plain-text projection used by terminals, file editors, and system clipboards. */
export function paragraphPlainText(paragraphs: readonly string[]) {
  return paragraphs.join("\n");
}

export function serializeParagraphs(paragraphs: readonly string[]) {
  return JSON.stringify({ version: 1, paragraphs });
}

export function deserializeParagraphs(value: string): string[] | null {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!parsed || typeof parsed !== "object") return null;
    const record = parsed as { version?: unknown; paragraphs?: unknown };
    if (
      record.version !== 1 ||
      !Array.isArray(record.paragraphs) ||
      record.paragraphs.length === 0 ||
      record.paragraphs.length > 500 ||
      record.paragraphs.some((paragraph) => typeof paragraph !== "string")
    )
      return null;
    const paragraphs = record.paragraphs as string[];
    return paragraphPlainText(paragraphs).length <= 10_000
      ? [...paragraphs]
      : null;
  } catch {
    return null;
  }
}

export function deleteParagraphs(
  paragraphs: readonly string[],
  selectedIndexes: readonly number[],
) {
  const indexes = normalizeParagraphIndexes(selectedIndexes, paragraphs.length);
  if (!indexes.length) return { paragraphs: [...paragraphs], selectedIndex: 0 };
  const selected = new Set(indexes);
  const remaining = paragraphs.filter((_, index) => !selected.has(index));
  if (!remaining.length) remaining.push("");
  return {
    paragraphs: remaining,
    selectedIndex: Math.min(indexes[0], remaining.length - 1),
  };
}

/** Move selected paragraphs as one stable block to the requested source-list gap. */
export function reorderParagraphs(
  paragraphs: readonly string[],
  selectedIndexes: readonly number[],
  targetIndex: number,
): ParagraphReorderResult {
  const indexes = normalizeParagraphIndexes(selectedIndexes, paragraphs.length);
  if (!indexes.length)
    return { paragraphs: [...paragraphs], selectedIndexes: [] };

  const selected = new Set(indexes);
  const moving = indexes.map((index) => paragraphs[index]);
  const remaining = paragraphs.filter((_, index) => !selected.has(index));
  const boundedTarget = Math.max(0, Math.min(targetIndex, paragraphs.length));
  const removedBeforeTarget = indexes.filter(
    (index) => index < boundedTarget,
  ).length;
  const insertionIndex = Math.max(
    0,
    Math.min(boundedTarget - removedBeforeTarget, remaining.length),
  );
  remaining.splice(insertionIndex, 0, ...moving);
  return {
    paragraphs: remaining,
    selectedIndexes: moving.map((_, offset) => insertionIndex + offset),
  };
}
