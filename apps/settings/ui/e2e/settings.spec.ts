import { expect, test, type Page } from "@playwright/test";

import {
  chordIn,
  chordsIn,
  expectRecordersLinedUp,
  lastCommandOf,
  openApp,
  sentCommands,
  showPage,
  spokenText,
  takeShortcut,
  toast,
} from "./support";

const group = (page: Page, title: string) => page.getByRole("region", { name: title, exact: true });

/** Reset the Snappy key from its panel's title, answering its question. */
async function resetSnappyKey(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Reset the Snappy key" }).click();
  await page
    .getByRole("dialog", { name: "Reset the Snappy key" })
    .getByRole("button", { name: "Reset" })
    .click();
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await showPage(page, "Settings");
});

const GROUPS = [
  "Snapping",
  "Snappy key",
  "Move between zones",
  "Focus and selection",
  "Workspaces",
  "Window history",
  "Startup & access",
  "Backup & restore",
  "Debug",
  "Reset",
];

test("the groups appear in order, each with one line under the title and more behind its ⓘ", async ({
  page,
}) => {
  await expect(page.getByRole("heading", { name: "Global settings" })).toBeVisible();
  expect(await page.locator(".group-title").allTextContents()).toEqual(GROUPS);
  for (const title of GROUPS) {
    const caption = group(page, title).locator(".group-header .group-caption");
    await expect(caption).toHaveCount(1);
    const lines = await caption.evaluate(
      (element) => element.getBoundingClientRect().height / parseFloat(getComputedStyle(element).lineHeight),
    );
    expect(Math.round(lines), `the line under ${title}`).toBe(1);
    await expect(group(page, title).getByRole("button", { name: `About ${title}` })).toBeVisible();
  }
  await expect(page.getByText("Excluded apps")).toHaveCount(0);
  await expect(page.getByText("Move to next display")).toHaveCount(0);
});

test("an ⓘ shows its explanation on hover and on click, and Escape hides it", async ({ page }) => {
  const focus = group(page, "Focus and selection");
  const info = focus.getByRole("button", { name: "About Focus and selection" });
  const note = page.getByRole("note", { name: "About Focus and selection" });
  await expect(info).toHaveAttribute("aria-expanded", "false");
  await expect(note).toHaveCount(0);

  await info.hover();
  await expect(note).toHaveText("Selected windows go along when you switch or send to a workspace.");
  await page.mouse.move(0, 0);
  await expect(note).toHaveCount(0);

  await info.focus();
  await page.keyboard.press("Enter");
  await expect(info).toHaveAttribute("aria-expanded", "true");
  await expect(info).toHaveAttribute("aria-controls", (await note.getAttribute("id")) ?? "");
  await page.keyboard.press("Escape");
  await expect(note).toHaveCount(0);

  await info.click();
  await expect(note).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(note).toBeVisible();
  await page.getByRole("heading", { name: "Global settings" }).click();
  await expect(note).toHaveCount(0);
});

test("the drag modifier changes, and the reverse key never offers it", async ({ page }) => {
  await expect(group(page, "Snapping").locator(".group-caption")).toHaveText(
    "Hold a key while dragging a window to snap it into a zone.",
  );
  await page.getByRole("button", { name: "Hold to snap" }).click();
  await page.getByRole("option", { name: "Option ⌥" }).click();
  expect(await lastCommandOf(page, "setDragModifier")).toEqual({ kind: "setDragModifier", modifier: "alt" });
  await expect(page.getByRole("button", { name: "Hold to snap" })).toHaveText("Option ⌥");

  await page.getByRole("button", { name: "Reverse key" }).click();
  const options = await page
    .getByRole("listbox", { name: "Reverse key" })
    .getByRole("option")
    .allTextContents();
  expect(options).toEqual(["Command ⌘", "Control ⌃", "Shift ⇧", "Off"]);
  await page.getByRole("option", { name: "Off" }).click();
  expect(await lastCommandOf(page, "setSwapModifier")).toEqual({ kind: "setSwapModifier", modifier: "off" });
  await expect(page.getByRole("button", { name: "Reverse key" })).toHaveText("Off");
});

