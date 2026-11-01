//! Purpose:
//! Remembers, between builds of one program, which autoloaded class files the compiler handed
//! back to the runtime autoloader, so the next build defers them up front.
//!
//! Called from:
//! - `crate::pipeline::compile()`, before the first type-check round (`load`) and after the last
//!   one succeeds (`store`).
//!
//! Key details:
//! - Every file the checker refuses costs a whole type-check round, and every body the backend
//!   cannot lower costs a whole second compile. On the Symfony demo that was 7 s of checking for
//!   files whose refusal no edit of the program had touched.
//! - An entry is reused only while the file's bytes hash the same, and the memo itself is keyed by
//!   the compiler binary (path, size, mtime), the entry path and the full argument list, so a new
//!   compiler, other flags or an edited deferred file all start from a clean round.
//! - A memo can only defer a file, which is always a legal build state: the file still runs, in
//!   the interpreter, exactly as when a round refuses it. The cost of a stale entry is a class
//!   left interpreted after an edit ELSEWHERE made it compilable; `ELEPHC_DEFERRAL_MEMO=0` builds
//!   without the memo.
//! - The file lives under the elephc cache root next to the assembler cache
//!   (`$XDG_CACHE_HOME/elephc/deferrals/<key>.txt`), one `hash<TAB>path<TAB>reason` line per file.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;

/// One remembered deferral: the file and why it was handed back.
pub struct MemoEntry {
    pub path: PathBuf,
    pub reason: String,
}

/// Whether the memo is consulted at all. Only `0` and `off` disable it.
fn enabled() -> bool {
    !matches!(
        std::env::var("ELEPHC_DEFERRAL_MEMO").as_deref(),
        Ok("0") | Ok("off")
    )
}

/// The cache root the assembler cache uses.
fn cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(path).join("elephc").join("deferrals")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".cache").join("elephc").join("deferrals")
    } else {
        std::env::temp_dir().join("elephc-cache").join("deferrals")
    }
}

/// The memo file for this compiler, entry program and argument list, if one can be named.
fn memo_path(entry: &Path) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let metadata = std::fs::metadata(&executable).ok()?;
    let mut hasher = DefaultHasher::new();
    FORMAT_VERSION.hash(&mut hasher);
    executable.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|age| age.as_nanos())
        .hash(&mut hasher);
    entry
        .canonicalize()
        .unwrap_or_else(|_| entry.to_path_buf())
        .hash(&mut hasher);
    // Where the binary is written does not change what compiles, so the output directory stays
    // out of the key: two builds of one program into two directories share a memo.
    let mut skip_value = false;
    for argument in std::env::args_os().skip(1) {
        if std::mem::take(&mut skip_value) {
            continue;
        }
        let text = argument.to_string_lossy();
        if text == "--output-dir" {
            skip_value = true;
            continue;
        }
        if text.starts_with("--output-dir=") {
            continue;
        }
        argument.hash(&mut hasher);
    }
    Some(cache_root().join(format!("{:016x}.txt", hasher.finish())))
}

/// A sibling memo of this build's deferral memo, with its own extension (`erased` for the
/// files `docblock::erasure` compiles with their written types).
pub fn sibling_memo_path(entry: &Path, extension: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    memo_path(entry).map(|path| path.with_extension(extension))
}

/// The hash of a file's current bytes, or `None` when it cannot be read.
pub fn content_hash(path: &Path) -> Option<u64> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}

/// The remembered deferrals whose files still hold the bytes they held when they were refused.
pub fn load(entry: &Path) -> Vec<MemoEntry> {
    if !enabled() {
        return Vec::new();
    }
    let Some(path) = memo_path(entry) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let hash = u64::from_str_radix(fields.next()?, 16).ok()?;
            let file = PathBuf::from(fields.next()?);
            let reason = fields.next().unwrap_or_default().to_string();
            (content_hash(&file)? == hash).then_some(MemoEntry { path: file, reason })
        })
        .collect()
}

/// Records the files this build handed back. An empty set removes the memo.
pub fn store(entry: &Path, deferred: &[MemoEntry]) {
    if !enabled() {
        return;
    }
    let Some(path) = memo_path(entry) else {
        return;
    };
    if deferred.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let mut text = String::new();
    for item in deferred {
        let Some(hash) = content_hash(&item.path) else {
            continue;
        };
        let reason = item.reason.replace(['\n', '\t'], " ");
        text.push_str(&format!("{hash:016x}\t{}\t{reason}\n", item.path.display()));
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Written beside the target and renamed over it, so a concurrent build reads either the old
    // memo or the new one, never half of one.
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&temporary, text).is_ok() {
        let _ = std::fs::rename(&temporary, &path);
    }
}
