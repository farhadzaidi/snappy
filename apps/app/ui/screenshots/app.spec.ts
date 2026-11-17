import { mkdirSync, writeFileSync } from "node:fs";

import { test, type Page } from "@playwright/test";

import { layoutAction, layoutCard, openApp, showPage, zone } from "../e2e/support";
import { CAPTURES, README_APP_CAPTURES } from "./paths";

type Shot = "layouts" | "editor" | "workspaces" | "settings";

/** The preset the editor shot opens a copy of. */
const EDITED_PRESET = "Main + stack";

async function show(page: Page, shot: Shot): Promise<void> {
  switch (shot) {
    case "layouts":
      await showPage(page, "Layouts");
      break;
    case "editor":
      await showPage(page, "Layouts");
      await layoutCard(page, EDITED_PRESET).click();
      await layoutAction(page, "Duplicate");
      break;
    case "workspaces":
      await showPage(page, "Workspaces");
      break;
    case "settings":
      await showPage(page, "Settings");
      break;
  }
  // The pointer rests where it lights nothing up, except in the editor, where it rests on
  // the selected zone to show its resize handles.
  if (shot === "editor") await zone(page, 1).hover();
  else await page.mouse.move(0, 0);
  // Fonts and the page's own transitions settle before the capture.
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(300);
}

async function capture(page: Page, shot: Shot, file: URL): Promise<void> {
  await show(page, shot);
  mkdirSync(new URL(".", file), { recursive: true });
  writeFileSync(file, await page.screenshot());
}

test.describe("for the README", () => {
  // As the e2e tests see the window, at twice the pixels for sharp text.
  test.use({ viewport: { width: 1080, height: 760 }, deviceScaleFactor: 2 });

  for (const shot of ["layouts", "editor"] as const) {
    test(shot, async ({ page }) => {
      await openApp(page, "macos");
      await capture(page, shot, README_APP_CAPTURES[shot]);
    });
  }
});

const STORE_SHOTS: Shot[] = ["layouts", "editor", "workspaces", "settings"];

test.describe("for the Mac App Store", () => {
  // 2880×1800, one of the 16:10 sizes App Store Connect takes.
  test.use({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });

  for (const shot of STORE_SHOTS) {
    test(shot, async ({ page }) => {
      await openApp(page, "macos", "app-store");
      await capture(page, shot, new URL(`store/mac-app-store/${shot}.png`, CAPTURES));
    });
  }
});

test.describe("for the Microsoft Store", () => {
  // 1920×1080, above the Store's 1366×768 minimum.
  test.use({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });

  for (const shot of STORE_SHOTS) {
    test(shot, async ({ page }) => {
      await openApp(page, "windows");
      await capture(page, shot, new URL(`store/microsoft-store/${shot}.png`, CAPTURES));
    });
  }
});