test("the reverse key's hint says the opposite of what a taken zone does", async ({ page }) => {
  const reverse = page.getByRole("button", { name: "Reverse key" });
  await expect(reverse).toHaveAccessibleDescription("Hold while dropping to stack instead of swapping");
  await page.getByRole("button", { name: "When the zone is taken" }).click();
  await page.getByRole("option", { name: "Stack on top" }).click();
  await expect(reverse).toHaveAccessibleDescription("Hold while dropping to swap instead of stacking");
  await expect(group(page, "Snapping")).toContainText("Hold while dropping to swap instead of stacking");
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
    "Move the focused window to a neighboring or empty zone.",
  );
  await expect
    .poll(() => chordsIn(zones))
    .toEqual(["Snappy + ←", "Snappy + →", "Snappy + ↑", "Snappy + ↓", "Snappy + Space"]);
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
  await expect(chordIn(recorder)).toHaveAttribute("aria-label", "Snappy + E");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "nearestEmpty",
    chord: { code: "KeyE", mods: ["snappy"] },
  });

  await recorder.click();
  await page.keyboard.press("Control+Alt+Meta+ArrowLeft");
  await expect
    .poll(() => spokenText(toast(page)))
    .toContain("Snappy + ← is already assigned to Move window left.");
  await page.keyboard.press("Escape");
  await expect(chordIn(recorder)).toHaveAttribute("aria-label", "Snappy + E");
});

test("the nearest-empty-zone default is Snappy + Space on Windows and Linux too", async ({ page }) => {
  for (const platform of ["windows", "linux"] as const) {
    await openApp(page, platform);
    await showPage(page, "Settings");
    const recorder = group(page, "Move between zones").getByRole("button", {
      name: "Move to the nearest empty zone",
      exact: true,
    });
    await expect(chordIn(recorder)).toHaveAttribute("aria-label", "Snappy + Space");
  }
});

test("the workspaces group shows the step and send shortcuts", async ({ page }) => {
  const workspaces = group(page, "Workspaces");
  const recorder = (name: string) => workspaces.getByRole("button", { name, exact: true });
  await expect(workspaces.locator(".recorder-button")).toHaveCount(4);
  await expect(chordIn(recorder("Previous workspace"))).toHaveAttribute("aria-label", "Snappy + ;");
  await expect(chordIn(recorder("Next workspace"))).toHaveAttribute("aria-label", "Snappy + '");
  await expect(chordIn(recorder("Send window to previous workspace"))).toHaveAttribute(
    "aria-label",
    "Snappy + .",
  );
  await expect(chordIn(recorder("Send window to next workspace"))).toHaveAttribute(
    "aria-label",
    "Snappy + /",
  );
  await expect(workspaces.getByText("Take along")).toHaveCount(0);
});

test("the focus and selection group shows its shortcuts", async ({ page }) => {
  const focus = group(page, "Focus and selection");
  await expect(focus.locator(".group-caption")).toHaveText(
    "Move focus between windows, and select windows to move together.",
  );
  await expect
    .poll(() => chordsIn(focus))
    .toEqual(["Snappy + I", "Snappy + J", "Snappy + K", "Snappy + L", "Snappy + ↩", "Snappy + \\"]);
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
  await expect(chordIn(select)).toHaveAttribute("aria-label", "Snappy + T");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "selectWindow",
    chord: { code: "KeyT", mods: ["snappy"] },
  });

  await select.click();
  await page.keyboard.press("Control+Alt+Meta+Space");
  await expect
    .poll(() => spokenText(toast(page)))
    .toContain("Snappy + Space is already assigned to Move to the nearest empty zone.");
  await page.keyboard.press("Escape");
  await expect(chordIn(select)).toHaveAttribute("aria-label", "Snappy + T");
});

