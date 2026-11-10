import type { Command, Rect, ToSettings } from "../core/model";

/** The pipe to the daemon: daemon messages in, commands out. */
export interface Transport {
  /** Start receiving daemon messages. Returns a function that stops receiving. */
  listen(onMessage: (message: ToSettings) => void): () => void;
  send(id: number, command: Command): void;
}

/** File pickers for import and export. They return null when cancelled. */
export interface FileDialogs {
  chooseImportPath(): Promise<string | null>;
  chooseExportPath(): Promise<string | null>;
}

export interface Bridge {
  transport: Transport;
  files: FileDialogs;
  /** The frame of the display the window is on, so the display chooser starts there. */
  windowDisplayFrame(): Promise<Rect | null>;
}

/** The file name export suggests. */
export const EXPORT_FILE_NAME = "Snappy-layouts.json";
