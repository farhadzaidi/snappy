import { expect, test, type Page } from "@playwright/test";

import {
  lastCommandOf,
  layoutAction,
  openApp,
  saveCustomCopy,
  sentCommands,
  showPage,
  toast,
} from "./support";

test.beforeEach(async ({ page }) => openApp(page));

const actions = (page: Page) => page.getByRole("toolbar", { name: "Layout actions" });
const preview = (page: Page) => page.getByRole("region", { name: "Layout preview" });

test("the page shows the title and the display chooser with its usable size, and no subtitle", async ({
  page,
}) => {
  await expect(page.getByRole("button", { name: "Display" })).toHaveText("Built-in Display");
  await expect(page.locator(".layouts-page .display-size")).toHaveText("3024 × 1898 px");
  await expect(page.locator(".layouts-page .page-subtitle")).toHaveCount(0);

  await page.getByRole("button", { name: "Display" }).click();
  await page.getByRole("option", { name: "Studio Display" }).click();
  await expect(page.locator(".layouts-page .display-size")).toHaveText("5120 × 2830 px");
});

test("tabs switch collections and browse the display's layout", async ({ page }) => {
  await expect(page.getByRole("tab", { name: "Presets" })).toHaveAttribute("aria-selected", "true");
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
  await page.getByRole("tab", { name: "My layouts" }).click();
  await expect(page.getByRole("tab", { name: "My layouts" })).toHaveAttribute("aria-selected", "true");
  await page.getByRole("tab", { name: "Presets" }).click();
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
  expect(await lastCommandOf(page, "applyLayout")).toBeUndefined();
});

test("an empty My layouts collection offers to create one", async ({ page }) => {
  await page.getByRole("tab", { name: "My layouts" }).click();
  await expect(page.getByText("No custom layouts yet")).toBeVisible();
  await expect(preview(page)).toHaveCount(0);
  await page.getByRole("button", { name: "Create layout" }).click();
  await expect(page.getByRole("textbox", { name: "Layout name" })).toHaveValue("Untitled layout");
});

