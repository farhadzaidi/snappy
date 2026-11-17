//! The uninstall scripts in `tools/uninstall/` name what the packages install and what the
//! daemon calls itself; these check the names they share with the rest of the repository.
#![allow(clippy::unwrap_used)]

use std::path::Path;

fn repository_file(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)).unwrap()
}

/// The values between `name=(` and the next `)` in a shell script, one per line.
fn shell_array<'a>(script: &'a str, name: &str) -> Vec<&'a str> {
    let (_, rest) = script.split_once(&format!("{name}=(")).unwrap();
    let (items, _) = rest.split_once(')').unwrap();
    items.split_whitespace().collect()
}

#[test]
fn the_shell_script_removes_exactly_what_the_tarball_holds() {
    let staging = repository_file("tools/release/linux.sh");
    let mut staged: Vec<&str> = staging
        .lines()
        .filter(|line| line.trim_start().starts_with("install -D"))
        .filter_map(|line| line.split("\"$tree/usr/").nth(1)?.split('"').next())
        .collect();
    // An install command can continue onto the next line, where the tree path is.
    staged.extend(
        staging
            .lines()
            .filter(|line| !line.trim_start().starts_with("install -D"))
            .filter_map(|line| line.trim().strip_prefix("\"$tree/usr/")?.strip_suffix('"')),
    );
    staged.sort_unstable();
    let script = repository_file("tools/uninstall/uninstall.sh");
    let mut removed = shell_array(&script, "tarball_files");
    removed.sort_unstable();
    assert_eq!(removed, staged);
}

#[test]
fn the_shell_script_names_the_package_the_bundle_and_the_cask() {
    let script = repository_file("tools/uninstall/uninstall.sh");
    assert!(repository_file("packaging/linux/control").starts_with("Package: snappy-app\n"));
    assert!(script.contains("\npackage=snappy-app\n"));
    assert!(script.contains("\nbundle_id=io.github.farhadzaidi.Snappy\n"));
    assert!(
        repository_file("packaging/macos/Snappy.plist")
            .contains("<string>io.github.farhadzaidi.Snappy</string>")
    );
    assert!(script.contains("\ncask=snappy\n"));
    assert!(repository_file("packaging/homebrew/snappy.rb.template").starts_with("cask \"snappy\" do"));
}

#[test]
fn the_powershell_script_names_the_daemons_window_and_the_installers_publisher() {
    let script = repository_file("tools/uninstall/uninstall.ps1");
    assert!(script.contains("$DaemonWindowClass = 'Snappy.Daemon'"));
    assert!(repository_file("crates/windows/src/app.rs").contains("w!(\"Snappy.Daemon\")"));
    assert!(script.contains("$Publisher = 'Syed F Zaidi'"));
    assert!(repository_file("packaging/windows/snappy.wxs").contains("Manufacturer=\"Syed F Zaidi\""));
    assert!(repository_file("packaging/windows/snappy.nsi").contains("!define PUBLISHER \"Syed F Zaidi\""));
    assert!(
        repository_file("packaging/msix/AppxManifest.xml.template")
            .contains("<PublisherDisplayName>Syed F Zaidi</PublisherDisplayName>")
    );
}

#[test]
fn every_release_carries_both_scripts() {
    let workflow = repository_file(".github/workflows/release.yml");
    for script in ["uninstall.sh", "uninstall.ps1"] {
        assert!(
            workflow.contains(&format!("\n            {script}\n")),
            "{script} isn't expected"
        );
        assert!(
            workflow.contains(&format!("source/tools/uninstall/{script}")),
            "{script} isn't copied"
        );
    }
}
