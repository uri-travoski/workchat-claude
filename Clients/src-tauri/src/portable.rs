//! Windows portable mode: next to the exe, `WorkChat-data` holds everything, so the folder can move between PCs.

use std::path::{Path, PathBuf};

pub const DATA_FOLDER: &str = "WorkChat-data";

/// The data folder for a portable run, or `None` for a normal (installed) run. Portable means the exe's file name
/// contains "portable" (any case) or a `portable.txt` file sits next to it.
pub fn portable_dir(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    let name = exe.file_name()?.to_string_lossy().to_ascii_lowercase();
    if name.contains("portable") || parent.join("portable.txt").exists() {
        Some(parent.join(DATA_FOLDER))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_name_or_marker_file() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("workchat.exe");
        assert_eq!(portable_dir(&exe), None);
        assert_eq!(portable_dir(&d.path().join("WorkChat-1.1.0-windows-x64-Portable.exe")), Some(d.path().join("WorkChat-data")));
        std::fs::write(d.path().join("portable.txt"), "").unwrap();
        assert_eq!(portable_dir(&exe), Some(d.path().join("WorkChat-data")));
    }
}
