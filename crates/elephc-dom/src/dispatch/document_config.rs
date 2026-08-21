//! Purpose:
//! Dispatches mutable legacy `DOMDocument` behavior flags retained across loads.
//! Keeps php-src parser configuration separate from document tree operations.
//!
//! Called from:
//! - `super::routes::dispatch()` for legacy document property access.
//!
//! Key details:
//! - Defaults and parser-option mapping are owned by `DocumentGraph`.
//! - Modern documents do not expose these legacy-only configuration properties.

use crate::context::Context;
use crate::objects::{DocumentFamily, LegacyDocumentFlag};
use crate::request::Request;

use super::{document, require_no_values, DispatchResult};

/// Returns PHP's deprecated always-null legacy DOM configuration placeholder.
pub(super) fn deprecated_config(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    Ok(DispatchResult::null().with_warning(
        b"Deprecated: Property DOMDocument::$config is deprecated\n",
    ))
}

/// Returns one legacy document behavior flag.
pub(super) fn get(
    context: &Context,
    request: &Request,
    flag: LegacyDocumentFlag,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    Ok(DispatchResult::boolean(target.legacy_flag(flag)))
}

/// Updates one legacy document behavior flag.
pub(super) fn set(
    context: &Context,
    request: &Request,
    flag: LegacyDocumentFlag,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.boolean(0)?;
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    target.set_legacy_flag(flag, value);
    Ok(DispatchResult::null())
}

/// Replaces legacy or modern XML version metadata with family-specific validation.
pub(super) fn set_version(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let version = request.optional_byte_string(0)?.unwrap_or_default();
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy
        && version != b"1.0"
        && version != b"1.1"
    {
        return Ok(DispatchResult::value_error(b"Invalid XML version"));
    }
    if !crate::native::document_set_version(target.pointer(), version) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Replaces a legacy document encoding after libxml validates its handler.
pub(super) fn set_encoding(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let Some(encoding) = request.optional_byte_string(0)? else {
        return Ok(DispatchResult::value_error(
            b"Invalid document encoding",
        ));
    };
    match crate::native::document_set_encoding(target.pointer(), encoding) {
        1 => Ok(DispatchResult::null()),
        -1 => Ok(DispatchResult::value_error(
            b"Invalid document encoding",
        )),
        _ => Err(()),
    }
}

/// Replaces legacy or modern XML standalone declaration state.
pub(super) fn set_standalone(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.boolean(0)?;
    let target = document(context, request.header.receiver)?;
    if !crate::native::document_set_standalone(
        target.pointer(),
        i32::from(value),
    ) {
        return Err(());
    }
    Ok(DispatchResult::null())
}
