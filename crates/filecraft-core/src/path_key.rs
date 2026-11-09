use std::path::Path;

/// Returns a comparison key for a path or folder name.
///
/// Windows file systems are case-insensitive, so keys are lowercased there. Other platforms
/// compare paths exactly.
pub fn path_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.into_owned()
    }
}

/// Returns a case-insensitive key for a folder name, used for ignored folder names on every platform.
pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}
