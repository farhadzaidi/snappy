import { expect, test, type Locator, type Page } from "@playwright/test";

import {
  chordIn,
  lastCommandOf,
  openApp,
  sentCommands,
  showPage,
  spokenText,
  takeShortcut,
  toast,
} from "./support";

const recorder = (page: Page) => page.getByRole("button", { name: "Move window left", exact: true });

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await showPage(page, "Settings");
});

test("recording captures a combination between begin and end", async ({ page }) => {
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  await recorder(page).click();
  await expect(recorder(page)).toHaveText("Press shortcut…");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "beginRecording" });

  await page.keyboard.press("Control+Alt+Meta+KeyM");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + M");
  const commands = (await sentCommands(page)).map((command) => command.kind);
  expect(commands.slice(-4)).toEqual([
    "beginRecording",
    "probeShortcut",
    "setNavigationShortcut",
    "endRecording",
  ]);
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: { code: "KeyM", mods: ["snappy"] },
  });
});

test("holding the Snappy key's modifiers with Shift records the Snappy key plus Shift", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Shift+Meta+KeyJ");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + Shift ⇧ + J");
  await expect(chordIn(recorder(page)).locator(".snappy-key")).toHaveText("Snappy");
  await expect(chordIn(recorder(page)).locator(".chord-key")).toHaveText(["Shift ⇧", "J"]);
  await expect(chordIn(recorder(page)).locator(".snappy-key-glyph")).toBeVisible();
  await expect(recorder(page).locator(".reference")).toHaveCount(0);
  await expect(recorder(page)).toHaveAccessibleDescription("Snappy + Shift ⇧ + J");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: { code: "KeyJ", mods: ["snappy", "shift"] },
  });
});

test("escape cancels and keeps the shortcut", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Escape");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
  expect(await lastCommandOf(page, "setNavigationShortcut")).toBeUndefined();
});

test("delete without modifiers clears the shortcut", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Backspace");
  await expect(recorder(page)).toHaveText("Record shortcut");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: null,
  });
});

test("a combination without control, option, or command shows and says the hint", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Shift+KeyJ");
  await expect(recorder(page)).toHaveText("Press shortcut…");
  await expect(toast(page)).toHaveText("Include Control, Option, or Command.");
  await expect(toast(page)).toHaveRole("alert");
  await expect(recorder(page)).toHaveAccessibleDescription("Include Control, Option, or Command.");
  expect(await lastCommandOf(page, "probeShortcut")).toBeUndefined();
});

test("a conflicting shortcut is refused with the conflict message", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Meta+KeyQ");
  await expect
    .poll(() => spokenText(toast(page)))
    .toBe("Command ⌘ + Q is already assigned to a standard app command.");
  await page.keyboard.press("Escape");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  expect(await lastCommandOf(page, "probeShortcut")).toBeUndefined();
});

test("a chord another recorder owns is refused, naming that recorder", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Meta+ArrowRight");
  await expect
    .poll(() => spokenText(toast(page)))
    .toBe("Snappy + → is already assigned to Move window right.");
  const reference = toast(page).locator(".reference").first();
  await expect(chordIn(reference)).toHaveAttribute("aria-label", "Snappy + →");
  await expect(chordIn(reference).locator(".snappy-key")).toHaveText("Snappy");
  await expect(chordIn(reference).locator(".chord-keys")).toHaveText("→");
  await expect(chordIn(toast(page)).locator(".snappy-key-glyph")).toHaveAttribute("aria-hidden", "true");
});

test("a shortcut's keys stand out from the + between them, in a recorder and in a sentence", async ({
  page,
}) => {
  const expectKeysStandOut = async (chord: Locator) => {
    const style = await chord.evaluate((root) => {
      const of = (selector: string) => {
        const part = root.querySelector(selector);
        if (!part) {
          throw new Error(`The shortcut has no ${selector}`);
        }
        const { fontWeight, color } = getComputedStyle(part);
        return { weight: Number(fontWeight), color };
      };
      return { key: of(".chord-key"), plus: of(".chord-plus"), around: getComputedStyle(root).color };
    });
    expect(style.key.weight).toBeGreaterThan(style.plus.weight);
    expect(style.key.color).toBe(style.around);
    expect(style.plus.color).not.toBe(style.key.color);
  };

  await expectKeysStandOut(chordIn(recorder(page)));
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Meta+ArrowRight");
  await expectKeysStandOut(toast(page).locator(".reference .chord"));
});

test("a Snappy key named in a sentence sits evenly inside its reference", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Meta+ArrowRight");
  const reference = toast(page).locator(".reference").first();
  await expect(reference.locator(".snappy-key")).toBeVisible();
  const room = await reference.evaluate((element) => {
    const outer = element.getBoundingClientRect();
    const border = parseFloat(getComputedStyle(element).borderTopWidth);
    const box = (selector: string) => {
      const part = element.querySelector(selector);
      if (!part) {
        throw new Error(`The reference has no ${selector}`);
      }
      return part.getBoundingClientRect();
    };
    const [pill, plus, keys] = [box(".snappy-key"), box(".chord-plus"), box(".chord-keys")];
    return {
      top: pill.top - outer.top - border,
      bottom: outer.bottom - border - pill.bottom,
      left: pill.left - outer.left - border,
      right: outer.right - border - keys.right,
      afterPill: plus.left - pill.right,
    };
  });
  for (const inset of [room.top, room.bottom, room.left]) {
    expect(inset).toBeGreaterThan(2);
    expect(Math.abs(inset - room.top)).toBeLessThan(0.3);
  }
  // The keys end as far from the reference's edge as they sit from the pill, and not much farther
  // than the pill sits from the other edge.
  expect(Math.abs(room.right - room.afterPill)).toBeLessThan(0.3);
  expect(room.right - room.left).toBeGreaterThanOrEqual(0);
  expect(room.right - room.left).toBeLessThan(1.5);
});

