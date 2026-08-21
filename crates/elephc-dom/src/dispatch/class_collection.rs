//! Purpose:
//! Implements modern live `getElementsByClassName()` collection descriptors.
//! Parses and matches PHP ordered sets with standards/full-quirks distinctions.
//!
//! Called from:
//! - `super::routes::dispatch()` when creating a class-name collection.
//! - `super::collection` when resolving the collection's live length or item.
//!
//! Key details:
//! - Limited quirks remains case-sensitive here, unlike CSS selector matching.
//! - Only the unnamespaced `class` attribute participates.

use crate::context::Context;
use crate::objects::{CollectionKind, DocumentFamily};
use crate::request::Request;

use super::{
    collection_result, receiver_pointer_and_graph, DispatchResult,
};

/// Creates one live descendant-element query by an ordered set of class names.
pub(super) fn elements_by_class_name(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let class_names = request.byte_string(0)?;
    if class_names.contains(&0) {
        return Ok(DispatchResult::value_error(
            class_names_null_message(request)?,
        ));
    }
    if class_names.len() > i32::MAX as usize {
        return Ok(DispatchResult::value_error(
            class_names_too_long_message(request)?,
        ));
    }
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if graph.family() == DocumentFamily::Legacy {
        return Err(());
    }
    let full_quirks = graph.family() == DocumentFamily::ModernHtml
        && crate::native::html_document_quirks_mode(graph.pointer()) == 1;
    let names = parse_ordered_set(class_names, full_quirks);
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::ElementsByClassName {
            names,
            full_quirks,
        },
    ))
}

/// Counts matching descendant elements by one linear tree-order traversal.
pub(super) fn length(
    root: usize,
    names: &[Vec<u8>],
    full_quirks: bool,
) -> usize {
    let mut count = 0;
    let mut current = crate::native::descendant_element_first(root);
    while let Some(element) = current {
        if element_matches_class_names(element, names, full_quirks) {
            count += 1;
        }
        current = crate::native::descendant_element_next(root, element);
    }
    count
}

/// Returns one matching descendant element by zero-based live index.
pub(super) fn item(
    root: usize,
    names: &[Vec<u8>],
    full_quirks: bool,
    mut index: usize,
) -> Option<usize> {
    let mut current = crate::native::descendant_element_first(root);
    while let Some(element) = current {
        if element_matches_class_names(element, names, full_quirks) {
            if index == 0 {
                return Some(element);
            }
            index -= 1;
        }
        current = crate::native::descendant_element_next(root, element);
    }
    None
}

/// Splits one PHP class-name query into a unique ordered ASCII-whitespace set.
fn parse_ordered_set(input: &[u8], lowercase: bool) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    for token in input
        .split(|byte| is_ascii_whitespace(*byte))
        .filter(|token| !token.is_empty())
    {
        let mut token = token.to_vec();
        if lowercase {
            token.make_ascii_lowercase();
        }
        if !names.contains(&token) {
            names.push(token);
        }
    }
    names
}

/// Reports whether one byte is HTML's ASCII whitespace.
fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

/// Reports whether one element's unnamespaced class tokens contain every query token.
fn element_matches_class_names(
    element: usize,
    names: &[Vec<u8>],
    full_quirks: bool,
) -> bool {
    if names.is_empty() {
        return false;
    }
    let Some(value) =
        crate::native::element_get_attribute_ns(element, None, b"class")
    else {
        return false;
    };
    names.iter().all(|name| {
        value
            .split(|byte| is_ascii_whitespace(*byte))
            .filter(|token| !token.is_empty())
            .any(|token| {
                if full_quirks {
                    token.eq_ignore_ascii_case(name)
                } else {
                    token == name
                }
            })
    })
}

/// Returns PHP's method-qualified class-name length `ValueError` text.
fn class_names_too_long_message(
    request: &Request,
) -> Result<&'static [u8], ()> {
    let key = crate::generated::opcodes::operation_key(
        request.header.opcode,
    )
    .ok_or(())?;
    match key {
        "method:dom\\document::getelementsbyclassname" => Ok(
            b"Dom\\Document::getElementsByClassName(): Argument #1 ($classNames) is too long",
        ),
        "method:dom\\element::getelementsbyclassname" => Ok(
            b"Dom\\Element::getElementsByClassName(): Argument #1 ($classNames) is too long",
        ),
        _ => Err(()),
    }
}

/// Returns PHP's method-qualified null-byte `ValueError` text.
fn class_names_null_message(
    request: &Request,
) -> Result<&'static [u8], ()> {
    let key = crate::generated::opcodes::operation_key(
        request.header.opcode,
    )
    .ok_or(())?;
    match key {
        "method:dom\\document::getelementsbyclassname" => Ok(
            b"Dom\\Document::getElementsByClassName(): Argument #1 ($classNames) must not contain any null bytes",
        ),
        "method:dom\\element::getelementsbyclassname" => Ok(
            b"Dom\\Element::getElementsByClassName(): Argument #1 ($classNames) must not contain any null bytes",
        ),
        _ => Err(()),
    }
}
