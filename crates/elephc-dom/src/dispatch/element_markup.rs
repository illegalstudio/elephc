//! Purpose:
//! Implements modern element fragment serialization, parsing, and markup replacement.
//! Shares atomic XML/HTML fragment mutation across innerHTML, outerHTML, and adjacent HTML.
//!
//! Called from:
//! - `super::routes` and `super::routes::properties` for modern element markup surfaces.
//!
//! Key details:
//! - XML syntax failures leave the original tree untouched.
//! - Template inner mutations target the private content fragment while retaining wrapper identity.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{DocumentFamily, DocumentGraph};
use crate::request::Request;

use super::{
    receiver_pointer_and_graph, require_no_values, DispatchResult,
};

const HTML_NAMESPACE: &[u8] = b"http://www.w3.org/1999/xhtml";

/// One normalized adjacent insertion position.
#[derive(Clone, Copy)]
enum AdjacentPosition {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

/// Returns one element's serialized child markup.
pub(super) fn inner_html(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    serialize(context, request, true)
}

/// Returns one element's serialized markup including the element itself.
pub(super) fn outer_html(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    serialize(context, request, false)
}

/// Returns one modern element's libxml-substituted descendant text.
pub(super) fn substituted_node_value(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (element, _) = modern_element(context, request)?;
    let Some(content) = crate::native::node_content(element) else {
        return Ok(invalid_state());
    };
    Ok(DispatchResult::bytes(content))
}

/// Replaces ordinary children from one entity-substituting content string.
pub(super) fn set_substituted_node_value(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let content = request.byte_string(0)?;
    let (element, graph) = modern_element(context, request)?;
    detach_children(context, element, &graph)?;
    if !crate::native::node_set_content(element, content) {
        return Ok(invalid_state());
    }
    Ok(DispatchResult::null())
}

/// Replaces one element's child content after a successful context-fragment parse.
pub(super) fn set_inner_html(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let (element, graph) = modern_element(context, request)?;
    let fragment = match parse_fragment(
        element,
        request.byte_string(0)?,
        &graph,
    ) {
        Ok(fragment) => fragment,
        Err(exception) => return Ok(exception),
    };
    let Some(container) =
        crate::native::element_content_container(element, true)
    else {
        unsafe {
            crate::native::node_free(fragment);
        }
        return Ok(invalid_state());
    };
    detach_children(context, container, &graph)?;
    insert_fragment(container, None, fragment)?;
    Ok(DispatchResult::null())
}

/// Replaces one attached element with parsed fragment children.
pub(super) fn set_outer_html(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let (element, graph) = modern_element(context, request)?;
    let Some(parent) = crate::native::node_parent(element) else {
        return Ok(DispatchResult::null());
    };
    if matches!(crate::native::node_type(parent), 9 | 13) {
        return Ok(DispatchResult::dom_exception(
            13,
            b"Invalid Modification Error",
        ));
    }
    let (parse_context, temporary_context) =
        fragment_context(parent, &graph)?;
    let parsed = parse_fragment(
        parse_context,
        request.byte_string(0)?,
        &graph,
    );
    free_temporary_context(temporary_context);
    let fragment = match parsed {
        Ok(fragment) => fragment,
        Err(exception) => return Ok(exception),
    };
    insert_fragment(parent, Some(element), fragment)?;
    if !crate::native::node_unlink_child(parent, element) {
        return Err(());
    }
    context.register_detached_root(element, graph);
    Ok(DispatchResult::null())
}

/// Parses markup and inserts its nodes at one modern adjacent position.
pub(super) fn insert_adjacent_html(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (element, graph) = modern_element(context, request)?;
    let position = parse_position(request.byte_string(0)?).ok_or(())?;
    let (parent, reference, raw_context) = match position {
        AdjacentPosition::BeforeBegin => {
            let Some(parent) = crate::native::node_parent(element) else {
                return Ok(no_modification());
            };
            if matches!(crate::native::node_type(parent), 9 | 13) {
                return Ok(no_modification());
            }
            (parent, Some(element), parent)
        }
        AdjacentPosition::AfterBegin => (
            element,
            crate::native::node_first_child(element),
            element,
        ),
        AdjacentPosition::BeforeEnd => (element, None, element),
        AdjacentPosition::AfterEnd => {
            let Some(parent) = crate::native::node_parent(element) else {
                return Ok(no_modification());
            };
            if matches!(crate::native::node_type(parent), 9 | 13) {
                return Ok(no_modification());
            }
            (
                parent,
                crate::native::node_next_sibling(element),
                parent,
            )
        }
    };
    let (parse_context, temporary_context) =
        fragment_context(raw_context, &graph)?;
    let parsed = parse_fragment(
        parse_context,
        request.byte_string(1)?,
        &graph,
    );
    free_temporary_context(temporary_context);
    let fragment = match parsed {
        Ok(fragment) => fragment,
        Err(exception) => return Ok(exception),
    };
    insert_fragment(parent, reference, fragment)?;
    Ok(DispatchResult::null())
}

/// Serializes one inner or outer modern element view.
fn serialize(
    context: &Context,
    request: &Request,
    inner: bool,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (element, graph) = modern_element(context, request)?;
    if graph.family() == DocumentFamily::ModernXml
        && !crate::native::element_xml_is_well_formed(element, inner)
    {
        return Ok(DispatchResult::dom_exception(
            12,
            b"The resulting XML serialization is not well-formed",
        ));
    }
    let bytes = crate::native::element_serialize_markup(
        element,
        inner,
        graph.family() == DocumentFamily::ModernHtml,
    )
    .ok_or(())?;
    Ok(DispatchResult::bytes(bytes))
}

/// Resolves one modern element receiver and its authoritative graph.
fn modern_element(
    context: &Context,
    request: &Request,
) -> Result<(usize, Rc<DocumentGraph>), ()> {
    let (pointer, graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if is_document
        || crate::native::node_type(pointer) != 1
        || graph.family() == DocumentFamily::Legacy
    {
        return Err(());
    }
    Ok((pointer, graph))
}

/// Parses one fragment and maps native failures to PHP DOM exceptions.
fn parse_fragment(
    context: usize,
    input: &[u8],
    graph: &DocumentGraph,
) -> Result<usize, DispatchResult> {
    crate::native::parse_fragment(
        context,
        input,
        graph.family() == DocumentFamily::ModernHtml,
    )
    .map_err(|code| {
        if code == 12 {
            DispatchResult::dom_exception(
                12,
                b"XML fragment is not well-formed",
            )
        } else {
            invalid_state()
        }
    })
}

/// Returns a real element context or a temporary HTML body for fragment parents.
fn fragment_context(
    context: usize,
    graph: &DocumentGraph,
) -> Result<(usize, Option<usize>), ()> {
    let requires_body = crate::native::node_type(context) != 1
        || (graph.family() == DocumentFamily::ModernHtml
            && crate::native::node_is_html_element(context)
            && crate::native::node_name(context).as_deref() == Some(b"html"));
    if !requires_body {
        return Ok((context, None));
    }
    let outcome = crate::native::document_create_element_ns(
        graph.pointer(),
        Some(HTML_NAMESPACE),
        b"body",
        None,
        true,
    );
    let body = outcome.pointer.ok_or(())?;
    Ok((body, Some(body)))
}

/// Frees a temporary parsing-only context element.
fn free_temporary_context(context: Option<usize>) {
    if let Some(context) = context {
        unsafe {
            crate::native::node_free(context);
        }
    }
}

/// Detaches every current child so canonical wrappers remain valid.
fn detach_children(
    context: &mut Context,
    parent: usize,
    graph: &Rc<DocumentGraph>,
) -> Result<(), ()> {
    while let Some(child) = crate::native::node_first_child(parent) {
        if !crate::native::node_unlink_child(parent, child) {
            return Err(());
        }
        context.register_detached_root(child, Rc::clone(graph));
    }
    Ok(())
}

/// Moves every parsed fragment child before one reference or appends it.
fn insert_fragment(
    parent: usize,
    reference: Option<usize>,
    fragment: usize,
) -> Result<(), ()> {
    while let Some(child) = crate::native::node_first_child(fragment) {
        if crate::native::node_insert_before(parent, child, reference)
            != Some(child)
        {
            unsafe {
                crate::native::node_free(fragment);
            }
            return Err(());
        }
    }
    unsafe {
        crate::native::node_free(fragment);
    }
    Ok(())
}

/// Parses the four string-backed enum values used by PHP.
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

/// Builds PHP's invalid-state DOM exception.
fn invalid_state() -> DispatchResult {
    DispatchResult::dom_exception(11, b"Invalid State Error")
}

/// Builds PHP's detached/document-parent adjacent-markup exception.
fn no_modification() -> DispatchResult {
    DispatchResult::dom_exception(7, b"No Modification Allowed Error")
}
