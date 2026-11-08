//! Purpose:
//! Exposes PHP's `proc_open()` family — spawning with a descriptor table, polling,
//! signalling and reaping — through a stable, panic-free C ABI of scalars and C strings.
//!
//! Called from:
//! - The `extern "elephc_proc"` block of the injected proc prelude (`src/proc_prelude`),
//!   linked into compiled PHP programs on demand.
//! - Unit tests through the crate's rlib form.
//!
//! Key details:
//! - Builders and processes live in two id-keyed tables and 0 means failure, so the
//!   prelude never handles a native pointer. Process ids start at [`PROCESS_ID_BASE`]:
//!   the id is the PHP resource's payload word, and the runtime's resource-id registry
//!   keys on payloads, so it must never collide with a descriptor (or the 0..2 standard
//!   streams the registry answers without a lookup) or a synthetic wrapper handle.
//! - PHP values (arrays, resources, warnings) stay on the PHP side: the prelude walks the
//!   descriptor spec and boxes the pipe descriptors this bridge hands back.
//! - `elephc_proc_last_error()` and `elephc_proc_command()` return thread-local buffers, never
//!   process-global ones (see `tests/ffi_buffer_hygiene.rs`).

mod spawn;
mod wait;

pub use wait::{
    FIELD_CACHED, FIELD_EXITCODE, FIELD_RUNNING, FIELD_SIGNALED, FIELD_STOPPED, FIELD_STOPSIG,
    FIELD_TERMSIG,
};

use spawn::Builder;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Mutex;
use wait::Process;

/// An id-keyed table of live bridge objects.
struct Table<T> {
    next: i64,
    entries: BTreeMap<i64, T>,
}

impl<T> Table<T> {
    /// Creates an empty table whose first id is `first`.
    const fn new(first: i64) -> Self {
        Self { next: first, entries: BTreeMap::new() }
    }
}

/// The first process id, far above every descriptor and synthetic stream handle.
pub const PROCESS_ID_BASE: i64 = 1 << 40;

static BUILDERS: Mutex<Table<Builder>> = Mutex::new(Table::new(1));
static PROCESSES: Mutex<Table<Process>> = Mutex::new(Table::new(PROCESS_ID_BASE));

thread_local! {
    /// The last failure text, owned per thread.
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
    /// The last command copy handed out by `elephc_proc_command`.
    static COMMAND: RefCell<CString> = RefCell::new(CString::default());
}

/// Runs `body` against the table behind `lock`.
fn with_table<T, R>(lock: &Mutex<Table<T>>, body: impl FnOnce(&mut Table<T>) -> R) -> R {
    let mut guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(&mut guard)
}

/// Inserts `value` and returns its new id.
fn insert<T>(lock: &Mutex<Table<T>>, value: T) -> i64 {
    with_table(lock, |table| {
        let id = table.next;
        table.next += 1;
        table.entries.insert(id, value);
        id
    })
}

/// Records one failure text for `elephc_proc_last_error()`.
fn record_error(message: String) {
    let message = CString::new(message.replace('\0', "")).unwrap_or_default();
    LAST_ERROR.with(|cell| *cell.borrow_mut() = message);
}

/// Copies a borrowed C string, treating null as empty.
///
/// # Safety
/// `value` must be null or point to a NUL-terminated string.
unsafe fn owned(value: *const c_char) -> CString {
    if value.is_null() {
        return CString::default();
    }
    CStr::from_ptr(value).to_owned()
}

/// Applies `body` to the live builder `builder`; returns 1 on success, 0 on failure.
fn with_builder(builder: i64, body: impl FnOnce(&mut Builder) -> Result<(), String>) -> i64 {
    let result = with_table(&BUILDERS, |table| match table.entries.get_mut(&builder) {
        Some(builder) => body(builder),
        None => Err("proc_open(): invalid process builder".to_string()),
    });
    match result {
        Ok(()) => 1,
        Err(message) => {
            record_error(message);
            0
        }
    }
}

/// Applies `body` to the live process `process`, or answers `missing`.
fn with_process<R>(process: i64, missing: R, body: impl FnOnce(&mut Process) -> R) -> R {
    with_table(&PROCESSES, |table| table.entries.get_mut(&process).map(body)).unwrap_or(missing)
}

