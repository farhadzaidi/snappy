//! The daemon has no way to reach the network, and never reads window content.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Crates that speak HTTP, WebSockets, or TLS, or that report crashes, analytics, or updates.
const NETWORK_CRATES: &[&str] = &[
    "reqwest",
    "hyper",
    "hyper-util",
    "h2",
    "h3",
    "ureq",
    "curl",
    "curl-sys",
    "isahc",
    "surf",
    "attohttpc",
    "minreq",
    "tungstenite",
    "tokio-tungstenite",
    "rustls",
    "native-tls",
    "openssl",
    "openssl-sys",
    "quinn",
    "sentry",
    "opentelemetry",
    "posthog-rs",
    "crash-handler",
    "minidumper",
    "self_update",
    "tauri-plugin-updater",
];

/// Network APIs that daemon code must never call.
const NETWORK_APIS: &[&str] = &[
    "std::net",
    "tokio::net",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "NSURLSession",
    "NSURLConnection",
    "CFNetwork",
    "WinHttp",
    "Networking_WinInet",
    "Networking_WinSock",
];

/// Every crate the daemon binary is built from.
const DAEMON_CRATES: &[&str] = &[
    "apps/snappy",
    "crates/core",
    "crates/engine",
    "crates/ipc",
    "crates/platform",
    "crates/macos",
    "crates/windows",
    "crates/x11",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Each locked package's dependencies by name, merged across versions.
fn lock_graph() -> BTreeMap<String, BTreeSet<String>> {
    let lock = std::fs::read_to_string(workspace_root().join("Cargo.lock")).unwrap();
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for package in lock.split("[[package]]").skip(1) {
        let name = package
            .lines()
            .find_map(|line| line.strip_prefix("name = "))
            .unwrap()
            .trim_matches('"')
            .to_owned();
        let listed = package
            .split_once("dependencies = [")
            .and_then(|(_, rest)| rest.split_once(']'))
            .map_or("", |(list, _)| list);
        let dependencies = listed
            .split(',')
            .filter_map(|entry| entry.trim().trim_matches('"').split(' ').next())
            .filter(|dependency| !dependency.is_empty())
            .map(str::to_owned);
        graph.entry(name).or_default().extend(dependencies);
    }
    graph
}

fn reachable_from(graph: &BTreeMap<String, BTreeSet<String>>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(name) = pending.pop() {
        if seen.insert(name.clone()) {
            pending.extend(graph.get(&name).into_iter().flatten().cloned());
        }
    }
    seen
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
fn the_daemon_depends_on_no_network_client() {
    let daemon = reachable_from(&lock_graph(), "snappy");
    for expected in ["snappy-engine", "snappy-macos", "tokio"] {
        assert!(daemon.contains(expected), "the lock walk missed {expected}");
    }

    let network: Vec<&&str> = NETWORK_CRATES
        .iter()
        .filter(|name| daemon.contains(**name))
        .collect();
    assert!(network.is_empty(), "the daemon depends on {network:?}");
}

#[test]
fn the_daemon_leaves_tokio_networking_off() {
    for folder in DAEMON_CRATES {
        let manifest = std::fs::read_to_string(workspace_root().join(folder).join("Cargo.toml")).unwrap();
        for line in manifest
            .lines()
            .filter(|line| line.trim_start().starts_with("tokio"))
        {
            assert!(
                !line.contains("\"net\"") && !line.contains("\"full\""),
                "{folder} enables tokio networking: {line}"
            );
        }
    }
}

/// Window titles, URLs, content, and typed text, as Accessibility attributes, window
/// server keys, X11 properties and selections, and key event accessors. X11 code reads
/// `_NET_DESKTOP_NAMES`, the names of the desktops themselves, to add Snappy's parking
/// desktop after the user's own; that names no window, so it is allowed.
const PRIVATE_CONTENT_APIS: &[&str] = &[
    "\"AXTitle\"",
    "\"AXURL\"",
    "\"AXDocument\"",
    "\"AXValue\"",
    "\"AXSelectedText\"",
    "\"AXDescription\"",
    "\"AXHelp\"",
    "kAXTitleAttribute",
    "kAXURLAttribute",
    "kAXDocumentAttribute",
    "kAXValueAttribute",
    "kCGWindowName",
    "characters()",
    "charactersIgnoringModifiers",
    "GetWindowText",
    "InternalGetWindowText",
    "WM_GETTEXT",
    "ToUnicode",
    "GetClipboardData",
    "WM_NAME",
    "WM_ICON_NAME",
    "_NET_WM_VISIBLE_NAME",
    "_NET_WM_VISIBLE_ICON_NAME",
    "CLIPBOARD",
    "convert_selection",
    "LookupString",
    "keysym_to_char",
];

#[test]
fn daemon_code_never_reads_titles_urls_contents_or_typed_text() {
    let mut sources = Vec::new();
    for folder in DAEMON_CRATES {
        rust_sources(&workspace_root().join(folder).join("src"), &mut sources);
    }
    assert!(!sources.is_empty());

    for path in sources {
        let code = std::fs::read_to_string(&path).unwrap();
        for api in PRIVATE_CONTENT_APIS {
            assert!(!code.contains(api), "{} reads {api}", path.display());
        }
    }
}

#[test]
fn daemon_code_calls_no_network_api() {
    let mut sources = Vec::new();
    for folder in DAEMON_CRATES {
        rust_sources(&workspace_root().join(folder).join("src"), &mut sources);
    }
    assert!(!sources.is_empty());

    for path in sources {
        let code = std::fs::read_to_string(&path).unwrap();
        for api in NETWORK_APIS {
            assert!(!code.contains(api), "{} uses {api}", path.display());
        }
    }
}
