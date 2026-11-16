// The editor's selection, keyboard shortcuts, Merge, cuts through the whole layout, snapping,
// and resize handles. The fake display is 3024 × 1898 pixels.

import { expect, test, type Page } from "@playwright/test";

import {
  canvas,
  canvasPoint,
  clickCanvas,
  dragOnCanvas,
  editorTool,
  openApp,
  showPage,
  zone,
} from "./support";

const zones = (page: Page) => canvas(page).locator(".zone");
const sizeOf = (page: Page, number: number) => zone(page, number).locator(".zone-size");
const mergeButton = (page: Page) => page.getByRole("button", { name: "Merge zones" });

async function openEditor(page: Page, platform = "macos") {
  await openApp(page, platform);
  await showPage(page, "Editor");
}

async function drawFullCanvas(page: Page) {
  await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.998, y: 0.998 });
  await expect(zones(page)).toHaveCount(1);
}

/** Point the mouse at a spot on the canvas, given as fractions of its size. */
async function pointAt(page: Page, point: { x: number; y: number }) {
  const { x, y } = await canvasPoint(page, point);
  await page.mouse.move(x, y);
}

async function shiftClick(page: Page, point: { x: number; y: number }) {
  await page.keyboard.down("Shift");
  await clickCanvas(page, point);
  await page.keyboard.up("Shift");
}

/** The canvas cut into quarters: 1 top left, 2 bottom left, 3 top right, 4 bottom right. */
async function quarters(page: Page) {
  await drawFullCanvas(page);
  await editorTool(page, "Vertical cut").click();
  await clickCanvas(page, { x: 0.5, y: 0.5 });
  await editorTool(page, "Horizontal cut").click();
  await shiftClick(page, { x: 0.25, y: 0.5 });
  await expect(zones(page)).toHaveCount(4);
  await editorTool(page, "Select").click();
}

