//! Tiaki Files — the native file manager.
//!
//! Tabs, places sidebar, sortable list, preview pane, search, and real
//! operations: rename, new folder/file, copy/cut/paste, trash, restore,
//! empty trash, bulk rename, compress/extract (tar), properties with
//! permissions. Complaints about other managers, answered on purpose:
//!
//! - Finder has no cut and hides the path: cut exists here, and the path is
//!   always on screen as a breadcrumb (click to edit, button to copy it).
//! - Nobody undoes file operations: every mutation journals an inverse.
//!   `Ctrl+Z` undoes, `Ctrl+Shift+Z` redoes, per window.
//! - Sorting that resets: sort column and direction persist in
//!   `~/.config/tiaki/`.
//! - Modal spam on conflicts: copies and moves keep both sides with
//!   `name (2)` and say so in the status line instead of asking.
//! - Trash with no way back: restore per item, undo restores, empty asks.
//!
//! The cross-window file clipboard lives in a runtime file (windows are
//! separate processes); the undo journal is per window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::os::unix::fs::PermissionsExt;

use crate::{glyphs, system, theme};

pub const STYLE_SHEETS: &[&str] = &[include_str!("../css/files.css")];

// ---------------------------------------------------------------------------
// Small filesystem helpers. std only: the shell adds no fs crates.

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/desktop"))
}

fn trash_base() -> PathBuf {
    home_dir().join(".local/share/Trash")
}

fn trash_files() -> PathBuf {
    trash_base().join("files")
}

fn trash_info_dir() -> PathBuf {
    trash_base().join("info")
}

fn ensure_trash() -> std::io::Result<()> {
    std::fs::create_dir_all(trash_files())?;
    std::fs::create_dir_all(trash_info_dir())
}

fn runtime_file(name: &str) -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(base).join(name)
}

fn config_file(name: &str) -> PathBuf {
    home_dir().join(".config/tiaki").join(name)
}

fn read_pref(name: &str) -> Option<String> {
    std::fs::read_to_string(config_file(name))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn write_pref(name: &str, value: &str) {
    let path = config_file(name);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, value);
}

fn fmt_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// Days since the Unix epoch to a Gregorian date (Howard Hinnant's civil
/// algorithm, `z` shifted from 1970-01-01 to the 0000-03-01 civil epoch):
/// no date crates needed for the Modified column.
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let doe = (shifted - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

fn fmt_time(time: std::time::SystemTime) -> String {
    // Signed seconds: duration_since fails before the epoch, and collapsing
    // those files to 1970-01-01 would be a lie.
    let secs = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(delta) => delta.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    };
    // For negative remainders, rem_euclid keeps the clock face correct.
    let days = secs.div_euclid(86_400);
    let (year, month, day) = days_to_ymd(days);
    let rest = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        (rest / 60) % 60
    )
}

fn fmt_free(path: &Path) -> String {
    let output = std::process::Command::new("df")
        .args(["-k", "--output=avail"])
        .arg(path)
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .nth(1)
            .and_then(|line| line.trim().parse::<u64>().ok())
            .map(|kb| fmt_size(kb * 1024))
            .unwrap_or_else(|| "—".to_string()),
        _ => "—".to_string(),
    }
}

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "markdown", "rs", "toml", "json", "log", "sh", "py", "js", "ts",
    "tsx", "css", "html", "xml", "ini", "conf", "cfg", "yaml", "yml", "c", "h",
    "cpp", "go", "java", "csv", "srt", "tex", "diff", "patch",
];

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "svg", "webp"];

fn ext_of(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_lowercase()
}

fn is_text_file(path: &Path) -> bool {
    if TEXT_EXTS.contains(&ext_of(&name_of(path)).as_str()) {
        return true;
    }
    // Sniff: no NUL byte in the first block means text.
    std::fs::File::open(path)
        .and_then(|mut file| {
            use std::io::Read;
            let mut buf = [0u8; 512];
            let count = file.read(&mut buf)?;
            Ok(!buf[..count].contains(&0))
        })
        .unwrap_or(false)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// A non-destructive unique sibling: `report.txt` → `report (2).txt`.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    let mut index = 2;
    loop {
        let candidate = dir.join(format!("{stem} ({index}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn copy_recursive(source: &Path, dest: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(source)?;
        std::os::unix::fs::symlink(target, dest)?;
    } else if meta.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(source, dest)?;
    }
    Ok(())
}

fn remove_all(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Byte + file totals for progress. Capped so a huge tree cannot stall the
/// estimate pass.
fn walk_totals(root: &Path) -> (u64, u64) {
    fn visit(path: &Path, depth: usize, bytes: &mut u64, files: &mut u64) {
        if depth > 16 || *files > 50_000 {
            return;
        }
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        if meta.is_dir() && !meta.file_type().is_symlink() {
            *files += 1;
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    visit(&entry.path(), depth + 1, bytes, files);
                }
            }
        } else {
            *files += 1;
            *bytes += meta.len();
        }
    }
    let (mut bytes, mut files) = (0, 0);
    visit(root, 0, &mut bytes, &mut files);
    (bytes, files)
}

// ---------------------------------------------------------------------------
// Trash (freedesktop spec, minimal): files/ plus info/*.trashinfo.

fn trash_name_for(path: &Path) -> String {
    let name = name_of(path);
    let candidate = trash_files().join(&name);
    if !candidate.exists() && !trash_info_dir().join(format!("{name}.trashinfo")).exists() {
        return name;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (name[..dot].to_string(), name[dot..].to_string()),
        _ => (name.clone(), String::new()),
    };
    let mut index = 2;
    loop {
        let name = format!("{stem} ({index}){ext}");
        if !trash_files().join(&name).exists() {
            return name;
        }
        index += 1;
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn deletion_stamp() -> String {
    // Genuine ISO-8601 from the same civil algorithm as the Modified column.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|delta| delta.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = days_to_ymd(now / 86_400);
    let rest = now.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        rest / 3600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// Move `path` into the trash. Returns the trashed file name.
fn trash_item(path: &Path) -> std::io::Result<String> {
    ensure_trash()?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let name = trash_name_for(&absolute);
    std::fs::rename(&absolute, trash_files().join(&name))?;
    let info = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode(&absolute.to_string_lossy()),
        deletion_stamp()
    );
    std::fs::write(trash_info_dir().join(format!("{name}.trashinfo")), info)?;
    Ok(name)
}

struct TrashedEntry {
    name: String,
    original: PathBuf,
    deleted: String,
}

fn read_trash() -> Vec<TrashedEntry> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(trash_files()) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let info_path = trash_info_dir().join(format!("{name}.trashinfo"));
        let (mut original, mut deleted) = (PathBuf::from(&name), String::new());
        if let Ok(text) = std::fs::read_to_string(&info_path) {
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("Path=") {
                    original = PathBuf::from(percent_decode(rest));
                } else if let Some(rest) = line.strip_prefix("DeletionDate=") {
                    deleted = rest.to_string();
                }
            }
        }
        out.push(TrashedEntry {
            name,
            original,
            deleted,
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

// ---------------------------------------------------------------------------
// Cross-window file clipboard: windows are separate processes, so the
// clipboard is a small runtime file, not memory.

#[derive(Clone)]
struct FileClipboard {
    cut: bool,
    paths: Vec<PathBuf>,
}

fn clipboard_path() -> PathBuf {
    runtime_file("tiaki-files-clipboard")
}

fn read_clipboard() -> Option<FileClipboard> {
    let text = std::fs::read_to_string(clipboard_path()).ok()?;
    let mut lines = text.lines();
    let cut = lines.next()? == "cut";
    let paths: Vec<PathBuf> = lines.map(PathBuf::from).filter(|p| p.exists()).collect();
    if paths.is_empty() {
        return None;
    }
    Some(FileClipboard { cut, paths })
}

fn write_clipboard(clipboard: &FileClipboard) {
    let mut text = if clipboard.cut { "cut\n" } else { "copy\n" }.to_string();
    for path in &clipboard.paths {
        text.push_str(&path.to_string_lossy());
        text.push('\n');
    }
    let _ = std::fs::write(clipboard_path(), text);
}

fn clear_clipboard() {
    let _ = std::fs::remove_file(clipboard_path());
}

// ---------------------------------------------------------------------------
// Undo journal. Every mutation records its inverse; Ctrl+Z / Ctrl+Shift+Z
// walk the two stacks. Trashed items restore; copies delete; moves move back.

enum Op {
    Rename { from: PathBuf, to: PathBuf },
    Mkdir { path: PathBuf },
    Mkfile { path: PathBuf },
    Create { paths: Vec<PathBuf> },
    Move { moved: Vec<(PathBuf, PathBuf)> },
    Trash { items: Vec<(PathBuf, String)> },
    Restore { items: Vec<(PathBuf, String)> },
    Perm { path: PathBuf, old: u32, new: u32 },
}

impl Op {
    fn undo(&self) -> std::io::Result<String> {
        match self {
            Op::Rename { from, to } => {
                std::fs::rename(to, from)?;
                Ok(format!("Renamed back to {}", name_of(from)))
            }
            Op::Mkdir { path } | Op::Mkfile { path } => {
                remove_all(path)?;
                Ok(format!("Removed {}", name_of(path)))
            }
            Op::Create { paths } => {
                for path in paths {
                    let _ = remove_all(path);
                }
                Ok("Removed created items".to_string())
            }
            Op::Move { moved } => {
                for (from, to) in moved.iter().rev() {
                    let dest = if from.exists() {
                        unique_path(
                            from.parent().unwrap_or_else(|| Path::new("/")),
                            &name_of(from),
                        )
                    } else {
                        from.clone()
                    };
                    std::fs::rename(to, &dest)?;
                }
                Ok("Moved back".to_string())
            }
            Op::Trash { items } => {
                let mut count = 0;
                for (original, name) in items {
                    let trashed = trash_files().join(name);
                    if !trashed.exists() {
                        continue;
                    }
                    let dest = unique_path(
                        original.parent().unwrap_or_else(|| Path::new("/")),
                        &name_of(original),
                    );
                    std::fs::rename(&trashed, &dest)?;
                    let _ =
                        std::fs::remove_file(trash_info_dir().join(format!("{name}.trashinfo")));
                    count += 1;
                }
                Ok(format!("Restored {count} item(s)"))
            }
            Op::Restore { items } => {
                for (original, _) in items {
                    let _ = trash_item(original);
                }
                Ok("Moved back to trash".to_string())
            }
            Op::Perm { path, old, .. } => {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(*old))?;
                Ok("Permissions restored".to_string())
            }
        }
    }

    fn redo(&self) -> std::io::Result<String> {
        match self {
            Op::Rename { from, to } => {
                std::fs::rename(from, to)?;
                Ok(format!("Renamed to {}", name_of(to)))
            }
            Op::Mkdir { path } => {
                std::fs::create_dir_all(path)?;
                Ok(format!("Recreated {}", name_of(path)))
            }
            Op::Mkfile { path } => {
                std::fs::write(path, "")?;
                Ok(format!("Recreated {}", name_of(path)))
            }
            Op::Create { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "re-creation needs its sources",
            )),
            Op::Move { moved } => {
                for (from, to) in moved {
                    if from.exists() {
                        let dest = unique_path(
                            to.parent().unwrap_or_else(|| Path::new("/")),
                            &name_of(to),
                        );
                        std::fs::rename(from, &dest)?;
                    }
                }
                Ok("Moved again".to_string())
            }
            Op::Trash { items } => {
                for (original, _) in items {
                    if original.exists() {
                        let _ = trash_item(original);
                    }
                }
                Ok("Trashed again".to_string())
            }
            Op::Restore { items } => {
                let mut count = 0;
                for (original, name) in items {
                    let trashed = trash_files().join(name);
                    if trashed.exists() {
                        let dest = unique_path(
                            original.parent().unwrap_or_else(|| Path::new("/")),
                            &name_of(original),
                        );
                        std::fs::rename(&trashed, &dest)?;
                        count += 1;
                    }
                }
                Ok(format!("Restored {count} item(s)"))
            }
            Op::Perm { path, new, .. } => {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(*new))?;
                Ok("Permissions reapplied".to_string())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Browser state. One struct per window; tabs share the single tree view and
// the toolbar reads the active tab.

#[derive(Clone, PartialEq)]
enum Location {
    Dir(PathBuf),
    Trash,
    Search { root: PathBuf, query: String },
}

impl Location {
    fn title(&self) -> String {
        match self {
            Location::Dir(path) if *path == home_dir() => "Home".to_string(),
            Location::Dir(path) => name_of(path),
            Location::Trash => "Trash".to_string(),
            Location::Search { query, .. } => format!("Search: {query}"),
        }
    }

    /// Where pasted and created items land. Search results paste into the
    /// searched root; trash refuses pastes.
    fn drop_dir(&self) -> Option<PathBuf> {
        match self {
            Location::Dir(path) => Some(path.clone()),
            Location::Search { root, .. } => Some(root.clone()),
            Location::Trash => None,
        }
    }
}

#[derive(Clone)]
struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_link: bool,
    len: u64,
    mtime: String,
    mtime_secs: u64,
    /// Trash only: where it came from + when it was deleted.
    origin: String,
}

fn entry_from_path(path: &Path) -> Option<Entry> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let is_link = meta.file_type().is_symlink();
    let target_meta = std::fs::metadata(path).ok();
    let is_dir = target_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let len = if is_dir {
        0
    } else {
        target_meta.map(|m| m.len()).unwrap_or(meta.len())
    };
    let mtime = meta
        .modified()
        .map(fmt_time)
        .unwrap_or_else(|_| "—".to_string());
    let mtime_secs = meta
        .modified()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "time")))
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(Entry {
        name: name_of(path),
        path: path.to_path_buf(),
        is_dir,
        is_link,
        len,
        mtime,
        mtime_secs,
        origin: String::new(),
    })
}