test("clearing the selection refuses the Snappy key with Delete on macOS", async ({ page }) => {
  const clear = group(page, "Focus and selection").getByRole("button", {
    name: "Clear selection",
    exact: true,
  });
  await clear.click();
  await page.keyboard.press("Control+Alt+Meta+Backspace");
  await expect
    .poll(() => spokenText(toast(page)))
    .toContain("macOS reserves Control ⌃ + Option ⌥ + Command ⌘ + Delete; it never reaches apps.");
  await page.keyboard.press("Escape");
  await expect(chordIn(clear)).toHaveAttribute("aria-label", "Snappy + \\");
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
  await expect(chordIn(clear)).toHaveAttribute("aria-label", "Snappy + ⌫");
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
    await expect
      .poll(() => chordsIn(group(page, "Workspaces")))
      .toEqual(["Snappy + ;", "Snappy + '", "Snappy + .", "Snappy + /"]);
    await expect
      .poll(() => chordsIn(group(page, "Focus and selection")))
      .toEqual(["Snappy + I", "Snappy + J", "Snappy + K", "Snappy + L", "Snappy + ↩", "Snappy + \\"]);
  }
});

test("window history has undo and redo recorders with no default", async ({ page }) => {
  const history = group(page, "Window history");
  await expect(history.locator(".recorder-button")).toHaveText(["Record shortcut", "Record shortcut"]);
  await expect(history.getByRole("button", { name: "Undo window move", exact: true })).toBeVisible();
  await expect(history.getByRole("button", { name: "Redo window move", exact: true })).toBeVisible();
});

test("startup and access show the login item and accessibility's status", async ({ page }) => {
  const startup = group(page, "Startup & access");
  await page.getByRole("switch", { name: "Open at login" }).click();
  expect(await lastCommandOf(page, "setLoginItem")).toEqual({ kind: "setLoginItem", enabled: true });
  const status = startup.locator(".row", { hasText: "Accessibility" }).locator(".status-badge");
  await expect(status).toHaveText("Allowed");
  await expect(status).toHaveAttribute("data-good", "true");
  await expect(startup.getByRole("button", { name: "Check access" })).toHaveCount(0);

  await page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      settings: { ...state.settings, snapping: { ...state.settings.snapping, enabled: false } },
      snapping: { ...state.snapping, live: false, permissionGranted: false },
    })),
  );
  await expect(status).toHaveText("Not allowed");
  await expect(status).toHaveAttribute("data-good", "false");
  await startup.getByRole("button", { name: "Check access" }).click();
  expect(await lastCommandOf(page, "setSnapping")).toEqual({ kind: "setSnapping", enabled: true });
  await expect(status).toHaveText("Not allowed");

  await page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      snapping: { ...state.snapping, live: true, permissionGranted: true },
    })),
  );
  await expect(status).toHaveText("Allowed");
  await expect(startup.getByRole("button", { name: "Check access" })).toHaveCount(0);
});

test("the debug log is turned on in Settings and shows where it's written", async ({ page }) => {
  const logToggle = page.getByRole("switch", { name: "Debug log" });
  await expect(logToggle).not.toBeChecked();
  await expect(page.getByText("debug.jsonl")).toHaveCount(0);
  await logToggle.click();
  expect(await lastCommandOf(page, "setDebugLog")).toEqual({ kind: "setDebugLog", enabled: true });
  await expect(logToggle).toBeChecked();
  await expect(group(page, "Debug").getByText("~/Library/Logs/Snappy/debug.jsonl")).toBeVisible();
});

