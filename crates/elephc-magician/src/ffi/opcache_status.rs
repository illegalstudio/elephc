//! Purpose:
//! Exports the runtime script cache's figures to natively compiled code, so an AOT
//! `opcache_get_status()` can report the dynamic tier instead of only the compile-time
//! manifest.
//!
//! Called from:
//! - Generated EIR backend assembly, through the `__elephc_eval_opcache_rt_*` symbols.
//!
//! Key details:
//! - Codegen emits these calls ONLY when this binary already links the eval bridge
//!   (`RuntimeFeatures::eval_bridge`); otherwise it folds the empty-cache answer in
//!   place. A binary with no dynamic tier therefore never references this module, which
//!   is what keeps `opcache_get_status()` from dragging the interpreter in.
//! - The key and field numbers are an ABI shared verbatim with the compiler through a
//!   `#[path]` include of `src/opcache/rt_status_keys.rs`.
//! - `__elephc_eval_opcache_rt_script_path` hands back a borrowed `(ptr, len)` into a
//!   THREAD-LOCAL buffer, never a process-global one: a `static` buffer handing out
//!   pointers is a cross-thread use-after-free, and `tests/ffi_buffer_hygiene.rs`
//!   enforces the rule. The caller copies the bytes with `__rt_str_persist` before the
//!   next call can overwrite them.

use std::cell::RefCell;

#[path = "../../../../src/opcache/rt_status_keys.rs"]
mod rt_status_keys;

use rt_status_keys::{
    rt_stat_value, RT_SCRIPT_HITS, RT_SCRIPT_LAST_USED, RT_SCRIPT_MEMORY, RT_SCRIPT_TIMESTAMP,
};

/// A borrowed `(pointer, length)` pair, returned in the first two result registers.
#[repr(C)]
pub struct BorrowedStr {
    /// Pointer to the bytes, or null when there is nothing to return.
    pub ptr: *const u8,
    /// Byte length, `0` when `ptr` is null.
    pub len: usize,
}

thread_local! {
    /// Holds the bytes of the most recently requested script path.
    ///
    /// Thread-local rather than a `static`: the pointer escapes to the caller, and a
    /// process-global buffer would let another thread's call free what this caller is
    /// still reading.
    static SCRIPT_PATH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };

    /// Holds the bytes of the most recently requested blacklist pattern.
    ///
    /// Its OWN buffer, not shared with `SCRIPT_PATH`: the prelude reads a script path and a
    /// blacklist entry in the same `opcache_get_status()` / `opcache_get_configuration()`
    /// pair, and one buffer would let the second read invalidate the first's borrow.
    static BLACKLIST_ENTRY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Returns one runtime script-cache figure, selected by `key`.
///
/// Unknown keys answer `0`, which is also what an empty cache answers, so a binary
/// compiled against a newer or older key space degrades rather than misreports.
#[no_mangle]
pub extern "C" fn __elephc_eval_opcache_rt_stat(key: i64) -> i64 {
    let stats = crate::script_cache::stats();
    rt_stat_value(
        key,
        stats.hits,
        stats.misses,
        stats.num_cached_scripts,
        stats.used_memory,
        stats.cache_full,
        stats.manual_restarts,
        stats.last_restart_time,
        stats.restart_pending,
        stats.blacklist_misses,
        crate::script_cache::blacklist_patterns().len(),
    )
}

/// Returns one cached script's numeric field, selected by `index` and `field`.
///
/// An out-of-range index or an unknown field answers `0`, matching the empty-cache
/// answer rather than trapping across the ABI.
#[no_mangle]
pub extern "C" fn __elephc_eval_opcache_rt_script_field(index: i64, field: i64) -> i64 {
    let scripts = crate::script_cache::cached_scripts();
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    let Some(script) = scripts.get(index) else {
        return 0;
    };
    match field {
        RT_SCRIPT_HITS => script.hits as i64,
        RT_SCRIPT_MEMORY => script.memory_consumption as i64,
        RT_SCRIPT_LAST_USED => script.last_used_timestamp,
        RT_SCRIPT_TIMESTAMP => script.timestamp,
        _ => 0,
    }
}