fn list_dir(dir: &Path, show_hidden: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        if let Some(item) = entry_from_path(&entry.path()) {
            out.push(item);
        }
    }
    out
}

fn list_trash() -> Vec<Entry> {
    read_trash()
        .into_iter()
        .map(|trashed| {
            let path = trash_files().join(&trashed.name);
            let mut entry = entry_from_path(&path).unwrap_or(Entry {
                name: trashed.name.clone(),
                path,
                is_dir: false,
                is_link: false,
                len: 0,
                mtime: String::new(),
                mtime_secs: 0,
                origin: String::new(),
            });
            entry.origin = trashed.original.to_string_lossy().into_owned();
            if !trashed.deleted.is_empty() {
                entry.mtime = trashed.deleted.replace('T', " ");
            }
            entry
        })
        .collect()
}

fn list_search(root: &Path, query: &str, show_hidden: bool) -> Vec<Entry> {
    fn visit(dir: &Path, needle: &str, show_hidden: bool, out: &mut Vec<Entry>, depth: usize) {
        if depth > 12 || out.len() >= 3000 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if name.to_lowercase().contains(needle) {
                if let Some(item) = entry_from_path(&path) {
                    out.push(item);
                    if out.len() >= 3000 {
                        return;
                    }
                }
            }
            // Recurse into real directories only: never follow symlinks.
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                if meta.is_dir() && !meta.file_type().is_symlink() {
                    visit(&path, needle, show_hidden, out, depth + 1);
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(root, &query.to_lowercase(), show_hidden, &mut out, 0);
    out
}

// ListStore columns: name, size text, size bytes, mtime text, mtime secs,
// path, is dir, origin.
const COL_NAME: i32 = 0;
const COL_SIZE_TEXT: i32 = 1;
const COL_SIZE_BYTES: i32 = 2;
const COL_MTIME_TEXT: i32 = 3;
const COL_MTIME_SECS: i32 = 4;
const COL_PATH: i32 = 5;
const COL_IS_DIR: i32 = 6;
const COL_ORIGIN: i32 = 7;

struct Tab {
    history: Vec<Location>,
    index: usize,
}

impl Tab {
    fn new(location: Location) -> Self {
        Tab {
            history: vec![location],
            index: 0,
        }
    }

    fn current(&self) -> Location {
        self.history[self.index].clone()
    }

    fn go(&mut self, location: Location) {
        self.history.truncate(self.index + 1);
        self.history.push(location);
        self.index += 1;
    }

    fn back(&mut self) -> bool {
        if self.index > 0 {
            self.index -= 1;
            true
        } else {
            false
        }
    }

    fn forward(&mut self) -> bool {
        if self.index + 1 < self.history.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

struct Browser {
    window: gtk::Window,
    store: gtk::ListStore,
    tree: gtk::TreeView,
    preview_box: gtk::Box,
    preview_slot: gtk::Box,
    preview_title: gtk::Label,
    preview_meta: gtk::Label,
    preview_text: gtk::TextView,
    preview_image: gtk::Image,
    status: gtk::Label,
    progress: gtk::ProgressBar,
    progress_cancel: gtk::Button,
    crumb_box: gtk::Box,
    path_entry: gtk::Entry,
    path_toggle: gtk::Button,
    search: gtk::Entry,
    tab_strip: gtk::Box,
    side_list: gtk::Box,
    back_btn: gtk::Button,
    fwd_btn: gtk::Button,
    up_btn: gtk::Button,
    new_tab_btn: gtk::Button,
    new_folder_btn: gtk::Button,
    new_file_btn: gtk::Button,
    paste_btn: gtk::Button,
    undo_btn: gtk::Button,
    redo_btn: gtk::Button,
    hidden_btn: gtk::ToggleButton,
    preview_btn: gtk::ToggleButton,
    empty_trash_btn: gtk::Button,
    restore_btn: gtk::Button,
    tabs: Vec<Tab>,
    active: usize,
    sort_col: i32,
    sort_asc: bool,
    show_hidden: bool,
    journal: Vec<Op>,
    redo: Vec<Op>,
    cancel: Arc<AtomicBool>,
    note: String,
}

type Shared = Rc<RefCell<Browser>>;

fn glyph_for(entry: &Entry) -> &'static str {
    if entry.is_dir {
        "folder"
    } else {
        match ext_of(&entry.name).as_str() {
            ext if IMAGE_EXTS.contains(&ext) => "image",
            "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" => "archive",
            _ if is_text_file(&entry.path) => "file",
            _ => "file",
        }
    }
}

fn new_tab_button(browser: &Shared, index: usize, title: &str, active: bool) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    theme::class(&button, "files-tab");
    if active {
        button.style_context().add_class("active");
    }
    let label = gtk::Label::new(Some(title));
    label.set_max_width_chars(18);
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    button.add(&label);
    let capture = browser.clone();
    button.connect_clicked(move |_| {
        capture.borrow_mut().switch_tab(index);
    });
    let close = gtk::Button::new();
    close.set_relief(gtk::ReliefStyle::None);
    theme::class(&close, "files-tabclose");
    close.add(&glyphs::status_icon_tinted("x", 10, (0.56, 0.54, 0.51)));
    let capture = browser.clone();
    close.connect_clicked(move |_| {
        capture.borrow_mut().close_tab(index);
    });
    row.pack_start(&button, false, false, 0);
    row.pack_start(&close, false, false, 0);
    row
}

impl Browser {
    fn location(&self) -> Location {
        self.tabs[self.active].current()
    }

    fn navigate(&mut self, location: Location) {
        self.tabs[self.active].go(location);
        self.refresh();
    }

    fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.refresh();
        }
    }

    fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(index);
        if self.active >= index && self.active > 0 {
            self.active -= 1;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        self.refresh();
    }

    fn new_tab(&mut self, location: Location) {
        self.tabs.push(Tab::new(location));
        self.active = self.tabs.len() - 1;
        self.refresh();
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        let selection = self.tree.selection();
        let (paths, model) = selection.selected_rows();
        paths
            .iter()
            .filter_map(|path| model.iter(path))
            .filter_map(|iter| {
                model
                    .value(&iter, COL_PATH)
                    .get::<String>()
                    .ok()
                    .map(PathBuf::from)
            })
            .collect()
    }

    fn selected_entries(&self) -> Vec<Entry> {
        let selection = self.tree.selection();
        let (rows, model) = selection.selected_rows();
        rows.iter()
            .filter_map(|row| model.iter(row))
            .map(|iter| {
                let get = |col: i32| {
                    model
                        .value(&iter, col)
                        .get::<String>()
                        .unwrap_or_default()
                };
                Entry {
                    name: get(COL_NAME),
                    path: PathBuf::from(get(COL_PATH)),
                    is_dir: model
                        .value(&iter, COL_IS_DIR)
                        .get::<bool>()
                        .unwrap_or(false),
                    is_link: false,
                    len: model
                        .value(&iter, COL_SIZE_BYTES)
                        .get::<u64>()
                        .unwrap_or(0),
                    mtime: get(COL_MTIME_TEXT),
                    mtime_secs: model
                        .value(&iter, COL_MTIME_SECS)
                        .get::<u64>()
                        .unwrap_or(0),
                    origin: get(COL_ORIGIN),
                }
            })
            .collect()
    }

    fn set_note(&mut self, note: impl Into<String>) {
        self.note = note.into();
        self.update_status();
    }

    fn do_undo(&mut self) {
        let Some(op) = self.journal.pop() else {
            self.set_note("Nothing to undo");
            return;
        };
        match op.undo() {
            Ok(note) => {
                self.redo.push(op);
                self.set_note(&note);
            }
            Err(error) => {
                let note = format!("Undo failed: {error}");
                self.set_note(note)
            }
        }
        self.refresh();
    }

    fn do_redo(&mut self) {
        let Some(op) = self.redo.pop() else {
            self.set_note("Nothing to redo");
            return;
        };
        match op.redo() {
            Ok(note) => {
                self.journal.push(op);
                self.set_note(&note);
            }
            Err(error) => {
                let note = format!("Redo failed: {error}");
                self.set_note(note)
            }
        }
        self.refresh();
    }

    fn current_entries(&self) -> Vec<Entry> {
        let mut entries = match self.location() {
            Location::Dir(path) => list_dir(&path, self.show_hidden),
            Location::Trash => list_trash(),
            Location::Search { root, query } => list_search(&root, &query, self.show_hidden),
        };
        entries.sort_by(|a, b| {
            let dirs_first = b.is_dir.cmp(&a.is_dir);
            if dirs_first != std::cmp::Ordering::Equal {
                return dirs_first;
            }
            let order = match self.sort_col {
                COL_SIZE_BYTES => a.len.cmp(&b.len),
                COL_MTIME_SECS => a.mtime_secs.cmp(&b.mtime_secs),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            };
            if self.sort_asc {
                order
            } else {
                order.reverse()
            }
        });
        entries
    }

    fn refresh(&mut self) {
        self.preview_box.set_visible(self.preview_btn.is_active());
        // Tabs.
        for child in self.tab_strip.children() {
            self.tab_strip.remove(&child);
        }
        for (index, tab) in self.tabs.iter().enumerate() {
            self.tab_strip.pack_start(
                &new_tab_button(&self_rc_placeholder(), index, &tab.current().title(), index == self.active),
                false,
                false,
                0,
            );
        }
        self.tab_strip.show_all();
        // One tab needs no tab strip; it would just repeat the breadcrumb.
        if let Some(row) = self.tab_strip.parent() {
            row.set_visible(self.tabs.len() > 1);
        }

        // Breadcrumb / path.
        for child in self.crumb_box.children() {
            self.crumb_box.remove(&child);
        }
        match self.location() {
            Location::Dir(path) => {
                let accumulated = PathBuf::from("/");
                let mut parts: Vec<String> = Vec::new();
                for component in path.components() {
                    use std::path::Component;
                    match component {
                        Component::RootDir => {}
                        Component::Normal(part) => {
                            parts.push(part.to_string_lossy().into_owned())
                        }
                        _ => {}
                    }
                }
                let home = home_dir();
                let home_str = home.to_string_lossy().into_owned();
                let path_str = path.to_string_lossy().into_owned();
                let display: Vec<(String, PathBuf)> = if path_str == home_str {
                    vec![("Home".to_string(), home.clone())]
                } else if let Some(rest) = path_str.strip_prefix(&format!("{home_str}/")) {
                    let mut items = vec![("Home".to_string(), home.clone())];
                    let mut current = home.clone();
                    for part in rest.split('/') {
                        current = current.join(part);
                        items.push((part.to_string(), current.clone()));
                    }
                    items
                } else {
                    let mut items = vec![("/".to_string(), PathBuf::from("/"))];
                    let mut current = PathBuf::from("/");
                    for part in parts {
                        current = current.join(&part);
                        items.push((part, current.clone()));
                    }
                    items
                };
                let _ = accumulated;
                for (index, (label, target)) in display.iter().enumerate() {
                    let last = index + 1 == display.len();
                    let button = gtk::Button::with_label(label);
                    button.set_relief(gtk::ReliefStyle::None);
                    theme::class(&button, "files-crumb");
                    if last {
                        button.style_context().add_class("current");
                    }
                    let browser = self_rc_placeholder();
                    let target = target.clone();
                    button.connect_clicked(move |_| {
                        browser.borrow_mut().navigate(Location::Dir(target.clone()));
                    });
                    self.crumb_box.pack_start(&button, false, false, 0);
                }
                self.path_entry.set_text(&path_str);
            }
            Location::Trash => {
                let label = gtk::Label::new(Some("Trash"));
                theme::class(&label, "files-crumb");
                label.style_context().add_class("current");
                self.crumb_box.pack_start(&label, false, false, 0);
                self.path_entry.set_text("trash:");
            }
            Location::Search { ref query, .. } => {
                let label = gtk::Label::new(Some(&format!("Search: {query}")));
                theme::class(&label, "files-crumb");
                label.style_context().add_class("current");
                self.crumb_box.pack_start(&label, false, false, 0);
                self.search.set_text(query);
            }
        }
        if !matches!(self.location(), Location::Search { .. }) && self.search.text().as_str() != "" {
            // Search text belongs to a previous search; clear it.
            self.search.set_text("");
        }
        self.crumb_box.show_all();

        // List.
        self.store.clear();
        let in_trash = matches!(self.location(), Location::Trash);
        for entry in self.current_entries() {
            let size = if entry.is_dir && !in_trash {
                "—".to_string()
            } else if entry.is_dir {
                fmt_size(entry.len)
            } else {
                fmt_size(entry.len)
            };
            self.store.set(
                &self.store.append(),
                &[
                    (COL_NAME as u32, &entry.name),
                    (COL_SIZE_TEXT as u32, &size),
                    (COL_SIZE_BYTES as u32, &entry.len),
                    (COL_MTIME_TEXT as u32, &entry.mtime),
                    (COL_MTIME_SECS as u32, &entry.mtime_secs),
                    (
                        COL_PATH as u32,
                        &entry.path.to_string_lossy().into_owned(),
                    ),
                    (COL_IS_DIR as u32, &entry.is_dir),
                    (COL_ORIGIN as u32, &entry.origin),
                ],
            );
        }

        // Nav buttons.
        let can_back = self.tabs[self.active].index > 0;
        let can_fwd = self.tabs[self.active].index + 1 < self.tabs[self.active].history.len();
        self.back_btn.set_sensitive(can_back);
        self.fwd_btn.set_sensitive(can_fwd);
        let can_up = matches!(self.location(), Location::Dir(_));
        self.up_btn.set_sensitive(can_up);
        self.paste_btn.set_sensitive(
            read_clipboard().is_some() && self.location().drop_dir().is_some(),
        );
        self.undo_btn.set_sensitive(!self.journal.is_empty());
        self.redo_btn.set_sensitive(!self.redo.is_empty());
        for column in self.tree.columns() {
            // Indicator follows our own sort state, not the store's.
            let mine = match column.title().as_deref() {
                Some("Name") => COL_NAME,
                Some("Size") => COL_SIZE_BYTES,
                Some("Modified") => COL_MTIME_SECS,
                _ => -1,
            };
            column.set_sort_indicator(mine == self.sort_col);
            column.set_sort_order(if self.sort_asc {
                gtk::SortType::Ascending
            } else {
                gtk::SortType::Descending
            });
        }
        self.empty_trash_btn.set_visible(in_trash);
        self.restore_btn.set_visible(in_trash);
        self.restore_btn
            .set_sensitive(in_trash && !self.selected_paths().is_empty());
        if let Some(column) = self.tree.column(3) {
            column.set_visible(in_trash);
        }

        self.refresh_sidebar();
        self.update_status();
        self.update_preview();
        self.window.set_title(&format!(
            "{} — Tiaki Files",
            self.location().title()
        ));
    }

    fn refresh_sidebar(&mut self) {
        for child in self.side_list.children() {
            self.side_list.remove(&child);
        }
        let browser = self_rc_placeholder();
        let place = |label: &str, glyph: &str, location: Location, active: bool| {
            let button = gtk::Button::new();
            button.set_relief(gtk::ReliefStyle::None);
            theme::class(&button, "files-place");
            if active {
                button.style_context().add_class("active");
            }
            // Aligned with the PLACES heading above, and stretched to the
            // sidebar's full width so the row is a target, not text.
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.set_margin_start(18);
            row.set_margin_end(12);
            row.pack_start(
                &glyphs::status_icon_tinted(glyph, 16, (0.902, 0.878, 0.831)),
                false,
                false,
                0,
            );
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            row.pack_start(&text, true, true, 0);
            button.add(&row);
            let target = location;
            let capture = browser.clone();
            button.connect_clicked(move |_| {
                capture.borrow_mut().navigate(target.clone());
            });
            self.side_list.pack_start(&button, false, false, 0);
        };
        let home = home_dir();
        let current = self.location();
        let is_home = current == Location::Dir(home.clone());
        let is_trash = current == Location::Trash;
        place("Home", "home", Location::Dir(home.clone()), is_home);
        for (label, dir) in [
            ("Desktop", home.join("Desktop")),
            ("Documents", home.join("Documents")),
            ("Downloads", home.join("Downloads")),
        ] {
            if dir.is_dir() {
                let active = current == Location::Dir(dir.clone());
                place(label, "folder", Location::Dir(dir.clone()), active);
            }
        }
        for mount in system_mounts() {
            let label = mount
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| mount.to_string_lossy().into_owned());
            let active = current == Location::Dir(mount.clone());
            place(&label, "drive", Location::Dir(mount.clone()), active);
        }
        place("Trash", "trash", Location::Trash, is_trash);
        self.side_list.show_all();
    }

    fn update_status(&self) {
        let count = self.store.iter_n_children(None);
        let selected = self.tree.selection().count_selected_rows();
        let free = match self.location() {
            Location::Dir(path) => fmt_free(&path),
            Location::Search { root, .. } => fmt_free(&root),
            Location::Trash => fmt_free(&home_dir()),
        };
        let items = if count == 1 { "item" } else { "items" };
        let mut text = format!("{count} {items} · {free} free");
        if selected > 1 {
            text = format!("{count} {items} · {selected} selected · {free} free");
        }
        if !self.note.is_empty() {
            text = format!("{} — {}", text, self.note);
        }
        self.status.set_text(&text);
    }

    fn update_preview(&self) {
        let selected = self.selected_entries();
        let Some(entry) = selected.first() else {
            self.preview_title.set_text("Nothing selected");
            self.preview_meta.set_text("Select a file to preview it");
            self.preview_text.buffer().map(|buffer| buffer.set_text(""));
            self.preview_image.hide();
            for child in self.preview_slot.children() {
                self.preview_slot.remove(&child);
            }
            return;
        };
        self.preview_title.set_text(&entry.name);
        let kind = if entry.is_dir {
            "Folder"
        } else if entry.is_link {
            "Link"
        } else {
            match ext_of(&entry.name).as_str() {
                ext if IMAGE_EXTS.contains(&ext) => "Image",
                "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" => "Archive",
                _ if is_text_file(&entry.path) => "Text",
                _ => "File",
            }
        };
        // Swap the glyph.
        for child in self.preview_slot.children() {
            self.preview_slot.remove(&child);
        }
        self.preview_slot.pack_start(
            &glyphs::status_icon_tinted(glyph_for(entry), 40, (0.902, 0.878, 0.831)),
            false,
            false,
            0,
        );
        self.preview_slot.show_all();
        let meta = if entry.is_dir {
            let (bytes, files) = walk_totals(&entry.path);
            format!("{kind} · {files} items · {}", fmt_size(bytes))
        } else if entry.origin.is_empty() {
            format!("{kind} · {} · {}", fmt_size(entry.len), entry.mtime)
        } else {
            format!("{kind} · {} · from {}", fmt_size(entry.len), entry.origin)
        };
        self.preview_meta.set_text(&meta);
        // Text preview: first 8 KB. Images: thumbnail.
        self.preview_text.buffer().map(|buffer| buffer.set_text(""));
        self.preview_image.hide();
        if kind == "Text" {
            if let Ok(text) = std::fs::read(&entry.path) {
                let end = text
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(text.len())
                    .min(8192);
                let snippet = String::from_utf8_lossy(&text[..end.min(text.len())]).into_owned();
                if let Some(buffer) = self.preview_text.buffer() {
                    buffer.set_text(&snippet);
                }
            }
        } else if kind == "Image" {
            if let Ok(pixbuf) =
                gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&entry.path, 180, 180, true)
            {
                self.preview_image.set_from_pixbuf(Some(&pixbuf));
                self.preview_image.show();
            }
        }
    }
}

