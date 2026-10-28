//! Purpose:
//! The Zend allocator family (`emalloc`, `efree`, `safe_emalloc`...), mapped
//! onto the C heap.
//!
//! Called from:
//! - Extension code, and every engine module that builds Zend values.
//!
//! Key details:
//! - Blocks come from the C heap, so a block from `malloc` may be released
//!   with `efree`, which extensions occasionally rely on.
//! - PHP's request allocator frees a whole request at once
//!   (`shutdown_memory_manager`). Here, a block the request allocator family
//!   (`emalloc`, `ecalloc`, `safe_emalloc`, `estrdup`...) hands out while a
//!   request is running is recorded, and request deactivation frees whatever
//!   the request left (`sweep_request_heap`), so a long-lived `--web` worker
//!   does not grow with every request. The persistent family (`pemalloc`,
//!   `zend_strndup`), module startup and the engine's own long-lived
//!   structures are not recorded. `efree`, `erealloc` and the engine's `free`
//!   keep the record current, so the sweep never frees a block twice.
//! - Allocation failure and size overflow are fatal, as in PHP.
//! - Every block this module hands out or releases moves `LIVE_BLOCKS`, so a
//!   test can tell a steady state from a leak (`elephc_zend_live_blocks`).

use std::collections::HashSet;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::global::Global;

/// Whether request-family allocations are being recorded: a request is running and no module
/// startup (MINIT) is.
static RECORDING: Global<bool> = Global::new(false);
/// The live blocks the request allocator family handed out during the current request.
static REQUEST_BLOCKS: Global<Option<HashSet<usize>>> = Global::new(None);

/// The current request's record of request-family blocks, created on first use.
fn request_blocks() -> &'static mut HashSet<usize> {
    unsafe { REQUEST_BLOCKS.get().get_or_insert_with(HashSet::new) }
}

/// Starts (or stops) recording request-family allocations; returns the previous state so a
/// caller can restore it (module startup pauses recording inside a request).
pub fn set_request_recording(on: bool) -> bool {
    unsafe { std::mem::replace(RECORDING.get(), on) }
}

/// Records a block the request allocator family just handed out, when a request is running.
fn record(ptr: *mut c_void) -> *mut c_void {
    if unsafe { *RECORDING.get() } {
        request_blocks().insert(ptr as usize);
    }
    ptr
}

/// Records a block a `pemalloc`-style allocation handed out unless it is persistent: strings,
/// arrays, hash storage and objects come from php-src's request allocator unless their persistent
/// flag says otherwise, so the request's end frees the ones an extension leaks.
pub fn record_unless_persistent(ptr: *mut c_void, persistent: bool) -> *mut c_void {
    if persistent {
        ptr
    } else {
        record(ptr)
    }
}

/// Forgets a block that is being released or moved; true when it was a recorded request block.
fn forget(ptr: *mut c_void) -> bool {
    let blocks = unsafe { REQUEST_BLOCKS.get() };
    blocks.as_mut().is_some_and(|blocks| !blocks.is_empty() && blocks.remove(&(ptr as usize)))
}

/// `shutdown_memory_manager` for a finished request: stops recording and frees every
/// request-family block the request did not release itself. Called by request deactivation once
/// the request's objects, resources and callbacks are gone.
pub unsafe fn sweep_request_heap() {
    set_request_recording(false);
    let blocks = std::mem::take(request_blocks());
    for ptr in blocks {
        LIVE_BLOCKS.fetch_sub(1, Ordering::Relaxed);
        c_free(ptr as *mut c_void);
    }
}

/// Blocks allocated here and not yet released. Persistent allocations (class
/// entries, interned strings) stay counted for the program's life, so only a
/// difference between two moments measures a leak.
static LIVE_BLOCKS: AtomicI64 = AtomicI64::new(0);

/// The number of engine blocks currently allocated, for leak tests.
/// c: hidden
#[no_mangle]
pub extern "C" fn elephc_zend_live_blocks() -> i64 {
    LIVE_BLOCKS.load(Ordering::Relaxed)
}

