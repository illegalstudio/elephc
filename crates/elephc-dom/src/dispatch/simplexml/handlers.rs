//! Purpose:
//! Dispatches the 12 locked SimpleXMLElement object-handler operations through the native
//! libxml2 adapter. Each handler produces a fresh non-canonical SimpleXML view so repeated
//! reads, reads of the same child, and `simplexml_import_dom` always return distinct wrappers.
//!
//! Called from:
//! - `super::routes::dispatch()` for `object-handler:simplexml::*` keys.
//!
//! Key details:
//! - Handler 4450 preserves the complete live selection and exact wrapper discriminator.
//! - `read_property`, `read_dimension`, `write_property`, `write_dimension`, `has_property`,
//!   `has_dimension`, `unset_property`, and `unset_dimension` all allocate a fresh wrapper
//!   per call to mirror PHP's non-canonical SimpleXML identity.
//! - Repeated children expose the same element name and re-evaluate to equal but non-identical
//!   wrappers, matching `simplexml.c` `sxe_object_cast_object` and read handlers.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{
    DocumentGraph, SimpleXmlIteratorState, SimpleXmlIteratorType, SimpleXmlObject,
    HANDLE_SIMPLEXML,
};
use crate::request::Request;

use super::DispatchResult;

/// Borrows the SimpleXML wrapper for the request receiver and copies its public state.
struct SimpleXmlSnapshot {
    pointer: usize,
    document: Rc<DocumentGraph>,
    wrapper_kind: u64,
    iter_type: SimpleXmlIteratorType,
    iter_name: Option<Vec<u8>>,
    iter_nsprefix: Option<Vec<u8>>,
    iter_isprefix: bool,
}

