//! Purpose:
//! Implements the 21 locked PHP 8.5 `SimpleXMLElement` methods over shared libxml2 graphs.
//! Owns live view selection, iterator identity, mutation, namespaces, serialization, and XPath.
//!
//! Called from:
//! - `super::super::routes::dispatch()` for `method:simplexmlelement::*` opcodes.
//!
//! Key details:
//! - Ordinary method results mint fresh SimpleXML handles; iterator data deliberately reuses one handle.
//! - Every view shares its authoritative `Rc<DocumentGraph>` without pointer canonicalization.
//! - Namespace registrations are wrapper-local and never leak to another view of the same node.

use std::rc::Rc;

use crate::abi::{
    Value, VALUE_ARRAY, VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_MAP, VALUE_NULL,
};
use crate::context::{Context, ResultFrame};
use crate::objects::{
    DomApiFamily, DocumentGraph, SimpleXmlIteratorState,
    SimpleXmlIteratorType, SimpleXmlObject,
};
use crate::request::Request;

use super::super::{libxml::record_errors, DispatchResult};

/// Borrow-independent state copied from one validated SimpleXML wrapper.
struct Snapshot {
    handle: u64,
    pointer: Option<usize>,
    document: Rc<DocumentGraph>,
    wrapper_kind: u64,
    iterator_kind: SimpleXmlIteratorType,
    name: Option<Vec<u8>>,
    namespace_or_prefix: Option<Vec<u8>>,
    is_prefix: bool,
    xpath_namespaces: Vec<(Vec<u8>, Vec<u8>)>,
    iterator_current: Option<u64>,
}

/// One PHP array key emitted by `SimpleXMLElement::__debugInfo()`.
enum DebugKey {
    Integer(i64),
    Bytes(Vec<u8>),
}

/// One recursive value in php-src's SimpleXML debug-property projection.
enum DebugValue {
    Bytes(Vec<u8>),
    Wrapper(usize),
    Array(Vec<DebugValue>),
    Map(Vec<(DebugKey, DebugValue)>),
}

/// Copies one receiver's complete method-visible state without retaining a table borrow.
fn snapshot(context: &Context, receiver: u64) -> Result<Snapshot, ()> {
    let object = super::object(context, receiver)?;
    Ok(Snapshot {
        handle: receiver,
        pointer: object.node_pointer(),
        document: object.document(),
        wrapper_kind: object.wrapper_kind(),
        iterator_kind: object.iterator().kind(),
        name: object.iterator().name().map(<[u8]>::to_vec),
        namespace_or_prefix: object
            .iterator()
            .namespace_or_prefix()
            .map(<[u8]>::to_vec),
        is_prefix: object.iterator().is_prefix(),
        xpath_namespaces: object.xpath_namespaces().to_vec(),
        iterator_current: object.iterator().current(),
    })
}

/// Reports whether one node matches the wrapper's namespace-or-prefix filter.
fn matches_namespace(snapshot: &Snapshot, pointer: usize) -> bool {
    let prefix = crate::native::node_prefix(pointer);
    let namespace_uri = crate::native::node_namespace_uri(pointer);
    match snapshot.namespace_or_prefix.as_deref() {
        None => namespace_uri.is_none() || prefix.is_none(),
        Some(expected) if snapshot.is_prefix => {
            prefix.as_deref() == Some(expected)
        }
        Some(expected) => namespace_uri.as_deref() == Some(expected),
    }
}

/// Reports whether one node matches the wrapper's selected local name.
fn matches_name(snapshot: &Snapshot, pointer: usize) -> bool {
    !snapshot.name.as_deref().is_some_and(|name| {
        crate::native::node_local_name(pointer).as_deref() != Some(name)
    })
}

/// Reports whether one iterator member matches its node type, name, and namespace filters.
fn matches_view(snapshot: &Snapshot, pointer: usize, expected_type: u32) -> bool {
    crate::native::node_type(pointer) == expected_type
        && matches_name(snapshot, pointer)
        && matches_namespace(snapshot, pointer)
}

/// Finds the first member exposed by one child, named-element, or attribute-list view.
fn first_member(snapshot: &Snapshot) -> Option<usize> {
    let pointer = snapshot.pointer?;
    match snapshot.iterator_kind {
        SimpleXmlIteratorType::AttrList => {
            let count = crate::native::element_attribute_count(pointer);
            (0..count)
                .filter_map(|index| {
                    crate::native::element_attribute_at(pointer, index)
                })
                .find(|pointer| matches_view(snapshot, *pointer, 2))
        }
        SimpleXmlIteratorType::None
        | SimpleXmlIteratorType::Element
        | SimpleXmlIteratorType::Child => {
            let mut child = crate::native::node_first_child(pointer);
            while let Some(candidate) = child {
                if matches_view(snapshot, candidate, 1) {
                    return Some(candidate);
                }
                child = crate::native::node_next_sibling(candidate);
            }
            None
        }
    }
}