test.describe("on macOS", () => {
  test.beforeEach(async ({ page }) => openEditor(page));

  test("holding Shift previews a cut through every zone and makes it as one step", async ({ page }) => {
    await drawFullCanvas(page);
    await editorTool(page, "Vertical cut").click();
    await clickCanvas(page, { x: 0.5, y: 0.5 });
    await editorTool(page, "Horizontal cut").click();
    await expect(page.locator(".toolbar-hint")).toHaveText("Hold Shift to cut across every zone.");

    await pointAt(page, { x: 0.25, y: 0.4 });
    await expect(canvas(page).locator(".cut-line")).toHaveCount(1);
    await expect(canvas(page).locator(".cut-through")).toHaveCount(0);
    await page.keyboard.down("Shift");
    await expect(canvas(page).locator(".cut-through")).toHaveCount(1);
    await expect(canvas(page).locator(".cut-line")).toHaveCount(2);
    await page.keyboard.up("Shift");
    await expect(canvas(page).locator(".cut-through")).toHaveCount(0);

    await shiftClick(page, { x: 0.25, y: 0.4 });
    await expect(zones(page)).toHaveCount(4);
    await expect(sizeOf(page, 1)).toHaveText("1512 × 760");
    await expect(sizeOf(page, 3)).toHaveText("1512 × 760");
    await editorTool(page, "Undo edit").click();
    await expect(zones(page)).toHaveCount(2);
  });

  test("a cut snaps to a third of the screen and shows the guide", async ({ page }) => {
    await drawFullCanvas(page);
    await editorTool(page, "Vertical cut").click();
    await pointAt(page, { x: 0.336, y: 0.5 });
    const guide = canvas(page).locator('.guide[data-vertical="true"]');
    await expect(guide).toHaveCount(1);
    const [line, area] = await Promise.all([guide.boundingBox(), canvas(page).boundingBox()]);
    expect(Math.abs((line?.x ?? 0) - (area?.x ?? 0) - (area?.width ?? 0) / 3)).toBeLessThan(1);

    await clickCanvas(page, { x: 0.336, y: 0.5 });
    await expect(sizeOf(page, 1)).toHaveText("1008 × 1898");
  });

  test("Shift-click selects several zones and Merge joins them only when they fill a rectangle", async ({
    page,
  }) => {
    await quarters(page);
    await clickCanvas(page, { x: 0.25, y: 0.25 });
    await expect(mergeButton(page)).toBeDisabled();
    await expect(mergeButton(page)).toHaveAttribute("title", "Shift-click another zone to merge them.");
    await expect(page.locator(".toolbar-hint")).toHaveText("Shift-click to select more than one zone.");

    await shiftClick(page, { x: 0.75, y: 0.75 });
    await expect(page.getByRole("heading", { name: "2 zones" })).toBeVisible();
    await expect(zone(page, 1)).toHaveAttribute("data-selected", "true");
    await expect(zone(page, 4)).toHaveAttribute("data-selected", "true");
    await expect(canvas(page).locator(".zone-handle")).toHaveCount(0);
    const refusal = "Zones merge only when together they fill a rectangle with no gaps.";
    await expect(mergeButton(page)).toBeDisabled();
    await expect(mergeButton(page)).toHaveAttribute("title", refusal);
    await expect(page.locator(".editor-sidebar").getByText(refusal)).toBeVisible();

    await shiftClick(page, { x: 0.75, y: 0.75 });
    await shiftClick(page, { x: 0.75, y: 0.25 });
    await expect(mergeButton(page)).toBeEnabled();
    await mergeButton(page).click();
    await expect(zones(page)).toHaveCount(3);
    await expect(sizeOf(page, 1)).toHaveText("3024 × 949");
    await expect(zone(page, 1)).toHaveAttribute("data-selected", "true");

    await clickCanvas(page, { x: 0.25, y: 0.75 });
    await shiftClick(page, { x: 0.75, y: 0.75 });
    await page.keyboard.press("Meta+Shift+KeyM");
    await expect(zones(page)).toHaveCount(2);
    await expect(sizeOf(page, 2)).toHaveText("3024 × 949");

    await page.keyboard.press("Meta+KeyZ");
    await expect(zones(page)).toHaveCount(3);
  });

  test("several zones show on a map of the layout, with every action named", async ({ page }) => {
    await quarters(page);
    await clickCanvas(page, { x: 0.25, y: 0.25 });
    const sidebar = page.locator(".editor-sidebar");
    await expect(sidebar.locator(".selection-summary")).toHaveCount(0);
    await expect(sidebar.locator(".coordinates")).toBeVisible();
    await expect(sidebar.locator(".selection-hint")).toHaveText([
      "Shift-click another zone to merge them.",
      "No free space for the copies.",
    ]);

    await shiftClick(page, { x: 0.75, y: 0.25 });
    const summary = sidebar.locator(".selection-summary");
    await expect(summary).toContainText("Zones 1 and 3");
    await expect(summary.locator('[data-highlighted="true"]')).toHaveCount(2);
    await expect(summary.locator("text")).toHaveText(["1", "2", "3", "4"]);
    await expect(sidebar.locator(".coordinates")).toHaveCount(0);
    await expect(mergeButton(page)).toHaveText("MergeShift ⇧ + Command ⌘ + M");
    await expect(page.getByRole("button", { name: "Delete zones" })).toHaveText("Delete⌫");
    await expect(page.getByRole("button", { name: "Duplicate zones" })).toHaveText("DuplicateCommand ⌘ + D");
    await expect(sidebar.locator(".selection-hint")).toHaveText(["No free space for the copies."]);

    await shiftClick(page, { x: 0.25, y: 0.75 });
    await expect(summary).toContainText("Zones 1, 2, and 3");
  });

  test("Duplicate copies every selected zone, from its button or Command-D", async ({ page }) => {
    await quarters(page);
    await clickCanvas(page, { x: 0.25, y: 0.25 });
    await shiftClick(page, { x: 0.25, y: 0.75 });
    const duplicate = page.getByRole("button", { name: "Duplicate zones" });
    await expect(duplicate).toBeDisabled();
    await expect(duplicate).toHaveAttribute("title", "No free space for the copies.");

    await page.keyboard.press("Meta+KeyD");
    await expect(zones(page)).toHaveCount(4);
    await clickCanvas(page, { x: 0.75, y: 0.25 });
    await shiftClick(page, { x: 0.75, y: 0.75 });
    await page.keyboard.press("Backspace");
    await expect(zones(page)).toHaveCount(2);

    await clickCanvas(page, { x: 0.25, y: 0.25 });
    await shiftClick(page, { x: 0.25, y: 0.75 });
    await expect(duplicate).toBeEnabled();
    await page.keyboard.press("Meta+KeyD");
    await expect(zones(page)).toHaveCount(4);
    await expect(sizeOf(page, 3)).toHaveText("1512 × 949");
    await expect(zone(page, 3)).toHaveAttribute("data-selected", "true");
    await expect(zone(page, 4)).toHaveAttribute("data-selected", "true");
    await page.keyboard.press("Meta+KeyZ");
    await expect(zones(page)).toHaveCount(2);

    await clickCanvas(page, { x: 0.25, y: 0.25 });
    await shiftClick(page, { x: 0.25, y: 0.75 });
    await duplicate.click();
    await expect(zones(page)).toHaveCount(4);
  });

  test("Delete removes every selected zone", async ({ page }) => {
    await quarters(page);
    await clickCanvas(page, { x: 0.25, y: 0.25 });
    await shiftClick(page, { x: 0.75, y: 0.75 });
    await page.keyboard.press("Backspace");
    await expect(zones(page)).toHaveCount(2);
    await page.keyboard.press("Meta+KeyZ");
    await expect(zones(page)).toHaveCount(4);
  });

  test("copy, paste, cut, and delete keys act on the selection while the canvas has focus", async ({
    page,
  }) => {
    await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.499, y: 0.998 });
    await clickCanvas(page, { x: 0.25, y: 0.5 });

    await page.keyboard.press("Meta+KeyC");
    await page.keyboard.press("Meta+KeyV");
    await expect(zones(page)).toHaveCount(2);
    await expect(sizeOf(page, 2)).toHaveText("1512 × 1898");
    await expect(zone(page, 2)).toHaveAttribute("data-selected", "true");
    await expect(page.getByRole("textbox", { name: "X pixels" })).toHaveValue("1512");

    await page.keyboard.press("Meta+KeyX");
    await expect(zones(page)).toHaveCount(1);
    await page.keyboard.press("Meta+KeyV");
    await expect(zones(page)).toHaveCount(2);
    await expect(page.getByRole("textbox", { name: "X pixels" })).toHaveValue("1512");

    await page.keyboard.press("Backspace");
    await expect(zones(page)).toHaveCount(1);
    await expect(zone(page, 1)).toHaveAttribute("data-selected", "true");
    await page.keyboard.press("Delete");
    await expect(zones(page)).toHaveCount(0);
    await page.keyboard.press("Meta+KeyZ");
    await expect(zones(page)).toHaveCount(1);
  });

  test("the canvas rings its focus when the keyboard reaches it, not after a click", async ({ page }) => {
    const surface = canvas(page);
    const outline = () => surface.evaluate((element) => getComputedStyle(element).outlineWidth);
    const unfocused = await outline();

    await clickCanvas(page, { x: 0.5, y: 0.5 });
    await expect(surface).toBeFocused();
    expect(await outline()).toBe(unfocused);

    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Tab");
    await expect(surface).toBeFocused();
    expect(await outline()).not.toBe(unfocused);
  });

  test("the shortcuts leave text fields and other controls alone", async ({ page }) => {
    await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.499, y: 0.998 });
    await clickCanvas(page, { x: 0.25, y: 0.5 });
    await page.keyboard.press("Meta+KeyC");

    const width = page.getByRole("textbox", { name: "Width pixels" });
    await width.click();
    await page.keyboard.press("Backspace");
    await page.keyboard.press("Meta+KeyV");
    await page.keyboard.press("Meta+KeyX");
    await expect(zones(page)).toHaveCount(1);
    await width.press("Escape");

    const name = page.getByRole("textbox", { name: "Layout name" });
    await name.click();
    await page.keyboard.press("Backspace");
    await expect(name).toHaveValue("Untitled layou");
    await expect(zones(page)).toHaveCount(1);

    // WebKit doesn't focus a clicked button, so focus it as the keyboard would; and Backspace
    // outside a text field goes back a page there, so press Delete, which deletes zones too.
    await editorTool(page, "Select").focus();
    await page.keyboard.press("Delete");
    await page.keyboard.press("Meta+KeyV");
    await expect(zones(page)).toHaveCount(1);
  });

  test("the selected zone's handles are round dots set in from its edges", async ({ page }) => {
    await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.499, y: 0.998 });
    await pointAt(page, { x: 0.25, y: 0.5 });
    const handles = zone(page, 1).locator("[data-handle]");
    await expect(handles).toHaveCount(8);
    await expect(handles.first()).toHaveCSS("opacity", "1");

    const box = await zone(page, 1).boundingBox();
    if (!box) {
      throw new Error("the zone is not visible");
    }
    for (const handle of await handles.all()) {
      const dot = await handle.boundingBox();
      if (!dot) {
        throw new Error("a handle is not visible");
      }
      expect(dot.width).toBeCloseTo(dot.height);
      // Wholly inside the zone, clear of its border.
      expect(dot.x).toBeGreaterThan(box.x + 2);
      expect(dot.y).toBeGreaterThan(box.y + 2);
      expect(dot.x + dot.width).toBeLessThan(box.x + box.width - 2);
      expect(dot.y + dot.height).toBeLessThan(box.y + box.height - 2);
    }
  });
});

test("on Windows, Control runs the clipboard and Merge keys", async ({ page }) => {
  await openEditor(page, "windows");
  await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.499, y: 0.998 });
  await clickCanvas(page, { x: 0.25, y: 0.5 });
  await page.keyboard.press("Meta+KeyC");
  await page.keyboard.press("Meta+KeyV");
  await expect(zones(page)).toHaveCount(1);

  await page.keyboard.press("Control+KeyC");
  await page.keyboard.press("Control+KeyV");
  await expect(zones(page)).toHaveCount(2);
  await shiftClick(page, { x: 0.25, y: 0.5 });
  await page.keyboard.press("Control+Shift+KeyM");
  await expect(zones(page)).toHaveCount(1);
  await expect(sizeOf(page, 1)).toHaveText("3024 × 1898");
});
