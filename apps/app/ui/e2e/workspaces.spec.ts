import { expect, test, type Page } from "@playwright/test";

import {
  chordIn,
  edgesOf,
  expectRecordersLinedUp,
  lastCommandOf,
  openApp,
  showPage,
  spokenText,
  toast,
  workspaceCard,
} from "./support";

const cardNames = (page: Page) => page.locator(".workspace-card-name").allTextContents();
const nameField = (page: Page) => page.getByRole("textbox", { name: "Workspace name" });

/** Give each workspace Control+n to switch, in place of its default, and Control+Shift+n to send. */
const assignShortcuts = (page: Page) =>
  page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      settings: {
        ...state.settings,
        workspaces: {
          ...state.settings.workspaces,
          items: state.settings.workspaces.items.map((item, index) => ({
            ...item,
            switchShortcut: { code: `Digit${index + 1}`, mods: ["ctrl"] },
            sendShortcut: { code: `Digit${index + 1}`, mods: ["shift", "ctrl"] },
          })),
        },
      },
    })),
  );

/** Give every workspace the same name. */
const nameEveryWorkspace = (page: Page, name: string) =>
  page.evaluate(
    (shared) =>
      window.snappyFake.patchState((state) => ({
        ...state,
        settings: {
          ...state.settings,
          workspaces: {
            ...state.settings.workspaces,
            items: state.settings.workspaces.items.map((item) => ({ ...item, name: shared })),
          },
        },
      })),
    name,
  );

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await showPage(page, "Workspaces");
});

test("the page explains how to start", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "Workspaces", exact: true })).toBeVisible();
  await expect(page.getByText("Switch between window groups and their layouts.")).toBeVisible();
  await expect(
    page.getByText("Enable workspaces to start. Your open windows will stay in the current workspace."),
  ).toBeVisible();
  await page.getByRole("switch", { name: "Enable workspaces" }).click();
  await expect(page.getByRole("switch", { name: "Enable workspaces" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.locator(".workspace-card").first()).toContainText("4 windows");
  await expect(page.locator(".workspace-detail-heading")).toContainText("Current");
});

test("cards show their switch shortcut, in saved order", async ({ page }) => {
  expect(await cardNames(page)).toEqual(["Workspace 1", "Workspace 2", "Workspace 3"]);
  const shortcut = chordIn(workspaceCard(page, "Workspace 2"));
  await expect(shortcut).toHaveAttribute("aria-label", "Snappy + 2");
  await expect(shortcut).toHaveText("Snappy+2");
  await expect(shortcut.locator(".snappy-key-glyph")).toBeVisible();
  await assignShortcuts(page);
  await expect(shortcut).toHaveAttribute("aria-label", "Control ⌃ + 2");
  await expect(shortcut.locator(".chord-key")).toHaveText(["Control ⌃", "2"]);
});

test("cards count windows, and say everything they show to assistive technology", async ({ page }) => {
  await expect(workspaceCard(page, "Workspace 1")).toHaveAccessibleName("Configure Workspace 1, Snappy + 1");
  await page.getByRole("switch", { name: "Enable workspaces" }).click();
  await expect(workspaceCard(page, "Workspace 1")).toContainText("4 windows");
  await expect(workspaceCard(page, "Workspace 3")).toContainText("0 windows");
  await expect(workspaceCard(page, "Workspace 1")).toHaveAccessibleName(
    "Configure Workspace 1, current workspace, Snappy + 1, 4 windows",
  );
  await expect(workspaceCard(page, "Workspace 3")).toHaveAccessibleName(
    "Configure Workspace 3, Snappy + 3, 0 windows",
  );
});

test("each workspace switches with the Snappy key and its number by default", async ({ page }) => {
  const switchRecorder = page.getByRole("button", { name: "Switch to this workspace", exact: true });
  for (const number of [1, 2, 3]) {
    await workspaceCard(page, `Workspace ${number}`).click();
    await expect(chordIn(switchRecorder)).toHaveAttribute("aria-label", `Snappy + ${number}`);
  }
  await expect(page.getByRole("button", { name: "Send window to this workspace", exact: true })).toHaveText(
    "Record shortcut",
  );
});

test("new workspaces are numbered, switch with their number, and are selected", async ({ page }) => {
  await page.getByRole("button", { name: "New workspace" }).click();
  await expect(workspaceCard(page, "Workspace 4")).toHaveAttribute("aria-pressed", "true");
  await expect(nameField(page)).toHaveValue("Workspace 4");
  await expect(
    chordIn(page.getByRole("button", { name: "Switch to this workspace", exact: true })),
  ).toHaveAttribute("aria-label", "Snappy + 4");
  await expect(
    page.getByRole("button", { name: "Send window to this workspace and switch", exact: true }),
  ).toHaveText("Record shortcut");
});

