//! Purpose:
//! Shares SimpleXML loader and DOM-interoperability dispatch primitives.
//! Owns fresh view materialization and PHP subclass discriminator validation.
//!
//! Called from:
//! - `super::routes::dispatch()` for locked SimpleXML and DOM interop opcodes.
//! - `super::reentrant::dispatch()` for callback-safe XML parsing.
//!
//! Key details:
//! - Every public SimpleXML result receives a fresh bridge handle.
//! - User subclasses are validated against the installed PHP class graph.
//! - The document-wide DOM API claim remains on `DocumentGraph`, never on a view.

pub(super) mod interop;
pub(super) mod handlers;
pub(super) mod loaders;
pub(super) mod methods;
pub(super) mod clone;

use crate::context::Context;
use crate::objects::{
    SimpleXmlIteratorType, SimpleXmlObject, HANDLE_SIMPLEXML,
};

use super::DispatchResult;

/// High-bit marker shared with the compiler for one concrete user-wrapper class id.
const USER_WRAPPER_MARKER: u64 = 1 << 63;

/// Borrows one validated SimpleXML view from a generation-checked handle.
pub(super) fn object(
    context: &Context,
    handle: u64,
) -> Result<&SimpleXmlObject, ()> {
    context
        .native_objects
        .get(handle, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml()
        .ok_or(())
}

/// Mutably borrows one validated SimpleXML view from a generation-checked handle.
pub(super) fn object_mut(
    context: &mut Context,
    handle: u64,
) -> Result<&mut SimpleXmlObject, ()> {
    context
        .native_objects
        .get_mut(handle, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml_mut()
        .ok_or(())
}

/// Inserts one fresh externally owned SimpleXML view and returns its typed result.
pub(super) fn fresh_result(
    context: &mut Context,
    object: SimpleXmlObject,
) -> DispatchResult {
    let wrapper_kind = object.wrapper_kind();
    let handle = context.insert_simplexml_external(object);
    DispatchResult::typed_bridge_handle(handle, wrapper_kind)
}

/// Resolves php-src's non-destructive exported node for one direct or iterator view.
pub(super) fn exported_pointer(object: &SimpleXmlObject) -> Option<usize> {
    let iterator = object.iterator();
    if iterator.kind() == SimpleXmlIteratorType::None {
        return Some(object.pointer());
    }
    let mut pointer = match iterator.kind() {
        SimpleXmlIteratorType::Element | SimpleXmlIteratorType::Child => {
            crate::native::node_first_child(object.pointer())
        }
        SimpleXmlIteratorType::AttrList => {
            crate::native::element_attribute_at(object.pointer(), 0)
        }
        SimpleXmlIteratorType::None => unreachable!("direct views returned above"),
    };
    while let Some(candidate) = pointer {
        let expected_type = match iterator.kind() {
            SimpleXmlIteratorType::Element | SimpleXmlIteratorType::Child => 1,
            SimpleXmlIteratorType::AttrList => 2,
            SimpleXmlIteratorType::None => unreachable!("direct views returned above"),
        };
        let name_matches = iterator.name().is_none_or(|name| {
            crate::native::node_name(candidate).as_deref() == Some(name)
        });
        if crate::native::node_type(candidate) == expected_type
            && name_matches
            && namespace_matches(candidate, iterator.namespace_or_prefix(), iterator.is_prefix())
        {
            return Some(candidate);
        }
        pointer = crate::native::node_next_sibling(candidate);
    }
    None
}

/// Applies SimpleXML's prefix-or-URI namespace selector to one candidate node.
fn namespace_matches(
    pointer: usize,
    namespace_or_prefix: Option<&[u8]>,
    is_prefix: bool,
) -> bool {
    let prefix = crate::native::node_prefix(pointer);
    let namespace_uri = crate::native::node_namespace_uri(pointer);
    match namespace_or_prefix {
        None => namespace_uri.is_none() || prefix.is_none(),
        Some(expected) if is_prefix => prefix.as_deref() == Some(expected),
        Some(expected) => namespace_uri.as_deref() == Some(expected),
    }
}

/// Resolves an optional SimpleXMLElement class name to its runtime discriminator.
pub(super) fn resolve_class_kind(
    context: &Context,
    class_name: Option<&[u8]>,
    callable: &[u8],
) -> Result<u64, DispatchResult> {
    let Some(class_name) = class_name else {
        return Ok(0);
    };
    if class_name.eq_ignore_ascii_case(b"SimpleXMLElement") {
        return Ok(0);
    }
    let Some(base) = context.class_metadata.by_name(b"SimpleXMLElement") else {
        return Err(class_name_type_error(callable, class_name));
    };
    let Some(candidate) = context.class_metadata.by_name(class_name) else {
        return Err(class_name_type_error(callable, class_name));
    };
    if !context
        .class_metadata
        .is_subclass_of(candidate.id, base.id)
    {
        return Err(class_name_type_error(callable, class_name));
    }
    Ok(USER_WRAPPER_MARKER | candidate.id)
}

/// Builds php-src's exact invalid SimpleXMLElement subclass `TypeError`.
fn class_name_type_error(callable: &[u8], class_name: &[u8]) -> DispatchResult {
    DispatchResult::type_error(
        &[
            callable,
            b": Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, ",
            class_name,
            b" given",
        ]
        .concat(),
    )
}

/// Returns one optional nullable byte-string while preserving omission.
fn optional_nullable_bytes(
    request: &crate::request::Request,
    index: usize,
) -> Result<Option<&[u8]>, ()> {
    if index >= request.values.len() {
        Ok(None)
    } else {
        request.optional_byte_string(index)
    }
}

/// Returns one optional boolean while preserving omission and explicit null.
fn optional_boolean(
    request: &crate::request::Request,
    index: usize,
) -> Result<bool, ()> {
    if index >= request.values.len() {
        Ok(false)
    } else {
        Ok(request.optional_boolean(index)?.unwrap_or(false))
    }
}

/// Returns one optional string while preserving the declared empty-string default.
fn optional_bytes(
    request: &crate::request::Request,
    index: usize,
) -> Result<Vec<u8>, ()> {
    if index >= request.values.len() {
        Ok(Vec::new())
    } else {
        Ok(request.byte_string(index)?.to_vec())
    }
}