/// Allocates a builder for one `proc_open()` call and returns its id.
#[no_mangle]
pub extern "C" fn elephc_proc_builder_new() -> i64 {
    insert(&BUILDERS, Builder::default())
}

/// Releases a builder that will not be spawned, closing every descriptor it opened.
#[no_mangle]
pub extern "C" fn elephc_proc_builder_free(builder: i64) {
    let removed = with_table(&BUILDERS, |table| table.entries.remove(&builder));
    drop(removed);
}

/// Selects the string command form, run as `/bin/sh -c <command>`.
///
/// # Safety
/// `command` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_builder_shell(builder: i64, command: *const c_char) -> i64 {
    let command = owned(command);
    with_builder(builder, |builder| {
        builder.shell = Some(command);
        Ok(())
    })
}

/// Appends one argv element of the array command form.
///
/// # Safety
/// `argument` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_builder_arg(builder: i64, argument: *const c_char) -> i64 {
    let argument = owned(argument);
    with_builder(builder, |builder| {
        builder.args.push(argument);
        Ok(())
    })
}

/// Sets the child's working directory.
///
/// # Safety
/// `cwd` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_builder_cwd(builder: i64, cwd: *const c_char) -> i64 {
    let cwd = owned(cwd);
    with_builder(builder, |builder| {
        builder.cwd = Some(cwd);
        Ok(())
    })
}

/// Replaces the inherited environment with an empty one; entries follow.
#[no_mangle]
pub extern "C" fn elephc_proc_builder_env_reset(builder: i64) -> i64 {
    with_builder(builder, |builder| {
        builder.env = Some(Vec::new());
        Ok(())
    })
}

/// Appends one `NAME=value` (or bare) entry to the replacement environment.
///
/// # Safety
/// `entry` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_builder_env(builder: i64, entry: *const c_char) -> i64 {
    let entry = owned(entry);
    with_builder(builder, |builder| {
        builder.env.get_or_insert_with(Vec::new).push(entry);
        Ok(())
    })
}

/// Stages `['pipe', $mode]` for child descriptor `index`.
///
/// # Safety
/// `mode` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_builder_pipe(builder: i64, index: i64, mode: *const c_char) -> i64 {
    let child_writes = owned(mode).as_bytes().first() == Some(&b'w');
    with_builder(builder, |builder| builder.pipe(index, child_writes))
}

/// Stages `['file', $path, $mode]` for child descriptor `index`.
///
/// # Safety
/// `path` and `mode` must be null or NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_proc_builder_file(
    builder: i64,
    index: i64,
    path: *const c_char,
    mode: *const c_char,
) -> i64 {
    let path = owned(path);
    let mode = owned(mode).to_string_lossy().into_owned();
    with_builder(builder, |builder| builder.file(index, &path, &mode))
}

/// Stages an existing stream's native descriptor `fd` for child descriptor `index`.
#[no_mangle]
pub extern "C" fn elephc_proc_builder_fd(builder: i64, index: i64, fd: i64) -> i64 {
    let fd = std::os::raw::c_int::try_from(fd).unwrap_or(-1);
    with_builder(builder, |builder| builder.inherit(index, fd))
}

/// Stages `['null']` for child descriptor `index`.
#[no_mangle]
pub extern "C" fn elephc_proc_builder_null(builder: i64, index: i64) -> i64 {
    with_builder(builder, |builder| builder.null(index))
}

/// Stages `['redirect', $target]` for child descriptor `index`.
#[no_mangle]
pub extern "C" fn elephc_proc_builder_redirect(builder: i64, index: i64, target: i64) -> i64 {
    with_builder(builder, |builder| builder.redirect(index, target))
}

/// Spawns a staged builder (always consuming it) and returns the new process id, or 0
/// after recording php-src's warning text.
#[no_mangle]
pub extern "C" fn elephc_proc_spawn(builder: i64) -> i64 {
    let Some(staged) = with_table(&BUILDERS, |table| table.entries.remove(&builder)) else {
        record_error("proc_open(): invalid process builder".to_string());
        return 0;
    };
    match staged.spawn() {
        Ok(spawned) => insert(
            &PROCESSES,
            Process::new(spawned.pid, spawned.command, spawned.pipes),
        ),
        Err(message) => {
            record_error(message);
            0
        }
    }
}

