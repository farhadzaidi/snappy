import { expect, test, type Page } from "@playwright/test";

import { lastCommandOf, openApp, sentCommands, showPage, takeShortcut, toast } from "./support";

const group = (page: Page, title: string) => page.getByRole("region", { name: title, exact: true });

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await showPage(page, "Settings");
});

test("the groups appear in order, each with its line under the title", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "Global settings" })).toBeVisible();
  const titles = await page.locator(".group-title").allTextContents();
  expect(titles).toEqual([
    "Snapping",
    "Snappy key",
    "Move between zones",
    "Focus and selection",
    "Workspaces",
    "Window history",
    "Startup & access",
    "Backup & restore",
    "Troubleshooting",
  ]);
  for (const title of titles) {
    await expect(group(page, title).locator(".group-header .group-caption")).toHaveCount(1);
  }
  await expect(page.getByText("Excluded apps")).toHaveCount(0);
  await expect(page.getByText("Move to next display")).toHaveCount(0);
});

test("the drag modifier changes, and the key for the opposite never offers it", async ({ page }) => {
  await expect(group(page, "Snapping").locator(".group-caption")).toHaveText(
    "Hold Shift ⇧ while dragging a window. Add Command ⌘ to stack on top instead of swapping.",
  );
  await page.getByRole("button", { name: "Hold to snap" }).click();
  await page.getByRole("option", { name: "Option ⌥" }).click();
  expect(await lastCommandOf(page, "setDragModifier")).toEqual({ kind: "setDragModifier", modifier: "alt" });
  await expect(page.getByRole("button", { name: "Hold to snap" })).toHaveText("Option ⌥");

  await page.getByRole("button", { name: "Hold to do the opposite" }).click();
  const options = await page
    .getByRole("listbox", { name: "Hold to do the opposite" })
    .getByRole("option")
    .allTextContents();
  expect(options).toEqual(["Command ⌘", "Control ⌃", "Shift ⇧", "Off"]);
  await page.getByRole("option", { name: "Off" }).click();
  await expect(group(page, "Snapping").locator(".group-caption")).toHaveText(
    "Hold Option ⌥ while dragging a window.",
  );
});

test("when the zone is taken swaps by default and can stack on top", async ({ page }) => {
  const choice = page.getByRole("button", { name: "When the zone is taken" });
  await expect(choice).toHaveText("Swap");
  await choice.click();
  const options = await page
    .getByRole("listbox", { name: "When the zone is taken" })
    .getByRole("option")
    .allTextContents();
  expect(options).toEqual(["Swap", "Stack on top"]);
  await page.getByRole("option", { name: "Stack on top" }).click();
  expect(await lastCommandOf(page, "setWhenTaken")).toEqual({ kind: "setWhenTaken", choice: "stack" });
  await expect(choice).toHaveText("Stack on top");
  await expect(group(page, "Snapping").locator(".group-caption")).toHaveText(
    "Hold Shift ⇧ while dragging a window. Add Command ⌘ to swap instead of stacking on top.",
  );
});

test("the keyboard shortcuts switch turns every shortcut off", async ({ page }) => {
  await page.getByRole("switch", { name: "Keyboard shortcuts" }).click();
  expect(await lastCommandOf(page, "setShortcutsEnabled")).toEqual({
    kind: "setShortcutsEnabled",
    enabled: false,
  });
  await expect(page.getByRole("switch", { name: "Keyboard shortcuts" })).toHaveAttribute(
    "aria-checked",
    "false",
  );
});

test("move between zones has a recorder per direction and one for the nearest empty zone", async ({
  page,
}) => {
  const zones = group(page, "Move between zones");
  await expect(zones.locator(".group-caption")).toHaveText(
    "Move the focused window to the next zone in that direction, or into the nearest empty zone.",
  );
  await expect(zones.locator(".recorder-button")).toHaveText([
    "Snappy + ←",
    "Snappy + →",
    "Snappy + ↑",
    "Snappy + ↓",
    "Snappy + Space",
  ]);
  await expect(zones.getByText("Move to the nearest empty zone", { exact: true })).toBeVisible();
});