test("renaming commits trimmed on enter and reverts when blank", async ({ page }) => {
  await nameField(page).fill("  Writing  ");
  await nameField(page).press("Enter");
  await expect(workspaceCard(page, "Writing")).toBeVisible();
  expect((await lastCommandOf(page, "updateWorkspace"))?.workspace.name).toBe("Writing");

  await nameField(page).fill("   ");
  await nameField(page).press("Enter");
  await expect(nameField(page)).toHaveValue("Writing");
});

test("dragging a card reorders the workspaces", async ({ page }) => {
  await assignShortcuts(page);
  const target = await workspaceCard(page, "Workspace 3").boundingBox();
  if (!target) {
    throw new Error("card missing");
  }
  await workspaceCard(page, "Workspace 1").dragTo(workspaceCard(page, "Workspace 3"), {
    targetPosition: { x: target.width / 2, y: target.height - 4 },
  });
  await expect.poll(() => cardNames(page)).toEqual(["Workspace 2", "Workspace 3", "Workspace 1"]);
  await expect(chordIn(workspaceCard(page, "Workspace 1"))).toHaveAttribute("aria-label", "Control ⌃ + 1");
});

test("option and arrow keys reorder a focused card, as its description says", async ({ page }) => {
  await expect(workspaceCard(page, "Workspace 3")).toHaveAccessibleDescription(
    "Press Option and the Up or Down arrow to move this workspace.",
  );
  await workspaceCard(page, "Workspace 3").focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(() => cardNames(page)).toEqual(["Workspace 1", "Workspace 3", "Workspace 2"]);
});

test("the delete dialog keeps focus on its buttons", async ({ page }) => {
  await workspaceCard(page, "Workspace 2").click();
  await page.getByRole("button", { name: "Delete workspace" }).click();
  const dialog = page.getByRole("alertdialog", { name: "Delete Workspace 2?" });
  const cancel = dialog.getByRole("button", { name: "Cancel" });
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(dialog.getByRole("button", { name: "Delete workspace" })).toBeFocused();
});

test("deleting asks first and names where the windows go", async ({ page }) => {
  await workspaceCard(page, "Workspace 2").click();
  await page.getByRole("button", { name: "Delete workspace" }).click();
  const dialog = page.getByRole("alertdialog", { name: "Delete Workspace 2?" });
  await expect(dialog).toContainText("Its windows move to Workspace 1. No windows close.");
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);

  await page.getByRole("button", { name: "Delete workspace" }).click();
  await page.getByRole("button", { name: "Delete workspace", exact: true }).last().click();
  await expect.poll(() => cardNames(page)).toEqual(["Workspace 1", "Workspace 3"]);
});

test("deleting the active workspace moves its windows to the first other one", async ({ page }) => {
  await page.getByRole("button", { name: "Delete workspace" }).click();
  await expect(page.getByRole("alertdialog")).toContainText("Its windows move to Workspace 2.");
});

test("a workspace's display layout changes only that workspace", async ({ page }) => {
  await page.getByRole("button", { name: "Layout on Studio Display" }).click();
  await page.getByRole("option", { name: "Two rows" }).click();
  const update = await lastCommandOf(page, "updateWorkspace");
  expect(update?.workspace.displays["display-2"]).toBe("rows");
  expect(await lastCommandOf(page, "applyLayout")).toBeUndefined();
});

