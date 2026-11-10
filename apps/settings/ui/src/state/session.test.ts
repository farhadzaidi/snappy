import { describe, expect, it } from "vitest";

import { FAKE_DISPLAYS } from "../bridge/fake/fixtures";
import { displayWithFrame } from "./session";

describe("session", () => {
  it("the chooser starts on the display whose frame holds the window", () => {
    const studio = FAKE_DISPLAYS[1];
    expect(displayWithFrame(FAKE_DISPLAYS, { x: 1512.4, y: 0, width: 2560, height: 1440 })).toBe(studio);
    expect(displayWithFrame(FAKE_DISPLAYS, { x: 9000, y: 0, width: 10, height: 10 })).toBeUndefined();
    expect(displayWithFrame(FAKE_DISPLAYS, null)).toBeUndefined();
  });
});
