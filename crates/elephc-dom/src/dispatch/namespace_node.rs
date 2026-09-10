//! Purpose:
//! Dispatches the ten generated `DOMNameSpaceNode` virtual-property reads and the
//! `__sleep`/`__wakeup` serialization rejections for standalone legacy XPath
//! namespace-declaration wrappers.
//!
//! Called from:
//! - `super::routes::dispatch()` for the `__sleep`/`__wakeup` methods.
//! - `super::routes::properties::dispatch()` for the `property-get` reads.
//!
//! Key details:
//! - Receivers are `HANDLE_NAMESPACE_NODE` handles, never document or node handles,
//!   so every read resolves through `super::namespace_node` instead of the node
//!   receiver helpers.
//! - The fake `xmlNode` keeps type `XML_NAMESPACE_DECL`, so nodeName/nodeValue and
//!   localName use dedicated native helpers; prefix, namespaceURI, nodeType,
//!   isConnected, parentNode, and parentElement reuse the shared node accessors.
//! - PHP 8.5 rejects serialization with a base `Exception`, mirroring `DOMNode`.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{
    NativeObject, NamespaceNodeAllocation, NamespaceNodeObject,
    HANDLE_NAMESPACE_NODE,
};
use crate::request::Request;

use super::{
    canonical_document_handle, canonical_pointer_result, namespace_node,
    require_no_values, wrapper_kind, DispatchResult,
};

/// Returns one namespace-declaration wrapper's PHP `nodeName`.
pub(super) fn name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        crate::native::namespace_node_name(namespace_node.pointer())
            .ok_or(())?,
    ))
}

/// Returns one namespace-declaration wrapper's PHP `nodeValue` (the namespace URI).
pub(super) fn node_value(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    match crate::native::namespace_node_value(namespace_node.pointer()) {
        Some(value) => Ok(DispatchResult::bytes(value)),
        None => Ok(DispatchResult::null()),
    }
}

/// Returns one namespace-declaration wrapper's numeric DOM node type (18).
pub(super) fn node_type(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    let value = crate::native::node_type(namespace_node.pointer());
    if value == 0 {
        return Err(());
    }
    Ok(DispatchResult::integer(i64::from(value)))
}

/// Returns one namespace-declaration wrapper's prefix, with PHP's empty fallback.
pub(super) fn prefix(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        crate::native::node_prefix(namespace_node.pointer()).unwrap_or_default(),
    ))
}

/// Returns one namespace-declaration wrapper's local name or PHP null.
pub(super) fn local_name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    match crate::native::namespace_node_local_name(namespace_node.pointer()) {
        Some(value) => Ok(DispatchResult::bytes(value)),
        None => Ok(DispatchResult::null()),
    }
}

/// Returns one namespace-declaration wrapper's namespace URI or PHP null.
pub(super) fn namespace_uri(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    match crate::native::node_namespace_uri(namespace_node.pointer()) {
        Some(value) => Ok(DispatchResult::bytes(value)),
        None => Ok(DispatchResult::null()),
    }
}

/// Reports whether one namespace-declaration wrapper stays connected to a document.
pub(super) fn is_connected(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(
        crate::native::node_is_connected(namespace_node.pointer()),
    ))
}

/// Returns one namespace-declaration wrapper's canonical owner document.
pub(super) fn owner_document(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let graph = namespace_node(context, request.header.receiver)?.document();
    let pointer = graph.pointer();
    let kind = wrapper_kind(&graph, pointer);
    let handle = canonical_document_handle(context, graph);
    Ok(DispatchResult::typed_bridge_handle(handle, kind))
}

/// Returns one namespace-declaration wrapper's canonical parent node or null.
pub(super) fn parent_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph) =
        namespace_node(context, request.header.receiver)
            .map(|namespace_node| {
                (
                    namespace_node.pointer(),
                    namespace_node.document(),
                )
            })?;
    let Some(parent) = crate::native::node_parent(pointer) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, parent, graph)
}

/// Returns one namespace-declaration wrapper's canonical parent element or null.
pub(super) fn parent_element(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph) =
        namespace_node(context, request.header.receiver)
            .map(|namespace_node| {
                (
                    namespace_node.pointer(),
                    namespace_node.document(),
                )
            })?;
    let Some(parent) = crate::native::node_parent_element(pointer) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, parent, graph)
}

/// Rejects serialization of a `DOMNameSpaceNode` with PHP's exact base exception.
pub(super) fn sleep(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let _ = namespace_node(context, request.header.receiver)?;
    Ok(DispatchResult::exception(
        b"Serialization of 'DOMNameSpaceNode' is not allowed, unless serialization methods are implemented in a subclass",
    ))
}

/// Rejects unserialization of a `DOMNameSpaceNode` with PHP's exact base exception.
pub(super) fn wakeup(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let _ = namespace_node(context, request.header.receiver)?;
    Ok(DispatchResult::exception(
        b"Unserialization of 'DOMNameSpaceNode' is not allowed, unless unserialization methods are implemented in a subclass",
    ))
}

/// Clones one namespace-declaration wrapper into a fresh standalone fake node.
///
/// PHP object cloning produces an independent `DOMNameSpaceNode` backed by a new
/// fake namespace-declaration allocation with the same parent element and binding.
/// The new wrapper owns the new fake node, so releasing either wrapper frees only
/// its own allocation.
pub(super) fn clone_object(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let namespace_node = namespace_node(context, request.header.receiver)?;
    let pointer = namespace_node.pointer();
    let parent = namespace_node.parent();
    let graph = namespace_node.document();
    let clone = crate::native::namespace_node_clone(pointer).ok_or(())?;
    let allocation = Rc::new(NamespaceNodeAllocation::new(clone, parent));
    let handle = context.native_objects.insert(
        HANDLE_NAMESPACE_NODE,
        NativeObject::NamespaceNode(NamespaceNodeObject::new(allocation, graph)),
    );
    context.namespace_node_handles.insert(clone, handle);
    Ok(DispatchResult::typed_bridge_handle(handle, 118))
}