/// Finds the next matching member after one materialized iterator-data node.
fn next_member(snapshot: &Snapshot, current: usize) -> Option<usize> {
    if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        let pointer = snapshot.pointer?;
        let count = crate::native::element_attribute_count(pointer);
        let mut after_current = false;
        for index in 0..count {
            let Some(candidate) =
                crate::native::element_attribute_at(pointer, index)
            else {
                continue;
            };
            if after_current && matches_view(snapshot, candidate, 2) {
                return Some(candidate);
            }
            after_current |= candidate == current;
        }
        return None;
    }
    let mut pointer = crate::native::node_next_sibling(current);
    while let Some(candidate) = pointer {
        if matches_view(snapshot, candidate, 1) {
            return Some(candidate);
        }
        pointer = crate::native::node_next_sibling(candidate);
    }
    None
}

/// Resolves php-src's non-destructive first node for one direct or filtered view.
fn effective_node(snapshot: &Snapshot) -> Option<usize> {
    if snapshot.iterator_kind == SimpleXmlIteratorType::None {
        snapshot.pointer
    } else {
        first_member(snapshot)
    }
}

/// Creates one unowned direct iterator-data view inheriting its parent's namespace filter.
fn iterator_data_object(snapshot: &Snapshot, pointer: usize) -> SimpleXmlObject {
    SimpleXmlObject::new(
        pointer,
        Rc::clone(&snapshot.document),
        snapshot.wrapper_kind,
        SimpleXmlIteratorState::direct(
            snapshot.namespace_or_prefix.clone(),
            snapshot.is_prefix,
        ),
    )
}

/// Exposes one eagerly materialized iterator-data wrapper through the private result channel.
///
/// PHP stores this object strongly in `sxe->iter.data` even though `rewind()` and
/// `next()` are publicly void. The compiler consumes this bridge handle into its
/// hidden strong-wrapper slot before exposing PHP's null result.
fn iterator_current_result(
    context: &mut Context,
    current: u64,
) -> Result<DispatchResult, ()> {
    context.expose_simplexml_handle(current)?;
    let wrapper_kind = super::object(context, current)?.wrapper_kind();
    Ok(DispatchResult::typed_bridge_handle(current, wrapper_kind))
}

/// Creates one fresh public view retaining the receiver's concrete PHP wrapper class.
fn fresh_view(
    context: &mut Context,
    snapshot: &Snapshot,
    pointer: usize,
    iterator: SimpleXmlIteratorState,
) -> DispatchResult {
    super::fresh_result(
        context,
        SimpleXmlObject::new(
            pointer,
            Rc::clone(&snapshot.document),
            snapshot.wrapper_kind,
            iterator,
        ),
    )
}

/// Reads one nullable string argument while preserving omission and explicit null.
fn optional_string(
    request: &Request,
    index: usize,
) -> Result<Option<&[u8]>, ()> {
    if index >= request.values.len() {
        return Ok(None);
    }
    request.optional_byte_string(index)
}

/// Extracts the non-empty prefix retained by php-src for an added QName.
fn qualified_name_prefix(qualified_name: &[u8]) -> Option<Vec<u8>> {
    let separator = qualified_name.iter().position(|byte| *byte == b':')?;
    (separator > 0 && separator + 1 < qualified_name.len())
        .then(|| qualified_name[..separator].to_vec())
}

/// Builds a null-initialized ABI record used while reserving flat descendants.
fn null_abi_value() -> Value {
    Value {
        tag: crate::abi::VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }
}

/// Appends owned bytes and returns their range record in one result frame.
fn append_debug_bytes(bytes: &[u8], result_bytes: &mut Vec<u8>) -> Value {
    let offset = result_bytes.len();
    result_bytes.extend_from_slice(bytes);
    Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: bytes.len() as u64,
    }
}

/// Flattens one debug-array key into a previously reserved ABI record.
fn flatten_debug_key(
    key: DebugKey,
    index: usize,
    values: &mut Vec<Value>,
    bytes: &mut Vec<u8>,
) {
    values[index] = match key {
        DebugKey::Integer(value) => Value {
            tag: crate::abi::VALUE_INT,
            flags: 0,
            payload0: value as u64,
            payload1: 0,
        },
        DebugKey::Bytes(value) => append_debug_bytes(&value, bytes),
    };
}

