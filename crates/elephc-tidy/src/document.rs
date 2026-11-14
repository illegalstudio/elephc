//! Purpose:
//! C ABI entry points for `tidy` document lifecycle, parsing, repair,
//! diagnostics, status, counts, and output serialization.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_tidy"` block declared by
//!   `elephc::tidy_prelude`.
//!
//! Key details:
//! - Every entry is panic-contained and fails closed on unknown handles.
//! - Parsing sets `initialized` before the libtidy call, matching php-src, so a
//!   failed parse still permits `tidy_get_status()`/counts but leaves
//!   `tidy_get_html_ver()` and friends throwing their initialized check in the
//!   prelude.
//! - Output and error buffers are copied with libtidy's trailing NUL removed,
//!   exactly as php-src's `FIX_BUFFER` / `size-1` reads do.

use std::ffi::{c_char, CString};

use crate::abi::{
    buffer_text, c_bytes, flag, guarded, read_bytes, stash, DOC_ERROR, DOC_OUTPUT, RELEASE_DATE,
};
use crate::ffi::{
    tidyAccessWarningCount, tidyCleanAndRepair, tidyConfigErrorCount, tidyDetectedGenericXml,
    tidyDetectedHtmlVersion, tidyDetectedXhtml, tidyErrorCount, tidyLoadConfig, tidyParseBuffer,
    tidyReleaseDate, tidyRunDiagnostics, tidySaveBuffer, tidySetCharEncoding, tidyStatus,
    tidyWarningCount, tidyBufAttach, tidyBufFree, tidyBufInit, TidyBuffer, TidyDoc,
};
use crate::registry::{
    create_document, drop_nodes_for_document, release_doc_locked, release_document, with_doc,
    with_doc_handle,
};

/// Serializes the current document into a fresh buffer and returns its text.
fn save_output(doc: *mut TidyDoc) -> Vec<u8> {
    let mut output = TidyBuffer::zeroed();
    unsafe {
        tidyBufInit(&mut output);
        tidySaveBuffer(doc, &mut output);
    }
    let text = buffer_text(&output);
    unsafe { tidyBufFree(&mut output) };
    text
}

/// C ABI: creates a document and returns its positive handle, or 0 on failure.
#[no_mangle]
pub extern "C" fn elephc_tidy_create() -> i64 {
    guarded(0, create_document)
}

/// C ABI: releases one document reference.
#[no_mangle]
pub extern "C" fn elephc_tidy_release(handle: i64) -> i64 {
    guarded(0, || release_document(handle))
}

/// C ABI: selects the input character encoding; negative means unsupported.
///
/// # Safety
/// `encoding` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_set_encoding(
    handle: i64,
    encoding: *const c_char,
) -> i64 {
    let bytes = c_bytes(encoding);
    guarded(-1, || {
        with_doc(handle, -1, |_, doc| {
            let Ok(encoding) = CString::new(bytes) else {
                return -1;
            };
            (unsafe { tidySetCharEncoding(doc, encoding.as_ptr()) }) as i64
        })
    })
}

/// C ABI: loads a libtidy configuration file; negative is I/O, positive is a
/// parse error, and 0 is success.
///
/// # Safety
/// `path` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_load_config(handle: i64, path: *const c_char) -> i64 {
    let bytes = c_bytes(path);
    guarded(-1, || {
        with_doc(handle, -1, |_, doc| {
            let Ok(path) = CString::new(bytes) else {
                return -1;
            };
            (unsafe { tidyLoadConfig(doc, path.as_ptr()) }) as i64
        })
    })
}

/// C ABI: parses exactly `len` bytes from `data`; returns 1 on success.
///
/// # Safety
/// `data` must point at `len` readable bytes when `len` is non-zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_tidy_parse(
    handle: i64,
    data: *const u8,
    len: i64,
) -> i64 {
    if len < 0 || len > u32::MAX as i64 {
        return 0;
    }
    let bytes = read_bytes(data, len as usize);
    guarded(0, || {
        with_doc_handle(handle, 0, |registry, doc_handle, doc| {
            // libtidy replaces the document's node tree on every parse. Invalidate
            // wrappers minted against the previous tree before the call releases it.
            //
            // `refs` always includes the caller's document object, but retain one extra
            // reference across the invalidation so a future registry change cannot release
            // the document while `doc` is still the pointer passed to `tidyParseBuffer`.
            if let Some(entry) = registry.docs.get_mut(&doc_handle) {
                entry.refs += 1;
                entry.initialized = true;
            }
            drop_nodes_for_document(registry, doc_handle);
            let mut input = TidyBuffer::zeroed();
            tidyBufInit(&mut input);
            tidyBufAttach(&mut input, bytes.as_ptr() as *mut u8, bytes.len() as u32);
            let status = tidyParseBuffer(doc, &mut input);
            release_doc_locked(registry, doc_handle);
            flag(status >= 0)
        })
    })
}

/// C ABI: runs cleanup and repair; returns 1 on success.
///
/// libtidy's cleaner frees pruned nodes, so wrappers minted against the previous tree
/// must be invalidated before the call exactly like a parse. Keep one extra document
/// reference across the invalidation so `doc` cannot be released before the call.
#[no_mangle]
pub extern "C" fn elephc_tidy_clean_repair(handle: i64) -> i64 {
    guarded(0, || {
        with_doc_handle(handle, 0, |registry, doc_handle, doc| {
            if let Some(entry) = registry.docs.get_mut(&doc_handle) {
                entry.refs += 1;
            }
            drop_nodes_for_document(registry, doc_handle);
            let status = unsafe { tidyCleanAndRepair(doc) };
            release_doc_locked(registry, doc_handle);
            flag(status >= 0)
        })
    })
}

