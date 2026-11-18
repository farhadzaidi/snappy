//! Native tab groups, read through Accessibility. `tab_matching` says how macOS shows them
//! and when a window counts as a hidden tab.

use std::collections::{BTreeSet, HashMap};

use snappy_core::geometry::Rect;
use snappy_platform::{TabGroup, WindowId};

use super::enumerate::{app_windows, is_minimized};
use super::registry;
use super::tab_matching::{MaybeTab, ShownTab};
use super::tracked::{Windows, window_facts};
use crate::apps;
use crate::ax::{self, AxElement, names};
use crate::space_visits;
use crate::thresholds::AX_ENUMERATION_TIMEOUT;

impl Windows {
    /// The tab groups, with a hidden tab Snappy has seen, of the apps owning `windows`.
    pub fn tab_groups(&mut self, windows: &[WindowId]) -> Vec<TabGroup> {
        if !ax::is_trusted() {
            return Vec::new();
        }
        let pids: BTreeSet<i32> = {
            let registry = registry::lock(&self.registry);
            windows
                .iter()
                .filter_map(|window| registry.get(*window))
                .map(|entry| entry.pid)
                .collect()
        };
        let groups = pids.into_iter().flat_map(|pid| self.tab_groups_of(pid)).collect();
        let registry = registry::lock(&self.registry);
        self.tabs.retain(|window| registry.get(window).is_some());
        groups
    }

    fn tab_groups_of(&mut self, pid: i32) -> Vec<TabGroup> {
        let Some(app) = apps::running(pid) else {
            return Vec::new();
        };
        let facts = apps::facts(&app);
        let application = AxElement::application(pid, AX_ENUMERATION_TIMEOUT);
        let (true, Some(application)) = (facts.is_supported(), application) else {
            return Vec::new();
        };
        let visit = space_visits::current();
        let listed = app_windows(&application, AX_ENUMERATION_TIMEOUT);
        let (on_list, off_list): (Vec<_>, Vec<_>) = registry::lock(&self.registry)
            .windows_of(pid)
            .into_iter()
            .partition(|(_, element)| listed.contains(element));

        // Hidden tabs are windows Snappy saw that the app no longer lists, though they are
        // still there and not minimized. Those Snappy can't tell from windows on another
        // Space aren't read at all.
        let mut maybe: Vec<MaybeTab> = off_list
            .iter()
            .filter(|(id, _)| self.tabs.may_be_hidden(*id, visit))
            .filter_map(|(id, element)| hidden_tab_candidate(*id, element))
            .collect();
        let minimized: Vec<bool> = listed.iter().map(is_minimized).collect();
        let remembered = on_list.iter().any(|(id, _)| self.tabs.remembers(*id));
        if maybe.is_empty() && !remembered && minimized.iter().filter(|minimized| **minimized).count() < 2 {
            let seen: Vec<WindowId> = on_list.iter().map(|(id, _)| *id).collect();
            self.tabs.saw(&seen, visit);
            return Vec::new();
        }

        let mut shown = Vec::new();
        let mut alone = Vec::new();
        let mut shown_elements: HashMap<WindowId, (AxElement, Rect, usize)> = HashMap::new();
        for (element, minimized) in listed.iter().zip(minimized) {
            let Ok(frame) = element.frame() else { continue };
            let id = self.identify(element, pid);
            match tab_count(element) {
                Some(tabs) if tabs > 1 => {
                    shown.push(ShownTab {
                        id,
                        frame,
                        tabs,
                        minimized,
                    });
                    shown_elements.insert(id, (element.clone(), frame, tabs));
                }
                _ if minimized => maybe.push(MaybeTab { id, frame, minimized }),
                _ => alone.push(id),
            }
        }

        self.tabs
            .groups(visit, &shown, &maybe, &alone)
            .into_iter()
            .filter(|(_, hidden)| !hidden.is_empty())
            .filter_map(|(shown, hidden)| {
                let (element, frame, tabs) = shown_elements.get(&shown)?;
                window_facts(element).check().ok()?;
                Some(TabGroup {
                    shown: self.describe(element, *frame, &facts),
                    tabs: *tabs,
                    hidden,
                })
            })
            .collect()
    }
}

/// A window its app no longer lists, read as a possible hidden tab. A minimized window isn't
/// one, and neither is a window with a tab bar of its own: only the tab showing in a group
/// has a tab bar, so that window is showing on another Space.
fn hidden_tab_candidate(id: WindowId, element: &AxElement) -> Option<MaybeTab> {
    element.set_timeout(AX_ENUMERATION_TIMEOUT);
    let frame = element.frame().ok()?;
    let candidate = !is_minimized(element) && tab_count(element).is_none();
    candidate.then_some(MaybeTab {
        id,
        frame,
        minimized: false,
    })
}

/// How many tabs the window's tab bar holds, when it has one.
fn tab_count(window: &AxElement) -> Option<usize> {
    window
        .elements_attribute(names::CHILDREN)
        .ok()?
        .iter()
        .find(|child| {
            child.set_timeout(AX_ENUMERATION_TIMEOUT);
            child.string_attribute(names::ROLE).as_deref() == Some(names::TAB_GROUP_ROLE)
        })
        .and_then(|bar| bar.elements_attribute(names::TABS).ok())
        .map(|tabs| tabs.len())
}
