//! The menu bar or tray menu's content.

use snappy_core::layout::Layout;
use snappy_platform::{Platform, TrayDisplay, TrayItem, TrayMenu};

use super::EngineState;

impl<P: Platform> EngineState<P> {
    pub fn update_tray(&mut self) {
        let workspaces = &self.settings.workspaces;
        let running = self.workspaces.is_running();
        let tooltip = match workspaces.active() {
            Some(active) if running => format!("Snappy · {}", active.name),
            _ => "Snappy — window zones".to_owned(),
        };
        let displays = self
            .displays
            .iter()
            .map(|display| {
                let current = self.settings.layout_for_display(&display.id).id.clone();
                let item = |layout: &Layout| TrayItem {
                    id: layout.id.clone(),
                    title: layout.name.clone(),
                    checked: layout.id == current,
                    enabled: self.settings.is_enabled(layout),
                };
                TrayDisplay {
                    id: display.id.clone(),
                    name: display.name.clone(),
                    presets: snappy_core::layout::presets().iter().map(item).collect(),
                    custom: self.settings.layouts.custom.iter().map(item).collect(),
                }
            })
            .collect();
        let items = workspaces
            .items
            .iter()
            .map(|item| TrayItem {
                id: item.id.clone(),
                title: item.name.clone(),
                checked: item.id == workspaces.active_id,
                enabled: running,
            })
            .collect();
        let menu = TrayMenu {
            live: self.live,
            tooltip,
            displays,
            workspaces: items,
            workspaces_running: running,
        };
        self.platform.update(&menu);
    }
}
