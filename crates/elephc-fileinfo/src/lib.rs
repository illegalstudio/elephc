//! Purpose:
//! Owns fileinfo's libmagic instances behind a panic-free, binary-safe C ABI.
//!
//! Called from:
//! - The compiler's fileinfo prelude through extern functions.
//!
//! Key details:
//! - Handles are monotonic IDs, never exposed pointers; destruction is idempotent.
//! - A mutex serializes access to each opaque instance, including flag restoration.
//! - Returned strings belong to the calling thread until its next bridge call.
//! - Native symbols resolve only when linking the managed libmagic package.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString, OsStr};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};

extern "C" {
    /// Creates a native cookie with the requested options.
    fn magic_open(flags: c_int) -> *mut c_void;
    /// Releases a native cookie and its database mappings.
    fn magic_close(cookie: *mut c_void);
    /// Loads a custom textual or compiled database.
    fn magic_load(cookie: *mut c_void, path: *const c_char) -> c_int;
    /// Loads the database embedded by the managed native recipe.
    fn elephc_magic_load_default(cookie: *mut c_void) -> c_int;
    /// Replaces the native cookie configuration flags.
    fn magic_setflags(cookie: *mut c_void, flags: c_int) -> c_int;
    /// Reads the current native cookie configuration flags.
    fn magic_getflags(cookie: *mut c_void) -> c_int;
    /// Identifies a borrowed binary byte span.
    fn magic_buffer(cookie: *mut c_void, data: *const c_void, length: usize) -> *const c_char;
    /// Identifies a borrowed open descriptor, preserving its position.
    fn magic_descriptor(cookie: *mut c_void, fd: c_int) -> *const c_char;
    /// Borrows the cookie error message until the next native operation.
    fn magic_error(cookie: *mut c_void) -> *const c_char;
}

/// One owned opaque pointer, accessed only while holding its instance mutex.
struct Magic(usize);
impl Drop for Magic {
    /// Releases exactly the native instance transferred to this owner.
    fn drop(&mut self) { unsafe { magic_close(self.0 as *mut c_void) }; }
}

#[derive(Default)]
struct Registry { next: i64, instances: HashMap<i64, Arc<Mutex<Magic>>> }

#[derive(Default)]
struct Reads { next: i64, buffers: HashMap<i64, Vec<u8>> }

/// Returns the process registry without exposing any interior pointer.
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// Retains one instance without keeping the registry locked during native work.
fn instance(handle: i64) -> Option<Arc<Mutex<Magic>>> {
    registry().lock().unwrap_or_else(|e| e.into_inner()).instances.get(&handle).cloned()
}

thread_local! {
    static RESULT: RefCell<CString> = RefCell::new(CString::default());
    static ERROR: RefCell<CString> = RefCell::new(CString::default());
    static SUCCESS: RefCell<bool> = const { RefCell::new(false) };
    static OPEN_FAILURE: RefCell<bool> = const { RefCell::new(false) };
    static READS: RefCell<Reads> = RefCell::new(Reads::default());
}

/// Copies a native string before libmagic can invalidate its backing storage.
unsafe fn copy_string(value: *const c_char) -> CString {
    if value.is_null() { CString::default() } else { CStr::from_ptr(value).to_owned() }
}

/// Stores a call's outcome without returning storage shared with other threads.
fn outcome(result: Option<CString>, error: CString) {
    OPEN_FAILURE.with(|slot| *slot.borrow_mut() = false);
    SUCCESS.with(|slot| *slot.borrow_mut() = result.is_some());
    RESULT.with(|slot| *slot.borrow_mut() = result.unwrap_or_default());
    ERROR.with(|slot| *slot.borrow_mut() = error);
}

/// Prevents a Rust panic from crossing the generated program's C ABI boundary.
fn guard<T: Default>(call: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_default()
}

/// Matches PHP fileinfo's directory-bit test, including socket and block-device mode bits.
fn directory_mode(mode: u32) -> bool { mode & 0o040000 != 0 }