/// Releases a block from this allocator (NULL is a no-op).
pub unsafe fn free(ptr: *mut c_void) {
    if !ptr.is_null() {
        LIVE_BLOCKS.fetch_sub(1, Ordering::Relaxed);
        forget(ptr);
    }
    c_free(ptr);
}

extern "C" {
    /// C library `malloc`: allocates `size` uninitialised bytes, returning NULL on failure.
    pub fn malloc(size: usize) -> *mut c_void;
    /// C library `calloc`: allocates `nmemb * size` zeroed bytes, returning NULL on failure or
    /// overflow.
    pub fn calloc(nmemb: usize, size: usize) -> *mut c_void;
    /// C library `realloc`: resizes a block from this allocator, possibly moving it; NULL on
    /// failure.
    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    /// C library `free`: releases a block from `malloc`/`calloc`/`realloc` (NULL is a no-op).
    #[link_name = "free"]
    fn c_free(ptr: *mut c_void);
}

/// Aborts with PHP's fatal "Out of memory (tried to allocate N bytes)" error; never returns.
fn out_of_memory(size: usize) -> ! {
    crate::error::fatal(&format!("Out of memory (tried to allocate {size} bytes)"))
}

/// Aborts with PHP's fatal "Possible integer overflow in memory allocation" error naming the
/// operands; never returns.
fn overflow(nmemb: usize, size: usize, offset: usize) -> ! {
    crate::error::fatal(&format!(
        "Possible integer overflow in memory allocation ({nmemb} * {size} + {offset})"
    ))
}

/// `malloc` that never returns NULL.
pub fn alloc(size: usize) -> *mut c_void {
    let ptr = unsafe { malloc(size.max(1)) };
    if ptr.is_null() {
        out_of_memory(size);
    }
    LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
    ptr
}

/// Zeroed `alloc`.
pub fn alloc_zeroed(size: usize) -> *mut c_void {
    let ptr = unsafe { calloc(1, size.max(1)) };
    if ptr.is_null() {
        out_of_memory(size);
    }
    LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
    ptr
}

/// A zeroed, typed allocation.
pub fn alloc_struct<T>() -> *mut T {
    alloc_zeroed(std::mem::size_of::<T>()).cast()
}

/// Computes `nmemb * size + offset`, raising the overflow fatal error instead of wrapping.
fn checked(nmemb: usize, size: usize, offset: usize) -> usize {
    nmemb
        .checked_mul(size)
        .and_then(|total| total.checked_add(offset))
        .unwrap_or_else(|| overflow(nmemb, size, offset))
}

/// `emalloc`: allocates `size` bytes from the C heap; fatal on exhaustion, never NULL.
#[no_mangle]
pub extern "C" fn emalloc(size: usize) -> *mut c_void {
    record(alloc(size))
}

/// `emalloc_large`: same as `emalloc`; the engine has no separate large-block allocator.
#[no_mangle]
pub extern "C" fn emalloc_large(size: usize) -> *mut c_void {
    record(alloc(size))
}

/// `emalloc_huge`: same as `emalloc`; the engine has no separate huge-block allocator.
#[no_mangle]
pub extern "C" fn emalloc_huge(size: usize) -> *mut c_void {
    record(alloc(size))
}

/// `ecalloc`: allocates `nmemb * size` zeroed bytes; fatal on overflow or exhaustion.
#[no_mangle]
pub extern "C" fn ecalloc(nmemb: usize, size: usize) -> *mut c_void {
    record(alloc_zeroed(checked(nmemb, size, 0)))
}

/// `erealloc`: resizes a block (a zero size still keeps a valid block); fatal on exhaustion.
#[no_mangle]
pub unsafe extern "C" fn erealloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    let recorded = !ptr.is_null() && forget(ptr);
    let grown = resize(ptr, size);
    if ptr.is_null() || recorded {
        record(grown);
    }
    grown
}

/// Resizes a block from this allocator without touching the request record; fatal on exhaustion.
unsafe fn resize(ptr: *mut c_void, size: usize) -> *mut c_void {
    let grown = realloc(ptr, size.max(1));
    if grown.is_null() {
        out_of_memory(size);
    }
    if ptr.is_null() {
        LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
    }
    grown
}

