//! Purpose:
//! Dispatches modern document encoding, head, body, and title virtual properties.
//! Mirrors PHP's HTML-document algorithms over either modern document class.
//!
//! Called from:
//! - `super::routes::properties::dispatch()` for `Dom\Document` metadata.
//!
//! Key details:
//! - Body replacement auto-adopts nodes and preserves every materialized wrapper.
//! - Title reads collapse direct text while writes retain the supplied whitespace.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::DocumentFamily;
use crate::request::Request;

use super::{
    canonical_pointer_result, document, receiver_pointer_and_graph,
    rehome_subtree_handles, require_no_values, DispatchResult,
};

/// Returns one modern document's direct HTML head element or PHP null.
pub(super) fn head(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = modern_document(context, request.header.receiver)?;
    let pointer = target.pointer();
    let graph = target.graph();
    match crate::native::document_head(pointer) {
        Some(head) => canonical_pointer_result(context, head, graph),
        None => Ok(DispatchResult::null()),
    }
}

/// Returns one modern document's direct HTML body or frameset element.
pub(super) fn body(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = modern_document(context, request.header.receiver)?;
    let pointer = target.pointer();
    let graph = target.graph();
    match crate::native::document_body(pointer) {
        Some(body) => canonical_pointer_result(context, body, graph),
        None => Ok(DispatchResult::null()),
    }
}

/// Canonicalizes and replaces all three modern document encoding aliases.
pub(super) fn set_encoding(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let encoding = request.byte_string(0)?;
    let target = modern_document(context, request.header.receiver)?;
    match crate::native::document_set_modern_encoding(
        target.pointer(),
        encoding,
    ) {
        1 => Ok(DispatchResult::null()),
        -1 => Ok(DispatchResult::value_error(
            b"Invalid document encoding",
        )),
        _ => Err(()),
    }
}

/// Replaces one modern document's body, auto-adopting it like php-src.
pub(super) fn set_body(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let Some(source_handle) = request.optional_bridge_handle(0)? else {
        return Ok(invalid_body());
    };
    let (source, _, source_is_document) =
        receiver_pointer_and_graph(context, source_handle)?;
    if source_is_document || crate::native::node_type(source) != 1 {
        return Ok(invalid_body());
    }
    if !crate::native::node_is_html_element(source) {
        return Ok(DispatchResult::error(
            b"Cannot assign Dom\\Element to property Dom\\Document::$body of type ?Dom\\HTMLElement",
        ));
    }
    if !crate::native::node_is_html_body(source) {
        return Ok(invalid_body());
    }

    let target = modern_document(context, request.header.receiver)?;
    let target_pointer = target.pointer();
    let target_graph = target.graph();
    let current_body = crate::native::document_body(target_pointer);
    if current_body == Some(source) {
        return Ok(DispatchResult::null());
    }
    let Some(root) = crate::native::document_element(target_pointer) else {
        return Ok(DispatchResult::dom_exception(
            3,
            b"A body can only be set if there is a document element",
        ));
    };

    let outcome =
        crate::native::document_adopt_node(target_pointer, source, true);
    if outcome.pointer != Some(source) {
        return match outcome.error_code {
            11 => Ok(super::dom_exception(11)),
            _ => Err(()),
        };
    }
    rehome_subtree_handles(context, source, Rc::clone(&target_graph))?;
    context.attach_detached_root(source);
    context.register_detached_root(source, Rc::clone(&target_graph));

    if let Some(current) = current_body {
        let replaced =
            crate::native::node_replace_child(root, source, current)
                .ok_or(())?;
        if replaced != current {
            return Err(());
        }
        context.attach_detached_root(source);
        context.register_detached_root(current, target_graph);
    } else {
        let inserted =
            crate::native::node_append_child(root, source).ok_or(())?;
        if inserted != source {
            return Err(());
        }
        context.attach_detached_root(source);
    }
    Ok(DispatchResult::null())
}

/// Returns one modern document's collapsed HTML or SVG title.
pub(super) fn title(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = modern_document(context, request.header.receiver)?;
    let value =
        crate::native::document_title(target.pointer()).ok_or(())?;
    Ok(DispatchResult::bytes(value))
}

/// Replaces or creates one modern document's HTML or SVG title text.
pub(super) fn set_title(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.byte_string(0)?;
    let target = modern_document(context, request.header.receiver)?;
    let pointer = target.pointer();
    let graph = target.graph();
    if let Some(title) = crate::native::document_title_element(pointer) {
        detach_title_children(context, title, &graph)?;
    }
    if !crate::native::document_set_title(pointer, value) {
        return Ok(super::dom_exception(11));
    }
    Ok(DispatchResult::null())
}

/// Detaches replaced title children so previously materialized wrappers remain live.
fn detach_title_children(
    context: &mut Context,
    title: usize,
    graph: &Rc<crate::objects::DocumentGraph>,
) -> Result<(), ()> {
    let mut child = crate::native::node_first_child(title);
    while let Some(pointer) = child {
        child = crate::native::node_next_sibling(pointer);
        if !crate::native::node_unlink_child(title, pointer) {
            return Err(());
        }
        if !context.detached_roots.contains_key(&pointer) {
            context.register_detached_root(pointer, Rc::clone(graph));
        }
    }
    Ok(())
}

/// Borrows a receiver and rejects the legacy document family.
fn modern_document(
    context: &Context,
    handle: u64,
) -> Result<&crate::objects::DocumentObject, ()> {
    let target = document(context, handle)?;
    if target.family() == DocumentFamily::Legacy {
        return Err(());
    }
    Ok(target)
}

/// Builds PHP's exact invalid-body hierarchy exception.
fn invalid_body() -> DispatchResult {
    DispatchResult::dom_exception(
        3,
        b"The new body must either be a body or a frameset tag",
    )
}
