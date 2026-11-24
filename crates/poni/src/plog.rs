//! Tiny file logger: %LOCALAPPDATA%\Hihu\hihu.log (rotated at 1 MB, at start).
//!
//! The file stays open from `init` on (audit E1): a line is one write, not
//! an open, a write and a close, and some come from the UI thread. Windows
//! still lets the file be read, moved or deleted while it's open.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG: Mutex<Option<File>> = Mutex::new(None);

pub fn init(path: PathBuf) {
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 1_000_000 {
            let _ = fs::rename(&path, path.with_extension("old.log"));
        }
    }
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let file = OpenOptions::new().create(true).append(true).open(path).ok();
    if let Ok(mut g) = LOG.lock() {
        *g = file;
    }
}

pub fn write(level: &str, msg: &str) {
    let t = crate::clock::local_now();
    let line = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} [{level}] {msg}\n",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    if let Ok(mut g) = LOG.lock() {
        if let Some(f) = g.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

#[macro_export]
macro_rules! pinfo {
    ($($t:tt)*) => { $crate::plog::write("INFO", &format!($($t)*)) };
}

#[macro_export]
macro_rules! pwarn {
    ($($t:tt)*) => { $crate::plog::write("WARN", &format!($($t)*)) };
}

#[macro_export]
macro_rules! perror {
    ($($t:tt)*) => { $crate::plog::write("ERROR", &format!($($t)*)) };
}
