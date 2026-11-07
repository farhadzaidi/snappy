import { expect, test, type Page } from "@playwright/test";

import { lastCommandOf, openApp, sentCommands, showPage, takeShortcut, toast } from "./support";

const recorder = (page: Page) => page.getByRole("button", { name: "Move window left", exact: true });

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await showPage(page, "Settings");
});

test("recording captures a combination between begin and end", async ({ page }) => {
  await expect(recorder(page)).toHaveText("Snappy + ←");
  await recorder(page).click();
  await expect(recorder(page)).toHaveText("Press shortcut…");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "beginRecording" });

  await page.keyboard.press("Control+Alt+Meta+KeyJ");
  await expect(recorder(page)).toHaveText("Snappy + J");
  const commands = (await sentCommands(page)).map((command) => command.kind);
  expect(commands.slice(-4)).toEqual([
    "beginRecording",
    "probeShortcut",
    "setNavigationShortcut",
    "endRecording",
  ]);
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    direction: "left",
    chord: { code: "KeyJ", mods: ["snappy"] },
  });
});

test("holding the Snappy key's modifiers with Shift records the Snappy key plus Shift", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Shift+Meta+KeyJ");
  await expect(recorder(page)).toHaveText("Snappy + Shift + J");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    direction: "left",
    chord: { code: "KeyJ", mods: ["snappy", "shift"] },
  });
});

test("escape cancels and keeps the shortcut", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Escape");
  await expect(recorder(page)).toHaveText("Snappy + ←");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
  expect(await lastCommandOf(page, "setNavigationShortcut")).toBeUndefined();
});

test("delete without modifiers clears the shortcut", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Backspace");
  await expect(recorder(page)).toHaveText("Record shortcut");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    direction: "left",
    chord: null,
  });
});

test("a combination without control, option, or command shows the hint", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Shift+KeyJ");
  await expect(recorder(page)).toHaveText("Press shortcut…");
  await expect(recorder(page)).toHaveAttribute("title", "Include Control, Option, or Command.");
});

test("a conflicting shortcut is refused with the conflict message", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Meta+KeyQ");
  await expect(toast(page)).toContainText("Command + Q is already assigned to a standard app command.");
  await page.keyboard.press("Escape");
  await expect(recorder(page)).toHaveText("Snappy + ←");
  expect(await lastCommandOf(page, "probeShortcut")).toBeUndefined();
});

test("a chord another recorder owns is refused", async ({ page }) => {
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+Meta+ArrowRight");
  await expect(toast(page)).toContainText("Snappy + → is already assigned to Move window");
});

test("a shortcut another app uses is refused after the probe", async ({ page }) => {
  await takeShortcut(page, { code: "KeyK", mods: ["ctrl", "alt"] });
  await recorder(page).click();
  await page.keyboard.press("Control+Alt+KeyK");
  await expect(toast(page)).toContainText(
    "Control + Option + K is already used by macOS or another app. Choose another shortcut.",
  );
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
  await expect(recorder(page)).toHaveText("Snappy + ←");
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "endRecording" });
});

test("recorders use platform names on Windows", async ({ page }) => {
  await openApp(page, "windows");
  await showPage(page, "Settings");
  await expect(recorder(page)).toHaveText("Snappy + ←");
  await recorder(page).click();
  await page.keyboard.press("Control+Meta+KeyJ");
  await expect(recorder(page)).toHaveText("Ctrl + Win + J");
});