/// Removable volumes from /proc/mounts under /media or /mnt.
fn system_mounts() -> Vec<PathBuf> {
    let text = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (_device, mount) = (parts.next(), parts.next().unwrap_or(""));
        if (mount.starts_with("/media/") || mount.starts_with("/mnt/")) && Path::new(mount).is_dir()
        {
            out.push(PathBuf::from(mount));
        }
    }
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Window construction.

fn tool_button(label: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_relief(gtk::ReliefStyle::None);
    button.set_tooltip_text(Some(tooltip));
    theme::class(&button, "files-tool");
    button
}

/// A 1px vertical hairline that groups toolbar clusters without a box.
fn vrule() -> gtk::Box {
    let rule = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rule.set_size_request(1, 18);
    rule.set_valign(gtk::Align::Center);
    theme::class(&rule, "files-vrule");
    rule
}

fn nav_button(glyph: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_tooltip_text(Some(tooltip));
    theme::class(&button, "files-navbtn");
    button.add(&glyphs::status_icon_tinted(glyph, 16, (0.902, 0.878, 0.831)));
    button
}

fn build_window(start: Location) -> Shared {
    let window = theme::base_window("files-window");
    window.set_title("Tiaki Files");
    window.set_resizable(true);
    // Sway floats new windows near ~900px regardless of requests, so
    // the chrome is budgeted for that: 150 + 200 + spacing leaves the
    // list ~520px for 250px of columns, at any sane window size.
    window.set_default_size(1120, 700);
    window.resize(1120, 700);

    let panel = gtk::EventBox::new();
    panel.set_visible_window(true);
    theme::class(&panel, "files-panel");
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add(&shell);

    // The chrome overlay reserves the top 30 px; keep content clear of it.
    // (No extra air here: the toolbar below already carries its own padding.)
    let chrome_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chrome_spacer.set_size_request(-1, 30);
    shell.pack_start(&chrome_spacer, false, false, 0);

    // ---------------------------------------------------------------- header
    // One row, one job each: navigate, show where we are, search. Nothing
    // else competes for this space.
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    theme::class(&header, "files-header");
    let back_btn = nav_button("back", "Back");
    let fwd_btn = nav_button("chevron_right", "Forward");
    let up_btn = nav_button("chevron_up", "Up one level");
    header.pack_start(&back_btn, false, false, 0);
    header.pack_start(&fwd_btn, false, false, 0);
    header.pack_start(&up_btn, false, false, 0);
    header.pack_start(&vrule(), false, false, 6);

    let crumb_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    crumb_box.set_hexpand(true);
    crumb_box.set_halign(gtk::Align::Start);
    header.pack_start(&crumb_box, true, true, 0);
    let path_entry = gtk::Entry::new();
    path_entry.set_has_frame(false);
    path_entry.set_hexpand(true);
    theme::class(&path_entry, "files-path");
    path_entry.set_no_show_all(true);
    header.pack_start(&path_entry, true, true, 0);
    let path_toggle = nav_button("file", "Edit location (Ctrl+L)");
    header.pack_start(&path_toggle, false, false, 0);

    let search = gtk::Entry::new();
    search.set_has_frame(false);
    search.set_width_chars(20);
    search.set_placeholder_text(Some("Search"));
    theme::class(&search, "files-search");
    header.pack_start(&search, false, false, 10);
    shell.pack_start(&header, false, false, 0);

    // ------------------------------------------------------------------ tabs
    // A single tab is just noise, so this strip only appears once there is
    // more than one. The toolbar below is always present.
    let tabrow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    theme::class(&tabrow, "files-tabs");
    let tab_strip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    tab_strip.set_hexpand(true);
    tabrow.pack_start(&tab_strip, true, true, 0);
    let new_tab_btn = tool_button("+", "New tab (Ctrl+T)");
    tabrow.pack_start(&new_tab_btn, false, false, 0);
    tabrow.set_no_show_all(true);
    shell.pack_start(&tabrow, false, false, 0);

    // --------------------------------------------------------------- actions
    // Verbs worth showing. Everything else lives in the right-click menu and
    // on the keyboard, which keeps this row readable at a glance.
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    theme::class(&actions, "files-actions");
    let new_folder_btn = tool_button("New Folder", "New folder (Ctrl+Shift+N)");
    let new_file_btn = tool_button("New File", "New empty file");
    let paste_btn = tool_button("Paste", "Paste (Ctrl+V)");
    let undo_btn = tool_button("Undo", "Undo (Ctrl+Z)");
    let redo_btn = tool_button("Redo", "Redo (Ctrl+Shift+Z)");
    actions.pack_start(&new_folder_btn, false, false, 0);
    actions.pack_start(&new_file_btn, false, false, 0);
    actions.pack_start(&vrule(), false, false, 4);
    actions.pack_start(&paste_btn, false, false, 0);
    actions.pack_start(&undo_btn, false, false, 0);
    actions.pack_start(&redo_btn, false, false, 0);

    let empty_trash_btn = tool_button("Empty Trash", "Delete everything in Trash forever");
    let restore_btn = tool_button("Restore", "Restore selected to its original place");
    empty_trash_btn.set_no_show_all(true);
    restore_btn.set_no_show_all(true);
    actions.pack_start(&empty_trash_btn, false, false, 0);
    actions.pack_start(&restore_btn, false, false, 0);

    let action_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_spacer.set_hexpand(true);
    actions.pack_start(&action_spacer, true, true, 0);

    let hidden_btn = gtk::ToggleButton::with_label("Hidden");
    hidden_btn.set_relief(gtk::ReliefStyle::None);
    hidden_btn.set_tooltip_text(Some("Show hidden files (Ctrl+H)"));
    theme::class(&hidden_btn, "files-tool");
    let preview_btn = gtk::ToggleButton::with_label("Preview");
    preview_btn.set_relief(gtk::ReliefStyle::None);
    preview_btn.set_tooltip_text(Some("Show the preview pane (Space)"));
    theme::class(&preview_btn, "files-tool");
    actions.pack_end(&preview_btn, false, false, 0);
    actions.pack_end(&hidden_btn, false, false, 0);
    shell.pack_start(&actions, false, false, 0);

    // ------------------------------------------------------------------ body
    // Sidebar | list | preview. The list is the only child that grows, so it
    // always owns the space instead of leaving a gutter.
    // A Grid, not a Box: three fixed/expanding columns where the list is the
    // only one that grows. GtkBox left the list at its natural width here,
    // stranding hundreds of pixels between the sidebar and the columns.
    let body = gtk::Grid::new();
    body.set_vexpand(true);
    body.set_hexpand(true);
    body.set_column_homogeneous(false);
    body.set_row_homogeneous(false);

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.set_size_request(208, -1);
    sidebar.set_hexpand(false);
    theme::class(&sidebar, "files-sidebar");
    let side_label = gtk::Label::new(Some("PLACES"));
    side_label.set_xalign(0.0);
    side_label.set_margin_start(18);
    side_label.set_margin_top(12);
    side_label.set_margin_bottom(4);
    theme::class(&side_label, "files-sect");
    sidebar.pack_start(&side_label, false, false, 0);
    let side_list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    // fill=true: place rows span the sidebar and start at its left edge
    // instead of floating in the middle of an empty column.
    sidebar.pack_start(&side_list, false, true, 0);
    body.attach(&sidebar, 0, 0, 1, 1);

    let store = gtk::ListStore::new(&[
        String::static_type(), // name
        String::static_type(), // size text
        u64::static_type(),     // size bytes
        String::static_type(), // mtime text
        u64::static_type(),     // mtime secs
        String::static_type(), // path
        bool::static_type(),   // is dir
        String::static_type(), // origin (trash)
    ]);
    let tree = gtk::TreeView::with_model(&store);
    tree.set_headers_visible(true);
    tree.set_hexpand(true);
    tree.set_vexpand(true);
    tree.selection().set_mode(gtk::SelectionMode::Multiple);
    theme::class(&tree, "files-list");
    let mut sort_id = COL_NAME;
    let mut sort_asc = true;
    if let Some(pref) = read_pref("files-sort") {
        let mut parts = pref.split(':');
        sort_id = match parts.next().unwrap_or("name") {
            "size" => COL_SIZE_BYTES,
            "mtime" => COL_MTIME_SECS,
            _ => COL_NAME,
        };
        sort_asc = parts.next().unwrap_or("asc") != "desc";
    }
    for (title, col, width, expand) in [
        ("Name", COL_NAME, 0, true),
        ("Size", COL_SIZE_BYTES, 84, false),
        ("Modified", COL_MTIME_SECS, 136, false),
    ] {
        let cell = gtk::CellRendererText::new();
        cell.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let column = gtk::TreeViewColumn::new();
        column.set_title(title);
        column.set_resizable(true);
        column.set_clickable(true);
        // Name absorbs the slack; Size and Modified keep a fixed width so a
        // long filename can never push the date out of view.
        column.set_expand(expand);
        if !expand {
            column.set_sizing(gtk::TreeViewColumnSizing::Fixed);
            column.set_fixed_width(width);
        }
        column.set_sort_indicator(col == sort_id);
        column.set_sort_order(if sort_asc {
            gtk::SortType::Ascending
        } else {
            gtk::SortType::Descending
        });
        column.connect_clicked(move |_| {
            let browser = self_rc_placeholder();
            let mut current = browser.borrow_mut();
            if current.sort_col == col {
                current.sort_asc = !current.sort_asc;
            } else {
                current.sort_col = col;
                current.sort_asc = true;
            }
            let name = match current.sort_col {
                COL_SIZE_BYTES => "size",
                COL_MTIME_SECS => "mtime",
                _ => "name",
            };
            let dir = if current.sort_asc { "asc" } else { "desc" };
            write_pref("files-sort", &format!("{name}:{dir}"));
            current.refresh();
        });
        let attr = if col == COL_NAME {
            COL_NAME
        } else if col == COL_SIZE_BYTES {
            COL_SIZE_TEXT
        } else {
            COL_MTIME_TEXT
        };
        gtk::prelude::CellLayoutExt::pack_start(&column, &cell, true);
        gtk::prelude::CellLayoutExt::add_attribute(&column, &cell, "text", attr);
        tree.append_column(&column);
    }
    // "From" column, trash only.
    let origin_cell = gtk::CellRendererText::new();
    origin_cell.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let origin_col = gtk::TreeViewColumn::new();
    origin_col.set_title("Original Location");
    origin_col.set_resizable(true);
    gtk::prelude::CellLayoutExt::pack_start(&origin_col, &origin_cell, true);
    gtk::prelude::CellLayoutExt::add_attribute(&origin_col, &origin_cell, "text", COL_ORIGIN);
    origin_col.set_visible(false);
    tree.append_column(&origin_col);
    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.add(&tree);
    body.attach(&scroll, 1, 0, 1, 1);

    // Preview pane: off by default. It is optional information, not a
    // permanent empty column the user has to interpret.
    let preview_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    preview_box.set_size_request(260, -1);
    preview_box.set_hexpand(false);
    theme::class(&preview_box, "files-preview");
    preview_box.set_no_show_all(true);
    let preview_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preview_slot.set_halign(gtk::Align::Center);
    preview_slot.set_margin_top(20);
    preview_box.pack_start(&preview_slot, false, false, 0);
    let preview_title = gtk::Label::new(None);
    preview_title.set_xalign(0.5);
    preview_title.set_line_wrap(true);
    preview_title.set_max_width_chars(28);
    theme::class(&preview_title, "files-name");
    preview_box.pack_start(&preview_title, false, false, 0);
    let preview_meta = gtk::Label::new(None);
    preview_meta.set_xalign(0.5);
    preview_meta.set_line_wrap(true);
    preview_meta.set_max_width_chars(30);
    theme::class(&preview_meta, "files-dim");
    preview_box.pack_start(&preview_meta, false, false, 0);
    let preview_text = gtk::TextView::new();
    preview_text.set_editable(false);
    preview_text.set_cursor_visible(false);
    preview_text.set_wrap_mode(gtk::WrapMode::WordChar);
    preview_text.set_left_margin(10);
    preview_text.set_right_margin(10);
    preview_text.set_top_margin(8);
    theme::class(&preview_text, "files-previewtext");
    let preview_scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    preview_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    preview_scroll.set_vexpand(true);
    preview_scroll.set_margin_start(12);
    preview_scroll.set_margin_end(12);
    preview_scroll.add(&preview_text);
    preview_box.pack_start(&preview_scroll, true, true, 0);
    let preview_image = gtk::Image::new();
    preview_image.set_halign(gtk::Align::Center);
    preview_box.pack_start(&preview_image, false, false, 0);
    preview_image.hide();
    body.attach(&preview_box, 2, 0, 1, 1);
    shell.pack_start(&body, true, true, 0);

    // ---------------------------------------------------------------- status
    let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    theme::class(&status_row, "files-status");
    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status_row.pack_start(&status, true, true, 0);
    let progress = gtk::ProgressBar::new();
    progress.set_size_request(200, -1);
    progress.set_valign(gtk::Align::Center);
    theme::class(&progress, "files-progress");
    progress.set_no_show_all(true);
    let progress_cancel = gtk::Button::with_label("Cancel");
    progress_cancel.set_relief(gtk::ReliefStyle::None);
    theme::class(&progress_cancel, "files-cancel");
    progress_cancel.set_no_show_all(true);
    status_row.pack_start(&progress, false, false, 0);
    status_row.pack_start(&progress_cancel, false, false, 0);
    shell.pack_start(&status_row, false, false, 0);

    window.add(&panel);
    let show_hidden = read_pref("files-show-hidden").as_deref() == Some("1");
    hidden_btn.set_active(show_hidden);

    let browser: Shared = Rc::new(RefCell::new(Browser {
        window: window.clone(),
        store,
        tree,
        preview_box: preview_box.clone(),
        preview_slot,
        preview_title,
        preview_meta,
        preview_text,
        preview_image,
        status,
        progress,
        progress_cancel,
        crumb_box,
        path_entry,
        path_toggle,
        search,
        tab_strip,
        side_list,
        back_btn,
        fwd_btn,
        up_btn,
        new_tab_btn,
        new_folder_btn,
        new_file_btn,
        paste_btn,
        undo_btn,
        redo_btn,
        hidden_btn,
        preview_btn,
        empty_trash_btn,
        restore_btn,
        tabs: vec![Tab::new(start)],
        active: 0,
        sort_col: sort_id,
        sort_asc,
        show_hidden,
        journal: Vec::new(),
        redo: Vec::new(),
        cancel: Arc::new(AtomicBool::new(false)),
        note: String::new(),
    }));
    install_rc(&browser);
    browser
}

