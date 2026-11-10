//! Recognizing the session, which decides how Snappy reaches other apps' windows: through
//! the X server, through GNOME Shell or KWin on Wayland, or not at all on other Wayland
//! desktops, where X11 clients see only XWayland.

/// The kind of session Snappy runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    X11,
    /// GNOME Shell on Wayland, which moves windows for Snappy through its extension.
    GnomeWayland,
    /// KDE Plasma on Wayland, where KWin moves windows for Snappy through its script.
    KdeWayland,
    /// Any other Wayland desktop, where Snappy can't control windows.
    OtherWayland,
}

/// True for a Wayland session: `XDG_SESSION_TYPE` says so, or `WAYLAND_DISPLAY` is set
/// and the session doesn't declare itself X11. X11 clients there reach only XWayland.
pub fn is_wayland(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    match session_type.map(str::to_ascii_lowercase).as_deref() {
        Some("wayland") => true,
        Some("x11") => false,
        _ => wayland_display.is_some_and(|display| !display.is_empty()),
    }
}

/// `XDG_CURRENT_DESKTOP` is a colon-separated list, such as `ubuntu:GNOME`.
fn names_desktop(current_desktop: Option<&str>, name: &str) -> bool {
    current_desktop.is_some_and(|desktops| {
        desktops
            .split(':')
            .any(|desktop| desktop.trim().eq_ignore_ascii_case(name))
    })
}

pub fn is_gnome(current_desktop: Option<&str>) -> bool {
    names_desktop(current_desktop, "GNOME")
}

/// Plasma sets `KDE`.
pub fn is_kde(current_desktop: Option<&str>) -> bool {
    names_desktop(current_desktop, "KDE")
}

pub fn classify(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    current_desktop: Option<&str>,
) -> Session {
    if !is_wayland(session_type, wayland_display) {
        Session::X11
    } else if is_gnome(current_desktop) {
        Session::GnomeWayland
    } else if is_kde(current_desktop) {
        Session::KdeWayland
    } else {
        Session::OtherWayland
    }
}

/// Reads the session from the environment.
pub fn current() -> Session {
    let variable = |name: &str| std::env::var(name).ok();
    classify(
        variable("XDG_SESSION_TYPE").as_deref(),
        variable("WAYLAND_DISPLAY").as_deref(),
        variable("XDG_CURRENT_DESKTOP").as_deref(),
    )
}

/// Reads whether the session is Wayland from the environment.
pub fn current_is_wayland() -> bool {
    current() != Session::X11
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_session_types_count_as_wayland() {
        assert!(is_wayland(Some("wayland"), None));
        assert!(is_wayland(Some("Wayland"), Some("wayland-0")));
    }

    #[test]
    fn a_wayland_display_without_an_x11_session_counts_as_wayland() {
        assert!(is_wayland(None, Some("wayland-0")));
        assert!(is_wayland(Some("tty"), Some("wayland-0")));
        assert!(!is_wayland(None, Some("")));
    }

    #[test]
    fn x11_and_unknown_sessions_count_as_x11() {
        assert!(!is_wayland(Some("x11"), None));
        assert!(!is_wayland(Some("x11"), Some("wayland-1")));
        assert!(!is_wayland(None, None));
    }

    #[test]
    fn gnome_is_found_in_the_desktop_list() {
        assert!(is_gnome(Some("GNOME")));
        assert!(is_gnome(Some("ubuntu:GNOME")));
        assert!(is_gnome(Some("gnome")));
        assert!(!is_gnome(Some("KDE")));
        assert!(!is_gnome(Some("GNOME-Flashback-ish")));
        assert!(!is_gnome(None));
    }

    #[test]
    fn kde_is_found_in_the_desktop_list() {
        assert!(is_kde(Some("KDE")));
        assert!(!is_kde(Some("GNOME")));
        assert!(!is_kde(None));
    }

    #[test]
    fn the_session_picks_the_adapter() {
        assert_eq!(classify(Some("x11"), None, Some("ubuntu:GNOME")), Session::X11);
        assert_eq!(
            classify(Some("wayland"), Some("wayland-0"), Some("ubuntu:GNOME")),
            Session::GnomeWayland
        );
        assert_eq!(
            classify(Some("wayland"), Some("wayland-0"), Some("KDE")),
            Session::KdeWayland
        );
        assert_eq!(classify(Some("x11"), None, Some("KDE")), Session::X11);
        assert_eq!(
            classify(Some("wayland"), Some("wayland-0"), Some("sway")),
            Session::OtherWayland
        );
        assert_eq!(classify(None, None, None), Session::X11);
    }
}
