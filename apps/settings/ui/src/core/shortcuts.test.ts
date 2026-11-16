import { describe, expect, it } from "vitest";

import type { ShortcutId } from "./model";
import { shortcutKey } from "./shortcuts";

describe("shortcut keys", () => {
  it("a shortcut's key is the same each time it is made", () => {
    const switchTwo: ShortcutId = { kind: "workspace", workspace: "workspace-2", shortcut: "switchShortcut" };
    expect(shortcutKey(switchTwo)).toBe(shortcutKey({ ...switchTwo }));
    expect(shortcutKey({ kind: "navigation", shortcut: "left" })).toBe("navigation/left");
  });

  it("every shortcut has a key of its own, whatever its workspace or layout is named", () => {
    const ids: ShortcutId[] = [
      { kind: "navigation", shortcut: "left" },
      { kind: "history", step: "undo" },
      { kind: "workspaceStep", step: "previous" },
      { kind: "workspace", workspace: "workspace-1", shortcut: "switchShortcut" },
      { kind: "workspace", workspace: "workspace-2", shortcut: "switchShortcut" },
      { kind: "workspace", workspace: "workspace-2", shortcut: "sendShortcut" },
      { kind: "layoutSelect", layout: "halves" },
      { kind: "zone", layout: "halves", zone: "halves-0" },
      { kind: "zone", layout: "my-halves", zone: "halves-0" },
    ];
    expect(new Set(ids.map(shortcutKey)).size).toBe(ids.length);
  });

  it("IDs holding the separator can't make two shortcuts read alike", () => {
    const zone = (layout: string, zone: string): ShortcutId => ({ kind: "zone", layout, zone });
    expect(shortcutKey(zone("a/b", "c"))).not.toBe(shortcutKey(zone("a", "b/c")));
  });
});
