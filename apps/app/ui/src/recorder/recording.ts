import type { Command } from "../bridge/protocol";
import type { Refusal } from "../core/model";
import { Observable } from "../state/observable";

/** Where the begin and end of recording go: the daemon client. */
interface CommandSender {
  send(command: Command): Promise<Refusal | null>;
}

/**
 * Which shortcut recorder is recording, if any. Only one records at a time: starting
 * another moves recording to it. The daemon keeps Snappy's own shortcuts unregistered from
 * the first start to the last stop, so a chord they hold reaches the recorder.
 */
export class Recording {
  /** The recording recorder's ID, or null while none records. */
  readonly active = new Observable<string | null>(null);

  constructor(private readonly client: CommandSender) {}

  start(recorder: string): void {
    const previous = this.active.get();
    if (previous === recorder) {
      return;
    }
    this.active.set(recorder);
    if (previous === null) {
      void this.client.send({ kind: "beginRecording" });
    }
  }

  /** Stops `recorder`, when it is the one recording. */
  stop(recorder: string): void {
    if (this.active.get() !== recorder) {
      return;
    }
    this.active.set(null);
    void this.client.send({ kind: "endRecording" });
  }

  isRecording(recorder: string): boolean {
    return this.active.get() === recorder;
  }
}
