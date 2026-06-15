//! File-based logging setup.
//!
//! The TUI owns the terminal (raw mode + alternate screen), so tracing must
//! not write to stdout/stderr — that would corrupt the rendered UI. Instead we
//! append to a log file under the XDG state directory.

use std::path::PathBuf;

use tracing_subscriber::EnvFilter;

/// Log file location, derived from `xdg_state_home` (`$XDG_STATE_HOME`) or
/// `home` (`$HOME`). Pure, so it is unit-testable. Returns `None` when neither
/// is available (no sensible place to write).
fn log_file_path_from(xdg_state_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match (xdg_state_home, home) {
        (Some(x), _) if !x.is_empty() => PathBuf::from(x),
        (_, Some(h)) if !h.is_empty() => PathBuf::from(h).join(".local/state"),
        _ => return None,
    };
    Some(base.join("zapret2-tui").join("zapret2-tui.log"))
}

/// Resolve the log file path from the current environment.
pub fn log_file_path() -> Option<PathBuf> {
    log_file_path_from(
        std::env::var("XDG_STATE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Initialize tracing to write to the log file. Returns the path on success.
///
/// On any failure (no path, cannot create dir/file) logging is left
/// uninitialized rather than falling back to stderr, which would corrupt the
/// TUI. The error is returned so the caller can warn before taking the screen.
pub fn init() -> Result<PathBuf, String> {
    let path = log_file_path().ok_or_else(|| "no XDG_STATE_HOME or HOME set".to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(filter)
        .with_writer(std::sync::Mutex::new(file))
        .init();

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_xdg_state_home() {
        let p = log_file_path_from(Some("/var/lib/state"), Some("/home/u")).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/var/lib/state/zapret2-tui/zapret2-tui.log")
        );
    }

    #[test]
    fn falls_back_to_home_local_state() {
        let p = log_file_path_from(None, Some("/home/u")).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/home/u/.local/state/zapret2-tui/zapret2-tui.log")
        );
    }

    #[test]
    fn empty_xdg_state_home_falls_back_to_home() {
        let p = log_file_path_from(Some(""), Some("/home/u")).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/home/u/.local/state/zapret2-tui/zapret2-tui.log")
        );
    }

    #[test]
    fn none_when_no_env() {
        assert!(log_file_path_from(None, None).is_none());
        assert!(log_file_path_from(Some(""), Some("")).is_none());
    }
}
