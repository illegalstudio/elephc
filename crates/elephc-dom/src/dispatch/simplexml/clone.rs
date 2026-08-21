//! Purpose:
//! Implements php-src's dedicated `SimpleXMLElement` object-clone handler.
//! Copies native XML data while deliberately leaving iterator data uninitialized.
//!
//! Called from:
//! - `super::super::routes::dispatch()` for `internal:bridge.object.clone`.
//!
//! Key details:
//! - Root views deep-clone their complete document; non-root views copy one detached subtree.
//! - Iterator filters survive cloning, but the eager current handle never does.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{SimpleXmlIteratorState, SimpleXmlObject};
use crate::request::Request;

use super::super::DispatchResult;

/// Clones one SimpleXML native view with php-src's root/document ownership split.
pub(in crate::dispatch) fn clone_object(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    super::super::require_no_values(request)?;
    let source = super::object(context, request.header.receiver)?;
    let source_pointer = source.pointer();
    let source_document = source.document();
    let wrapper_kind = source.wrapper_kind();
    let iterator = SimpleXmlIteratorState::new(
        source.iterator().kind(),
        source.iterator().name().map(<[u8]>::to_vec),
        source
            .iterator()
            .namespace_or_prefix()
            .map(<[u8]>::to_vec),
        source.iterator().is_prefix(),
    );
    let is_root = crate::native::node_parent(source_pointer)
        .is_some_and(|parent| matches!(crate::native::node_type(parent), 9 | 13));
    let (pointer, document) = if is_root {
        let cloned_document = crate::native::document_clone(
            source_document.pointer(),
            true,
            source_document.family(),
        )
        .ok_or(())?;
        let pointer = crate::native::document_element(cloned_document).ok_or(())?;
        (
            pointer,
            Rc::new(source_document.replacement(cloned_document)),
        )
    } else {
        let pointer =
            crate::native::node_clone(source_pointer, true, false).ok_or(())?;
        context.register_detached_root(pointer, Rc::clone(&source_document));
        (pointer, source_document)
    };
    Ok(super::fresh_result(
        context,
        SimpleXmlObject::new(pointer, document, wrapper_kind, iterator),
    ))
}
