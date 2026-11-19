//! Purpose:
//! Owns PHP's `PREG_*_ERROR` codes, their `preg_last_error_msg()` strings, and the
//! mapping from the managed PCRE2 shim's exec status to those codes.
//!
//! Called from:
//! - The PCRE constant catalog, the AOT `__rt_preg_exec` wrapper and message table, and
//!   Magician's eval regex engine and `preg_last_error_msg()`.
//!
//! Key details:
//! - The shim reports a match failure as a POSIX `REG_*` status or one of its private
//!   statuses; [`preg_error_for_exec_status`] turns each into the code php-src's
//!   `pcre_handle_exec_error()` would record. `REG_NOMATCH` is not an error.

/// `PREG_NO_ERROR`.
pub const PREG_NO_ERROR: i64 = 0;
/// `PREG_INTERNAL_ERROR`: an invalid pattern, PCRE2's heap limit or memory, anything else.
pub const PREG_INTERNAL_ERROR: i64 = 1;
/// `PREG_BACKTRACK_LIMIT_ERROR`: PCRE2's match limit.
pub const PREG_BACKTRACK_LIMIT_ERROR: i64 = 2;
/// `PREG_RECURSION_LIMIT_ERROR`: PCRE2's depth limit.
pub const PREG_RECURSION_LIMIT_ERROR: i64 = 3;
/// `PREG_BAD_UTF8_ERROR`: a malformed UTF-8 subject under `/u`.
pub const PREG_BAD_UTF8_ERROR: i64 = 4;
/// `PREG_BAD_UTF8_OFFSET_ERROR`: a start offset inside a UTF-8 code point.
pub const PREG_BAD_UTF8_OFFSET_ERROR: i64 = 5;
/// `PREG_JIT_STACKLIMIT_ERROR`. The managed PCRE2 build has no JIT, so it never occurs.
pub const PREG_JIT_STACKLIMIT_ERROR: i64 = 6;

/// PHP 8.5's `preg_last_error_msg()` strings, indexed by `PREG_*_ERROR` code.
pub const PREG_ERROR_MESSAGES: [&str; 7] = [
    "No error",
    "Internal error",
    "Backtrack limit exhausted",
    "Recursion limit exhausted",
    "Malformed UTF-8 characters, possibly incorrectly encoded",
    "The offset did not correspond to the beginning of a valid UTF-8 code point",
    "JIT stack limit exhausted",
];

/// Shim exec status for PCRE2's match limit (POSIX `REG_ESPACE`).
pub const PCRE2_SHIM_MATCH_LIMIT: i32 = 14;
/// Shim exec status for malformed UTF-8 (POSIX `REG_INVARG`).
pub const PCRE2_SHIM_BAD_UTF8: i32 = 16;
/// Shim exec status for an ordinary failed match (POSIX `REG_NOMATCH`).
pub const PCRE2_SHIM_NOMATCH: i32 = 17;
/// Shim exec status for PCRE2's depth limit (`ELEPHC_PCRE2_DEPTH_LIMIT`).
pub const PCRE2_SHIM_DEPTH_LIMIT: i32 = 0x103;
/// Shim exec status for a start offset inside a code point (`ELEPHC_PCRE2_BAD_UTF_OFFSET`).
pub const PCRE2_SHIM_BAD_UTF_OFFSET: i32 = 0x105;

/// Each shim failure status with a dedicated `PREG_*_ERROR` code; any other non-zero,
/// non-`REG_NOMATCH` status is `PREG_INTERNAL_ERROR`.
pub const PCRE2_SHIM_STATUS_ERRORS: [(i32, i64); 4] = [
    (PCRE2_SHIM_MATCH_LIMIT, PREG_BACKTRACK_LIMIT_ERROR),
    (PCRE2_SHIM_DEPTH_LIMIT, PREG_RECURSION_LIMIT_ERROR),
    (PCRE2_SHIM_BAD_UTF8, PREG_BAD_UTF8_ERROR),
    (PCRE2_SHIM_BAD_UTF_OFFSET, PREG_BAD_UTF8_OFFSET_ERROR),
];

/// Returns the `PREG_*_ERROR` code for a shim exec status (`0` for a match or no match).
pub const fn preg_error_for_exec_status(status: i32) -> i64 {
    if status == 0 || status == PCRE2_SHIM_NOMATCH {
        return PREG_NO_ERROR;
    }
    let mut index = 0;
    while index < PCRE2_SHIM_STATUS_ERRORS.len() {
        if PCRE2_SHIM_STATUS_ERRORS[index].0 == status {
            return PCRE2_SHIM_STATUS_ERRORS[index].1;
        }
        index += 1;
    }
    PREG_INTERNAL_ERROR
}

/// Returns PHP's message for a `PREG_*_ERROR` code; unknown codes read as "No error".
pub fn preg_error_message(code: i64) -> &'static str {
    usize::try_from(code)
        .ok()
        .and_then(|index| PREG_ERROR_MESSAGES.get(index))
        .copied()
        .unwrap_or(PREG_ERROR_MESSAGES[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shim status maps to the code php-src records for the PCRE2 error behind it.
    #[test]
    fn exec_statuses_map_to_php_codes() {
        assert_eq!(preg_error_for_exec_status(0), PREG_NO_ERROR);
        assert_eq!(preg_error_for_exec_status(PCRE2_SHIM_NOMATCH), PREG_NO_ERROR);
        assert_eq!(preg_error_for_exec_status(14), PREG_BACKTRACK_LIMIT_ERROR);
        assert_eq!(preg_error_for_exec_status(0x103), PREG_RECURSION_LIMIT_ERROR);
        assert_eq!(preg_error_for_exec_status(16), PREG_BAD_UTF8_ERROR);
        assert_eq!(preg_error_for_exec_status(0x105), PREG_BAD_UTF8_OFFSET_ERROR);
        assert_eq!(preg_error_for_exec_status(1), PREG_INTERNAL_ERROR);
        assert_eq!(preg_error_message(PREG_RECURSION_LIMIT_ERROR), "Recursion limit exhausted");
        assert_eq!(preg_error_message(99), "No error");
    }
}
