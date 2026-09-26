//! Purpose:
//! Dispatches encoding guesses using shared request defaults and host-proven catalog identity.
//!
//! Called from:
//! - The direct wire ABI and protected native/eval invocation coordinator.
//!
//! Key details:
//! - Omitted strictness observes request state; explicit false overrides it.
//! - Identical candidate names do not imply the cached catalog's unweighted priority.

use super::*;
use crate::encoding::EncodingList;

/// Applies fully prepared candidate lists without running callbacks under the state borrow.
pub(super) fn dispatch(args: &Arguments<'_>, state: &State) -> Outcome {
    let names;
    let list = match args.kind(1) {
        ARG_NULL => None,
        ARG_STRING => Some(EncodingList::CommaSeparated(args.string(1))),
        ARG_ARRAY => {
            let Some(values) = args.encoding_names(1) else { return Outcome::fatal(); };
            names = values;
            Some(EncodingList::Array(&names))
        },
        _ => return Outcome::fatal(),
    };
    let strict = args.supplied(2).then(|| args.boolean(2));
    match state.detect_encoding(args.string(0), list, strict, !args.encoding_catalog(1)) {
        Ok(Some(encoding)) => Outcome::string(Ok(encoding.name().as_bytes().to_vec())),
        Ok(None) => Outcome::boolean(false),
        Err(error) => Outcome::error(error),
    }
}
