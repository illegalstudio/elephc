//! Purpose:
//! Dispatches modern DOM CSS selector queries through PHP's pinned Lexbor adapter.
//! Preserves query snapshots, canonical node wrappers, and exact selector diagnostics.
//!
//! Called from:
//! - `super::routes::dispatch()` for `querySelector*`, `matches`, and `closest`.
//!
//! Key details:
//! - HTML limited/full quirks mode controls PHP's selector comparison rules.
//! - `querySelectorAll()` captures a static `NodeList`, unlike live DOM collections.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{CollectionKind, DocumentFamily, NamespaceNodeAllocation};
use crate::request::Request;

use super::{
    canonical_pointer_result, collection_result,
    receiver_pointer_and_graph, DispatchResult,
};

const QUERY_FIRST: i32 = 0;
const QUERY_ALL: i32 = 1;
const MATCHES: i32 = 2;
const CLOSEST: i32 = 3;
const UNSUPPORTED_SELECTOR: i32 = 100;

/// Executes a selector query after validating its sole PHP string argument.
fn execute(
    context: &Context,
    request: &Request,
    operation: i32,
) -> Result<
    (
        usize,
        std::rc::Rc<crate::objects::DocumentGraph>,
        crate::native::SelectorOutcome,
    ),
    (),
> {
    if request.values.len() != 1 {
        return Err(());
    }
    let selectors = request.byte_string(0)?;
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if graph.family() == DocumentFamily::Legacy {
        return Err(());
    }
    let quirks = graph.family() == DocumentFamily::ModernHtml
        && crate::native::html_document_quirks_mode(graph.pointer()) != 0;
    let outcome =
        crate::native::selector_query(root, selectors, operation, quirks);
    Ok((root, graph, outcome))
}

/// Converts a native selector failure into PHP's exact catchable result.
fn selector_error(
    request: &Request,
    outcome: &crate::native::SelectorOutcome,
) -> Result<Option<DispatchResult>, ()> {
    match outcome.error_code {
        0 => Ok(None),
        9 | 12 => Ok(Some(DispatchResult::dom_exception(
            outcome.error_code,
            &outcome.message,
        ))),
        UNSUPPORTED_SELECTOR => Ok(Some(DispatchResult::value_error(
            unsupported_selector_message(request)?,
        ))),
        _ => Err(()),
    }
}

/// Returns PHP's method-qualified unsupported-selector `ValueError` text.
fn unsupported_selector_message(
    request: &Request,
) -> Result<&'static [u8], ()> {
    let key = crate::generated::opcodes::operation_key(
        request.header.opcode,
    )
    .ok_or(())?;
    match key {
        "method:dom\\document::queryselector" => Ok(
            b"Dom\\Document::querySelector(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\document::queryselectorall" => Ok(
            b"Dom\\Document::querySelectorAll(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\documentfragment::queryselector" => Ok(
            b"Dom\\DocumentFragment::querySelector(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\documentfragment::queryselectorall" => Ok(
            b"Dom\\DocumentFragment::querySelectorAll(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\element::queryselector" => Ok(
            b"Dom\\Element::querySelector(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\element::queryselectorall" => Ok(
            b"Dom\\Element::querySelectorAll(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\element::matches" => Ok(
            b"Dom\\Element::matches(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\element::closest" => Ok(
            b"Dom\\Element::closest(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\parentnode::queryselector" => Ok(
            b"Dom\\ParentNode::querySelector(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        "method:dom\\parentnode::queryselectorall" => Ok(
            b"Dom\\ParentNode::querySelectorAll(): Argument #1 ($selectors) contains an unsupported selector",
        ),
        _ => Err(()),
    }
}

/// Returns the first matching descendant or PHP null.
pub(super) fn query_selector(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (_, graph, outcome) = execute(context, request, QUERY_FIRST)?;
    if let Some(error) = selector_error(request, &outcome)? {
        return Ok(error);
    }
    let Some(pointer) = outcome.pointers.first().copied() else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, pointer, graph)
}

/// Returns a static `NodeList` snapshot of all matching descendants.
pub(super) fn query_selector_all(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (root, graph, outcome) = execute(context, request, QUERY_ALL)?;
    if let Some(error) = selector_error(request, &outcome)? {
        return Ok(error);
    }
    let pointers: Vec<Option<usize>> =
        outcome.pointers.into_iter().map(Some).collect();
    let namespace_allocations: Vec<Option<Rc<NamespaceNodeAllocation>>> =
        vec![None; pointers.len()];
    let member_documents = vec![None; pointers.len()];
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::Snapshot {
            pointers,
            member_documents,
            namespace_allocations,
        },
    ))
}

/// Reports whether the receiver element matches the supplied selector.
pub(super) fn matches(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (_, _, outcome) = execute(context, request, MATCHES)?;
    if let Some(error) = selector_error(request, &outcome)? {
        return Ok(error);
    }
    Ok(DispatchResult::boolean(outcome.matched))
}

/// Returns the receiver's nearest matching inclusive element ancestor.
pub(super) fn closest(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let (_, graph, outcome) = execute(context, request, CLOSEST)?;
    if let Some(error) = selector_error(request, &outcome)? {
        return Ok(error);
    }
    let Some(pointer) = outcome.pointers.first().copied() else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, pointer, graph)
}
