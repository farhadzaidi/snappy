import { expect, type Locator, type Page } from "@playwright/test";

import type { FakeDaemon } from "../src/bridge/fake/fakeDaemon";
import type { Command, KeyChord } from "../src/core/model";

declare global {
  interface Window {
    snappyFake: FakeDaemon;
  }
}

/** Open the settings app against a fresh fake daemon. */
export async function openApp(page: Page, platform = "macos"): Promise<void> {
  await page.goto(`/?platform=${platform}`);
  await expect(page.getByRole("heading", { name: "Layouts", exact: true })).toBeVisible();
}

export async function showPage(
  page: Page,
  title: "Layouts" | "Workspaces" | "Editor" | "Settings",
): Promise<void> {
  await page
    .getByRole("navigation", { name: "Pages" })
    .getByRole("button", { name: title, exact: true })
    .click();
}

/** Every command the UI has sent to the fake daemon. */
export function sentCommands(page: Page): Promise<Command[]> {
  return page.evaluate(() => window.snappyFake.commands);
}

export async function lastCommandOf<K extends Command["kind"]>(
  page: Page,
  kind: K,
): Promise<Extract<Command, { kind: K }> | undefined> {
  const commands = await sentCommands(page);
  return commands.filter((command): command is Extract<Command, { kind: K }> => command.kind === kind).at(-1);
}

/** Make the availability probe refuse a chord, as if another app owned it. */
export function takeShortcut(page: Page, chord: KeyChord): Promise<void> {
  return page.evaluate((taken) => window.snappyFake.takeShortcut(taken), chord);
}

export function toast(page: Page): Locator {
  return page.locator(".toast");
}

export function canvas(page: Page): Locator {
  return page.getByRole("application", { name: "Layout canvas" });
}

/** Drag across the canvas between two points given as fractions of its size. */
export async function dragOnCanvas(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
): Promise<void> {
  const box = await canvas(page).boundingBox();
  if (!box) {
    throw new Error("the canvas is not visible");
  }
  const at = (point: { x: number; y: number }) => ({
    x: box.x + point.x * box.width,
    y: box.y + point.y * box.height,
  });
  const start = at(from);
  const end = at(to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move((start.x + end.x) / 2, (start.y + end.y) / 2, { steps: 4 });
  await page.mouse.move(end.x, end.y, { steps: 4 });
  await page.mouse.up();
}

/** Click a point on the canvas given as fractions of its size. */
export async function clickCanvas(page: Page, point: { x: number; y: number }): Promise<void> {
  const box = await canvas(page).boundingBox();
  if (!box) {
    throw new Error("the canvas is not visible");
  }
  await page.mouse.move(box.x + point.x * box.width, box.y + point.y * box.height);
  await page.mouse.down();
  await page.mouse.up();
}

export function zone(page: Page, number: number): Locator {
  return canvas(page).getByLabel(`Zone ${number}`, { exact: true });
}

/** Duplicate a preset and save it as a custom layout named "<preset> copy". */
export async function saveCustomCopy(page: Page, preset: string): Promise<void> {
  await page.getByRole("button", { name: preset, exact: true }).click();
  await page
    .getByRole("toolbar", { name: "Layout actions" })
    .getByRole("button", { name: "Duplicate" })
    .click();
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(toast(page)).toContainText(`${preset} copy saved.`);
}
