import { expect, test, type Page } from "@playwright/test";

import { lastCommandOf, openApp, showPage } from "./support";

const card = (page: Page, name: string) =>
  page.getByRole("button", { name: `Configure ${name}`, exact: true });
const cardNames = (page: Page) => page.locator(".workspace-card-name").allTextContents();
const nameField = (page: Page) => page.getByRole("textbox", { name: "Workspace name" });

/** Workspaces have no shortcuts by default; give each Control+n to switch and Control+Shift+n to send. */
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

test("cards show their switch shortcut in saved order", async ({ page }) => {
  expect(await cardNames(page)).toEqual(["Workspace 1", "Workspace 2", "Workspace 3"]);
  await expect(card(page, "Workspace 2")).not.toContainText("Ctrl");
  await assignShortcuts(page);
  await expect(card(page, "Workspace 2")).toContainText("Ctrl + 2");
});

test("new workspaces are numbered, have no shortcuts, and are selected", async ({ page }) => {
  await page.getByRole("button", { name: "New workspace" }).click();
  await expect(card(page, "Workspace 4")).toHaveAttribute("aria-pressed", "true");
  await expect(nameField(page)).toHaveValue("Workspace 4");
  await expect(page.getByRole("button", { name: "Switch to this workspace", exact: true })).toHaveText(
    "Record shortcut",
  );
});

test("renaming commits trimmed on enter and reverts when blank", async ({ page }) => {
  await nameField(page).fill("  Writing  ");
  await nameField(page).press("Enter");
  await expect(card(page, "Writing")).toBeVisible();
  expect((await lastCommandOf(page, "updateWorkspace"))?.workspace.name).toBe("Writing");

  await nameField(page).fill("   ");
  await nameField(page).press("Enter");
  await expect(nameField(page)).toHaveValue("Writing");
});

test("dragging a card reorders the workspaces", async ({ page }) => {
  await assignShortcuts(page);
  const target = await card(page, "Workspace 3").boundingBox();
  if (!target) {
    throw new Error("card missing");
  }
  await card(page, "Workspace 1").dragTo(card(page, "Workspace 3"), {
    targetPosition: { x: target.width / 2, y: target.height - 4 },
  });
  await expect.poll(() => cardNames(page)).toEqual(["Workspace 2", "Workspace 3", "Workspace 1"]);
  await expect(card(page, "Workspace 1")).toContainText("Ctrl + 1");
});

test("option and arrow keys reorder a focused card", async ({ page }) => {
  await card(page, "Workspace 3").focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(() => cardNames(page)).toEqual(["Workspace 1", "Workspace 3", "Workspace 2"]);
});

test("deleting asks first and names where the windows go", async ({ page }) => {
  await card(page, "Workspace 2").click();
  await page.getByRole("button", { name: "Delete workspace" }).click();
  const dialog = page.getByRole("alertdialog", { name: "Delete Workspace 2?" });
  await expect(dialog).toContainText(
    "Its windows will be moved to Workspace 1. Your windows and layouts will be kept.",
  );
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);

  await page.getByRole("button", { name: "Delete workspace" }).click();
  await page.getByRole("button", { name: "Delete workspace", exact: true }).last().click();
  await expect.poll(() => cardNames(page)).toEqual(["Workspace 1", "Workspace 3"]);
});

test("deleting the active workspace moves its windows to the first other one", async ({ page }) => {
  await page.getByRole("button", { name: "Delete workspace" }).click();
  await expect(page.getByRole("alertdialog")).toContainText("Its windows will be moved to Workspace 2.");
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
  await expect(page.getByRole("button", { name: "Switch to this workspace", exact: true })).toHaveText(
    "Ctrl + 1",
  );
  await expect(page.getByRole("button", { name: "Move window here", exact: true })).toHaveText(
    "Ctrl + Shift + 1",
  );
  await expect(page.getByRole("button", { name: "Move here and switch", exact: true })).toBeVisible();
  await expect(page.getByText("Switch between workspaces")).toHaveCount(0);
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
  await expect(page.locator(".workspace-detail, .page").getByText(reason).last()).toBeVisible();
  await expect(page.getByText("Waiting for Accessibility access.")).toHaveCount(0);
});
