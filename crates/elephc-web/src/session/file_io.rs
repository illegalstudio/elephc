//! Purpose:
//! Owns session file I/O for `--web` mode: locked read/write/destroy/abort,
//! the `session_reset`/`lazy_write` snapshot and mtime-touch primitives, a
//! filesystem existence check for `session.use_strict_mode`, and mtime-based
//! garbage collection of `sess_<id>` files under the configured save path.
//!
//! Called from:
//! - The compiled `--web` web prelude via the `elephc_web_session_read/write/
//!   destroy/abort/gc/snapshot/file_exists/touch/should_gc` C-ABI symbols.
//! - `session::state::elephc_web_session_reset`, which calls `release_lock` to
//!   drop any held file lock at the start of every request.
//!
//! Key details:
//! - One process per prefork worker, single-threaded, so the held-fd state
//!   (`SESSION_FILE`, exposed to sibling state as the `SESSION_FD` sentinel) is
//!   race-free across a request's read → write/destroy/abort/touch sequence.
//! - Save paths follow php-src's `[depth;[mode;]]path` grammar; every operation
//!   shares the same ID-derived path calculation and no-follow/owner checks.
//! - BUG-3: `write` now writes in place on the held fd/inode as its *primary*
//!   path (truncate+seek+write+sync), since the advisory lock is held on that
//!   file — a temp+rename would swap in a new file the lock no longer covers,
//!   breaking concurrent-writer serialization. Temp+rename remains the
//!   fallback for the no-file-held branch (e.g. `session_regenerate_id`'s
//!   fresh id, which has no concurrent reader to serialize against).
//! - Unix uses `flock`; Windows uses an exclusive whole-file `LockFileEx`
//!   range and retains the `File` so a pointer-sized HANDLE is never truncated.
//! - §2.5 `touch` (lazy_write) MUST release the held lock exactly like
//!   `write` does — leaving it held would self-deadlock the next `read`
//!   (BUG-1's mechanism).
//! - §2.6 `gc` excludes the currently active `SESSION_ID`'s file regardless of
//!   its mtime, so garbage collection never unlinks the open+locked file for
//!   the in-flight request.

use super::id::{read_random, validate_session_id};
use super::state::{
    cstr_to_string, input_bytes, opt_ptr, publish_bytes, set_cstr, GC_DIVISOR, GC_PROBABILITY,
    RET_STRING, SESSION_FD, SESSION_ID, SESSION_SNAPSHOT,
};
use std::ffi::c_char;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;

/// Result of the most recent binary-safe files-handler read operation.
static mut LAST_READ_OK: i64 = 1;

/// Session file kept open and exclusively locked between read and completion.
///
/// `SESSION_FD` remains the small cross-module held/not-held sentinel, while
/// this owner retains the actual Rust file so Windows HANDLE values are never
/// truncated into an `i32`.
static mut SESSION_FILE: Option<File> = None;

/// Releases and closes the held session file. No-op when none is held.
/// Single-threaded per worker.
pub(super) unsafe fn release_lock() {
    let file = core::ptr::replace(core::ptr::addr_of_mut!(SESSION_FILE), None);
    if let Some(file) = file {
        unlock_file(&file);
        drop(file);
    }
    core::ptr::write(core::ptr::addr_of_mut!(SESSION_FD), -1);
}

/// Builds the full path for a session file: `<save_path>/sess_<id>`.
#[cfg(test)]
pub(super) fn session_file_path(save_path: &PathBuf, id: &str) -> PathBuf {
    save_path.join(format!("sess_{id}"))
}

/// Parsed `session.save_path` configuration used by PHP's files handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FilesSavePath {
    /// Base directory containing flat files or the first sharding directory.
    pub(super) base: PathBuf,
    /// Number of one-character directory levels derived from the session ID.
    pub(super) depth: usize,
    /// Unix creation mode for newly-created session files.
    pub(super) mode: u32,
}

/// Parses php-src's `[depth;[mode;]]path` files-handler save-path grammar.
pub(super) fn parse_save_path(configured: &str) -> Option<FilesSavePath> {
    if configured.is_empty() {
        return Some(FilesSavePath {
            base: std::env::temp_dir(),
            depth: 0,
            mode: 0o600,
        });
    }
    let parts: Vec<&str> = configured.splitn(3, ';').collect();
    match parts.as_slice() {
        [path] => Some(FilesSavePath {
            base: PathBuf::from(path),
            depth: 0,
            mode: 0o600,
        }),
        [depth, path] => Some(FilesSavePath {
            base: PathBuf::from(path),
            depth: depth.parse().ok()?,
            mode: 0o600,
        }),
        [depth, mode, path] => {
            let mode = u32::from_str_radix(mode, 8).ok()?;
            if mode > 0o7777 {
                return None;
            }
            Some(FilesSavePath {
                base: PathBuf::from(path),
                depth: depth.parse().ok()?,
                mode,
            })
        }
        _ => None,
    }
}

