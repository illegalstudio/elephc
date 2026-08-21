//! Purpose:
//! Implements legacy and modern element-adjacent node and text insertion.
//! Keeps position parsing, auto-adoption, and strict-error behavior in one leaf.
//!
//! Called from:
//! - `super::routes::dispatch()` for `insertAdjacentElement()` and `insertAdjacentText()`.
//!
//! Key details:
//! - Legacy string positions are ASCII case-insensitive; modern enum backing values arrive as bytes.
//! - Element arguments are auto-adopted before hierarchy validation, matching php-src mutation order.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{DocumentFamily, DocumentGraph, LegacyDocumentFlag};
use crate::request::Request;

use super::{
    node, receiver_pointer_and_graph, rehome_subtree_handles, DispatchResult,
};

/// One normalized insertion position shared by the legacy string and modern enum surfaces.
#[derive(Clone, Copy)]
enum AdjacentPosition {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

/// One resolved parent and reference pair for native pre-insertion.
struct InsertionLocation {
    parent: usize,
    reference: Option<usize>,
}

/// Inserts one element at the requested position and preserves its wrapper identity.
pub(super) fn insert_element(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (receiver, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let method = b"insertAdjacentElement";
    let Some(position) = parse_position(request.byte_string(0)?) else {
        return Ok(adjacent_failure(&graph, method, 12, b"Syntax Error"));
    };
    let Some(location) = insertion_location(receiver, position) else {
        return Ok(DispatchResult::null());
    };
    let child_handle = request.bridge_handle(1)?;
    let child_pointer = node(context, child_handle)?.pointer();
    if crate::native::node_type(child_pointer) != 1 {
        return Err(());
    }
    if location.reference == Some(child_pointer) {
        return Ok(DispatchResult::bridge_handle(child_handle));
    }

    let modern = graph.family() != DocumentFamily::Legacy;
    let outcome =
        crate::native::document_adopt_node(graph.pointer(), child_pointer, modern);
    if outcome.error_code != 0 {
        return Ok(adoption_failure(&graph, method, outcome.error_code));
    }
    if outcome.pointer != Some(child_pointer) {
        return Err(());
    }
    rehome_subtree_handles(context, child_pointer, Rc::clone(&graph))?;
    context.attach_detached_root(child_pointer);
    context.register_detached_root(child_pointer, Rc::clone(&graph));

    if let Some((code, message)) =
        validate_insertion(location.parent, &graph, child_pointer)
    {
        return Ok(adjacent_failure(&graph, method, code, message));
    }
    let inserted = crate::native::node_insert_before(
        location.parent,
        child_pointer,
        location.reference,
    )
    .ok_or(())?;
    if inserted != child_pointer {
        return Err(());
    }
    context.attach_detached_root(child_pointer);
    Ok(DispatchResult::bridge_handle(child_handle))
}

/// Creates and inserts one text node at the requested position.
pub(super) fn insert_text(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (receiver, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let method = b"insertAdjacentText";
    let Some(position) = parse_position(request.byte_string(0)?) else {
        return Ok(adjacent_failure(&graph, method, 12, b"Syntax Error"));
    };
    let Some(location) = insertion_location(receiver, position) else {
        return Ok(DispatchResult::null());
    };
    let text =
        crate::native::document_create_text(graph.pointer(), request.byte_string(1)?)
            .ok_or(())?;
    context.register_detached_root(text, Rc::clone(&graph));
    if let Some((code, message)) = validate_insertion(location.parent, &graph, text)
    {
        return Ok(adjacent_failure(&graph, method, code, message));
    }
    let inserted =
        crate::native::node_insert_before(location.parent, text, location.reference)
            .ok_or(())?;
    if inserted != text {
        return Err(());
    }
    context.attach_detached_root(text);
    Ok(DispatchResult::null())
}

/// Parses PHP's four case-insensitive position spellings.
fn parse_position(value: &[u8]) -> Option<AdjacentPosition> {
    if value.eq_ignore_ascii_case(b"beforebegin") {
        Some(AdjacentPosition::BeforeBegin)
    } else if value.eq_ignore_ascii_case(b"afterbegin") {
        Some(AdjacentPosition::AfterBegin)
    } else if value.eq_ignore_ascii_case(b"beforeend") {
        Some(AdjacentPosition::BeforeEnd)
    } else if value.eq_ignore_ascii_case(b"afterend") {
        Some(AdjacentPosition::AfterEnd)
    } else {
        None
    }
}

/// Resolves an adjacent position before adoption can unlink the supplied child.
fn insertion_location(
    receiver: usize,
    position: AdjacentPosition,
) -> Option<InsertionLocation> {
    match position {
        AdjacentPosition::BeforeBegin => Some(InsertionLocation {
            parent: crate::native::node_parent(receiver)?,
            reference: Some(receiver),
        }),
        AdjacentPosition::AfterBegin => Some(InsertionLocation {
            parent: receiver,
            reference: crate::native::node_first_child(receiver),
        }),
        AdjacentPosition::BeforeEnd => Some(InsertionLocation {
            parent: receiver,
            reference: None,
        }),
        AdjacentPosition::AfterEnd => Some(InsertionLocation {
            parent: crate::native::node_parent(receiver)?,
            reference: crate::native::node_next_sibling(receiver),
        }),
    }
}

/// Validates cycle and modern-document constraints after PHP-style adoption.
fn validate_insertion(
    parent: usize,
    graph: &Rc<DocumentGraph>,
    child: usize,
) -> Option<(i32, &'static [u8])> {
    if crate::native::node_contains(child, parent) {
        return Some((3, b"Hierarchy Request Error"));
    }
    if graph.family() != DocumentFamily::Legacy
        && crate::native::node_type(parent) == 9
    {
        if crate::native::node_type(child) == 3 {
            return Some((3, b"Cannot insert text as a child of a document"));
        }
        if crate::native::node_type(child) == 1
            && crate::native::document_element(parent)
                .is_some_and(|element| element != child)
        {
            return Some((
                3,
                b"Cannot have more than one element child in a document",
            ));
        }
    }
    if matches!(crate::native::node_type(parent), 1 | 9 | 11) {
        None
    } else {
        Some((3, b"Hierarchy Request Error"))
    }
}

/// Maps native adoption failures to the family-specific public error channel.
fn adoption_failure(
    graph: &DocumentGraph,
    method: &[u8],
    code: i32,
) -> DispatchResult {
    let mapped = match code {
        9 => 9,
        11 => 11,
        _ => 11,
    };
    let message = match mapped {
        9 => b"Not Supported Error".as_slice(),
        _ => b"Invalid State Error".as_slice(),
    };
    adjacent_failure(graph, method, mapped, message)
}

/// Returns a catchable exception or the legacy warning/null pair for loose mode.
fn adjacent_failure(
    graph: &DocumentGraph,
    method: &[u8],
    code: i32,
    message: &[u8],
) -> DispatchResult {
    if graph.family() != DocumentFamily::Legacy
        || graph.legacy_flag(LegacyDocumentFlag::StrictErrorChecking)
    {
        return DispatchResult::dom_exception(code, message);
    }
    let mut warning = Vec::with_capacity(method.len() + message.len() + 32);
    warning.extend_from_slice(b"Warning: DOMElement::");
    warning.extend_from_slice(method);
    warning.extend_from_slice(b"(): ");
    warning.extend_from_slice(message);
    warning.push(b'\n');
    DispatchResult::null().with_warning(&warning)
}
