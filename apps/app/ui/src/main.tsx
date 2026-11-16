import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import type { Bridge } from "./bridge/bridge";
import type { FakeBuild } from "./bridge/fake/fixtures";
import { tauriBridge } from "./bridge/tauriBridge";
import type { PlatformName } from "./core/model";
import { loadCore } from "./core/snappyCore";
import { EditorStore } from "./editor/editorStore";
import { Recording } from "./recorder/recording";
import { App } from "./shell/App";
import { DaemonClient } from "./state/daemonClient";
import { ServicesProvider } from "./state/services";
import { createSession } from "./state/session";
import "./styles/theme.css";
import "./styles/base.css";
import "./ui/button.css";
import "./ui/reset-button.css";
import "./ui/tabs.css";
import "./ui/toggle.css";
import "./ui/popover.css";
import "./ui/choice-menu.css";
import "./ui/confirm-dialog.css";
import "./ui/confirm-popover.css";
import "./ui/chord.css";
import "./ui/reference.css";
import "./ui/group.css";
import "./ui/info-hint.css";
import "./ui/status-dot.css";
import "./ui/zone-thumbnail.css";
import "./recorder/recorder.css";
import "./shortcuts/shortcuts.css";
import "./shell/shell.css";
import "./layouts/layouts.css";
import "./editor/editor.css";
import "./workspaces/workspaces.css";
import "./settings/settings.css";

/** `pnpm dev` and the end-to-end tests run against the in-browser fake daemon. */
async function fakeDaemonBridge(): Promise<Bridge> {
  const { FakeDaemon, fakeBridge } = await import("./bridge/fake/fakeDaemon");
  const { FakeFiles } = await import("./bridge/fake/fakeFiles");
  const query = new URLSearchParams(window.location.search);
  const platform = (query.get("platform") ?? "macos") as PlatformName;
  const build = (query.get("build") ?? "github") as FakeBuild;
  const daemon = new FakeDaemon(platform, build);
  const files = new FakeFiles();
  Object.assign(window, { snappyFake: daemon, snappyFiles: files });
  return fakeBridge(daemon, files);
}

async function start(): Promise<void> {
  await loadCore();
  const bridge = import.meta.env.DEV ? await fakeDaemonBridge() : tauriBridge;
  const client = new DaemonClient(bridge.transport);
  const session = createSession();
  const editor = new EditorStore(client, session);
  const recording = new Recording(client);
  client.connect();

  const root = document.getElementById("root");
  if (!root) {
    return;
  }
  createRoot(root).render(
    <StrictMode>
      <ServicesProvider services={{ client, session, editor, recording, bridge }}>
        <App />
      </ServicesProvider>
    </StrictMode>,
  );
}

void start();
