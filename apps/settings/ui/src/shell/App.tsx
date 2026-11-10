import { useEffect } from "react";

import { EditorPage } from "../editor/EditorPage";
import { LayoutsPage } from "../layouts/LayoutsPage";
import { SettingsPage } from "../settings/SettingsPage";
import { useObservable } from "../state/observable";
import { displayWithFrame, type Page } from "../state/session";
import { useAppState, useServices, useSession } from "../state/services";
import { WorkspacesPage } from "../workspaces/WorkspacesPage";
import { Header } from "./Header";
import { Toast } from "./Toast";
import { ShellNotices } from "./ShellNotices";

/**
 * The window: header, why snapping can't run when it can't, the current page, and the toast. Pages
 * wait for the daemon's first State.
 */
export function App() {
  const { client } = useServices();
  const connected = useObservable(client.state) !== null;
  const { page } = useSession();

  return (
    <div className="app">
      <Header />
      {connected && <ShellNotices />}
      <main className="page" data-page={page}>
        {connected && (
          <>
            <WindowBehavior />
            <CurrentPage page={page} />
          </>
        )}
      </main>
      <Toast />
    </div>
  );
}

function CurrentPage({ page }: { page: Page }) {
  switch (page) {
    case "layouts":
      return <LayoutsPage />;
    case "workspaces":
      return <WorkspacesPage />;
    case "editor":
      return <EditorPage />;
    case "settings":
      return <SettingsPage />;
  }
}

/** Behavior that spans pages once the daemon is connected. */
function WindowBehavior() {
  useStartOnWindowDisplay();
  useEscapeHidesPreview();
  useEditorFollowsSettings();
  return null;
}

/** The display chooser starts on the display the window is on. */
function useStartOnWindowDisplay() {
  const { bridge, client, session } = useServices();
  useEffect(() => {
    let cancelled = false;
    void bridge.windowDisplayFrame().then((frame) => {
      const display = displayWithFrame(client.state.get()?.displays ?? [], frame);
      if (!cancelled && display && session.get().displayId === null) {
        session.update((current) => ({ ...current, displayId: display.id }));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [bridge, client, session]);
}

/** Escape while the settings app is focused hides the on-screen preview. */
function useEscapeHidesPreview() {
  const { client } = useServices();
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && client.state.get()?.snapping.previewVisible) {
        void client.send({ kind: "hidePreview" });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [client]);
}

function useEditorFollowsSettings() {
  const { editor } = useServices();
  const { settings } = useAppState();
  useEffect(() => editor.follow(settings), [editor, settings]);
}
