//! Chrome ignores bounds changes while its enhanced accessibility mode is on.
//! The mode is turned off for the length of one bounds change, unless VoiceOver or
//! Switch Control needs it, and turned back on afterwards or at quit.

use std::collections::HashMap;
use std::time::Duration;

use objc2_app_kit::NSWorkspace;

use crate::apps;
use crate::ax::{AxElement, names};
use crate::ffi::Pid;
use crate::thresholds::{AX_MESSAGING_TIMEOUT, CHROME_RESTORE_DELAY};

const CHROME_BUNDLE_ID: &str = "com.google.Chrome";

/// The system calls the workaround makes.
pub trait ChromeSystem {
    /// An app's accessibility element.
    type App;

    fn is_chrome(&self, pid: Pid) -> bool;
    /// VoiceOver or Switch Control is on.
    fn assistive_technology_on(&self) -> bool;
    fn application(&self, pid: Pid) -> Option<Self::App>;
    fn enhanced_mode(&self, app: &Self::App) -> Option<bool>;
    /// Whether the change was accepted.
    fn set_enhanced_mode(&self, app: &Self::App, on: bool) -> bool;
    fn is_running(&self, pid: Pid) -> bool;
}

pub struct ChromeWorkaround<S: ChromeSystem = MacChrome> {
    system: S,
    /// Apps whose enhanced mode Snappy turned off.
    suspended: HashMap<Pid, S::App>,
}

impl<S: ChromeSystem + Default> Default for ChromeWorkaround<S> {
    fn default() -> Self {
        Self {
            system: S::default(),
            suspended: HashMap::new(),
        }
    }
}

impl<S: ChromeSystem> ChromeWorkaround<S> {
    pub fn begin(&mut self, pid: Pid) {
        let system = &self.system;
        if self.suspended.contains_key(&pid) || !system.is_chrome(pid) || system.assistive_technology_on() {
            return;
        }
        let Some(application) = system.application(pid) else {
            return;
        };
        if system.enhanced_mode(&application) != Some(true) {
            return;
        }
        if system.set_enhanced_mode(&application, false) {
            self.suspended.insert(pid, application);
        }
    }

    /// Turns the mode back on. Returns how long Chrome needs to settle afterwards.
    pub fn end(&mut self, pid: Pid) -> Duration {
        if self.restore(pid) {
            CHROME_RESTORE_DELAY
        } else {
            Duration::ZERO
        }
    }

    /// Restores every outstanding change, at quit.
    pub fn restore_all(&mut self) {
        let pids: Vec<Pid> = self.suspended.keys().copied().collect();
        for pid in pids {
            self.restore(pid);
        }
    }

    /// Whether the mode was turned back on. A failed restore stays outstanding while
    /// Chrome runs.
    fn restore(&mut self, pid: Pid) -> bool {
        let Some(application) = self.suspended.get(&pid) else {
            return false;
        };
        let restored = self.system.set_enhanced_mode(application, true);
        if restored || !self.system.is_running(pid) {
            self.suspended.remove(&pid);
        }
        restored
    }
}

/// The real system: Accessibility and `NSWorkspace`.
#[derive(Default)]
pub struct MacChrome;

impl ChromeSystem for MacChrome {
    type App = AxElement;

    fn is_chrome(&self, pid: Pid) -> bool {
        apps::running(pid)
            .and_then(|app| app.bundleIdentifier())
            .is_some_and(|bundle| bundle.to_string() == CHROME_BUNDLE_ID)
    }

    fn assistive_technology_on(&self) -> bool {
        let workspace = NSWorkspace::sharedWorkspace();
        workspace.isVoiceOverEnabled() || workspace.isSwitchControlEnabled()
    }

    fn application(&self, pid: Pid) -> Option<AxElement> {
        AxElement::application(pid, AX_MESSAGING_TIMEOUT)
    }

    fn enhanced_mode(&self, app: &AxElement) -> Option<bool> {
        app.bool_attribute(names::ENHANCED_USER_INTERFACE)
    }

