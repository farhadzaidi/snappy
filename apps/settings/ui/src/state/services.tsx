import { createContext, useContext, type ReactNode } from "react";

import type { Bridge } from "../bridge/bridge";
import type { AppState, Command, PlatformName } from "../core/model";
import type { EditorStore } from "../editor/editorStore";
import type { DaemonClient } from "./daemonClient";
import { useObservable, type Observable } from "./observable";
import { selectedDisplay, type Page, type Session } from "./session";

/** Everything pages talk to, created once per window. */
export interface Services {
  client: DaemonClient;
  session: Observable<Session>;
  editor: EditorStore;
  bridge: Bridge;
}

const ServicesContext = createContext<Services | null>(null);

export function ServicesProvider({ services, children }: { services: Services; children: ReactNode }) {
  return <ServicesContext.Provider value={services}>{children}</ServicesContext.Provider>;
}

export function useServices(): Services {
  const services = useContext(ServicesContext);
  if (!services) {
    throw new Error("useServices needs a ServicesProvider");
  }
  return services;
}

/** The daemon's latest snapshot. Pages render only once one has arrived. */
export function useAppState(): AppState {
  const state = useObservable(useServices().client.state);
  if (!state) {
    throw new Error("pages render only after the first State message");
  }
  return state;
}

export function usePlatform(): PlatformName {
  return useAppState().platform;
}

export function useSession(): Session {
  return useObservable(useServices().session);
}

export function useSend(): (command: Command) => Promise<string | null> {
  const { client } = useServices();
  return (command) => client.send(command);
}

/** The display chosen for the Layouts page and the editor. */
export function useSelectedDisplay() {
  return selectedDisplay(useAppState(), useSession());
}

/**
 * Page and display changes hide the on-screen preview.
 */
export function useNavigation() {
  const { client, session } = useServices();
  const hidePreview = () => {
    if (client.state.get()?.snapping.previewVisible) {
      void client.send({ kind: "hidePreview" });
    }
  };
  return {
    showPage(page: Page) {
      if (session.get().page !== page) {
        hidePreview();
      }
      session.update((current) => ({ ...current, page }));
    },
    chooseDisplay(displayId: string) {
      const app = client.state.get();
      if (app && selectedDisplay(app, session.get())?.id !== displayId) {
        hidePreview();
      }
      session.update((current) => ({ ...current, displayId }));
    },
  };
}
