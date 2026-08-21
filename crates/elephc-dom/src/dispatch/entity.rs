//! Purpose:
//! Dispatches scalar metadata properties for legacy and modern entity nodes.
//! Keeps libxml2 DTD entity layout details behind the native adapter.
//!
//! Called from:
//! - `super::routes::dispatch()` for `DOMEntity` and `Dom\Entity`.
//!
//! Key details:
//! - Internal entities resolve every identifier to PHP null.
//! - External unparsed entities expose their stored public and system identifiers.
//! - Notation names are only populated for external unparsed entities.

use crate::context::Context;
use crate::request::Request;

use super::{
    receiver_pointer_and_graph, require_no_values, DispatchResult,
};

/// Returns one entity's public identifier or PHP null for non-external entities.
pub(super) fn public_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    optional_bytes_field(
        context,
        request,
        crate::native::entity_public_id,
    )
}

/// Returns one notation's public identifier, using PHP's empty string when absent.
pub(super) fn notation_public_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    optional_bytes_field(context, request, crate::native::notation_public_id)
}

/// Returns one notation's system identifier, using PHP's empty string when absent.
pub(super) fn notation_system_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    optional_bytes_field(context, request, crate::native::notation_system_id)
}

/// Reproduces php-src 8.5's modern-notation handler registration bug.
pub(super) fn modern_notation_uninitialized(
    request: &Request,
    property: &str,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    Ok(DispatchResult::error(
        format!(
            "Typed property Dom\\Notation::${property} must not be accessed before initialization"
        )
        .as_bytes(),
    ))
}

/// Returns one entity's system identifier or PHP null for non-external entities.
pub(super) fn system_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    optional_bytes_field(
        context,
        request,
        crate::native::entity_system_id,
    )
}

/// Returns one entity's resolved notation name or PHP null for non-external entities.
pub(super) fn notation_name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    optional_bytes_field(
        context,
        request,
        crate::native::entity_notation_name,
    )
}

/// Returns one entity's encoding or PHP null; the property is deprecated in PHP 8.5.
pub(super) fn encoding(
    _context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    Ok(DispatchResult::null().with_warning(
        b"Deprecated: Property DOMEntity::$encoding is deprecated\n",
    ))
}

/// Returns one entity's actual encoding or PHP null; the property is deprecated in PHP 8.5.
pub(super) fn actual_encoding(
    _context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    Ok(DispatchResult::null().with_warning(
        b"Deprecated: Property DOMEntity::$actualEncoding is deprecated\n",
    ))
}

/// Returns one entity's declared version or PHP null; the property is deprecated in PHP 8.5.
pub(super) fn version(
    _context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    Ok(DispatchResult::null().with_warning(
        b"Deprecated: Property DOMEntity::$version is deprecated\n",
    ))
}

/// Returns one optional byte-string entity field, surfacing null when absent.
fn optional_bytes_field(
    context: &Context,
    request: &Request,
    accessor: fn(usize) -> Option<Vec<u8>>,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(
        match accessor(pointer) {
            Some(value) => DispatchResult::bytes(value),
            None => DispatchResult::null(),
        },
    )
}