/// Derives php-src's sharded session path, rejecting IDs shorter than depth.
pub(super) fn configured_session_file_path(configured: &str, id: &str) -> Option<PathBuf> {
    let parsed = parse_save_path(configured)?;
    if id.len() <= parsed.depth {
        return None;
    }
    let mut path = parsed.base;
    for byte in id.as_bytes().iter().take(parsed.depth) {
        path.push((*byte as char).to_string());
    }
    path.push(format!("sess_{id}"));
    Some(path)
}

/// Opens a session file without following symlinks and verifies its owner.
pub(super) fn open_session_file(
    path: &std::path::Path,
    mode: u32,
) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    let _ = mode;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    #[cfg(windows)]
    options.custom_flags(
        windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
    );
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        let metadata = file.metadata()?;
        let uid = unsafe { libc::getuid() };
        let euid = unsafe { libc::geteuid() };
        if uid != 0 && metadata.uid() != 0 && metadata.uid() != uid && metadata.uid() != euid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "session file is owned by another user",
            ));
        }
    }
    #[cfg(windows)]
    if file.metadata()?.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "session file is a reparse-point symbolic link",
        ));
    }
    Ok(file)
}

/// Acquires an exclusive advisory lock, retrying when interrupted by a signal.
#[cfg(unix)]
pub(super) fn lock_exclusive(file: &File) -> bool {
    let fd = file.as_raw_fd();
    loop {
        let result = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if result == 0 {
            return true;
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return false;
        }
    }
}

/// Acquires an exclusive Windows byte-range lock covering the entire file.
#[cfg(windows)]
pub(super) fn lock_exclusive(file: &File) -> bool {
    let mut overlapped = windows_sys::Win32::System::IO::OVERLAPPED::default();
    unsafe {
        windows_sys::Win32::Storage::FileSystem::LockFileEx(
            file.as_raw_handle(),
            windows_sys::Win32::Storage::FileSystem::LOCKFILE_EXCLUSIVE_LOCK,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        ) != 0
    }
}

/// Releases a Unix advisory lock held by `file`.
#[cfg(unix)]
fn unlock_file(file: &File) {
    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }
}

/// Releases the Windows byte-range lock held by `file`.
#[cfg(windows)]
fn unlock_file(file: &File) {
    let mut overlapped = windows_sys::Win32::System::IO::OVERLAPPED::default();
    unsafe {
        windows_sys::Win32::Storage::FileSystem::UnlockFileEx(
            file.as_raw_handle(),
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        );
    }
}

/// Replaces a locked session file's contents in place and flushes them.
pub(super) fn write_file_in_place(file: &mut File, data: &[u8]) -> std::io::Result<()> {
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(data)?;
    file.sync_all()
}

/// Opens a no-follow temporary session file for the atomic replacement path.
fn open_temporary_session_file(path: &std::path::Path, mode: u32) -> std::io::Result<File> {
    #[cfg(windows)]
    let _ = mode;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    #[cfg(windows)]
    options.custom_flags(
        windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
    );
    options.open(path)
}

/// Updates a session file's access and modification timestamps to now.
fn touch_session_file(path: &std::path::Path) -> std::io::Result<()> {
    let now = std::time::SystemTime::now();
    OpenOptions::new()
        .write(true)
        .open(path)?
        .set_times(FileTimes::new().set_accessed(now).set_modified(now))
}

/// Reads the session file for `id` under `save_path`. Opens read/write with
/// creation, acquires the platform lock, reads the content, and retains the
/// file for later `session_write`/`session_destroy`/`session_abort`.
/// When `read_and_close=1`, releases the lock and closes the fd immediately
/// after reading (no write will happen at handler end). Publishes the exact file
/// bytes in the shared pointer/length transfer buffer and returns its pointer.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_read_bytes(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
    read_and_close: i64,
) -> i64 {
    core::ptr::write(core::ptr::addr_of_mut!(LAST_READ_OK), 0);
    let id = cstr_to_string(id_ptr);
    let sp = cstr_to_string(save_path_ptr);
    if id.is_empty() {
        return publish_bytes(&[]);
    }
    // Validate the session ID before touching the filesystem (spec 3.8).
    if !validate_session_id(&id) {
        return publish_bytes(&[]);
    }
    let Some(config) = parse_save_path(&sp) else {
        return publish_bytes(&[]);
    };
    let Some(path) = configured_session_file_path(&sp, &id) else {
        return publish_bytes(&[]);
    };

    // Open with O_RDWR | O_CREAT, mode 0600.
    let mut file = match open_session_file(&path, config.mode) {
        Ok(f) => f,
        Err(_) => return publish_bytes(&[]),
    };

    // Acquire exclusive lock (blocks until the lock is available).
    if !lock_exclusive(&file) {
        return publish_bytes(&[]);
    }

    // Read the full content.
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        unlock_file(&file);
        return publish_bytes(&[]);
    }

    if read_and_close != 0 {
        // Read-and-close: release lock and close fd immediately.
        drop(file);
        // fd is closed by drop; flock is released on close.
    } else {
        // Retain the Rust file and lock for later write/destroy/abort. The
        // numeric state is only a held/not-held sentinel on Windows.
        core::ptr::write(core::ptr::addr_of_mut!(SESSION_FILE), Some(file));
        core::ptr::write(core::ptr::addr_of_mut!(SESSION_FD), 0);
    }

    // Store the snapshot for session_reset/session_abort.
    (*core::ptr::addr_of_mut!(SESSION_SNAPSHOT)).clear();
    (*core::ptr::addr_of_mut!(SESSION_SNAPSHOT)).extend_from_slice(&data);
    core::ptr::write(core::ptr::addr_of_mut!(LAST_READ_OK), 1);

    publish_bytes(&data)
}

