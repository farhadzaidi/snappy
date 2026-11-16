//! The settings file: its model, defaults, validation, and the edits the app asks for.

mod edits;
mod error;
mod file;
mod history;
mod layouts;
mod model;
mod navigation;
mod reset;
#[cfg(test)]
mod test_support;
mod validation;
mod workspaces;

pub use error::{LayoutEditError, SettingsError};
pub use history::{HistoryStep, MoveHistory};
pub use layouts::{LayoutOptions, LayoutShortcut, Layouts};
pub use model::{
    DEFAULT_WORKSPACES, DisplayAssignments, DragModifier, FORMAT_VERSION, MAX_CUSTOM_LAYOUTS,
    MAX_DISPLAY_ASSIGNMENTS, MAX_FILE_BYTES, MAX_LAYOUT_OPTIONS, MAX_WORKSPACE_ID_LENGTH, MAX_WORKSPACES,
    Settings, Snapping,
};
pub use navigation::{NEAREST_EMPTY_ZONE_TITLE, Navigation, NavigationShortcut};
pub use reset::Reset;
pub(crate) use workspaces::numbered_switch_shortcut;
pub use workspaces::{NUMBERED_SWITCH_SHORTCUTS, Workspace, WorkspaceShortcut, WorkspaceStep, Workspaces};
