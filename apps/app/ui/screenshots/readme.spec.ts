import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

import { test, type Page } from "@playwright/test";

import { README_IMAGES, README_SHOTS, readmeCapture } from "./paths";

/** WebP quality for the README's images: small files, no visible loss at their size. */
const WEBP_QUALITY = 0.86;

/** A made-up origin the page and the image share, so the canvas can be read back. */
const ORIGIN = "https://capture.invalid";

/** Re-encodes a PNG as WebP with the browser's own encoder. */
async function toWebp(page: Page, png: Uint8Array): Promise<Uint8Array> {
  await page.route(`${ORIGIN}/**`, (route) =>
    route.request().url().endsWith(".png")
      ? route.fulfill({ body: png, contentType: "image/png" })
      : route.fulfill({ body: "<!doctype html>", contentType: "text/html" }),
  );
  await page.goto(`${ORIGIN}/`);
  const dataUrl = await page.evaluate(
    async ({ source, quality }) => {
      const image = new Image();
      image.src = source;
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      canvas.getContext("2d")?.drawImage(image, 0, 0);
      return canvas.toDataURL("image/webp", quality);
    },
    { source: `${ORIGIN}/capture.png`, quality: WEBP_QUALITY },
  );
  return Uint8Array.from(atob(dataUrl.slice(dataUrl.indexOf(",") + 1)), (char) => char.charCodeAt(0));
}

for (const shot of README_SHOTS) {
  test(shot, async ({ page }) => {
    mkdirSync(README_IMAGES, { recursive: true });
    writeFileSync(
      new URL(`${shot}.webp`, README_IMAGES),
      await toWebp(page, readFileSync(readmeCapture(shot))),
    );
  });
}
