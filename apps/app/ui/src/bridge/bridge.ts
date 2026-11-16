import type { Rect } from "../core/model";
import type { Command, ToApp } from "./protocol";

/** The pipe to the daemon: daemon messages in, commands out. */
export interface Transport {
  /** Start receiving daemon messages. Returns a function that stops receiving. */
  listen(onMessage: (message: ToApp) => void): () => void;
  send(id: number, command: Command): void;
}

/**
 * The files settings are imported from and exported to. The app reads and writes them, not
 * the daemon: a file picker grants the chosen file to the app, which showed it.
 */
export interface SettingsFiles {
  /** The file picker for import. Null when cancelled. */
  chooseImportPath(): Promise<string | null>;
  /** The file picker for export. Null when cancelled. */
  chooseExportPath(): Promise<string | null>;
  /** The file's text. Rejects with a message for the user. */
  read(path: string): Promise<string>;
  /** Write `contents` to the file. Rejects with a message for the user. */
  write(path: string, contents: string): Promise<void>;
}

export interface Bridge {
  transport: Transport;
  files: SettingsFiles;
  /** The frame of the display the window is on, so the display chooser starts there. */
  windowDisplayFrame(): Promise<Rect | null>;
}

/** The file name export suggests. */
export const EXPORT_FILE_NAME = "Snappy-layouts.json";

/**
 * The shell's messages for a file it can't read or write, also shown when a failure comes
 * without one. The shell checks they match its own.
 */
export const UNREADABLE_FILE = "Couldn't read that file.";
export const UNWRITABLE_FILE = "Couldn't save to that file.";
