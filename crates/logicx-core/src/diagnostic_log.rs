//! Persistent diagnostic logs under Application Support (survive plugin reloads / crashes).

use crate::runtime::support_dir;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const PLUGIN_LOG: &str = "plugin.log";
const MAX_PLUGIN_LOG_BYTES: u64 = 512 * 1024;

pub fn plugin_log_path() -> PathBuf {
    support_dir().join(PLUGIN_LOG)
}

pub fn bridge_log_path() -> PathBuf {
    support_dir().join("bridge.log")
}

pub fn append_plugin_log(line: impl AsRef<str>) {
    append_line(
        &plugin_log_path(),
        line.as_ref(),
        Some(MAX_PLUGIN_LOG_BYTES),
    );
}

pub fn append_bridge_log(line: impl AsRef<str>) {
    append_line(&bridge_log_path(), line.as_ref(), None);
}

pub fn read_plugin_log_tail(max_bytes: usize) -> String {
    read_tail(&plugin_log_path(), max_bytes)
}

fn append_line(path: &PathBuf, line: &str, rotate_at: Option<u64>) {
    let _ = fs::create_dir_all(support_dir());
    if let Some(limit) = rotate_at {
        trim_if_large(path, limit);
    }
    let ts = timestamp();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{ts}] {line}");
    }
}

fn trim_if_large(path: &PathBuf, max_bytes: u64) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() <= max_bytes {
        return;
    }
    let keep = (max_bytes / 2) as usize;
    let tail = read_tail(path, keep);
    let _ = fs::write(path, tail);
}

fn read_tail(path: &PathBuf, max_bytes: usize) -> String {
    let Ok(data) = fs::read(path) else {
        return String::new();
    };
    if data.len() <= max_bytes {
        return String::from_utf8_lossy(&data).into_owned();
    }
    String::from_utf8_lossy(&data[data.len() - max_bytes..]).into_owned()
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("{}.{:03}", d.as_secs(), d.subsec_millis()))
        .unwrap_or_else(|_| "?".into())
}

/// Log panics to plugin.log (survives AU reload).
pub fn install_panic_hook(label: &str) {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    let label = label.to_string();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(move |info| {
            append_plugin_log(format!("PANIC [{label}]: {info}"));
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tail_missing_file_is_empty() {
        let path = plugin_log_path().with_file_name("logicx-mcp-test-missing.log");
        let _ = fs::remove_file(&path);
        assert!(read_tail(&path, 1024).is_empty());
    }
}