/// Returns one cached script's canonical path as borrowed bytes.
///
/// The bytes live in this thread's `SCRIPT_PATH` buffer and stay valid only until the
/// next call on this thread. An out-of-range index answers a null pointer with length
/// `0`, which the caller turns into the empty string.
#[no_mangle]
pub extern "C" fn __elephc_eval_opcache_rt_script_path(index: i64) -> BorrowedStr {
    let scripts = crate::script_cache::cached_scripts();
    let path = usize::try_from(index)
        .ok()
        .and_then(|index| scripts.get(index))
        .map(|script| script.full_path.clone());
    let Some(path) = path else {
        return BorrowedStr {
            ptr: std::ptr::null(),
            len: 0,
        };
    };
    SCRIPT_PATH.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        buffer.clear();
        buffer.extend_from_slice(path.as_bytes());
        BorrowedStr {
            ptr: buffer.as_ptr(),
            len: buffer.len(),
        }
    })
}

/// Returns one loaded blacklist pattern as borrowed bytes.
///
/// Same contract as `__elephc_eval_opcache_rt_script_path`: the bytes live in this thread's
/// buffer and stay valid only until the next call on this thread, and an out-of-range index
/// answers a null pointer with length `0` for the caller to turn into the empty string.
#[no_mangle]
pub extern "C" fn __elephc_eval_opcache_rt_blacklist_entry(index: i64) -> BorrowedStr {
    let patterns = crate::script_cache::blacklist_patterns();
    let pattern = usize::try_from(index)
        .ok()
        .and_then(|index| patterns.get(index))
        .cloned();
    let Some(pattern) = pattern else {
        return BorrowedStr {
            ptr: std::ptr::null(),
            len: 0,
        };
    };
    BLACKLIST_ENTRY.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        buffer.clear();
        buffer.extend_from_slice(pattern.as_bytes());
        BorrowedStr {
            ptr: buffer.as_ptr(),
            len: buffer.len(),
        }
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the C-ABI answers for an empty cache and for out-of-range requests, which are
    //! the shapes generated code has to survive.
    //!
    //! Called from:
    //! - `cargo test -p elephc-magician` through Rust's test harness.
    //!
    //! Key details:
    //! - These take the shared script-cache test guard: the cache is a process-wide
    //!   singleton, so the "empty" assertions need it actually empty.

    use super::*;
    use rt_status_keys::{RT_STAT_HITS, RT_STAT_MISSES, RT_STAT_SCRIPT_COUNT};

    /// Verifies an empty cache reports zeros rather than anything fabricated.
    #[test]
    fn an_empty_cache_reports_zeros() {
        let _guard = crate::script_cache::store::lock_for_test();

        assert_eq!(__elephc_eval_opcache_rt_stat(RT_STAT_HITS), 0);
        assert_eq!(__elephc_eval_opcache_rt_stat(RT_STAT_MISSES), 0);
        assert_eq!(__elephc_eval_opcache_rt_stat(RT_STAT_SCRIPT_COUNT), 0);
    }

    /// Verifies an unknown key answers `0` instead of trapping.
    #[test]
    fn an_unknown_key_answers_zero() {
        let _guard = crate::script_cache::store::lock_for_test();

        assert_eq!(__elephc_eval_opcache_rt_stat(4242), 0);
    }

    /// Verifies an out-of-range script index answers zero and a null path.
    #[test]
    fn an_out_of_range_index_answers_zero_and_null() {
        let _guard = crate::script_cache::store::lock_for_test();

        assert_eq!(__elephc_eval_opcache_rt_script_field(7, RT_SCRIPT_HITS), 0);
        let path = __elephc_eval_opcache_rt_script_path(7);
        assert!(path.ptr.is_null());
        assert_eq!(path.len, 0);
    }

    /// Verifies a negative index is refused rather than wrapping into a valid slot.
    #[test]
    fn a_negative_index_is_refused() {
        let _guard = crate::script_cache::store::lock_for_test();

        assert_eq!(__elephc_eval_opcache_rt_script_field(-1, RT_SCRIPT_HITS), 0);
        assert!(__elephc_eval_opcache_rt_script_path(-1).ptr.is_null());
    }
}