test("clicking a card browses it without applying it", async ({ page }) => {
  await page.getByRole("button", { name: "Halves", exact: true }).click();
  await expect(page.getByRole("button", { name: "Halves", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(preview(page).getByRole("heading")).toHaveText("Halves");
  expect(await lastCommandOf(page, "applyLayout")).toBeUndefined();
});

test("the preview and cards show which layout is in use", async ({ page }) => {
  await expect(preview(page).getByText("In use")).toBeVisible();
  await expect(preview(page).getByLabel("Zone 1", { exact: true })).toHaveText("1");
  await expect(preview(page).getByLabel("Zone 3", { exact: true })).toHaveText("3");
  await expect(
    page.getByRole("button", { name: "Center stage", exact: true }).getByTitle("In use on this display"),
  ).toBeVisible();
  await expect(actions(page).getByRole("button", { name: "Use layout" })).toHaveCount(0);
});

test("use layout applies it to the selected display", async ({ page }) => {
  await page.getByRole("button", { name: "Two rows", exact: true }).click();
  await actions(page).getByRole("button", { name: "Use layout" }).click();
  expect(await lastCommandOf(page, "applyLayout")).toEqual({
    kind: "applyLayout",
    display: "display-1",
    layout: "rows",
  });
  await expect(preview(page).getByText("In use")).toBeVisible();
  await expect(actions(page).getByRole("button", { name: "Use layout" })).toHaveCount(0);
  await expect(toast(page)).toHaveCount(0);
});

test("show on screen toggles to hide zones and back", async ({ page }) => {
  await actions(page).getByRole("button", { name: "Show on screen" }).click();
  expect(await lastCommandOf(page, "showPreview")).toEqual({
    kind: "showPreview",
    display: "display-1",
    layout: "focus",
  });
  const hide = actions(page).getByRole("button", { name: "Hide zones" });
  await expect(hide).toHaveAttribute("aria-pressed", "true");
  await hide.click();
  await expect(actions(page).getByRole("button", { name: "Show on screen" })).toBeVisible();
  expect((await sentCommands(page)).at(-1)).toEqual({ kind: "hidePreview" });
});

test("escape, switching pages, and changing the display hide the preview", async ({ page }) => {
  const show = actions(page).getByRole("button", { name: "Show on screen" });
  const hide = actions(page).getByRole("button", { name: "Hide zones" });

  await show.click();
  await expect(hide).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(show).toBeVisible();

  await show.click();
  await expect(hide).toBeVisible();
  await showPage(page, "Settings");
  await showPage(page, "Layouts");
  await expect(show).toBeVisible();

  await show.click();
  await expect(hide).toBeVisible();
  await page.getByRole("button", { name: "Display" }).click();
  await page.getByRole("option", { name: "Studio Display" }).click();
  await expect(show).toBeVisible();
  const hides = (await sentCommands(page)).filter((command) => command.kind === "hidePreview");
  expect(hides).toHaveLength(3);
});

test("the action bar disables and enables a layout", async ({ page }) => {
  await page.getByRole("button", { name: "Two rows", exact: true }).click();
  await layoutAction(page, "Disable");
  await expect(toast(page)).toContainText("Two rows disabled.");
  await expect(preview(page).getByText("Inactive")).toBeVisible();
  await expect(page.getByRole("button", { name: "Two rows", exact: true })).toHaveAttribute(
    "data-inactive",
    "true",
  );

  await actions(page).locator(".action-primary").click();
  await expect(toast(page)).toContainText("Two rows enabled.");
  await expect(actions(page).getByRole("button", { name: "Use layout" })).toBeVisible();
});

test("duplicate opens an unsaved copy in the editor", async ({ page }) => {
  await page.getByRole("button", { name: "Four corners", exact: true }).click();
  await layoutAction(page, "Duplicate");
  await expect(page.getByRole("textbox", { name: "Layout name" })).toHaveValue("Four corners copy");
  await expect(page.getByRole("button", { name: "Save", exact: true })).toBeEnabled();
});

test("presets open read-only with their shortcuts", async ({ page }) => {
  await layoutAction(page, "Shortcuts");
  await expect(page.getByRole("heading", { name: "Center stage" })).toBeVisible();
  await expect(page.getByTitle("Preset shapes cannot be edited. Duplicate to customize.")).toBeVisible();
  await expect(page.getByRole("toolbar", { name: "Editing tools" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Zone 1 shortcut", exact: true })).toHaveText("Snappy + 1");
  await expect(page.getByRole("button", { name: "Duplicate", exact: true })).toBeVisible();
});

test("deleting a custom layout asks first", async ({ page }) => {
  await saveCustomCopy(page, "Halves");
  await showPage(page, "Layouts");
  await expect(page.getByRole("tab", { name: "My layouts" })).toHaveAttribute("aria-selected", "true");
  await expect(preview(page).getByRole("heading")).toHaveText("Halves copy");

  await layoutAction(page, "Delete");
  const confirm = page.getByRole("dialog", { name: "Delete layout" });
  await expect(confirm.getByText("Delete Halves copy?")).toBeVisible();
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(confirm).toHaveCount(0);

  await layoutAction(page, "Delete");
  await page
    .getByRole("dialog", { name: "Delete layout" })
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await expect(toast(page)).toContainText("Custom layout deleted.");
  await expect(page.getByText("No custom layouts yet")).toBeVisible();
});

test("the action bar stays on one row and moves what doesn't fit into a menu", async ({ page }) => {
  const bar = actions(page);
  const more = bar.getByRole("button", { name: "More actions" });
  await saveCustomCopy(page, "Halves");
  await showPage(page, "Layouts");

  await page.setViewportSize({ width: 1700, height: 760 });
  await expect(more).toHaveCount(0);
  for (const name of ["Show on screen", "Edit", "Duplicate", "Disable", "Delete"]) {
    await expect(bar.getByRole("button", { name, exact: true })).toBeVisible();
  }
  await expect(bar.locator(".action-divider")).toHaveCount(1);

  await page.setViewportSize({ width: 760, height: 760 });
  await expect(more).toBeVisible();
  const rowTops = await bar
    .locator(":scope > *")
    .evaluateAll((items) => items.map((item) => Math.round(item.getBoundingClientRect().top)));
  expect(new Set(rowTops).size).toBe(1);
  await expect(bar.locator(".action-divider")).toHaveCount(0);
  const heights = await bar
    .getByRole("button")
    .evaluateAll((buttons) => buttons.map((button) => Math.round(button.getBoundingClientRect().height)));
  expect(new Set(heights)).toEqual(new Set([32]));

  // The actions in line, then those in the menu, keep their order; the last move first.
  const inline = (await bar.locator(".tool-button").allTextContents()).map((text) => text.trim());
  await more.click();
  const menu = page.getByRole("menu", { name: "More actions" });
  const overflow = (await menu.getByRole("menuitem").allTextContents()).map((text) => text.trim());
  expect(overflow.length).toBeGreaterThan(0);
  expect([...inline, ...overflow]).toEqual(["Show on screen", "Edit", "Duplicate", "Disable", "Delete"]);
  await menu.getByRole("menuitem", { name: "Delete" }).click();
  await expect(menu).toHaveCount(0);
  await expect(page.getByRole("dialog", { name: "Delete layout" })).toContainText("Delete Halves copy?");
});

test("new layout opens an empty editor", async ({ page }) => {
  await page.getByRole("button", { name: "New layout" }).click();
  await expect(page.getByText("Drag to draw your first zone")).toBeVisible();
});
