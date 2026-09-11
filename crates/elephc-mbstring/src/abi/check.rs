//! Purpose:
//! Dispatches string, recursive array, and request-wide mbstring encoding checks.
//!
//! Called from:
//! - The versioned bridge after validating and decoding borrowed arguments.
//!
//! Key details:
//! - Encoding resolution precedes deprecated null-input diagnostics.
//! - Array traversal preserves warning order without changing request error counts.

use super::*;

/// Checks the selected value and preserves encoding, recursion, and null-input diagnostics.
pub(super) fn dispatch(args: &Arguments<'_>, state: &mut State) -> Outcome {
    let resolved = match state.resolve_encoding(args.nullable_string(1), args.contract.name, 2, "encoding") {
        Ok(resolved) => resolved, Err(error) => return Outcome::error(error),
    };
    let mut diagnostics = resolved.deprecation.map_or_else(Vec::new, |message|
        format!("Deprecated: {}(): {message}\n", args.contract.name).into_bytes());
    let valid = match args.kind(0) {
        ARG_ARRAY => {
            let checked = crate::arrays::check_encoding(args.array(0).expect("validated array"), resolved.encoding);
            for warning in checked.warnings {
                diagnostics.extend_from_slice(format!("Warning: {warning}\n").as_bytes());
            }
            checked.valid
        }
        ARG_STRING => resolved.encoding.decode(args.string(0)).is_valid(),
        ARG_NULL => {
            diagnostics.extend_from_slice(b"Deprecated: mb_check_encoding(): Calling mb_check_encoding() without argument is deprecated\n");
            state.illegal_chars() == 0
        }
        _ => unreachable!("validated array/string/null contract"),
    };
    let mut result = Outcome::boolean(valid);
    result.diagnostics = diagnostics;
    result
}
