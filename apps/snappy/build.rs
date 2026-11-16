//! Links Snappy's icon into the Windows executable, for Explorer, the taskbar, and Task
//! Manager. `tools/icon/generate.sh` writes the compiled resource, and the MSVC linker takes
//! a `.res` as an input, so no resource compiler runs here.

use std::env;
use std::path::Path;

const RESOURCE: &str = "../../assets/icon/snappy.res";

fn main() {
    println!("cargo:rerun-if-changed={RESOURCE}");
    let target_is = |key: &str, value: &str| env::var(key).is_ok_and(|actual| actual == value);
    if !(target_is("CARGO_CFG_TARGET_OS", "windows") && target_is("CARGO_CFG_TARGET_ENV", "msvc")) {
        return;
    }
    // The linker runs elsewhere, so it needs the full path.
    match env::var("CARGO_MANIFEST_DIR") {
        Ok(manifest) => println!(
            "cargo:rustc-link-arg-bins={}",
            Path::new(&manifest).join(RESOURCE).display()
        ),
        Err(_) => println!("cargo:warning=no CARGO_MANIFEST_DIR; snappy.exe is built without its icon"),
    }
}