/// Creates a native instance, loading a custom database or the embedded default.
/// The path must be a valid NUL-terminated string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_fileinfo_open(flags: i64, path: *const c_char) -> i64 {
    guard(|| {
        let path = match expand_database(path) {
            Ok(path) => path,
            Err(error) => { outcome(None, CString::new(error.to_string()).unwrap_or_default()); return 0; }
        };
        let cookie = magic_open(flags as c_int);
        if cookie.is_null() {
            outcome(None, CString::new(format!("Invalid mode '{flags}'.")).unwrap());
            return 0;
        }
        let instance = Magic(cookie as usize);
        let loaded = if path.is_empty() {
            elephc_magic_load_default(cookie)
        } else { magic_load(cookie, path.as_ptr()) };
        if loaded != 0 {
            let mut message = b"Failed to load magic database at \"".to_vec();
            message.extend_from_slice(path.as_bytes());
            message.push(b'"');
            outcome(None, CString::new(message).unwrap_or_default());
            return 0;
        }
        let mut state = registry().lock().unwrap_or_else(|e| e.into_inner());
        let Some(id) = state.next.checked_add(1) else { return 0; };
        state.next = id;
        state.instances.insert(id, Arc::new(Mutex::new(instance)));
        outcome(Some(CString::default()), CString::default());
        id
    })
}

/// Resolves custom database names like PHP's lexical CWD_EXPAND, retaining missing leaves.
unsafe fn expand_database(path: *const c_char) -> std::io::Result<CString> {
    if path.is_null() || CStr::from_ptr(path).is_empty() { return Ok(CString::default()); }
    let path = std::path::Path::new(OsStr::from_bytes(CStr::from_ptr(path).to_bytes()));
    let absolute = if path.is_absolute() { path.to_owned() } else { std::env::current_dir()?.join(path) };
    let mut normalized = std::path::PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir => { normalized.pop(); },
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(CString::new(normalized.as_os_str().as_bytes()).unwrap_or_default())
}

/// Releases an owned handle; an already released handle is harmless.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_free(handle: i64) -> i64 {
    guard(|| {
        let removed = registry().lock().unwrap_or_else(|e| e.into_inner()).instances.remove(&handle);
        drop(removed);
        1
    })
}

/// Changes the instance's persistent libmagic flags.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_set_flags(handle: i64, flags: i64) -> i64 {
    guard(|| {
        let Some(owner) = instance(handle) else { return 0; };
        let instance = owner.lock().unwrap_or_else(|e| e.into_inner());
        i64::from(unsafe { magic_setflags(instance.0 as *mut c_void, flags as c_int) } == 0)
    })
}

/// Reads the instance flags without holding its mutex while PHP stream callbacks execute.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_get_flags(handle: i64) -> i64 {
    guard(|| {
        let Some(owner) = instance(handle) else { return 0; };
        let instance = owner.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { i64::from(magic_getflags(instance.0 as *mut c_void)) }
    })
}

/// Queries bytes or a local filename, restoring per-call flags after success or failure.
unsafe fn query(handle: i64, data: *const c_char, length: i64, flags: i64, file: bool) {
    let Some(owner) = instance(handle) else {
        outcome(None, CString::new("Invalid finfo object").unwrap());
        return;
    };
    let instance = owner.lock().unwrap_or_else(|e| e.into_inner());
    let cookie = instance.0 as *mut c_void;
    let old = magic_getflags(cookie);
    let selected = if flags == 0 { old } else { flags as c_int };
    // PHP opens the stream first: paths follow symlinks and an unreadable file fails.
    magic_setflags(cookie, selected);
    let input = if file {
        match std::fs::File::open(std::path::Path::new(OsStr::from_bytes(CStr::from_ptr(data).to_bytes()))) {
            Ok(input) => Some(input),
            Err(error) => {
                magic_setflags(cookie, old);
                let message = error.to_string();
                let suffix = error.raw_os_error().map(|code| format!(" (os error {code})")).unwrap_or_default();
                outcome(None, CString::new(message.strip_suffix(&suffix).unwrap_or(&message)).unwrap_or_default());
                OPEN_FAILURE.with(|slot| *slot.borrow_mut() = true);
                return;
            }
        }
    } else { None };
    if input.as_ref().is_some_and(|file| file.metadata().is_ok_and(|metadata| directory_mode(metadata.mode()))) {
        magic_setflags(cookie, old);
        outcome(Some(CString::new("directory").unwrap()), CString::default());
        return;
    }
    let result = if let Some(input) = &input { magic_descriptor(cookie, input.as_raw_fd()) } else {
        magic_buffer(cookie, data.cast(), length.max(0) as usize)
    };
    let copied = (!result.is_null()).then(|| copy_string(result));
    let error = copy_string(magic_error(cookie));
    magic_setflags(cookie, old);
    outcome(copied, error);
}

