//! Purpose:
//! Encodes one bridge result as the packet string the PHP prelude decodes.
//!
//! Called from:
//! - `crate::ops` and `crate::abi` before stashing a result in the C cell.
//!
//! Key details:
//! - The first byte is the outcome: `O` success, `V` ValueError, `T` TypeError,
//!   `Z` DivisionByZeroError, `F` PHP false, `E` internal error.
//! - Payloads never contain NUL, so a packet always fits a `CString`, and multi-value
//!   results use `|` (GMP decimal text can never contain it).

/// Returns a successful packet carrying `payload`.
pub fn ok(payload: impl Into<String>) -> String {
    format!("O{}", payload.into())
}

/// Returns a PHP `false` packet.
pub fn false_value() -> String {
    "F".to_string()
}

/// Returns a `ValueError` packet carrying `message`.
pub fn value_error(message: impl Into<String>) -> String {
    format!("V{}", message.into())
}

/// Returns a `DivisionByZeroError` packet carrying `message`.
pub fn division_by_zero(message: impl Into<String>) -> String {
    format!("Z{}", sanitize(message.into()))
}

/// Returns an internal-error packet for an unimplemented or impossible operation.
pub fn internal_error(message: impl Into<String>) -> String {
    format!("E{}", sanitize(message.into()))
}

/// Replaces NUL bytes so an error message can cross a C string boundary.
fn sanitize(message: String) -> String {
    message.replace('\0', "?")
}
