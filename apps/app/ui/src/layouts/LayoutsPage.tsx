import { useEffect } from "react";

import { DisplayChooser } from "../shell/DisplayChooser";
import type { Collection } from "../state/session";
import { useAppState, useServices, useSession } from "../state/services";
import { Button } from "../ui/Button";
import { EmptyLayoutsIcon, PlusIcon } from "../ui/icons";
import { tabPanelProps, Tabs, useTabIds } from "../ui/Tabs";
import { LayoutCard } from "./LayoutCard";
import { LayoutPreview } from "./LayoutPreview";
import { library } from "./library";

const COLLECTIONS: readonly { value: Collection; title: string }[] = [
  { value: "presets", title: "Presets" },
  { value: "custom", title: "My layouts" },
];

/** The layout library. */
export function LayoutsPage() {
  const app = useAppState();
  const session = useSession();
  const tabIds = useTabIds();
  const panel = tabPanelProps(tabIds, session.collection);
  const { session: sessionStore, editor, client } = useServices();
  const { layouts, browsed, inUse } = library(app, session);
  useKeepBrowsing(browsed?.id);

  /** Switching collections hides the preview and browses the display's layout. */
  const showCollection = (collection: Collection) => {
    if (app.snapping.previewVisible) {
      void client.send({ kind: "hidePreview" });
    }
    sessionStore.update((current) => ({ ...current, collection, browsedLayoutId: null }));
  };

  return (
    <div className="layouts-page">
      <div className="page-heading">
        <h1 className="page-title">Layouts</h1>
        <DisplayChooser />
      </div>

      <div className="library-bar">
        <Tabs
          label="Collections"
          tabs={COLLECTIONS}
          selected={session.collection}
          ids={tabIds}
          onSelect={showCollection}
        />
        <Button onClick={() => editor.requestNew()}>
          <PlusIcon />
          New layout
        </Button>
      </div>

      {browsed ? (
        <div className="library" {...panel}>
          <LayoutPreview layout={browsed} inUse={inUse} />
          <div className="layout-grid" role="list" aria-label="Layouts">
            {layouts.map((layout) => (
              <LayoutCard key={layout.id} layout={layout} browsed={layout.id === browsed.id} inUse={inUse} />
            ))}
          </div>
        </div>
      ) : (
        <div className="empty-collection" {...panel}>
          <span className="muted">
            <EmptyLayoutsIcon />
          </span>
          <p className="empty-title">No custom layouts yet</p>
          <Button variant="primary" onClick={() => editor.requestNew()}>
            Create layout
          </Button>
        </div>
      )}
    </div>
  );
}

/** Keep browsing the layout the page shows, so it changes only when the user picks another. */
function useKeepBrowsing(layoutId: string | undefined) {
  const { session } = useServices();
  useEffect(() => {
    if (layoutId !== undefined && session.get().browsedLayoutId !== layoutId) {
      session.update((current) => ({ ...current, browsedLayoutId: layoutId }));
    }
  }, [session, layoutId]);
}