/// Identifies exactly `length` bytes, including embedded NULs; data must span that length.
#[no_mangle]
pub unsafe extern "C" fn elephc_fileinfo_buffer(handle: i64, data: *const c_char, length: i64, flags: i64) -> *const c_char {
    guard(|| {
        query(handle, data, length, flags, false);
        RESULT.with(|slot| slot.borrow().as_ptr())
    })
}

/// Identifies a local NUL-terminated filename without depending on a system database.
#[no_mangle]
pub unsafe extern "C" fn elephc_fileinfo_file(handle: i64, path: *const c_char, flags: i64) -> *const c_char {
    guard(|| {
        query(handle, path, 0, flags, true);
        RESULT.with(|slot| slot.borrow().as_ptr())
    })
}

/// Reports whether the calling thread's last query succeeded.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_ok() -> i64 { SUCCESS.with(|slot| i64::from(*slot.borrow())) }

/// Returns the calling thread's last copied native error message.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_error() -> *const c_char { ERROR.with(|slot| slot.borrow().as_ptr()) }

/// Distinguishes descriptor-open failures from magic errors for PHP stream warning formatting.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_open_failed() -> i64 { OPEN_FAILURE.with(|slot| i64::from(*slot.borrow())) }

/// Copies a connector's captured errno or thread-local TLS cause into fileinfo-owned storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_fileinfo_network_error(code: i64, detail: *const c_char) -> *const c_char {
    guard(|| {
        let message = if code > 0 {
            std::io::Error::from_raw_os_error(code as c_int).to_string()
        } else if code < 0 {
            "Failed to resolve host".to_owned()
        } else if !detail.is_null() && !CStr::from_ptr(detail).is_empty() {
            CStr::from_ptr(detail).to_string_lossy().into_owned()
        } else { "operation failed".to_owned() };
        // Rust's native I/O display appends an implementation-specific errno suffix.
        let text = message.rsplit_once(" (os error ").filter(|(_, code)|
            code.strip_suffix(')').is_some_and(|code| !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_digit())))
            .map_or(message.as_str(), |(text, _)| text);
        ERROR.with(|slot| {
            *slot.borrow_mut() = CString::new(text).unwrap_or_default();
            slot.borrow().as_ptr()
        })
    })
}

/// Reads a nested plain-file resource into caller-owned bytes, bounded like PHP's magic stream window.
/// The path must be NUL-terminated; positive IDs must be released, -1 denotes a directory, and 0 fails.
#[no_mangle]
pub unsafe extern "C" fn elephc_fileinfo_read(path: *const c_char) -> i64 {
    guard(|| {
        use std::io::Read;
        let path = std::path::Path::new(OsStr::from_bytes(CStr::from_ptr(path).to_bytes()));
        let mut bytes = Vec::new();
        let read = std::fs::File::open(path).and_then(|file| {
            if directory_mode(file.metadata()?.mode()) { return Ok(true); }
            file.take(7340032).read_to_end(&mut bytes).map(|_| false)
        });
        if matches!(read, Ok(true)) { return -1; }
        if let Err(error) = read {
            outcome(None, CString::new(error.to_string()).unwrap_or_default());
            return 0;
        }
        READS.with(|reads| {
            let mut reads = reads.borrow_mut();
            let Some(id) = reads.next.checked_add(1) else { return 0; };
            reads.next = id;
            reads.buffers.insert(id, bytes);
            id
        })
    })
}

/// Borrows a read's bytes until that ID is released, independently of other or nested reads.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_read_data(id: i64) -> *const c_void {
    guard(|| READS.with(|reads| reads.borrow().buffers.get(&id).filter(|bytes| !bytes.is_empty())
        .map_or(c"".as_ptr().cast(), |bytes| bytes.as_ptr().cast())))
}

/// Returns the exact binary length of a caller-owned temporary read.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_read_length(id: i64) -> i64 {
    guard(|| READS.with(|reads| reads.borrow().buffers.get(&id).map_or(0, |bytes| bytes.len() as i64)))
}

/// Retires exactly one temporary read; releasing an already retired ID is harmless.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_read_release(id: i64) {
    guard(|| { READS.with(|reads| { reads.borrow_mut().buffers.remove(&id); }); });
}

#[cfg(all(test, elephc_fileinfo_native))]
mod tests;

/// Returns the number of live opaque instances for ownership diagnostics.
#[no_mangle]
pub extern "C" fn elephc_fileinfo_live_handles() -> i64 {
    guard(|| registry().lock().unwrap_or_else(|e| e.into_inner()).instances.len() as i64)
}