test("the three per-workspace shortcuts are recorders", async ({ page }) => {
  await assignShortcuts(page);
  await expect(
    chordIn(page.getByRole("button", { name: "Switch to this workspace", exact: true })),
  ).toHaveAttribute("aria-label", "Control ⌃ + 1");
  await expect(
    chordIn(page.getByRole("button", { name: "Send window to this workspace", exact: true })),
  ).toHaveAttribute("aria-label", "Control ⌃ + Shift ⇧ + 1");
  await expect(
    page.getByRole("button", { name: "Send window to this workspace and switch", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Switch between workspaces")).toHaveCount(0);
});

test("of two workspaces with one name, a refusal links only to the shortcut of the one it names", async ({
  page,
}) => {
  await nameEveryWorkspace(page, "Work");
  await page.locator(".workspace-card").nth(1).click();
  const send = page.getByRole("button", { name: "Send window to this workspace", exact: true });
  const link = toast(page).getByRole("button", { name: "Go to Switch to Work" });

  await send.click();
  await page.keyboard.press("Control+Alt+Meta+Digit1");
  await expect
    .poll(() => spokenText(toast(page)))
    .toContain("Snappy + 1 is already assigned to Switch to Work.");
  await expect(toast(page).locator(".reference").last()).toHaveText("Switch to Work");
  await expect(link).toHaveCount(0);

  await page.keyboard.press("Control+Alt+Meta+Digit2");
  await expect
    .poll(() => spokenText(toast(page)))
    .toContain("Snappy + 2 is already assigned to Switch to Work.");
  await link.click();
  const own = page.getByRole("button", { name: "Switch to this workspace", exact: true });
  await expect(own).toBeFocused();
  await expect(chordIn(own)).toHaveAttribute("aria-label", "Snappy + 2");
});

test("the per-workspace recorders are one width, as wide as the longest shortcut needs", async ({ page }) => {
  const shortcuts = page.getByRole("region", { name: "Shortcuts", exact: true });
  const usual = await expectRecordersLinedUp(shortcuts, 3);
  const sendAndSwitch = page.getByRole("button", {
    name: "Send window to this workspace and switch",
    exact: true,
  });
  await sendAndSwitch.click();
  await page.keyboard.press("Control+Alt+Shift+KeyS");
  await expect(chordIn(sendAndSwitch)).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Shift ⇧ + S");
  expect(await expectRecordersLinedUp(shortcuts, 3)).toBeGreaterThan(usual);
});

test("when Snappy can't control windows, the page says why instead of asking for access", async ({
  page,
}) => {
  const reason = "Log out and back in to finish setting up Snappy's GNOME Shell extension.";
  await page.evaluate(
    (unavailableReason) =>
      window.snappyFake.patchState((state) => ({
        ...state,
        platform: "linux",
        settings: { ...state.settings, workspaces: { ...state.settings.workspaces, enabled: true } },
        snapping: { ...state.snapping, live: false, needsPermission: false, unavailableReason },
        workspaces: { ...state.workspaces, running: false, pauseMessage: null },
      })),
    reason,
  );
  await expect(page.locator(".workspace-detail").getByText(reason, { exact: true })).toBeVisible();
  await expect(page.getByText("Waiting for Accessibility access.")).toHaveCount(0);
});

test("each card's shortcut sits at its right edge, apart from its name and windows", async ({ page }) => {
  await page.getByRole("switch", { name: "Enable workspaces" }).click();
  await expect(page.locator(".workspace-card-windows")).toHaveCount(3);
  const cards = await page.locator(".workspace-card").evaluateAll((buttons) =>
    buttons.map((button) => {
      const box = (selector: string) => {
        const part = button.querySelector(selector);
        if (!part) {
          throw new Error(`The card has no ${selector}`);
        }
        return part.getBoundingClientRect();
      };
      const name = button.querySelector(".workspace-card-name");
      const [title, chord, windows] = [
        box(".workspace-card-title"),
        box(".chord"),
        box(".workspace-card-windows"),
      ];
      const content = box(".workspace-card-top");
      return {
        nameWhole: name !== null && name.scrollWidth <= name.clientWidth,
        titleLeft: Math.round(title.left - content.left),
        chordRight: Math.round(content.right - chord.right),
        windowsLeft: Math.round(windows.left - content.left),
        windowsBelow: windows.top >= title.bottom,
      };
    }),
  );
  for (const each of cards) {
    expect(each).toEqual({
      nameWhole: true,
      titleLeft: 0,
      chordRight: 0,
      windowsLeft: 0,
      windowsBelow: true,
    });
  }
});

test("a long shortcut on a card ends in an ellipsis and leaves the name whole", async ({ page }) => {
  await page.setViewportSize({ width: 940, height: 760 });
  await workspaceCard(page, "Workspace 3").click();
  await page.getByRole("button", { name: "Switch to this workspace", exact: true }).click();
  await page.keyboard.press("Control+Alt+Shift+PageDown");
  const shortcut = chordIn(workspaceCard(page, "Workspace 3"));
  await expect(shortcut).toHaveAttribute("aria-label", "Control ⌃ + Option ⌥ + Shift ⇧ + PageDown");
  const fit = await workspaceCard(page, "Workspace 3").evaluate((button) => {
    const fits = (selector: string) => {
      const part = button.querySelector(selector);
      return part !== null && part.scrollWidth <= part.clientWidth;
    };
    const inside = (selector: string) => {
      const part = button.querySelector(selector)?.getBoundingClientRect();
      const edges = button.getBoundingClientRect();
      return part !== undefined && part.left >= edges.left && part.right <= edges.right;
    };
    return { name: fits(".workspace-card-name"), keys: fits(".chord-keys"), inside: inside(".chord") };
  });
  expect(fit).toEqual({ name: true, keys: false, inside: true });
});

test("the new workspace tile lines up with the cards", async ({ page }) => {
  const tile = page.getByRole("button", { name: "New workspace" });
  expect(await edgesOf(tile)).toEqual(await edgesOf(page.locator(".workspace-card").first()));
});
