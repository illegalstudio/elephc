//! Purpose:
//! Transfers shared HTTP input information into the mbstring result ABI.
//!
//! Called from:
//! - The mb_http_input operation after outer nullable-string coercion.
//!
//! Key details:
//! - Unknown selectors are ValueErrors; missing identification is PHP false.

use super::*;
use crate::state::InputInformation;

/// Returns a configured name list or recorded identification without running a new detector.
pub(super) fn dispatch(args: &Arguments<'_>, state: &State) -> Outcome {
    match state.http_input(args.nullable_string(0)) {
        Ok(InputInformation::Unidentified) => Outcome::boolean(false),
        Ok(InputInformation::String(bytes)) => Outcome::string(Ok(bytes)),
        Ok(InputInformation::List(values)) => Outcome::strings(Ok(values)),
        Err(error) => Outcome::error(error),
    }
}