/// `efree`: releases a block from `emalloc` and friends; the pointer must come from this allocator
/// or be NULL.
#[no_mangle]
pub unsafe extern "C" fn efree(ptr: *mut c_void) {
    free(ptr);
}

/// `efree_size`: same as `efree`; the size hint is ignored.
#[no_mangle]
pub unsafe extern "C" fn efree_size(ptr: *mut c_void, _size: usize) {
    free(ptr);
}

/// `safe_emalloc`: allocates `nmemb * size + offset` bytes, fatal if the arithmetic overflows.
#[no_mangle]
pub extern "C" fn safe_emalloc(nmemb: usize, size: usize, offset: usize) -> *mut c_void {
    record(alloc(checked(nmemb, size, offset)))
}

/// `safe_erealloc`: resizes a block to `nmemb * size + offset` bytes, fatal if the arithmetic
/// overflows.
#[no_mangle]
pub unsafe extern "C" fn safe_erealloc(ptr: *mut c_void, nmemb: usize, size: usize, offset: usize) -> *mut c_void {
    erealloc(ptr, checked(nmemb, size, offset))
}

/// `estrndup`: copies `length` bytes of `s` into a new NUL-terminated block.
#[no_mangle]
pub unsafe extern "C" fn estrndup(s: *const c_char, length: usize) -> *mut c_char {
    record(strndup(s, length).cast()).cast()
}

/// Copies `length` bytes of `s` into a new NUL-terminated block, without touching the request
/// record.
unsafe fn strndup(s: *const c_char, length: usize) -> *mut c_char {
    let copy = alloc(checked(length, 1, 1)).cast::<c_char>();
    std::ptr::copy_nonoverlapping(s, copy, length);
    *copy.add(length) = 0;
    copy
}

/// `estrdup`: duplicates a NUL-terminated C string into a new allocation.
#[no_mangle]
pub unsafe extern "C" fn estrdup(s: *const c_char) -> *mut c_char {
    estrndup(s, std::ffi::CStr::from_ptr(s).to_bytes().len())
}

/// `zend_strndup`: the persistent spelling of `estrndup`: same copy, never swept with a request.
#[no_mangle]
pub unsafe extern "C" fn zend_strndup(s: *const c_char, length: usize) -> *mut c_char {
    strndup(s, length)
}

/// `__zend_malloc`: the persistent `pemalloc` backend: like `emalloc`, never swept with a request.
#[no_mangle]
pub extern "C" fn __zend_malloc(len: usize) -> *mut c_void {
    alloc(len)
}

/// `__zend_calloc`: the persistent `pecalloc` backend: zeroed, never swept with a request.
#[no_mangle]
pub extern "C" fn __zend_calloc(nmemb: usize, len: usize) -> *mut c_void {
    alloc_zeroed(checked(nmemb, len, 0))
}

/// `__zend_realloc`: the persistent `perealloc` backend: resizes without recording the block.
#[no_mangle]
pub unsafe extern "C" fn __zend_realloc(p: *mut c_void, len: usize) -> *mut c_void {
    // A request block an extension grows with `perealloc(p, n, 1)` moves: the sweep must follow
    // it, never free the old address.
    let recorded = !p.is_null() && forget(p);
    let grown = resize(p, len);
    if recorded {
        record(grown);
    }
    grown
}

/// `zend_object_alloc`: an object struct of `obj_size` bytes plus the
/// class's property slots, with everything before the embedded
/// `zend_object` zeroed.
#[no_mangle]
pub unsafe extern "C" fn zend_object_alloc(obj_size: usize, ce: *mut crate::abi::zend_class_entry) -> *mut c_void {
    let slots = crate::object::properties_size(ce);
    let total = (obj_size as isize + slots).max(obj_size as isize) as usize;
    // php-src's `emalloc`: an object nobody freed goes with its request.
    let obj = record(alloc(total));
    let head = obj_size.saturating_sub(std::mem::size_of::<crate::abi::zend_object>());
    std::ptr::write_bytes(obj.cast::<u8>(), 0, head);
    obj
}
