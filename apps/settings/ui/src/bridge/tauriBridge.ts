import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

import type { Rect, ToSettings } from "../core/model";
import { EXPORT_FILE_NAME, type Bridge, type Transport } from "./bridge";

/** The event the Rust shell relays daemon messages on. */
const MESSAGE_EVENT = "snappy://message";
const SETTINGS_FILES = [{ name: "Snappy settings", extensions: ["json"] }];

const transport: Transport = {
  listen(onMessage) {
    let stopped = false;
    let stop: (() => void) | undefined;
    void listen<ToSettings>(MESSAGE_EVENT, (event) => onMessage(event.payload)).then((unlisten) => {
      if (stopped) {
        unlisten();
        return;
      }
      stop = unlisten;
      // The shell holds daemon messages until the UI is listening.
      void invoke("ui_ready");
    });
    return () => {
      stopped = true;
      stop?.();
    };
  },

  send(id, command) {
    void invoke("send_command", { id, command });
  },
};

export const tauriBridge: Bridge = {
  transport,
  files: {
    chooseImportPath: () => open({ multiple: false, directory: false, filters: SETTINGS_FILES }),
    chooseExportPath: () => save({ defaultPath: EXPORT_FILE_NAME, filters: SETTINGS_FILES }),
  },
  windowDisplayFrame: () => invoke<Rect | null>("window_display_frame"),
};
