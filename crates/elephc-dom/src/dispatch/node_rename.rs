//! Purpose:
//! Dispatches PHP 8.5 modern element and attribute renaming.
//! Maps native namespace and class-stability checks to exact DOM exceptions.
//!
//! Called from:
//! - `super::routes::dispatch()` for `Dom\Element::rename()` and `Dom\Attr::rename()`.
//!
//! Key details:
//! - Rename mutates the existing native node, so every canonical wrapper retains identity.
//! - HTML namespace transitions and template renames are rejected before any node mutation.

use crate::context::Context;
use crate::request::Request;

use super::{receiver_pointer_and_graph, DispatchResult};

/// Renames one modern element or attribute in place.
pub(super) fn rename(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let (pointer, _, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if is_document || !matches!(crate::native::node_type(pointer), 1 | 2) {
        return Err(());
    }
    let status = crate::native::node_rename(
        pointer,
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
    );
    Ok(match status {
        0 => DispatchResult::null(),
        5 => DispatchResult::dom_exception(5, b"Invalid Character Error"),
        14 => DispatchResult::dom_exception(14, b"Namespace Error"),
        1301 => DispatchResult::dom_exception(
            13,
            b"An attribute with the given name in the given namespace already exists",
        ),
        1302 => DispatchResult::dom_exception(
            13,
            b"It is not possible to move an element out of the HTML namespace because the HTML namespace is tied to the HTMLElement class",
        ),
        1303 => DispatchResult::dom_exception(
            13,
            b"It is not possible to move an element into the HTML namespace because the HTML namespace is tied to the HTMLElement class",
        ),
        1304 => DispatchResult::dom_exception(
            13,
            b"It is not possible to rename the template element because it hosts a document fragment",
        ),
        11 => DispatchResult::dom_exception(11, b"Invalid State Error"),
        _ => return Err(()),
    })
}