/// Flattens one recursive debug value and mints fresh same-class wrapper handles.
fn flatten_debug_value(
    context: &mut Context,
    snapshot: &Snapshot,
    value: DebugValue,
    index: usize,
    values: &mut Vec<Value>,
    bytes: &mut Vec<u8>,
) {
    match value {
        DebugValue::Bytes(value) => {
            values[index] = append_debug_bytes(&value, bytes);
        }
        DebugValue::Wrapper(pointer) => {
            let object = SimpleXmlObject::new(
                pointer,
                Rc::clone(&snapshot.document),
                snapshot.wrapper_kind,
                SimpleXmlIteratorState::direct(
                    snapshot.namespace_or_prefix.clone(),
                    snapshot.is_prefix,
                ),
            );
            let handle = context.insert_simplexml_external(object);
            values[index] = Value {
                tag: VALUE_BRIDGE_HANDLE,
                flags: 0,
                payload0: handle,
                payload1: snapshot.wrapper_kind,
            };
        }
        DebugValue::Array(children) => {
            let start = values.len();
            let count = children.len();
            values.resize_with(start + count, null_abi_value);
            values[index] = Value {
                tag: VALUE_ARRAY,
                flags: 0,
                payload0: start as u64,
                payload1: count as u64,
            };
            for (offset, child) in children.into_iter().enumerate() {
                flatten_debug_value(
                    context,
                    snapshot,
                    child,
                    start + offset,
                    values,
                    bytes,
                );
            }
        }
        DebugValue::Map(entries) => {
            let start = values.len();
            let count = entries.len();
            values.resize_with(start + count * 2, null_abi_value);
            values[index] = Value {
                tag: VALUE_MAP,
                flags: 0,
                payload0: start as u64,
                payload1: count as u64,
            };
            for (offset, (key, child)) in entries.into_iter().enumerate() {
                let pair = start + offset * 2;
                flatten_debug_key(key, pair, values, bytes);
                flatten_debug_value(
                    context,
                    snapshot,
                    child,
                    pair + 1,
                    values,
                    bytes,
                );
            }
        }
    }
}

/// Coalesces duplicate SimpleXML property names into insertion-ordered arrays.
fn add_debug_property(
    entries: &mut Vec<(DebugKey, DebugValue)>,
    name: Vec<u8>,
    value: DebugValue,
) {
    let existing = entries.iter_mut().find_map(|(key, value)| match key {
        DebugKey::Bytes(key) if *key == name => Some(value),
        DebugKey::Integer(_) | DebugKey::Bytes(_) => None,
    });
    let Some(existing) = existing else {
        entries.push((DebugKey::Bytes(name), value));
        return;
    };
    match existing {
        DebugValue::Array(values) => values.push(value),
        _ => {
            let first = std::mem::replace(existing, DebugValue::Array(Vec::new()));
            *existing = DebugValue::Array(vec![first, value]);
        }
    }
}

/// Produces php-src's scalar-or-wrapper representation for one XML node.
fn debug_base_value(pointer: usize) -> DebugValue {
    let first_child = crate::native::node_first_child(pointer);
    if first_child.is_some_and(|child| {
        crate::native::node_type(child) == 3
            && !crate::native::text_is_blank(child)
    }) {
        DebugValue::Bytes(crate::native::simplexml_node_list_content(pointer))
    } else {
        DebugValue::Wrapper(pointer)
    }
}

/// Selects the first node and traversal mode used by php-src's property hash builder.
fn first_debug_content(snapshot: &Snapshot, node: usize) -> (Option<usize>, bool) {
    match snapshot.iterator_kind {
        SimpleXmlIteratorType::AttrList => (None, false),
        SimpleXmlIteratorType::None => {
            (crate::native::node_first_child(node), false)
        }
        SimpleXmlIteratorType::Child => (Some(node), false),
        SimpleXmlIteratorType::Element => {
            let first_child = crate::native::node_first_child(node);
            let parent = crate::native::node_parent(node);
            let must_read_children = first_child.is_none()
                || parent.is_none()
                || crate::native::node_next_sibling(node).is_none()
                || first_child
                    .is_some_and(|child| crate::native::node_next_sibling(child).is_some())
                || first_child
                    .is_some_and(|child| crate::native::node_first_child(child).is_some())
                || parent.is_some_and(|parent| {
                    crate::native::node_first_child(parent)
                        == crate::native::node_last_child(parent)
                });
            if must_read_children {
                (first_child, false)
            } else {
                (Some(node), true)
            }
        }
    }
}

/// Advances one php-src debug-property traversal with or without iterator filtering.
fn next_debug_content(
    snapshot: &Snapshot,
    pointer: usize,
    uses_iterator: bool,
) -> Option<usize> {
    if uses_iterator {
        next_member(snapshot, pointer)
    } else {
        crate::native::node_next_sibling(pointer)
    }
}

/// Reads one optional boolean using its declared default when omitted or null.
fn optional_bool(
    request: &Request,
    index: usize,
    default: bool,
) -> Result<bool, ()> {
    if index >= request.values.len() {
        return Ok(default);
    }
    Ok(request.optional_boolean(index)?.unwrap_or(default))
}

/// Attaches PHP's implicit-null deprecation for one declared boolean parameter.
fn with_null_bool_deprecation(
    result: DispatchResult,
    request: &Request,
    index: usize,
    message: &[u8],
) -> DispatchResult {
    if request
        .values
        .get(index)
        .is_some_and(|value| value.tag == VALUE_NULL)
    {
        result.with_warning(message)
    } else {
        result
    }
}

