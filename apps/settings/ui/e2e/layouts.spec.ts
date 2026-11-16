import { expect, test, type Page } from "@playwright/test";

import {
  edgesOf,
  lastCommandOf,
  layoutAction,
  layoutCard,
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
  await expect(page.getByRole("button", { name: "Display", exact: true })).toHaveText("Built-in Display");
  await expect(page.locator(".layouts-page .display-size")).toHaveText("3024 × 1898 px");
  await expect(page.locator(".layouts-page .page-subtitle")).toHaveCount(0);

  await page.getByRole("button", { name: "Display", exact: true }).click();
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

test("the arrow keys move between collection tabs, which control the library", async ({ page }) => {
  const presets = page.getByRole("tab", { name: "Presets" });
  await presets.focus();
  await page.keyboard.press("ArrowRight");
  const custom = page.getByRole("tab", { name: "My layouts" });
  await expect(custom).toBeFocused();
  await expect(custom).toHaveAttribute("aria-selected", "true");
  await expect(presets).toHaveAttribute("tabindex", "-1");
  const panel = page.getByRole("tabpanel", { name: "My layouts" });
  await expect(panel).toContainText("No custom layouts yet");
  await expect(custom).toHaveAttribute("aria-controls", (await panel.getAttribute("id")) ?? "");
  await page.keyboard.press("ArrowRight");
  await expect(presets).toBeFocused();
});

test("an empty My layouts collection offers to create one", async ({ page }) => {
  await page.getByRole("tab", { name: "My layouts" }).click();
  await expect(page.getByText("No custom layouts yet")).toBeVisible();
  await expect(preview(page)).toHaveCount(0);
  await page.getByRole("button", { name: "Create layout" }).click();
  await expect(page.getByRole("textbox", { name: "Layout name" })).toHaveValue("Untitled layout");
});

test("clicking a card browses it without applying it", async ({ page }) => {
  await layoutCard(page, "Halves").click();
  await expect(layoutCard(page, "Halves")).toHaveAttribute("aria-pressed", "true");
  await expect(preview(page).getByRole("heading")).toHaveText("Halves");
  expect(await lastCommandOf(page, "applyLayout")).toBeUndefined();
});

test("the preview and cards show which layout is in use", async ({ page }) => {
  await expect(preview(page).getByText("In use")).toBeVisible();
  await expect(preview(page).getByLabel("Zone 1", { exact: true })).toHaveText("1");
  await expect(preview(page).getByLabel("Zone 3", { exact: true })).toHaveText("3");
  await expect(layoutCard(page, "Center stage")).toHaveAccessibleName("Center stage, in use on this display");
  await expect(layoutCard(page, "Two rows")).toHaveAccessibleName("Two rows");
  await expect(preview(page).getByRole("button", { name: "Use layout" })).toHaveCount(0);
});

test("use layout, beside the name, applies it to the selected display", async ({ page }) => {
  await layoutCard(page, "Two rows").click();
  await preview(page).getByRole("button", { name: "Use layout" }).click();
  expect(await lastCommandOf(page, "applyLayout")).toEqual({
    kind: "applyLayout",
    display: "display-1",
    layout: "rows",
  });
  await expect(preview(page).getByText("In use")).toBeVisible();
  await expect(preview(page).getByRole("button", { name: "Use layout" })).toHaveCount(0);
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

test("hide zones hides them on its click though the press already hid them", async ({ page }) => {
  // The daemon hides the preview at every press, before the click that follows it.
  await page.evaluate(() =>
    document.addEventListener(
      "pointerdown",
      () =>
        window.snappyFake.patchState((state) => ({
          ...state,
          snapping: { ...state.snapping, previewVisible: false },
        })),
      { capture: true },
    ),
  );
  await actions(page).getByRole("button", { name: "Show on screen" }).click({ delay: 80 });
  const hide = actions(page).getByRole("button", { name: "Hide zones" });
  await expect(hide).toBeVisible();

  const sentBefore = (await sentCommands(page)).length;
  await hide.click({ delay: 80 });
  await expect(actions(page).getByRole("button", { name: "Show on screen" })).toBeVisible();
  expect((await sentCommands(page)).slice(sentBefore)).toEqual([{ kind: "hidePreview" }]);
});

test("show on screen from the keyboard follows what the button shows", async ({ page }) => {
  const show = actions(page).getByRole("button", { name: "Show on screen" });
  await show.focus();
  await page.keyboard.press("Enter");
  await expect(actions(page).getByRole("button", { name: "Hide zones" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(show).toBeVisible();
  const kinds = (await sentCommands(page)).map((command) => command.kind);
  expect(kinds.slice(-2)).toEqual(["showPreview", "hidePreview"]);
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
  await page.getByRole("button", { name: "Display", exact: true }).click();
  await page.getByRole("option", { name: "Studio Display" }).click();
  await expect(show).toBeVisible();
  const hides = (await sentCommands(page)).filter((command) => command.kind === "hidePreview");
  expect(hides).toHaveLength(3);
});

test("the action bar disables and enables a layout from the same place", async ({ page }) => {
  await layoutCard(page, "Two rows").click();
  const first = actions(page).getByRole("button").first();
  // The button's fill with the pointer away from it.
  const fill = async () => {
    await page.mouse.move(0, 0);
    return first.evaluate((button) => getComputedStyle(button).backgroundColor);
  };
  await expect(first).toHaveText("Disable");
  const quiet = await fill();
  await first.click();
  await expect(toast(page)).toContainText("Two rows disabled.");
  await expect(preview(page).getByText("Disabled")).toBeVisible();
  await expect(preview(page).getByRole("button", { name: "Use layout" })).toHaveCount(0);
  await expect(layoutCard(page, "Two rows")).toHaveAccessibleName("Two rows, disabled");

  await expect(first).toHaveText("Enable");
  // Enable stands out from the other actions until the layout is on again.
  await expect.poll(fill).not.toBe(quiet);
  await expect(actions(page).getByRole("button", { name: "Enable" })).toHaveCount(1);
  await first.click();
  await expect(toast(page)).toContainText("Two rows enabled.");
  await expect(first).toHaveText("Disable");
  await expect.poll(fill).toBe(quiet);
  await expect(preview(page).getByRole("button", { name: "Use layout" })).toBeVisible();
});

test("disabling the layout in use keeps browsing it rather than the display's next layout", async ({
  page,
}) => {
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
  await layoutAction(page, "Disable");
  await expect(toast(page)).toContainText("Center stage disabled.");
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
  await expect(preview(page).getByText("Disabled")).toBeVisible();
  await expect(layoutCard(page, "Center stage")).toHaveAttribute("aria-pressed", "true");

  await layoutAction(page, "Enable");
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
});

test("the browsed layout stays when the display's layout changes elsewhere", async ({ page }) => {
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
  await page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      settings: {
        ...state.settings,
        layouts: { ...state.settings.layouts, displays: { "display-1": "halves" } },
      },
    })),
  );
  await expect(layoutCard(page, "Halves")).toHaveAccessibleName("Halves, in use on this display");
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
});

test("choosing another display browses its layout", async ({ page }) => {
  await page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      settings: {
        ...state.settings,
        layouts: { ...state.settings.layouts, displays: { "display-2": "halves" } },
      },
    })),
  );
  await expect(preview(page).getByRole("heading")).toHaveText("Center stage");
  await page.getByRole("button", { name: "Display", exact: true }).click();
  await page.getByRole("option", { name: "Studio Display" }).click();
  await expect(preview(page).getByRole("heading")).toHaveText("Halves");
});