// One installed Rc per window, set once in run(). Callbacks created during
// refresh (crumbs, tabs, sidebar) clone from here; the window itself is
// built once, so there is exactly one Rc to find.
thread_local! {
    static CURRENT: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

fn self_rc_placeholder() -> Shared {
    CURRENT.with(|slot| {
        slot.borrow()
            .clone()
            .expect("browser Rc must be installed before refresh")
    })
}

fn install_rc(browser: &Shared) {
    CURRENT.with(|slot| {
        *slot.borrow_mut() = Some(browser.clone());
    });
}

// ---------------------------------------------------------------------------
// Opening, deleting, compressing.

fn open_entry(browser: &Shared, entry: &Entry) {
    if entry.is_dir {
        browser.borrow_mut().navigate(Location::Dir(entry.path.clone()));
        return;
    }
    let name = entry.name.clone();
    if is_text_file(&entry.path) || TEXT_EXTS.contains(&ext_of(&name).as_str()) {
        system::spawn_app(&format!("mousepad {}", entry.path.to_string_lossy()), "foreground");
        return;
    }
    match ext_of(&name).as_str() {
        "tar" | "tgz" | "tar.gz" => extract_archive(browser, &entry.path),
        _ => {
            if IMAGE_EXTS.contains(&ext_of(&name).as_str()) {
                browser
                    .borrow_mut()
                    .set_note("Image preview is on the right — no image viewer installed yet");
            } else {
                browser.borrow_mut().set_note(&format!(
                    "No opener installed for .{} files — preview on the right",
                    ext_of(&name)
                ));
            }
        }
    }
}

fn delete_selected(browser: &Shared, permanent: bool) {
    let paths = browser.borrow().selected_paths();
    if paths.is_empty() {
        return;
    }
    let in_trash = matches!(browser.borrow().location(), Location::Trash);
    if permanent || in_trash {
        let window = browser.borrow().window.clone();
        let what = if paths.len() == 1 {
            name_of(&paths[0])
        } else {
            format!("{} items", paths.len())
        };
        if !confirm_dialog(
            &window,
            "Delete forever?",
            &format!("{what} will be deleted forever. Undo cannot bring it back."),
            "Delete",
        ) {
            return;
        }
        let mut failed = 0;
        for path in paths {
            if remove_all(&path).is_err() {
                failed += 1;
            }
        }
        let mut current = browser.borrow_mut();
        if failed > 0 {
            current.set_note(&format!("Could not delete {failed} item(s)"));
        } else {
            current.set_note("Deleted forever");
        }
        current.refresh();
        return;
    }
    let mut items = Vec::new();
    let mut failed = Vec::new();
    for path in paths {
        match trash_item(&path) {
            Ok(name) => items.push((path, name)),
            Err(_) => failed.push(name_of(&path)),
        }
    }
    let mut current = browser.borrow_mut();
    if !failed.is_empty() {
        current.set_note(&format!("Could not trash {}", failed.join(", ")));
    }
    if !items.is_empty() {
        let count = items.len();
        current.journal.push(Op::Trash { items });
        current.redo.clear();
        current.set_note(&format!("Trashed {count} item(s) — Ctrl+Z restores"));
    }
    current.refresh();
}

fn restore_selected(browser: &Shared) {
    let names: Vec<String> = browser
        .borrow()
        .selected_entries()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    if names.is_empty() {
        return;
    }
    let mut items = Vec::new();
    for name in names {
        let trashed = trash_files().join(&name);
        if !trashed.exists() {
            continue;
        }
        let info_path = trash_info_dir().join(format!("{name}.trashinfo"));
        let original = std::fs::read_to_string(&info_path)
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("Path="))
                    .map(|rest| PathBuf::from(percent_decode(rest)))
            })
            .unwrap_or_else(|| home_dir().join(&name));
        let dest = unique_path(
            original.parent().unwrap_or_else(|| Path::new("/")),
            &name_of(&original),
        );
        match std::fs::rename(&trashed, &dest) {
            Ok(()) => {
                let _ = std::fs::remove_file(info_path);
                items.push((dest, name));
            }
            Err(error) => {
                browser
                    .borrow_mut()
                    .set_note(&format!("Could not restore {name}: {error}"));
                return;
            }
        }
    }
    let mut current = browser.borrow_mut();
    if !items.is_empty() {
        let count = items.len();
        // Journal as a restore so redo re-trashes; undo of a restore would
        // be odd here, so record it as a Trash of the restored paths.
        let trash_forms: Vec<(PathBuf, String)> = items
            .iter()
            .map(|(dest, name)| (dest.clone(), name.clone()))
            .collect();
        current.journal.push(Op::Restore { items: trash_forms });
        current.redo.clear();
        current.set_note(&format!("Restored {count} item(s)"));
    }
    current.refresh();
}

