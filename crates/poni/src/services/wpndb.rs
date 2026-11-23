//! The sender's picture for a notification card: the toast's own
//! `appLogoOverride` image (WhatsApp's photo of who wrote), which Windows'
//! notification listener doesn't hand over, read from Windows' own
//! notification database (`%LOCALAPPDATA%\Microsoft\Windows\Notifications\
//! wpndatabase.db`) with Windows' own SQLite (`winsqlite3.dll` in System32,
//! loaded the first time it's needed: nothing is added to the exe).
//!
//! Only the one notification the card shows is read, by its id, read-only;
//! only its image's place is taken from it, and only a picture already on
//! the PC is used (one on the internet is left alone). Nothing is written,
//! kept or sent.

use std::ffi::c_void;
use std::sync::OnceLock;
use windows::core::{s, w};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

type Db = *mut c_void;
type Stmt = *mut c_void;

const SQLITE_OK: i32 = 0;
const SQLITE_ROW: i32 = 100;
const SQLITE_OPEN_READONLY: i32 = 0x0000_0001;
/// The largest picture read (bytes): a sender's photo is a few KB.
const MAX_IMAGE: u64 = 4 << 20;

struct Sqlite {
    open: unsafe extern "system" fn(*const u8, *mut Db, i32, *const u8) -> i32,
    prepare: unsafe extern "system" fn(Db, *const u8, i32, *mut Stmt, *mut *const u8) -> i32,
    bind_int64: unsafe extern "system" fn(Stmt, i32, i64) -> i32,
    step: unsafe extern "system" fn(Stmt) -> i32,
    column_blob: unsafe extern "system" fn(Stmt, i32) -> *const c_void,
    column_bytes: unsafe extern "system" fn(Stmt, i32) -> i32,
    finalize: unsafe extern "system" fn(Stmt) -> i32,
    close: unsafe extern "system" fn(Db) -> i32,
}

/// Windows' SQLite, once (None where it isn't there).
fn sqlite() -> Option<&'static Sqlite> {
    static LIB: OnceLock<Option<Sqlite>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let lib = LoadLibraryExW(w!("winsqlite3.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
        macro_rules! f {
            ($name:expr) => {
                std::mem::transmute(GetProcAddress(lib, $name)?)
            };
        }
        Some(Sqlite {
            open: f!(s!("sqlite3_open_v2")),
            prepare: f!(s!("sqlite3_prepare_v2")),
            bind_int64: f!(s!("sqlite3_bind_int64")),
            step: f!(s!("sqlite3_step")),
            column_blob: f!(s!("sqlite3_column_blob")),
            column_bytes: f!(s!("sqlite3_column_bytes")),
            finalize: f!(s!("sqlite3_finalize")),
            close: f!(s!("sqlite3_close")),
        })
    })
    .as_ref()
}

/// Notification `id`'s toast XML, from Windows' database.
fn payload(id: u32) -> Option<String> {
    let sql = sqlite()?;
    let path = std::env::var_os("LOCALAPPDATA")?;
    let path = std::path::Path::new(&path).join("Microsoft\\Windows\\Notifications\\wpndatabase.db");
    let mut cpath = path.to_str()?.as_bytes().to_vec();
    cpath.push(0);
    unsafe {
        let mut db: Db = std::ptr::null_mut();
        if (sql.open)(cpath.as_ptr(), &mut db, SQLITE_OPEN_READONLY, std::ptr::null()) != SQLITE_OK {
            if !db.is_null() {
                (sql.close)(db);
            }
            return None;
        }
        let query = b"SELECT Payload FROM Notification WHERE Id = ?1\0";
        let mut stmt: Stmt = std::ptr::null_mut();
        let mut out = None;
        if (sql.prepare)(db, query.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) == SQLITE_OK && !stmt.is_null() {
            (sql.bind_int64)(stmt, 1, i64::from(id));
            if (sql.step)(stmt) == SQLITE_ROW {
                let p = (sql.column_blob)(stmt, 0) as *const u8;
                let n = (sql.column_bytes)(stmt, 0);
                if !p.is_null() && n > 0 {
                    out = Some(String::from_utf8_lossy(std::slice::from_raw_parts(p, n as usize)).into_owned());
                }
            }
            (sql.finalize)(stmt);
        }
        (sql.close)(db);
        out
    }
}

/// The local file a toast shows as its sender (`appLogoOverride`), if any.
pub fn sender_picture_path(xml: &str) -> Option<std::path::PathBuf> {
    let mut rest = xml;
    while let Some(i) = rest.find("<image") {
        let tag_end = rest[i..].find('>').map(|e| i + e)?;
        let tag = &rest[i..tag_end];
        rest = &rest[tag_end..];
        if !tag.contains("placement=\"appLogoOverride\"") {
            continue;
        }
        let src = attr(tag, "src")?;
        return local_path(&src);
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let i = tag.find(&key)? + key.len();
    let j = tag[i..].find('"')? + i;
    Some(tag[i..j].replace("&amp;", "&").replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">"))
}

/// A picture on this PC: a Windows path, or a file:/// address of one.
/// Internet and app-package addresses are left alone.
fn local_path(src: &str) -> Option<std::path::PathBuf> {
    let s = src.trim();
    let s = if let Some(rest) = s.strip_prefix("file:///") { percent_decode(rest).replace('/', "\\") } else { s.to_string() };
    let b = s.as_bytes();
    let drive = b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
    drive.then(|| std::path::PathBuf::from(s))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Notification `id`'s sender picture, decoded, if it has one on this PC.
pub fn sender_picture(id: u32, px: u32) -> Option<crate::gfx::Image> {
    let path = sender_picture_path(&payload(id)?)?;
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_IMAGE {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    crate::services::imaging::decode_bytes(&bytes, px).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sender_picture_is_the_app_logo_override() {
        let xml = r#"<toast><visual><binding template="ToastGeneric"><text>No Modem</text><image src="C:\Users\A\AppData\Local\Temp\abc.png" placement="appLogoOverride" hint-crop="circle"/><image placement="hero" src="C:\x\hero.png"/></binding></visual></toast>"#;
        assert_eq!(sender_picture_path(xml), Some(std::path::PathBuf::from(r"C:\Users\A\AppData\Local\Temp\abc.png")));
        let file = r#"<image placement="appLogoOverride" src="file:///C:/Users/A%20B/p.png"/>"#;
        assert_eq!(sender_picture_path(file), Some(std::path::PathBuf::from(r"C:\Users\A B\p.png")));
        // On the internet, or none: nothing.
        assert_eq!(sender_picture_path(r#"<image placement="appLogoOverride" src="https://x.y/p.png"/>"#), None);
        assert_eq!(sender_picture_path(r#"<image placement="hero" src="C:\x.png"/>"#), None);
    }
}