test("duplicate opens an unsaved copy in the editor", async ({ page }) => {
  await layoutCard(page, "Four corners").click();
  await layoutAction(page, "Duplicate");
  await expect(page.getByRole("textbox", { name: "Layout name" })).toHaveValue("Four corners copy");
  await expect(page.getByRole("button", { name: "Save", exact: true })).toBeEnabled();
});

test("presets open read-only with their shortcuts", async ({ page }) => {
  await layoutAction(page, "Shortcuts");
  await expect(page.getByRole("heading", { name: "Center stage" })).toBeVisible();
  await expect(page.getByTitle("Duplicate a preset to change its zones.")).toBeVisible();
  await expect(page.getByRole("toolbar", { name: "Editing tools" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Zone 1 of this layout", exact: true })).toHaveText(
    "Record shortcut",
  );
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
  await expect(actions(page).getByRole("button", { name: "Delete", exact: true })).toBeFocused();

  await layoutAction(page, "Delete");
  await page
    .getByRole("dialog", { name: "Delete layout" })
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await expect(toast(page)).toContainText("Custom layout deleted.");
  await expect(page.getByText("No custom layouts yet")).toBeVisible();
});

test("the action bar keeps one order, and a narrow bar shows icons instead of hiding actions", async ({
  page,
}) => {
  const bar = actions(page);
  await saveCustomCopy(page, "Halves");
  await showPage(page, "Layouts");
  const order = ["Disable", "Show on screen", "Edit", "Duplicate", "Delete"];
  const names = () =>
    bar
      .getByRole("button")
      .evaluateAll((buttons) =>
        buttons.map((button) => button.getAttribute("aria-label") ?? button.textContent),
      );

  await page.setViewportSize({ width: 1700, height: 760 });
  await expect.poll(names).toEqual(order);
  await expect(bar.locator(".tool-button-compact")).toHaveCount(0);
  await expect(bar.locator(".action-divider")).toHaveCount(1);
  await expect(bar.getByRole("button", { name: "More actions" })).toHaveCount(0);

  await page.setViewportSize({ width: 760, height: 760 });
  await expect(bar.locator(".tool-button-compact")).toHaveCount(4);
  await expect.poll(names).toEqual(order);
  await expect(bar.getByRole("button", { name: "Disable" })).toHaveText("Disable");
  await expect(bar.getByRole("button", { name: "Duplicate" })).toHaveAttribute("title", "Duplicate");
  const rowCenters = await bar
    .locator(":scope > *")
    .evaluateAll((items) =>
      items.map((item) =>
        Math.round(item.getBoundingClientRect().top + item.getBoundingClientRect().height / 2),
      ),
    );
  expect(new Set(rowCenters).size).toBe(1);
  const heights = await bar
    .getByRole("button")
    .evaluateAll((buttons) => buttons.map((button) => Math.round(button.getBoundingClientRect().height)));
  expect(new Set(heights)).toEqual(new Set([32]));

  await bar.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByRole("dialog", { name: "Delete layout" })).toContainText("Delete Halves copy?");
});

test("new layout opens an empty editor", async ({ page }) => {
  await page.getByRole("button", { name: "New layout" }).click();
  await expect(page.getByText("Drag to draw your first zone")).toBeVisible();
});

test("the heading and the actions share a panel as wide as the zones, lined up inside it", async ({
  page,
}) => {
  await layoutCard(page, "Two rows").click();
  const canvas = await edgesOf(preview(page).locator(".preview-canvas"));
  expect(await edgesOf(preview(page).locator(".preview-details"))).toEqual(canvas);

  const name = await edgesOf(preview(page).locator(".preview-name"));
  const use = await edgesOf(preview(page).getByRole("button", { name: "Use layout" }));
  const buttons = actions(page).getByRole("button");
  const firstIcon = await edgesOf(buttons.first().locator("svg"));
  const lastLabel = await edgesOf(buttons.last().locator("span"));
  expect(name.left).toBe(firstIcon.left);
  expect(use.right).toBe(lastLabel.right);
  await expect(preview(page).locator(".preview-status")).toHaveCount(0);
});