/// Returns 1 when the latest files-handler read completed successfully and 0
/// when it failed validation, save-path parsing, open, locking, or I/O.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_last_read_ok() -> i64 {
    *core::ptr::addr_of!(LAST_READ_OK)
}

/// Backward-compatible C-string reader for textual callers and unit tests.
/// Binary-safe generated PHP uses [`elephc_web_session_read_bytes`].
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_read(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
    read_and_close: i64,
) -> *const c_char {
    let _ = elephc_web_session_read_bytes(id_ptr, save_path_ptr, read_and_close);
    let bytes = (*core::ptr::addr_of!(super::state::DATA_BUFFER)).clone();
    set_cstr(
        core::ptr::addr_of_mut!(RET_STRING),
        &String::from_utf8_lossy(&bytes),
    );
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// Writes `data` to the session file for `id` under `save_path`, then
/// releases the held lock and closes the fd. Returns 1 on success, 0 on
/// failure.
///
/// BUG-3: when this request already holds the session fd (from an earlier
/// `elephc_web_session_read`), that is the **primary** path — write in place
/// on the held fd (`ftruncate`+`pwrite`+`fsync`), since the `flock` is on that
/// specific inode. Writing to a temp file and `rename`-ing over the original
/// would swap in a *new* inode the held lock no longer covers, breaking
/// serialization against any other process still waiting on the old inode's
/// lock. When no fd is held (e.g. `session_regenerate_id`'s fresh id, which
/// has no concurrent reader to serialize against), fall back to the atomic
/// temp+rename path.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_write_bytes(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    let id = cstr_to_string(id_ptr);
    let sp = cstr_to_string(save_path_ptr);
    if id.is_empty() {
        return 0;
    }
    if !validate_session_id(&id) {
        return 0;
    }
    let Some(config) = parse_save_path(&sp) else {
        return 0;
    };
    let Some(path) = configured_session_file_path(&sp, &id) else {
        return 0;
    };
    let data = input_bytes(data_ptr, data_len);

    if *core::ptr::addr_of!(SESSION_FD) >= 0 {
        // Primary path (BUG-3): in-place write on the held fd/inode.
        let file = &mut *core::ptr::addr_of_mut!(SESSION_FILE);
        let ok = file
            .as_mut()
            .is_some_and(|held| write_file_in_place(held, data).is_ok());
        release_lock();
        return i64::from(ok);
    }

    // No fd held: fall back to atomic temp+rename. The temp name includes the
    // session ID to avoid collisions between concurrent requests (and
    // parallel tests) in the same process.
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let tmp_path = dir.join(format!(".sess_tmp_{}_{}", std::process::id(), id));

    let result = (|| -> std::io::Result<()> {
        {
            let mut tmp = open_temporary_session_file(&tmp_path, config.mode)?;
            tmp.write_all(data)?;
            // Ensure the data hits disk before the rename.
            tmp.sync_all()?;
        }
        fs::rename(&tmp_path, &path)?;
        Ok(())
    })();

    // No fd was held in this branch, so there is nothing for release_lock to
    // release, but call it anyway for symmetry/defense-in-depth in case a
    // future caller path changes.
    release_lock();
    if result.is_ok() {
        1
    } else {
        0
    }
}

/// Backward-compatible C-string writer. Generated PHP uses the binary-safe
/// pointer/length variant instead.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_write(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
    data_ptr: *const c_char,
) -> i64 {
    let data = if data_ptr.is_null() {
        &[][..]
    } else {
        std::ffi::CStr::from_ptr(data_ptr).to_bytes()
    };
    elephc_web_session_write_bytes(id_ptr, save_path_ptr, data.as_ptr(), data.len() as i64)
}

/// Destroys the session file for `id` under `save_path` (deletes the file),
/// then releases any held lock. Returns 1 on success, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_destroy(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
) -> i64 {
    let id = cstr_to_string(id_ptr);
    let sp = cstr_to_string(save_path_ptr);
    if id.is_empty() {
        return 0;
    }
    if !validate_session_id(&id) {
        return 0;
    }
    let Some(path) = configured_session_file_path(&sp, &id) else {
        release_lock();
        return 0;
    };

    // Keep the lock through unlink so another waiter cannot race the destroy.
    let result = match fs::remove_file(&path) {
        Ok(()) => 1,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 1,
        Err(_) => 0,
    };
    release_lock();
    result
}

