import { describe, expect, it } from "vitest";

import type { Command } from "../core/model";
import { Recording } from "./recording";

function recordingWithLog() {
  const sent: Command["kind"][] = [];
  const recording = new Recording({
    send: (command) => {
      sent.push(command.kind);
      return Promise.resolve(null);
    },
  });
  return { recording, sent };
}

describe("recording", () => {
  it("one recorder records at a time, and starting another moves recording to it", () => {
    const { recording, sent } = recordingWithLog();
    recording.start("a");
    recording.start("b");
    expect(recording.active.get()).toBe("b");
    expect(recording.isRecording("a")).toBe(false);
    expect(sent).toEqual(["beginRecording"]);
  });

  it("a recorder that no longer records can't end another's recording", () => {
    const { recording, sent } = recordingWithLog();
    recording.start("a");
    recording.start("b");
    recording.stop("a");
    expect(recording.isRecording("b")).toBe(true);
    expect(sent).toEqual(["beginRecording"]);
  });

  it("the daemon gets its shortcuts back once the recording recorder stops", () => {
    const { recording, sent } = recordingWithLog();
    recording.start("a");
    recording.start("a");
    recording.stop("a");
    recording.stop("a");
    expect(recording.active.get()).toBeNull();
    expect(sent).toEqual(["beginRecording", "endRecording"]);
  });
});
