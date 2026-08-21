//! Purpose:
//! Implements PHP ParentNode and ChildNode variadic tree-mutation algorithms.
//! Converts node/string inputs in source order while preserving wrapper identity.
//!
//! Called from:
//! - `super::routes::dispatch()` for legacy and modern convenience mutators.
//!
//! Key details:
//! - Viable sibling selection happens before argument nodes move out of their trees.
//! - Document fragments contribute their children and remain empty live wrappers.
//! - Validation precedes insertion, while directly passed nodes remain detached on failure.

use std::collections::HashSet;
use std::rc::Rc;

use crate::abi::{VALUE_BRIDGE_HANDLE, VALUE_BYTES};
use crate::context::Context;
use crate::objects::{
    DocumentFamily, DocumentGraph, LegacyDocumentFlag,
};
use crate::request::Request;

use super::{
    node::adopt_direct_legacy_node, receiver_pointer_and_graph,
    require_no_values, DispatchResult,
};

/// Appends converted node/string arguments to one parent receiver.
pub(super) fn append(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (parent, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(error) = validate_arguments(context, request, &graph)? {
        return Ok(error);
    }
    let nodes = prepare_nodes(context, request, &graph)?;
    if let Some(error) = validate_sequence(parent, &graph, &nodes) {
        return Ok(error);
    }
    insert_sequence(context, parent, &nodes, None)?;
    Ok(DispatchResult::null())
}

/// Prepends converted node/string arguments before the parent's remaining first child.
pub(super) fn prepend(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (parent, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(error) = validate_arguments(context, request, &graph)? {
        return Ok(error);
    }
    let nodes = prepare_nodes(context, request, &graph)?;
    if let Some(error) = validate_sequence(parent, &graph, &nodes) {
        return Ok(error);
    }
    let reference = crate::native::node_first_child(parent);
    insert_sequence(context, parent, &nodes, reference)?;
    Ok(DispatchResult::null())
}

/// Inserts converted node/string arguments before one child receiver.
pub(super) fn before(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    sibling_insertion(context, request, false)
}

/// Inserts converted node/string arguments after one child receiver.
pub(super) fn after(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    sibling_insertion(context, request, true)
}

/// Replaces one child receiver with converted node/string arguments.
pub(super) fn replace_with(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (child, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(error) = validate_arguments(context, request, &graph)? {
        return Ok(error);
    }
    let Some(parent) = crate::native::node_parent(child) else {
        return Ok(DispatchResult::null());
    };
    let argument_pointers = argument_node_pointers(context, request)?;
    let argument_set = argument_pointers.into_iter().collect::<HashSet<_>>();
    let mut reference = crate::native::node_next_sibling(child);
    while reference.is_some_and(|pointer| argument_set.contains(&pointer)) {
        reference = reference.and_then(crate::native::node_next_sibling);
    }
    let nodes = prepare_nodes(context, request, &graph)?;
    if let Some(error) = validate_sequence(parent, &graph, &nodes) {
        return Ok(error);
    }
    if crate::native::node_parent(child) == Some(parent) {
        detach_node(context, parent, child, &graph)?;
    }
    insert_sequence(context, parent, &nodes, reference)?;
    Ok(DispatchResult::null())
}

/// Replaces every direct child with converted node/string arguments.
pub(super) fn replace_children(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (parent, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(error) = validate_arguments(context, request, &graph)? {
        return Ok(error);
    }
    let nodes = prepare_nodes(context, request, &graph)?;
    if let Some(error) = validate_sequence(parent, &graph, &nodes) {
        return Ok(error);
    }
    let mut child = crate::native::node_first_child(parent);
    while let Some(pointer) = child {
        child = crate::native::node_next_sibling(pointer);
        detach_node(context, parent, pointer, &graph)?;
    }
    insert_sequence(context, parent, &nodes, None)?;
    Ok(DispatchResult::null())
}

/// Removes one child receiver or applies PHP's missing-parent error policy.
pub(super) fn remove(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (child, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let Some(parent) = crate::native::node_parent(child) else {
        return Ok(exception_or_null(&graph, 8, b"Not Found Error"));
    };
    detach_node(context, parent, child, &graph)?;
    Ok(DispatchResult::null())
}

/// Implements viable-sibling selection shared by `before()` and `after()`.
fn sibling_insertion(
    context: &mut Context,
    request: &Request,
    after: bool,
) -> Result<DispatchResult, ()> {
    let (receiver, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(error) = validate_arguments(context, request, &graph)? {
        return Ok(error);
    }
    let Some(parent) = crate::native::node_parent(receiver) else {
        return Ok(DispatchResult::null());
    };
    let argument_pointers = argument_node_pointers(context, request)?;
    let argument_set = argument_pointers.into_iter().collect::<HashSet<_>>();
    let mut viable = if after {
        crate::native::node_next_sibling(receiver)
    } else {
        crate::native::node_previous_sibling(receiver)
    };
    while viable.is_some_and(|pointer| argument_set.contains(&pointer)) {
        viable = if after {
            viable.and_then(crate::native::node_next_sibling)
        } else {
            viable.and_then(crate::native::node_previous_sibling)
        };
    }
    let nodes = prepare_nodes(context, request, &graph)?;
    if let Some(error) = validate_sequence(parent, &graph, &nodes) {
        return Ok(error);
    }
    let reference = if after {
        viable
    } else {
        match viable {
            Some(previous) => crate::native::node_next_sibling(previous),
            None => crate::native::node_first_child(parent),
        }
    };
    insert_sequence(context, parent, &nodes, reference)?;
    Ok(DispatchResult::null())
}

/// Validates argument tags and same-document ownership before any node moves.
fn validate_arguments(
    context: &mut Context,
    request: &Request,
    graph: &Rc<DocumentGraph>,
) -> Result<Option<DispatchResult>, ()> {
    for (index, value) in request.values.iter().enumerate() {
        match value.tag {
            VALUE_BYTES => {
                let _ = request.byte_string(index)?;
            }
            VALUE_BRIDGE_HANDLE => {
                let handle = request.bridge_handle(index)?;
                if let Err(exception) =
                    adopt_direct_legacy_node(context, handle, graph)
                {
                    return Ok(Some(exception));
                }
                let (_, argument_graph, _) =
                    receiver_pointer_and_graph(context, handle)?;
                if !Rc::ptr_eq(graph, &argument_graph) {
                    return Ok(Some(exception_or_null(
                        graph,
                        4,
                        b"Wrong Document Error",
                    )));
                }
            }
            _ => return Err(()),
        }
    }
    Ok(None)
}

/// Captures directly passed native node pointers for viable-sibling selection.
fn argument_node_pointers(
    context: &Context,
    request: &Request,
) -> Result<Vec<usize>, ()> {
    let mut pointers = Vec::new();
    for (index, value) in request.values.iter().enumerate() {
        if value.tag == VALUE_BRIDGE_HANDLE {
            let handle = request.bridge_handle(index)?;
            pointers.push(receiver_pointer_and_graph(context, handle)?.0);
        }
    }
    Ok(pointers)
}

/// Converts strings to text and moves passed nodes into one detached ordered list.
fn prepare_nodes(
    context: &mut Context,
    request: &Request,
    graph: &Rc<DocumentGraph>,
) -> Result<Vec<usize>, ()> {
    let mut nodes = Vec::new();
    for (index, value) in request.values.iter().enumerate() {
        if value.tag == VALUE_BYTES {
            let pointer = crate::native::document_create_text(
                graph.pointer(),
                request.byte_string(index)?,
            )
            .ok_or(())?;
            context.register_detached_root(pointer, Rc::clone(graph));
            nodes.push(pointer);
            continue;
        }
        let handle = request.bridge_handle(index)?;
        let (pointer, _, is_document) =
            receiver_pointer_and_graph(context, handle)?;
        if crate::native::node_type(pointer) == 11 {
            let mut child = crate::native::node_first_child(pointer);
            while let Some(fragment_child) = child {
                child = crate::native::node_next_sibling(fragment_child);
                detach_node(context, pointer, fragment_child, graph)?;
                nodes.push(fragment_child);
            }
        } else {
            if !is_document {
                detach_if_attached(context, pointer, graph)?;
            }
            nodes.push(pointer);
        }
    }
    Ok(nodes)
}

/// Detaches one passed node from its current parent when necessary.
fn detach_if_attached(
    context: &mut Context,
    pointer: usize,
    graph: &Rc<DocumentGraph>,
) -> Result<(), ()> {
    if let Some(parent) = crate::native::node_parent(pointer) {
        detach_node(context, parent, pointer, graph)?;
    }
    Ok(())
}

/// Validates hierarchy and cycle constraints for a complete converted sequence.
fn validate_sequence(
    parent: usize,
    graph: &Rc<DocumentGraph>,
    nodes: &[usize],
) -> Option<DispatchResult> {
    let parent_type = crate::native::node_type(parent);
    let mut distinct_elements = HashSet::new();
    for pointer in nodes.iter().copied().collect::<HashSet<_>>() {
        let node_type = crate::native::node_type(pointer);
        if pointer == parent
            || crate::native::node_contains(pointer, parent)
            || node_type == 9
        {
            return Some(exception_or_null(
                graph,
                3,
                b"Hierarchy Request Error",
            ));
        }
        let allowed = match parent_type {
            9 => matches!(node_type, 1 | 7 | 8 | 10),
            1 | 11 => matches!(node_type, 1 | 3 | 4 | 5 | 7 | 8),
            _ => false,
        };
        if !allowed {
            return Some(exception_or_null(
                graph,
                3,
                b"Hierarchy Request Error",
            ));
        }
        if node_type == 1 {
            distinct_elements.insert(pointer);
        }
    }
    if parent_type == 9 {
        if let Some(existing) = crate::native::document_element(parent) {
            distinct_elements.insert(existing);
        }
        if distinct_elements.len() > 1 {
            let message = if graph.family() == DocumentFamily::Legacy {
                b"Hierarchy Request Error".as_slice()
            } else {
                b"Cannot have more than one element child in a document"
                    .as_slice()
            };
            return Some(exception_or_null(graph, 3, message));
        }
    }
    None
}

/// Inserts converted nodes in order before one fixed viable reference.
fn insert_sequence(
    context: &mut Context,
    parent: usize,
    nodes: &[usize],
    reference: Option<usize>,
) -> Result<(), ()> {
    for pointer in nodes {
        let inserted =
            crate::native::node_insert_before(parent, *pointer, reference)
                .ok_or(())?;
        if inserted != *pointer {
            return Err(());
        }
        context.attach_detached_root(*pointer);
    }
    Ok(())
}

/// Detaches one direct child and retains it for wrapper-safe context cleanup.
fn detach_node(
    context: &mut Context,
    parent: usize,
    pointer: usize,
    graph: &Rc<DocumentGraph>,
) -> Result<(), ()> {
    if !crate::native::node_unlink_child(parent, pointer) {
        return Err(());
    }
    if !context.detached_roots.contains_key(&pointer) {
        context.register_detached_root(pointer, Rc::clone(graph));
    }
    Ok(())
}

/// Applies legacy strict-error suppression or returns one structured exception.
fn exception_or_null(
    graph: &DocumentGraph,
    code: i32,
    message: &[u8],
) -> DispatchResult {
    if graph.family() == DocumentFamily::Legacy
        && !graph.legacy_flag(LegacyDocumentFlag::StrictErrorChecking)
    {
        DispatchResult::null()
    } else {
        DispatchResult::dom_exception(code, message)
    }
}