/// Builds one associative string array as alternating key/value ABI records.
fn namespace_array(items: Vec<(Vec<u8>, Vec<u8>)>) -> DispatchResult {
    let mut values = Vec::with_capacity(items.len() * 2);
    let mut bytes = Vec::new();
    for (prefix, namespace_uri) in items {
        let prefix_offset = bytes.len();
        bytes.extend_from_slice(&prefix);
        values.push(Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: prefix_offset as u64,
            payload1: prefix.len() as u64,
        });
        let uri_offset = bytes.len();
        bytes.extend_from_slice(&namespace_uri);
        values.push(Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: uri_offset as u64,
            payload1: namespace_uri.len() as u64,
        });
    }
    DispatchResult {
        frame: ResultFrame::map(values.len() / 2, values, bytes),
        value_tag: VALUE_MAP,
    }
}

/// Builds one indexed array of fresh SimpleXML wrapper handles.
fn node_array(
    context: &mut Context,
    snapshot: &Snapshot,
    pointers: Vec<usize>,
) -> DispatchResult {
    let mut values = Vec::with_capacity(pointers.len());
    for pointer in pointers {
        let pointer = if crate::native::node_type(pointer) == 3 {
            crate::native::node_parent(pointer).unwrap_or(pointer)
        } else {
            pointer
        };
        let object = if crate::native::node_type(pointer) == 2 {
            let parent = crate::native::node_parent(pointer).unwrap_or(pointer);
            SimpleXmlObject::new(
                parent,
                Rc::clone(&snapshot.document),
                snapshot.wrapper_kind,
                SimpleXmlIteratorState::new(
                    SimpleXmlIteratorType::AttrList,
                    crate::native::node_local_name(pointer),
                    crate::native::node_namespace_uri(pointer),
                    false,
                ),
            )
        } else {
            SimpleXmlObject::new(
                pointer,
                Rc::clone(&snapshot.document),
                snapshot.wrapper_kind,
                SimpleXmlIteratorState::direct(None, false),
            )
        };
        let wrapper_kind = object.wrapper_kind();
        let handle = context.insert_simplexml_external(object);
        values.push(Value {
            tag: VALUE_BRIDGE_HANDLE,
            flags: 0,
            payload0: handle,
            payload1: wrapper_kind,
        });
    }
    DispatchResult {
        frame: ResultFrame::array(values.len(), values, Vec::new()),
        value_tag: VALUE_ARRAY,
    }
}

/// Executes `SimpleXMLElement::__debugInfo()` with the method's ordinary array result shape.
pub(in crate::dispatch) fn debug_info(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let mut entries = Vec::new();

    let attribute_root = if snapshot.iterator_kind == SimpleXmlIteratorType::Element {
        effective_node(&snapshot)
    } else {
        snapshot.pointer
    };
    if let Some(attribute_root) = attribute_root {
        if crate::native::node_type(attribute_root) == 1 {
            let mut attributes = Vec::new();
            let attribute_count =
                crate::native::element_attribute_count(attribute_root);
            for index in 0..attribute_count {
                let Some(attribute) =
                    crate::native::element_attribute_at(attribute_root, index)
                else {
                    continue;
                };
                if crate::native::node_type(attribute) != 2
                    || !matches_namespace(&snapshot, attribute)
                    || (snapshot.iterator_kind == SimpleXmlIteratorType::AttrList
                        && !matches_name(&snapshot, attribute))
                {
                    continue;
                }
                let Some(name) = crate::native::node_local_name(attribute) else {
                    continue;
                };
                attributes.push((
                    DebugKey::Bytes(name),
                    DebugValue::Bytes(
                        crate::native::simplexml_node_list_content(attribute),
                    ),
                ));
            }
            if !attributes.is_empty() {
                entries.push((
                    DebugKey::Bytes(b"@attributes".to_vec()),
                    DebugValue::Map(attributes),
                ));
            }
        }
    }

    if let Some(node) = effective_node(&snapshot) {
        if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
            // php-src exposes attributes above but does not traverse child nodes.
        } else if crate::native::node_type(node) == 2 {
            entries.push((
                DebugKey::Integer(0),
                DebugValue::Bytes(
                    crate::native::simplexml_node_list_content(node),
                ),
            ));
        } else {
            let (mut current, uses_iterator) = first_debug_content(&snapshot, node);
            let mut next_index = 0_i64;
            while let Some(pointer) = current {
                let node_type = crate::native::node_type(pointer);
                if node_type == 3 {
                    let isolated = crate::native::node_previous_sibling(pointer).is_none()
                        && crate::native::node_next_sibling(pointer).is_none();
                    if isolated && !crate::native::text_is_blank(pointer) {
                        let content =
                            crate::native::node_content(pointer).unwrap_or_default();
                        if !content.is_empty() {
                            entries.push((
                                DebugKey::Integer(next_index),
                                DebugValue::Bytes(content),
                            ));
                            next_index += 1;
                        }
                    }
                } else if node_type != 1 || matches_namespace(&snapshot, pointer) {
                    if let Some(name) = crate::native::simplexml_node_name(pointer) {
                        let value = debug_base_value(pointer);
                        if uses_iterator {
                            entries.push((DebugKey::Integer(next_index), value));
                            next_index += 1;
                        } else {
                            add_debug_property(&mut entries, name, value);
                        }
                    }
                }
                if node_type == 17 {
                    break;
                }
                current = next_debug_content(&snapshot, pointer, uses_iterator);
            }
        }
    }

    let count = entries.len();
    let mut values = Vec::new();
    let mut bytes = Vec::new();
    values.resize_with(count * 2, null_abi_value);
    for (offset, (key, value)) in entries.into_iter().enumerate() {
        let pair = offset * 2;
        flatten_debug_key(key, pair, &mut values, &mut bytes);
        flatten_debug_value(
            context,
            &snapshot,
            value,
            pair + 1,
            &mut values,
            &mut bytes,
        );
    }
    Ok(DispatchResult {
        frame: ResultFrame::map(count, values, bytes),
        value_tag: VALUE_MAP,
    })
}