/// C ABI: runs diagnostics; returns 1 when the document is initialized and
/// diagnostics succeed.
#[no_mangle]
pub extern "C" fn elephc_tidy_diagnose(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |entry, doc| {
            if !entry.initialized {
                return 0;
            }
            flag(unsafe { tidyRunDiagnostics(doc) } >= 0)
        })
    })
}

/// C ABI: returns whether a parse has been attempted.
#[no_mangle]
pub extern "C" fn elephc_tidy_initialized(handle: i64) -> i64 {
    guarded(0, || with_doc(handle, 0, |entry, _| flag(entry.initialized)))
}

/// C ABI: serializes the document and returns the output text.
#[no_mangle]
pub extern "C" fn elephc_tidy_output(handle: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || with_doc(handle, Vec::new(), |_, doc| save_output(doc)));
    stash(&DOC_OUTPUT, bytes)
}

/// C ABI: returns whether `tidySaveBuffer` produced a buffer for the document.
///
/// php-src updates `tidy::$value` when the buffer size is nonzero, including the
/// trailing NUL of an empty document. The text entry point alone cannot distinguish
/// that empty string from "no output buffer".
#[no_mangle]
pub extern "C" fn elephc_tidy_output_present(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| {
            let mut output = TidyBuffer::zeroed();
            unsafe {
                tidyBufInit(&mut output);
                tidySaveBuffer(doc, &mut output);
            }
            let present = !output.bp.is_null() && output.size > 0;
            unsafe { tidyBufFree(&mut output) };
            flag(present)
        })
    })
}

/// C ABI: returns the accumulated error buffer text (empty when absent).
#[no_mangle]
pub extern "C" fn elephc_tidy_error_buffer(handle: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_doc(handle, Vec::new(), |entry, _| buffer_text(&entry.errbuf))
    });
    stash(&DOC_ERROR, bytes)
}

/// C ABI: returns whether the error buffer holds at least one message.
#[no_mangle]
pub extern "C" fn elephc_tidy_error_buffer_present(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |entry, _| {
            flag(!entry.errbuf.bp.is_null() && entry.errbuf.size > 0)
        })
    })
}

/// C ABI: returns the document status (0 clean, 1 warnings, 2 errors).
#[no_mangle]
pub extern "C" fn elephc_tidy_status(handle: i64) -> i64 {
    guarded(0, || with_doc(handle, 0, |_, doc| unsafe { tidyStatus(doc) } as i64))
}

/// C ABI: returns the detected HTML version.
#[no_mangle]
pub extern "C" fn elephc_tidy_html_version(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| unsafe { tidyDetectedHtmlVersion(doc) } as i64)
    })
}

/// C ABI: returns whether the document is XHTML.
#[no_mangle]
pub extern "C" fn elephc_tidy_is_xhtml(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| flag(unsafe { tidyDetectedXhtml(doc) } != 0))
    })
}

/// C ABI: returns whether the document is generic XML.
#[no_mangle]
pub extern "C" fn elephc_tidy_is_xml(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| flag(unsafe { tidyDetectedGenericXml(doc) } != 0))
    })
}

/// C ABI: returns the document's error count.
#[no_mangle]
pub extern "C" fn elephc_tidy_error_count(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| unsafe { tidyErrorCount(doc) } as i64)
    })
}

/// C ABI: returns the document's warning count.
#[no_mangle]
pub extern "C" fn elephc_tidy_warning_count(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| unsafe { tidyWarningCount(doc) } as i64)
    })
}

/// C ABI: returns the document's accessibility warning count.
#[no_mangle]
pub extern "C" fn elephc_tidy_access_count(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| unsafe { tidyAccessWarningCount(doc) } as i64)
    })
}

/// C ABI: returns the document's configuration error count.
#[no_mangle]
pub extern "C" fn elephc_tidy_config_count(handle: i64) -> i64 {
    guarded(0, || {
        with_doc(handle, 0, |_, doc| unsafe { tidyConfigErrorCount(doc) } as i64)
    })
}

/// C ABI: returns libtidy's release date.
#[no_mangle]
pub extern "C" fn elephc_tidy_release_date() -> *const c_char {
    let bytes = guarded(Vec::new(), || unsafe {
        let pointer = tidyReleaseDate();
        if pointer.is_null() {
            b"unknown".to_vec()
        } else {
            std::ffi::CStr::from_ptr(pointer).to_bytes().to_vec()
        }
    });
    stash(&RELEASE_DATE, bytes)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pure helper tests for document buffer handling.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These tests never call libtidy.

    use super::*;

    /// An empty buffer reads as empty text.
    #[test]
    fn empty_buffer_has_no_text() {
        assert!(buffer_text(&TidyBuffer::zeroed()).is_empty());
    }

    /// A null output pointer reads as empty text even with a stale size.
    #[test]
    fn null_buffer_pointer_is_empty() {
        let mut buffer = TidyBuffer::zeroed();
        buffer.size = 4;
        assert!(buffer_text(&buffer).is_empty());
    }
}
