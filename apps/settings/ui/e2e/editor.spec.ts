import { expect, test, type Locator, type Page } from "@playwright/test";

import {
  canvas,
  clickCanvas,
  dragOnCanvas,
  lastCommandOf,
  openApp,
  saveCustomCopy,
  showPage,
  toast,
  zone,
  layoutAction,
} from "./support";

const save = (page: Page) => page.getByRole("button", { name: "Save", exact: true });
const tool = (page: Page, name: string) =>
  page.getByRole("toolbar", { name: "Editing tools" }).getByRole("button", { name });

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await showPage(page, "Editor");
});

/** Draw one zone covering the left half of the canvas. */
async function drawLeftHalf(page: Page) {
  await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.499, y: 0.998 });
  await expect(zone(page, 1)).toBeVisible();
}

test("the editor opens on an empty canvas named Untitled layout", async ({ page }) => {
  await expect(page.getByRole("textbox", { name: "Layout name" })).toHaveValue("Untitled layout");
  await expect(page.getByText("Drag to draw your first zone")).toBeVisible();
  await expect(page.getByText("Draw a zone on the canvas.")).toBeVisible();
  await expect(save(page)).toBeDisabled();
});

test("dragging draws a zone that shows its size in pixels", async ({ page }) => {
  await drawLeftHalf(page);
  await expect(zone(page, 1)).toHaveAttribute("data-selected", "true");
  await expect(zone(page, 1).locator(".zone-size")).toHaveText("1512 × 1898");
  await expect(page.getByRole("heading", { name: "Zone 1" })).toBeVisible();
});

test("save enables only after a change", async ({ page }) => {
  await expect(save(page)).toBeDisabled();
  await drawLeftHalf(page);
  await expect(save(page)).toBeEnabled();
});

test("undoing back to the saved content disables save", async ({ page }) => {
  await showPage(page, "Layouts");
  await layoutAction(page, "Shortcuts");
  await expect(save(page)).toBeDisabled();
  await page.getByRole("switch", { name: "Layout active" }).click();
  await expect(save(page)).toBeEnabled();
  await page.keyboard.press("Meta+KeyZ");
  await expect(save(page)).toBeDisabled();
});

test("a vertical cut splits the zone and selects the second part", async ({ page }) => {
  await drawLeftHalf(page);
  await tool(page, "Vertical cut").click();
  await clickCanvas(page, { x: 0.25, y: 0.5 });
  await expect(zone(page, 2)).toBeVisible();
  await expect(zone(page, 2)).toHaveAttribute("data-selected", "true");
  await expect(zone(page, 1).locator(".zone-size")).toHaveText("756 × 1898");
  await expect(zone(page, 2).locator(".zone-size")).toHaveText("756 × 1898");
});

test("a horizontal cut splits across", async ({ page }) => {
  await drawLeftHalf(page);
  await tool(page, "Horizontal cut").click();
  await clickCanvas(page, { x: 0.25, y: 0.5 });
  await expect(zone(page, 2).locator(".zone-size")).toHaveText("1512 × 949");
});

test("undo and redo work on whole edits from the toolbar and keyboard", async ({ page }) => {
  await drawLeftHalf(page);
  await tool(page, "Vertical cut").click();
  await clickCanvas(page, { x: 0.25, y: 0.5 });
  await expect(zone(page, 2)).toBeVisible();

  await page.getByRole("button", { name: "Undo edit" }).click();
  await expect(zone(page, 2)).toHaveCount(0);
  await page.getByRole("button", { name: "Redo edit" }).click();
  await expect(zone(page, 2)).toBeVisible();

  await page.keyboard.press("Meta+KeyZ");
  await page.keyboard.press("Meta+KeyZ");
  await expect(zone(page, 1)).toHaveCount(0);
  await page.keyboard.press("Shift+Meta+KeyZ");
  await expect(zone(page, 1)).toBeVisible();
});

test("dragging a zone's body moves it and stops at the canvas edge", async ({ page }) => {
  await drawLeftHalf(page);
  await dragOnCanvas(page, { x: 0.25, y: 0.5 }, { x: 0.95, y: 0.5 });
  await expect(page.getByRole("textbox", { name: "X pixels" })).toHaveValue("1512");
});

