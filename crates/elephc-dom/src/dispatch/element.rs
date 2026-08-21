//! Purpose:
//! Dispatches core DOM element attributes and element-only navigation properties.
//! Preserves legacy empty-string/false results versus modern nullable/void results.
//!
//! Called from:
//! - `super::dispatch()` for legacy and modern element operations.
//!
//! Key details:
//! - Attribute nodes materialize through the shared canonical wrapper cache.
//! - Element traversal skips comments, text, and processing instructions.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{DocumentFamily, LegacyDocumentFlag};
use crate::request::Request;

use super::{
    canonical_pointer_result, dom_exception, node,
    node::adopt_direct_legacy_node, receiver_pointer_and_graph,
    rehome_node_handle, require_no_values, DispatchResult,
};

/// Returns PHP's unimplemented always-null schema type information placeholder.
pub(super) fn schema_type_info(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = node(context, request.header.receiver)?;
    if !matches!(crate::native::node_type(target.pointer()), 1 | 2) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Returns one attribute value with family-specific missing-attribute semantics.
pub(super) fn get_attribute(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(match crate::native::element_get_attribute(pointer, name) {
        Some(value) => DispatchResult::bytes(value),
        None if graph.family() == DocumentFamily::Legacy => {
            DispatchResult::bytes(Vec::new())
        }
        None => DispatchResult::null(),
    })
}

/// Reports whether one element has an attribute by qualified name.
pub(super) fn has_attribute(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(
        crate::native::element_get_attribute_node(pointer, name).is_some(),
    ))
}

/// Reports whether one element owns an attribute selected by namespace and local name.
pub(super) fn has_attribute_ns(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(
        crate::native::element_get_attribute_node_ns(
            pointer,
            request.optional_byte_string(0)?,
            request.byte_string(1)?,
        )
        .is_some(),
    ))
}

/// Returns one canonical attribute wrapper or the family's missing result.
pub(super) fn get_attribute_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let Some(attribute) = crate::native::element_get_attribute_node(pointer, name) else {
        return Ok(if graph.family() == DocumentFamily::Legacy {
            DispatchResult::boolean(false)
        } else {
            DispatchResult::null()
        });
    };
    canonical_pointer_result(context, attribute, graph)
}

/// Returns all qualified attribute names in PHP's family-specific namespace order.
pub(super) fn get_attribute_names(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let names = crate::native::element_attribute_names(
        pointer,
        graph.family() == DocumentFamily::Legacy,
    )
    .ok_or(())?;
    Ok(DispatchResult::byte_strings(names))
}

/// Returns PHP's ordered in-scope namespace information for one modern element.
pub(super) fn get_in_scope_namespaces(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    namespace_infos(context, request, false)
}

/// Returns in-scope namespace information for one element and all descendants.
pub(super) fn get_descendant_namespaces(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    namespace_infos(context, request, true)
}

/// Collects and materializes modern namespace-info value objects.
fn namespace_infos(
    context: &mut Context,
    request: &Request,
    include_descendants: bool,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if graph.family() == DocumentFamily::Legacy
        || crate::native::node_type(pointer) != 1
    {
        return Err(());
    }
    let infos =
        crate::native::element_namespace_info(pointer, include_descendants)?;
    DispatchResult::namespace_infos(context, graph, infos)
}

/// Returns one namespaced attribute value with family-specific missing semantics.
pub(super) fn get_attribute_ns(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(match crate::native::element_get_attribute_ns(
        pointer,
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
    ) {
        Some(value) => DispatchResult::bytes(value),
        None if graph.family() == DocumentFamily::Legacy => {
            DispatchResult::bytes(Vec::new())
        }
        None => DispatchResult::null(),
    })
}

/// Returns one canonical namespaced attribute wrapper or PHP null.
pub(super) fn get_attribute_node_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let Some(attribute) = crate::native::element_get_attribute_node_ns(
        pointer,
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
    ) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, attribute, graph)
}

