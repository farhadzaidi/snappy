/// After a switch or send, app activations are ignored for this long, because the
/// switch itself causes some.
pub const FOCUS_GATE_SECONDS: f64 = 0.75;

/// Decides when an app activation may switch to its window's workspace. Explicit
/// switches always win; focus notifications never switch.
#[derive(Debug, Clone, Default)]
pub struct FocusPolicy {
    changing: bool,
    quiet_until: Option<f64>,
}

impl FocusPolicy {
    pub fn is_changing(&self) -> bool {
        self.changing
    }

    pub fn begin_change(&mut self) {
        self.changing = true;
    }

    /// `now` is a monotonic time in seconds.
    pub fn end_change(&mut self, now: f64) {
        self.changing = false;
        self.quiet_until = Some(now + FOCUS_GATE_SECONDS);
    }

    /// The workspace to switch to after a focus change, if any.
    pub fn follow<'a>(
        &self,
        window_workspace: &'a str,
        active: &str,
        by_activation: bool,
        now: f64,
    ) -> Option<&'a str> {
        let settled = self.quiet_until.is_none_or(|until| now >= until);
        (by_activation && !self.changing && settled && window_workspace != active).then_some(window_workspace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_notifications_never_switch() {
        let policy = FocusPolicy::default();
        assert_eq!(policy.follow("two", "one", false, 10.0), None);
        assert_eq!(policy.follow("two", "one", true, 10.0), Some("two"));
    }

    #[test]
    fn activations_wait_for_the_gate_after_a_change() {
        let mut policy = FocusPolicy::default();
        policy.begin_change();
        assert_eq!(policy.follow("two", "one", true, 10.0), None);
        policy.end_change(10.0);
        assert_eq!(policy.follow("two", "one", true, 10.5), None);
        assert_eq!(policy.follow("two", "one", true, 10.75), Some("two"));
    }
}
