//! Scratch folders for the store's tests.

use std::fs;
use std::path::PathBuf;

pub(super) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("snappy-store-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