/// Returns the last failure text (a thread-local buffer valid until the next failure).
#[no_mangle]
pub extern "C" fn elephc_proc_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}

/// Returns how many pipes the process was spawned with.
#[no_mangle]
pub extern "C" fn elephc_proc_pipe_count(process: i64) -> i64 {
    with_process(process, 0, |process| process.pipes.len() as i64)
}

/// Returns the child descriptor number of pipe `position`, or -1.
#[no_mangle]
pub extern "C" fn elephc_proc_pipe_index(process: i64, position: i64) -> i64 {
    with_process(process, -1, |process| {
        usize::try_from(position)
            .ok()
            .and_then(|position| process.pipes.get(position))
            .map_or(-1, |(index, _)| *index)
    })
}

/// Returns the parent's descriptor of pipe `position`, or -1. Ownership moves to the
/// PHP stream the prelude boxes it into; `elephc_proc_close` only closes it while it
/// still names the spawned pipe (see `wait`).
#[no_mangle]
pub extern "C" fn elephc_proc_pipe_fd(process: i64, position: i64) -> i64 {
    with_process(process, -1, |process| {
        usize::try_from(position)
            .ok()
            .and_then(|position| process.pipes.get(position))
            .map_or(-1, |(_, fd)| i64::from(*fd))
    })
}

/// Returns the child's process id, or -1 for an unknown process.
#[no_mangle]
pub extern "C" fn elephc_proc_pid(process: i64) -> i64 {
    with_process(process, -1, |process| i64::from(process.pid))
}

/// Returns `proc_get_status()`'s `command` (a thread-local copy).
#[no_mangle]
pub extern "C" fn elephc_proc_command(process: i64) -> *const c_char {
    let command = with_process(process, CString::default(), |process| process.command.clone());
    COMMAND.with(|cell| {
        *cell.borrow_mut() = command;
        cell.borrow().as_ptr()
    })
}

/// Runs one non-blocking status observation; returns 1, or 0 for an unknown process.
#[no_mangle]
pub extern "C" fn elephc_proc_poll(process: i64) -> i64 {
    with_process(process, 0, |process| {
        process.poll();
        1
    })
}

/// Returns one field (`FIELD_*`) of the last observation, or -1.
#[no_mangle]
pub extern "C" fn elephc_proc_status_field(process: i64, field: i64) -> i64 {
    let field = std::os::raw::c_int::try_from(field).unwrap_or(-1);
    with_process(process, -1, |process| process.field(field))
}

/// Sends `signal` to the child; returns 1 when `kill()` succeeded.
#[no_mangle]
pub extern "C" fn elephc_proc_terminate(process: i64, signal: i64) -> i64 {
    let signal = std::os::raw::c_int::try_from(signal).unwrap_or(-1);
    with_process(process, 0, |process| {
        i64::from(unsafe { libc::kill(process.pid, signal) } == 0)
    })
}

/// Waits for the child and forgets it; returns `proc_close()`'s status.
#[no_mangle]
pub extern "C" fn elephc_proc_close(process: i64) -> i64 {
    let Some(mut record) = with_table(&PROCESSES, |table| table.entries.remove(&process)) else {
        return -1;
    };
    record.close()
}

/// `proc_nice()`: changes the priority by `increment`; returns 1 on success, 0 when the
/// C library reported an error (php-src tests errno, since -1 is a valid result).
#[no_mangle]
pub extern "C" fn elephc_proc_nice(increment: i64) -> i64 {
    let increment = std::os::raw::c_int::try_from(increment).unwrap_or(std::os::raw::c_int::MAX);
    unsafe {
        *errno_location() = 0;
        libc::nice(increment);
        i64::from(*errno_location() == 0)
    }
}

/// Returns a writable pointer to the calling thread's errno.
#[cfg(target_vendor = "apple")]
unsafe fn errno_location() -> *mut std::os::raw::c_int {
    libc::__error()
}

/// Returns a writable pointer to the calling thread's errno.
#[cfg(not(target_vendor = "apple"))]
unsafe fn errno_location() -> *mut std::os::raw::c_int {
    libc::__errno_location()
}

#[cfg(test)]
mod tests;