test("the nearest-empty-zone shortcut is recorded like the others", async ({ page }) => {
  const recorder = group(page, "Move between zones").getByRole("button", {
    name: "Move to the nearest empty zone",
    exact: true,
  });
  await recorder.click();
  await expect(recorder).toHaveText("Press shortcut…");
  await page.keyboard.press("Control+Alt+Meta+KeyE");
  await expect(recorder).toHaveText("Snappy + E");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "nearestEmpty",
    chord: { code: "KeyE", mods: ["snappy"] },
  });

  await recorder.click();
  await page.keyboard.press("Control+Alt+Meta+ArrowLeft");
  await expect(toast(page)).toContainText("Snappy + ← is already assigned to Move window left.");
  await page.keyboard.press("Escape");
  await expect(recorder).toHaveText("Snappy + E");
});

test("the nearest-empty-zone default is Snappy + Space on Windows and Linux too", async ({ page }) => {
  for (const platform of ["windows", "linux"] as const) {
    await openApp(page, platform);
    await showPage(page, "Settings");
    const recorder = group(page, "Move between zones").getByRole("button", {
      name: "Move to the nearest empty zone",
      exact: true,
    });
    await expect(recorder).toHaveText("Snappy + Space");
  }
});

test("the workspaces group shows the step and send shortcuts", async ({ page }) => {
  const workspaces = group(page, "Workspaces");
  const recorder = (name: string) => workspaces.getByRole("button", { name, exact: true });
  await expect(workspaces.locator(".recorder-button")).toHaveCount(4);
  await expect(recorder("Previous workspace")).toHaveText("Snappy + ;");
  await expect(recorder("Next workspace")).toHaveText("Snappy + '");
  await expect(recorder("Send window to previous workspace")).toHaveText("Snappy + .");
  await expect(recorder("Send window to next workspace")).toHaveText("Snappy + /");
  await expect(workspaces.getByText("Take along")).toHaveCount(0);
});

test("the focus and selection group shows its shortcuts", async ({ page }) => {
  const focus = group(page, "Focus and selection");
  await expect(focus).toContainText(
    "Move focus to the next window in a direction, from zone to zone when windows are in zones.",
  );
  await expect(focus.locator(".recorder-button")).toHaveText([
    "Snappy + I",
    "Snappy + J",
    "Snappy + K",
    "Snappy + L",
    "Snappy + ↩",
    "Snappy + \\",
  ]);
  for (const name of [
    "Focus up",
    "Focus left",
    "Focus down",
    "Focus right",
    "Select window",
    "Clear selection",
  ]) {
    await expect(focus.getByRole("button", { name, exact: true })).toBeVisible();
  }
});

test("the select shortcut can be changed and refuses a chord in use", async ({ page }) => {
  const select = group(page, "Focus and selection").getByRole("button", {
    name: "Select window",
    exact: true,
  });
  await select.click();
  await page.keyboard.press("Control+Alt+Meta+KeyT");
  await expect(select).toHaveText("Snappy + T");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "selectWindow",
    chord: { code: "KeyT", mods: ["snappy"] },
  });

  await select.click();
  await page.keyboard.press("Control+Alt+Meta+Space");
  await expect(toast(page)).toContainText(
    "Snappy + Space is already assigned to Move to the nearest empty zone.",
  );
  await page.keyboard.press("Escape");
  await expect(select).toHaveText("Snappy + T");
});

test("clearing the selection refuses the Snappy key with Delete on macOS", async ({ page }) => {
  const clear = group(page, "Focus and selection").getByRole("button", {
    name: "Clear selection",
    exact: true,
  });
  await clear.click();
  await page.keyboard.press("Control+Alt+Meta+Backspace");
  await expect(toast(page)).toContainText(
    "macOS reserves Control + Option + Command + Delete; it never reaches apps.",
  );
  await page.keyboard.press("Escape");
  await expect(clear).toHaveText("Snappy + \\");
  expect(await lastCommandOf(page, "probeShortcut")).toBeUndefined();
  expect(await lastCommandOf(page, "setNavigationShortcut")).toBeUndefined();
});

test("clearing the selection can use the Snappy key with Backspace on Windows", async ({ page }) => {
  await openApp(page, "windows");
  await showPage(page, "Settings");
  const clear = group(page, "Focus and selection").getByRole("button", {
    name: "Clear selection",
    exact: true,
  });
  await clear.click();
  await page.keyboard.press("Control+Alt+Meta+Backspace");
  await expect(clear).toHaveText("Snappy + ⌫");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "clearSelection",
    chord: { code: "Backspace", mods: ["snappy"] },
  });
});

