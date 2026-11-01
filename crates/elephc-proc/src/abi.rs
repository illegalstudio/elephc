//! Purpose:
//! Exposes `proc_open()` and its status calls through a stable, panic-free C ABI of integers and
//! NUL-terminated strings, the only shapes an elephc prelude `extern` block can pass.
//!
//! Called from:
//! - The AOT `proc_*` prelude (`src/proc_prelude.rs`) through `extern "elephc_proc"` calls.
//!
//! Key details:
//! - A builder is an opaque integer handle (a boxed `Builder`); `elephc_proc_free()` releases it.
//! - Strings are copied on entry, so the caller's temporary C string may be freed after the call.
//! - Calls on an unknown handle or token answer `-1` (or a null string) instead of panicking.
//! - Returned strings live in thread-local storage and are valid until the next call that returns
//!   a string on the same thread.

use crate::spawn::{Request, Source, Spawned};
use crate::table;
use libc::c_char;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::ffi::{CStr, CString};

/// A request under construction plus, after a successful spawn, the started child.
struct Builder {
    request: Request,
    spawned: Option<Spawned>,
}

thread_local! {
    static RETURNED: RefCell<CString> = RefCell::new(CString::default());
}

/// Borrows the builder behind a handle.
///
/// # Safety
/// `handle` must be zero or a value returned by `elephc_proc_new()` and not yet freed.
unsafe fn builder<'a>(handle: i64) -> Option<&'a mut Builder> {
    let pointer = builders().lock().ok()?.get(&handle).copied()?;
    (pointer as *mut Builder).as_mut()
}

/// Maps small integer handles to live builders.
///
/// A handle crosses the `extern` boundary as a PHP `int`, which the compiled caller narrows to 32
/// bits, so it is a counter value rather than a raw pointer.
fn builders() -> &'static Mutex<HashMap<i64, usize>> {
    static BUILDERS: OnceLock<Mutex<HashMap<i64, usize>>> = OnceLock::new();
    BUILDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Removes a handle from the registry and returns the builder it owned.
///
/// # Safety
/// `handle` must be zero or a value returned by `elephc_proc_new()` and not yet released.
unsafe fn take_builder(handle: i64) -> Option<Box<Builder>> {
    let pointer = builders().lock().ok()?.remove(&handle)?;
    Some(Box::from_raw(pointer as *mut Builder))
}

/// Copies a NUL-terminated string argument.
///
/// # Safety
/// `value` must be null or point to a NUL-terminated string.
unsafe fn copy(value: *const c_char) -> Option<CString> {
    if value.is_null() {
        return None;
    }
    Some(CStr::from_ptr(value).to_owned())
}

/// Hands a string back to the caller through thread-local storage.
fn returned(value: &str) -> *const c_char {
    RETURNED.with(|cell| {
        let converted = CString::new(value.replace('\0', "")).unwrap_or_default();
        *cell.borrow_mut() = converted;
        cell.borrow().as_ptr()
    })
}

/// Starts a request; `shell` is nonzero when the single argument is a shell command line.
#[no_mangle]
pub extern "C" fn elephc_proc_new(shell: i64) -> i64 {
    let builder = Box::new(Builder {
        request: Request {
            shell: shell != 0,
            args: Vec::new(),
            cwd: None,
            env: None,
            descriptors: Vec::new(),
        },
        spawned: None,
    });
    static NEXT: AtomicI64 = AtomicI64::new(1);
    let handle = NEXT.fetch_add(1, Ordering::Relaxed) & 0x3fff_ffff;
    match builders().lock() {
        Ok(mut table) => {
            table.insert(handle, Box::into_raw(builder) as usize);
            handle
        }
        Err(_) => 0,
    }
}

/// Releases a builder and every parent pipe end it still owns.
///
/// # Safety
/// `handle` must come from `elephc_proc_new()` and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_free(handle: i64) {
    if let Some(builder) = take_builder(handle) {
        if let Some(spawned) = &builder.spawned {
            for (_, fd) in &spawned.pipes {
                libc::close(*fd);
            }
        }
    }
}