/// Executes `SimpleXMLElement::__toString()` without invoking an overridden user method.
pub(in crate::dispatch) fn to_string(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        effective_node(&snapshot)
            .map(crate::native::simplexml_node_list_content)
            .unwrap_or_default(),
    ))
}

/// Executes `SimpleXMLElement::addAttribute()` with exact warning and void contracts.
pub(in crate::dispatch) fn add_attribute(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() < 2 || request.values.len() > 3 {
        return Err(());
    }
    let qualified_name = request.byte_string(0)?;
    let value = request.byte_string(1)?;
    let namespace_uri = optional_string(request, 2)?;
    if qualified_name.is_empty() {
        return Ok(DispatchResult::value_error(
            b"SimpleXMLElement::addAttribute(): Argument #1 ($qualifiedName) must not be empty",
        ));
    }
    let snapshot = snapshot(context, request.header.receiver)?;
    let Some(mut node) = effective_node(&snapshot) else {
        return Ok(DispatchResult::null().with_callsite_location_warning(
            b"Warning: SimpleXMLElement::addAttribute(): Unable to locate parent Element",
        ));
    };
    if crate::native::node_type(node) != 1 {
        let Some(parent) = crate::native::node_parent(node) else {
            return Ok(DispatchResult::null().with_callsite_location_warning(
                b"Warning: SimpleXMLElement::addAttribute(): Unable to locate parent Element",
            ));
        };
        node = parent;
    }
    let status = crate::native::simplexml_add_attribute(
        snapshot.document.pointer(),
        node,
        qualified_name,
        value,
        namespace_uri,
    );
    Ok(match status {
        0 => DispatchResult::null(),
        -2 => DispatchResult::null().with_callsite_location_warning(
            b"Warning: SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace",
        ),
        -3 => DispatchResult::null().with_callsite_location_warning(
            b"Warning: SimpleXMLElement::addAttribute(): Attribute already exists",
        ),
        _ => DispatchResult::error(
            b"SimpleXMLElement::addAttribute(): memory allocation failed",
        ),
    })
}

/// Executes `SimpleXMLElement::addChild()` and returns a fresh same-class wrapper.
pub(in crate::dispatch) fn add_child(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.is_empty() || request.values.len() > 3 {
        return Err(());
    }
    let qualified_name = request.byte_string(0)?;
    let value = optional_string(request, 1)?;
    let namespace_uri = optional_string(request, 2)?;
    if qualified_name.is_empty() {
        return Ok(DispatchResult::value_error(
            b"SimpleXMLElement::addChild(): Argument #1 ($qualifiedName) must not be empty",
        ));
    }
    let snapshot = snapshot(context, request.header.receiver)?;
    if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        return Ok(DispatchResult::null().with_callsite_location_warning(
            b"Warning: SimpleXMLElement::addChild(): Cannot add element to attributes",
        ));
    }
    let Some(node) = effective_node(&snapshot) else {
        return Ok(DispatchResult::null().with_callsite_location_warning(
            b"Warning: SimpleXMLElement::addChild(): Cannot add child. Parent is not a permanent member of the XML tree",
        ));
    };
    let outcome = crate::native::simplexml_add_child(
        snapshot.document.pointer(),
        node,
        qualified_name,
        value,
        namespace_uri,
    );
    let Some(child) = outcome.pointer else {
        return Ok(DispatchResult::error(
            b"SimpleXMLElement::addChild(): memory allocation failed",
        ));
    };
    let iterator_name = crate::native::node_local_name(child);
    let iterator_prefix = qualified_name_prefix(qualified_name);
    Ok(fresh_view(
        context,
        &snapshot,
        child,
        SimpleXmlIteratorState::new(
            SimpleXmlIteratorType::None,
            iterator_name,
            iterator_prefix,
            false,
        ),
    ))
}