fn empty_trash(browser: &Shared) {
    let window = browser.borrow().window.clone();
    if !confirm_dialog(
        &window,
        "Empty Trash?",
        "Everything in Trash will be deleted forever.",
        "Empty Trash",
    ) {
        return;
    }
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(trash_files()) {
        for entry in entries.flatten() {
            if remove_all(&entry.path()).is_ok() {
                count += 1;
            }
        }
    }
    let _ = std::fs::read_dir(trash_info_dir()).map(|entries| {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    });
    let mut current = browser.borrow_mut();
    current.set_note(&format!("Emptied Trash ({count} item(s))"));
    current.refresh();
}

fn extract_archive(browser: &Shared, path: &Path) {
    let name = name_of(path);
    let stem = name
        .strip_suffix(".tar.gz")
        .or_else(|| name.strip_suffix(".tgz"))
        .or_else(|| name.strip_suffix(".tar"))
        .unwrap_or(&name);
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(home_dir);
    let dest = unique_path(&parent, stem);
    let status = std::process::Command::new("tar")
        .args(["xf", &path.to_string_lossy(), "-C", &parent.to_string_lossy()])
        .arg("--one-top-level")
        .arg(&dest)
        .status();
    // Older tar lacks --one-top-level: fall back to plain extract into dest.
    let status = match status {
        Ok(ok) if ok.success() => Ok(ok),
        _ => {
            let _ = std::fs::create_dir_all(&dest);
            std::process::Command::new("tar")
                .args(["-xf", &path.to_string_lossy(), "-C", &dest.to_string_lossy()])
                .status()
        }
    };
    let mut current = browser.borrow_mut();
    match status {
        Ok(ok) if ok.success() => {
            current.journal.push(Op::Create {
                paths: vec![dest.clone()],
            });
            current.redo.clear();
            current.set_note(&format!("Extracted to {}", name_of(&dest)));
        }
        _ => current.set_note("Could not extract that archive"),
    }
    current.refresh();
}

fn compress_selection(browser: &Shared) {
    let paths = browser.borrow().selected_paths();
    if paths.is_empty() {
        return;
    }
    let window = browser.borrow().window.clone();
    let first = name_of(&paths[0]);
    let default_name = if paths.len() == 1 {
        format!("{first}.tar.gz")
    } else {
        format!("Archive.tar.gz")
    };
    let Some(name) = text_dialog(
        &window,
        "Compress",
        &format!("Archive {} item(s) as:", paths.len()),
        &default_name,
        "Compress",
    ) else {
        return;
    };
    let Some(dir) = browser.borrow().location().drop_dir() else {
        browser.borrow_mut().set_note("Cannot compress here");
        return;
    };
    let dest = unique_path(&dir, &name);
    let status = std::process::Command::new("tar")
        .arg("czf")
        .arg(&dest)
        .args(["-C", &dir.to_string_lossy()])
        .args(paths.iter().map(|p| name_of(p)))
        .status();
    let mut current = browser.borrow_mut();
    match status {
        Ok(ok) if ok.success() => {
            current.journal.push(Op::Create {
                paths: vec![dest.clone()],
            });
            current.redo.clear();
            current.set_note(&format!("Created {}", name_of(&dest)));
        }
        _ => current.set_note("Could not create that archive"),
    }
    current.refresh();
}

fn do_paste(browser: &Shared) {
    let Some(clipboard) = read_clipboard() else {
        browser.borrow_mut().set_note("Clipboard is empty");
        return;
    };
    let Some(dest) = browser.borrow().location().drop_dir() else {
        browser.borrow_mut().set_note("Cannot paste here");
        return;
    };
    run_transfer(browser, clipboard.cut, clipboard.paths, dest);
}

fn copy_path(browser: &Shared) {
    let paths = browser.borrow().selected_paths();
    if paths.is_empty() {
        return;
    }
    let text = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&text);
    browser.borrow_mut().set_note(if paths.len() == 1 {
        "Path copied"
    } else {
        "Paths copied"
    });
}

fn open_terminal_here(browser: &Shared) {
    let dir = browser
        .borrow()
        .location()
        .drop_dir()
        .unwrap_or_else(home_dir);
    system::spawn_app(
        &format!("sh -c 'cd {} && exec foot -c /etc/tiaki/foot.ini'", dir.to_string_lossy()),
        "foreground",
    );
}

// ---------------------------------------------------------------------------
// Context menu and keyboard.

fn popup_menu(browser: &Shared, entries: &[Entry]) {
    let menu = gtk::Menu::new();
    let item = |label: &str, enabled: bool, action: Box<dyn Fn() + 'static>| {
        let menu_item = gtk::MenuItem::with_label(label);
        menu_item.set_sensitive(enabled);
        menu_item.connect_activate(move |_| action());
        menu.append(&menu_item);
    };
    let in_trash = matches!(browser.borrow().location(), Location::Trash);
    let capture = browser.clone();
    if entries.is_empty() && !in_trash {
        item("New Folder", true, Box::new(move || new_folder(&capture)));
        let capture = browser.clone();
        item("New File", true, Box::new(move || new_file(&capture)));
        let capture = browser.clone();
        item("Paste", read_clipboard().is_some(), Box::new(move || do_paste(&capture)));
        let capture = browser.clone();
        item(
            "Open Terminal Here",
            true,
            Box::new(move || open_terminal_here(&capture)),
        );
        let capture = browser.clone();
        item("Select All", true, Box::new(move || capture.borrow().tree.selection().select_all()));
    } else if in_trash {
        let capture = browser.clone();
        item(
            "Restore",
            !entries.is_empty(),
            Box::new(move || restore_selected(&capture)),
        );
        let capture = browser.clone();
        item(
            "Delete Forever",
            !entries.is_empty(),
            Box::new(move || delete_selected(&capture, true)),
        );
    } else {
        let capture = browser.clone();
        item(
            "Open",
            entries.len() == 1,
            Box::new(move || {
                if let Some(entry) = capture.borrow().selected_entries().into_iter().next() {
                    open_entry(&capture, &entry);
                }
            }),
        );
        let capture = browser.clone();
        item("Copy", !entries.is_empty(), Box::new(move || copy_selected(&capture, false)));
        let capture = browser.clone();
        item("Cut", !entries.is_empty(), Box::new(move || copy_selected(&capture, true)));
        let capture = browser.clone();
        item("Rename", entries.len() == 1, Box::new(move || rename_selected(&capture)));
        let capture = browser.clone();
        item(
            "Bulk Rename…",
            entries.len() > 1,
            Box::new(move || {
                let names: Vec<String> = capture
                    .borrow()
                    .selected_entries()
                    .into_iter()
                    .map(|entry| entry.name)
                    .collect();
                if let Some((pattern, start)) = bulk_rename_dialog(&capture, &names) {
                    apply_bulk_rename(&capture, &pattern, start);
                }
            }),
        );
        let capture = browser.clone();
        item("Compress…", !entries.is_empty(), Box::new(move || compress_selection(&capture)));
        let capture = browser.clone();
        item(
            "Move to Trash",
            !entries.is_empty(),
            Box::new(move || delete_selected(&capture, false)),
        );
        let capture = browser.clone();
        item(
            "Properties",
            entries.len() == 1,
            Box::new(move || {
                if let Some(path) = capture.borrow().selected_paths().into_iter().next() {
                    properties_dialog(&capture, &path);
                }
            }),
        );
        let capture = browser.clone();
        item(
            "Copy Path",
            !entries.is_empty(),
            Box::new(move || copy_path(&capture)),
        );
        let capture = browser.clone();
        item(
            "Open Terminal Here",
            true,
            Box::new(move || open_terminal_here(&capture)),
        );
    }
    menu.show_all();
    menu.popup_at_pointer(None);
}

fn copy_selected(browser: &Shared, cut: bool) {
    let paths = browser.borrow().selected_paths();
    if paths.is_empty() {
        return;
    }
    write_clipboard(&FileClipboard { cut, paths: paths.clone() });
    let note = if cut {
        format!("Cut {} item(s) — paste to move", paths.len())
    } else {
        format!("Copied {} item(s)", paths.len())
    };
    let mut current = browser.borrow_mut();
    current.set_note(note);
    current.paste_btn.set_sensitive(current.location().drop_dir().is_some());
}

fn rename_selected(browser: &Shared) {
    let Some(path) = browser.borrow().selected_paths().into_iter().next() else {
        return;
    };
    let window = browser.borrow().window.clone();
    let Some(name) = text_dialog(&window, "Rename", "New name:", &name_of(&path), "Rename") else {
        return;
    };
    let Some(parent) = path.parent().map(Path::to_path_buf) else {
        return;
    };
    let dest = parent.join(&name);
    if dest == path {
        return;
    }
    if dest.exists() {
        browser
            .borrow_mut()
            .set_note("That name is taken — pick another");
        return;
    }
    let mut current = browser.borrow_mut();
    match std::fs::rename(&path, &dest) {
        Ok(()) => {
            current.journal.push(Op::Rename {
                from: path,
                to: dest.clone(),
            });
            current.redo.clear();
            current.set_note(&format!("Renamed to {}", name_of(&dest)));
        }
        Err(error) => current.set_note(&format!("Could not rename: {error}")),
    }
    current.refresh();
}

