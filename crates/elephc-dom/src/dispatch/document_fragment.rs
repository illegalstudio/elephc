//! Purpose:
//! Implements XML balanced-chunk insertion for legacy and modern document fragments.
//! Maps libxml failures to PHP booleans, diagnostics, and context-local error state.
//!
//! Called from:
//! - `super::routes::dispatch()` for `DOMDocumentFragment::appendXML()` and its modern alias.
//!
//! Key details:
//! - Direct legacy fragments retain a private backing document but remain PHP-unbound.
//! - Parsing follows php-src's global-option sanitization and never invokes the host loader.

use crate::context::Context;
use crate::request::Request;

use super::{libxml, node, DispatchResult};

/// Parses and appends one XML balanced chunk with PHP's fragment error behavior.
pub(super) fn append_xml(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let input = request.byte_string(0)?;
    let (pointer, has_owner_document) = {
        let fragment = node(context, request.header.receiver)?;
        (fragment.pointer(), fragment.owner_document_exposed())
    };
    if !has_owner_document {
        return Ok(DispatchResult::dom_exception(
            7,
            b"No Modification Allowed Error",
        ));
    }

    let emit_warnings = !context.internal_errors;
    let outcome = crate::native::fragment_append_xml(pointer, input)?;
    libxml::record_errors(context, &outcome.errors);
    let result = DispatchResult::boolean(outcome.appended);
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(method, &outcome.errors, 0)
    } else {
        result
    })
}
