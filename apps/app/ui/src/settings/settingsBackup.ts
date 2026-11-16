// Import and export: the app reads and writes the file the user chose, and the daemon checks
// and makes the settings in it.

import { UNREADABLE_FILE, UNWRITABLE_FILE, type SettingsFiles } from "../bridge/bridge";
import type { DaemonClient } from "../state/daemonClient";

/** What export says once the file is written. */
export const EXPORTED_MESSAGE = "Layouts and preferences exported.";

/** Pick a file and send its text to the daemon, which checks it and replaces the settings. */
export async function importSettings(files: SettingsFiles, client: DaemonClient): Promise<void> {
  const path = await files.chooseImportPath();
  if (path === null) {
    return;
  }
  let contents: string;
  try {
    contents = await files.read(path);
  } catch (error) {
    client.announce(messageOf(error, UNREADABLE_FILE), true);
    return;
  }
  await client.send({ kind: "importSettings", contents });
}

/** Pick a file, then write the daemon's settings text to it. */
export async function exportSettings(files: SettingsFiles, client: DaemonClient): Promise<void> {
  const path = await files.chooseExportPath();
  if (path === null) {
    return;
  }
  const contents = await client.exportSettings();
  if (contents === null) {
    return;
  }
  try {
    await files.write(path, contents);
  } catch (error) {
    client.announce(messageOf(error, UNWRITABLE_FILE), true);
    return;
  }
  client.announce(EXPORTED_MESSAGE, false);
}

/** The shell rejects with the message for the user; anything else gets `fallback`. */
function messageOf(error: unknown, fallback: string): string {
  return typeof error === "string" && error !== "" ? error : fallback;
}
