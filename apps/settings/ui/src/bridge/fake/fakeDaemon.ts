// An in-memory daemon for `pnpm dev` and the tests. It speaks the same messages as the real
// one, applies commands with the real `snappy-core` rules, and records every command.

import type { AppState, Command, KeyChord, Modifier, PlatformName, ToSettings } from "../../core/model";
import { core } from "../../core/snappyCore";
import type { Bridge, Transport } from "../bridge";
import { EXPORT_FILE_NAME } from "../bridge";
import { FAKE_DISPLAYS, initialState } from "./fixtures";
import { applyCommand } from "./reducer";

const PROTOCOL_VERSION = 6;

/**
 * The same key for every spelling of one shortcut, such as Snappy + K and its modifiers held
 * by hand: the modifiers to hold with the Snappy key standing for `snappyKey`.
 */
function comboKey(chord: KeyChord, snappyKey: Modifier[]): string {
  const expanded = core.expandChord(chord, snappyKey);
  return `${[...expanded.mods].sort().join("+")}+${expanded.code}`;
}

export class FakeDaemon implements Transport {
  readonly commands: Command[] = [];
  private state: AppState;
  private readonly listeners = new Set<(message: ToSettings) => void>();
  private readonly takenByOtherApps = new Set<string>();
  private capsLockRefusal: string | undefined;
  /** Probe answers kept back, while `holdProbeAnswers` holds them. */
  private heldProbeAnswers: ToSettings[] | null = null;

  constructor(platform: PlatformName) {
    this.state = initialState(platform);
  }

  listen(onMessage: (message: ToSettings) => void): () => void {
    this.listeners.add(onMessage);
    this.deliver(onMessage, [{ type: "hello", protocolVersion: PROTOCOL_VERSION }, this.stateMessage()]);
    return () => this.listeners.delete(onMessage);
  }

  send(id: number, command: Command): void {
    this.commands.push(command);
    const snappyKey = this.state.settings.snappyKey;
    const outcome = applyCommand(this.state, command, {
      isAvailable: (chord) => !this.takenByOtherApps.has(comboKey(chord, snappyKey)),
      capsLockRefusal: this.capsLockRefusal,
    });
    this.state = outcome.state;

    const error =
      outcome.error === null ? null : { message: outcome.error, shortcut: outcome.shortcut ?? null };
    const messages: ToSettings[] = [{ type: "reply", id, error }];
    if (outcome.probe !== undefined) {
      const answer: ToSettings = { type: "probeResult", available: outcome.probe };
      (this.heldProbeAnswers ?? messages).push(answer);
    }
    messages.push(this.stateMessage());
    this.listeners.forEach((listener) => this.deliver(listener, messages));
  }

  /**
   * Make the availability probe refuse `chord`, as if another app had registered it. A
   * Snappy key chord is taken with the modifiers the Snappy key stands for now.
   */
  takeShortcut(chord: KeyChord): void {
    this.takenByOtherApps.add(comboKey(chord, this.state.settings.snappyKey));
  }

  /**
   * Refuse Caps Lock as the Snappy key with `reason`, as when the system already remaps it,
   * or stop refusing with `undefined`.
   */
  refuseCapsLock(reason: string | undefined): void {
    this.capsLockRefusal = reason;
  }

  /** Keep availability answers back until `answerProbes`, as a daemon slow to ask the OS would. */
  holdProbeAnswers(): void {
    this.heldProbeAnswers ??= [];
  }

  /** Send the availability answers kept back, and answer at once again. */
  answerProbes(): void {
    const answers = this.heldProbeAnswers ?? [];
    this.heldProbeAnswers = null;
    this.listeners.forEach((listener) => this.deliver(listener, answers));
  }

  /** Replace part of the state and broadcast it, as the daemon does after a change. */
  patchState(change: (state: AppState) => AppState): void {
    this.state = change(this.state);
    this.listeners.forEach((listener) => this.deliver(listener, [this.stateMessage()]));
  }

  private stateMessage(): ToSettings {
    return { type: "state", ...this.state };
  }

  /** Messages arrive asynchronously, as they do over the pipe. */
  private deliver(listener: (message: ToSettings) => void, messages: ToSettings[]): void {
    setTimeout(() => messages.forEach(listener), 0);
  }
}

export function fakeBridge(daemon: FakeDaemon): Bridge {
  return {
    transport: daemon,
    files: {
      chooseImportPath: () => Promise.resolve(`/Users/snappy/${EXPORT_FILE_NAME}`),
      chooseExportPath: () => Promise.resolve(`/Users/snappy/${EXPORT_FILE_NAME}`),
    },
    windowDisplayFrame: () => Promise.resolve(FAKE_DISPLAYS[0]?.frame ?? null),
  };
}
