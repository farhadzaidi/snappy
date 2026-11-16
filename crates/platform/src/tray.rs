//! The menu bar or tray menu: its content, the trait that shows it, and what it sends back.

/// The menu bar menu's content.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayMenu {
    pub live: bool,
    pub tooltip: String,
    pub displays: Vec<TrayDisplay>,
    pub workspaces: Vec<TrayItem>,
    pub workspaces_running: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayDisplay {
    pub id: String,
    pub name: String,
    pub presets: Vec<TrayItem>,
    pub custom: Vec<TrayItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayItem {
    pub id: String,
    pub title: String,
    pub checked: bool,
    pub enabled: bool,
}

pub trait Tray {
    fn update(&mut self, menu: &TrayMenu);
}

/// A choice made in the menu.
#[derive(Debug, Clone, PartialEq)]
pub enum TrayCommand {
    ManageWorkspaces,
    SelectLayout { display: String, layout: String },
    SwitchWorkspace(String),
}
