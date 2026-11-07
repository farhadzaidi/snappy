//! Recognizing a Wayland session, where X11 can't see or move other apps' windows, so
//! Snappy turns live control off.

/// True for a Wayland session: `XDG_SESSION_TYPE` says so, or `WAYLAND_DISPLAY` is set
/// and the session doesn't declare itself X11. X11 clients there reach only XWayland.
pub fn is_wayland(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    match session_type.map(str::to_ascii_lowercase).as_deref() {
        Some("wayland") => true,
        Some("x11") => false,
        _ => wayland_display.is_some_and(|display| !display.is_empty()),
    }
}

/// Reads the session from the environment.
pub fn current_is_wayland() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    is_wayland(session_type.as_deref(), wayland_display.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d8_wayland_session_type_turns_live_control_off() {
        assert!(is_wayland(Some("wayland"), None));
        assert!(is_wayland(Some("Wayland"), Some("wayland-0")));
    }

    #[test]
    fn d8_wayland_display_without_an_x11_session_counts_as_wayland() {
        assert!(is_wayland(None, Some("wayland-0")));
        assert!(is_wayland(Some("tty"), Some("wayland-0")));
        assert!(!is_wayland(None, Some("")));
    }

    #[test]
    fn d8_x11_sessions_keep_live_control() {
        assert!(!is_wayland(Some("x11"), None));
        assert!(!is_wayland(Some("x11"), Some("wayland-1")));
        assert!(!is_wayland(None, None));
    }
}
