//! Purpose:
//! Implements stateless legacy and modern DOM implementation operations.
//! Validates direct zero-handle wrappers and document-associated retained wrappers uniformly.
//!
//! Called from:
//! - `super::routes::dispatch()` for `DOMImplementation` and `Dom\Implementation`.
//!
//! Key details:
//! - Directly created wrappers legitimately carry receiver handle zero.
//! - Detached document types hide their private lifetime anchor until document creation adopts them.

use crate::context::Context;
use crate::objects::{
    DocumentFamily, DocumentObject, HANDLE_IMPLEMENTATION,
};
use crate::request::Request;

use super::{
    direct_node_result, dom_exception, optional_bytes,
    rehome_subtree_handles, DispatchResult,
};

/// Implements PHP's retained DOM Level feature table.
pub(super) fn has_feature(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_receiver_family(context, request, DocumentFamily::Legacy)?;
    if request.values.len() != 2 {
        return Err(());
    }
    let feature = request.byte_string(0)?;
    let version = request.byte_string(1)?;
    let supported_version = matches!(version, b"" | b"1.0" | b"2.0");
    let xml = feature.eq_ignore_ascii_case(b"XML");
    let core = feature.eq_ignore_ascii_case(b"Core") && version == b"1.0";
    Ok(DispatchResult::boolean(supported_version && (xml || core)))
}

/// Creates one detached legacy or modern document type with exact validation policy.
pub(super) fn create_document_type(
    context: &mut Context,
    request: &Request,
    family: DocumentFamily,
) -> Result<DispatchResult, ()> {
    require_receiver_family(context, request, family)?;
    let expected = if family == DocumentFamily::Legacy {
        1..=3
    } else {
        3..=3
    };
    if !expected.contains(&request.values.len()) {
        return Err(());
    }
    let qualified_name = request.byte_string(0)?;
    if family == DocumentFamily::Legacy && qualified_name.is_empty() {
        return Ok(DispatchResult::value_error(
            b"DOMImplementation::createDocumentType(): Argument #1 ($qualifiedName) must not be empty",
        ));
    }
    if family != DocumentFamily::Legacy
        && !crate::native::validate_qname(qualified_name)
    {
        return Ok(dom_exception(14));
    }
    let public_id = optional_bytes(request, 1, b"")?;
    let system_id = optional_bytes(request, 2, b"")?;
    let pointer = crate::native::document_type_new(
        qualified_name,
        public_id,
        system_id,
    )
    .ok_or(())?;
    let graph_pointer = crate::native::document_new(b"1.0", b"").ok_or(())?;
    let graph = DocumentObject::new(graph_pointer, family).graph();
    Ok(direct_node_result(context, pointer, graph))
}

/// Creates a legacy or modern XML document with an optional root and document type.
pub(super) fn create_document(
    context: &mut Context,
    request: &Request,
    family: DocumentFamily,
) -> Result<DispatchResult, ()> {
    require_receiver_family(context, request, family)?;
    let expected = if family == DocumentFamily::Legacy {
        0..=3
    } else {
        2..=3
    };
    if !expected.contains(&request.values.len()) {
        return Err(());
    }
    let namespace_uri = if request.values.is_empty() {
        None
    } else {
        request.optional_byte_string(0)?
    };
    let qualified_name = optional_bytes(request, 1, b"")?;
    let doctype = if request.values.len() < 3 {
        None
    } else {
        request.optional_bridge_handle(2)?
    };
    let doctype_pointer = match doctype {
        Some(handle) => {
            let node = super::node(context, handle)?;
            let pointer = node.pointer();
            let storage_type = crate::native::node_storage_type(pointer);
            if family == DocumentFamily::Legacy && storage_type == 10 {
                return Ok(DispatchResult::value_error(
                    b"DOMImplementation::createDocument(): Argument #3 ($doctype) is an invalid DocumentType object",
                ));
            }
            if storage_type != 14 {
                return Err(());
            }
            let source_family = node.document().family();
            if (family == DocumentFamily::Legacy)
                != (source_family == DocumentFamily::Legacy)
            {
                return Err(());
            }
            if family == DocumentFamily::Legacy
                && crate::native::node_document(pointer).is_some()
            {
                return Ok(dom_exception(4));
            }
            Some(pointer)
        }
        None => None,
    };
    let modern = family != DocumentFamily::Legacy;
    let encoding = if modern { b"UTF-8".as_slice() } else { b"".as_slice() };
    let pointer = crate::native::document_new(b"1.0", encoding).ok_or(())?;
    if modern && !crate::native::document_convert_modern_xml(pointer) {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Err(());
    }
    let root = if qualified_name.is_empty() {
        None
    } else {
        let outcome = crate::native::document_create_implementation_root(
            pointer,
            namespace_uri,
            qualified_name,
            modern,
        );
        if outcome.error_code != 0 {
            unsafe {
                crate::native::document_free(pointer);
            }
            return Ok(dom_exception(outcome.error_code));
        }
        let Some(root) = outcome.pointer else {
            unsafe {
                crate::native::document_free(pointer);
            }
            return Err(());
        };
        Some(root)
    };
    if let Some(doctype_pointer) = doctype_pointer {
        let error = crate::native::document_attach_doctype(
            pointer,
            doctype_pointer,
            modern,
        );
        if error != 0 {
            if let Some(root) = root {
                unsafe {
                    crate::native::node_free(root);
                }
            }
            unsafe {
                crate::native::document_free(pointer);
            }
            return if error > 0 {
                Ok(dom_exception(error))
            } else {
                Err(())
            };
        }
    }
    if let Some(root) = root {
        if crate::native::node_append_child(pointer, root) != Some(root) {
            unsafe {
                crate::native::node_free(root);
                crate::native::document_free(pointer);
            }
            return Err(());
        }
    }
    let result = super::document::insert_pointer(context, pointer, family);
    if let Some(doctype_pointer) = doctype_pointer {
        let graph =
            super::document(context, result.frame.payload0)?.graph();
        rehome_subtree_handles(context, doctype_pointer, graph)?;
        context.attach_detached_root(doctype_pointer);
    }
    Ok(result)
}

/// Creates a complete modern HTML document with PHP's optional-title distinction.
pub(super) fn create_html_document(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_receiver_family(context, request, DocumentFamily::ModernHtml)?;
    if request.values.len() > 1 {
        return Err(());
    }
    let title = if request.values.is_empty() {
        None
    } else {
        request.optional_byte_string(0)?
    };
    let pointer = crate::native::document_new_html(title).ok_or(())?;
    Ok(super::document::insert_pointer(
        context,
        pointer,
        DocumentFamily::ModernHtml,
    ))
}

/// Validates a zero-handle direct wrapper or a retained implementation handle.
fn require_receiver_family(
    context: &Context,
    request: &Request,
    family: DocumentFamily,
) -> Result<(), ()> {
    if request.header.receiver == 0 {
        return Ok(());
    }
    let implementation = context
        .native_objects
        .get(request.header.receiver, HANDLE_IMPLEMENTATION)
        .map_err(|_| ())?
        .implementation()
        .ok_or(())?;
    let matches = implementation.family() == family
        || (implementation.family() != DocumentFamily::Legacy
            && family != DocumentFamily::Legacy);
    matches.then_some(()).ok_or(())
}
