import type { Encrypt } from "../encrypt";
import type { WsClient, WsServer } from "../protocol";

export type TerminalCheckpointResult = { sequence: number; state: any };
type Request = {
  epoch: number;
  id: number;
  generation: number;
  message: WsClient;
  resolve(result: TerminalCheckpointResult): void;
  reject(error: Error): void;
  timer?: ReturnType<typeof setTimeout>;
};

/** Correlation, rate control, deadlines, and decryption; never owns a renderer. */
export class TerminalCheckpointClient {
  private requests = new Map<string, Request>();
  private queue: string[] = [];
  private timer?: ReturnType<typeof setTimeout>;
  private disposed = false;
  private epoch = 0;

  private encrypt: Encrypt;
  private send: (message: WsClient) => void;
  constructor(encrypt: Encrypt, send: (message: WsClient) => void) {
    this.encrypt = encrypt;
    this.send = send;
  }

  async latest(id: number, page: number, generation: number, history: number) {
    const epoch = this.epoch;
    for (let attempt = 0; ; attempt++) {
      try {
        return await this.request(id, page, generation, history);
      } catch (error) {
        if (attempt >= 2 || this.disposed || epoch !== this.epoch) throw error;
        await new Promise((resolve) => setTimeout(resolve, 350));
        if (epoch !== this.epoch) throw error;
      }
    }
  }

  request(
    id: number,
    page: number,
    generation: number,
    history: number,
    archive?: { epoch: number; before: number },
  ): Promise<TerminalCheckpointResult> {
    if (this.disposed || this.requests.size >= 100)
      return Promise.reject(new Error("Terminal history requests unavailable"));
    const requestId = Array.from(
      crypto.getRandomValues(new Uint8Array(16)),
      (value) => value.toString(16).padStart(2, "0"),
    ).join("");
    return new Promise((resolve, reject) => {
      this.requests.set(requestId, {
        epoch: this.epoch,
        id,
        generation,
        resolve,
        reject,
        message: {
          terminalCheckpoint: [
            id,
            page,
            generation,
            requestId,
            Math.min(10000, Math.max(0, Math.floor(history))),
            archive?.epoch ?? null,
            archive?.before ?? 0,
          ],
        },
      });
      this.queue.push(requestId);
      this.drain();
    });
  }

  private drain() {
    if (this.timer || this.disposed || !this.queue.length) return;
    const pending = [...this.requests.values()].filter(
      (request) => request.timer,
    ).length;
    if (pending < 4) {
      const id = this.queue.shift()!;
      const request = this.requests.get(id);
      if (request) {
        request.timer = setTimeout(() => {
          this.requests.delete(id);
          request.reject(new Error("Terminal history request timed out"));
        }, 4000);
        this.send(request.message);
      }
    }
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.drain();
    }, 70);
  }

  async accept(response: NonNullable<WsServer["terminalCheckpoint"]>) {
    const request = this.requests.get(response.requestId);
    if (!request) return;
    this.requests.delete(response.requestId);
    clearTimeout(request.timer);
    try {
      if (
        response.id !== request.id ||
        response.generation !== request.generation ||
        !response.data.length
      )
        throw new Error(
          "Terminal checkpoint unavailable; using retained output",
        );
      const plaintext = await this.encrypt.openCheckpoint(
        response.nonce,
        response.data,
      );
      const value = JSON.parse(
        new TextDecoder("utf-8", { fatal: true }).decode(plaintext),
      );
      if (
        this.disposed ||
        request.epoch !== this.epoch ||
        value.id !== request.id ||
        value.requestId !== response.requestId ||
        value.generation !== request.generation ||
        value.sequence !== response.sequence ||
        !Number.isSafeInteger(value.sequence) ||
        value.sequence < 0 ||
        !value.state
      )
        throw new Error("Terminal checkpoint identity mismatch");
      request.resolve({ sequence: value.sequence, state: value.state });
    } catch {
      request.reject(
        new Error("Terminal checkpoint could not be loaded safely"),
      );
    }
  }

  reset() {
    this.epoch++;
    clearTimeout(this.timer);
    this.timer = undefined;
    for (const request of this.requests.values()) {
      clearTimeout(request.timer);
      request.reject(new Error("Terminal history connection closed"));
    }
    this.requests.clear();
    this.queue.length = 0;
  }

  dispose() {
    this.disposed = true;
    this.reset();
  }
}