test("a focus shortcut can be cleared", async ({ page }) => {
  const right = group(page, "Focus and selection").getByRole("button", { name: "Focus right", exact: true });
  await right.click();
  await page.keyboard.press("Backspace");
  await expect(right).toHaveText("Record shortcut");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "focusRight",
    chord: null,
  });
});

test("the workspace, focus, and selection defaults use the Snappy key on Windows and Linux too", async ({
  page,
}) => {
  for (const platform of ["windows", "linux"] as const) {
    await openApp(page, platform);
    await showPage(page, "Settings");
    await expect(group(page, "Workspaces").locator(".recorder-button")).toHaveText([
      "Snappy + ;",
      "Snappy + '",
      "Snappy + .",
      "Snappy + /",
    ]);
    await expect(group(page, "Focus and selection").locator(".recorder-button")).toHaveText([
      "Snappy + I",
      "Snappy + J",
      "Snappy + K",
      "Snappy + L",
      "Snappy + ↩",
      "Snappy + \\",
    ]);
  }
});

test("window history has undo and redo recorders with no default", async ({ page }) => {
  const history = group(page, "Window history");
  await expect(history.locator(".recorder-button")).toHaveText(["Record shortcut", "Record shortcut"]);
  await expect(history.getByRole("button", { name: "Undo window move", exact: true })).toBeVisible();
  await expect(history.getByRole("button", { name: "Redo window move", exact: true })).toBeVisible();
});

test("startup and access show the login item and accessibility", async ({ page }) => {
  await page.getByRole("switch", { name: "Open at login" }).click();
  expect(await lastCommandOf(page, "setLoginItem")).toEqual({ kind: "setLoginItem", enabled: true });
  await expect(group(page, "Startup & access").getByText("Accessibility allowed")).toBeVisible();
  await expect(page.getByRole("button", { name: "Check access" })).toHaveCount(0);
  await page.getByRole("button", { name: "Snapping on" }).click();
  await page.getByRole("button", { name: "Check access" }).click();
  expect(await lastCommandOf(page, "setSnapping")).toEqual({ kind: "setSnapping", enabled: true });
  await expect(page.getByRole("button", { name: "Check access" })).toHaveCount(0);
});

test("the debug log is turned on in Settings and shows where it's written", async ({ page }) => {
  const logToggle = page.getByRole("switch", { name: "Debug log" });
  await expect(logToggle).not.toBeChecked();
  await expect(page.getByText("debug.jsonl")).toHaveCount(0);
  await logToggle.click();
  expect(await lastCommandOf(page, "setDebugLog")).toEqual({ kind: "setDebugLog", enabled: true });
  await expect(logToggle).toBeChecked();
  await expect(group(page, "Troubleshooting").getByText("~/Library/Logs/Snappy/debug.jsonl")).toBeVisible();
});

