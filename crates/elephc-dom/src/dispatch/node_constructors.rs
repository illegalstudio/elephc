//! Purpose:
//! Constructs legacy DOM nodes that PHP permits independently of a public document wrapper.
//! Retains one private libxml2 owner graph while keeping `ownerDocument` observably null.
//!
//! Called from:
//! - `super::routes::dispatch()` for public legacy node constructors.
//!
//! Key details:
//! - Directly constructed nodes become ordinary owner-document nodes after later adoption.
//! - Invalid XML names and namespaces use the same structured DOMException channel as factories.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{DocumentFamily, DocumentGraph, DocumentObject};
use crate::request::Request;

use super::{
    direct_node_result, dom_exception, optional_bytes, require_no_values,
    DispatchResult,
};

/// Constructs one standalone legacy element with optional content and namespace.
pub(super) fn element(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if !(1..=3).contains(&request.values.len()) {
        return Err(());
    }
    let qualified_name = request.byte_string(0)?;
    let value = if request.values.len() > 1 {
        request.optional_byte_string(1)?
    } else {
        None
    };
    let namespace = optional_bytes(request, 2, b"")?;
    if namespace.is_empty() && qualified_name.contains(&b':') {
        return Ok(dom_exception(14));
    }
    let graph = private_legacy_graph()?;
    let pointer = if namespace.is_empty() {
        crate::native::document_create_element(
            graph.pointer(),
            qualified_name,
            value,
            false,
        )
        .ok_or_else(|| dom_exception(5))
    } else {
        let outcome = crate::native::document_create_element_ns(
            graph.pointer(),
            Some(namespace),
            qualified_name,
            value,
            false,
        );
        if outcome.error_code != 0 {
            Err(dom_exception(outcome.error_code))
        } else {
            outcome.pointer.ok_or_else(|| dom_exception(5))
        }
    };
    match pointer {
        Ok(pointer) => finish(context, request, pointer, graph),
        Err(exception) => Ok(exception),
    }
}

/// Constructs one standalone legacy attribute with an optional string value.
pub(super) fn attribute(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if !(1..=2).contains(&request.values.len()) {
        return Err(());
    }
    let graph = private_legacy_graph()?;
    let Some(pointer) =
        crate::native::document_create_attribute(graph.pointer(), request.byte_string(0)?)
    else {
        return Ok(dom_exception(5));
    };
    let value = optional_bytes(request, 1, b"")?;
    if !crate::native::node_set_content(pointer, value) {
        unsafe {
            crate::native::node_free(pointer);
        }
        return Err(());
    }
    finish(context, request, pointer, graph)
}

/// Constructs one standalone legacy text node with optional content.
pub(super) fn text(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    construct_character_data(
        context,
        request,
        0,
        |document, value| crate::native::document_create_text(document, value),
    )
}

/// Constructs one standalone legacy comment with optional content.
pub(super) fn comment(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    construct_character_data(
        context,
        request,
        0,
        |document, value| crate::native::document_create_comment(document, value),
    )
}

/// Constructs one standalone legacy CDATA section with required content.
pub(super) fn cdata(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    construct_character_data(
        context,
        request,
        1,
        |document, value| crate::native::document_create_cdata(document, value),
    )
}

/// Constructs one standalone legacy document fragment.
pub(super) fn fragment(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let graph = private_legacy_graph()?;
    let pointer =
        crate::native::document_create_fragment(graph.pointer()).ok_or(())?;
    finish(context, request, pointer, graph)
}

/// Constructs one standalone legacy processing instruction.
pub(super) fn processing_instruction(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if !(1..=2).contains(&request.values.len()) {
        return Err(());
    }
    let graph = private_legacy_graph()?;
    let pointer = crate::native::document_create_pi(
        graph.pointer(),
        request.byte_string(0)?,
        optional_bytes(request, 1, b"")?,
    );
    match pointer {
        Some(pointer) => finish(context, request, pointer, graph),
        None => Ok(dom_exception(5)),
    }
}

/// Constructs one standalone legacy entity-reference node.
pub(super) fn entity_reference(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let graph = private_legacy_graph()?;
    let pointer = crate::native::document_create_entity_reference(
        graph.pointer(),
        request.byte_string(0)?,
    );
    match pointer {
        Some(pointer) => finish(context, request, pointer, graph),
        None => Ok(dom_exception(5)),
    }
}

/// Constructs one standalone character-data node through a selected native factory.
fn construct_character_data(
    context: &mut Context,
    request: &Request,
    required_parameters: usize,
    factory: impl FnOnce(usize, &[u8]) -> Option<usize>,
) -> Result<DispatchResult, ()> {
    if request.values.len() < required_parameters || request.values.len() > 1 {
        return Err(());
    }
    let value = optional_bytes(request, 0, b"")?;
    let graph = private_legacy_graph()?;
    let pointer = factory(graph.pointer(), value).ok_or(())?;
    finish(context, request, pointer, graph)
}

/// Returns a new wrapper handle or rebinds a manual-constructor receiver in place.
fn finish(
    context: &mut Context,
    request: &Request,
    pointer: usize,
    graph: Rc<DocumentGraph>,
) -> Result<DispatchResult, ()> {
    if request.header.receiver == 0 {
        return Ok(direct_node_result(context, pointer, graph));
    }
    context.reconstruct_direct_node(
        request.header.receiver,
        pointer,
        graph,
    )?;
    Ok(DispatchResult::null())
}

/// Allocates one private legacy document graph used only to own a direct node.
fn private_legacy_graph() -> Result<Rc<DocumentGraph>, ()> {
    let pointer = crate::native::document_new(b"1.0", b"").ok_or(())?;
    Ok(DocumentObject::new(pointer, DocumentFamily::Legacy).graph())
}