/// Executes `SimpleXMLElement::asXML()` and its `saveXML()` alias.
pub(in crate::dispatch) fn as_xml(
    context: &Context,
    request: &Request,
    callable: &'static str,
) -> Result<DispatchResult, ()> {
    match prepare_as_xml(context, request, callable)? {
        super::super::document_io::FilePreparation::Ready(prepared) => {
            let written = super::super::document_io::resolve_path(&prepared.path)
                .is_some_and(|path| std::fs::write(path, &prepared.bytes).is_ok());
            Ok(DispatchResult::boolean(written))
        }
        super::super::document_io::FilePreparation::Complete(result) => Ok(result),
    }
}

/// Validates and serializes `asXML()` without invoking registered stream callbacks.
pub(in crate::dispatch) fn prepare_as_xml(
    context: &Context,
    request: &Request,
    callable: &'static str,
) -> Result<super::super::document_io::FilePreparation, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let filename = optional_string(request, 0)?;
    if filename.is_some_and(<[u8]>::is_empty) {
        return Ok(super::super::document_io::FilePreparation::Complete(
            DispatchResult::value_error(b"Path must not be empty"),
        ));
    }
    if filename.is_some_and(|filename| filename.contains(&0)) {
        let mut message = callable.as_bytes().to_vec();
        message.extend_from_slice(
            b"(): Argument #1 ($filename) must not contain any null bytes",
        );
        return Ok(super::super::document_io::FilePreparation::Complete(
            DispatchResult::value_error(&message),
        ));
    }
    let snapshot = snapshot(context, request.header.receiver)?;
    let Some(node) = effective_node(&snapshot) else {
        return Ok(super::super::document_io::FilePreparation::Complete(
            DispatchResult::boolean(false),
        ));
    };
    let mode = if snapshot.document.dom_api() == Some(DomApiFamily::Modern) {
        1
    } else {
        0
    };
    let is_document_element = crate::native::node_parent(node)
        .is_some_and(|parent| crate::native::node_type(parent) == 9);
    let Some(bytes) = (if is_document_element {
        crate::native::document_serialize(
            snapshot.document.pointer(),
            None,
            false,
            mode,
            0,
        )
    } else if snapshot.document.dom_api() == Some(DomApiFamily::Modern) {
        crate::native::document_serialize_node(
            snapshot.document.pointer(),
            node,
            false,
            mode,
            0,
        )
    } else {
        crate::native::simplexml_serialize_node(
            snapshot.document.pointer(),
            node,
        )
    }) else {
        return Ok(super::super::document_io::FilePreparation::Complete(
            DispatchResult::boolean(false),
        ));
    };
    let Some(filename) = filename else {
        return Ok(super::super::document_io::FilePreparation::Complete(
            DispatchResult::bytes(bytes),
        ));
    };
    Ok(super::super::document_io::FilePreparation::Ready(
        super::super::document_io::PreparedFile {
            path: filename.to_vec(),
            bytes,
            method: callable,
        },
    ))
}

/// Executes `SimpleXMLElement::attributes()` as a fresh filtered view.
pub(in crate::dispatch) fn attributes(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 2 {
        return Err(());
    }
    let namespace_or_prefix = optional_string(request, 0)?
        .filter(|namespace| !namespace.is_empty())
        .map(<[u8]>::to_vec);
    let is_prefix = optional_bool(request, 1, false)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let result = if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        DispatchResult::null()
    } else if let Some(node) = effective_node(&snapshot) {
        fresh_view(
            context,
            &snapshot,
            node,
            SimpleXmlIteratorState::new(
                SimpleXmlIteratorType::AttrList,
                None,
                namespace_or_prefix,
                is_prefix,
            ),
        )
    } else {
        DispatchResult::null()
    };
    Ok(with_null_bool_deprecation(
        result,
        request,
        1,
        b"Deprecated: SimpleXMLElement::attributes(): Passing null to parameter #2 ($isPrefix) of type bool is deprecated\n",
    ))
}

/// Executes `SimpleXMLElement::children()` as a fresh filtered view.
pub(in crate::dispatch) fn children(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 2 {
        return Err(());
    }
    let namespace_or_prefix = optional_string(request, 0)?
        .filter(|namespace| !namespace.is_empty())
        .map(<[u8]>::to_vec);
    let is_prefix = optional_bool(request, 1, false)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let result = if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        DispatchResult::null()
    } else if let Some(node) = effective_node(&snapshot) {
        fresh_view(
            context,
            &snapshot,
            node,
            SimpleXmlIteratorState::new(
                SimpleXmlIteratorType::Child,
                None,
                namespace_or_prefix,
                is_prefix,
            ),
        )
    } else {
        DispatchResult::null()
    };
    Ok(with_null_bool_deprecation(
        result,
        request,
        1,
        b"Deprecated: SimpleXMLElement::children(): Passing null to parameter #2 ($isPrefix) of type bool is deprecated\n",
    ))
}

