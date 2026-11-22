//! Opening a downloaded attachment with the system's default application, and noticing when that fails.
//!
//! Linux and macOS run `xdg-open` / `open` ourselves and look at the exit code (the generic "open" helpers return as
//! soon as the process starts, so a missing handler would be silent). Windows uses ShellExecute through the opener
//! plugin. When nothing opens the file the UI shows a small dialog: Open with, Save as, Show in folder.

use serde::Serialize;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What `open_attachment` reports to the UI. `opened == false` means "show the fallback dialog".
#[derive(Debug, Clone, Serialize)]
pub struct OpenOutcome {
    pub opened: bool,
    /// The cached copy that was (or should have been) opened.
    pub path: String,
    /// Why it did not open (empty when it did).
    pub reason: String,
}

/// Runs `program <path>` and waits up to `wait` for it to finish. Exit 0 or still running after `wait` (the handler is
/// an app that stays open) counts as opened; a non-zero exit (xdg-open uses 3 for "no handler") or a spawn error is a failure.
pub fn run_opener(program: &OsStr, args: &[&OsStr], wait: Duration) -> Result<(), String> {
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{} is not installed.", program.to_string_lossy())
        } else {
            format!("Could not start {}: {e}", program.to_string_lossy())
        }
    })?;
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => {
                let mut err = String::new();
                if let Some(mut se) = child.stderr.take() {
                    use std::io::Read;
                    let _ = se.read_to_string(&mut err);
                }
                let err = err.trim();
                return Err(if err.is_empty() { format!("No application is set to open this kind of file (exit code {}).", st.code().unwrap_or(-1)) } else { err.to_string() });
            }
            Ok(None) if t0.elapsed() >= wait => {
                // Still running: a viewer that stays in the foreground. Reap it in the background.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// The default-app command for this OS (`xdg-open`, `open`); `None` on Windows and Android.
pub fn default_opener() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        Some("xdg-open")
    } else if cfg!(target_os = "macos") {
        Some("open")
    } else {
        None
    }
}

/// Opens `path` with the default app. Err carries a reason fit to show.
pub fn open_default(path: &Path) -> Result<(), String> {
    match default_opener() {
        Some(p) => run_opener(OsStr::new(p), &[path.as_os_str()], Duration::from_millis(2500)),
        None => Err("not supported here".into()),
    }
}

/// Opens `file` with a chosen program (`app` is an executable on Linux, an .app bundle on macOS).
pub fn open_with(app: &Path, file: &Path) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        run_opener(OsStr::new("open"), &[OsStr::new("-a"), app.as_os_str(), file.as_os_str()], Duration::from_millis(2500))
    } else {
        run_opener(app.as_os_str(), &[file.as_os_str()], Duration::from_millis(1500))
    }
}

/// Windows "Open with" chooser (the system dialog) for `file`.
#[cfg(windows)]
pub fn windows_open_as(file: &Path) -> Result<(), String> {
    Command::new("rundll32.exe").arg("shell32.dll,OpenAs_RunDLL").arg(file).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn stub(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn a_handler_that_succeeds_gets_the_file_path() {
        let d = tempfile::tempdir().unwrap();
        let rec = d.path().join("rec.txt");
        let s = stub(d.path(), "xdg-open", &format!("echo \"$1\" > '{}'", rec.display()));
        let file = d.path().join("report.pdf");
        run_opener(s.as_os_str(), &[file.as_os_str()], Duration::from_secs(3)).unwrap();
        assert_eq!(std::fs::read_to_string(&rec).unwrap().trim(), file.display().to_string());
    }

    #[test]
    fn exit_code_3_means_no_handler_and_is_reported() {
        let d = tempfile::tempdir().unwrap();
        let s = stub(d.path(), "xdg-open", "echo 'no method available for opening' >&2; exit 3");
        let e = run_opener(s.as_os_str(), &[OsStr::new("/x.docx")], Duration::from_secs(3)).unwrap_err();
        assert!(e.contains("no method"), "{e}");
        let s = stub(d.path(), "silent", "exit 3");
        assert!(run_opener(s.as_os_str(), &[OsStr::new("/x")], Duration::from_secs(3)).unwrap_err().contains("exit code 3"));
    }

    #[test]
    fn a_missing_program_is_an_error_not_silence() {
        let e = run_opener(OsStr::new("/nonexistent/xdg-open"), &[OsStr::new("/x")], Duration::from_secs(1)).unwrap_err();
        assert!(e.contains("not installed"), "{e}");
    }

    #[test]
    fn a_viewer_that_keeps_running_counts_as_opened() {
        let d = tempfile::tempdir().unwrap();
        let s = stub(d.path(), "viewer", "sleep 3");
        let t = Instant::now();
        run_opener(s.as_os_str(), &[OsStr::new("/x")], Duration::from_millis(300)).unwrap();
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
