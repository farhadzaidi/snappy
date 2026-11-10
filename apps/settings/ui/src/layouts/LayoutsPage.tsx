import { DisplayChooser } from "../shell/DisplayChooser";
import type { Collection } from "../state/session";
import { useAppState, useServices, useSession } from "../state/services";
import { Button } from "../ui/Button";
import { EmptyLayoutsIcon, PlusIcon } from "../ui/icons";
import { LayoutCard } from "./LayoutCard";
import { LayoutPreview } from "./LayoutPreview";
import { library } from "./library";

const COLLECTIONS: readonly { collection: Collection; title: string }[] = [
  { collection: "presets", title: "Presets" },
  { collection: "custom", title: "My layouts" },
];

/** The layout library. */
export function LayoutsPage() {
  const app = useAppState();
  const session = useSession();
  const { session: sessionStore, editor, client } = useServices();
  const { layouts, browsed, inUse } = library(app, session);

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
        <div className="segmented" role="tablist" aria-label="Collections">
          {COLLECTIONS.map(({ collection, title }) => (
            <button
              type="button"
              role="tab"
              key={collection}
              className="segment"
              aria-selected={session.collection === collection}
              onClick={() => showCollection(collection)}
            >
              {title}
            </button>
          ))}
        </div>
        <Button onClick={() => editor.requestNew()}>
          <PlusIcon />
          New layout
        </Button>
      </div>

      {browsed ? (
        <div className="library">
          <LayoutPreview layout={browsed} inUse={inUse} />
          <div className="layout-grid" role="list" aria-label="Layouts">
            {layouts.map((layout) => (
              <LayoutCard key={layout.id} layout={layout} browsed={layout.id === browsed.id} inUse={inUse} />
            ))}
          </div>
        </div>
      ) : (
        <div className="empty-collection">
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
