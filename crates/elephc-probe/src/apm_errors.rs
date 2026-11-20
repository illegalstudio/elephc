//! Purpose:
//! The errors behind failed transactions, grouped: per transaction, per exception class and
//! per place it was thrown from (`file:line`), counted in the shared region so every worker
//! of a service counts into one table, and read into the snapshot with the routes.
//!
//! Called from:
//! - `elephc_probe_note_error`, which compiled code calls where an exception is caught on
//!   its way out of a transaction: the `--web` handler's catch-all, Symfony's
//!   `HttpKernel::handleThrowable`, Messenger's failure event, a named boundary's wrap
//!   (`crate::apm_prelude` in the compiler).
//! - `apm_runtime::record_transaction`, which commits the noted error once the transaction's
//!   outcome is known, and `apm_runtime::read_snapshot` / `reset_routes_exclusive`.
//!
//! Key details:
//! - An error is only noted while its transaction runs, and only counted when that
//!   transaction ends as a failure (a 5xx, a failed message or boundary): a framework turns
//!   a missing page into an exception too, and a 404 is no error of the service.
//! - The first error noted in a transaction is its error: what a failed renderer throws
//!   after it is a consequence, not the cause.
//! - A row is claimed by a compare-exchange on its key, a hash of the transaction and the
//!   error, and published with a release store once its text is written; a snapshot reads
//!   only published rows. 256 rows; past that, new errors are not grouped.
//! - Rows hold the route id, so the route-table reset clears them with the routes.

use std::sync::atomic::{AtomicU64, Ordering};

/// Rows of the error table.
pub(crate) const ERROR_SLOTS: usize = 256;
/// Bytes per row: its header words, then the text `class\tfile:line`.
const ERROR_ROW_BYTES: usize = 512;
/// Header words: key, published, count, route id, text length, then the trace id of the
/// error's last occurrence as 32 hex digits in four words (zero without one).
const HEAD_WORDS: usize = 9;
/// Room for the text in a row.
const TEXT_MAX: usize = ERROR_ROW_BYTES - HEAD_WORDS * 8;
/// Bytes the table takes in the shared region.
pub(crate) const ERROR_TABLE_BYTES: usize = ERROR_SLOTS * ERROR_ROW_BYTES;

const WORD_KEY: usize = 0;
const WORD_PUBLISHED: usize = 1;
const WORD_COUNT: usize = 2;
const WORD_ROUTE: usize = 3;
const WORD_LEN: usize = 4;
const WORD_TRACE0: usize = 5;

