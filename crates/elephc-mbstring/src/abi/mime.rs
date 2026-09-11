//! Purpose:
//! Resolves MIME encoder options and request defaults before the shared byte algorithm.
//!
//! Called from:
//! - The versioned mbstring ABI after ordered parameter coercion.
//!
//! Key details:
//! - Explicit charsets select Base64 by default, independently of the request language.
//! - Charset validation and its cached deprecation precede even an empty-input result.

use super::*;

/// Validates the destination and delegates encoding without updating rejected-character counts.
pub(super) fn encode(args: &Arguments<'_>, state: &mut State) -> Outcome {
    let mail = state.language().mail_encodings();
    let (destination, deprecation, mut base64) = if args.supplied(1) {
        let resolved = match state.resolve_encoding(Some(args.string(1)), args.contract.name, 2, "charset") {
            Ok(resolved) => resolved, Err(error) => return Outcome::error(error),
        };
        (resolved.encoding, resolved.deprecation, true)
    } else { (mail[0], None, !mail[1].name().starts_with(['Q', 'q'])) };
    if args.supplied(2) && args.string(2).first().is_some_and(|byte| matches!(byte, b'Q' | b'q')) {
        base64 = false;
    }
    let mut result = if destination.supports_mime_header() {
        Outcome::string(Ok(crate::mime::encode_header(args.string(0), state.internal_encoding(),
            destination, base64, args.string(3), args.integer(4))))
    } else {
        Outcome::error(MbError::argument_value(args.contract.name, 2, "charset", "", args.string(1),
            " cannot be used for MIME header encoding"))
    };
    if let Some(message) = deprecation {
        result.diagnostics = format!("Deprecated: {}(): {message}\n", args.contract.name).into_bytes();
    }
    result
}
