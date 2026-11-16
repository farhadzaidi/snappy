// Putting shortcuts and settings back to their defaults: each panel's Reset, the Settings
// page's Reset group, and clearing a layout's shortcuts in the editor.

import { expect, test, type Page } from "@playwright/test";

import {
  chordIn,
  lastCommandOf,
  layoutAction,
  layoutCard,
  openApp,
  saveCustomCopy,
  sentCommands,
  showPage,
  toast,
  workspaceCard,
} from "./support";

const group = (page: Page, title: string) => page.getByRole("region", { name: title, exact: true });

/** The Reset button in the title of the panel named `title`. */
const resetButton = (page: Page, title: string) =>
  page.getByRole("button", { name: `Reset ${title} shortcuts`, exact: true });

test.beforeEach(async ({ page }) => openApp(page));

test("a section's Reset is off while its shortcuts are the defaults", async ({ page }) => {
  await showPage(page, "Settings");
  for (const title of ["Move between zones", "Focus and selection", "Workspaces", "Window history"]) {
    await expect(resetButton(page, title)).toBeDisabled();
  }
  await expect(page.getByRole("button", { name: "Reset the Snappy key" })).toBeDisabled();
});

test("a section resets its shortcuts after asking, and only its own", async ({ page }) => {
  await showPage(page, "Settings");
  const left = page.getByRole("button", { name: "Move window left", exact: true });
  const focusLeft = page.getByRole("button", { name: "Focus left", exact: true });
  for (const recorder of [left, focusLeft]) {
    await recorder.click();
    await page.keyboard.press(recorder === left ? "Control+Alt+Meta+KeyM" : "Control+Alt+Meta+KeyH");
  }
  await expect(chordIn(left)).toHaveAttribute("aria-label", "Snappy + M");

  const reset = resetButton(page, "Move between zones");
  await reset.click();
  const ask = page.getByRole("dialog", { name: "Reset Move between zones shortcuts" });
  await expect(ask).toContainText("Reset Move between zones shortcuts to their defaults?");
  await expect(ask.locator(".reference")).toHaveText("Move between zones");
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(chordIn(left)).toHaveAttribute("aria-label", "Snappy + M");
  expect(await lastCommandOf(page, "reset")).toBeUndefined();

  await reset.click();
  await page
    .getByRole("dialog", { name: "Reset Move between zones shortcuts" })
    .getByRole("button", { name: "Reset" })
    .click();
  expect(await lastCommandOf(page, "reset")).toEqual({
    kind: "reset",
    reset: { scope: "shortcuts", section: "moveBetweenZones" },
  });
  await expect(chordIn(left)).toHaveAttribute("aria-label", "Snappy + ←");
  await expect(chordIn(focusLeft)).toHaveAttribute("aria-label", "Snappy + H");
  await expect(toast(page)).toContainText("Move between zones shortcuts reset.");
  await expect(reset).toBeDisabled();
});

