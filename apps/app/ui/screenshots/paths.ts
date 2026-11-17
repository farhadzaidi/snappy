/** Where the captures go, from this folder: drafts in the build output, the README's images in assets. */

const repository = new URL("../../../../", import.meta.url);

/** Raw captures and store drafts, not committed. */
export const CAPTURES = new URL("target/screenshots/", repository);

/** The README's images, committed. */
export const README_IMAGES = new URL("assets/screenshots/", repository);

/** The app's captures that the README shows. */
export const README_APP_CAPTURES = {
  layouts: new URL("app/readme/layouts.png", CAPTURES),
  editor: new URL("app/readme/editor.png", CAPTURES),
};

/** The desktop capture that the README shows, from `tools/screenshots/capture.sh desktop`. */
export const DESKTOP_CAPTURE = new URL("desktop/dragging.png", CAPTURES);
