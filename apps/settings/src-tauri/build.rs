use std::fs;
use std::path::Path;

/// Where Vite writes the web UI; `tauri.conf.json` embeds it.
const FRONTEND_DIST: &str = "../ui/dist";

fn main() {
    ensure_frontend_dist();
    tauri_build::build();
}

/// Workspace-wide cargo commands compile this crate before the web UI is built. Tauri needs
/// the asset folder to exist, so create it empty; `pnpm build` fills it.
fn ensure_frontend_dist() {
    let dist = Path::new(FRONTEND_DIST);
    if dist.exists() {
        return;
    }
    match fs::create_dir_all(dist) {
        Ok(()) => println!("cargo:warning=the web UI is not built yet; run `pnpm -C apps/settings/ui build`"),
        Err(error) => println!("cargo:warning=could not create {FRONTEND_DIST}: {error}"),
    }
}
