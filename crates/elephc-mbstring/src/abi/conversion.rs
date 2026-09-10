//! Purpose:
//! Dispatches string and recursive-array conversion through prepared shared codecs.
//!
//! Called from:
//! - Direct wire dispatch and protected host invocation after ordered encoding preparation.
//!
//! Key details:
//! - Destination validation precedes source-list validation and input traversal.
//! - Converted graphs own all output bytes and retain exact PHP key identities.
//! - Request accounting includes rejected bytes even when entries are later discarded.

use super::*;
use crate::{encoding::{Encoding, EncodingList}, text::ConversionSources};

/// Runs conversion after both encoding arguments are resolved without further PHP callbacks.
pub(super) fn convert(args: &Arguments<'_>, state: &mut State, to: Encoding, sources: &ConversionSources) -> Outcome {
    match args.kind(0) {
        ARG_STRING => match state.convert_string(args.string(0), to, sources) {
            Some(bytes) => Outcome::string(Ok(bytes)),
            None => Outcome { diagnostics: b"Warning: mb_convert_encoding(): Unable to detect character encoding\n".to_vec(),
                ..Outcome::boolean(false) },
        },
        ARG_ARRAY => {
            let converted = state.convert_array(args.array(0).expect("validated conversion array"), to, sources);
            let mut output = match converted.result {
                Ok(graph) => Outcome { bytes: graph.encode(), ..Outcome::empty(RESULT_ARRAY) },
                Err(_) => Outcome::error(MbError::Runtime("mb_convert_encoding(): Cannot convert a nonterminating recursive array".into())),
            };
            for warning in converted.warnings { output.diagnostics.extend_from_slice(format!("Warning: {warning}\n").as_bytes()); }
            output
        }
        _ => Outcome::fatal(),
    }
}

/// Parses source names using current request defaults after the destination has been selected.
pub(super) fn sources(args: &Arguments<'_>, state: &State) -> Result<ConversionSources, MbError> {
    let names;
    let list = match args.kind(2) {
        ARG_NULL => None,
        ARG_STRING => Some(EncodingList::CommaSeparated(args.string(2))),
        ARG_ARRAY => {
            names = args.encoding_names(2).ok_or_else(|| MbError::Runtime("Invalid prepared encoding list".into()))?;
            Some(EncodingList::Array(&names))
        }
        _ => unreachable!("validated source encoding argument"),
    };
    state.conversion_sources(list)
}

/// Preserves destination deprecations before source errors on the callback-free wire surface.
pub(super) fn dispatch(args: &Arguments<'_>, state: &mut State) -> Outcome {
    let resolved = match state.resolve_encoding(Some(args.string(1)), args.contract.name, 2, "to_encoding") {
        Ok(resolved) => resolved, Err(error) => return Outcome::error(error),
    };
    let mut diagnostics = resolved.deprecation.map_or_else(Vec::new, |message|
        format!("Deprecated: {}(): {message}\n", args.contract.name).into_bytes());
    let mut result = match sources(args, state) {
        Ok(sources) => convert(args, state, resolved.encoding, &sources),
        Err(error) => Outcome::error(error),
    };
    diagnostics.append(&mut result.diagnostics);
    result.diagnostics = diagnostics;
    result
}
