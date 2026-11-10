import { expect, test, type Page } from "@playwright/test";

import { lastCommandOf, openApp, sentCommands, showPage, toast } from "./support";

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
    "Move between zones",
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
  expect(await lastCommandOf(page, "setNearestEmptyShortcut")).toEqual({
    kind: "setNearestEmptyShortcut",
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

test("the workspaces group shows the step, send, and take-along shortcuts", async ({ page }) => {
  const workspaces = group(page, "Workspaces");
  const recorder = (name: string) => workspaces.getByRole("button", { name, exact: true });
  await expect(workspaces.locator(".recorder-button")).toHaveCount(5);
  await expect(recorder("Previous workspace")).toHaveText("Snappy + ;");
  await expect(recorder("Next workspace")).toHaveText("Snappy + '");
  await expect(recorder("Send window to previous workspace")).toHaveText("Snappy + .");
  await expect(recorder("Send window to next workspace")).toHaveText("Snappy + /");
  await expect(recorder("Take along")).toHaveText("Snappy + ↩");
});

test("the take-along shortcut can be changed and refuses a chord in use", async ({ page }) => {
  const takeAlong = group(page, "Workspaces").getByRole("button", { name: "Take along", exact: true });
  await takeAlong.click();
  await page.keyboard.press("Control+Alt+Meta+KeyT");
  await expect(takeAlong).toHaveText("Snappy + T");
  expect(await lastCommandOf(page, "setWorkspaceStepShortcut")).toMatchObject({
    step: "takeAlong",
    chord: { code: "KeyT", mods: ["snappy"] },
  });

  await takeAlong.click();
  await page.keyboard.press("Control+Alt+Meta+Slash");
  await expect(toast(page)).toContainText("Snappy + / is already assigned to Send window to next workspace.");
  await page.keyboard.press("Escape");
  await expect(takeAlong).toHaveText("Snappy + T");
});

test("the workspace defaults use the Snappy key on Windows and Linux too", async ({ page }) => {
  for (const platform of ["windows", "linux"] as const) {
    await openApp(page, platform);
    await showPage(page, "Settings");
    await expect(group(page, "Workspaces").locator(".recorder-button")).toHaveText([
      "Snappy + ;",
      "Snappy + '",
      "Snappy + .",
      "Snappy + /",
      "Snappy + ↩",
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
  await expect(help).toContainText("Switch to workspace 1–9Snappy + 1–9");
  await expect(help).toContainText("Send window to previous or next workspaceSnappy + . /");
  await expect(help).toContainText("Take window alongSnappy + Return");
  await expect(help).toContainText("Move to the neighboring zoneSnappy + ← → ↑ ↓");
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