fn new_folder(browser: &Shared) {
    let Some(dir) = browser.borrow().location().drop_dir() else {
        browser.borrow_mut().set_note("Cannot create here");
        return;
    };
    let window = browser.borrow().window.clone();
    let Some(name) = text_dialog(&window, "New Folder", "Name:", "Untitled Folder", "Create") else {
        return;
    };
    let dest = unique_path(&dir, &name);
    let mut current = browser.borrow_mut();
    match std::fs::create_dir_all(&dest) {
        Ok(()) => {
            current.journal.push(Op::Mkdir { path: dest.clone() });
            current.redo.clear();
            current.set_note(&format!("Created {}", name_of(&dest)));
        }
        Err(error) => current.set_note(&format!("Could not create folder: {error}")),
    }
    current.refresh();
}

fn new_file(browser: &Shared) {
    let Some(dir) = browser.borrow().location().drop_dir() else {
        browser.borrow_mut().set_note("Cannot create here");
        return;
    };
    let window = browser.borrow().window.clone();
    let Some(name) = text_dialog(&window, "New File", "Name:", "Untitled.txt", "Create") else {
        return;
    };
    let dest = unique_path(&dir, &name);
    let mut current = browser.borrow_mut();
    match std::fs::write(&dest, "") {
        Ok(()) => {
            current.journal.push(Op::Mkfile { path: dest.clone() });
            current.redo.clear();
            current.set_note(&format!("Created {}", name_of(&dest)));
        }
        Err(error) => current.set_note(&format!("Could not create file: {error}")),
    }
    current.refresh();
}

// ---------------------------------------------------------------------------
// Small dialogs. Modal, ink, lamp primary — same language as the shell.

fn text_dialog(
    parent: &gtk::Window,
    title: &str,
    label: &str,
    initial: &str,
    ok_label: &str,
) -> Option<String> {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(parent),
        gtk::DialogFlags::MODAL,
        &[],
    );
    dialog.set_default_size(420, 0);
    dialog.set_resizable(false);
    theme::class(&dialog, "files-dialog");
    let area = dialog.content_area();
    area.set_margin_top(18);
    area.set_margin_bottom(18);
    area.set_margin_start(20);
    area.set_margin_end(20);
    area.set_spacing(12);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    theme::class(&heading, "files-dlg-title");
    area.pack_start(&heading, false, false, 0);
    let prompt = gtk::Label::new(Some(label));
    prompt.set_xalign(0.0);
    theme::class(&prompt, "files-dim");
    area.pack_start(&prompt, false, false, 0);
    let entry = gtk::Entry::new();
    entry.set_has_frame(false);
    entry.set_text(initial);
    theme::class(&entry, "files-entry");
    area.pack_start(&entry, false, false, 0);
    let cancel = dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    theme::class(&cancel, "files-cancel");
    let ok = dialog.add_button(ok_label, gtk::ResponseType::Ok);
    theme::class(&ok, "files-ok");
    dialog.set_default_response(gtk::ResponseType::Ok);
    entry.set_activates_default(true);
    area.show_all();
    let answer = if dialog.run() == gtk::ResponseType::Ok {
        Some(entry.text().to_string())
    } else {
        None
    };
    dialog.close();
    answer.filter(|text| !text.trim().is_empty())
}

fn confirm_dialog(parent: &gtk::Window, title: &str, body: &str, action: &str) -> bool {
    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(parent),
        gtk::DialogFlags::MODAL,
        &[],
    );
    dialog.set_default_size(420, 0);
    dialog.set_resizable(false);
    theme::class(&dialog, "files-dialog");
    let area = dialog.content_area();
    area.set_margin_top(18);
    area.set_margin_bottom(18);
    area.set_margin_start(20);
    area.set_margin_end(20);
    area.set_spacing(8);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    theme::class(&heading, "files-dlg-title");
    area.pack_start(&heading, false, false, 0);
    let text = gtk::Label::new(Some(body));
    text.set_xalign(0.0);
    text.set_line_wrap(true);
    text.set_max_width_chars(52);
    theme::class(&text, "files-dim");
    area.pack_start(&text, false, false, 0);
    let cancel = dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    theme::class(&cancel, "files-cancel");
    let go = dialog.add_button(action, gtk::ResponseType::Ok);
    theme::class(&go, "files-danger");
    dialog.set_default_response(gtk::ResponseType::Cancel);
    area.show_all();
    let ok = dialog.run() == gtk::ResponseType::Ok;
    dialog.close();
    ok
}

fn properties_dialog(browser: &Shared, path: &Path) {
    let window = browser.borrow().window.clone();
    let dialog = gtk::Dialog::with_buttons(
        Some("Properties"),
        Some(&window),
        gtk::DialogFlags::MODAL,
        &[("Close", gtk::ResponseType::Close)],
    );
    dialog.set_default_size(440, 0);
    dialog.set_resizable(false);
    theme::class(&dialog, "files-dialog");
    let area = dialog.content_area();
    area.set_margin_top(18);
    area.set_margin_bottom(18);
    area.set_margin_start(20);
    area.set_margin_end(20);
    area.set_spacing(6);
    if let Some(widget) = dialog.widget_for_response(gtk::ResponseType::Close) {
        if let Ok(button) = widget.dynamic_cast::<gtk::Button>() {
            theme::class(&button, "files-cancel");
        }
    }
    let heading = gtk::Label::new(Some(&name_of(path)));
    heading.set_xalign(0.0);
    theme::class(&heading, "files-dlg-title");
    area.pack_start(&heading, false, false, 0);
    let meta = std::fs::symlink_metadata(path);
    let (kind, size_text, mtime) = match &meta {
        Ok(meta) => {
            let is_dir = meta.is_dir() && !meta.file_type().is_symlink();
            let kind = if meta.file_type().is_symlink() {
                "Link"
            } else if is_dir {
                "Folder"
            } else {
                "File"
            };
            let size = if is_dir {
                let (bytes, files) = walk_totals(path);
                format!("{} in {} items", fmt_size(bytes), files)
            } else {
                fmt_size(meta.len())
            };
            let mtime = meta.modified().map(fmt_time).unwrap_or_else(|_| "—".to_string());
            (kind.to_string(), size, mtime)
        }
        Err(_) => ("Unknown".to_string(), "—".to_string(), "—".to_string()),
    };
    for (key, value) in [
        ("Kind", kind),
        ("Size", size_text),
        ("Modified", mtime),
        ("Location", path.to_string_lossy().into_owned()),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let key_label = gtk::Label::new(Some(key));
        key_label.set_xalign(0.0);
        key_label.set_size_request(90, -1);
        theme::class(&key_label, "files-dim");
        row.pack_start(&key_label, false, false, 0);
        let value_label = gtk::Label::new(Some(&value));
        value_label.set_xalign(0.0);
        value_label.set_selectable(true);
        value_label.set_line_wrap(true);
        value_label.set_hexpand(true);
        theme::class(&value_label, "files-name");
        row.pack_start(&value_label, true, true, 0);
        area.pack_start(&row, false, false, 0);
    }
    if let Ok(meta) = meta {
        let mode = meta.permissions().mode() & 0o777;
        let perm_label = gtk::Label::new(Some("Permissions"));
        perm_label.set_xalign(0.0);
        perm_label.set_margin_top(8);
        theme::class(&perm_label, "files-dim");
        area.pack_start(&perm_label, false, false, 0);
        let grid = gtk::Grid::new();
        grid.set_row_spacing(6);
        grid.set_column_spacing(14);
        let mut boxes = Vec::new();
        for (row, name) in [(0, "Owner"), (1, "Group"), (2, "Other")].iter() {
            let tag = gtk::Label::new(Some(name));
            tag.set_xalign(0.0);
            theme::class(&tag, "files-dim");
            grid.attach(&tag, 0, *row, 1, 1);
            for (col, (letter, bit)) in [("R", 4u32), ("W", 2), ("X", 1)].iter().enumerate() {
                let shift = (2 - row) * 3;
                let check = gtk::CheckButton::with_label(letter);
                check.set_active(mode & (bit << shift) != 0);
                grid.attach(&check, col as i32 + 1, *row, 1, 1);
                boxes.push(check);
            }
        }
        area.pack_start(&grid, false, false, 0);
        let apply = gtk::Button::with_label("Apply Permissions");
        apply.set_relief(gtk::ReliefStyle::None);
        apply.set_halign(gtk::Align::Start);
        apply.set_margin_top(8);
        theme::class(&apply, "files-ok");
        let path = path.to_path_buf();
        let capture = browser.clone();
        let dialog_clone = dialog.clone();
        apply.connect_clicked(move |_| {
            let mut new_mode = 0u32;
            for (index, check) in boxes.iter().enumerate() {
                if check.is_active() {
                    let shift = (2 - index / 3) * 3;
                    let bit = [4u32, 2, 1][index % 3];
                    new_mode |= bit << shift;
                }
            }
            match std::fs::metadata(&path) {
                Ok(meta) => {
                    let old = meta.permissions().mode() & 0o777;
                    if old != new_mode {
                        if std::fs::set_permissions(
                            &path,
                            std::fs::Permissions::from_mode(new_mode),
                        )
                        .is_ok()
                        {
                            let mut current = capture.borrow_mut();
                            current.journal.push(Op::Perm {
                                path: path.clone(),
                                old,
                                new: new_mode,
                            });
                            current.redo.clear();
                            current.set_note("Permissions updated");
                            current.refresh();
                            dialog_clone.close();
                        } else {
                            let mut current = capture.borrow_mut();
                            current.set_note("Could not change permissions");
                        }
                    } else {
                        dialog_clone.close();
                    }
                }
                Err(_) => dialog_clone.close(),
            }
        });
        area.pack_start(&apply, false, false, 0);
    }
    area.show_all();
    dialog.run();
    dialog.close();
}

fn bulk_rename_dialog(browser: &Shared, names: &[String]) -> Option<(String, u32)> {
    let window = browser.borrow().window.clone();
    let dialog = gtk::Dialog::with_buttons(
        Some("Bulk Rename"),
        Some(&window),
        gtk::DialogFlags::MODAL,
        &[],
    );
    dialog.set_default_size(440, 0);
    dialog.set_resizable(false);
    theme::class(&dialog, "files-dialog");
    let area = dialog.content_area();
    area.set_margin_top(18);
    area.set_margin_bottom(18);
    area.set_margin_start(20);
    area.set_margin_end(20);
    area.set_spacing(8);
    let heading = gtk::Label::new(Some(&format!("Rename {} items", names.len())));
    heading.set_xalign(0.0);
    theme::class(&heading, "files-dlg-title");
    area.pack_start(&heading, false, false, 0);
    let hint = gtk::Label::new(Some("Use # for the counter: photo-### keeps each extension."));
    hint.set_xalign(0.0);
    hint.set_line_wrap(true);
    theme::class(&hint, "files-dim");
    area.pack_start(&hint, false, false, 0);
    let entry = gtk::Entry::new();
    entry.set_has_frame(false);
    entry.set_text("item-###");
    theme::class(&entry, "files-entry");
    area.pack_start(&entry, false, false, 0);
    let start_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let start_label = gtk::Label::new(Some("Start at"));
    theme::class(&start_label, "files-dim");
    start_row.pack_start(&start_label, false, false, 0);
    let start_entry = gtk::Entry::new();
    start_entry.set_has_frame(false);
    start_entry.set_text("1");
    start_entry.set_width_chars(6);
    theme::class(&start_entry, "files-entry");
    start_row.pack_start(&start_entry, false, false, 0);
    area.pack_start(&start_row, false, false, 0);
    let cancel = dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    theme::class(&cancel, "files-cancel");
    let go = dialog.add_button("Rename", gtk::ResponseType::Ok);
    theme::class(&go, "files-ok");
    dialog.set_default_response(gtk::ResponseType::Ok);
    entry.set_activates_default(true);
    area.show_all();
    let answer = if dialog.run() == gtk::ResponseType::Ok {
        let pattern = entry.text().to_string();
        let start = start_entry.text().parse::<u32>().unwrap_or(1);
        if pattern.trim().is_empty() {
            None
        } else {
            Some((pattern, start))
        }
    } else {
        None
    };
    dialog.close();
    answer
}