test("platforms without a permission step hide the access controls", async ({ page }) => {
  await openApp(page, "linux");
  await showPage(page, "Settings");
  await expect(page.getByText("Accessibility allowed")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Check access" })).toHaveCount(0);
});

test("export and import go through the file picker", async ({ page }) => {
  await page.getByRole("button", { name: "Export settings…" }).click();
  await expect(toast(page)).toContainText("Layouts and preferences exported.");
  expect(await lastCommandOf(page, "exportSettings")).toEqual({
    kind: "exportSettings",
    path: "/Users/snappy/Snappy-layouts.json",
  });
  await page.getByRole("button", { name: "Import settings…" }).click();
  await expect(toast(page)).toContainText("Layouts and preferences imported.");
});

test("the snapping button follows live control and toggles it", async ({ page }) => {
  const button = page.locator(".snapping-button");
  await expect(button).toHaveText("Snapping on");
  await button.click();
  await expect(button).toHaveText("Enable snapping");
  await button.click();
  await expect(button).toHaveText("Snapping on");
  await expect(toast(page)).toContainText("Snapping is on. Hold Shift ⇧ while dragging a window.");
  expect((await sentCommands(page)).filter((command) => command.kind === "setSnapping")).toEqual([
    { kind: "setSnapping", enabled: false },
    { kind: "setSnapping", enabled: true },
  ]);
});

test("the help popover explains snapping", async ({ page }) => {
  await page.getByRole("button", { name: "Shortcuts and help" }).click();
  const help = page.getByRole("dialog", { name: "Shortcuts and help" });
  await expect(help).toContainText("Hold Shift ⇧, drag a window, then release over a zone.");
  await expect(help).toContainText("A window already in the zone trades places with it.");
  await expect(help).toContainText("Stack on top while dragging");
  await expect(help).toContainText("Shift ⇧ + Command ⌘");
  await expect(help).toContainText("Snappy key");
  await expect(help).toContainText("Control + Option + Command");
  await expect(help).toContainText("Switch to workspace 1–9, taking the selection alongSnappy + 1–9");
  await expect(help).toContainText(
    "Send the window or selection to the previous or next workspaceSnappy + . /",
  );
  await expect(help).toContainText("Move to the neighboring zoneSnappy + ← → ↑ ↓");
  await expect(help).toContainText("Focus the window up, left, down, or rightSnappy + I J K L");
  await expect(help).toContainText("Select or unselect the windowSnappy + Return");
  await expect(help).toContainText("Clear the selectionSnappy + \\");
  await expect(help).toContainText(
    "Move to the nearest empty zone, or the selection into empty zonesSnappy + Space",
  );
  await expect(help).not.toContainText("Take window along");
  await expect(help).not.toContainText("Default zone shortcuts");
  await expect(help).not.toContainText("Swap with the keyboard");
  await expect(help).not.toContainText("Hold Shift with a zone or arrow shortcut");
  await expect(help).not.toContainText("Next display");
  await expect(help.getByRole("button")).toHaveCount(0);
  await expect(help).not.toContainText("Workspace shortcuts are in Workspaces");
});

test("messages can be dismissed", async ({ page }) => {
  await page.getByRole("button", { name: "Export settings…" }).click();
  await page.getByRole("button", { name: "Dismiss message" }).click();
  await expect(toast(page)).toHaveCount(0);
});

test("every button and switch has an accessible name", async ({ page }) => {
  for (const title of ["Layouts", "Workspaces", "Editor", "Settings"] as const) {
    await showPage(page, title);
    const unnamed = await page.evaluate(
      () =>
        [...document.querySelectorAll("button, input, [role=switch]")].filter((element) => {
          const label = element.getAttribute("aria-label") ?? element.textContent ?? "";
          const labelled =
            element.getAttribute("aria-labelledby") ?? (element as HTMLInputElement).labels?.length;
          return label.trim() === "" && !labelled;
        }).length,
    );
    expect(unnamed, `unnamed controls on ${title}`).toBe(0);
  }
});

const snappyKeyToggles = (page: Page) =>
  group(page, "Snappy key").getByRole("group", { name: "Snappy key modifiers" }).getByRole("button");

test("the Snappy key shows its modifiers as toggles named for each platform", async ({ page }) => {
  await expect(snappyKeyToggles(page)).toHaveText(["Control", "Option", "Command", "Shift"]);
  const pressed = async () =>
    snappyKeyToggles(page).evaluateAll((buttons) =>
      buttons.map((button) => button.getAttribute("aria-pressed")),
    );
  expect(await pressed()).toEqual(["true", "true", "true", "false"]);
  await expect(group(page, "Snappy key").locator(".group-caption")).toHaveText(
    "Shortcuts shown as Snappy hold the Snappy key, Control + Option + Command. Choose at least two modifiers.",
  );
  await expect(group(page, "Snappy key").getByRole("button", { name: "Reset to default" })).toBeDisabled();

  for (const [platform, names] of [
    ["windows", ["Ctrl", "Alt", "Win", "Shift"]],
    ["linux", ["Ctrl", "Alt", "Super", "Shift"]],
  ] as const) {
    await openApp(page, platform);
    await showPage(page, "Settings");
    await expect(snappyKeyToggles(page)).toHaveText([...names]);
  }
});

test("changing the Snappy key keeps shortcuts named Snappy and the labels follow it", async ({ page }) => {
  await group(page, "Snappy key").getByRole("button", { name: "Command", exact: true }).click();
  expect(await lastCommandOf(page, "setSnappyKey")).toEqual({
    kind: "setSnappyKey",
    modifiers: ["ctrl", "alt"],
  });
  await expect(toast(page)).toContainText("The Snappy key is now Control + Option.");
  await expect(group(page, "Snappy key").locator(".group-caption")).toContainText(
    "the Snappy key, Control + Option.",
  );
  await expect(
    group(page, "Snappy key").getByRole("button", { name: "Command", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  const left = page.getByRole("button", { name: "Move window left", exact: true });
  await expect(left).toHaveText("Snappy + ←");
  await expect(left).toHaveAttribute(
    "title",
    "Click and press a combination with Control, Option, or Command; hold Control + Option for the Snappy key. Escape cancels; Delete clears.",
  );

  await left.click();
  await page.keyboard.press("Control+Alt+KeyM");
  await expect(left).toHaveText("Snappy + M");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: { code: "KeyM", mods: ["snappy"] },
  });

  await page.getByRole("button", { name: "Shortcuts and help" }).click();
  const help = page.getByRole("dialog", { name: "Shortcuts and help" });
  await expect(help).toContainText("Snappy keyControl + Option");
  await expect(help).toContainText("Move to the neighboring zoneSnappy + ← → ↑ ↓");
  await page.keyboard.press("Escape");

  await group(page, "Snappy key").getByRole("button", { name: "Shift", exact: true }).click();
  expect(await lastCommandOf(page, "setSnappyKey")).toEqual({
    kind: "setSnappyKey",
    modifiers: ["ctrl", "alt", "shift"],
  });
  await expect(group(page, "Snappy key").locator(".group-caption")).toContainText(
    "the Snappy key, Control + Option + Shift.",
  );
});

test("reset to default brings back Control, Option, and Command", async ({ page }) => {
  const reset = group(page, "Snappy key").getByRole("button", { name: "Reset to default" });
  await group(page, "Snappy key").getByRole("button", { name: "Control", exact: true }).click();
  await expect(reset).toBeEnabled();
  await reset.click();
  expect(await lastCommandOf(page, "setSnappyKey")).toEqual({
    kind: "setSnappyKey",
    modifiers: ["ctrl", "alt", "meta"],
  });
  await expect(group(page, "Snappy key").locator(".group-caption")).toContainText(
    "the Snappy key, Control + Option + Command.",
  );
  await expect(reset).toBeDisabled();
});

test("a Snappy key change that clashes shows why and changes nothing", async ({ page }) => {
  const snappyKey = group(page, "Snappy key");
  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  await snappyKey.getByRole("button", { name: "Option", exact: true }).click();
  await expect(snappyKey.getByRole("alert")).toHaveText("The Snappy key needs at least two modifiers.");
  await expect(snappyKey.getByRole("button", { name: "Option", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  await snappyKey.getByRole("button", { name: "Reset to default" }).click();
  await expect(snappyKey.getByRole("alert")).toHaveCount(0);
  await takeShortcut(page, { code: "Space", mods: ["ctrl", "alt"] });
  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  const refusal =
    "Snappy + Space for Move to the nearest empty zone would be Control + Option + Space, which macOS or another app already uses.";
  await expect(snappyKey.getByRole("alert")).toHaveText(refusal);
  await expect(toast(page)).toContainText(refusal);
  await expect(snappyKey.getByRole("button", { name: "Command", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  const undo = group(page, "Window history").getByRole("button", { name: "Undo window move", exact: true });
  await undo.click();
  await page.keyboard.press("Control+Alt+KeyJ");
  await expect(undo).toHaveText("Ctrl + Opt + J");
  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  await expect(snappyKey.getByRole("alert")).toHaveText(
    "Snappy + J for Focus left would collide with Undo window move.",
  );
  await expect(undo).toHaveText("Ctrl + Opt + J");
});

test("a shortcut recorded without the Snappy key keeps its keys when the key comes to match them", async ({
  page,
}) => {
  const snappyKey = group(page, "Snappy key");
  const undo = group(page, "Window history").getByRole("button", { name: "Undo window move", exact: true });
  await undo.click();
  await page.keyboard.press("Control+Alt+Shift+KeyU");
  await expect(undo).toHaveText("Ctrl + Opt + Shift + U");
  expect(await lastCommandOf(page, "setHistoryShortcut")).toEqual({
    kind: "setHistoryShortcut",
    step: "undo",
    chord: { code: "KeyU", mods: ["shift", "ctrl", "alt"] },
  });

  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  await snappyKey.getByRole("button", { name: "Shift", exact: true }).click();
  await expect(snappyKey.locator(".group-caption")).toContainText(
    "the Snappy key, Control + Option + Shift.",
  );
  await expect(undo).toHaveText("Ctrl + Opt + Shift + U");

  await undo.click();
  await page.keyboard.press("Control+Alt+Shift+KeyU");
  await expect(undo).toHaveText("Snappy + U");
  await snappyKey.getByRole("button", { name: "Reset to default" }).click();
  await expect(undo).toHaveText("Snappy + U");
});