test("platforms without a permission step hide the access controls", async ({ page }) => {
  await openApp(page, "linux");
  await showPage(page, "Settings");
  await expect(page.locator(".status-badge")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Check access" })).toHaveCount(0);
});

test("export and import go through the file picker", async ({ page }) => {
  await page.getByRole("button", { name: "Export settings" }).click();
  await expect(toast(page)).toContainText("Layouts and preferences exported.");
  expect(await lastCommandOf(page, "exportSettings")).toEqual({
    kind: "exportSettings",
    path: "/Users/snappy/Snappy-layouts.json",
  });
  await page.getByRole("button", { name: "Import settings" }).click();
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
  const spoken = () => spokenText(help);
  await expect.poll(spoken).toContain("Stack on top while draggingShift ⇧ + Command ⌘");
  await expect.poll(spoken).toContain("Snappy keyControl ⌃ + Option ⌥ + Command ⌘");
  await expect(help.locator(".help-row .snappy-key-glyph")).not.toHaveCount(0);
  await expect(help.locator(".help-row .snappy-key").first()).toHaveText("Snappy");
  await expect(help.locator(".reference")).toHaveCount(0);
  await expect.poll(spoken).toContain("Switch to workspace 1–9, taking the selection alongSnappy + 1–9");
  await expect
    .poll(spoken)
    .toContain("Send the window or selection to the previous or next workspaceSnappy + . /");
  await expect.poll(spoken).toContain("Move to the neighboring zoneSnappy + ← → ↑ ↓");
  await expect.poll(spoken).toContain("Focus the window up, left, down, or rightSnappy + I J K L");
  await expect.poll(spoken).toContain("Select or unselect the windowSnappy + ↩");
  await expect.poll(spoken).toContain("Clear the selectionSnappy + \\");
  await expect
    .poll(spoken)
    .toContain("Move to the nearest empty zone, or the selection into empty zonesSnappy + Space");
  await expect(help.getByRole("img", { name: "Snappy + ← → ↑ ↓" })).toHaveText("Snappy+← → ↑ ↓");
  await expect(help).not.toContainText("Take window along");
  await expect(help).not.toContainText("Default zone shortcuts");
  await expect(help).not.toContainText("Swap with the keyboard");
  await expect(help).not.toContainText("Hold Shift with a zone or arrow shortcut");
  await expect(help).not.toContainText("Next display");
  await expect(help.getByRole("button")).toHaveCount(0);
  await expect(help).not.toContainText("Workspace shortcuts are in Workspaces");
});

test("messages can be dismissed", async ({ page }) => {
  await page.getByRole("button", { name: "Export settings" }).click();
  await page.getByRole("button", { name: "Dismiss message" }).click();
  await expect(toast(page)).toHaveCount(0);
});

test("every button and switch has an accessible name", async ({ page }) => {
  for (const title of ["Layouts", "Editor", "Workspaces", "Settings"] as const) {
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

/** The modifiers the Snappy key toggles show as held. */
const heldModifiers = (page: Page) =>
  snappyKeyToggles(page).evaluateAll((buttons) =>
    buttons
      .filter((button) => button.getAttribute("aria-pressed") === "true")
      .map((button) => button.textContent),
  );

test("the Snappy key shows its modifiers as toggles named for each platform", async ({ page }) => {
  await expect(snappyKeyToggles(page)).toHaveText(["Control", "Option", "Command", "Shift"]);
  expect(await heldModifiers(page)).toEqual(["Control", "Option", "Command"]);
  const caption = group(page, "Snappy key").locator(".group-caption");
  await expect(caption).toHaveText("The keys every Snappy shortcut holds.");
  await expect(caption.locator(".snappy-key-glyph")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Reset the Snappy key" })).toBeDisabled();

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
  await expect(toast(page)).toContainText("The Snappy key is now Control ⌃ + Option ⌥.");
  await expect.poll(() => heldModifiers(page)).toEqual(["Control", "Option"]);
  await expect(
    group(page, "Snappy key").getByRole("button", { name: "Command", exact: true }),
  ).toHaveAttribute("aria-pressed", "false");
  const left = page.getByRole("button", { name: "Move window left", exact: true });
  await expect(chordIn(left)).toHaveAttribute("aria-label", "Snappy + ←");
  await expect(left).toHaveAttribute(
    "title",
    "Click and press a combination with Control, Option, or Command; hold Control ⌃ + Option ⌥ for the Snappy key. Escape cancels; Delete clears.",
  );

  await left.click();
  await page.keyboard.press("Control+Alt+KeyM");
  await expect(chordIn(left)).toHaveAttribute("aria-label", "Snappy + M");
  expect(await lastCommandOf(page, "setNavigationShortcut")).toEqual({
    kind: "setNavigationShortcut",
    shortcut: "left",
    chord: { code: "KeyM", mods: ["snappy"] },
  });

  await page.getByRole("button", { name: "Shortcuts and help" }).click();
  const help = page.getByRole("dialog", { name: "Shortcuts and help" });
  await expect.poll(() => spokenText(help)).toContain("Snappy keyControl ⌃ + Option ⌥");
  await expect.poll(() => spokenText(help)).toContain("Move to the neighboring zoneSnappy + ← → ↑ ↓");
  await page.keyboard.press("Escape");

  await group(page, "Snappy key").getByRole("button", { name: "Shift", exact: true }).click();
  expect(await lastCommandOf(page, "setSnappyKey")).toEqual({
    kind: "setSnappyKey",
    modifiers: ["ctrl", "alt", "shift"],
  });
  await expect.poll(() => heldModifiers(page)).toEqual(["Control", "Option", "Shift"]);
});

test("caps lock as the snappy key is a switch of the snappy key, refused with the reason", async ({
  page,
}) => {
  const snappyKey = group(page, "Snappy key");
  const capsLock = snappyKey.getByRole("switch", { name: "Also use Caps Lock as the Snappy key" });
  await expect(capsLock).toHaveAttribute("aria-checked", "false");
  await expect(capsLock).toHaveAccessibleDescription("A tap still toggles Caps Lock.");

  const remapped =
    "Caps Lock is already remapped in macOS. Remove that mapping to use Caps Lock as the Snappy key.";
  await page.evaluate((reason) => window.snappyFake.refuseCapsLock(reason), remapped);
  await capsLock.click();
  await expect(snappyKey.getByRole("alert")).toHaveText(remapped);
  await expect(capsLock).toHaveAttribute("aria-checked", "false");

  await page.evaluate(() => window.snappyFake.refuseCapsLock(undefined));
  await capsLock.click();
  expect(await lastCommandOf(page, "setCapsLockAsSnappyKey")).toEqual({
    kind: "setCapsLockAsSnappyKey",
    enabled: true,
  });
  await expect(capsLock).toHaveAttribute("aria-checked", "true");
  await expect(snappyKey.getByRole("alert")).toHaveCount(0);
  await expect(toast(page)).toContainText("Hold Caps Lock for the Snappy key.");

  const problem = "Caps Lock is remapped by the keyboard option caps:escape now.";
  await page.evaluate(
    (reason) => window.snappyFake.patchState((state) => ({ ...state, capsLockProblem: reason })),
    problem,
  );
  await expect(snappyKey.getByRole("alert")).toHaveText(problem);
});

test("reset brings back Control, Option, and Command after asking", async ({ page }) => {
  const reset = page.getByRole("button", { name: "Reset the Snappy key" });
  await group(page, "Snappy key").getByRole("button", { name: "Control", exact: true }).click();
  await expect(reset).toBeEnabled();
  await reset.click();
  await expect(page.getByRole("dialog", { name: "Reset the Snappy key" })).toContainText(
    "Reset the Snappy key to Control ⌃ + Option ⌥ + Command ⌘?",
  );
  await page
    .getByRole("dialog", { name: "Reset the Snappy key" })
    .getByRole("button", { name: "Reset" })
    .click();
  expect(await lastCommandOf(page, "setSnappyKey")).toEqual({
    kind: "setSnappyKey",
    modifiers: ["ctrl", "alt", "meta"],
  });
  await expect.poll(() => heldModifiers(page)).toEqual(["Control", "Option", "Command"]);
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

  await resetSnappyKey(page);
  await expect(snappyKey.getByRole("alert")).toHaveCount(0);
  await takeShortcut(page, { code: "Space", mods: ["ctrl", "alt"] });
  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  const refusal =
    "Snappy + Space for Move to the nearest empty zone is already used by macOS or another app.";
  await expect.poll(() => spokenText(snappyKey.getByRole("alert"))).toBe(refusal);
  const named = chordIn(snappyKey.getByRole("alert").locator(".reference"));
  await expect(named).toHaveAttribute("aria-label", "Snappy + Space");
  await expect(named.locator(".snappy-key-glyph")).toBeVisible();
  await expect(toast(page)).toHaveCount(0);
  await expect.poll(async () => (await sentCommands(page)).at(-1)).toEqual({ kind: "dismissStatus" });
  const link = snappyKey
    .getByRole("alert")
    .getByRole("button", { name: "Go to Move to the nearest empty zone" });
  await expect(link).toHaveText("Move to the nearest empty zone");
  await link.click();
  await expect(
    page.getByRole("button", { name: "Move to the nearest empty zone", exact: true }),
  ).toBeFocused();
  await expect(snappyKey.getByRole("button", { name: "Command", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  const undo = group(page, "Window history").getByRole("button", { name: "Undo window move", exact: true });
  await undo.click();
  await page.keyboard.press("Control+Alt+KeyJ");
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + J");
  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  await expect
    .poll(() => spokenText(snappyKey.getByRole("alert")))
    .toBe("Snappy + J (Focus left) would clash with Undo window move.");
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + J");
});

test("a shortcut recorded without the Snappy key keeps its keys when the key comes to match them", async ({
  page,
}) => {
  const snappyKey = group(page, "Snappy key");
  const undo = group(page, "Window history").getByRole("button", { name: "Undo window move", exact: true });
  await undo.click();
  await page.keyboard.press("Control+Alt+Shift+KeyU");
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Shift ⇧ + U");
  expect(await lastCommandOf(page, "setHistoryShortcut")).toEqual({
    kind: "setHistoryShortcut",
    step: "undo",
    chord: { code: "KeyU", mods: ["shift", "ctrl", "alt"] },
  });

  await snappyKey.getByRole("button", { name: "Command", exact: true }).click();
  await snappyKey.getByRole("button", { name: "Shift", exact: true }).click();
  await expect.poll(() => heldModifiers(page)).toEqual(["Control", "Option", "Shift"]);
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Shift ⇧ + U");

  await undo.click();
  await page.keyboard.press("Control+Alt+Shift+KeyU");
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Snappy + U");
  await resetSnappyKey(page);
  await expect(chordIn(undo)).toHaveAttribute("aria-label", "Snappy + U");
});

test("a switch row is as tall as a row with a menu or a recorder", async ({ page }) => {
  const snapping = group(page, "Snapping");
  const heights = await snapping
    .locator(".group-body > *")
    .evaluateAll((rows) => rows.map((row) => Math.round(row.getBoundingClientRect().height)));
  expect(heights.length).toBe(4);
  const [hold, , , shortcuts] = heights;
  expect(shortcuts).toBe(hold);
  const recorderRow = group(page, "Window history").locator(".row").first();
  expect(Math.round((await recorderRow.boundingBox())?.height ?? 0)).toBe(hold);
});

test("a group's recorders are one width, as wide as its longest shortcut needs", async ({ page }) => {
  const moves = group(page, "Move between zones");
  const usual = await expectRecordersLinedUp(moves, 5);
  const right = moves.getByRole("button", { name: "Move window right", exact: true });
  await right.click();
  await page.keyboard.press("Control+Alt+Shift+KeyF");
  await expect(chordIn(right)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Shift ⇧ + F");
  expect(await expectRecordersLinedUp(moves, 5)).toBeGreaterThan(usual);

  await expectRecordersLinedUp(group(page, "Focus and selection"), 6);
  await expectRecordersLinedUp(group(page, "Workspaces"), 4);
  await expectRecordersLinedUp(group(page, "Window history"), 2);
});