/// Copies one generation-checked receiver's pointer, graph, and complete view state.
fn snapshot(context: &Context, receiver: u64) -> Result<SimpleXmlSnapshot, ()> {
    let simplexml = context
        .native_objects
        .get(receiver, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml()
        .ok_or(())?;
    Ok(SimpleXmlSnapshot {
        pointer: simplexml.pointer(),
        document: simplexml.document(),
        wrapper_kind: simplexml.wrapper_kind(),
        iter_type: simplexml.iterator().kind(),
        iter_name: simplexml.iterator().name().map(<[u8]>::to_vec),
        iter_nsprefix: simplexml
            .iterator()
            .namespace_or_prefix()
            .map(<[u8]>::to_vec),
        iter_isprefix: simplexml.iterator().is_prefix(),
    })
}

/// Resolves the first node selected by the snapshot without altering iterator identity.
fn first_view_node(snapshot: &SimpleXmlSnapshot) -> Option<usize> {
    crate::native_simplexml_handlers::view_first(
        snapshot.pointer,
        iter_type_value(snapshot.iter_type),
        snapshot.iter_name.as_deref(),
        snapshot.iter_nsprefix.as_deref(),
        snapshot.iter_isprefix,
    )
}

/// Resolves the base node used for a nested property access.
fn property_base_node(snapshot: &SimpleXmlSnapshot) -> Option<usize> {
    if snapshot.iter_type == SimpleXmlIteratorType::Child {
        Some(snapshot.pointer)
    } else {
        first_view_node(snapshot)
    }
}

/// Resolves the element that owns a named dimension attribute, creating a missing
/// named element view with the receiver node's exact namespace binding.
///
/// php-src uses `xmlNewChild(mynode, mynode->ns, iter.name, NULL)` for this case.
/// The new node stays attached to the existing document graph, so live view identity,
/// document ownership, and detached-root bookkeeping remain unchanged.
fn dimension_attribute_base_node(snapshot: &SimpleXmlSnapshot) -> Result<Option<usize>, ()> {
    if let Some(node) = property_base_node(snapshot) {
        return Ok(Some(node));
    }
    if snapshot.iter_type != SimpleXmlIteratorType::Element {
        return Ok(None);
    }
    let name = snapshot.iter_name.as_deref().ok_or(())?;
    let namespace_uri = crate::native::node_namespace_uri(snapshot.pointer);
    crate::native::simplexml_add_child(
        snapshot.document.pointer(),
        snapshot.pointer,
        name,
        None,
        namespace_uri.as_deref(),
    )
    .pointer
    .map(Some)
    .ok_or(())
}

/// Decodes Zend's `check_empty` handler flag, defaulting to `isset` semantics.
fn check_empty(request: &Request, index: usize) -> Result<i32, ()> {
    match request.value(index) {
        Err(()) => Ok(0),
        Ok(value) if value.tag == crate::abi::VALUE_BOOL => {
            Ok(i32::from(request.boolean(index)?))
        }
        Ok(value) if value.tag == crate::abi::VALUE_INT => {
            Ok(i32::from(request.integer(index)? != 0))
        }
        Ok(_) => Err(()),
    }
}

/// Resolves the first child element selected by one property name and view namespace.
fn property_node(snapshot: &SimpleXmlSnapshot, name: &[u8]) -> Option<usize> {
    let base = property_base_node(snapshot)?;
    crate::native_simplexml_handlers::view_offset(
        base,
        iter_type_value(SimpleXmlIteratorType::Element),
        Some(name),
        snapshot.iter_nsprefix.as_deref(),
        snapshot.iter_isprefix,
        0,
    )
}

/// Allocates a fresh direct-node wrapper while preserving the source namespace filter.
fn fresh_view_handle(
    context: &mut Context,
    snapshot: &SimpleXmlSnapshot,
    pointer: usize,
    iterator: SimpleXmlIteratorState,
) -> u64 {
    context.insert_simplexml_external(SimpleXmlObject::new(
        pointer,
        Rc::clone(&snapshot.document),
        snapshot.wrapper_kind,
        iterator,
    ))
}

/// Allocates a fresh direct-node wrapper while preserving the source namespace filter.
fn direct_view_handle(
    context: &mut Context,
    snapshot: &SimpleXmlSnapshot,
    pointer: usize,
) -> u64 {
    fresh_view_handle(
        context,
        snapshot,
        pointer,
        SimpleXmlIteratorState::direct(
            snapshot.iter_nsprefix.clone(),
            snapshot.iter_isprefix,
        ),
    )
}

/// Registers every child detached by a content replacement before native unlinking.
fn retain_replaced_children(
    context: &mut Context,
    snapshot: &SimpleXmlSnapshot,
    target: usize,
) {
    let mut child = crate::native::node_first_child(target);
    while let Some(pointer) = child {
        child = crate::native::node_next_sibling(pointer);
        context.register_detached_root(pointer, Rc::clone(&snapshot.document));
    }
}

/// Replaces scalar content while preserving wrappers for every detached former child.
fn replace_node_text(
    context: &mut Context,
    snapshot: &SimpleXmlSnapshot,
    target: usize,
    value: &[u8],
) -> bool {
    retain_replaced_children(context, snapshot, target);
    crate::native_simplexml_handlers::set_node_text(target, value) == 0
}

/// Unlinks one node and retains its detached allocation for existing wrappers.
fn detach_node(
    context: &mut Context,
    snapshot: &SimpleXmlSnapshot,
    target: usize,
) {
    crate::native_simplexml_handlers::unlink_node(target);
    context.register_detached_root(target, Rc::clone(&snapshot.document));
}

/// Reads an optional Zend BP_VAR mode, defaulting to ordinary read mode.
fn access_mode(request: &Request, index: usize) -> Result<i64, ()> {
    match request.value(index) {
        Err(()) => Ok(0),
        Ok(value) if value.tag == crate::abi::VALUE_INT => request.integer(index),
        Ok(_) => Err(()),
    }
}

/// Builds php-src's dynamic warning detail for a numeric access beyond a selection.
fn element_number_warning_detail(
    snapshot: &SimpleXmlSnapshot,
    index: i64,
    count: i32,
) -> Vec<u8> {
    let name = snapshot
        .iter_name
        .clone()
        .or_else(|| crate::native::node_name(snapshot.pointer))
        .unwrap_or_default();
    [
        b"Cannot add element ".as_slice(),
        &name,
        b" number ",
        index.to_string().as_bytes(),
        b" when only ",
        count.to_string().as_bytes(),
        b" such elements exist",
    ]
    .concat()
}

/// Returns the leading ASCII numeric token accepted by PHP scalar casts.
fn numeric_prefix(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start += 1;
    }
    let mut end = start;
    if matches!(bytes.get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    let integer_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let mut has_digits = end > integer_start;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let fraction_start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        has_digits |= end > fraction_start;
    }
    if !has_digits {
        return &[];
    }
    let exponent = end;
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        end += 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let exponent_digits = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == exponent_digits {
            end = exponent;
        }
    }
    &bytes[start..end]
}

