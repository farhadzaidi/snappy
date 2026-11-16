import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

import type { Rect } from "../core/model";
import { EXPORT_FILE_NAME, type Bridge, type Transport } from "./bridge";
import type { ToApp } from "./protocol";

/** The event the Rust shell relays daemon messages on; the shell checks the names match its own. */
const MESSAGE_EVENT = "snappy://message";
/** The Rust shell's commands. */
const UI_READY = "ui_ready";
const SEND_COMMAND = "send_command";
const WINDOW_DISPLAY_FRAME = "window_display_frame";
const SETTINGS_FILES = [{ name: "Snappy settings", extensions: ["json"] }];

const transport: Transport = {
  listen(onMessage) {
    let stopped = false;
    let stop: (() => void) | undefined;
    void listen<ToApp>(MESSAGE_EVENT, (event) => onMessage(event.payload)).then((unlisten) => {
      if (stopped) {
        unlisten();
        return;
      }
      stop = unlisten;
      // The shell holds daemon messages until the UI is listening.
      void invoke(UI_READY);
    });
    return () => {
      stopped = true;
      stop?.();
    };
  },

  send(id, command) {
    void invoke(SEND_COMMAND, { id, command });
  },
};

export const tauriBridge: Bridge = {
  transport,
  files: {
    chooseImportPath: () => open({ multiple: false, directory: false, filters: SETTINGS_FILES }),
    chooseExportPath: () => save({ defaultPath: EXPORT_FILE_NAME, filters: SETTINGS_FILES }),
  },
  windowDisplayFrame: () => invoke<Rect | null>(WINDOW_DISPLAY_FRAME),
};
