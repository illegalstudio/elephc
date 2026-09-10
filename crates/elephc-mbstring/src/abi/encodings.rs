//! Purpose:
//! Exposes encoding aliases and preferred MIME names from the authoritative catalog.
//!
//! Called from:
//! - Versioned mbstring dispatch after borrowed argument validation.
//!
//! Key details:
//! - Alias lookup shares PHP's explicit-encoding deprecation cache.
//! - MIME-name lookup bypasses that cache and warns when no MIME name exists.

use super::*;
use crate::encoding::Encoding;

/// Resolves encoding metadata with the correct cache, diagnostics, and binary name handling.
pub(super) fn dispatch(operation: RuntimeBuiltinId, args: &Arguments<'_>, state: &mut State) -> Outcome {
    let name = args.string(0);
    if operation == RuntimeBuiltinId::MbEncodingAliases {
        let resolved = match state.resolve_encoding(Some(name), args.contract.name, 1, "encoding") {
            Ok(resolved) => resolved, Err(error) => return Outcome::error(error),
        };
        let mut result = Outcome::strings(Ok(resolved.encoding.aliases().iter().map(|alias| alias.as_bytes().to_vec()).collect()));
        if let Some(message) = resolved.deprecation {
            result.diagnostics = format!("Deprecated: {}(): {message}\n", args.contract.name).into_bytes();
        }
        return result;
    }
    let Some(encoding) = Encoding::lookup_c_string(name) else {
        return Outcome::error(MbError::argument_value(args.contract.name, 1, "encoding", "must be a valid encoding, ", name, " given"));
    };
    if let Some(mime) = encoding.mime_name().filter(|mime| !mime.is_empty()) {
        return Outcome::string(Ok(mime.as_bytes().to_vec()));
    }
    let mut result = Outcome::boolean(false);
    result.diagnostics.extend_from_slice(b"Warning: mb_preferred_mime_name(): No MIME preferred name corresponding to \"");
    result.diagnostics.extend_from_slice(name.split(|&byte| byte == 0).next().unwrap_or_default());
    result.diagnostics.extend_from_slice(b"\"\n");
    result
}
