//! Purpose:
//! Pure-Rust hashing/HMAC bridge staticlib for elephc's PHP hash() family.
//! Exposes a C ABI of raw-digest functions keyed by algorithm name, consumed by
//! compiled PHP binaries via function-pointer slots (see src/codegen runtime).
//! Also exposes an incremental streaming ABI: `init`/`init_hmac`/`update`/`final`/`clone`/`free`.
//!
//! Called from:
//! - Compiled PHP program assembly through the `_elephc_crypto_*_fn` slots.
//! - `cargo test -p elephc-crypto` (the rlib) for in-isolation validation.
//!
//! Key details:
//! - All ABI functions are `#[no_mangle] pub extern "C"`; raw digests are written
//!   into a caller-provided 64-byte buffer (max digest size across supported algos).
//! - `ctx` handles are thin pointers to a boxed `HashCtx`. The boxed `Mixed`
//!   resource cell owns the handle for its whole lifetime: `free` is the sole
//!   destructor (driven by compiler scope-cleanup), while `final` finalizes a
//!   *clone* and leaves the original handle live. This keeps a context that is
//!   finalized and then dropped at scope exit a single, safe free.

mod algos;
mod hmac;

pub use algos::HashState;
use algos::make;
use std::os::raw::c_void;

/// Builds a byte slice from a possibly-null/zero-length C pointer pair.
unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Reads a UTF-8 algorithm name from a C pointer pair (lossy on invalid UTF-8,
/// which simply fails to match any known algorithm).
unsafe fn name_str<'a>(ptr: *const u8, len: usize) -> std::borrow::Cow<'a, str> {
    String::from_utf8_lossy(slice(ptr, len))
}

/// Computes a one-shot raw HMAC of `data` keyed by `key` under `name`, writing
/// the digest to `out` (64-byte buffer). Returns the digest length, or -1 for an
/// unknown algorithm or a non-crypto checksum (which PHP rejects for HMAC).
///
/// # Safety
/// All pointers must be valid for their stated lengths; `out` must hold 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_hmac(
    name_ptr: *const u8,
    name_len: usize,
    key_ptr: *const u8,
    key_len: usize,
    data_ptr: *const u8,
    data_len: usize,
    out_ptr: *mut u8,
) -> isize {
    let name = name_str(name_ptr, name_len);
    let digest = match hmac::hmac(&name, slice(key_ptr, key_len), slice(data_ptr, data_len)) {
        Some(d) => d,
        None => return -1,
    };
    std::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, digest.len());
    digest.len() as isize
}

/// Computes a one-shot raw digest of `data` under `name`, writing the bytes to
/// `out` (caller guarantees a 64-byte buffer). Returns the digest length, or -1
/// for an unknown algorithm.
///
/// # Safety
/// All pointers must be valid for their stated lengths; `out` must hold 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_hash(
    name_ptr: *const u8,
    name_len: usize,
    data_ptr: *const u8,
    data_len: usize,
    out_ptr: *mut u8,
) -> isize {
    let name = name_str(name_ptr, name_len);
    let mut st = match make(&name) {
        Some(s) => s,
        None => return -1,
    };
    st.update(slice(data_ptr, data_len));
    let digest = st.finalize_box();
    std::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, digest.len());
    digest.len() as isize
}

/// Heap context behind a `HashContext` handle: either a plain running digest or
/// an HMAC-streaming state (the inner digest already primed with K'⊕ipad).
enum HashCtx {
    Plain(Box<dyn HashState>),
    /// `opad_material` stores K'⊕0x5c (the outer-padding material), not the raw key.
    Hmac { algo: String, opad_material: Vec<u8>, inner: Box<dyn HashState> },
}

impl HashCtx {
    /// Feeds more input into the running context.
    fn update(&mut self, data: &[u8]) {
        match self {
            HashCtx::Plain(s) => s.update(data),
            HashCtx::Hmac { inner, .. } => inner.update(data),
        }
    }

    /// Finalizes the context into its raw digest, consuming it.
    fn finalize(self) -> Vec<u8> {
        match self {
            HashCtx::Plain(s) => s.finalize_box(),
            HashCtx::Hmac { algo, opad_material, inner } => {
                let inner_digest = inner.finalize_box();
                // constructively unreachable: make() is a static table and this algo was accepted
                // just above; note a panic here would abort across the extern "C" boundary.
                let mut outer = make(&algo).expect("hmac algo was validated at init");
                outer.update(&opad_material);
                outer.update(&inner_digest);
                outer.finalize_box()
            }
        }
    }

