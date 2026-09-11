//! Purpose:
//! Dispatches scalar text operations over validated, borrowed mbstring arguments.
//!
//! Called from:
//! - The panic-contained versioned bridge while it owns request state.
//!
//! Key details:
//! - PHP-specific prechecks precede encoding resolution where required.
//! - Encoding diagnostics survive subsequent operation errors.
//! - Optional results distinguish false, zero, and an empty owned string.

use super::*;
use crate::{text, unicode::CaseMode};
use text::{KanaMode, SearchMode, TrimSide};

/// Runs one text operation with PHP's parameter defaults and validation ordering.
pub(super) fn dispatch(operation: RuntimeBuiltinId, args: &Arguments<'_>, state: &mut State) -> Outcome {
    let contract = args.contract;
    let subject = if operation == RuntimeBuiltinId::MbChr { &[][..] } else { args.string(0) };
    match operation {
        RuntimeBuiltinId::MbOrd if subject.is_empty() =>
            return Outcome::error(MbError::empty(contract.name, 1, "string")),
        RuntimeBuiltinId::MbSubstrCount if args.string(1).is_empty() =>
            return Outcome::error(MbError::empty(contract.name, 2, "needle")),
        RuntimeBuiltinId::MbStrSplit => {
            if let Err(error) = text::validate_split_length(args.integer(1)) { return Outcome::error(error); }
        }
        RuntimeBuiltinId::MbSubstr => {
            if let Err(error) = text::validate_substr_bounds(args.integer(1), args.nullable_integer(2)) {
                return Outcome::error(error);
            }
        }
        _ => {},
    }
    let kana = if operation == RuntimeBuiltinId::MbConvertKana {
        match KanaMode::parse(args.string(1)) { Ok(mode) => Some(mode), Err(error) => return Outcome::error(error) }
    } else { None };
    let encoding_index = contract.params.len() - 1;
    let resolved = match state.resolve_encoding(args.nullable_string(encoding_index), contract.name, encoding_index + 1, "encoding") {
        Ok(resolved) => resolved,
        Err(error) => return Outcome::error(error),
    };
    let encoding = resolved.encoding;
    let substitution = state.substitute();
    let mut diagnostics = resolved.deprecation.map_or_else(Vec::new, |message|
        format!("Deprecated: {}(): {message}\n", contract.name).into_bytes());
    let mut result = match operation {
        RuntimeBuiltinId::MbStrlen => Outcome::integer(encoding.strlen(subject) as i64),
        RuntimeBuiltinId::MbStrwidth => Outcome::integer(text::strwidth(subject, encoding) as i64),
        RuntimeBuiltinId::MbStrtoupper | RuntimeBuiltinId::MbStrtolower | RuntimeBuiltinId::MbConvertCase => {
            let mode = match operation {
                RuntimeBuiltinId::MbStrtoupper => Some(CaseMode::Upper),
                RuntimeBuiltinId::MbStrtolower => Some(CaseMode::Lower),
                _ => CaseMode::from_php(args.integer(1)),
            };
            match mode {
                Some(mode) => Outcome::string(Ok(text::convert_case(subject, mode, encoding, substitution))),
                None => Outcome::error(MbError::argument("mb_convert_case", 2, "mode", "must be one of the MB_CASE_* constants")),
            }
        }
        RuntimeBuiltinId::MbUcfirst | RuntimeBuiltinId::MbLcfirst =>
            Outcome::string(text::first_case(subject, operation == RuntimeBuiltinId::MbUcfirst, encoding, substitution)),
        RuntimeBuiltinId::MbStrimwidth => {
            let (start, width) = (args.integer(1), args.integer(2));
            let total = encoding.strlen(subject) as i128;
            let offset = if start < 0 { total + start as i128 } else { start as i128 };
            if width < 0 && offset >= 0 && offset <= total {
                diagnostics.extend_from_slice(b"Deprecated: mb_strimwidth(): passing a negative integer to argument #3 ($width) is deprecated\n");
            }
            Outcome::string(text::strimwidth(subject, start, width, args.string(3), encoding, substitution))
        }
        RuntimeBuiltinId::MbStrSplit => Outcome::strings(text::str_split(subject, args.integer(1), encoding, substitution)),
        RuntimeBuiltinId::MbSubstr =>
            Outcome::string(text::substr(subject, args.integer(1), args.nullable_integer(2), encoding, substitution)),
        RuntimeBuiltinId::MbStrcut =>
            Outcome::string(text::strcut(subject, args.integer(1), args.nullable_integer(2), encoding, substitution)),
        RuntimeBuiltinId::MbScrub => Outcome::string(Ok(state.scrub(subject, encoding))),
        RuntimeBuiltinId::MbTrim | RuntimeBuiltinId::MbLtrim | RuntimeBuiltinId::MbRtrim => {
            let side = match operation { RuntimeBuiltinId::MbLtrim => TrimSide::Left,
                RuntimeBuiltinId::MbRtrim => TrimSide::Right, _ => TrimSide::Both };
            Outcome::string(text::trim(subject, args.nullable_string(1), side, encoding, substitution))
        }
        RuntimeBuiltinId::MbStrPad =>
            Outcome::string(text::str_pad(subject, args.integer(1), args.string(2), args.integer(3), encoding, substitution)),
        RuntimeBuiltinId::MbConvertKana =>
            Outcome::string(Ok(text::convert_kana(subject, kana.expect("validated kana mode"), encoding, substitution))),
        RuntimeBuiltinId::MbSubstrCount => match text::substr_count(subject, args.string(1), encoding) {
            Ok(count) => Outcome::integer(count as i64), Err(error) => Outcome::error(error),
        },
        RuntimeBuiltinId::MbOrd => optional_integer(text::ord(subject, encoding).map(|value| value.map(i64::from))),
        RuntimeBuiltinId::MbChr => optional_string(text::chr(args.integer(0), encoding)),
        RuntimeBuiltinId::MbStrpos | RuntimeBuiltinId::MbStripos | RuntimeBuiltinId::MbStrrpos | RuntimeBuiltinId::MbStrripos =>
            optional_integer(text::strpos(subject, args.string(1), args.integer(2), encoding, search_mode(operation))
                .map(|value| value.map(|position| position as i64))),
        RuntimeBuiltinId::MbStrstr | RuntimeBuiltinId::MbStristr | RuntimeBuiltinId::MbStrrchr | RuntimeBuiltinId::MbStrrichr =>
            optional_string(text::strstr(subject, args.string(1), args.boolean(2), encoding, search_mode(operation), substitution)),
        _ => Outcome::unsupported(),
    };
    result.diagnostics = diagnostics;
    result
}

/// Selects direction and case folding for both character-position and substring searches.
fn search_mode(operation: RuntimeBuiltinId) -> SearchMode {
    SearchMode {
        reverse: matches!(operation, RuntimeBuiltinId::MbStrrpos | RuntimeBuiltinId::MbStrripos
            | RuntimeBuiltinId::MbStrrchr | RuntimeBuiltinId::MbStrrichr),
        insensitive: matches!(operation, RuntimeBuiltinId::MbStripos | RuntimeBuiltinId::MbStrripos
            | RuntimeBuiltinId::MbStristr | RuntimeBuiltinId::MbStrrichr),
    }
}

/// Preserves PHP false when a successful search or ordinal has no integer result.
fn optional_integer(result: crate::error::MbResult<Option<i64>>) -> Outcome {
    match result { Ok(Some(value)) => Outcome::integer(value), Ok(None) => Outcome::boolean(false), Err(error) => Outcome::error(error) }
}

/// Preserves PHP false when a successful search or character cannot produce a string.
fn optional_string(result: crate::error::MbResult<Option<Vec<u8>>>) -> Outcome {
    match result { Ok(Some(value)) => Outcome::string(Ok(value)), Ok(None) => Outcome::boolean(false), Err(error) => Outcome::error(error) }
}
