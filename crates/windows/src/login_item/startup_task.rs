//! Open at login for a packaged Snappy: the package's startup task, which its manifest
//! declares off and Windows lists under Settings > Apps > Startup. The user and policy can
//! hold it either way there, and Snappy can't override them.

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::core::HSTRING;

use crate::messages;

/// The task's ID in `packaging/msix/AppxManifest.xml.template`.
const TASK_ID: &str = "SnappyStartup";

fn task() -> windows::core::Result<StartupTask> {
    StartupTask::GetAsync(&HSTRING::from(TASK_ID))?.join()
}

/// Whether Windows will start Snappy at sign-in.
fn starts_at_login(state: StartupTaskState) -> bool {
    state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy
}

pub fn is_enabled() -> bool {
    task().and_then(|task| task.State()).is_ok_and(starts_at_login)
}

pub fn set_enabled(on: bool) -> Result<(), String> {
    let not_changed = |_| messages::LOGIN_ITEM_NOT_CHANGED.to_owned();
    let task = task().map_err(not_changed)?;
    let state = if on {
        task.RequestEnableAsync().and_then(|request| request.join())
    } else {
        // Disabling fails while policy holds the task on; the state read after says so.
        let _ = task.Disable();
        task.State()
    };
    outcome(on, state.map_err(not_changed)?).map_err(str::to_owned)
}

/// What the task's state after a change means for the user, who asked to turn it `on` or off.
fn outcome(on: bool, state: StartupTaskState) -> Result<(), &'static str> {
    if starts_at_login(state) == on {
        Ok(())
    } else if state == StartupTaskState::DisabledByUser {
        Err(messages::LOGIN_ITEM_OFF_IN_SETTINGS)
    } else if state == StartupTaskState::DisabledByPolicy {
        Err(messages::LOGIN_ITEM_BLOCKED_BY_POLICY)
    } else if state == StartupTaskState::EnabledByPolicy {
        Err(messages::LOGIN_ITEM_REQUIRED_BY_POLICY)
    } else {
        Err(messages::LOGIN_ITEM_NOT_CHANGED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_id_is_the_one_the_package_declares() {
        let manifest = include_str!("../../../../packaging/msix/AppxManifest.xml.template");
        assert!(manifest.contains(&format!(r#"TaskId="{TASK_ID}""#)));
    }

    #[test]
    fn turning_on_or_off_succeeds_when_the_task_ends_up_that_way() {
        assert_eq!(outcome(true, StartupTaskState::Enabled), Ok(()));
        assert_eq!(outcome(true, StartupTaskState::EnabledByPolicy), Ok(()));
        assert_eq!(outcome(false, StartupTaskState::Disabled), Ok(()));
        assert_eq!(outcome(false, StartupTaskState::DisabledByUser), Ok(()));
        assert_eq!(outcome(false, StartupTaskState::DisabledByPolicy), Ok(()));
    }

    #[test]
    fn a_task_held_by_the_user_or_policy_says_where_to_change_it() {
        assert_eq!(
            outcome(true, StartupTaskState::DisabledByUser),
            Err(messages::LOGIN_ITEM_OFF_IN_SETTINGS)
        );
        assert_eq!(
            outcome(true, StartupTaskState::DisabledByPolicy),
            Err(messages::LOGIN_ITEM_BLOCKED_BY_POLICY)
        );
        assert_eq!(
            outcome(false, StartupTaskState::EnabledByPolicy),
            Err(messages::LOGIN_ITEM_REQUIRED_BY_POLICY)
        );
        assert_eq!(
            outcome(true, StartupTaskState::Disabled),
            Err(messages::LOGIN_ITEM_NOT_CHANGED)
        );
    }
}