test("closing a reset's question gives focus back to its Reset", async ({ page }) => {
  await showPage(page, "Settings");
  const undo = group(page, "Window history").getByRole("button", { name: "Undo window move", exact: true });
  await undo.click();
  await page.keyboard.press("Control+Alt+Meta+KeyZ");
  const reset = resetButton(page, "Window history");
  const ask = page.getByRole("dialog", { name: "Reset Window history shortcuts" });

  await reset.click();
  await expect(ask.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(ask).toHaveCount(0);
  await expect(reset).toBeFocused();

  await reset.click();
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(reset).toBeFocused();
});

test("Window history's reset clears what was recorded, since neither has a default", async ({ page }) => {
  await showPage(page, "Settings");
  const undo = group(page, "Window history").getByRole("button", { name: "Undo window move", exact: true });
  await undo.click();
  await page.keyboard.press("Control+Alt+Meta+KeyZ");
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Snappy + Z");
  await resetButton(page, "Window history").click();
  await page.getByRole("dialog").getByRole("button", { name: "Reset" }).click();
  await expect(undo).toHaveText("Record shortcut");
});

test("a workspace's shortcuts reset to switching by its number", async ({ page }) => {
  await showPage(page, "Workspaces");
  await workspaceCard(page, "Workspace 2").click();
  const send = page.getByRole("button", { name: "Send window to this workspace", exact: true });
  await send.click();
  await page.keyboard.press("Control+Alt+Meta+KeyY");
  await expect(chordIn(send)).toHaveAttribute("aria-label", "Snappy + Y");

  await resetButton(page, "Workspace 2").click();
  await page.getByRole("dialog").getByRole("button", { name: "Reset" }).click();
  expect(await lastCommandOf(page, "reset")).toEqual({
    kind: "reset",
    reset: { scope: "workspaceShortcuts", workspace: "workspace-2" },
  });
  await expect(send).toHaveText("Record shortcut");
  await expect(
    chordIn(page.getByRole("button", { name: "Switch to this workspace", exact: true })),
  ).toHaveAttribute("aria-label", "Snappy + 2");
  await expect(toast(page)).toContainText("Shortcuts for Workspace 2 reset.");
});

test("resetting settings asks first and keeps layouts and workspaces", async ({ page }) => {
  await saveCustomCopy(page, "Halves");
  await showPage(page, "Settings");
  const debugLog = page.getByRole("switch", { name: "Debug log" });
  await debugLog.click();
  await expect(debugLog).toHaveAttribute("aria-checked", "true");

  const resets = group(page, "Reset");
  await resets.getByRole("button", { name: "Reset", exact: true }).click();
  const dialog = page.getByRole("alertdialog", { name: "Reset settings and shortcuts?" });
  await expect(dialog).toContainText("Your layouts and workspaces stay.");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  expect(await lastCommandOf(page, "reset")).toBeUndefined();

  await resets.getByRole("button", { name: "Reset", exact: true }).click();
  await page
    .getByRole("alertdialog", { name: "Reset settings and shortcuts?" })
    .getByRole("button", { name: "Reset" })
    .click();
  expect(await lastCommandOf(page, "reset")).toEqual({ kind: "reset", reset: { scope: "settings" } });
  await expect(debugLog).toHaveAttribute("aria-checked", "false");
  await expect(toast(page)).toContainText("Settings and shortcuts reset.");

  await showPage(page, "Layouts");
  await page.getByRole("tab", { name: "My layouts" }).click();
  await expect(layoutCard(page, "Halves copy")).toBeVisible();
});

test("erasing everything names what goes, and removes custom layouts", async ({ page }) => {
  await saveCustomCopy(page, "Halves");
  await showPage(page, "Settings");
  await group(page, "Reset").getByRole("button", { name: "Reset everything", exact: true }).click();
  const dialog = page.getByRole("alertdialog", { name: "Reset everything?" });
  await expect(dialog).toContainText("This removes your 1 custom layout and 3 workspaces");
  await dialog.getByRole("button", { name: "Reset everything" }).click();
  expect(await lastCommandOf(page, "reset")).toEqual({ kind: "reset", reset: { scope: "everything" } });
  await expect(toast(page)).toContainText("Snappy is reset.");
  await expect(
    group(page, "Reset").getByRole("button", { name: "Reset everything", exact: true }),
  ).toBeDisabled();

  await showPage(page, "Layouts");
  await page.getByRole("tab", { name: "My layouts" }).click();
  await expect(page.getByText("No custom layouts yet")).toBeVisible();
});

test("Clear all in the editor clears the layout's shortcut and its zones', as one step", async ({ page }) => {
  await layoutCard(page, "Halves").click();
  await layoutAction(page, "Duplicate");
  await page.getByRole("tab", { name: "Shortcuts" }).click();
  const select = page.getByRole("button", { name: "Select this layout", exact: true });
  const zone = page.getByRole("button", { name: "Zone 1 of this layout", exact: true });
  const clear = page.getByRole("button", { name: "Clear all" });
  await expect(clear).toBeDisabled();
  for (const [recorder, keys] of [
    [select, "Control+Alt+KeyY"],
    [zone, "Control+Alt+KeyU"],
  ] as const) {
    await recorder.click();
    await page.keyboard.press(keys);
  }
  await expect(chordIn(select)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Y");

  await clear.click();
  await expect(select).toHaveText("Record shortcut");
  await expect(zone).toHaveText("Record shortcut");
  await expect(clear).toBeDisabled();
  expect((await sentCommands(page)).some((command) => command.kind === "reset")).toBe(false);

  await page.keyboard.press("Meta+KeyZ");
  await expect(chordIn(select)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Y");
  await expect(chordIn(zone)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + U");
});