    fn set_enhanced_mode(&self, app: &AxElement, on: bool) -> bool {
        app.set_bool(names::ENHANCED_USER_INTERFACE, on).is_ok()
    }

    fn is_running(&self, pid: Pid) -> bool {
        apps::is_running(pid)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    const CHROME: Pid = 10;
    const SAFARI: Pid = 20;

    /// Apps by process, each with its enhanced mode.
    #[derive(Default)]
    struct FakeSystem {
        chrome: BTreeSet<Pid>,
        enhanced: RefCell<BTreeMap<Pid, bool>>,
        assistive_technology: bool,
        refuse_restore: Cell<bool>,
        quit: BTreeSet<Pid>,
    }

    impl ChromeSystem for FakeSystem {
        type App = Pid;

        fn is_chrome(&self, pid: Pid) -> bool {
            self.chrome.contains(&pid)
        }

        fn assistive_technology_on(&self) -> bool {
            self.assistive_technology
        }

        fn application(&self, pid: Pid) -> Option<Pid> {
            self.enhanced.borrow().contains_key(&pid).then_some(pid)
        }

        fn enhanced_mode(&self, app: &Pid) -> Option<bool> {
            self.enhanced.borrow().get(app).copied()
        }

        fn set_enhanced_mode(&self, app: &Pid, on: bool) -> bool {
            if on && self.refuse_restore.get() {
                return false;
            }
            self.enhanced.borrow_mut().insert(*app, on);
            true
        }

        fn is_running(&self, pid: Pid) -> bool {
            !self.quit.contains(&pid)
        }
    }

    fn workaround(assistive_technology: bool) -> ChromeWorkaround<FakeSystem> {
        let system = FakeSystem {
            chrome: BTreeSet::from([CHROME]),
            enhanced: RefCell::new(BTreeMap::from([(CHROME, true), (SAFARI, true)])),
            assistive_technology,
            ..FakeSystem::default()
        };
        ChromeWorkaround {
            system,
            suspended: HashMap::new(),
        }
    }

    fn enhanced(chrome: &ChromeWorkaround<FakeSystem>, pid: Pid) -> Option<bool> {
        chrome.system.enhanced.borrow().get(&pid).copied()
    }

    #[test]
    fn chrome_leaves_enhanced_mode_for_one_change_then_settles() {
        let mut chrome = workaround(false);
        chrome.begin(CHROME);
        assert_eq!(enhanced(&chrome, CHROME), Some(false));
        assert_eq!(chrome.end(CHROME), CHROME_RESTORE_DELAY);
        assert_eq!(enhanced(&chrome, CHROME), Some(true));
        assert_eq!(chrome.end(CHROME), Duration::ZERO, "nothing left to restore");

        chrome.begin(SAFARI);
        assert_eq!(enhanced(&chrome, SAFARI), Some(true), "only Chrome");
        assert_eq!(chrome.end(SAFARI), Duration::ZERO);
    }

    #[test]
    fn voiceover_or_switch_control_keeps_enhanced_mode_on() {
        let mut chrome = workaround(true);
        chrome.begin(CHROME);
        assert_eq!(enhanced(&chrome, CHROME), Some(true));
        assert_eq!(chrome.end(CHROME), Duration::ZERO);
    }

    #[test]
    fn a_failed_restore_stays_outstanding_until_quit() {
        let mut chrome = workaround(false);
        chrome.begin(CHROME);
        chrome.system.refuse_restore.set(true);
        assert_eq!(chrome.end(CHROME), Duration::ZERO);
        assert_eq!(enhanced(&chrome, CHROME), Some(false));

        chrome.system.refuse_restore.set(false);
        chrome.restore_all();
        assert_eq!(enhanced(&chrome, CHROME), Some(true), "undone at quit");
        assert!(chrome.suspended.is_empty());
    }

    #[test]
    fn a_restore_for_a_chrome_that_quit_is_dropped() {
        let mut chrome = workaround(false);
        chrome.begin(CHROME);
        chrome.system.refuse_restore.set(true);
        chrome.system.quit.insert(CHROME);
        chrome.end(CHROME);
        assert!(chrome.suspended.is_empty());
    }
}
