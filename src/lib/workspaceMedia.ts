import type {
  ChatRecord,
  WireChatRecord,
  WorkspaceAttachment,
} from "./protocol";
import { type FileRequestClient, randomHex } from "./fileRequests.ts";
import { decodeDownloadContent, safeDownloadName } from "./fileDownload.ts";

export const MAX_ATTACHMENT_BYTES = 20 << 20;
const CHUNK = 64 << 10;
const PREVIEW_BUDGET = 64 << 20;
const MEDIA_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
  "image/avif",
  "video/mp4",
  "video/webm",
]);

/** Preserve ordering and deduplicate a live event that overlaps initial history. */
export function mergeChatHistory(
  history: ChatRecord[],
  incoming: WireChatRecord[],
) {
  const records = new Map(history.map((record) => [record.id, record]));
  for (const record of incoming)
    records.set(record.id, { ...record, sentAt: Number(record.sentAt) });
  return [...records.values()].sort((a, b) => a.sentAt - b.sentAt).slice(-500);
}

export function attachmentMetadata(file: File): WorkspaceAttachment {
  if (file.size === 0 || file.size > MAX_ATTACHMENT_BYTES)
    throw new Error("Choose a non-empty file up to 20 MiB.");
  if (file.type.startsWith("audio/"))
    throw new Error("Audio sending is not supported.");
  const name = safeDownloadName(file.name);
  if (new TextEncoder().encode(name).length > 255)
    throw new Error("The file name is too long.");
  return {
    id: randomHex(16),
    name,
    mediaType: MEDIA_TYPES.has(file.type)
      ? file.type
      : "application/octet-stream",
    size: file.size,
  };
}

/** One session's bounded, encrypted attachment transfers and preview URLs. */
export class WorkspaceMedia {
  private retainedBytes = 0;
  private releases = new Set<() => void>();
  private uploading = false;
  private disposed = false;
  private requests: FileRequestClient;
  constructor(requests: FileRequestClient) {
    this.requests = requests;
  }

  async upload(
    file: File,
    signal: AbortSignal,
    progress: (value: number) => void,
  ) {
    if (this.uploading)
      throw new Error("Another attachment is uploading. Please wait.");
    const metadata = attachmentMetadata(file);
    this.uploading = true;
    try {
      for (let offset = 0; offset < file.size; offset += CHUNK) {
        signal.throwIfAborted();
        if (this.disposed) throw new Error("The workspace was closed.");
        const bytes = new Uint8Array(
          await file.slice(offset, offset + CHUNK).arrayBuffer(),
        );
        const content = btoa(
          Array.from(bytes, (byte) => String.fromCharCode(byte)).join(""),
        );
        const response = await this.requests.requestAttachment({
          operation: "write",
          path: metadata.id,
          offset,
          total: file.size,
          content,
        });
        if (!response.ok)
          throw new Error(response.error || "Attachment upload failed.");
        if (response.size !== offset + bytes.length)
          throw new Error("Attachment upload returned an invalid offset.");
        progress(Math.round(((offset + bytes.length) / file.size) * 100));
      }
      signal.throwIfAborted();
      return metadata;
    } finally {
      this.uploading = false;
    }
  }

  async open(attachment: WorkspaceAttachment, signal: AbortSignal) {
    if (
      !Number.isInteger(attachment.size) ||
      attachment.size <= 0 ||
      attachment.size > MAX_ATTACHMENT_BYTES
    )
      throw new Error("Invalid attachment size.");
    if (
      this.releases.size >= 4 ||
      this.retainedBytes + attachment.size > PREVIEW_BUDGET
    )
      throw new Error(
        "Close another attachment preview before opening more (64 MiB limit).",
      );
    this.retainedBytes += attachment.size;
    let url = "";
    const release = () => {
      if (!this.releases.delete(release)) return;
      this.retainedBytes -= attachment.size;
      if (url) URL.revokeObjectURL(url);
    };
    this.releases.add(release);
    try {
      const data = new Uint8Array(attachment.size);
      for (let offset = 0; offset < data.length;) {
        signal.throwIfAborted();
        if (this.disposed) throw new Error("The workspace was closed.");
        const response = await this.requests.requestAttachment({
          operation: "read",
          path: attachment.id,
          offset,
        });
        if (!response.ok)
          throw new Error(response.error || "Attachment download failed.");
        const bytes = decodeDownloadContent(response.content || "", "base64");
        if (
          response.size !== data.length ||
          bytes.length === 0 ||
          bytes.length > CHUNK ||
          offset + bytes.length > data.length
        )
          throw new Error("Attachment size does not match its saved metadata.");
        data.set(bytes, offset);
        offset += bytes.length;
      }
      signal.throwIfAborted();
      if (this.disposed) throw new Error("The workspace was closed.");
      url = URL.createObjectURL(
        new Blob([data.buffer], {
          type: MEDIA_TYPES.has(attachment.mediaType)
            ? attachment.mediaType
            : "application/octet-stream",
        }),
      );
      return { url, release };
    } catch (error) {
      release();
      throw error;
    }
  }

  dispose() {
    this.disposed = true;
    for (const release of this.releases) release();
  }
}
