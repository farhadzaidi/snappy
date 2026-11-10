import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import type { Bridge } from "./bridge/bridge";
import { tauriBridge } from "./bridge/tauriBridge";
import type { PlatformName } from "./core/model";
import { loadCore } from "./core/snappyCore";
import { EditorStore } from "./editor/editorStore";
import { App } from "./shell/App";
import { DaemonClient } from "./state/daemonClient";
import { ServicesProvider } from "./state/services";
import { createSession } from "./state/session";
import "./styles/theme.css";
import "./styles/controls.css";
import "./styles/shell.css";
import "./styles/layouts.css";
import "./styles/editor.css";
import "./styles/workspaces.css";
import "./styles/settings.css";

/** `pnpm dev` and the end-to-end tests run against the in-browser fake daemon. */
async function fakeDaemonBridge(): Promise<Bridge> {
  const { FakeDaemon, fakeBridge } = await import("./bridge/fake/fakeDaemon");
  const platform = (new URLSearchParams(window.location.search).get("platform") ?? "macos") as PlatformName;
  const daemon = new FakeDaemon(platform);
  Object.assign(window, { snappyFake: daemon });
  return fakeBridge(daemon);
}

async function start(): Promise<void> {
  await loadCore();
  const bridge = import.meta.env.DEV ? await fakeDaemonBridge() : tauriBridge;
  const client = new DaemonClient(bridge.transport);
  const session = createSession();
  const editor = new EditorStore(client, session);
  client.connect();

  const root = document.getElementById("root");
  if (!root) {
    return;
  }
  createRoot(root).render(
    <StrictMode>
      <ServicesProvider services={{ client, session, editor, bridge }}>
        <App />
      </ServicesProvider>
    </StrictMode>,
  );
}

void start();