test("dragging a divider moves the seam between zones", async ({ page }) => {
  await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.998, y: 0.998 });
  await tool(page, "Vertical cut").click();
  await clickCanvas(page, { x: 0.5, y: 0.5 });
  await tool(page, "Select").click();
  const divider = canvas(page).getByLabel("Vertical divider");
  const box = await divider.boundingBox();
  const area = await canvas(page).boundingBox();
  if (!box || !area) {
    throw new Error("divider missing");
  }
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(area.x + area.width * 0.75, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect(zone(page, 1).locator(".zone-size")).toHaveText("2268 × 1898");
  await expect(zone(page, 2).locator(".zone-size")).toHaveText("756 × 1898");
});

test("coordinate fields commit on enter and show the result", async ({ page }) => {
  await drawLeftHalf(page);
  const width = page.getByRole("textbox", { name: "Width pixels" });
  await expect(width).toHaveValue("1512");
  await width.fill("1000");
  await width.press("Enter");
  await expect(zone(page, 1).locator(".zone-size")).toHaveText("1000 × 1898");

  await width.fill("2000");
  await width.press("Escape");
  await expect(width).toHaveValue("1000");
});

test("a typed coordinate counts as a change and save commits it", async ({ page }) => {
  await drawLeftHalf(page);
  await save(page).click();
  await expect(save(page)).toBeDisabled();
  const x = page.getByRole("textbox", { name: "X pixels" });
  await x.fill("500");
  await expect(save(page)).toBeEnabled();
  await save(page).click();
  const saved = await lastCommandOf(page, "saveLayout");
  expect(saved?.layout.zones[0]?.rect.x).toBeCloseTo(500 / 3024, 6);
});

test("duplicate and delete zones from the inspector", async ({ page }) => {
  await drawLeftHalf(page);
  await page.getByRole("button", { name: "Duplicate zone" }).click();
  await expect(zone(page, 2).locator(".zone-size")).toHaveText("1512 × 1898");
  await expect(page.getByRole("button", { name: "Duplicate zone" })).toBeDisabled();
  await page.getByRole("button", { name: "Delete zone" }).click();
  await expect(zone(page, 2)).toHaveCount(0);
  await expect(zone(page, 1)).toHaveAttribute("data-selected", "true");
});

test("save stores the layout, keeps the editor open, and browses it", async ({ page }) => {
  await drawLeftHalf(page);
  const name = page.getByRole("textbox", { name: "Layout name" });
  await name.fill("Coding");
  await name.press("Enter");
  await save(page).click();
  await expect(toast(page)).toContainText("Coding saved.");
  await expect(save(page)).toBeDisabled();
  const saved = await lastCommandOf(page, "saveLayout");
  expect(saved?.display).toBe("display-1");
  expect(saved?.layout.name).toBe("Coding");

  await showPage(page, "Layouts");
  await expect(page.getByRole("region", { name: "Layout preview" }).getByRole("heading")).toHaveText(
    "Coding",
  );
  await expect(page.getByRole("region", { name: "Layout preview" }).getByText("In use")).toBeVisible();
});

test("the draft survives page switches", async ({ page }) => {
  await drawLeftHalf(page);
  await showPage(page, "Settings");
  await showPage(page, "Editor");
  await expect(zone(page, 1)).toBeVisible();
});

test("new asks before discarding unsaved changes", async ({ page }) => {
  await drawLeftHalf(page);
  await page.getByRole("button", { name: "New", exact: true }).click();
  const dialog = page.getByRole("alertdialog", { name: "Start a new layout?" });
  await expect(dialog).toContainText("Your unsaved changes will be discarded. Saved layouts will be kept.");
  await expect(dialog.getByRole("button", { name: "Keep editing" })).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(zone(page, 1)).toBeVisible();

  await page.getByRole("button", { name: "New", exact: true }).click();
  await page.getByRole("button", { name: "Discard changes" }).click();
  await expect(zone(page, 1)).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Undo edit" })).toBeDisabled();
});

test("new without changes starts over at once", async ({ page }) => {
  await page.getByRole("button", { name: "New", exact: true }).click();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
});

test("the editor shares the display choice and changing it is not an edit", async ({ page }) => {
  await drawLeftHalf(page);
  await save(page).click();
  await page.locator(".editor-sidebar").getByRole("button", { name: "Display" }).click();
  await page.getByRole("option", { name: "Studio Display" }).click();
  await expect(zone(page, 1).locator(".zone-size")).toHaveText("2560 × 2830");
  await expect(page.locator(".editor-sidebar .display-size")).toHaveText("5120 × 2830 px");
  await expect(save(page)).toBeDisabled();
  await showPage(page, "Layouts");
  await expect(page.getByRole("button", { name: "Display" })).toHaveText("Studio Display");
});