/// Appends one argument (or the shell command line).
///
/// # Safety
/// `handle` must be live; `value` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_add_arg(handle: i64, value: *const c_char) -> i64 {
    match (builder(handle), copy(value)) {
        (Some(builder), Some(value)) => {
            builder.request.args.push(value);
            0
        }
        _ => -1,
    }
}

/// Sets the child's working directory.
///
/// # Safety
/// `handle` must be live; `path` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_set_cwd(handle: i64, path: *const c_char) -> i64 {
    match (builder(handle), copy(path)) {
        (Some(builder), Some(path)) => {
            builder.request.cwd = Some(path);
            0
        }
        _ => -1,
    }
}

/// Switches the child to an explicit environment, initially empty.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_begin_env(handle: i64) -> i64 {
    match builder(handle) {
        Some(builder) => {
            builder.request.env.get_or_insert_with(Vec::new);
            0
        }
        None => -1,
    }
}

/// Appends one `NAME=value` entry to the explicit environment.
///
/// # Safety
/// `handle` must be live; `entry` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_add_env(handle: i64, entry: *const c_char) -> i64 {
    match (builder(handle), copy(entry)) {
        (Some(builder), Some(entry)) => {
            builder.request.env.get_or_insert_with(Vec::new).push(entry);
            0
        }
        _ => -1,
    }
}

/// Declares a pipe at child descriptor `fd`; `child_reads` selects the pipe direction.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_desc_pipe(handle: i64, fd: i64, child_reads: i64) -> i64 {
    match builder(handle) {
        Some(builder) => {
            builder
                .request
                .descriptors
                .push((fd as i32, Source::Pipe { child_reads: child_reads != 0 }));
            0
        }
        None => -1,
    }
}

/// Declares a file opened with an `fopen()` mode at child descriptor `fd`.
///
/// # Safety
/// `handle` must be live; `path` and `mode` must be NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_desc_file(
    handle: i64,
    fd: i64,
    path: *const c_char,
    mode: *const c_char,
) -> i64 {
    match (builder(handle), copy(path), copy(mode)) {
        (Some(builder), Some(path), Some(mode)) => {
            builder.request.descriptors.push((
                fd as i32,
                Source::File { path, mode: mode.to_bytes().to_vec() },
            ));
            0
        }
        _ => -1,
    }
}

/// Declares an existing parent descriptor `source` at child descriptor `fd`.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_desc_fd(handle: i64, fd: i64, source: i64) -> i64 {
    match builder(handle) {
        Some(builder) => {
            builder.request.descriptors.push((fd as i32, Source::Fd(source as i32)));
            0
        }
        None => -1,
    }
}

/// Declares `/dev/null` at child descriptor `fd`.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_desc_null(handle: i64, fd: i64) -> i64 {
    match builder(handle) {
        Some(builder) => {
            builder.request.descriptors.push((fd as i32, Source::Null));
            0
        }
        None => -1,
    }
}

/// Declares that child descriptor `fd` duplicates child descriptor `target`.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_desc_redirect(handle: i64, fd: i64, target: i64) -> i64 {
    match builder(handle) {
        Some(builder) => {
            builder.request.descriptors.push((fd as i32, Source::Redirect(target as i32)));
            0
        }
        None => -1,
    }
}

/// Starts the child. Returns its pid, or the negated errno that stopped it.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_spawn(handle: i64) -> i64 {
    let Some(builder) = builder(handle) else { return -(libc::EINVAL as i64) };
    match crate::spawn::spawn(&builder.request) {
        Ok(spawned) => {
            let pid = i64::from(spawned.pid);
            builder.spawned = Some(spawned);
            pid
        }
        Err(code) => -i64::from(code),
    }
}

