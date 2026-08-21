import type { Encrypt } from "./encrypt";
import type {
  FileOperationRequest,
  FileOperationResponse,
  WsClient,
} from "./protocol";

type PendingRequest = {
  stream: bigint;
  resolve: (response: FileOperationResponse) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

const FILE_OPERATIONS = new Set<FileOperationRequest["operation"]>([
  "list",
  "read",
  "write",
  "createFile",
  "createDirectory",
  "rename",
  "move",
  "delete",
]);

export function randomHex(bytes: number) {
  if (!Number.isInteger(bytes) || bytes <= 0)
    throw new Error("Random byte length must be a positive integer.");
  const data = new Uint8Array(bytes);
  crypto.getRandomValues(data);
  return Array.from(data, (value) => value.toString(16).padStart(2, "0")).join(
    "",
  );
}

export function randomEncryptedStream() {
  const data = new Uint8Array(8);
  crypto.getRandomValues(data);
  let stream = 0n;
  for (const value of data) stream = (stream << 8n) | BigInt(value);
  return stream | (1n << 63n);
}

function decodeResponse(data: Uint8Array): FileOperationResponse {
  const parsed = JSON.parse(new TextDecoder().decode(data)) as unknown;
  if (
    !parsed ||
    typeof parsed !== "object" ||
    typeof (parsed as FileOperationResponse).ok !== "boolean" ||
    typeof (parsed as FileOperationResponse).path !== "string" ||
    !FILE_OPERATIONS.has((parsed as FileOperationResponse).operation)
  )
    throw new Error("The daemon returned an invalid filesystem response.");
  return parsed as FileOperationResponse;
}

/** Owns encrypted filesystem request correlation, timeout, and cleanup. */
export class FileRequestClient {
  private readonly pending = new Map<string, PendingRequest>();
  private readonly encrypt: Encrypt;
  private readonly isConnected: () => boolean;
  private readonly send: (message: WsClient) => void;

  constructor(
    encrypt: Encrypt,
    isConnected: () => boolean,
    send: (message: WsClient) => void,
  ) {
    this.encrypt = encrypt;
    this.isConnected = isConnected;
    this.send = send;
  }

  async request(
    shellId: number,
    pageId: number,
    request: FileOperationRequest,
  ): Promise<FileOperationResponse> {
    if (!this.isConnected()) throw new Error("The daemon is not connected.");
    const requestId = randomHex(16);
    const requestStream = randomEncryptedStream();
    let responseStream = randomEncryptedStream();
    while (responseStream === requestStream)
      responseStream = randomEncryptedStream();
    const plaintext = new TextEncoder().encode(JSON.stringify(request));
    const data = await this.encrypt.segment(requestStream, 0n, plaintext);
    const response = new Promise<FileOperationResponse>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        reject(new Error("Filesystem request timed out."));
      }, 35_000);
      this.pending.set(requestId, {
        stream: responseStream,
        resolve,
        reject,
        timer,
      });
    });
    this.send({
      fileRequest: [
        shellId,
        pageId,
        requestId,
        requestStream,
        responseStream,
        data,
      ],
    });
    return response;
  }

  handleResponse(requestId: string, stream: bigint, data: Uint8Array) {
    const pending = this.pending.get(requestId);
    if (!pending || stream !== pending.stream) return false;
    this.pending.delete(requestId);
    clearTimeout(pending.timer);
    void this.encrypt
      .segment(pending.stream, 0n, data)
      .then(decodeResponse)
      .then(pending.resolve, (cause) =>
        pending.reject(
          cause instanceof Error ? cause : new Error(String(cause)),
        ),
      );
    return true;
  }

  rejectAll(reason: string) {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error(reason));
    }
    this.pending.clear();
  }

  dispose() {
    this.rejectAll("Filesystem request client was disposed.");
  }
}