/// Applies PHP's numeric-string conversion to a SimpleXML text payload.
fn numeric_value(bytes: &[u8]) -> f64 {
    std::str::from_utf8(numeric_prefix(bytes))
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Materializes php-src's `_IS_NUMBER` result as an integer or floating-point value.
fn numeric_number(bytes: &[u8]) -> DispatchResult {
    let token = numeric_prefix(bytes);
    if token
        .iter()
        .any(|byte| matches!(byte, b'.' | b'e' | b'E'))
    {
        return DispatchResult::float(numeric_value(token));
    }
    match std::str::from_utf8(token)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
    {
        Some(value) => DispatchResult::integer(value),
        None if token.is_empty() => DispatchResult::integer(0),
        None => DispatchResult::float(numeric_value(token)),
    }
}

/// Converts the Rust view discriminator to the pinned native `SXE_ITER` code.
fn iter_type_value(iter_type: SimpleXmlIteratorType) -> i32 {
    match iter_type {
        SimpleXmlIteratorType::None => 0,
        SimpleXmlIteratorType::Element => 1,
        SimpleXmlIteratorType::Child => 2,
        SimpleXmlIteratorType::AttrList => 3,
    }
}

/// Extracts the selected node's PHP string value into bridge-owned bytes.
fn cast_string(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let node = first_view_node(&snap).unwrap_or(0);
    let outcome = crate::native_simplexml_handlers::cast_string(
        snap.document.pointer(),
        node,
        iter_type_value(SimpleXmlIteratorType::None),
    );
    if outcome.error_code != 0 {
        return Ok(DispatchResult::error(
            b"SimpleXML string cast allocation failed",
        ));
    }
    Ok(DispatchResult::bytes(outcome.bytes.unwrap_or_default()))
}

/// Applies PHP SimpleXML boolean truthiness to the complete immutable view.
fn cast_bool(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let truthy = crate::native_simplexml_handlers::cast_bool(
        snap.document.pointer(),
        snap.pointer,
        iter_type_value(snap.iter_type),
        snap.iter_name.as_deref(),
        snap.iter_nsprefix.as_deref(),
        snap.iter_isprefix,
    );
    Ok(DispatchResult::boolean(truthy))
}

/// Compares two SimpleXML wrappers sharing php-src's handler table by node/document identity.
fn compare(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let node1 = context
        .native_objects
        .get(request.header.receiver, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml()
        .ok_or(())?
        .pointer();
    let doc1 = context
        .native_objects
        .get(request.header.receiver, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml()
        .ok_or(())?
        .document()
        .pointer();
    let other = request.bridge_handle(0)?;
    let node2 = context
        .native_objects
        .get(other, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml()
        .ok_or(())?
        .pointer();
    let doc2 = context
        .native_objects
        .get(other, HANDLE_SIMPLEXML)
        .map_err(|_| ())?
        .simplexml()
        .ok_or(())?
        .document()
        .pointer();
    let cmp = crate::native_simplexml_handlers::compare(node1, node2, doc1, doc2);
    Ok(DispatchResult::integer(cmp as i64))
}

/// Counts every live member selected by the receiver's iterator state.
fn count(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let count = crate::native_simplexml_handlers::view_count(
        snap.pointer,
        iter_type_value(snap.iter_type),
        snap.iter_name.as_deref(),
        snap.iter_nsprefix.as_deref(),
        snap.iter_isprefix,
    );
    Ok(DispatchResult::integer(count as i64))
}

/// Produces a fresh named-child view rooted at the receiver's selected base node.
fn read_property(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let name = request.byte_string(0)?;
    let mode = access_mode(request, 1)?;
    let property_address = match request.value(2) {
        Err(()) => false,
        Ok(value) if value.tag == crate::abi::VALUE_BOOL => request.boolean(2)?,
        Ok(_) => return Err(()),
    };
    let Some(base) = property_base_node(&snap) else {
        return Ok(DispatchResult::null());
    };
    if property_address {
        let append_target = match request.value(3) {
            Err(()) => false,
            Ok(value) if value.tag == crate::abi::VALUE_BOOL => request.boolean(3)?,
            Ok(_) => return Err(()),
        };
        if !append_target {
            let selected = property_node(&snap, name).or_else(|| {
                crate::native::simplexml_add_child(
                    snap.document.pointer(),
                    base,
                    name,
                    None,
                    None,
                )
                .pointer
            });
            if selected.is_none() {
                return Ok(DispatchResult::error(
                    b"SimpleXML property autovivification failed",
                ));
            }
        }
        let handle = fresh_view_handle(
            context,
            &snap,
            base,
            SimpleXmlIteratorState::new(
                SimpleXmlIteratorType::Element,
                Some(name.to_vec()),
                snap.iter_nsprefix.clone(),
                snap.iter_isprefix,
            ),
        );
        return Ok(DispatchResult::typed_bridge_handle(
            handle,
            snap.wrapper_kind,
        ));
    }
    if mode == 3 && property_node(&snap, name).is_none() {
        return Ok(DispatchResult::null());
    }
    let handle = fresh_view_handle(
        context,
        &snap,
        base,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::Element,
            Some(name.to_vec()),
            snap.iter_nsprefix.clone(),
            snap.iter_isprefix,
        ),
    );
    Ok(DispatchResult::typed_bridge_handle(handle, snap.wrapper_kind))
}

/// Reads an element offset or named attribute into a fresh direct-node wrapper.
fn read_dimension(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let mode = access_mode(request, 1)?;
    if request.value(0)?.tag == crate::abi::VALUE_SIMPLEXML_APPEND {
        if snap.iter_type != SimpleXmlIteratorType::Element {
            return Ok(DispatchResult::null());
        }
        let name = snap.iter_name.as_deref().ok_or(())?;
        let selected = crate::native::simplexml_add_child(
            snap.document.pointer(),
            snap.pointer,
            name,
            None,
            None,
        )
        .pointer;
        let Some(selected) = selected else {
            return Ok(DispatchResult::null());
        };
        let handle = direct_view_handle(context, &snap, selected);
        return Ok(DispatchResult::typed_bridge_handle(handle, snap.wrapper_kind));
    }
    if request.value(0)?.tag == crate::abi::VALUE_NULL {
        return Ok(DispatchResult::null());
    }
    if let Ok(index) = request.integer(0) {
        let original_count = (snap.iter_type == SimpleXmlIteratorType::Element).then(|| {
            crate::native_simplexml_handlers::view_count(
                snap.pointer,
                iter_type_value(snap.iter_type),
                snap.iter_name.as_deref(),
                snap.iter_nsprefix.as_deref(),
                snap.iter_isprefix,
            )
        });
        let selected = if snap.iter_type == SimpleXmlIteratorType::None {
            Some(snap.pointer)
        } else {
            crate::native_simplexml_handlers::view_offset(
                snap.pointer,
                iter_type_value(snap.iter_type),
                snap.iter_name.as_deref(),
                snap.iter_nsprefix.as_deref(),
                snap.iter_isprefix,
                index,
            )
        };
        let selected = if selected.is_none()
            && matches!(mode, 1 | 2)
            && snap.iter_type == SimpleXmlIteratorType::Element
        {
            let name = snap.iter_name.as_deref().ok_or(())?;
            crate::native::simplexml_add_child(
                snap.document.pointer(),
                snap.pointer,
                name,
                None,
                None,
            )
            .pointer
        } else {
            selected
        };
        let warning = if snap.iter_type == SimpleXmlIteratorType::None && index > 0 {
            Some(element_number_warning_detail(&snap, index, 0))
        } else if selected.is_some()
            && matches!(mode, 1 | 2)
            && snap.iter_type == SimpleXmlIteratorType::Element
        {
            let count = original_count.unwrap_or_default();
            (index > i64::from(count))
                .then(|| element_number_warning_detail(&snap, index, count))
        } else {
            None
        };
        let Some(selected) = selected else {
            return Ok(DispatchResult::null());
        };
        let handle = direct_view_handle(context, &snap, selected);
        let mut result = DispatchResult::typed_bridge_handle(handle, snap.wrapper_kind);
        if let Some(warning) = warning {
            result = result.with_callsite_warning(&warning);
        }
        return Ok(result);
    }
    if let Ok(name) = request.byte_string(0) {
        let attribute = crate::native_simplexml_handlers::view_attribute(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
            name,
        );
        let Some(attribute) = attribute else {
            return Ok(DispatchResult::null());
        };
        let handle = direct_view_handle(context, &snap, attribute);
        return Ok(DispatchResult::typed_bridge_handle(handle, snap.wrapper_kind));
    }
    Err(())
}

/// Implements `isset`/`empty` for one named child property.
fn has_property(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let name = request.byte_string(0)?;
    let Some(node) = property_node(&snap, name) else {
        return Ok(DispatchResult::boolean(false));
    };
    let empty = check_empty(request, 1)? != 0
        && crate::native_simplexml_handlers::selected_is_empty(node);
    Ok(DispatchResult::boolean(!empty))
}

/// Implements `isset`/`empty` for one numeric element or named attribute dimension.
fn has_dimension(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    if let Ok(index) = request.integer(0) {
        let selected = crate::native_simplexml_handlers::view_offset(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
            index,
        );
        let exists = selected.is_some();
        let empty = check_empty(request, 1)? != 0
            && selected.is_some_and(crate::native_simplexml_handlers::selected_is_empty);
        return Ok(DispatchResult::boolean(exists && !empty));
    }
    if let Ok(name) = request.byte_string(0) {
        let selected = crate::native_simplexml_handlers::view_attribute(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
            name,
        );
        let exists = selected.is_some();
        let empty = check_empty(request, 1)? != 0
            && selected.is_some_and(crate::native_simplexml_handlers::selected_is_empty);
        return Ok(DispatchResult::boolean(exists && !empty));
    }
    Err(())
}

/// Unlinks every matching child property while preserving detached wrapper liveness.
fn unset_property(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let name = request.byte_string(0)?;
    while let Some(node) = property_node(&snap, name) {
        detach_node(context, &snap, node);
    }
    Ok(DispatchResult::null())
}

/// Unlinks one selected numeric element or named attribute dimension.
fn unset_dimension(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    if let Ok(index) = request.integer(0) {
        if let Some(selected) = crate::native_simplexml_handlers::view_offset(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
            index,
        ) {
            detach_node(context, &snap, selected);
        }
        return Ok(DispatchResult::null());
    }
    if let Ok(name) = request.byte_string(0) {
        if let Some(selected) = crate::native_simplexml_handlers::view_attribute(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
            name,
        ) {
            detach_node(context, &snap, selected);
        }
        return Ok(DispatchResult::null());
    }
    Err(())
}

/// Formats one floating-point scalar with PHP-compatible non-finite spellings.
fn scalar_float_bytes(value: f64) -> Vec<u8> {
    if value.is_nan() {
        b"NAN".to_vec()
    } else if value == f64::INFINITY {
        b"INF".to_vec()
    } else if value == f64::NEG_INFINITY {
        b"-INF".to_vec()
    } else {
        value.to_string().into_bytes()
    }
}

/// Builds the exact complex-assignment `TypeError` shared by both write handlers.
fn complex_assignment_error(target: &[u8], value_type: &[u8]) -> DispatchResult {
    DispatchResult::type_error(
        &[
            b"It's not possible to assign a complex type to ".as_slice(),
            target,
            b", ",
            value_type,
            b" given",
        ]
        .concat(),
    )
}

/// Coerces one permitted PHP scalar or SimpleXMLElement write value to bytes.
fn write_value_bytes(
    context: &Context,
    request: &Request,
    index: usize,
    target: &[u8],
) -> Result<Result<Vec<u8>, DispatchResult>, ()> {
    let value = request.value(index)?;
    Ok(match value.tag {
        crate::abi::VALUE_BYTES => Ok(request.byte_string(index)?.to_vec()),
        crate::abi::VALUE_INT => Ok(request.integer(index)?.to_string().into_bytes()),
        crate::abi::VALUE_BOOL => Ok(if request.boolean(index)? {
            b"1".to_vec()
        } else {
            Vec::new()
        }),
        crate::abi::VALUE_FLOAT => Ok(scalar_float_bytes(f64::from_bits(value.payload0))),
        crate::abi::VALUE_NULL => Ok(Vec::new()),
        crate::abi::VALUE_BRIDGE_HANDLE => {
            let handle = request.bridge_handle(index)?;
            match snapshot(context, handle) {
                Ok(snapshot) => {
                    let node = first_view_node(&snapshot).unwrap_or(0);
                    Ok(crate::native_simplexml_handlers::cast_string(
                        snapshot.document.pointer(),
                        node,
                        0,
                    )
                    .bytes
                    .unwrap_or_default())
                }
                Err(()) => Err(complex_assignment_error(target, b"object")),
            }
        }
        crate::abi::VALUE_ARRAY | crate::abi::VALUE_MAP => {
            Err(complex_assignment_error(target, b"array"))
        }
        crate::abi::VALUE_OBJECT => {
            Err(complex_assignment_error(target, b"stdClass"))
        }
        crate::abi::VALUE_RESOURCE => {
            Err(complex_assignment_error(target, b"resource"))
        }
        _ => return Err(()),
    })
}

/// Replaces or creates one named child property's scalar text value.
fn write_property(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let name = request.byte_string(0)?;
    let value = match write_value_bytes(context, request, 1, b"properties")? {
        Ok(value) => value,
        Err(error) => return Ok(error),
    };
    if name.is_empty() {
        return Ok(DispatchResult::value_error(
            b"Cannot create element with an empty name",
        ));
    }
    let Some(node) = property_base_node(&snap) else {
        return Ok(DispatchResult::null());
    };
    let count = crate::native_simplexml_handlers::view_count(
        node,
        iter_type_value(SimpleXmlIteratorType::Element),
        Some(name),
        snap.iter_nsprefix.as_deref(),
        snap.iter_isprefix,
    );
    match count {
        0 => {
            let outcome = crate::native::simplexml_add_child(
                snap.document.pointer(),
                node,
                name,
                Some(&value),
                None,
            );
            if outcome.pointer.is_none() {
                return Ok(DispatchResult::error(
                    b"SimpleXML property allocation failed",
                ));
            }
        }
        1 => {
            let selected = property_node(&snap, name).ok_or(())?;
            if !replace_node_text(context, &snap, selected, &value) {
                return Ok(DispatchResult::error(
                    b"SimpleXML property content replacement failed",
                ));
            }
        }
        _ => {
            return Ok(DispatchResult::null().with_warning(
                b"Warning: Unknown: Cannot assign to an array of nodes (duplicate subnodes or attr detected)\n",
            ));
        }
    }
    Ok(DispatchResult::null())
}

/// Replaces one selected element/attribute value or appends one filtered sibling.
fn write_dimension(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let snap = snapshot(context, request.header.receiver)?;
    let append = request.value(0)?.tag == crate::abi::VALUE_SIMPLEXML_APPEND;
    if request.value(0)?.tag == crate::abi::VALUE_NULL {
        return Ok(DispatchResult::value_error(
            b"Cannot create attribute with an empty name",
        ));
    }
    let numeric_index = request.integer(0).ok();
    let target = if (numeric_index.is_some() || append)
        && snap.iter_type != SimpleXmlIteratorType::AttrList
    {
        b"properties".as_slice()
    } else {
        b"attributes".as_slice()
    };
    let value = match write_value_bytes(context, request, 1, target)? {
        Ok(value) => value,
        Err(error) => return Ok(error),
    };
    if append {
        if snap.iter_type == SimpleXmlIteratorType::AttrList {
            return Ok(DispatchResult::error(
                b"Cannot append to an attribute list",
            ));
        }
        let Some(selected) = first_view_node(&snap) else {
            return Ok(DispatchResult::null());
        };
        if crate::native::node_type(selected) == 2 {
            return Ok(DispatchResult::error(
                b"Cannot create duplicate attribute",
            ));
        }
        let parent = crate::native::node_parent(selected);
        if snap.iter_type == SimpleXmlIteratorType::None
            && parent.is_some_and(|parent| crate::native::node_type(parent) == 9)
        {
            return Ok(DispatchResult::value_error(
                b"Cannot append to an attribute list",
            ));
        }
        let (parent, name) = if snap.iter_type == SimpleXmlIteratorType::Element {
            (Some(snap.pointer), snap.iter_name.clone())
        } else {
            (parent, crate::native::node_name(selected))
        };
        let (Some(parent), Some(name)) = (parent, name) else {
            return Ok(DispatchResult::null());
        };
        if crate::native::simplexml_add_child(
            snap.document.pointer(),
            parent,
            &name,
            Some(&value),
            None,
        )
        .pointer
        .is_none()
        {
            return Ok(DispatchResult::error(
                b"SimpleXML dimension allocation failed",
            ));
        }
        return Ok(DispatchResult::null());
    }
    if let Some(index) = numeric_index {
        let count = crate::native_simplexml_handlers::view_count(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
        );
        let selected = if snap.iter_type == SimpleXmlIteratorType::None {
            Some(snap.pointer)
        } else {
            crate::native_simplexml_handlers::view_offset(
                snap.pointer,
                iter_type_value(snap.iter_type),
                snap.iter_name.as_deref(),
                snap.iter_nsprefix.as_deref(),
                snap.iter_isprefix,
                index,
            )
        };
        if let Some(selected) = selected {
            if !replace_node_text(context, &snap, selected, &value) {
                return Ok(DispatchResult::error(
                    b"SimpleXML dimension content replacement failed",
                ));
            }
        } else if snap.iter_type == SimpleXmlIteratorType::Element {
            let name = snap.iter_name.as_deref().ok_or(())?;
            if crate::native::simplexml_add_child(
                snap.document.pointer(),
                snap.pointer,
                name,
                Some(&value),
                None,
            )
            .pointer
            .is_none()
            {
                return Ok(DispatchResult::error(
                    b"SimpleXML dimension allocation failed",
                ));
            }
        }
        let warning = if snap.iter_type == SimpleXmlIteratorType::None && index > 0 {
            Some(element_number_warning_detail(&snap, index, 0))
        } else if snap.iter_type == SimpleXmlIteratorType::Element && index > i64::from(count) {
            Some(element_number_warning_detail(&snap, index, count))
        } else {
            None
        };
        let mut result = DispatchResult::null();
        if let Some(warning) = warning {
            result = result.with_callsite_warning(&warning);
        }
        return Ok(result);
    }
    if let Ok(name) = request.byte_string(0) {
        let name = trim_php_string(name);
        if name.is_empty() {
            return Ok(DispatchResult::value_error(
                b"Cannot create attribute with an empty name",
            ));
        }
        if let Some(attribute) = crate::native_simplexml_handlers::view_attribute(
            snap.pointer,
            iter_type_value(snap.iter_type),
            snap.iter_name.as_deref(),
            snap.iter_nsprefix.as_deref(),
            snap.iter_isprefix,
            name,
        ) {
            if !replace_node_text(context, &snap, attribute, &value) {
                return Ok(DispatchResult::error(
                    b"SimpleXML attribute content replacement failed",
                ));
            }
        } else if snap.iter_type != SimpleXmlIteratorType::AttrList {
            let node = match dimension_attribute_base_node(&snap) {
                Ok(Some(node)) => node,
                Ok(None) => return Ok(DispatchResult::null()),
                Err(()) => {
                    return Ok(DispatchResult::error(
                        b"SimpleXML element autovivification failed",
                    ));
                }
            };
            let status = crate::native_simplexml_handlers::write_dimension_attribute(
                snap.document.pointer(),
                node,
                name,
                &value,
                iter_type_value(SimpleXmlIteratorType::None),
            );
            if status != 0 {
                return Ok(DispatchResult::error(
                    b"SimpleXML attribute allocation failed",
                ));
            }
        }
        return Ok(DispatchResult::null());
    }
    Err(())
}

/// Trims the byte set used by PHP's default `trim()` during dimension writes.
fn trim_php_string(bytes: &[u8]) -> &[u8] {
    let is_trimmed = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0 | 0x0b);
    let start = bytes
        .iter()
        .position(|byte| !is_trimmed(*byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !is_trimmed(*byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

/// Routes the compact cast discriminator to the matching PHP materializer.
pub(in crate::dispatch) fn cast(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    // The bridge reference contract accepts 0 bool, 1 int, 2 float, 3 string,
    // 4 for php-src's dynamic `_IS_NUMBER` arithmetic conversion, and 5 for
    // the array-cast property projection.
    // The earlier boolean flag remains accepted so bridge-only probes stay useful.
    if let Ok(kind) = request.integer(0) {
        return match kind {
            0 => cast_bool(context, request),
            1 => {
                let bytes = cast_string(context, request)?.frame.bytes;
                Ok(DispatchResult::integer(numeric_value(&bytes) as i64))
            }
            2 => {
                let bytes = cast_string(context, request)?.frame.bytes;
                Ok(DispatchResult::float(numeric_value(&bytes)))
            }
            3 => cast_string(context, request),
            4 => {
                let bytes = cast_string(context, request)?.frame.bytes;
                Ok(numeric_number(&bytes))
            }
            5 => super::methods::debug_info(context, &request.receiver_only()),
            _ => Err(()),
        };
    }
    if let Ok(true) = request.boolean(0) {
        return cast_bool(context, request);
    }
    cast_string(context, request)
}

/// Dispatches the generated compare opcode.
pub(in crate::dispatch) fn compare_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    compare(context, request)
}

/// Dispatches the generated count opcode.
pub(in crate::dispatch) fn count_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    count(context, request)
}

/// Exposes the receiver as the native iterator source without allocating a second wrapper.
///
/// php-src allocates an internal `zend_object_iterator` which strongly retains the original
/// `SimpleXMLElement`; Elephc's iterator opcodes already keep that object alive and drive its
/// public iterator methods. Returning the same generation-checked handle preserves that identity
/// and reuses the eager strong `iter.data` owner installed by `rewind()` and `next()`.
pub(in crate::dispatch) fn get_iterator_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snap = snapshot(context, request.header.receiver)?;
    Ok(DispatchResult::typed_bridge_handle(
        request.header.receiver,
        snap.wrapper_kind,
    ))
}

/// Dispatches the generated property-existence opcode.
pub(in crate::dispatch) fn has_property_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    has_property(context, request)
}

/// Dispatches the generated property-read opcode.
pub(in crate::dispatch) fn read_property_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    read_property(context, request)
}

/// Dispatches the generated property-write opcode.
pub(in crate::dispatch) fn write_property_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    write_property(context, request)
}

/// Dispatches the generated property-unset opcode.
pub(in crate::dispatch) fn unset_property_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    unset_property(context, request)
}

/// Dispatches the generated dimension-existence opcode.
pub(in crate::dispatch) fn has_dimension_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    has_dimension(context, request)
}

/// Dispatches the generated dimension-read opcode.
pub(in crate::dispatch) fn read_dimension_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    read_dimension(context, request)
}

/// Dispatches the generated dimension-write opcode.
pub(in crate::dispatch) fn write_dimension_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    write_dimension(context, request)
}

/// Dispatches the generated dimension-unset opcode.
pub(in crate::dispatch) fn unset_dimension_dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    unset_dimension(context, request)
}

#[cfg(test)]
#[path = "handlers_tests.rs"]
mod tests;
