//! Purpose:
//! Transfers operation-local libxml diagnostics to the compiled PHP warning dispatcher.
//!
//! Called from:
//! - Document parsing, XPath evaluation, and the diagnostic string C ABI.
//!
//! Key details:
//! - Native callbacks finish before PHP handlers run; consuming the string prevents stale reports.

use super::*;

thread_local! { static PENDING: RefCell<String> = RefCell::new(String::new()); }

/// Clears a previous operation's unconsumed messages before an early failure is possible.
pub(super) fn reset() { PENDING.with(|pending| pending.borrow_mut().clear()); }

/// Copies and frees the shim's completed operation-local buffer.
pub(super) unsafe fn capture(raw: *mut c_char) {
    let text = owned_string(raw);
    PENDING.with(|pending| *pending.borrow_mut() = text);
}

/// Consumes a diagnostic snapshot so nested PHP handlers cannot overwrite its contents.
pub(super) fn take() -> String { PENDING.with(|pending| std::mem::take(&mut *pending.borrow_mut())) }
