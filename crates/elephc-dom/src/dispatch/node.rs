//! Purpose:
//! Dispatches PHP DOM node traversal, identity, and basic tree-mutation operations.
//! Keeps canonical wrapper identity tied to the authoritative native document graph.
//!
//! Called from:
//! - `super::dispatch()` for legacy and modern DOM node operations.
//!
//! Key details:
//! - Tree mutations reject cross-document nodes and preserve detached roots.
//! - Pointer results always materialize through the canonical wrapper cache.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::DocumentFamily;
use crate::request::Request;

use super::{
    canonical_document_handle, canonical_pointer_result, dom_exception, node,
    node_name, receiver_pointer_and_graph, rehome_subtree_handles,
    require_no_values, wrapper_kind, DispatchResult,
};

/// Appends one same-document node without merging adjacent text wrapper identities.
///
/// Legacy PHP reports an empty document fragment as a failed append and leaves its
/// hidden owner graph unchanged. The modern API instead returns the unchanged fragment.
pub(super) fn append_child(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let child_handle = request.bridge_handle(0)?;
    let (parent_pointer, parent_graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(result) = dtd_mutation_result(
        parent_graph.family(),
        crate::native::node_type(parent_pointer),
        DtdMutation::AppendOrInsert,
    ) {
        return Ok(result);
    }
    let candidate = node(context, child_handle)?;
    if crate::native::node_type(candidate.pointer()) == 11
        && crate::native::node_first_child(candidate.pointer()).is_none()
    {
        return Ok(if parent_graph.family() == DocumentFamily::Legacy {
            DispatchResult::boolean(false).with_callsite_location_warning(
                b"Warning: DOMNode::appendChild(): Document Fragment is empty",
            )
        } else {
            DispatchResult::bridge_handle(child_handle)
        });
    }
    if let Err(exception) =
        adopt_direct_legacy_node(context, child_handle, &parent_graph)
    {
        return Ok(exception);
    }
    let child = node(context, child_handle)?;
    let child_pointer = child.pointer();
    let child_graph = child.document();
    if let Err(exception) = validate_insertion(
        parent_pointer,
        &parent_graph,
        is_document,
        child_pointer,
        &child_graph,
        None,
    ) {
        return Ok(exception);
    }
    let appended =
        crate::native::node_append_child(parent_pointer, child_pointer).ok_or(())?;
    if appended != child_pointer {
        return Err(());
    }
    context.attach_detached_root(child_pointer);
    Ok(DispatchResult::bridge_handle(child_handle))
}

/// Inserts one same-document node before a direct reference child or appends for null.
pub(super) fn insert_before(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let child_handle = request.bridge_handle(0)?;
    let reference_handle = request.optional_bridge_handle(1)?;
    let (parent_pointer, parent_graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(result) = dtd_mutation_result(
        parent_graph.family(),
        crate::native::node_type(parent_pointer),
        DtdMutation::AppendOrInsert,
    ) {
        return Ok(result);
    }
    if let Err(exception) =
        adopt_direct_legacy_node(context, child_handle, &parent_graph)
    {
        return Ok(exception);
    }
    let child = node(context, child_handle)?;
    let child_pointer = child.pointer();
    let child_graph = child.document();
    let reference_pointer = match reference_handle {
        Some(handle) => {
            let reference = node(context, handle)?;
            if reference.pointer() == child_pointer
                && crate::native::node_parent(reference.pointer()) == Some(parent_pointer)
            {
                return Ok(DispatchResult::bridge_handle(child_handle));
            }
            if crate::native::node_parent(reference.pointer()) != Some(parent_pointer) {
                return Ok(not_found());
            }
            Some(reference.pointer())
        }
        None => None,
    };
    if let Err(exception) = validate_insertion(
        parent_pointer,
        &parent_graph,
        is_document,
        child_pointer,
        &child_graph,
        None,
    ) {
        return Ok(exception);
    }
    let inserted =
        crate::native::node_insert_before(parent_pointer, child_pointer, reference_pointer)
            .ok_or(())?;
    if inserted != child_pointer {
        return Err(());
    }
    context.attach_detached_root(child_pointer);
    Ok(DispatchResult::bridge_handle(child_handle))
}

/// Detaches one direct child while retaining its wrapper and authoritative document graph.
pub(super) fn remove_child(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let child_handle = request.bridge_handle(0)?;
    let (parent_pointer, parent_graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let child = node(context, child_handle)?;
    let child_pointer = child.pointer();
    if !Rc::ptr_eq(&parent_graph, &child.document()) {
        return Ok(not_found());
    }
    if !crate::native::node_unlink_child(parent_pointer, child_pointer) {
        return Ok(not_found());
    }
    context.register_detached_root(child_pointer, parent_graph);
    Ok(DispatchResult::bridge_handle(child_handle))
}

/// Replaces one direct child with a same-document node and detaches the prior child.
pub(super) fn replace_child(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let child_handle = request.bridge_handle(0)?;
    let replaced_handle = request.bridge_handle(1)?;
    let (parent_pointer, parent_graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(result) = dtd_mutation_result(
        parent_graph.family(),
        crate::native::node_type(parent_pointer),
        DtdMutation::Replace,
    ) {
        return Ok(result);
    }
    let replaced = node(context, replaced_handle)?;
    let replaced_pointer = replaced.pointer();
    if !Rc::ptr_eq(&parent_graph, &replaced.document())
        || crate::native::node_parent(replaced_pointer) != Some(parent_pointer)
    {
        return Ok(not_found());
    }
    if child_handle == replaced_handle {
        return Ok(DispatchResult::bridge_handle(replaced_handle));
    }
    if let Err(exception) =
        adopt_direct_legacy_node(context, child_handle, &parent_graph)
    {
        return Ok(exception);
    }
    let child = node(context, child_handle)?;
    let child_pointer = child.pointer();
    let child_graph = child.document();
    if let Err(exception) = validate_insertion(
        parent_pointer,
        &parent_graph,
        is_document,
        child_pointer,
        &child_graph,
        Some(replaced_pointer),
    ) {
        return Ok(exception);
    }
    let detached =
        crate::native::node_replace_child(parent_pointer, child_pointer, replaced_pointer)
            .ok_or(())?;
    if detached != replaced_pointer {
        return Err(());
    }
    context.attach_detached_root(child_pointer);
    context.register_detached_root(replaced_pointer, parent_graph);
    Ok(DispatchResult::bridge_handle(replaced_handle))
}

/// Reports whether one document or node receiver currently has children.
pub(super) fn has_child_nodes(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(
        crate::native::node_has_children(pointer),
    ))
}

/// Reports native node identity for one DOM wrapper.
pub(super) fn is_same(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let other_handle = request.bridge_handle(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let (other_pointer, _, _) =
        receiver_pointer_and_graph(context, other_handle)?;
    Ok(DispatchResult::boolean(pointer == other_pointer))
}

/// Reports PHP's structural node equality, including family-specific namespace rules.
pub(super) fn is_equal(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let Some(other_handle) = request.optional_bridge_handle(0)? else {
        return Ok(DispatchResult::boolean(false));
    };
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let (other_pointer, _, _) =
        receiver_pointer_and_graph(context, other_handle)?;
    Ok(DispatchResult::boolean(crate::native::node_is_equal(
        pointer,
        other_pointer,
        graph.family() != DocumentFamily::Legacy,
    )))
}

/// Returns PHP's document-position relation bitmask for two DOM nodes.
pub(super) fn compare_document_position(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let other_handle = request.bridge_handle(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let (other_pointer, _, _) =
        receiver_pointer_and_graph(context, other_handle)?;
    Ok(DispatchResult::integer(
        crate::native::node_compare_document_position(pointer, other_pointer)
            .ok_or(())?,
    ))
}

/// Reports whether the receiver is an inclusive ancestor of another DOM wrapper.
pub(super) fn contains(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let other_handle = request.bridge_handle(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let (other_pointer, _, _) =
        receiver_pointer_and_graph(context, other_handle)?;
    Ok(DispatchResult::boolean(crate::native::node_contains(
        pointer,
        other_pointer,
    )))
}

/// Returns the canonical wrapper for one receiver's topmost ancestor.
pub(super) fn root(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let root = crate::native::node_root(pointer).ok_or(())?;
    canonical_pointer_result(context, root, graph)
}

/// Returns one document or node receiver's PHP-compatible node name.
pub(super) fn name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(node_name(&graph, pointer)?))
}

/// Returns one document or node receiver's numeric DOM node type.
pub(super) fn node_type(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let value = crate::native::node_type(pointer);
    if value == 0 {
        return Err(());
    }
    Ok(DispatchResult::integer(i64::from(value)))
}

/// Returns recursively concatenated text content for one document or node receiver.
///
/// Synthesized DTD notation nodes have no libxml2 content allocation, but PHP exposes
/// their text content as an empty string rather than failing the property read.
pub(super) fn text_content(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let node_type = crate::native::node_type(pointer);
    if node_type == 12
        || (node_type == 17 && graph.family() == DocumentFamily::Legacy)
    {
        return Ok(DispatchResult::bytes(Vec::new()));
    }
    if graph.family() != DocumentFamily::Legacy
        && !matches!(node_type, 1 | 2 | 3 | 4 | 7 | 8 | 11)
    {
        return Ok(DispatchResult::null());
    }
    Ok(DispatchResult::bytes(
        crate::native::node_content(pointer).ok_or(())?,
    ))
}

/// Returns a node's canonical owner-document handle or null for a document receiver.
pub(super) fn owner_document(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    if let Ok(node) = node(context, request.header.receiver) {
        if !node.owner_document_exposed() {
            return Ok(DispatchResult::null());
        }
    }
    let (_, graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if is_document {
        return Ok(DispatchResult::null());
    }
    let pointer = graph.pointer();
    let handle = canonical_document_handle(context, Rc::clone(&graph));
    Ok(DispatchResult::typed_bridge_handle(
        handle,
        wrapper_kind(&graph, pointer),
    ))
}

/// Returns a node's canonical parent wrapper, including its document, or null.
pub(super) fn parent(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if is_document {
        return Ok(DispatchResult::null());
    }
    let Some(parent) = crate::native::node_parent(pointer) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, parent, graph)
}

/// Returns a node's canonical parent element wrapper or null.
pub(super) fn parent_element(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let Some(parent) = crate::native::node_parent_element(pointer) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, parent, graph)
}

/// Returns one canonical relative-node wrapper selected by a native pointer accessor.
pub(super) fn relative(
    context: &mut Context,
    request: &Request,
    accessor: fn(usize) -> Option<usize>,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let Some(relative) = accessor(pointer) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, relative, graph)
}

/// Reports whether one document or node receiver has a document ancestor.
pub(super) fn is_connected(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(
        crate::native::node_is_connected(pointer),
    ))
}

/// Normalizes descendant exclusive text nodes with legacy or modern recursion rules.
pub(super) fn normalize(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let legacy = graph.family() == DocumentFamily::Legacy;
    normalize_children(context, pointer, &graph, legacy)?;
    Ok(DispatchResult::null())
}

/// Recursively merges adjacent text nodes and detaches empty or merged wrappers.
fn normalize_children(
    context: &mut Context,
    parent: usize,
    graph: &Rc<crate::objects::DocumentGraph>,
    legacy: bool,
) -> Result<(), ()> {
    let mut child = crate::native::node_first_child(parent);
    while let Some(pointer) = child {
        let next = crate::native::node_next_sibling(pointer);
        match crate::native::node_type(pointer) {
            3 => {
                let mut content =
                    crate::native::node_content(pointer).ok_or(())?;
                if content.is_empty() {
                    detach_normalized_node(
                        context,
                        parent,
                        pointer,
                        graph,
                    )?;
                    child = next;
                    continue;
                }
                let mut adjacent = next;
                while let Some(candidate) = adjacent {
                    if crate::native::node_type(candidate) != 3 {
                        break;
                    }
                    let after =
                        crate::native::node_next_sibling(candidate);
                    content.extend(
                        crate::native::node_content(candidate).ok_or(())?,
                    );
                    detach_normalized_node(
                        context,
                        parent,
                        candidate,
                        graph,
                    )?;
                    adjacent = after;
                }
                if !crate::native::node_set_content(pointer, &content) {
                    return Err(());
                }
                child = adjacent;
                continue;
            }
            1 => {
                normalize_children(context, pointer, graph, legacy)?;
                if legacy {
                    let count =
                        crate::native::element_attribute_count(pointer);
                    for index in 0..count {
                        let attribute =
                            crate::native::element_attribute_at(pointer, index)
                                .ok_or(())?;
                        normalize_children(
                            context,
                            attribute,
                            graph,
                            legacy,
                        )?;
                    }
                }
            }
            _ => {}
        }
        child = next;
    }
    Ok(())
}

/// Detaches one normalized-away text node while preserving live wrapper usability.
fn detach_normalized_node(
    context: &mut Context,
    parent: usize,
    pointer: usize,
    graph: &Rc<crate::objects::DocumentGraph>,
) -> Result<(), ()> {
    if !crate::native::node_unlink_child(parent, pointer) {
        return Err(());
    }
    context.register_detached_root(pointer, Rc::clone(graph));
    Ok(())
}

/// Adopts one directly constructed legacy node into its first public owner document.
pub(super) fn adopt_direct_legacy_node(
    context: &mut Context,
    handle: u64,
    target: &Rc<crate::objects::DocumentGraph>,
) -> Result<(), DispatchResult> {
    let candidate = node(context, handle).map_err(|_| dom_exception(11))?;
    let pointer = candidate.pointer();
    let source = candidate.document();
    if candidate.owner_document_exposed() || Rc::ptr_eq(target, &source) {
        return Ok(());
    }
    if target.family() != DocumentFamily::Legacy {
        return Err(wrong_document());
    }
    let outcome =
        crate::native::document_adopt_node(target.pointer(), pointer, false);
    if outcome.error_code != 0 {
        return Err(dom_exception(outcome.error_code));
    }
    if outcome.pointer != Some(pointer) {
        return Err(dom_exception(11));
    }
    rehome_subtree_handles(context, pointer, Rc::clone(target))
        .map_err(|_| dom_exception(11))
}

/// Validates document ownership, cycle safety, and modern single-root rules before mutation.
fn validate_insertion(
    parent_pointer: usize,
    parent_graph: &Rc<crate::objects::DocumentGraph>,
    is_document: bool,
    child_pointer: usize,
    child_graph: &Rc<crate::objects::DocumentGraph>,
    replaced_pointer: Option<usize>,
) -> Result<(), DispatchResult> {
    if !Rc::ptr_eq(parent_graph, child_graph) {
        return Err(wrong_document());
    }
    if crate::native::node_contains(child_pointer, parent_pointer) {
        return Err(hierarchy_request(b"Hierarchy Request Error"));
    }
    if is_document
        && parent_graph.family() != DocumentFamily::Legacy
        && crate::native::node_type(child_pointer) == 1
        && crate::native::document_element(parent_pointer)
            .is_some_and(|element| {
                element != child_pointer && Some(element) != replaced_pointer
            })
    {
        return Err(hierarchy_request(
            b"Cannot have more than one element child in a document",
        ));
    }
    Ok(())
}

/// DTD parent operations whose php-src results precede ordinary tree validation.
enum DtdMutation {
    AppendOrInsert,
    Replace,
}

/// Returns PHP's declaration-node mutation result before touching libxml2 layout fields.
fn dtd_mutation_result(
    family: DocumentFamily,
    node_type: u32,
    operation: DtdMutation,
) -> Option<DispatchResult> {
    if !matches!(node_type, 12 | 17) {
        return None;
    }
    Some(match (family, node_type, operation) {
        (DocumentFamily::Legacy, 17, DtdMutation::AppendOrInsert) => {
            DispatchResult::dom_exception(7, b"No Modification Allowed Error")
        }
        (DocumentFamily::Legacy, 12, DtdMutation::AppendOrInsert)
        | (DocumentFamily::Legacy, 17, DtdMutation::Replace) => {
            DispatchResult::boolean(false)
        }
        (
            DocumentFamily::ModernXml | DocumentFamily::ModernHtml,
            12 | 17,
            DtdMutation::AppendOrInsert,
        )
        | (
            DocumentFamily::ModernXml | DocumentFamily::ModernHtml,
            17,
            DtdMutation::Replace,
        ) => hierarchy_request(b"Hierarchy Request Error"),
        (
            DocumentFamily::Legacy
            | DocumentFamily::ModernXml
            | DocumentFamily::ModernHtml,
            12,
            DtdMutation::Replace,
        ) => wrong_document(),
        _ => return None,
    })
}

/// Builds PHP's `HIERARCHY_REQUEST_ERR` result with one operation-specific message.
fn hierarchy_request(message: &[u8]) -> DispatchResult {
    DispatchResult::dom_exception(3, message)
}

/// Builds PHP's canonical `WRONG_DOCUMENT_ERR` result.
fn wrong_document() -> DispatchResult {
    DispatchResult::dom_exception(4, b"Wrong Document Error")
}

/// Builds PHP's canonical `NOT_FOUND_ERR` result.
fn not_found() -> DispatchResult {
    DispatchResult::dom_exception(8, b"Not Found Error")
}