/// Executes `SimpleXMLElement::count()` without disturbing live iterator data.
pub(in crate::dispatch) fn count(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let mut count = 0_i64;
    let mut pointer = first_member(&snapshot);
    while let Some(current) = pointer {
        count = count.checked_add(1).ok_or(())?;
        pointer = next_member(&snapshot, current);
    }
    Ok(DispatchResult::integer(count))
}

/// Executes `SimpleXMLElement::current()` and exposes the exact iterator-data identity.
pub(in crate::dispatch) fn current(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let Some(current) = snapshot.iterator_current else {
        return Ok(DispatchResult::error(
            b"Iterator not initialized or already consumed",
        ));
    };
    context.expose_simplexml_handle(current)?;
    let wrapper_kind = super::object(context, current)?.wrapper_kind();
    Ok(DispatchResult::typed_bridge_handle(current, wrapper_kind))
}

/// Executes `SimpleXMLElement::getChildren()` as an alias for the current iterator object.
pub(in crate::dispatch) fn get_children(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        return Ok(DispatchResult::null());
    }
    let Some(current) = snapshot.iterator_current else {
        return Ok(DispatchResult::null());
    };
    context.expose_simplexml_handle(current)?;
    let wrapper_kind = super::object(context, current)?.wrapper_kind();
    Ok(DispatchResult::typed_bridge_handle(current, wrapper_kind))
}

/// Executes `SimpleXMLElement::getDocNamespaces()` with root/raw-node selection semantics.
pub(in crate::dispatch) fn get_doc_namespaces(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 2 {
        return Err(());
    }
    let recursive = optional_bool(request, 0, false)?;
    let from_root = optional_bool(request, 1, true)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let selected = if from_root {
        crate::native::document_element(snapshot.document.pointer())
    } else {
        snapshot.pointer
    };
    let result = match selected {
        None if from_root => DispatchResult::boolean(false),
        None => DispatchResult::error(
            b"SimpleXMLElement is not properly initialized",
        ),
        Some(node) => {
            let include_xmlns =
                snapshot.document.dom_api() == Some(DomApiFamily::Modern);
            let outcome = crate::native::simplexml_get_doc_namespaces(
                snapshot.document.pointer(),
                Some(node),
                recursive,
                from_root,
                include_xmlns,
            )?;
            namespace_array(outcome.items)
        }
    };
    let result = with_null_bool_deprecation(
        result,
        request,
        0,
        b"Deprecated: SimpleXMLElement::getDocNamespaces(): Passing null to parameter #1 ($recursive) of type bool is deprecated\n",
    );
    Ok(with_null_bool_deprecation(
        result,
        request,
        1,
        b"Deprecated: SimpleXMLElement::getDocNamespaces(): Passing null to parameter #2 ($fromRoot) of type bool is deprecated\n",
    ))
}

/// Executes `SimpleXMLElement::getName()` over the non-destructive selected node.
pub(in crate::dispatch) fn get_name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        effective_node(&snapshot)
            .and_then(crate::native::node_local_name)
            .unwrap_or_default(),
    ))
}

/// Executes `SimpleXMLElement::getNamespaces()` over the non-destructive selected node.
pub(in crate::dispatch) fn get_namespaces(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let recursive = optional_bool(request, 0, false)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let result = if let Some(node) = effective_node(&snapshot) {
        let outcome = crate::native::simplexml_get_namespaces(node, recursive)?;
        namespace_array(outcome.items)
    } else {
        namespace_array(Vec::new())
    };
    Ok(with_null_bool_deprecation(
        result,
        request,
        0,
        b"Deprecated: SimpleXMLElement::getNamespaces(): Passing null to parameter #1 ($recursive) of type bool is deprecated\n",
    ))
}

/// Executes `SimpleXMLElement::hasChildren()` for the current iterator data.
pub(in crate::dispatch) fn has_children(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        return Ok(DispatchResult::boolean(false));
    }
    let Some(current) = snapshot.iterator_current else {
        return Ok(DispatchResult::boolean(false));
    };
    let current = super::object(context, current)?.pointer();
    let mut child = crate::native::node_first_child(current);
    while let Some(pointer) = child {
        if crate::native::node_type(pointer) == 1 {
            return Ok(DispatchResult::boolean(true));
        }
        child = crate::native::node_next_sibling(pointer);
    }
    Ok(DispatchResult::boolean(false))
}