test("the shortcuts tab records zone shortcuts and selects zones", async ({ page }) => {
  await drawLeftHalf(page);
  await page.getByRole("tab", { name: "Shortcuts" }).click();
  const recorder = page.getByRole("button", { name: "Zone 1 shortcut", exact: true });
  await expect(recorder).toHaveText("Record shortcut");
  await recorder.click();
  await page.keyboard.press("Control+Alt+Meta+KeyJ");
  await expect(recorder).toHaveText("Snappy + J");
  await page.getByRole("button", { name: "Clear all" }).click();
  await expect(recorder).toHaveText("Record shortcut");
});

test("a conflict shows in the sidebar and blocks saving", async ({ page }) => {
  await page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      settings: {
        ...state.settings,
        layouts: {
          ...state.settings.layouts,
          custom: [
            { id: "c", name: "Clash", zones: [{ id: "z", rect: { x: 0, y: 0, width: 1, height: 1 } }] },
          ],
          options: {
            ...state.settings.layouts.options,
            c: {
              enabled: false,
              selectShortcut: { code: "ArrowLeft", mods: ["ctrl", "alt", "meta"] },
              zoneShortcuts: {},
            },
          },
        },
      },
    })),
  );
  await showPage(page, "Layouts");
  await page.getByRole("tab", { name: "My layouts" }).click();
  await layoutAction(page, "Edit");
  await page.getByRole("switch", { name: "Layout active" }).click();
  await expect(page.locator(".editor-sidebar").getByRole("alert")).toContainText(
    "Snappy + ← is already assigned to Move window left.",
  );
  await expect(save(page)).toBeDisabled();
});

test("the toolbar offers three tools and history, and opening a layout resets the tool and selection", async ({
  page,
}) => {
  const tools = page.getByRole("toolbar", { name: "Editing tools" });
  await expect(tools.getByRole("button")).toHaveCount(5);
  for (const name of ["Select", "Vertical cut", "Horizontal cut"]) {
    await expect(tool(page, name)).toBeVisible();
  }
  await expect(tools.getByRole("button", { name: "Undo edit" })).toBeVisible();
  await expect(tools.getByRole("button", { name: "Redo edit" })).toBeVisible();
  await expect(tool(page, "Select")).toHaveAttribute("aria-pressed", "true");

  await showPage(page, "Layouts");
  await saveCustomCopy(page, "Halves");
  await clickCanvas(page, { x: 0.75, y: 0.5 });
  await expect(zone(page, 2)).toHaveAttribute("data-selected", "true");
  await tool(page, "Horizontal cut").click();
  await expect(tool(page, "Horizontal cut")).toHaveAttribute("aria-pressed", "true");

  await showPage(page, "Layouts");
  await layoutAction(page, "Edit");
  await expect(tool(page, "Select")).toHaveAttribute("aria-pressed", "true");
  await expect(zone(page, 1)).toHaveAttribute("data-selected", "true");
  await expect(zone(page, 2)).toHaveAttribute("data-selected", "false");
});

const cursorOf = (locator: Locator) => locator.evaluate((element) => getComputedStyle(element).cursor);

test("cursors show resizing, grabbing, and cutting", async ({ page }) => {
  await dragOnCanvas(page, { x: 0.008, y: 0.012 }, { x: 0.998, y: 0.998 });
  await tool(page, "Vertical cut").click();
  await clickCanvas(page, { x: 0.5, y: 0.5 });
  await expect(cursorOf(canvas(page))).resolves.toBe("crosshair");
  await expect(cursorOf(zone(page, 1))).resolves.toBe("crosshair");

  await tool(page, "Select").click();
  await expect(cursorOf(canvas(page).getByLabel("Vertical divider"))).resolves.toBe("col-resize");

  const box = await zone(page, 2).boundingBox();
  if (!box) {
    throw new Error("zone 2 is not visible");
  }
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await expect(cursorOf(zone(page, 2))).resolves.toBe("grab");
  await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2);
  await expect.poll(() => cursorOf(zone(page, 2))).toBe("ew-resize");
  await page.mouse.move(box.x + box.width / 2, box.y + 2);
  await expect.poll(() => cursorOf(zone(page, 2))).toBe("ns-resize");

  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 - 40, box.y + box.height / 2, { steps: 4 });
  await expect.poll(() => cursorOf(zone(page, 2))).toBe("grabbing");
  await page.mouse.up();
});