fn apply_bulk_rename(browser: &Shared, pattern: &str, mut counter: u32) {
    let paths = browser.borrow().selected_paths();
    let Some(dir) = browser.borrow().location().drop_dir() else {
        browser.borrow_mut().set_note("Nothing to rename into");
        return;
    };
    let pad = pattern.matches('#').count().max(1) as usize;
    let mut renames = Vec::new();
    for path in paths {
        if !path.exists() || path.parent().as_deref() != Some(dir.as_path()) {
            // Bulk rename applies within the shown folder.
            continue;
        }
        let old_name = name_of(&path);
        let numbered = pattern.replace(&"#".repeat(pad), &format!("{:01$}", counter, pad));
        counter += 1;
        let new_name = match old_name.rfind('.') {
            Some(dot) if dot > 0 && !std::fs::symlink_metadata(&path).map(|m| m.is_dir()).unwrap_or(false) => {
                format!("{}{}", numbered, &old_name[dot..])
            }
            _ => numbered,
        };
        if new_name == old_name {
            continue;
        }
        let dest = unique_path(&dir, &new_name);
        match std::fs::rename(&path, &dest) {
            Ok(()) => renames.push((path, dest)),
            Err(error) => {
                browser
                    .borrow_mut()
                    .set_note(&format!("Could not rename {}: {error}", name_of(&path)));
                return;
            }
        }
    }
    if renames.is_empty() {
        browser.borrow_mut().set_note("Nothing renamed");
        return;
    }
    let count = renames.len();
    let mut current = browser.borrow_mut();
    for (from, to) in renames {
        current.journal.push(Op::Rename { from, to });
    }
    current.redo.clear();
    current.set_note(&format!("Renamed {count} item(s)"));
    current.refresh();
}

// ---------------------------------------------------------------------------
// Transfers with progress. Small jobs run inline; big ones get a worker.

enum WorkerMsg {
    Progress { done: u64, total: u64, current: String },
    Done { created: Vec<PathBuf>, moved: Vec<(PathBuf, PathBuf)>, renames: usize, error: Option<String>, is_move: bool },
}

fn copy_file_chunked(
    source: &Path,
    dest: &Path,
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let mut input = std::fs::File::open(source)?;
    let mut output = std::fs::File::create(dest)?;
    let mut buffer = [0u8; 256 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(dest);
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        on_bytes(count as u64);
    }
    // Keep the source's permissions on the copy.
    if let Ok(meta) = std::fs::symlink_metadata(source) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(meta.permissions().mode()));
    }
    Ok(())
}

fn run_transfer(browser: &Shared, is_move: bool, sources: Vec<PathBuf>, dest_dir: PathBuf) {
    let sources: Vec<PathBuf> = sources.into_iter().filter(|p| p.exists()).collect();
    if sources.is_empty() {
        browser.borrow_mut().set_note("Nothing to paste");
        return;
    }
    if sources.iter().any(|src| {
        src.parent()
            .map(|parent| parent == dest_dir)
            .unwrap_or(false)
            && is_move
    }) {
        browser.borrow_mut().set_note("Already here");
        return;
    }
    let (mut total_bytes, mut total_files) = (0u64, 0u64);
    for source in &sources {
        let (bytes, files) = walk_totals(source);
        total_bytes += bytes;
        total_files += files;
    }
    // Fast path: small jobs run inline, no thread, no progress bar.
    if total_bytes < 4 * 1024 * 1024 && total_files < 200 {
        let mut created = Vec::new();
        let mut moved = Vec::new();
        let mut renames = 0;
        for source in &sources {
            let dest = unique_path(&dest_dir, &name_of(source));
            if dest != dest_dir.join(name_of(source)) {
                renames += 1;
            }
            let result = if is_move {
                match std::fs::rename(source, &dest) {
                    Ok(()) => {
                        moved.push((source.clone(), dest.clone()));
                        Ok(())
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
                        copy_recursive(source, &dest).and_then(|_| {
                            remove_all(source)?;
                            moved.push((source.clone(), dest.clone()));
                            Ok(())
                        })
                    }
                    Err(error) => Err(error),
                }
            } else {
                copy_recursive(source, &dest).map(|_| {
                    created.push(dest.clone());
                })
            };
            if let Err(error) = result {
                browser
                    .borrow_mut()
                    .set_note(&format!("Could not paste {}: {error}", name_of(source)));
                break;
            }
        }
        let mut current = browser.borrow_mut();
        if is_move {
            if !moved.is_empty() {
                current.journal.push(Op::Move { moved });
                current.redo.clear();
            }
        } else if !created.is_empty() {
            current.journal.push(Op::Create { paths: created });
            current.redo.clear();
        }
        let mut note = if is_move { "Moved".to_string() } else { "Copied".to_string() };
        if renames > 0 {
            note = format!("{note} ({renames} renamed to avoid a clash)");
        }
        if is_move {
            clear_clipboard();
        }
        current.set_note(&note);
        current.refresh();
        return;
    }
    // Worker path with progress + cancel.
    #[allow(deprecated)]
    let (sender, receiver) = glib::MainContext::channel(glib::Priority::DEFAULT);
    let cancel = browser.borrow().cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let progress_sender = sender.clone();
    let report = move |done: u64, total: u64, current: String| {
        let _ = progress_sender.send(WorkerMsg::Progress { done, total, current });
    };
    {
        let current = browser.borrow();
        current.progress.set_fraction(0.0);
        current.progress.show();
        current.progress_cancel.show();
    }
    let cancel_flag = cancel.clone();
    std::thread::spawn(move || {
        let mut done_sources = 0u64;
        let mut done_bytes = 0u64;
        let mut created = Vec::new();
        let mut moved = Vec::new();
        let mut renames = 0;
        let mut error: Option<String> = None;
        for source in &sources {
            if cancel_flag.load(Ordering::Relaxed) {
                error = Some("Cancelled".to_string());
                break;
            }
            let dest = unique_path(&dest_dir, &name_of(source));
            if dest != dest_dir.join(name_of(source)) {
                renames += 1;
            }
            // Big single files copy in chunks so progress and cancel stay
            // live mid-file; everything else copies whole.
            let big_file = !is_move
                && std::fs::symlink_metadata(source)
                    .map(|m| !m.is_dir() && !m.file_type().is_symlink() && m.len() > 4 * 1024 * 1024)
                    .unwrap_or(false);
            let result = if is_move {
                match std::fs::rename(source, &dest) {
                    Ok(()) => {
                        moved.push((source.clone(), dest.clone()));
                        Ok(())
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
                        let outcome = (|| -> std::io::Result<()> {
                            copy_recursive(source, &dest)?;
                            remove_all(source)?;
                            Ok(())
                        })();
                        if outcome.is_ok() {
                            moved.push((source.clone(), dest.clone()));
                        }
                        outcome
                    }
                    Err(error) => Err(error),
                }
            } else if big_file {
                let outcome = (|| -> std::io::Result<()> {
                    let mut written = 0u64;
                    copy_file_chunked(source, &dest, &cancel_flag, &mut |count| {
                        written += count;
                        report(
                            done_bytes + written,
                            total_bytes.max(1),
                            name_of(&dest),
                        );
                        if cancel_flag.load(Ordering::Relaxed) {
                            // Bail out of the chunk loop early.
                        }
                    })?;
                    if cancel_flag.load(Ordering::Relaxed) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "cancelled",
                        ));
                    }
                    Ok(())
                })();
                if outcome.is_ok() {
                    created.push(dest.clone());
                }
                outcome
            } else {
                copy_recursive(source, &dest).map(|_| {
                    created.push(dest.clone());
                })
            };
            if let Err(problem) = result {
                if problem.kind() == std::io::ErrorKind::Interrupted {
                    error = Some("Cancelled".to_string());
                } else {
                    error = Some(format!("Could not paste {}: {problem}", name_of(source)));
                }
                break;
            }
            done_sources += 1;
            // Byte-accurate for chunked files; per-item otherwise.
            if !big_file {
                done_bytes += walk_totals(source).0;
            } else if let Ok(meta) = std::fs::symlink_metadata(source) {
                done_bytes += meta.len();
            }
            report(
                done_bytes.max(done_sources),
                total_bytes.max(done_sources).max(1),
                name_of(&dest),
            );
        }
        let _ = sender.send(WorkerMsg::Done {
            created,
            moved,
            renames,
            error,
            is_move,
        });
    });
    let capture = browser.clone();
    receiver.attach(None, move |message| {
        let mut current = capture.borrow_mut();
        match message {
            WorkerMsg::Progress { done, total, current: name } => {
                current.progress.set_fraction(done as f64 / total.max(1) as f64);
                current.progress.set_text(Some(&name));
                current.set_note(&format!("Copying {name}"));
            }
            WorkerMsg::Done { created, moved, renames, error, is_move } => {
                current.progress.hide();
                current.progress_cancel.hide();
                if let Some(problem) = error {
                    current.set_note(&problem);
                } else {
                    if is_move {
                        if !moved.is_empty() {
                            current.journal.push(Op::Move { moved });
                            current.redo.clear();
                        }
                        clear_clipboard();
                    } else if !created.is_empty() {
                        current.journal.push(Op::Create { paths: created });
                        current.redo.clear();
                    }
                    let mut note = if is_move { "Moved".to_string() } else { "Copied".to_string() };
                    if renames > 0 {
                        note = format!("{note} ({renames} renamed to avoid a clash)");
                    }
                    current.set_note(&note);
                }
                current.refresh();
            }
        }
        glib::ControlFlow::Continue
    });
}

// ---------------------------------------------------------------------------
// Wiring: toolbar, tree, search, keys.

