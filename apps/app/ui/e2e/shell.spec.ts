import { expect, test, type Page } from "@playwright/test";

import { FIRST_LOAD_TIMEOUT_MS, lastCommandOf, openApp, widthsOf } from "./support";

const reason = "Log out and back in to finish setting up Snappy's GNOME Shell extension.";

const setUnavailableReason = (page: Page, unavailableReason: string | null) =>
  page.evaluate(
    (unavailableReason) =>
      window.snappyFake.patchState((state) => ({
        ...state,
        snapping: { ...state.snapping, live: false, unavailableReason },
      })),
    unavailableReason,
  );

test("the page tabs put the editor beside the layouts", async ({ page }) => {
  await openApp(page);
  const tabs = page.getByRole("navigation", { name: "Pages" }).getByRole("button");
  await expect(tabs).toHaveText(["Layouts", "Editor", "Workspaces", "Settings"]);
});

test("the window shows once the UI has drawn, and takes the first click", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(() => typeof window.snappyWindow?.shownText === "string", null, {
    timeout: FIRST_LOAD_TIMEOUT_MS,
  });
  const shownText = await page.evaluate(() => window.snappyWindow.shownText);
  for (const tab of ["Layouts", "Editor", "Workspaces", "Settings"]) {
    expect(shownText).toContain(tab);
  }

  await page.getByRole("navigation", { name: "Pages" }).getByRole("button", { name: "Settings" }).click();
  await expect(page.locator(".page")).toHaveAttribute("data-page", "settings");
});

test("why snapping can't run shows under the header until it can", async ({ page }) => {
  await openApp(page, "linux");
  await expect(page.locator(".shell-notice")).toHaveCount(0);

  await setUnavailableReason(page, reason);
  await expect(page.getByRole("status").filter({ hasText: reason })).toBeVisible();
  await expect(page.locator(".snapping-button")).toBeDisabled();

  await setUnavailableReason(page, null);
  await expect(page.locator(".shell-notice")).toHaveCount(0);
  await expect(page.locator(".snapping-button")).toBeEnabled();
});

test("a fix Snappy can do itself is a button, and clicking it asks the daemon", async ({ page }) => {
  await openApp(page, "linux");
  await page.evaluate(() =>
    window.snappyFake.patchState((state) => ({
      ...state,
      snapping: {
        ...state.snapping,
        live: false,
        unavailableReason: "One last step: log out of Ubuntu and sign back in to finish setting up Snappy.",
        unavailableRemedy: "logOut",
      },
    })),
  );
  await page.getByRole("button", { name: "Log out now" }).click();
  expect(await lastCommandOf(page, "applyRemedy")).toEqual({ kind: "applyRemedy", remedy: "logOut" });
});

test("notes about the desktop show under the header", async ({ page }) => {
  await openApp(page, "linux");
  const note = "Ubuntu's Enhanced Tiling also snaps windows you drag to screen edges.";
  await page.evaluate(
    (note) => window.snappyFake.patchState((state) => ({ ...state, platformNotes: [note] })),
    note,
  );
  await expect(page.getByRole("note").filter({ hasText: note })).toBeVisible();
  await expect(page.getByRole("button", { name: "Log out now" })).toHaveCount(0);
});

test("the page tabs are all one width", async ({ page }) => {
  await openApp(page);
  const widths = await widthsOf(page.getByRole("navigation", { name: "Pages" }).getByRole("button"));
  expect(widths).toHaveLength(4);
  expect(new Set(widths).size).toBe(1);
});
