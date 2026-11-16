import { expect, type Locator, type Page } from "@playwright/test";

import type { FakeDaemon } from "../src/bridge/fake/fakeDaemon";
import type { Command, KeyChord } from "../src/core/model";

declare global {
  interface Window {
    snappyFake: FakeDaemon;
  }
}

/** The rounded widths of every element `scope` matches, in order. */
export function widthsOf(scope: Locator): Promise<number[]> {
  return scope.evaluateAll((elements) =>
    elements.map((element) => Math.round(element.getBoundingClientRect().width)),
  );
}

/** Where an element's left and right edges are, rounded. */
export async function edgesOf(element: Locator): Promise<{ left: number; right: number }> {
  return element.evaluate((node) => {
    const box = node.getBoundingClientRect();
    return { left: Math.round(box.left), right: Math.round(box.right) };
  });
}

/**
 * Expect the `count` recorders in `scope` to be one width and line up at both edges, and say
 * whether any shortcut among them is cut short. Gives their width.
 */
export async function expectRecordersLinedUp(
  scope: Locator,
  count: number,
  { cutShort = false } = {},
): Promise<number> {
  const recorders = await scope.locator(".recorder-button").evaluateAll((buttons) =>
    buttons.map((button) => {
      const box = button.getBoundingClientRect();
      const overflows = (node: Element) => node.scrollWidth > node.clientWidth;
      return {
        left: Math.round(box.left),
        right: Math.round(box.right),
        cutShort: [button, ...button.querySelectorAll(".chord, .chord-keys")].some(overflows),
      };
    }),
  );
  expect(recorders).toHaveLength(count);
  expect(new Set(recorders.map((recorder) => recorder.left)).size).toBe(1);
  expect(new Set(recorders.map((recorder) => recorder.right)).size).toBe(1);
  expect(recorders.some((recorder) => recorder.cutShort)).toBe(cutShort);
  return recorders[0].right - recorders[0].left;
}

/** Open the settings app against a fresh fake daemon. */
export async function openApp(page: Page, platform = "macos"): Promise<void> {
  await page.goto(`/?platform=${platform}`);
  await expect(page.getByRole("heading", { name: "Layouts", exact: true })).toBeVisible();
}

export async function showPage(
  page: Page,
  title: "Layouts" | "Editor" | "Workspaces" | "Settings",
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

/** The shortcut a recorder, card, or row shows. Its accessible name is its long label, "Snappy + ←". */
export function chordIn(scope: Locator): Locator {
  return scope.locator(".chord");
}

/** The long labels of every shortcut `scope` shows, in order. */
export function chordsIn(scope: Locator): Promise<string[]> {
  return chordIn(scope).evaluateAll((chords) =>
    chords.map((chord) => chord.getAttribute("aria-label") ?? ""),
  );
}

/**
 * The element's text as assistive technology reads it: the Snappy key's glyph and every
 * shortcut drawn in symbols read as their labels, so "Snappy + J is taken." reads as written.
 */
export function spokenText(element: Locator): Promise<string> {
  return element.evaluate((root) => {
    const copy = root.cloneNode(true) as Element;
    copy.querySelectorAll("[role=img][aria-label]").forEach((image) => {
      image.replaceWith(image.getAttribute("aria-label") ?? "");
    });
    return copy.textContent ?? "";
  });
}

export function canvas(page: Page): Locator {
  return page.getByRole("application", { name: "Layout canvas" });
}

/**
 * Where a point given as fractions of the canvas's size is on the page, in whole pixels:
 * WebKit moves the mouse only by whole pixels, so both browsers then press at the same spot.
 */
export async function canvasPoint(
  page: Page,
  point: { x: number; y: number },
): Promise<{ x: number; y: number }> {
  const box = await canvas(page).boundingBox();
  if (!box) {
    throw new Error("the canvas is not visible");
  }
  return { x: Math.round(box.x + point.x * box.width), y: Math.round(box.y + point.y * box.height) };
}

/** Drag across the canvas between two points given as fractions of its size. */
export async function dragOnCanvas(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
): Promise<void> {
  const start = await canvasPoint(page, from);
  const end = await canvasPoint(page, to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(Math.round((start.x + end.x) / 2), Math.round((start.y + end.y) / 2), { steps: 4 });
  await page.mouse.move(end.x, end.y, { steps: 4 });
  await page.mouse.up();
}

/** Click a point on the canvas given as fractions of its size. */
export async function clickCanvas(page: Page, point: { x: number; y: number }): Promise<void> {
  const { x, y } = await canvasPoint(page, point);
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.up();
}

/** A button of the editor's toolbar: a tool, Undo edit, or Redo edit. */
export function editorTool(page: Page, name: string): Locator {
  return page.getByRole("toolbar", { name: "Editing tools" }).getByRole("button", { name });
}

export function zone(page: Page, number: number): Locator {
  return canvas(page).getByLabel(`Zone ${number}`, { exact: true });
}

/** Clicks a layout action. A narrow bar shows its actions as icons alone, named the same. */
export async function layoutAction(page: Page, name: string): Promise<void> {
  await page
    .getByRole("toolbar", { name: "Layout actions" })
    .getByRole("button", { name, exact: true })
    .click();
}

/** Duplicate a preset and save it as a custom layout named "<preset> copy". */
export async function saveCustomCopy(page: Page, preset: string): Promise<void> {
  await layoutCard(page, preset).click();
  await layoutAction(page, "Duplicate");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(toast(page)).toContainText(`${preset} copy saved.`);
}

/** An accessible name that is `start`, alone or followed by more after a comma. */
function namedFirst(start: string): RegExp {
  return new RegExp(`^${start.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(,|$)`);
}

/** A card of the Layouts page, by its layout's name; its accessible name goes on with its status. */
export function layoutCard(page: Page, name: string): Locator {
  return page.getByRole("button", { name: namedFirst(name) });
}

/**
 * A card of the Workspaces page, by its workspace's name; its accessible name goes on with
 * what else the card shows.
 */
export function workspaceCard(page: Page, name: string): Locator {
  return page.getByRole("button", { name: namedFirst(`Configure ${name}`) });
}
