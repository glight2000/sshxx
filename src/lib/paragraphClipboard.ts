import {
  deserializeParagraphs,
  PARAGRAPH_CLIPBOARD_TYPE,
  paragraphPlainText,
  serializeParagraphs,
} from "./paragraphs";

type RememberedClipboard = {
  paragraphs: string[];
  text: string;
};

let remembered: RememberedClipboard | null = null;

export function rememberParagraphs(paragraphs: readonly string[]) {
  remembered = {
    paragraphs: [...paragraphs],
    text: paragraphPlainText(paragraphs),
  };
  return remembered.text;
}

export function writeParagraphClipboard(
  clipboard: DataTransfer,
  paragraphs: readonly string[],
) {
  const text = rememberParagraphs(paragraphs);
  clipboard.setData("text/plain", text);
  clipboard.setData(PARAGRAPH_CLIPBOARD_TYPE, serializeParagraphs(paragraphs));
}

export function readParagraphClipboard(clipboard: DataTransfer) {
  const encoded = clipboard.getData(PARAGRAPH_CLIPBOARD_TYPE);
  const structured = encoded ? deserializeParagraphs(encoded) : null;
  if (structured) {
    rememberParagraphs(structured);
    return structured;
  }
  const text = clipboard.getData("text/plain");
  return remembered && remembered.text === text
    ? [...remembered.paragraphs]
    : null;
}

export async function copyParagraphs(paragraphs: readonly string[]) {
  const text = rememberParagraphs(paragraphs);
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // LAN HTTP and embedded webviews may deny the async Clipboard API.
    }
  }
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.readOnly = true;
  Object.assign(textarea.style, {
    position: "fixed",
    left: "-10000px",
    top: "-10000px",
  });
  document.body.append(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) throw new Error("Clipboard access is unavailable.");
}
