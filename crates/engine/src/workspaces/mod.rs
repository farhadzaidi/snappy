//! Running workspaces against real windows, split by what drives it: what the engine keeps
//! between events (`controller`), starting and stopping (`lifecycle`), the window access a
//! session moves windows through (`core_io`), bringing windows into the session (`members`),
//! switching and sending (`switching`), what the platform reports (`events`), keeping parked
//! windows hidden (`containment`), and what a restart keeps (`recovery`).

mod containment;
mod controller;
mod core_io;
mod events;
mod lifecycle;
mod members;
mod recovery;
mod switching;

pub(crate) use containment::window_moved;
pub(crate) use controller::WorkspaceController;
pub(crate) use events::{
    app_terminated, displays_changed, focus_changed, native_desktop_changed, window_destroyed,
};
pub(crate) use lifecycle::{configure, detach, resume, stop};
pub(crate) use switching::{
    AfterSend, SwitchFocus, cycle, remove_workspace, request_switch, send_and_stay, send_focused,
    shown_on_active_workspace,
};
