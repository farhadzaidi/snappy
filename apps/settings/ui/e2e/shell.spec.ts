import { expect, test, type Page } from "@playwright/test";

import { openApp } from "./support";

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
  await expect(page.locator(".unavailable-notice")).toHaveCount(0);

  await setUnavailableReason(page, reason);
  await expect(page.getByRole("status").filter({ hasText: reason })).toBeVisible();
  await expect(page.locator(".snapping-button")).toBeDisabled();

  await setUnavailableReason(page, null);
  await expect(page.locator(".unavailable-notice")).toHaveCount(0);
  await expect(page.locator(".snapping-button")).toBeEnabled();
});