/// Aborts the session: releases the held lock without writing (discards any
/// in-memory changes). Returns 1 on success.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_abort(
    _id_ptr: *const c_char,
    _save_path_ptr: *const c_char,
) -> i64 {
    // Release the held lock without writing; the file keeps its original content.
    release_lock();
    1
}

/// BUG-1/2: returns the read-time `SESSION_SNAPSHOT` (the session file
/// content as of the last `elephc_web_session_read`) as a NUL-terminated C
/// bytes, without touching the filesystem or the held lock. `session_reset`
/// uses this to restore post-`session_start` state instead of re-opening (and
/// re-`flock`ing) a file this process already holds locked, which previously
/// self-deadlocked. Also used by lazy_write (§2.5) to compare freshly-encoded
/// data against the unchanged-since-read baseline. The returned pointer is
/// valid until the next session C-ABI call.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_snapshot_bytes() -> i64 {
    let snapshot = (*core::ptr::addr_of!(SESSION_SNAPSHOT)).clone();
    publish_bytes(&snapshot)
}

/// Backward-compatible textual snapshot getter used by existing Rust tests.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_snapshot() -> *const c_char {
    let _ = elephc_web_session_snapshot_bytes();
    let bytes = (*core::ptr::addr_of!(super::state::DATA_BUFFER)).clone();
    set_cstr(
        core::ptr::addr_of_mut!(RET_STRING),
        &String::from_utf8_lossy(&bytes),
    );
    opt_ptr(core::ptr::addr_of!(RET_STRING))
}

/// §2.2 `session.use_strict_mode`: returns 1 if the session file for `id`
/// under `save_path` exists on disk, 0 otherwise (including invalid IDs or
/// empty arguments). Used by `session_start` under strict mode to detect a
/// client-supplied ID that doesn't correspond to a real session and mint a
/// fresh one instead (anti-fixation). Note: this check and the subsequent
/// `read` are not atomic (a TOCTOU window exists) — this matches PHP's own
/// non-atomic strict-mode design.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_file_exists(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
) -> i64 {
    let id = cstr_to_string(id_ptr);
    let sp = cstr_to_string(save_path_ptr);
    if id.is_empty() || !validate_session_id(&id) {
        return 0;
    }
    let Some(path) = configured_session_file_path(&sp, &id) else {
        return 0;
    };
    if path.is_file() {
        1
    } else {
        0
    }
}

/// §2.5 `lazy_write`: bumps the session file's mtime (and atime) to "now" via
/// `utimes`, without rewriting its content. Used when the freshly-encoded
/// session data is byte-identical to the read-time snapshot, so a full
/// rewrite can be skipped while the expiry clock still resets. Returns 1 on
/// success, 0 on failure (missing file, invalid ID, or empty arguments).
///
/// ⚠️ CRITICAL: releases the held `SESSION_FD`/`flock` exactly like `write`
/// does, regardless of success or failure. Skipping this would leave the lock
/// held for the rest of the unchanged request, self-deadlocking the next
/// `read` of the same session (the same mechanism as BUG-1).
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_touch(
    id_ptr: *const c_char,
    save_path_ptr: *const c_char,
) -> i64 {
    let id = cstr_to_string(id_ptr);
    let sp = cstr_to_string(save_path_ptr);
    let mut ok: i64 = 0;
    if !id.is_empty() && validate_session_id(&id) {
        let Some(path) = configured_session_file_path(&sp, &id) else {
            release_lock();
            return 0;
        };
        if touch_session_file(&path).is_ok() {
            ok = 1;
        }
    }
    release_lock();
    ok
}

// ═══════════════════════════════════════════════════════════════════════════
// Garbage collection
// ═══════════════════════════════════════════════════════════════════════════

/// Scans `save_path` for files matching `sess_*` and deletes those whose mtime
/// is older than `maxlifetime` seconds. Returns the number of deleted files.
///
/// §2.6: never deletes the currently active session's file (the one named by
/// `state::SESSION_ID`), regardless of its mtime. Without this exclusion, GC
/// running mid-request (e.g. right after `session_start`'s read) could unlink
/// the open+locked file for the in-flight request, losing the later write.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_gc(
    save_path_ptr: *const c_char,
    maxlifetime: i64,
) -> i64 {
    let sp = cstr_to_string(save_path_ptr);
    let Some(config) = parse_save_path(&sp) else {
        return -1;
    };
    let dir = config.base.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cutoff = now - maxlifetime;
    // The active session's filename, if any, is never a GC candidate.
    let active_id = cstr_to_string(opt_ptr(core::ptr::addr_of!(SESSION_ID)));
    let active_file_name = if active_id.is_empty() {
        None
    } else {
        Some(format!("sess_{active_id}"))
    };

    /// Recursively removes expired session files at the configured leaf depth.
    fn cleanup_dir(
        dir: &std::path::Path,
        remaining_depth: usize,
        cutoff: i64,
        now: i64,
        active_path: Option<&std::path::Path>,
    ) -> i64 {
        let Ok(entries) = fs::read_dir(dir) else {
            return -1;
        };
        let mut deleted = 0;
        for entry in entries.flatten() {
            if remaining_depth > 0 {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    let count =
                        cleanup_dir(&entry.path(), remaining_depth - 1, cutoff, now, active_path);
                    if count >= 0 {
                        deleted += count;
                    }
                }
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("sess_") {
                continue;
            }
            if active_path.is_some_and(|active| active == entry.path()) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let mtime_secs = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(now);
                    if mtime_secs < cutoff {
                        if fs::remove_file(entry.path()).is_ok() {
                            deleted += 1;
                        }
                    }
                }
            }
        }
        deleted
    }

    let active_path = active_file_name
        .as_ref()
        .and_then(|_| configured_session_file_path(&sp, &active_id));
    cleanup_dir(&dir, config.depth, cutoff, now, active_path.as_deref())
}