fn wire(browser: &Shared) {
    let capture = browser.clone();
    browser.borrow().back_btn.connect_clicked(move |_| {
        let mut current = capture.borrow_mut();
        let active = current.active;
        if current.tabs[active].back() {
            current.refresh();
        }
    });
    let capture = browser.clone();
    browser.borrow().fwd_btn.connect_clicked(move |_| {
        let mut current = capture.borrow_mut();
        let active = current.active;
        if current.tabs[active].forward() {
            current.refresh();
        }
    });
    let capture = browser.clone();
    browser.borrow().up_btn.connect_clicked(move |_| {
        let parent = match capture.borrow().location() {
            Location::Dir(path) => path.parent().map(Path::to_path_buf),
            Location::Search { root, .. } => Some(root),
            Location::Trash => None,
        };
        if let Some(dir) = parent {
            capture.borrow_mut().navigate(Location::Dir(dir));
        }
    });
    let capture = browser.clone();
    browser.borrow().new_tab_btn.connect_clicked(move |_| {
        let location = capture.borrow().location();
        capture.borrow_mut().new_tab(location);
    });
    let capture = browser.clone();
    browser.borrow().new_folder_btn.connect_clicked(move |_| new_folder(&capture));
    let capture = browser.clone();
    browser.borrow().new_file_btn.connect_clicked(move |_| new_file(&capture));
    let capture = browser.clone();
    browser.borrow().paste_btn.connect_clicked(move |_| do_paste(&capture));
    let capture = browser.clone();
    browser.borrow().undo_btn.connect_clicked(move |_| capture.borrow_mut().do_undo());
    let capture = browser.clone();
    browser.borrow().redo_btn.connect_clicked(move |_| capture.borrow_mut().do_redo());
    let capture = browser.clone();
    browser.borrow().hidden_btn.connect_toggled(move |button| {
        let show = button.is_active();
        write_pref("files-show-hidden", if show { "1" } else { "0" });
        let mut current = capture.borrow_mut();
        current.show_hidden = show;
        current.refresh();
    });
    let capture = browser.clone();
    browser.borrow().preview_btn.connect_toggled(move |button| {
        capture.borrow().preview_box.set_visible(button.is_active());
    });
    let capture = browser.clone();
    browser.borrow().empty_trash_btn.connect_clicked(move |_| empty_trash(&capture));
    let capture = browser.clone();
    browser.borrow().restore_btn.connect_clicked(move |_| restore_selected(&capture));
    let capture = browser.clone();
    browser.borrow().path_toggle.connect_clicked(move |_| {
        let current = capture.borrow();
        let editing = current.path_entry.get_visible();
        if editing {
            current.path_entry.hide();
            current.crumb_box.show();
        } else {
            current.crumb_box.hide();
            current.path_entry.show();
            current.path_entry.grab_focus();
        }
    });
    let capture = browser.clone();
    browser.borrow().path_entry.connect_activate(move |entry| {
        let text = entry.text().to_string();
        let trimmed = text.trim();
        if trimmed == "trash:" {
            capture.borrow_mut().navigate(Location::Trash);
            return;
        }
        let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
            home_dir().join(rest)
        } else {
            PathBuf::from(trimmed)
        };
        let target = if expanded.is_absolute() {
            expanded
        } else {
            match capture.borrow().location().drop_dir() {
                Some(dir) => dir.join(expanded),
                None => home_dir().join(expanded),
            }
        };
        if target.is_dir() {
            capture.borrow_mut().navigate(Location::Dir(target));
        } else if let Some(parent) = target.parent().filter(|p| p.is_dir()) {
            capture.borrow_mut().navigate(Location::Dir(parent.to_path_buf()));
            capture.borrow_mut().set_note("Not a folder — showing its parent");
        } else {
            capture.borrow_mut().set_note("No such folder");
        }
    });
    let capture = browser.clone();
    browser.borrow().search.connect_changed(move |entry| {
        let query = entry.text().to_string();
        let mut current = capture.borrow_mut();
        if query.trim().len() >= 2 {
            let root = match current.location() {
                Location::Dir(path) => path,
                Location::Search { root, .. } => root,
                Location::Trash => home_dir(),
            };
            current.navigate(Location::Search {
                root,
                query: query.trim().to_string(),
            });
        } else if query.is_empty() && matches!(current.location(), Location::Search { .. }) {
            let root = match current.location() {
                Location::Search { root, .. } => root,
                _ => home_dir(),
            };
            current.navigate(Location::Dir(root));
        }
    });

    let capture = browser.clone();
    browser.borrow().tree.connect_cursor_changed(move |_| {
        let current = capture.borrow();
        current.update_preview();
        current.update_status();
        if matches!(current.location(), Location::Trash) {
            current.restore_btn.set_sensitive(!current.selected_paths().is_empty());
        }
    });
    let capture = browser.clone();
    browser.borrow().tree.connect_row_activated(move |_, _, _| {
        if let Some(entry) = capture.borrow().selected_entries().into_iter().next() {
            open_entry(&capture, &entry);
        }
    });
    let capture = browser.clone();
    browser.borrow().tree.connect_button_press_event(move |tree, event| {
        if event.button() == 3 {
            if let Some((Some(path), ..)) = tree.path_at_pos(
                event.position().0 as i32,
                event.position().1 as i32,
            ) {
                let selection = tree.selection();
                if !selection.path_is_selected(&path) {
                    selection.unselect_all();
                    selection.select_path(&path);
                }
            }
            let entries = capture.borrow().selected_entries();
            popup_menu(&capture, &entries);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });

    let capture = browser.clone();
    browser.borrow().progress_cancel.connect_clicked(move |_| {
        capture.borrow().cancel.store(true, Ordering::Relaxed);
    });

    // Keyboard.
    let capture = browser.clone();
    browser.borrow().window.connect_key_press_event(move |_, event| {
        use gdk::keys::constants as key;
        use gtk::gdk::ModifierType;
        let state = event.state();
        let ctrl = state.contains(ModifierType::CONTROL_MASK);
        let shift = state.contains(ModifierType::SHIFT_MASK);
        let keyval = event.keyval();
        // Typing in the search or path fields belongs to them — except
        // Escape, which always backs out first.
        let in_field = {
            let current = capture.borrow();
            current.search.has_focus() || current.path_entry.has_focus()
        };
        if in_field && keyval != key::Escape {
            return glib::Propagation::Proceed;
        }
        let mut current = capture.borrow_mut();
        if ctrl && keyval == key::z && !shift {
            current.do_undo();
        } else if ctrl && (keyval == key::Z || (keyval == key::z && shift)) {
            current.do_redo();
        } else if ctrl && keyval == key::c {
            let paths = current.selected_paths();
            drop(current);
            if !paths.is_empty() {
                write_clipboard(&FileClipboard { cut: false, paths });
                let mut current = capture.borrow_mut();
                current.set_note("Copied");
                current.refresh();
            }
        } else if ctrl && keyval == key::x {
            let paths = current.selected_paths();
            drop(current);
            if !paths.is_empty() {
                write_clipboard(&FileClipboard { cut: true, paths });
                let mut current = capture.borrow_mut();
                current.set_note("Cut — paste to move");
                current.refresh();
            }
        } else if ctrl && keyval == key::v {
            drop(current);
            do_paste(&capture);
        } else if ctrl && keyval == key::a {
            current.tree.selection().select_all();
        } else if keyval == key::Delete {
            let permanent = state.contains(ModifierType::SHIFT_MASK);
            drop(current);
            delete_selected(&capture, permanent);
        } else if keyval == key::F2 {
            drop(current);
            rename_selected(&capture);
        } else if keyval == key::Return || keyval == key::KP_Enter {
            let selected = current.selected_entries().into_iter().next();
            drop(current);
            if let Some(entry) = selected {
                open_entry(&capture, &entry);
            }
        } else if keyval == key::BackSpace
            || (event.state().contains(ModifierType::MOD1_MASK) && keyval == key::Up)
        {
            let parent = match current.location() {
                Location::Dir(path) => path.parent().map(Path::to_path_buf),
                Location::Search { root, .. } => Some(root),
                Location::Trash => None,
            };
            if let Some(dir) = parent {
                current.navigate(Location::Dir(dir));
            }
        } else if ctrl && keyval == key::t {
            let location = current.location();
            current.new_tab(location);
        } else if ctrl && keyval == key::w {
            let active = current.active;
            current.close_tab(active);
        } else if ctrl && keyval == key::h {
            let show = !current.show_hidden;
            drop(current);
            // The toggle's own handler persists, applies and refreshes.
            capture.borrow().hidden_btn.set_active(show);
        } else if ctrl && keyval == key::l {
            let editing = current.path_entry.get_visible();
            if editing {
                current.path_entry.hide();
                current.crumb_box.show();
            } else {
                current.crumb_box.hide();
                current.path_entry.show();
                current.path_entry.grab_focus();
            }
        } else if ctrl && keyval == key::f {
            current.search.grab_focus();
        } else if keyval == key::space {
            let visible = current.preview_box.get_visible();
            current.preview_box.set_visible(!visible);
            current.preview_btn.set_active(!visible);
        } else if keyval == key::Escape {
            if !current.search.text().is_empty() {
                current.search.set_text("");
            } else if !matches!(current.location(), Location::Trash) {
                current.window.close();
            }
        } else {
            return glib::Propagation::Proceed;
        }
        glib::Propagation::Stop
    });

    browser
        .borrow()
        .window
        .connect_destroy(|_| gtk::main_quit());
}

// ---------------------------------------------------------------------------
// Entry point.

fn start_location(arg: Option<&str>) -> Location {
    match arg {
        Some("trash:") => Location::Trash,
        Some(path) => {
            let expanded = if let Some(rest) = path.strip_prefix("~/") {
                home_dir().join(rest)
            } else {
                PathBuf::from(path)
            };
            if expanded.is_dir() {
                Location::Dir(expanded)
            } else if let Some(parent) = expanded.parent().filter(|p| p.is_dir()) {
                Location::Dir(parent.to_path_buf())
            } else {
                Location::Dir(home_dir())
            }
        }
        None => Location::Dir(home_dir()),
    }
}

pub fn run(start: Option<&str>) {
    let browser = build_window(start_location(start));
    install_rc(&browser);
    wire(&browser);
    browser.borrow_mut().refresh();
    browser.borrow().window.show_all();
    browser.borrow().path_entry.hide();
    browser.borrow().progress.hide();
    browser.borrow().progress_cancel.hide();
    if !matches!(browser.borrow().location(), Location::Trash) {
        browser.borrow().empty_trash_btn.hide();
        browser.borrow().restore_btn.hide();
    }
    gtk::main();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// A scratch HOME so trash/prefs never touch the real machine.
    fn scratch_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tiaki-files-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("HOME", &dir);
        dir
    }

    #[test]
    fn dates_format_without_crates() {
        let _guard = TEST_LOCK.lock().unwrap();
        let epoch = std::time::UNIX_EPOCH;
        assert_eq!(fmt_time(epoch), "1970-01-01 00:00");
        let y2k = epoch + std::time::Duration::from_secs(10957 * 86_400);
        assert_eq!(fmt_time(y2k), "2000-01-01 00:00");
        let leap = epoch + std::time::Duration::from_secs(19782 * 86_400);
        assert_eq!(fmt_time(leap), "2024-02-29 00:00");
        let before = epoch - std::time::Duration::from_secs(86_400);
        assert_eq!(fmt_time(before), "1969-12-31 00:00");
    }

    #[test]
    fn trash_path_codec_roundtrips() {
        let _guard = TEST_LOCK.lock().unwrap();
        for path in [
            "/home/desktop/My Report (final).txt",
            "/tmp/spaces and ünicode ♥.md",
            "/home/desktop/plain.txt",
        ] {
            assert_eq!(percent_decode(&percent_encode(path)), path);
        }
        // Trash paths stay readable by other implementations.
        assert_eq!(percent_encode("/home/desktop/a b.txt"), "/home/desktop/a%20b.txt");
    }

    #[test]
    fn unique_names_never_collide() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("tiaki-uniq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a.txt"));
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a (2).txt"));
        std::fs::write(dir.join("a (2).txt"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a (3).txt"));
        assert_eq!(unique_path(&dir, "noext"), dir.join("noext"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trash_roundtrip_restores() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = scratch_home("trash");
        let file = home.join("note.txt");
        std::fs::write(&file, b"hello").unwrap();
        let name = trash_item(&file).unwrap();
        assert!(!file.exists());
        assert!(trash_files().join(&name).exists());
        let entries = read_trash();
        assert!(entries.iter().any(|entry| entry.name == name
            && entry.original == file
            && !entry.deleted.is_empty()));
        // Undo restores to the original path.
        Op::Trash {
            items: vec![(file.clone(), name)],
        }
        .undo()
        .unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"hello");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn journal_mkdir_mkfile_rename_perm() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = scratch_home("journal");
        let dir = home.join("work");
        Op::Mkdir { path: dir.clone() }.redo().unwrap();
        assert!(dir.is_dir());
        Op::Mkdir { path: dir.clone() }.undo().unwrap();
        assert!(!dir.exists());

        let file = home.join("a.txt");
        Op::Mkfile { path: file.clone() }.redo().unwrap();
        assert!(file.exists());
        let renamed = home.join("b.txt");
        let rename = Op::Rename { from: file.clone(), to: renamed.clone() };
        rename.redo().unwrap();
        assert!(!file.exists() && renamed.exists());
        rename.undo().unwrap();
        assert!(file.exists() && !renamed.exists());

        use std::os::unix::fs::PermissionsExt;
        let before = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        let perm = Op::Perm { path: file.clone(), old: before, new: 0o600 };
        perm.redo().unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        perm.undo().unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, before);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn clipboard_roundtrips() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir();
        let file = dir.join(format!("tiaki-clip-{}.txt", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        write_clipboard(&FileClipboard { cut: true, paths: vec![file.clone()] });
        let back = read_clipboard().unwrap();
        assert!(back.cut && back.paths == vec![file.clone()]);
        clear_clipboard();
        assert!(read_clipboard().is_none());
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn text_sniffing() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir();
        let text = dir.join("tiaki-sniff.txt");
        let binary = dir.join("tiaki-sniff.bin");
        std::fs::write(&text, b"hello\nworld\n").unwrap();
        std::fs::write(&binary, b"\x89PNG\x00\x01\x02").unwrap();
        assert!(is_text_file(&text));
        assert!(!is_text_file(&binary));
        // Unknown extension falls back to sniffing.
        let unknown = dir.join("tiaki-sniff-payload");
        std::fs::write(&unknown, b"#!/bin/sh\necho hi\n").unwrap();
        assert!(is_text_file(&unknown));
        for path in [&text, &binary, &unknown] {
            let _ = std::fs::remove_file(path);
        }
    }
}