test("the shortcut a refusal names is a link that goes to it", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Meta+ArrowRight");
  const link = toast(page).getByRole("button", { name: "Go to Move window right" });
  await expect(link).toHaveText("Move window right");
  await link.click();
  const right = page.getByRole("button", { name: "Move window right", exact: true });
  await expect(right).toBeFocused();
  await expect(right).toBeInViewport();
});

test("a refusal names a shortcut on another page without a link", async ({ page }) => {
  await page.getByRole("button", { name: "Previous workspace", exact: true }).click();
  await page.keyboard.press("Control+Alt+Meta+Digit1");
  await expect(toast(page)).toContainText("is already assigned to Switch to Workspace 1.");
  await expect(toast(page).locator(".reference").last()).toHaveText("Switch to Workspace 1");
  await expect(toast(page).getByRole("button", { name: "Go to Switch to Workspace 1" })).toHaveCount(0);
});

test("escape ends recording while the probe has yet to answer, and its answer changes nothing", async ({
  page,
}) => {
  await page.evaluate(() => window.snappyFake.holdProbeAnswers());
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Meta+KeyM");
  await expect.poll(() => lastCommandOf(page, "probeShortcut")).toBeDefined();
  await page.keyboard.press("Escape");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });

  await recorder(page).click();
  await page.evaluate(() => window.snappyFake.answerProbes());
  await page.keyboard.press("Control+Alt+Meta+KeyN");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + N");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: { code: "KeyN", mods: ["snappy"] },
  });
});

test("a shortcut another app uses is refused after the probe", async ({ page }) => {
  await takeShortcut(page, { code: "KeyK", mods: ["ctrl", "alt"] });
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+KeyK");
  await expect
    .poll(() => spokenText(toast(page)))
    .toContain("Control ⌃ + Option ⌥ + K for Move window left is already used by macOS or another app.");
  await expect(toast(page).getByRole("button", { name: "Go to Move window left" })).toBeVisible();
  await expect(recorder(page)).toHaveText("Press shortcut…");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toBeUndefined();
});

test("the clear button removes the shortcut", async ({ page }) => {
  await page.getByRole("button", { name: "Clear Move window left" }).click();
  await expect(recorder(page)).toHaveText("Record shortcut");
  await expect(page.getByRole("button", { name: "Clear Move window left" })).toHaveCount(0);
});

test("leaving the recorder ends recording", async ({ page }) => {
  await recorder(page).click();
  await page.getByRole("heading", { name: "Global settings" }).click();
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
});

test("only one recorder records at a time", async ({ page }) => {
  const right = page.getByRole("button", { name: "Move window right", exact: true });
  await recorder(page).click();
  await right.click();
  await expect(right).toHaveText("Press shortcut…");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  await expect(page.getByRole("button", { name: "Cancel recording" })).toHaveCount(1);

  await page.keyboard.press("Control+Alt+Meta+KeyM");
  await expect(chordIn(right)).toHaveAttribute("aria-label", "Snappy + M");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  const kinds = (await sentCommands(page)).map((command) => command.kind);
  expect(kinds.filter((kind) => kind === "beginRecording")).toHaveLength(
    kinds.filter((kind) => kind === "endRecording").length,
  );
});

test("cancel ends recording and keeps the shortcut, in place of the clear button", async ({ page }) => {
  const clear = page.getByRole("button", { name: "Clear Move window left" });
  await expect(clear).toBeVisible();
  await recorder(page).click();
  await expect(clear).toHaveCount(0);
  const cancel = page.getByRole("button", { name: "Cancel recording" });
  await expect(cancel).toHaveText("esc");
  await cancel.click();
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  await expect(cancel).toHaveCount(0);
  await expect(clear).toBeVisible();
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
  expect(await lastCommandOf(page, "setNavigationShortcut")).toBeUndefined();
});

/** Clicking a button in WebKit doesn't focus it; start recording the same way. */
async function startWithoutFocus(page: Page): Promise<void> {
  await page.evaluate(() => {
    HTMLElement.prototype.focus = () => undefined;
  });
  await recorder(page).dispatchEvent("click");
  await expect(recorder(page)).toHaveText("Press shortcut…");
  await expect(recorder(page)).not.toBeFocused();
}

test("recording works when a click doesn't focus the recorder, as in WebKit", async ({ page }) => {
  await startWithoutFocus(page);
  await page.keyboard.press("Control+Alt+Meta+KeyM");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + M");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: { code: "KeyM", mods: ["snappy"] },
  });
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
});

test("escape cancels when focus isn't on the recorder", async ({ page }) => {
  await startWithoutFocus(page);
  await page.keyboard.press("Escape");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
});

test("clicking a recorder puts focus on it", async ({ page }) => {
  await recorder(page).click();
  await expect(recorder(page)).toBeFocused();
});

test("recorders use platform names on Windows", async ({ page }) => {
  await openApp(page, "windows");
  await showPage(page, "Settings");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Snappy + ←");
  await recorder(page).click();
  await page.keyboard.press("Control+Meta+KeyJ");
  await expect(chordIn(recorder(page))).toHaveAttribute("aria-label", "Ctrl + Win + J");
  await expect(chordIn(recorder(page)).locator(".chord-key")).toHaveText(["Ctrl", "Win", "J"]);
});
