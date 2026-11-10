//! What the macOS bundle declares and which system services the daemon calls.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bundle_file(name: &str) -> String {
    std::fs::read_to_string(workspace_root().join("tools/macos").join(name)).unwrap()
}

/// The value after `<key>key</key>` in a property list: a whole array, or else its first tag.
fn plist_value<'a>(plist: &'a str, key: &str) -> &'a str {
    let marker = format!("<key>{key}</key>");
    let (_, rest) = plist.split_once(&marker).unwrap();
    let rest = rest.trim_start();
    let end = if rest.starts_with("<array>") {
        rest.find("</array>").unwrap() + "</array>".len()
    } else {
        rest.find('>').unwrap() + 1
    };
    &rest[..end]
}

fn rust_sources(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn snappy_and_its_settings_app_have_no_dock_icon() {
    for plist in ["Snappy.plist", "SnappySettings.plist"] {
        assert_eq!(
            plist_value(&bundle_file(plist), "LSUIElement"),
            "<true/>",
            "{plist}"
        );
    }
    let script = bundle_file("bundle.sh");
    assert!(script.contains(r#"cp "$here/Snappy.plist" "$app/Contents/Info.plist""#));
    assert!(script.contains(r#"cp "$here/SnappySettings.plist" "$helper/Contents/Info.plist""#));
}

#[test]
fn launches_at_login_stay_in_the_background() {
    let agent = bundle_file("local.snappy.agent.plist");
    let arguments = plist_value(&agent, "ProgramArguments");
    assert!(arguments.contains("<string>--background</string>"), "{arguments}");
}

/// Calls that would make macOS ask for Screen Recording or Input Monitoring.
const OTHER_PERMISSION_APIS: &[&str] = &[
    "CGEventTapCreate",
    "CGPreflightScreenCaptureAccess",
    "CGRequestScreenCaptureAccess",
    "CGPreflightListenEventAccess",
    "CGRequestListenEventAccess",
    "CGWindowListCreateImage",
    "CGDisplayStream",
    "SCShareableContent",
    "SCStream",
    "IOHIDRequestAccess",
    "IOHIDCheckAccess",
    "kCGWindowName",
];

#[test]
fn snappy_asks_for_accessibility_only() {
    for plist in ["Snappy.plist", "SnappySettings.plist"] {
        assert!(
            !bundle_file(plist).contains("UsageDescription"),
            "{plist} declares a privacy usage"
        );
    }

    let mut sources = Vec::new();
    for folder in [
        "apps/snappy/src",
        "crates/macos/src",
        "apps/settings/src-tauri/src",
    ] {
        rust_sources(&workspace_root().join(folder), &mut sources);
    }
    assert!(!sources.is_empty());
    for path in sources {
        let code = std::fs::read_to_string(&path).unwrap();
        for api in OTHER_PERMISSION_APIS {
            assert!(!code.contains(api), "{} calls {api}", path.display());
        }
    }
}
