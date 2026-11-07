import type { AppState, Display, Rect } from "../core/model";
import { Observable } from "./observable";

export type Page = "layouts" | "workspaces" | "editor" | "settings";
export type Collection = "presets" | "custom";

export const PAGES: readonly { page: Page; title: string }[] = [
  { page: "layouts", title: "Layouts" },
  { page: "workspaces", title: "Workspaces" },
  { page: "editor", title: "Editor" },
  { page: "settings", title: "Settings" },
];

/** What the window shows, kept for as long as the settings app runs. */
export interface Session {
  page: Page;
  /** The display chosen on the Layouts page and in the editor; they share it. */
  displayId: string | null;
  collection: Collection;
  /** The layout previewed on the Layouts page. Null browses the display's current one. */
  browsedLayoutId: string | null;
}

export function createSession(): Observable<Session> {
  return new Observable<Session>({
    page: "layouts",
    displayId: null,
    collection: "presets",
    browsedLayoutId: null,
  });
}

/** The chosen display, or the first one when the choice is unset or disconnected. */
export function selectedDisplay(app: AppState, session: Session): Display | undefined {
  return app.displays.find((display) => display.id === session.displayId) ?? app.displays[0];
}

/** Frames this close on every side name the same display. */
const FRAME_TOLERANCE = 1;

/** The display whose frame is `frame`, as the shell reports the window's display. */
export function displayWithFrame(displays: Display[], frame: Rect | null): Display | undefined {
  if (!frame) {
    return undefined;
  }
  const close = (a: number, b: number) => Math.abs(a - b) <= FRAME_TOLERANCE;
  return displays.find(
    ({ frame: candidate }) =>
      close(candidate.x, frame.x) &&
      close(candidate.y, frame.y) &&
      close(candidate.width, frame.width) &&
      close(candidate.height, frame.height),
  );
}
