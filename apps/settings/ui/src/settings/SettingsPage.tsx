import { BackupGroup, DebugGroup, StartupGroup } from "./AppGroups";
import { ResetGroup } from "./Resets";
import { ShortcutGroups } from "./ShortcutGroups";
import { SnappingGroup } from "./SnappingGroup";
import { SnappyKeyGroup } from "./SnappyKeyGroup";

/** The Settings page. */
export function SettingsPage() {
  return (
    <div className="settings-page">
      <div className="settings-column">
        <h1 className="page-title">Global settings</h1>
        <SnappingGroup />
        <SnappyKeyGroup />
        <ShortcutGroups />
        <StartupGroup />
        <BackupGroup />
        <DebugGroup />
        <ResetGroup />
      </div>
    </div>
  );
}
