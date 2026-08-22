export const FILE_DOWNLOAD_LIMIT_BYTES = 8 << 20;

export type DownloadEncoding = "utf8" | "utf16le" | "utf16be" | "base64";

export function decodeDownloadContent(
  content: string,
  encoding: DownloadEncoding,
): Uint8Array {
  if (encoding === "base64") {
    const binary = atob(content);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1)
      bytes[index] = binary.charCodeAt(index);
    return bytes;
  }
  if (encoding === "utf8") return new TextEncoder().encode(content);

  const littleEndian = encoding === "utf16le";
  const bytes = new Uint8Array(2 + content.length * 2);
  bytes[0] = littleEndian ? 0xff : 0xfe;
  bytes[1] = littleEndian ? 0xfe : 0xff;
  for (let index = 0; index < content.length; index += 1) {
    const unit = content.charCodeAt(index);
    bytes[2 + index * 2] = littleEndian ? unit & 0xff : unit >> 8;
    bytes[3 + index * 2] = littleEndian ? unit >> 8 : unit & 0xff;
  }
  return bytes;
}

export function safeDownloadName(name: string) {
  const cleaned = name.replace(/[\0\u0001-\u001f\u007f/\\]/g, "_").trim();
  return cleaned && cleaned !== "." && cleaned !== ".." ? cleaned : "download";
}

export function startBrowserDownload(
  bytes: Uint8Array,
  name: string,
  type: string,
) {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  const url = URL.createObjectURL(new Blob([copy.buffer], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = safeDownloadName(name);
  anchor.hidden = true;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}
