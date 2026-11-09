//! Operating system integration that differs between platforms.

use std::io;
use std::path::Path;
use std::process::Command;

/// Opens a folder in the platform file manager.
pub fn open_folder(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";

    // The file manager keeps running on its own, so the child is not awaited.
    Command::new(program).arg(path).spawn().map(drop)
}
