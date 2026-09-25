use log::{debug, error};
use once_cell::sync::Lazy;
use std::path::PathBuf;

const EXECUTABLE_NAME: &str = "ffmpeg";

static FFMPEG_PATH: Lazy<Option<PathBuf>> = Lazy::new(find_ffmpeg_path_internal);

pub fn find_ffmpeg_path() -> Option<PathBuf> {
    FFMPEG_PATH.as_ref().map(|p| p.clone())
}

fn find_ffmpeg_path_internal() -> Option<PathBuf> {
    debug!("Starting search for the bundled ffmpeg executable");

    // Only the ffmpeg sidecar binary bundled next to the application
    // executable is used. No PATH lookup, no user-writable directory
    // fallback (e.g. under the home directory), and no on-demand download:
    // if it is missing, the app install is broken and callers must surface
    // a clear "reinstall" error instead of silently reaching outside the
    // app bundle for a substitute binary.
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_folder) = exe_path.parent() {
            let bundled = exe_folder.join(EXECUTABLE_NAME);
            if bundled.exists() && bundled.is_file() {
                debug!("Found bundled ffmpeg: {:?}", bundled);
                return Some(bundled);
            }

            // `cargo test` binaries live one directory deeper than the crate's
            // own binary (target/<profile>/deps/ vs. target/<profile>/), where
            // the sidecar built by build.rs is placed. This never applies to a
            // packaged app, only to test runs.
            #[cfg(test)]
            if let Some(parent_folder) = exe_folder.parent() {
                let bundled_in_parent = parent_folder.join(EXECUTABLE_NAME);
                if bundled_in_parent.exists() && bundled_in_parent.is_file() {
                    debug!("Found bundled ffmpeg in parent dir (test build): {:?}", bundled_in_parent);
                    return Some(bundled_in_parent);
                }
            }
        }
    }

    error!("bundled ffmpeg executable not found next to the application executable");
    None
}
