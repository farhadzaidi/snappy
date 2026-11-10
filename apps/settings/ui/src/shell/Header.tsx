import { useObservable } from "../state/observable";
import { PAGES } from "../state/session";
import { useNavigation, useServices, useSession } from "../state/services";
import { LogoIcon } from "../ui/icons";
import { HelpButton } from "./HelpPopover";
import { SnappingButton } from "./SnappingButton";

/** The name, the page tabs, the snapping button, and help. */
export function Header() {
  const { client } = useServices();
  const connected = useObservable(client.state) !== null;
  const { page } = useSession();
  const { showPage } = useNavigation();

  return (
    <header className="header">
      <div className="brand">
        <span className="brand-logo">
          <LogoIcon />
        </span>
        <span className="brand-name">Snappy</span>
      </div>
      <nav className="tabs" aria-label="Pages">
        {PAGES.map((item) => (
          <button
            type="button"
            key={item.page}
            className="tab"
            aria-current={item.page === page ? "page" : undefined}
            onClick={() => showPage(item.page)}
          >
            {item.title}
          </button>
        ))}
      </nav>
      <div className="header-actions">
        {connected && <SnappingButton />}
        {connected && <HelpButton />}
      </div>
    </header>
  );
}
