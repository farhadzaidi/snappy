//! What each exported function takes.

use std::collections::BTreeMap;

use serde::Deserialize;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{KeyChord, Modifier, Mods, Os, SnappyKey};
use snappy_core::layout::{Layout, UnitRect, Zone};
use snappy_core::settings::{LayoutOptions, Reset, Settings, Workspace};
use snappy_core::shortcuts::{NamedShortcut, ShortcutId};

use super::{Axis, CutScope, Field, Handle};

#[derive(Deserialize)]
pub struct Empty {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneFramesRequest {
    pub zones: Vec<Zone>,
    pub work_area: Rect,
    pub spacing_factor: f64,
}

#[derive(Deserialize)]
pub struct ZonesRequest {
    pub zones: Vec<Zone>,
}

#[derive(Deserialize)]
pub struct HandleAtRequest {
    pub point: Point,
    pub size: Size,
}

#[derive(Deserialize)]
pub struct SizeRequest {
    pub size: Size,
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub zones: Vec<Zone>,
    pub id: String,
    pub delta: Point,
    pub tolerance: Size,
}

#[derive(Deserialize)]
pub struct ResizeRequest {
    pub zones: Vec<Zone>,
    pub id: String,
    pub handle: Handle,
    pub delta: Point,
    pub tolerance: Size,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividerRequest {
    pub zones: Vec<Zone>,
    pub zone_id: String,
    pub axis: Axis,
    pub line: f64,
    pub delta: f64,
    pub tolerance: f64,
}

#[derive(Deserialize)]
pub struct DrawRequest {
    pub zones: Vec<Zone>,
    pub from: Point,
    pub to: Point,
    pub tolerance: Size,
}

#[derive(Deserialize)]
pub struct CopiesRequest {
    pub zones: Vec<Zone>,
    pub copied: Vec<UnitRect>,
}

#[derive(Deserialize)]
pub struct CutPreviewRequest {
    pub zones: Vec<Zone>,
    pub scope: CutScope,
    pub axis: Axis,
    pub at: f64,
    pub tolerance: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutRequest {
    pub zones: Vec<Zone>,
    pub scope: CutScope,
    pub axis: Axis,
    /// The line itself, already snapped by the preview.
    pub at: f64,
    /// At least one per zone the cut may split.
    pub new_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub zones: Vec<Zone>,
    pub ids: Vec<String>,
    pub zone_shortcuts: BTreeMap<String, KeyChord>,
}

#[derive(Deserialize)]
pub struct FieldRequest {
    pub zones: Vec<Zone>,
    pub id: String,
    pub field: Field,
    pub value: f64,
}

#[derive(Deserialize)]
pub struct ChordRequest {
    pub chord: KeyChord,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct UnavailableRequest {
    pub chord: KeyChord,
    /// The shortcut being recorded, with the title its recorder shows.
    pub shortcut: NamedShortcut,
    pub platform: Os,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyedChordRequest {
    pub chord: KeyChord,
    pub snappy_key: SnappyKey,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnappyKeyRequest {
    pub snappy_key: SnappyKey,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct SnappyKeyChangeRequest {
    pub settings: Settings,
    pub modifiers: Mods,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct ModifierRequest {
    pub modifier: Modifier,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct PlatformRequest {
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct SettingsRequest {
    pub settings: Settings,
    pub platform: Os,
}

/// A settings file's text, as import reads it.
#[derive(Deserialize)]
pub struct SettingsFileRequest {
    pub contents: String,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct RecordingRequest {
    pub settings: Settings,
    /// The shortcut being recorded, which the message names from its side.
    pub shortcut: ShortcutId,
    pub chord: KeyChord,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct ResetRequest {
    pub settings: Settings,
    pub reset: Reset,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct LayoutConflictRequest {
    pub settings: Settings,
    pub layout: Layout,
    pub options: LayoutOptions,
    pub platform: Os,
}

#[derive(Deserialize)]
pub struct LayoutRequest {
    pub layout: Layout,
}

#[derive(Deserialize)]
pub struct DisplayLayoutRequest {
    pub settings: Settings,
    pub display: String,
    /// The workspace to answer for; without one, the display's layout now.
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Deserialize)]
pub struct LayoutOptionsRequest {
    pub settings: Settings,
    pub layout: Layout,
}

#[derive(Deserialize)]
pub struct SectionsRequest {
    pub settings: Settings,
}

#[derive(Deserialize)]
pub struct WorkspaceNameRequest {
    pub workspace: String,
}

#[derive(Deserialize)]
pub struct LayoutTitlesRequest {
    pub layout: String,
    pub zones: usize,
}

#[derive(Deserialize)]
pub struct EditRequest {
    pub settings: Settings,
    pub edit: SettingsEdit,
    pub platform: Os,
}

/// A change the app asks the daemon for, by the `snappy-core` rule that makes it.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SettingsEdit {
    AssignLayout {
        display: String,
        layout: String,
    },
    SaveLayout {
        layout: Layout,
        options: LayoutOptions,
        display: String,
    },
    RemoveLayout {
        layout: String,
    },
    SetLayoutEnabled {
        layout: String,
        enabled: bool,
    },
    /// Turn workspaces on, with the IDs of the connected displays.
    EnableWorkspaces {
        displays: Vec<String>,
    },
    AddWorkspace,
    UpdateWorkspace {
        workspace: Workspace,
    },
    ReorderWorkspaces {
        from: Vec<usize>,
        to: usize,
    },
    RemoveWorkspace {
        workspace: String,
    },
}
