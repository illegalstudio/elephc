//! Purpose:
//! Dispatches scalar mbstring settings against the shared request state.
//!
//! Called from:
//! - The versioned bridge after scalar argument validation.
//!
//! Key details:
//! - Omitted and explicit null arguments are getters; strings and integer codepoints are setters.
//! - Setters preserve engine validation and return PHP true only after success.

use super::*;
use crate::encoding::SubstituteMode;

/// Reads or updates one setting without touching the ordinary encoding-lookup cache.
pub(super) fn dispatch(operation: RuntimeBuiltinId, args: &Arguments<'_>, state: &mut State) -> Outcome {
    if operation == RuntimeBuiltinId::MbDetectOrder { return detect_order(args, state); }
    if operation == RuntimeBuiltinId::MbSubstituteCharacter { return substitute(args, state); }
    let Some(name) = args.nullable_string(0) else {
        let value = match operation {
            RuntimeBuiltinId::MbLanguage => state.language().name(),
            RuntimeBuiltinId::MbInternalEncoding => state.internal_encoding().name(),
            RuntimeBuiltinId::MbHttpOutput => state.http_output().name(),
            _ => return Outcome::unsupported(),
        };
        return Outcome::string(Ok(value.as_bytes().to_vec()));
    };
    let result = match operation {
        RuntimeBuiltinId::MbLanguage => state.set_language(name),
        RuntimeBuiltinId::MbInternalEncoding => state.set_internal_encoding(name),
        RuntimeBuiltinId::MbHttpOutput => state.set_http_output(name),
        _ => return Outcome::unsupported(),
    };
    match result { Ok(()) => Outcome::boolean(true), Err(error) => Outcome::error(error) }
}

/// Preserves integer codepoints versus named modes and changes settings only after validation.
fn substitute(args: &Arguments<'_>, state: &mut State) -> Outcome {
    let changed = match args.kind(0) {
        ARG_INT => state.set_substitute_codepoint(args.integer(0)),
        ARG_STRING => state.set_substitute_mode(args.string(0)),
        ARG_NULL => {
            let setting = state.substitute();
            let name = match setting.mode {
                SubstituteMode::Character => return Outcome::integer(setting.character as i64),
                SubstituteMode::None => "none",
                SubstituteMode::Long => "long",
                SubstituteMode::Entity => "entity",
            };
            return Outcome::string(Ok(name.as_bytes().to_vec()));
        }
        _ => unreachable!("validated substitution setting argument"),
    };
    match changed { Ok(()) => Outcome::boolean(true), Err(error) => Outcome::error(error) }
}

/// Applies a fully coerced encoding list, committing request state only after all entries validate.
fn detect_order(args: &Arguments<'_>, state: &mut State) -> Outcome {
    use crate::encoding::EncodingList;
    let names;
    let list = match args.kind(0) {
        ARG_NULL => return Outcome::strings(Ok(state.detect_order().iter()
            .map(|encoding| encoding.name().as_bytes().to_vec()).collect())),
        ARG_STRING => EncodingList::CommaSeparated(args.string(0)),
        ARG_ARRAY => {
            let Some(values) = args.encoding_names(0) else { return Outcome::fatal(); };
            names = values;
            EncodingList::Array(&names)
        },
        _ => return Outcome::fatal(),
    };
    match state.set_detect_order(list) { Ok(()) => Outcome::boolean(true), Err(error) => Outcome::error(error) }
}