thread_local! {
    /// The error the current transaction noted, `class\tfile:line`; empty for none. Per
    /// thread: a transaction is noted, ended and committed on the thread that runs its PHP.
    static PENDING: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// One row's header word.
///
/// # Safety
/// `base` must be the mapped region, `slot < ERROR_SLOTS`, `word < HEAD_WORDS`.
unsafe fn row_word<'a>(base: usize, slot: usize, word: usize) -> &'a AtomicU64 {
    &*((base + crate::ERROR_TABLE_OFFSET + slot * ERROR_ROW_BYTES + word * 8) as *const AtomicU64)
}

/// Where a row's text starts.
fn row_text(base: usize, slot: usize) -> usize {
    base + crate::ERROR_TABLE_OFFSET + slot * ERROR_ROW_BYTES + HEAD_WORDS * 8
}

/// A piece of text with no tab or line break, which the table and the snapshot separate
/// fields with.
fn one_field(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// Notes the error of the current transaction, unless it noted one already.
pub(crate) fn note(class: &str, file: &str, line: i64) {
    PENDING.with_borrow_mut(|pending| {
        if pending.is_empty() {
            *pending = format!("{}\t{}:{line}", one_field(class.trim_start_matches('\\')), one_field(file));
        }
    });
}

/// Forgets the noted error: a new transaction starts.
pub(crate) fn clear() {
    PENDING.with_borrow_mut(String::clear);
}

/// Counts the noted error on `route_id` when the transaction failed, and forgets it.
pub(crate) fn commit(base: usize, route_id: usize, failed: bool) {
    let text = PENDING.with_borrow_mut(std::mem::take);
    if failed && !text.is_empty() {
        count(base, route_id, &text, &crate::apm_trace::current_trace());
    }
}

/// Writes a trace id into a row's trace words: the last occurrence's, so a reader may see
/// one occurrence's id mixed with the next one's, never anything but hex digits.
fn write_trace(base: usize, slot: usize, trace: &str) {
    let mut bytes = [0u8; 32];
    if trace.len() == 32 {
        bytes.copy_from_slice(trace.as_bytes());
    }
    for (index, chunk) in bytes.chunks(8).enumerate() {
        let word = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
        unsafe { row_word(base, slot, WORD_TRACE0 + index) }.store(word, Ordering::Relaxed);
    }
}

/// A row's trace id; empty without one.
fn read_trace(base: usize, slot: usize) -> String {
    let bytes: Vec<u8> = (0..4)
        .flat_map(|index| unsafe { row_word(base, slot, WORD_TRACE0 + index) }.load(Ordering::Relaxed).to_le_bytes())
        .collect();
    if bytes.iter().all(u8::is_ascii_hexdigit) {
        String::from_utf8(bytes).unwrap_or_default()
    } else {
        String::new()
    }
}

/// The row key of an error on a route: FNV-1a over both, never zero (an empty row).
fn key(route_id: usize, text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in (route_id as u64).to_le_bytes().iter().chain(text.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash | 1
}

/// Adds one to the row of `text` on `route_id`, claiming one when there is none, and keeps
/// `trace` as the trace of its last occurrence.
fn count(base: usize, route_id: usize, text: &str, trace: &str) {
    let mut len = text.len().min(TEXT_MAX);
    while !text.is_char_boundary(len) {
        len -= 1;
    }
    let text = &text[..len];
    let key = key(route_id, text);
    for probe in 0..ERROR_SLOTS {
        let slot = (key as usize).wrapping_add(probe) % ERROR_SLOTS;
        let key_word = unsafe { row_word(base, slot, WORD_KEY) };
        match key_word.compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => unsafe {
                row_word(base, slot, WORD_ROUTE).store(route_id as u64, Ordering::Relaxed);
                std::ptr::copy_nonoverlapping(text.as_ptr(), row_text(base, slot) as *mut u8, text.len());
                row_word(base, slot, WORD_LEN).store(text.len() as u64, Ordering::Relaxed);
                write_trace(base, slot, trace);
                row_word(base, slot, WORD_COUNT).fetch_add(1, Ordering::Relaxed);
                row_word(base, slot, WORD_PUBLISHED).store(1, Ordering::Release);
                return;
            },
            Err(existing) if existing == key => {
                write_trace(base, slot, trace);
                unsafe { row_word(base, slot, WORD_COUNT) }.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(_) => {}
        }
    }
}

/// The published rows: route id, class, `file:line`, count, last trace id.
pub(crate) fn read(base: usize) -> Vec<(usize, String, String, u64, String)> {
    let mut rows = Vec::new();
    for slot in 0..ERROR_SLOTS {
        if unsafe { row_word(base, slot, WORD_PUBLISHED) }.load(Ordering::Acquire) != 1 {
            continue;
        }
        let count = unsafe { row_word(base, slot, WORD_COUNT) }.load(Ordering::Relaxed);
        let route_id = unsafe { row_word(base, slot, WORD_ROUTE) }.load(Ordering::Relaxed) as usize;
        let len = (unsafe { row_word(base, slot, WORD_LEN) }.load(Ordering::Relaxed) as usize).min(TEXT_MAX);
        let bytes = unsafe { std::slice::from_raw_parts(row_text(base, slot) as *const u8, len) };
        let text = String::from_utf8_lossy(bytes);
        let (class, location) = text.split_once('\t').unwrap_or((&text, ""));
        if count > 0 {
            rows.push((route_id, class.to_string(), location.to_string(), count, read_trace(base, slot)));
        }
    }
    rows
}

/// Empties the table, with the route table it refers to.
pub(crate) fn reset(base: usize) {
    for slot in 0..ERROR_SLOTS {
        // Unpublished first: a reader never takes a row being emptied for a whole one.
        unsafe { row_word(base, slot, WORD_PUBLISHED) }.store(0, Ordering::Release);
        for word in [WORD_COUNT, WORD_ROUTE, WORD_LEN, WORD_KEY, WORD_TRACE0, WORD_TRACE0 + 1, WORD_TRACE0 + 2, WORD_TRACE0 + 3] {
            unsafe { row_word(base, slot, word) }.store(0, Ordering::Relaxed);
        }
    }
}

/// Notes the error a transaction is failing with: its class, and the file and line it was
/// thrown from. Counted with the transaction, when it ends as a failure.
///
/// # Safety
/// `class` and `file` are null or NUL-terminated strings valid for this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_note_error(class: *const libc::c_char, file: *const libc::c_char, line: i64) {
    if !crate::apm_runtime::APM_ON.load(Ordering::Relaxed) || class.is_null() {
        return;
    }
    // Not from inside a nested boundary: its caller may catch the exception and fail later
    // with another, which must be the one counted. One the nested code lets through is
    // noted where it leaves the outer transaction (review).
    if crate::apm_runtime::NESTED_DEPTH.load(Ordering::Relaxed) > 0 {
        return;
    }
    let class = std::ffi::CStr::from_ptr(class).to_string_lossy();
    let file = if file.is_null() { "".into() } else { std::ffi::CStr::from_ptr(file).to_string_lossy() };
    note(&class, &file, line);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failed transaction counts its first error once per occurrence, in one row per
    /// transaction and error; a successful one counts none; a reset empties the table.
    #[test]
    fn errors_are_grouped_per_transaction_and_place() {
        let _guard = crate::tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut region = vec![0u64; crate::REGION_BYTES / 8 + 1];
        let base = region.as_mut_ptr() as usize;
        clear();
        for _ in 0..3 {
            note("\\App\\PaymentFailed", "/srv/src/Pay.php", 42);
            note("Twig\\Error\\RuntimeError", "/srv/templates/error.html.twig", 1);
            commit(base, 5, true);
        }
        note("App\\PaymentFailed", "/srv/src/Pay.php", 42);
        commit(base, 6, true);
        note("NotFoundHttpException", "/srv/vendor/Router.php", 9);
        commit(base, 5, false);
        let mut rows: Vec<(usize, String, String, u64)> = read(base).into_iter().map(|(id, class, at, n, _)| (id, class, at, n)).collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                (5, "App\\PaymentFailed".to_string(), "/srv/src/Pay.php:42".to_string(), 3),
                (6, "App\\PaymentFailed".to_string(), "/srv/src/Pay.php:42".to_string(), 1),
            ]
        );
        reset(base);
        assert!(read(base).is_empty());
        drop(region);
    }
}