    /// Deep-clones the running context (backs hash_copy / `clone $ctx`).
    fn clone_box(&self) -> HashCtx {
        match self {
            HashCtx::Plain(s) => HashCtx::Plain(s.box_clone()),
            HashCtx::Hmac { algo, opad_material, inner } => HashCtx::Hmac {
                algo: algo.clone(),
                opad_material: opad_material.clone(),
                inner: inner.box_clone(),
            },
        }
    }
}

/// Creates a plain hashing context for `name`. Returns a `HashContext` handle,
/// or null for an unknown algorithm.
///
/// # Safety
/// `name_ptr` must be valid for `name_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_init(name_ptr: *const u8, name_len: usize) -> *mut c_void {
    let name = name_str(name_ptr, name_len);
    match make(&name) {
        Some(s) => Box::into_raw(Box::new(HashCtx::Plain(s))) as *mut c_void,
        None => std::ptr::null_mut(),
    }
}

/// Creates an HMAC-streaming context (PHP `hash_init($algo, HASH_HMAC, $key)`).
/// Returns a handle, or null for an unknown algorithm or a non-crypto checksum
/// (block_key rejects block_size==0, matching PHP's ValueError).
///
/// # Safety
/// `name_ptr`/`key_ptr` must be valid for their stated lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_init_hmac(
    name_ptr: *const u8,
    name_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> *mut c_void {
    let name = name_str(name_ptr, name_len).into_owned();
    let k = match hmac::block_key(&name, slice(key_ptr, key_len)) {
        Some(k) => k,
        None => return std::ptr::null_mut(),
    };
    let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    let opad_material: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    // constructively unreachable: make() is a static table and this algo was accepted
    // just above; note a panic here would abort across the extern "C" boundary.
    let mut inner = make(&name).expect("algo validated by block_key");
    inner.update(&ipad);
    Box::into_raw(Box::new(HashCtx::Hmac { algo: name, opad_material, inner })) as *mut c_void
}

/// Feeds `data` into the context.
///
/// # Safety
/// `ctx` must be a live handle from init/init_hmac/clone; `data_ptr` valid for `data_len`.
/// The caller must hold exclusive access to `*ctx` for the duration of the call — no other
/// live reference (shared or mutable) to the same handle may exist.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_update(
    ctx: *mut c_void,
    data_ptr: *const u8,
    data_len: usize,
) {
    if ctx.is_null() {
        return;
    }
    let ctx = &mut *(ctx as *mut HashCtx);
    ctx.update(slice(data_ptr, data_len));
}

/// Finalizes a *clone* of the context into `out`, leaving the original handle
/// live and owned by its boxed `Mixed` resource cell. Returns the digest length,
/// or -1 for a null handle.
///
/// The handle is intentionally NOT freed here: ownership stays with the boxed
/// resource cell, whose scope-cleanup destructor calls `elephc_crypto_free`
/// exactly once when the box is released. Finalizing a clone keeps the handle
/// valid afterwards, so a redundant `hash_final()`/`hash_update()` on the same
/// handle (which PHP rejects) stays memory-safe instead of being a use-after-free
/// or a double-free against scope cleanup.
///
/// # Safety
/// `ctx` must be a live handle; `out` must hold 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_final(ctx: *mut c_void, out_ptr: *mut u8) -> isize {
    if ctx.is_null() {
        return -1;
    }
    let ctx = &*(ctx as *mut HashCtx);
    let digest = ctx.clone_box().finalize();
    std::ptr::copy_nonoverlapping(digest.as_ptr(), out_ptr, digest.len());
    digest.len() as isize
}

/// Deep-clones a context, returning a new independent handle (null if input null).
///
/// # Safety
/// `ctx` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_clone(ctx: *mut c_void) -> *mut c_void {
    if ctx.is_null() {
        return std::ptr::null_mut();
    }
    let ctx = &*(ctx as *mut HashCtx);
    Box::into_raw(Box::new(ctx.clone_box())) as *mut c_void
}

/// Frees a context (scope-exit / error cleanup) without finalizing.
///
/// This is the single owner that frees a `HashContext`: the compiler's Resource
/// scope-cleanup boxes the handle as a `Mixed` resource cell (tag 9, kind 2) and
/// releases it here through `__rt_mixed_free_deep` → `__rt_hash_ctx_free` when the
/// owning variable leaves scope. Because `elephc_crypto_final` no longer frees,
/// finalizing a context and then dropping it at scope exit is a single free with
/// no double-free.
///
/// # Safety
/// `ctx` must be a live handle and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn elephc_crypto_free(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    drop(Box::from_raw(ctx as *mut HashCtx));
}
