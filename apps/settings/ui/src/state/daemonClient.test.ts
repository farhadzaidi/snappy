import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Transport } from "../bridge/bridge";
import { initialState } from "../bridge/fake/fixtures";
import type { AppState, Command, Status, ToSettings } from "../core/model";
import { DaemonClient, TOAST_DURATION_MS } from "./daemonClient";

class ScriptedTransport implements Transport {
  readonly sent: { id: number; command: Command }[] = [];
  private listener: ((message: ToSettings) => void) | undefined;

  listen(onMessage: (message: ToSettings) => void) {
    this.listener = onMessage;
    return () => undefined;
  }

  send(id: number, command: Command) {
    this.sent.push({ id, command });
  }

  deliver(message: ToSettings) {
    this.listener?.(message);
  }
}

function stateWith(status: Status | null): ToSettings {
  const state: AppState = { ...initialState("macos"), status };
  return { type: "state", ...state };
}

const status = (id: number, isError: boolean, notify = true): Status => ({
  id,
  message: `message ${id}`,
  isError,
  notify,
});

describe("daemon client", () => {
  let transport: ScriptedTransport;
  let client: DaemonClient;

  beforeEach(() => {
    vi.useFakeTimers();
    transport = new ScriptedTransport();
    client = new DaemonClient(transport);
    client.connect();
  });

  afterEach(() => vi.useRealTimers());

  it("normal messages hide after the toast duration", () => {
    transport.deliver(stateWith(null));
    transport.deliver(stateWith(status(1, false)));
    expect(client.toast.get()?.message).toBe("message 1");
    vi.advanceTimersByTime(TOAST_DURATION_MS);
    expect(client.toast.get()).toBeNull();
  });

  it("errors stay until dismissed, and dismissing tells the daemon", () => {
    transport.deliver(stateWith(null));
    transport.deliver(stateWith(status(1, true)));
    vi.advanceTimersByTime(TOAST_DURATION_MS * 10);
    expect(client.toast.get()?.isError).toBe(true);
    client.dismissToast();
    expect(client.toast.get()).toBeNull();
    expect(transport.sent.at(-1)?.command).toEqual({ kind: "dismissStatus" });
  });

  it("quiet updates are never shown and dismiss a visible error", () => {
    transport.deliver(stateWith(null));
    transport.deliver(stateWith(status(1, true)));
    transport.deliver(stateWith(status(2, false, false)));
    expect(client.toast.get()).toBeNull();
  });

  it("an error raised while the window was closed shows when it opens", () => {
    transport.deliver(stateWith(status(5, true)));
    expect(client.toast.get()?.message).toBe("message 5");
    const fresh = new DaemonClient(transport);
    fresh.connect();
    transport.deliver(stateWith(status(6, false)));
    expect(fresh.toast.get()).toBeNull();
  });

  it("every command resolves with its reply, and errors show", async () => {
    const reply = client.send({ kind: "setDebugLog", enabled: true });
    const id = transport.sent.at(-1)?.id ?? 0;
    transport.deliver({ type: "reply", id, error: "Could not save settings: disk full" });
    await expect(reply).resolves.toBe("Could not save settings: disk full");
    expect(client.toast.get()).toMatchObject({
      isError: true,
      message: "Could not save settings: disk full",
    });
  });

  it("probes resolve with the daemon's answer", async () => {
    const answer = client.probe({ code: "KeyK", mods: ["ctrl", "alt"] });
    transport.deliver({ type: "probeResult", available: false });
    await expect(answer).resolves.toBe(false);
  });
});
