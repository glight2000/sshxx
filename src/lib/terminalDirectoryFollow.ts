import type { FileOperationResponse } from "./protocol.ts";
import type { TerminalLocation } from "./terminalLocation.ts";

type FollowCallbacks = {
  probe(): Promise<FileOperationResponse>;
  reveal(path: string, current: () => boolean): Promise<boolean>;
  status(message: string): void;
};

/** One enabled browser window owns one probe/listing at a time. */
export class TerminalDirectoryFollow {
  private scope: string | null = null;
  private paused = false;
  private report: TerminalLocation | null = null;
  private host: string | null = null;
  private lastPath = "";
  private revision = 0;
  private busy = false;
  private timer?: ReturnType<typeof setTimeout>;

  private readonly callbacks: FollowCallbacks;

  constructor(callbacks: FollowCallbacks) {
    this.callbacks = callbacks;
  }

  update(
    scope: string | null,
    paused: boolean,
    report: TerminalLocation | null,
  ) {
    const reset = scope !== this.scope;
    const changed =
      report?.workingDirectory !== this.report?.workingDirectory ||
      report?.workingDirectoryHost !== this.report?.workingDirectoryHost;
    this.report = report;
    if (!reset && paused === this.paused && !(this.host && changed)) return;
    if (reset) {
      this.host = null;
    }
    if (reset || (this.paused && !paused)) this.lastPath = "";
    this.scope = scope;
    this.paused = paused;
    this.revision++;
    clearTimeout(this.timer);
    if (!scope) this.callbacks.status("");
    else if (paused)
      this.callbacks.status(
        "Following paused while the file browser is in use.",
      );
    else if (!this.busy) this.schedule(150);
  }

  private schedule(delay: number) {
    this.timer = setTimeout(() => void this.refresh(), delay);
  }

  private async refresh() {
    const revision = this.revision;
    const current = () => revision === this.revision;
    this.busy = true;
    let poll = false;
    try {
      let path = "";
      if (this.host === null) {
        const result = await this.callbacks.probe();
        if (!current()) return;
        if (result.operation !== "terminalDirectory")
          throw new Error("Update the daemon to follow terminal directories.");
        if (!result.ok)
          throw new Error(result.error || "Directory lookup failed.");
        this.host =
          result.directoryHost?.toLowerCase().replace(/\.$/, "") ?? null;
        path = result.path;
      }
      if (this.host !== null) {
        if (!this.report?.workingDirectoryHost) {
          this.callbacks.status(
            "Waiting for the SSH shell to report its directory (OSC 7).",
          );
          return;
        }
        if (this.report.workingDirectoryHost !== this.host) {
          this.callbacks.status(
            "Following paused: the reported host differs from the file browser’s SSH host.",
          );
          return;
        }
        path = this.report.workingDirectory;
      }
      if (!path)
        throw new Error("The terminal host did not report a directory.");
      if (path !== this.lastPath) {
        if ((await this.callbacks.reveal(path, current)) && current())
          this.lastPath = path;
      }
      if (current()) this.callbacks.status("");
      poll = this.host === null;
    } catch (cause) {
      if (current()) {
        const reason =
          cause instanceof Error ? cause.message : "Directory lookup failed.";
        this.callbacks.status(`${reason} Toggle following to retry.`);
      }
    } finally {
      this.busy = false;
      if (this.scope && !this.paused) {
        if (!current()) this.schedule(150);
        else if (poll) this.schedule(1500);
      }
    }
  }

  dispose() {
    this.update(null, false, null);
  }
}
