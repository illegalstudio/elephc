//! Purpose:
//! Returns request information through the shared mbstring result ABI.
//!
//! Called from:
//! - The mb_get_info operation after complete outer argument coercion.
//!
//! Key details:
//! - Unknown selectors warn and return false; unset HTTP identification returns null.

use super::*;
use crate::state::Information;

/// Transfers the selected scalar, indexed list, or associative snapshot into the result wire.
pub(super) fn dispatch(args: &Arguments<'_>, state: &State) -> Outcome {
    match state.info(args.string(0)) {
        Some(Information::Null) => Outcome::empty(RESULT_NULL),
        Some(Information::Integer(value)) => Outcome::integer(value),
        Some(Information::String(bytes)) => Outcome::string(Ok(bytes)),
        Some(Information::Strings(values)) => Outcome::strings(Ok(values)),
        Some(Information::All(graph)) => Outcome { bytes: graph.encode(), ..Outcome::empty(RESULT_ARRAY) },
        None => Outcome { diagnostics: b"Warning: mb_get_info(): argument #1 ($type) must be a valid type\n".to_vec(),
            ..Outcome::boolean(false) },
    }
}
