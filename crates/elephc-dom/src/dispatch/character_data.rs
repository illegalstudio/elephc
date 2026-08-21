//! Purpose:
//! Dispatches PHP character-data properties and UTF-8 code-point mutation methods.
//! Preserves legacy boolean returns and modern void semantics over one libxml2 node.
//!
//! Called from:
//! - `super::routes::dispatch()` for `DOMCharacterData` and `Dom\CharacterData`.
//!
//! Key details:
//! - Offsets and counts use Unicode code points, matching php-src's libxml2 helpers.
//! - Positive native error code one becomes a catchable `INDEX_SIZE_ERR`.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::DocumentFamily;
use crate::request::Request;

use super::{
    canonical_pointer_result, receiver_pointer_and_graph, require_no_values,
    DispatchResult,
};

/// Returns one character-data node's exact byte content as a PHP string.
pub(super) fn data(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        crate::native::node_content(pointer).ok_or(())?,
    ))
}

/// Replaces one character-data node's exact byte content.
pub(super) fn set_data(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.byte_string(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if !crate::native::node_set_content(pointer, value) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Returns one character-data node's UTF-8 code-point length.
pub(super) fn length(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::integer(
        crate::native::character_data_length(pointer).ok_or(())?,
    ))
}

/// Returns one PHP-compatible code-point substring.
pub(super) fn substring(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let offset = request.integer(0)?;
    let count = request.integer(1)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let outcome = crate::native::character_data_substring(
        pointer,
        offset,
        count,
        graph.family() != DocumentFamily::Legacy,
    );
    match outcome.error_code {
        0 => Ok(DispatchResult::bytes(outcome.bytes.ok_or(())?)),
        1 => Ok(index_size_error()),
        _ => Err(()),
    }
}

/// Appends exact bytes and returns the family-specific PHP result.
pub(super) fn append(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.byte_string(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if !crate::native::character_data_append(pointer, value) {
        return Err(());
    }
    Ok(success_result(graph.family()))
}

/// Inserts exact bytes at one UTF-8 code-point offset.
pub(super) fn insert(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let offset = request.integer(0)?;
    let value = request.byte_string(1)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    modification_result(
        graph.family(),
        crate::native::character_data_insert(
            pointer,
            offset,
            value,
            graph.family() != DocumentFamily::Legacy,
        ),
    )
}

/// Deletes one UTF-8 code-point range.
pub(super) fn delete(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let offset = request.integer(0)?;
    let count = request.integer(1)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    modification_result(
        graph.family(),
        crate::native::character_data_delete(
            pointer,
            offset,
            count,
            graph.family() != DocumentFamily::Legacy,
        ),
    )
}

/// Replaces one UTF-8 code-point range with exact bytes.
pub(super) fn replace(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 3 {
        return Err(());
    }
    let offset = request.integer(0)?;
    let count = request.integer(1)?;
    let value = request.byte_string(2)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    modification_result(
        graph.family(),
        crate::native::character_data_replace(
            pointer,
            offset,
            count,
            value,
            graph.family() != DocumentFamily::Legacy,
        ),
    )
}

/// Returns the complete adjacent text/CDATA run containing this text node.
pub(super) fn whole_text(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        crate::native::text_whole_text(pointer).ok_or(())?,
    ))
}

/// Splits one text node and preserves the new node's canonical graph identity.
pub(super) fn split_text(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let offset = request.integer(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if offset < 0 {
        let message = if graph.family() == DocumentFamily::Legacy {
            b"DOMText::splitText(): Argument #1 ($offset) must be greater than or equal to 0"
                .as_slice()
        } else {
            b"Dom\\Text::splitText(): Argument #1 ($offset) must be greater than or equal to 0"
                .as_slice()
        };
        return Ok(DispatchResult::value_error(message));
    }
    let was_attached = crate::native::node_parent(pointer).is_some();
    let outcome = crate::native::text_split(pointer, offset);
    match outcome.error_code {
        0 => {
            let split = outcome.pointer.ok_or(())?;
            if !was_attached {
                context.register_detached_root(split, Rc::clone(&graph));
            }
            canonical_pointer_result(context, split, graph)
        }
        1 if graph.family() == DocumentFamily::Legacy => {
            Ok(DispatchResult::boolean(false))
        }
        1 => Ok(index_size_error()),
        11 => Ok(DispatchResult::dom_exception(
            11,
            b"Invalid State Error",
        )),
        _ => Err(()),
    }
}

/// Reports the legacy libxml2 blank-text predicate used by both PHP aliases.
pub(super) fn is_whitespace(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(crate::native::text_is_blank(pointer)))
}

/// Maps one native mutation status into PHP success or `INDEX_SIZE_ERR`.
fn modification_result(
    family: DocumentFamily,
    status: i32,
) -> Result<DispatchResult, ()> {
    match status {
        0 => Ok(success_result(family)),
        1 => Ok(index_size_error()),
        _ => Err(()),
    }
}

/// Returns `true` for legacy methods and `null` for modern void methods.
fn success_result(family: DocumentFamily) -> DispatchResult {
    if family == DocumentFamily::Legacy {
        DispatchResult::boolean(true)
    } else {
        DispatchResult::null()
    }
}

/// Builds PHP's canonical `INDEX_SIZE_ERR` result.
fn index_size_error() -> DispatchResult {
    DispatchResult::dom_exception(1, b"Index Size Error")
}
