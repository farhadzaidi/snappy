/** Where the captures go, from this folder: drafts in the build output, the README's images in assets. */

const repository = new URL("../../../../", import.meta.url);

/** Raw captures and store drafts, not committed. */
export const CAPTURES = new URL("target/screenshots/", repository);

/** The README's images, committed. */
export const README_IMAGES = new URL("assets/screenshots/", repository);

/** The app's pages that the README shows, each captured here and then made a WebP of its name. */
export const README_SHOTS = ["layouts", "editor", "workspaces", "settings"] as const;

export function readmeCapture(shot: (typeof README_SHOTS)[number]): URL {
  return new URL(`app/readme/${shot}.png`, CAPTURES);
}