/// Creates or updates one attribute and returns PHP's family-specific result.
pub(super) fn set_attribute(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let value = request.byte_string(1)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let attribute =
        crate::native::element_set_attribute(pointer, name, value).ok_or(())?;
    if graph.family() == DocumentFamily::Legacy {
        canonical_pointer_result(context, attribute, graph)
    } else {
        Ok(DispatchResult::null())
    }
}

/// Creates or updates one namespaced attribute with exact QName exceptions.
pub(super) fn set_attribute_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 3 {
        return Err(());
    }
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let outcome = crate::native::element_set_attribute_ns(
        pointer,
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
        request.byte_string(2)?,
        graph.family() != DocumentFamily::Legacy,
    );
    if outcome.error_code != 0 {
        return Ok(dom_exception(outcome.error_code));
    }
    outcome.pointer.ok_or(())?;
    Ok(DispatchResult::null())
}

/// Removes one attribute and returns PHP's legacy boolean or modern void result.
pub(super) fn remove_attribute(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let attribute = crate::native::element_remove_attribute(pointer, name);
    if let Some(attribute) = attribute {
        context.register_detached_root(attribute, graph.clone());
    }
    Ok(if graph.family() == DocumentFamily::Legacy {
        DispatchResult::boolean(attribute.is_some())
    } else {
        DispatchResult::null()
    })
}

/// Removes one namespaced attribute or legacy namespace declaration.
pub(super) fn remove_attribute_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Some(attribute) = crate::native::element_remove_attribute_ns(
        pointer,
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
        graph.family() == DocumentFamily::Legacy,
    ) {
        context.register_detached_root(attribute, graph);
    }
    Ok(DispatchResult::null())
}

/// Adds, removes, or preserves one attribute according to nullable force semantics.
pub(super) fn toggle_attribute(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if !(1..=2).contains(&request.values.len()) {
        return Err(());
    }
    let name = request.byte_string(0)?;
    if !crate::native::validate_name(name) {
        return Ok(dom_exception(5));
    }
    let force = if request.values.len() == 2 {
        request.optional_boolean(1)?
    } else {
        None
    };
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let existing = crate::native::element_get_attribute_node(pointer, name);
    if existing.is_none() {
        if force == Some(false) {
            return Ok(DispatchResult::boolean(false));
        }
        crate::native::element_set_attribute(pointer, name, b"").ok_or(())?;
        return Ok(DispatchResult::boolean(true));
    }
    if force == Some(true) {
        return Ok(DispatchResult::boolean(true));
    }
    let attribute = crate::native::element_remove_attribute(pointer, name).ok_or(())?;
    context.register_detached_root(attribute, graph);
    Ok(DispatchResult::boolean(false))
}

