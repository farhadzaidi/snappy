import type { Transport } from "../bridge/bridge";
import type { AppState, Command, KeyChord, Refusal, Status, ToSettings } from "../core/model";
import { Observable } from "./observable";

/** Non-error messages hide after this long. */
export const TOAST_DURATION_MS = 3000;

export interface SendOptions {
  /**
   * The caller shows a refusal where the change was made, so it isn't also a toast. The
   * daemon posts every refusal as its status too; that copy is dismissed rather than shown.
   */
  inPlace?: boolean;
}

export interface Toast {
  key: number;
  message: string;
  isError: boolean;
  /** Set when the toast shows the daemon's status, so dismissing it can tell the daemon. */
  fromStatus: boolean;
  /** The title of a setting the message names, which the toast links to. */
  setting: string | null;
}

/**
 * The settings app's view of the daemon. State arrives only in `State` messages; user
 * actions leave as commands, each answered by a reply.
 */
export class DaemonClient {
  /** The latest snapshot, or null before the daemon has sent one. */
  readonly state = new Observable<AppState | null>(null);
  readonly toast = new Observable<Toast | null>(null);

  private nextCommandId = 1;
  private nextToastKey = 1;
  private toastTimer: ReturnType<typeof setTimeout> | undefined;
  private readonly replies = new Map<
    number,
    { resolve: (error: Refusal | null) => void; inPlace: boolean }
  >();
  /** A refusal shown in place, whose status copy from the daemon is due next. */
  private refusalShownInPlace: string | null = null;
  private readonly probes: ((available: boolean) => void)[] = [];

  constructor(private readonly transport: Transport) {}

  connect(): () => void {
    return this.transport.listen((message) => this.receive(message));
  }

  /** Send a command and resolve with its refusal, or null on success. Refusals also show as a toast. */
  send(command: Command, { inPlace = false }: SendOptions = {}): Promise<Refusal | null> {
    const id = this.nextCommandId++;
    const reply = new Promise<Refusal | null>((resolve) => this.replies.set(id, { resolve, inPlace }));
    this.transport.send(id, command);
    return reply;
  }

  /** Ask whether the system and other apps leave `chord` free. */
  probe(chord: KeyChord): Promise<boolean> {
    const result = new Promise<boolean>((resolve) => this.probes.push(resolve));
    void this.send({ kind: "probeShortcut", chord });
    return result;
  }

  /** Show a message that comes from the settings app itself, naming `setting` when given. */
  announce(message: string, isError: boolean, setting: string | null = null): void {
    this.showToast({ message, isError, fromStatus: false, setting });
  }

  dismissToast(): void {
    const toast = this.toast.get();
    this.clearToast();
    if (toast?.fromStatus && toast.isError) {
      void this.send({ kind: "dismissStatus" });
    }
  }

  private receive(message: ToSettings): void {
    switch (message.type) {
      case "state": {
        const { type: _type, ...next } = message;
        const previous = this.state.get();
        this.state.set(next);
        this.showStatus(previous, next.status);
        break;
      }
      case "reply":
        this.resolveReply(message.id, message.error);
        break;
      case "probeResult":
        this.probes.shift()?.(message.available);
        break;
      default:
        break;
    }
  }

  private resolveReply(id: number, error: Refusal | null): void {
    const pending = this.replies.get(id);
    this.replies.delete(id);
    if (error !== null && pending?.inPlace) {
      this.refusalShownInPlace = error.message;
    } else if (error !== null) {
      this.announce(error.message, true, error.setting);
    }
    pending?.resolve(error);
  }

  /**
   * Notifying statuses show; quiet ones dismiss a visible error. At first contact
   * only an error is shown, the one raised while the window was closed.
   */
  private showStatus(previous: AppState | null, status: Status | null): void {
    if (!status || status.id === previous?.status?.id) {
      return;
    }
    if (status.isError && status.message === this.refusalShownInPlace) {
      this.refusalShownInPlace = null;
      void this.send({ kind: "dismissStatus" });
      return;
    }
    if (previous === null && !status.isError) {
      return;
    }
    if (!status.notify) {
      if (this.toast.get()?.isError) {
        this.clearToast();
      }
      return;
    }
    this.showToast({ message: status.message, isError: status.isError, fromStatus: true, setting: null });
  }

  private showToast(toast: Omit<Toast, "key">): void {
    clearTimeout(this.toastTimer);
    this.toast.set({ ...toast, key: this.nextToastKey++ });
    if (!toast.isError) {
      this.toastTimer = setTimeout(() => this.clearToast(), TOAST_DURATION_MS);
    }
  }

  private clearToast(): void {
    clearTimeout(this.toastTimer);
    this.toast.set(null);
  }
}
