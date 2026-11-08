import { expect, test, type Page } from "@playwright/test";

import { lastCommandOf, openApp } from "./support";

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