/// §2.6: rolls the auto-GC probability gate (`gc_probability`/`gc_divisor`)
/// and returns 1 if garbage collection should run this request, 0 otherwise.
/// Reuses `id::read_random` (the existing `/dev/urandom` primitive) rather
/// than pulling in a new crate. `gc_probability<=0` or `gc_divisor<=0`
/// unconditionally disables auto-GC (returns 0), matching PHP's `0`-disables
/// convention.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_session_should_gc() -> i64 {
    let probability = *core::ptr::addr_of!(GC_PROBABILITY);
    let divisor = *core::ptr::addr_of!(GC_DIVISOR);
    if probability <= 0 || divisor <= 0 {
        return 0;
    }
    let mut buf = [0u8; 4];
    read_random(&mut buf);
    let roll = (u32::from_le_bytes(buf) % divisor as u32) as i64;
    if roll < probability {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::elephc_web_session_reset;
    use super::super::state::test_lock as lock;
    use super::*;

    /// Verifies files save-path parsing and PHP's ID-derived directory layout.
    #[test]
    fn save_path_grammar_and_sharding() {
        let parsed = parse_save_path("2;0640;/tmp/session-store").unwrap();
        assert_eq!(parsed.base, PathBuf::from("/tmp/session-store"));
        assert_eq!(parsed.depth, 2);
        assert_eq!(parsed.mode, 0o640);
        assert_eq!(
            configured_session_file_path("2;0640;/tmp/session-store", "abcdef"),
            Some(PathBuf::from("/tmp/session-store/a/b/sess_abcdef"))
        );
        assert_eq!(
            configured_session_file_path("2;0640;/tmp/session-store", "ab"),
            None
        );
        assert!(parse_save_path("2;9999;/tmp/session-store").is_none());
    }

    /// Verifies pointer/length file I/O preserves embedded NUL bytes exactly.
    #[test]
    fn binary_session_file_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let dir = std::env::temp_dir().join(format!(
                "elephc_session_binary_{}_{}",
                std::process::id(),
                read_random_u64()
            ));
            let id = std::ffi::CString::new("binaryid").unwrap();
            fs::create_dir_all(dir.join("b").join("i")).unwrap();
            let configured = format!("2;0640;{}", dir.to_string_lossy());
            let save_path = std::ffi::CString::new(configured).unwrap();
            let payload = b"key|s:3:\"a\0b\";";
            assert_eq!(
                elephc_web_session_write_bytes(
                    id.as_ptr(),
                    save_path.as_ptr(),
                    payload.as_ptr(),
                    payload.len() as i64,
                ),
                1
            );
            let pointer = elephc_web_session_read_bytes(id.as_ptr(), save_path.as_ptr(), 1);
            assert_eq!(
                super::super::state::elephc_web_session_data_len(),
                payload.len() as i64
            );
            assert_eq!(
                std::slice::from_raw_parts(pointer as *const u8, payload.len()),
                payload
            );
            let metadata = fs::metadata(dir.join("b/i/sess_binaryid")).unwrap();
            #[cfg(unix)]
            assert_eq!(metadata.mode() & 0o7777, 0o640);
            #[cfg(windows)]
            assert!(metadata.is_file());
            let _ = fs::remove_dir_all(dir);
        }
    }

    /// Generates a test-only random suffix without introducing shared state.
    fn read_random_u64() -> u64 {
        let mut bytes = [0u8; 8];
        read_random(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    /// Sets a fixture file's access and modification times to a Unix timestamp.
    fn set_file_timestamp(path: &std::path::Path, seconds: i64) {
        let timestamp = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(u64::try_from(seconds).unwrap_or(0));
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_accessed(timestamp)
                    .set_modified(timestamp),
            )
            .unwrap();
    }

    /// Verifies session read/write/destroy round-trip with file locking.
    #[test]
    fn session_file_read_write_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let sp = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            let sp_ptr = sp.as_ptr();

            // Generate a unique session ID for this test to avoid collisions.
            let id = format!("testrw{}", std::process::id());
            let id_c = std::ffi::CString::new(id).unwrap();
            let id_ptr = id_c.as_ptr();

            // Write data.
            let data = std::ffi::CString::new(b"count|i:42;name|s:5:\"World\";".to_vec()).unwrap();
            let result = elephc_web_session_write(id_ptr, sp_ptr, data.as_ptr());
            assert_eq!(result, 1);

            // Read it back (read_and_close=0, lock held).
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(id_ptr, sp_ptr, 0));
            assert_eq!(raw.to_str().unwrap(), data.to_str().unwrap());

            // Clean up: release lock + destroy.
            release_lock();
            elephc_web_session_destroy(id_ptr, sp_ptr);
        }
    }

    /// Verifies read_and_close=1 reads and immediately releases the lock.
    #[test]
    fn session_read_and_close() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let sp = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            let sp_ptr = sp.as_ptr();
            let id = format!("testrc{}", std::process::id());
            let id_c = std::ffi::CString::new(id).unwrap();
            let id_ptr = id_c.as_ptr();
            let data = std::ffi::CString::new(b"x|i:1;".to_vec()).unwrap();

            // Write first.
            elephc_web_session_write(id_ptr, sp_ptr, data.as_ptr());

            // Read with read_and_close=1 — lock should not be held after.
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(id_ptr, sp_ptr, 1));
            assert_eq!(raw.to_str().unwrap(), data.to_str().unwrap());

            // fd should be -1 (read_and_close does not hold it).
            assert_eq!(*core::ptr::addr_of!(SESSION_FD), -1);

            // Clean up.
            elephc_web_session_destroy(id_ptr, sp_ptr);
        }
    }

    /// Reports files-handler read failures separately from a valid empty
    /// session payload so `session_start()` can return false like php-src.
    #[test]
    fn session_read_status_distinguishes_failure_from_empty_data() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let invalid_path = std::ffi::CString::new("not-a-depth;/tmp").unwrap();
            let id = std::ffi::CString::new("validreadstatusid").unwrap();
            elephc_web_session_read_bytes(id.as_ptr(), invalid_path.as_ptr(), 1);
            assert_eq!(elephc_web_session_last_read_ok(), 0);

            let path = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            elephc_web_session_read_bytes(id.as_ptr(), path.as_ptr(), 1);
            assert_eq!(elephc_web_session_last_read_ok(), 1);
            elephc_web_session_destroy(id.as_ptr(), path.as_ptr());
        }
    }

    /// Verifies session_abort releases the lock without writing.
    #[test]
    fn session_abort_releases_lock() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let sp = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            let sp_ptr = sp.as_ptr();
            let id = format!("testabort{}", std::process::id());
            let id_c = std::ffi::CString::new(id).unwrap();
            let id_ptr = id_c.as_ptr();

            // Write initial data.
            let data = std::ffi::CString::new(b"v|i:1;".to_vec()).unwrap();
            elephc_web_session_write(id_ptr, sp_ptr, data.as_ptr());

            // Read (hold lock).
            elephc_web_session_read(id_ptr, sp_ptr, 0);
            assert!(*core::ptr::addr_of!(SESSION_FD) >= 0);

            // Abort — should release lock.
            elephc_web_session_abort(id_ptr, sp_ptr);
            assert_eq!(*core::ptr::addr_of!(SESSION_FD), -1);

            // Clean up.
            elephc_web_session_destroy(id_ptr, sp_ptr);
        }
    }

    /// Verifies GC deletes expired files and leaves fresh ones.
    #[test]
    fn session_gc_deletes_expired() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp = std::env::temp_dir();
            let sp = std::ffi::CString::new(tmp.to_string_lossy().into_owned()).unwrap();
            let sp_ptr = sp.as_ptr();

            // Create a fresh file.
            let fresh_id = format!("testgcfresh{}", std::process::id());
            let fresh_c = std::ffi::CString::new(fresh_id.clone()).unwrap();
            let fresh_ptr = fresh_c.as_ptr();
            let fresh_data = std::ffi::CString::new(b"x|i:1;".to_vec()).unwrap();
            elephc_web_session_write(fresh_ptr, sp_ptr, fresh_data.as_ptr());

            // Create a file and backdate its mtime by 2 hours.
            let old_id = format!("testgcold{}", std::process::id());
            let old_c = std::ffi::CString::new(old_id.clone()).unwrap();
            let old_ptr = old_c.as_ptr();
            let old_data = std::ffi::CString::new(b"x|i:2;".to_vec()).unwrap();
            elephc_web_session_write(old_ptr, sp_ptr, old_data.as_ptr());
            let old_path = session_file_path(&tmp, &old_id);
            // Backdate the fixture through Rust's portable file-time API.
            let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
            let secs = two_hours_ago
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            set_file_timestamp(&old_path, secs);

            // GC with maxlifetime=3600 (1 hour) should delete the old file only.
            let deleted = elephc_web_session_gc(sp_ptr, 3600);
            assert!(deleted >= 1, "expected at least 1 deleted, got {deleted}");
            assert!(!old_path.exists(), "old file should have been deleted");

            // Clean up the fresh file.
            elephc_web_session_destroy(fresh_ptr, sp_ptr);
        }
    }

    /// Verifies GC descends through the configured files-handler shard depth.
    #[test]
    fn session_gc_recurses_through_sharded_path() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let base = std::env::temp_dir().join(format!(
                "elephc_gc_nested_{}_{}",
                std::process::id(),
                read_random_u64()
            ));
            fs::create_dir_all(base.join("a/b")).unwrap();
            let configured = std::ffi::CString::new(format!(
                "2;0600;{}",
                base.to_string_lossy()
            ))
            .unwrap();
            let path = base.join("a/b/sess_abcd");
            fs::write(&path, b"x|i:1;").unwrap();
            let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
            let seconds = old
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            set_file_timestamp(&path, seconds);
            assert_eq!(elephc_web_session_gc(configured.as_ptr(), 3600), 1);
            assert!(!path.exists());
            let _ = fs::remove_dir_all(base);
        }
    }

    /// BUG-3: when a read already holds the session fd/lock, a subsequent
    /// write goes through the in-place path (ftruncate+pwrite+fsync on the
    /// held fd), not temp+rename — and still releases the lock afterward.
    #[test]
    fn session_write_in_place_round_trip() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let sp = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            let sp_ptr = sp.as_ptr();
            let id = format!("testinplace{}", std::process::id());
            let id_c = std::ffi::CString::new(id).unwrap();
            let id_ptr = id_c.as_ptr();

            // Seed the file, then read to acquire the held fd/lock.
            let seed = std::ffi::CString::new(b"a|i:1;".to_vec()).unwrap();
            elephc_web_session_write(id_ptr, sp_ptr, seed.as_ptr());
            elephc_web_session_read(id_ptr, sp_ptr, 0);
            assert!(
                *core::ptr::addr_of!(SESSION_FD) >= 0,
                "expected held fd after read"
            );

            // Write while the fd is held: must go through the in-place path.
            let new_data = std::ffi::CString::new(b"b|i:2;".to_vec()).unwrap();
            let result = elephc_web_session_write(id_ptr, sp_ptr, new_data.as_ptr());
            assert_eq!(result, 1);

            // The in-place write path must release the lock afterward, same
            // as the temp+rename path.
            assert_eq!(*core::ptr::addr_of!(SESSION_FD), -1);

            // Content is truncated + rewritten (not appended) by the new data.
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(id_ptr, sp_ptr, 1));
            assert_eq!(raw.to_str().unwrap(), new_data.to_str().unwrap());

            elephc_web_session_destroy(id_ptr, sp_ptr);
        }
    }

    /// BUG-1/2: `snapshot` returns the read-time content without re-opening
    /// or touching the held lock.
    #[test]
    fn session_snapshot_matches_read() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let sp = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            let sp_ptr = sp.as_ptr();
            let id = format!("testsnap{}", std::process::id());
            let id_c = std::ffi::CString::new(id).unwrap();
            let id_ptr = id_c.as_ptr();

            let data = std::ffi::CString::new(b"a|i:1;".to_vec()).unwrap();
            elephc_web_session_write(id_ptr, sp_ptr, data.as_ptr());
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(id_ptr, sp_ptr, 0))
                .to_str()
                .unwrap()
                .to_string();
            let snap = std::ffi::CStr::from_ptr(elephc_web_session_snapshot())
                .to_str()
                .unwrap()
                .to_string();
            assert_eq!(snap, raw);

            release_lock();
            elephc_web_session_destroy(id_ptr, sp_ptr);
        }
    }

    /// §2.2: `file_exists` reflects filesystem state before/after write and
    /// destroy, and rejects an invalid ID.
    #[test]
    fn session_file_exists_reflects_disk_state() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let sp = std::ffi::CString::new(std::env::temp_dir().to_string_lossy().into_owned())
                .unwrap();
            let sp_ptr = sp.as_ptr();
            let id = format!("testexists{}", std::process::id());
            let id_c = std::ffi::CString::new(id).unwrap();
            let id_ptr = id_c.as_ptr();

            assert_eq!(elephc_web_session_file_exists(id_ptr, sp_ptr), 0);

            let data = std::ffi::CString::new(b"a|i:1;".to_vec()).unwrap();
            elephc_web_session_write(id_ptr, sp_ptr, data.as_ptr());
            assert_eq!(elephc_web_session_file_exists(id_ptr, sp_ptr), 1);

            elephc_web_session_destroy(id_ptr, sp_ptr);
            assert_eq!(elephc_web_session_file_exists(id_ptr, sp_ptr), 0);

            // Invalid ID charset is rejected regardless of disk state.
            let bad_id = std::ffi::CString::new("bad;id").unwrap();
            assert_eq!(elephc_web_session_file_exists(bad_id.as_ptr(), sp_ptr), 0);
        }
    }

    /// §2.5 lazy_write: `touch` bumps mtime without rewriting content, and
    /// MUST release the held lock exactly like `write` — otherwise the next
    /// read on the same session self-deadlocks (BUG-1's mechanism).
    #[test]
    fn session_touch_bumps_mtime_and_releases_lock() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp = std::env::temp_dir();
            let sp = std::ffi::CString::new(tmp.to_string_lossy().into_owned()).unwrap();
            let sp_ptr = sp.as_ptr();
            let id = format!("testtouch{}", std::process::id());
            let id_c = std::ffi::CString::new(id.clone()).unwrap();
            let id_ptr = id_c.as_ptr();

            let data = std::ffi::CString::new(b"a|i:1;".to_vec()).unwrap();
            elephc_web_session_write(id_ptr, sp_ptr, data.as_ptr());

            // Backdate the file so the touch's mtime bump is observable.
            let path = session_file_path(&tmp, &id);
            let old_secs = (std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            set_file_timestamp(&path, old_secs);

            // Hold the lock via read, then touch instead of write.
            elephc_web_session_read(id_ptr, sp_ptr, 0);
            assert!(
                *core::ptr::addr_of!(SESSION_FD) >= 0,
                "expected held fd after read"
            );

            let result = elephc_web_session_touch(id_ptr, sp_ptr);
            assert_eq!(result, 1);

            // CRITICAL: the lock must be released, or the next read deadlocks.
            assert_eq!(
                *core::ptr::addr_of!(SESSION_FD),
                -1,
                "touch must release the held lock"
            );

            // mtime should now be recent, not the backdated hour-old value.
            let meta = fs::metadata(&path).unwrap();
            let mtime = meta.modified().unwrap();
            let age = std::time::SystemTime::now()
                .duration_since(mtime)
                .unwrap_or_default();
            assert!(age.as_secs() < 60, "expected a fresh mtime, age={:?}", age);

            // Content is unchanged (touch never rewrites the file body).
            let raw = std::ffi::CStr::from_ptr(elephc_web_session_read(id_ptr, sp_ptr, 1));
            assert_eq!(raw.to_str().unwrap(), data.to_str().unwrap());

            elephc_web_session_destroy(id_ptr, sp_ptr);
        }
    }

    /// §2.6: `should_gc` never fires when gc_probability is 0 (disabled), and
    /// always fires when gc_probability == gc_divisor (deterministic, avoids
    /// a flaky probabilistic assertion).
    #[test]
    fn session_should_gc_probability_gate() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();

            super::super::state::elephc_web_session_set_gc_probability(0);
            for _ in 0..20 {
                assert_eq!(elephc_web_session_should_gc(), 0);
            }

            super::super::state::elephc_web_session_set_gc_divisor(1);
            super::super::state::elephc_web_session_set_gc_probability(1);
            for _ in 0..20 {
                assert_eq!(elephc_web_session_should_gc(), 1);
            }

            elephc_web_session_reset();
        }
    }

    /// §2.6: GC must never delete the currently active session's file, even
    /// if its mtime is older than the cutoff (it may be, e.g., a long-running
    /// request that read the file before an aggressive maxlifetime).
    ///
    /// Uses an isolated subdirectory (not the raw shared system temp dir) as
    /// `save_path`: `gc` scans every `sess_*` file in that directory, and the
    /// shared system temp dir can accumulate unrelated stale session files
    /// across unrelated test runs (the pre-existing `session_gc_deletes_expired`
    /// test above tolerates this with a lenient `>= 1` assertion; this test
    /// needs an exact `deleted == 0`, so it isolates instead).
    #[test]
    fn session_gc_excludes_active_session() {
        let _g = lock();
        unsafe {
            elephc_web_session_reset();
            let tmp =
                std::env::temp_dir().join(format!("elephc_gc_active_test_{}", std::process::id()));
            let _ = fs::create_dir_all(&tmp);
            let sp = std::ffi::CString::new(tmp.to_string_lossy().into_owned()).unwrap();
            let sp_ptr = sp.as_ptr();

            let active_id = format!("testgcactive{}", std::process::id());
            let active_c = std::ffi::CString::new(active_id.clone()).unwrap();
            let active_ptr = active_c.as_ptr();
            let data = std::ffi::CString::new(b"x|i:1;".to_vec()).unwrap();
            elephc_web_session_write(active_ptr, sp_ptr, data.as_ptr());

            // Backdate the active file well past any reasonable maxlifetime.
            let active_path = session_file_path(&tmp, &active_id);
            let old_secs = (std::time::SystemTime::now() - std::time::Duration::from_secs(7200))
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            set_file_timestamp(&active_path, old_secs);

            // Mark this ID as the active session (mirrors what session_start
            // does via elephc_web_session_set_id).
            super::super::state::elephc_web_session_set_id(active_ptr);

            let deleted = elephc_web_session_gc(sp_ptr, 3600);
            assert_eq!(deleted, 0, "active session file must not be GC'd");
            assert!(active_path.exists(), "active session file must survive GC");

            elephc_web_session_destroy(active_ptr, sp_ptr);
            let _ = fs::remove_dir_all(&tmp);
            elephc_web_session_reset();
        }
    }
}