/// Updates an attribute selected by qualified or namespace-aware name as an XML ID.
pub(super) fn set_id_attribute(
    context: &Context,
    request: &Request,
    use_namespace: bool,
) -> Result<DispatchResult, ()> {
    let expected = if use_namespace { 3 } else { 2 };
    if request.values.len() != expected {
        return Err(());
    }
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let (attribute, is_id) = if use_namespace {
        (
            crate::native::element_get_attribute_node_ns(
                pointer,
                request.optional_byte_string(0)?,
                request.byte_string(1)?,
            ),
            request.boolean(2)?,
        )
    } else {
        (
            crate::native::element_get_attribute_node(
                pointer,
                request.byte_string(0)?,
            ),
            request.boolean(1)?,
        )
    };
    let Some(attribute) = attribute else {
        return Ok(id_attribute_not_found(&graph));
    };
    if !crate::native::attribute_set_is_id(attribute, is_id) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Updates one exact attached attribute wrapper as an XML ID.
pub(super) fn set_id_attribute_node(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let attribute_handle = request.bridge_handle(0)?;
    let is_id = request.boolean(1)?;
    let (element_pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let attribute = node(context, attribute_handle)?;
    let attribute_pointer = attribute.pointer();
    if crate::native::node_type(attribute_pointer) != 2
        || crate::native::node_parent(attribute_pointer)
            != Some(element_pointer)
    {
        return Ok(id_attribute_not_found(&graph));
    }
    if !crate::native::attribute_set_is_id(attribute_pointer, is_id) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Attaches one attribute node, returning the replaced wrapper or PHP null.
pub(super) fn set_attribute_node(
    context: &mut Context,
    request: &Request,
    use_namespace: bool,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let attribute_handle = request.bridge_handle(0)?;
    let (element_pointer, element_graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if let Err(exception) =
        adopt_direct_legacy_node(context, attribute_handle, &element_graph)
    {
        return Ok(exception);
    }
    let attribute = node(context, attribute_handle)?;
    let attribute_pointer = attribute.pointer();
    let attribute_graph = attribute.document();
    if crate::native::node_type(attribute_pointer) != 2 {
        return Err(());
    }
    let modern = element_graph.family() != DocumentFamily::Legacy;
    let attribute_parent = crate::native::node_parent(attribute_pointer);
    if modern
        && attribute_parent.is_some()
        && attribute_parent != Some(element_pointer)
    {
        return Ok(dom_exception(10));
    }
    if !modern && !Rc::ptr_eq(&element_graph, &attribute_graph) {
        return Ok(dom_exception(4));
    }
    if modern && !Rc::ptr_eq(&element_graph, &attribute_graph) {
        if !crate::native::attribute_adopt(
            attribute_pointer,
            element_graph.pointer(),
        ) {
            return Ok(dom_exception(11));
        }
        rehome_node_handle(
            context,
            attribute_handle,
            Rc::clone(&element_graph),
        )?;
    }
    let replaced = crate::native::element_set_attribute_node(
        element_pointer,
        attribute_pointer,
        use_namespace,
    );
    context.attach_detached_root(attribute_pointer);
    let Some(replaced) = replaced else {
        return Ok(DispatchResult::null());
    };
    context.register_detached_root(replaced, Rc::clone(&element_graph));
    canonical_pointer_result(context, replaced, element_graph)
}

/// Detaches one exact attribute wrapper or throws PHP's `NOT_FOUND_ERR`.
pub(super) fn remove_attribute_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let attribute_handle = request.bridge_handle(0)?;
    let (element_pointer, element_graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let attribute = node(context, attribute_handle)?;
    let attribute_pointer = attribute.pointer();
    if !Rc::ptr_eq(&element_graph, &attribute.document())
        || crate::native::node_parent(attribute_pointer) != Some(element_pointer)
    {
        return Ok(dom_exception(8));
    }
    if !crate::native::element_remove_attribute_node(
        element_pointer,
        attribute_pointer,
    ) {
        return Ok(dom_exception(8));
    }
    context.register_detached_root(attribute_pointer, element_graph);
    Ok(DispatchResult::bridge_handle(attribute_handle))
}

/// Maps a missing ID attribute through legacy strict-error or modern exception rules.
fn id_attribute_not_found(
    graph: &crate::objects::DocumentGraph,
) -> DispatchResult {
    if graph.family() == DocumentFamily::Legacy
        && !graph.legacy_flag(LegacyDocumentFlag::StrictErrorChecking)
    {
        DispatchResult::null()
    } else {
        dom_exception(8)
    }
}

/// Returns one element's qualified tag name.
pub(super) fn tag_name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(super::node_name(&graph, pointer)?))
}

/// Returns one element's `id` attribute with PHP's empty-string fallback.
pub(super) fn id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    attribute_property(context, request, b"id")
}

/// Returns one element's `class` attribute with PHP's empty-string fallback.
pub(super) fn class_name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    attribute_property(context, request, b"class")
}

/// Replaces one string-valued element property through its reflected attribute.
pub(super) fn set_attribute_property(
    context: &mut Context,
    request: &Request,
    name: &[u8],
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.byte_string(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    crate::native::element_set_attribute(pointer, name, value).ok_or(())?;
    Ok(DispatchResult::null())
}

/// Returns one canonical element-only relative wrapper or PHP null.
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

/// Counts one element's direct element children.
pub(super) fn child_count(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::integer(
        crate::native::element_child_count(pointer),
    ))
}

/// Returns one ordinary string-valued attribute property.
fn attribute_property(
    context: &Context,
    request: &Request,
    name: &[u8],
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        crate::native::element_get_attribute(pointer, name).unwrap_or_default(),
    ))
}