/// Returns the parent end of the pipe declared at child descriptor `fd`, or `-1`.
///
/// # Safety
/// `handle` must be live.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_parent_fd(handle: i64, fd: i64) -> i64 {
    let Some(builder) = builder(handle) else { return -1 };
    let Some(spawned) = &builder.spawned else { return -1 };
    spawned
        .pipes
        .iter()
        .find(|(child, _)| i64::from(*child) == fd)
        .map(|(_, parent)| i64::from(*parent))
        .unwrap_or(-1)
}

/// Moves the started child into the status table under `token` and releases the builder.
///
/// The pipe ends already belong to the caller's stream resources, so they are not closed.
///
/// # Safety
/// `handle` must be live and spawned; it must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_register(handle: i64, token: i64) -> i64 {
    let Some(owned) = take_builder(handle) else { return -1 };
    let Some(spawned) = &owned.spawned else { return -1 };
    let command = owned
        .request
        .args
        .first()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let pipe_fds: Vec<libc::c_int> = spawned.pipes.iter().map(|(_, fd)| *fd).collect();
    table::register(token, spawned.pid, &command, &pipe_fds);
    0
}

/// Polls the process under `token`; returns `1`, or `0` for an unknown token.
#[no_mangle]
pub extern "C" fn elephc_proc_poll(token: i64) -> i64 {
    i64::from(table::poll(token).is_some())
}

/// Reads one field of the last polled snapshot: 0 pid, 1 running, 2 signaled, 3 stopped,
/// 4 exitcode, 5 termsig, 6 stopsig, 7 cached. Unknown tokens and fields answer `-1`.
#[no_mangle]
pub extern "C" fn elephc_proc_info(token: i64, field: i64) -> i64 {
    let Some(snapshot) = table::last_snapshot(token) else { return -1 };
    match field {
        0 => table::describe(token).map(|(pid, _)| i64::from(pid)).unwrap_or(-1),
        1 => i64::from(snapshot.running),
        2 => i64::from(snapshot.signaled),
        3 => i64::from(snapshot.stopped),
        4 => snapshot.exitcode,
        5 => snapshot.termsig,
        6 => snapshot.stopsig,
        7 => i64::from(snapshot.cached),
        _ => -1,
    }
}

/// Returns the command text recorded for the process under `token`.
#[no_mangle]
pub extern "C" fn elephc_proc_command(token: i64) -> *const c_char {
    returned(&table::describe(token).map(|(_, command)| command).unwrap_or_default())
}

/// Waits for the process under `token`, forgets it, and returns `proc_close()`'s result.
/// An unknown token answers `-2` (distinct from the legitimate `-1`).
#[no_mangle]
pub extern "C" fn elephc_proc_close(token: i64) -> i64 {
    table::close(token).unwrap_or(-2)
}

/// Sends `signal` to the process under `token`; returns `1` when it was delivered.
#[no_mangle]
pub extern "C" fn elephc_proc_terminate(token: i64, signal: i64) -> i64 {
    i64::from(table::terminate(token, signal as i32).unwrap_or(false))
}

/// Returns whether `token` names a registered process.
#[no_mangle]
pub extern "C" fn elephc_proc_known(token: i64) -> i64 {
    i64::from(table::describe(token).is_some())
}

/// Adjusts the scheduling priority like `nice(3)`; returns the new niceness, or `-1000` on error.
#[no_mangle]
pub extern "C" fn elephc_proc_nice(increment: i64) -> i64 {
    unsafe {
        #[cfg(target_os = "linux")]
        let errno_slot = libc::__errno_location();
        #[cfg(target_os = "macos")]
        let errno_slot = libc::__error();
        *errno_slot = 0;
        let value = libc::nice(increment as libc::c_int);
        if value == -1 && *errno_slot != 0 {
            return -1000;
        }
        i64::from(value)
    }
}

/// Returns `strerror(code)` for a spawn failure code.
#[no_mangle]
pub extern "C" fn elephc_proc_strerror(code: i64) -> *const c_char {
    returned(&std::io::Error::from_raw_os_error(code as i32).to_string().split(" (os error").next().unwrap_or("").to_string())
}