/// Executes `SimpleXMLElement::key()` for the exact iterator-data node.
pub(in crate::dispatch) fn key(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let Some(current) = snapshot.iterator_current else {
        return Ok(DispatchResult::error(
            b"Iterator not initialized or already consumed",
        ));
    };
    let pointer = super::object(context, current)?.pointer();
    Ok(DispatchResult::bytes(
        crate::native::node_local_name(pointer).unwrap_or_default(),
    ))
}

/// Executes `SimpleXMLElement::next()` while balancing the prior internal owner.
pub(in crate::dispatch) fn next(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    let Some(current_handle) = snapshot.iterator_current else {
        return Ok(DispatchResult::null());
    };
    let current = super::object(context, current_handle)?.pointer();
    let next = next_member(&snapshot, current);
    context.clear_simplexml_iterator_current(snapshot.handle)?;
    if let Some(next) = next {
        let current = context.install_fresh_simplexml_iterator_current(
            snapshot.handle,
            iterator_data_object(&snapshot, next),
        )?;
        return iterator_current_result(context, current);
    }
    Ok(DispatchResult::null())
}

/// Executes `SimpleXMLElement::registerXPathNamespace()` on wrapper-local state.
pub(in crate::dispatch) fn register_xpath_namespace(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let prefix = request.byte_string(0)?.to_vec();
    let namespace_uri = request.byte_string(1)?.to_vec();
    if prefix.is_empty() {
        return Ok(DispatchResult::boolean(false));
    }
    let snapshot = snapshot(context, request.header.receiver)?;
    let probe = crate::native::xpath_evaluate(
        snapshot.document.pointer(),
        effective_node(&snapshot),
        false,
        false,
        false,
        b"true()",
        &[(prefix.clone(), namespace_uri.clone())],
        0,
        None,
        0,
        &[],
    );
    if probe.is_err() {
        return Ok(DispatchResult::boolean(false));
    }
    super::object_mut(context, snapshot.handle)?
        .register_xpath_namespace(prefix, namespace_uri);
    Ok(DispatchResult::boolean(true))
}

/// Executes `SimpleXMLElement::rewind()` and installs one internally owned current wrapper.
pub(in crate::dispatch) fn rewind(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    context.clear_simplexml_iterator_current(snapshot.handle)?;
    if let Some(first) = first_member(&snapshot) {
        let current = context.install_fresh_simplexml_iterator_current(
            snapshot.handle,
            iterator_data_object(&snapshot, first),
        )?;
        return iterator_current_result(context, current);
    }
    Ok(DispatchResult::null())
}

/// Executes `SimpleXMLElement::valid()` from the live iterator-data handle.
pub(in crate::dispatch) fn valid(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    Ok(DispatchResult::boolean(
        context
            .simplexml_iterator_current(request.header.receiver)?
            .is_some(),
    ))
}

/// Executes `SimpleXMLElement::xpath()` with in-scope and wrapper-registered namespaces.
pub(in crate::dispatch) fn xpath(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let expression = request.byte_string(0)?;
    let snapshot = snapshot(context, request.header.receiver)?;
    if snapshot.iterator_kind == SimpleXmlIteratorType::AttrList {
        return Ok(DispatchResult::null());
    }
    if snapshot.pointer.is_none() {
        return Ok(DispatchResult::error(
            b"SimpleXMLElement is not properly initialized",
        ));
    }
    let Some(node) = effective_node(&snapshot) else {
        return Ok(DispatchResult::null());
    };
    let outcome = crate::native::xpath_evaluate(
        snapshot.document.pointer(),
        Some(node),
        false,
        true,
        false,
        expression,
        &snapshot.xpath_namespaces,
        0,
        None,
        0,
        &[],
    )?;
    record_errors(context, &outcome.errors);
    if outcome.status != 0 {
        let result = DispatchResult::boolean(false);
        return Ok(if context.internal_errors {
            result
        } else {
            result.with_callsite_location_libxml_warnings(
                b"SimpleXMLElement::xpath",
                &outcome.errors,
            )
        });
    }
    match outcome.value {
        crate::native::XPathValue::Nodes(mut pointers) => {
            pointers.retain(|pointer| {
                matches!(crate::native::node_type(*pointer), 1 | 2 | 3 | 7 | 8)
            });
            Ok(node_array(context, &snapshot, pointers))
        }
        crate::native::XPathValue::Boolean(_) => Ok(
            DispatchResult::boolean(false).with_callsite_location_warning(
                b"Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, bool returned",
            ),
        ),
        crate::native::XPathValue::Number(_) => Ok(
            DispatchResult::boolean(false).with_callsite_location_warning(
                b"Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, number returned",
            ),
        ),
        crate::native::XPathValue::Bytes(_) => Ok(
            DispatchResult::boolean(false).with_callsite_location_warning(
                b"Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, string returned",
            ),
        ),
        crate::native::XPathValue::Null => Ok(
            DispatchResult::boolean(false).with_callsite_location_warning(
                b"Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, undefined returned",
            ),
        ),
    }
}
