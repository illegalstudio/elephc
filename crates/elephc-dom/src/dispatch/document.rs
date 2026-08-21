//! Purpose:
//! Dispatches PHP DOM document construction, parsing, serialization, and node factories.
//! Owns document-local flags and canonical root-element materialization.
//!
//! Called from:
//! - `super::dispatch()` for legacy and modern document operations.
//!
//! Key details:
//! - Failed legacy parsing preserves the receiver's prior authoritative graph.
//! - Newly created native nodes remain detached roots until a tree mutation attaches them.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{
    DocumentFamily, DocumentObject, ImplementationObject, NativeObject,
    HANDLE_DOCUMENT, HANDLE_IMPLEMENTATION,
};
use crate::request::Request;

use super::{
    canonical_pointer_result, document, document_mut, dom_exception,
    libxml::record_errors, optional_bytes, optional_i32,
    receiver_pointer_and_graph, register_detached_node, rehome_subtree_handles,
    require_no_receiver, require_no_values, DispatchResult,
};

/// Constructs one empty legacy `DOMDocument` using PHP's default version and encoding.
pub(super) fn construct_legacy(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 2 {
        return Err(());
    }
    let version = optional_bytes(request, 0, b"1.0")?;
    let encoding = optional_bytes(request, 1, b"")?;
    if request.header.receiver == 0 {
        return insert(context, version, encoding, DocumentFamily::Legacy);
    }
    document(context, request.header.receiver)?;
    let pointer = crate::native::document_new(version, encoding).ok_or(())?;
    if context
        .reconstruct_legacy_document(request.header.receiver, pointer)
        .is_err()
    {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Constructs one empty modern XML document using PHP's UTF-8 default.
pub(super) fn create_empty_modern_xml(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    let version = optional_bytes(request, 0, b"1.0")?;
    let encoding = optional_bytes(request, 1, b"UTF-8")?;
    insert(context, version, encoding, DocumentFamily::ModernXml)
}

/// Constructs one empty modern HTML document with PHP's XML serialization flags.
pub(super) fn create_empty_modern_html(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    let encoding = optional_bytes(request, 0, b"UTF-8")?;
    let result = insert(context, b"1.0", encoding, DocumentFamily::ModernHtml)?;
    let handle = result.frame.payload0;
    let document = document_mut(context, handle)?;
    if !crate::native::document_set_standalone(document.pointer(), 1) {
        let _ = context.native_objects.remove(handle, HANDLE_DOCUMENT);
        return Err(());
    }
    Ok(result)
}

/// Parses one modern XML source into a fresh authoritative document handle.
pub(super) fn create_modern_xml_from_string(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    const VALID_OPTIONS: u64 = 1
        | 2
        | 4
        | 8
        | 16
        | 32
        | 64
        | 128
        | 256
        | 1_024
        | 2_048
        | 8_192
        | 16_384
        | 65_536
        | 524_288
        | 4_194_304
        | 8_388_608;

    require_no_receiver(request)?;
    if request.values.is_empty() || request.values.len() > 3 {
        return Err(());
    }
    let source = request.byte_string(0)?;
    if source.is_empty() {
        return Ok(DispatchResult::value_error(
            b"Dom\\XMLDocument::createFromString(): Argument #1 ($source) must not be empty",
        ));
    }
    let options = if request.values.len() > 1 {
        request.integer(1)? as u64
    } else {
        0
    };
    if options & !VALID_OPTIONS != 0 {
        return Ok(DispatchResult::value_error(
            b"Dom\\XMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_RECOVER, LIBXML_NOENT, LIBXML_NO_XXE, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES)",
        ));
    }
    let override_encoding = if request.values.len() > 2 {
        request.optional_byte_string(2)?
    } else {
        None
    };
    if override_encoding
        .is_some_and(|encoding| !crate::native::encoding_is_valid(encoding))
    {
        return Ok(DispatchResult::value_error(
            b"Dom\\XMLDocument::createFromString(): Argument #3 ($overrideEncoding) must be a valid document encoding",
        ));
    }
    let outcome = crate::native::document_parse_xml(
        source,
        options as i32,
        override_encoding,
        None,
    )?;
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = dom_exception(12);
        return Ok(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"Dom\\XMLDocument::createFromString",
                &outcome.errors,
                options as i32,
            )
        } else {
            result
        });
    };
    let encoding = override_encoding.unwrap_or(b"UTF-8");
    if crate::native::document_encoding(pointer).is_none()
        && crate::native::document_set_encoding(pointer, encoding) != 1
    {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Ok(dom_exception(11));
    }
    if !crate::native::document_convert_modern_xml(pointer) {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Err(());
    }
    let handle = context.native_objects.insert(
        HANDLE_DOCUMENT,
        NativeObject::Document(DocumentObject::new(
            pointer,
            DocumentFamily::ModernXml,
        )),
    );
    context.document_handles.insert(pointer, handle);
    let result = DispatchResult::bridge_handle(handle);
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"Dom\\XMLDocument::createFromString",
            &outcome.errors,
            options as i32,
        )
    } else {
        result
    })
}

/// Parses one modern HTML5 source into a fresh authoritative document handle.
pub(super) fn create_modern_html_from_string(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    const VALID_OPTIONS: u64 = 32 | 65_536 | 8_192 | 2_147_483_648;

    require_no_receiver(request)?;
    if request.values.is_empty() || request.values.len() > 3 {
        return Err(());
    }
    let source = request.byte_string(0)?;
    let options = if request.values.len() > 1 {
        request.integer(1)? as u64
    } else {
        0
    };
    if options & !VALID_OPTIONS != 0 {
        return Ok(DispatchResult::value_error(
            b"Dom\\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\\HTML_NO_DEFAULT_NS)",
        ));
    }
    let override_encoding = if request.values.len() > 2 {
        request.optional_byte_string(2)?
    } else {
        None
    };
    let outcome = match crate::native::document_parse_html5(
        source,
        options as u32,
        override_encoding,
        b"Entity",
    ) {
        Ok(outcome) => outcome,
        Err(crate::native::HtmlParseError::InvalidEncoding) => {
            return Ok(DispatchResult::value_error(
                b"Dom\\HTMLDocument::createFromString(): Argument #3 ($overrideEncoding) must be a valid document encoding",
            ));
        }
        Err(crate::native::HtmlParseError::Allocation) => {
            return Ok(dom_exception(11));
        }
    };
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let pointer = outcome.document.ok_or(())?;
    let handle = context.native_objects.insert(
        HANDLE_DOCUMENT,
        NativeObject::Document(DocumentObject::new(
            pointer,
            DocumentFamily::ModernHtml,
        )),
    );
    context.document_handles.insert(pointer, handle);
    let result = DispatchResult::bridge_handle(handle);
    Ok(if emit_warnings {
        result.with_libxml_warnings(
            b"Dom\\HTMLDocument::createFromString",
            &outcome.errors,
        )
    } else {
        result
    })
}

/// Parses XML into a legacy receiver while preserving its old graph on failure.
pub(super) fn load_legacy_xml(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let source = request.byte_string(0)?;
    let explicit_options = optional_i32(request, 1, 0)?;
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let options = explicit_options | target.legacy_parser_options();
    let outcome =
        crate::native::document_parse_xml(source, options, None, None)?;
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = DispatchResult::boolean(false);
        return Ok(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"DOMDocument::loadXML",
                &outcome.errors,
                options,
            )
        } else {
            result
        });
    };
    let document = document_mut(context, request.header.receiver)?;
    let previous_pointer = document.pointer();
    document.replace_pointer(pointer);
    context.document_handles.remove(&previous_pointer);
    context.document_handles.insert(pointer, request.header.receiver);
    let result = DispatchResult::boolean(true);
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"DOMDocument::loadXML",
            &outcome.errors,
            options,
        )
    } else {
        result
    })
}

/// Parses legacy HTML into a receiver while preserving its old graph on failure.
pub(super) fn load_legacy_html(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let source = request.byte_string(0)?;
    if source.is_empty() {
        return Ok(DispatchResult::value_error(
            b"DOMDocument::loadHTML(): Argument #1 ($source) must not be empty",
        ));
    }
    let options = optional_i32(request, 1, 0)?;
    let target = document(context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let outcome =
        crate::native::document_parse_html4(source, options, None)?;
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = DispatchResult::boolean(false);
        return Ok(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"DOMDocument::loadHTML",
                &outcome.errors,
                options,
            )
        } else {
            result
        });
    };
    let document = document_mut(context, request.header.receiver)?;
    let previous_pointer = document.pointer();
    document.replace_pointer(pointer);
    context.document_handles.remove(&previous_pointer);
    context.document_handles.insert(pointer, request.header.receiver);
    let result = DispatchResult::boolean(true);
    Ok(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"DOMDocument::loadHTML",
            &outcome.errors,
            options,
        )
    } else {
        result
    })
}

/// Serializes one complete document or one same-document node using its format flag.
pub(super) fn serialize_xml(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 2 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let options = if request.values.len() == 2 {
        request.integer(1)? as i32
    } else {
        0
    };
    let node_handle = if request.values.is_empty() {
        None
    } else {
        request.optional_bridge_handle(0)?
    };
    let mut serialized_document_node = node_handle.is_none();
    let mut bytes = if let Some(node_handle) = node_handle {
        let (node_pointer, node_graph, _) =
            receiver_pointer_and_graph(context, node_handle)?;
        if !Rc::ptr_eq(&document.graph(), &node_graph) {
            return Ok(dom_exception(4));
        }
        serialized_document_node = node_pointer == document.pointer();
        crate::native::document_serialize_node(
            document.pointer(),
            node_pointer,
            document.format_output(),
            match document.family() {
                DocumentFamily::Legacy => 0,
                DocumentFamily::ModernXml => 1,
                DocumentFamily::ModernHtml => 2,
            },
            options & crate::native::XML_SAVE_NO_EMPTY,
        )
        .ok_or(())?
    } else {
        crate::native::document_serialize(
            document.pointer(),
            None,
            document.format_output(),
            match document.family() {
                DocumentFamily::Legacy => 0,
                DocumentFamily::ModernXml => 1,
                DocumentFamily::ModernHtml => 2,
            },
            options
                & (crate::native::XML_SAVE_NO_DECL
                    | crate::native::XML_SAVE_NO_EMPTY),
        )
        .ok_or(())?
    };
    if serialized_document_node
        && document.family() != DocumentFamily::Legacy
        && crate::native::document_element(document.pointer()).is_some()
        && bytes.last() == Some(&b'\n')
    {
        bytes.pop();
    }
    Ok(DispatchResult::bytes(bytes))
}

/// Serializes one legacy HTML document or same-document node with HTML4 rules.
pub(super) fn serialize_legacy_html(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    if document.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let node_handle = if request.values.is_empty() {
        None
    } else {
        request.optional_bridge_handle(0)?
    };
    let node_pointer = if let Some(node_handle) = node_handle {
        let (node_pointer, node_graph, _) =
            receiver_pointer_and_graph(context, node_handle)?;
        if !Rc::ptr_eq(&document.graph(), &node_graph) {
            return Ok(dom_exception(4));
        }
        Some(node_pointer)
    } else {
        None
    };
    let bytes = crate::native::document_serialize_html4(
        document.pointer(),
        node_pointer,
        document.format_output(),
    )
    .ok_or(())?;
    Ok(DispatchResult::bytes(bytes))
}

/// Serializes one modern HTML document or same-document node with HTML5 rules.
pub(super) fn serialize_html(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    if document.family() != DocumentFamily::ModernHtml {
        return Err(());
    }
    let node_handle = if request.values.is_empty() {
        None
    } else {
        request.optional_bridge_handle(0)?
    };
    let node_pointer = if let Some(node_handle) = node_handle {
        let (node_pointer, node_graph, _) =
            receiver_pointer_and_graph(context, node_handle)?;
        if !Rc::ptr_eq(&document.graph(), &node_graph) {
            return Ok(dom_exception(4));
        }
        Some(node_pointer)
    } else {
        None
    };
    let bytes = crate::native::document_serialize_html5(
        document.pointer(),
        node_pointer,
    )
    .ok_or(())?;
    Ok(DispatchResult::bytes(bytes))
}

/// Returns one document's XML version bytes.
pub(super) fn version(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let document = document(context, request.header.receiver)?;
    let version = crate::native::document_version(document.pointer()).ok_or(())?;
    Ok(DispatchResult::bytes(version))
}

/// Returns one document's encoding bytes or PHP null when it has none.
pub(super) fn encoding(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let document = document(context, request.header.receiver)?;
    Ok(match crate::native::document_encoding(document.pointer()) {
        Some(encoding) => DispatchResult::bytes(encoding),
        None => DispatchResult::null(),
    })
}

/// Returns one document's libxml standalone integer flag.
pub(super) fn standalone(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let document = document(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(
        crate::native::document_standalone(document.pointer()) == 1,
    ))
}

/// Creates one detached legacy or modern element and returns its canonical node handle.
pub(super) fn create_element(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let document = document(context, request.header.receiver)?;
    let family = document.family();
    let document_pointer = document.pointer();
    let graph = document.graph();
    let name = request.byte_string(0)?;
    let value = match family {
        DocumentFamily::Legacy => {
            if request.values.len() > 2 {
                return Err(());
            }
            Some(optional_bytes(request, 1, b"")?)
        }
        DocumentFamily::ModernXml | DocumentFamily::ModernHtml => {
            if request.values.len() != 1 {
                return Err(());
            }
            None
        }
    };
    let pointer = crate::native::document_create_element(
        document_pointer,
        name,
        value,
        family == DocumentFamily::ModernHtml,
    )
    .ok_or(())?;
    context.register_detached_root(pointer, Rc::clone(&graph));
    canonical_pointer_result(context, pointer, graph)
}

/// Creates one detached namespaced element with legacy or modern QName rules.
pub(super) fn create_element_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let document = document(context, request.header.receiver)?;
    let family = document.family();
    let value = match family {
        DocumentFamily::Legacy => {
            if !(2..=3).contains(&request.values.len()) {
                return Err(());
            }
            Some(optional_bytes(request, 2, b"")?)
        }
        DocumentFamily::ModernXml | DocumentFamily::ModernHtml => {
            if request.values.len() != 2 {
                return Err(());
            }
            None
        }
    };
    let outcome = crate::native::document_create_element_ns(
        document.pointer(),
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
        value,
        family != DocumentFamily::Legacy,
    );
    if outcome.error_code != 0 {
        return Ok(dom_exception(outcome.error_code));
    }
    register_detached_node(
        context,
        document.graph(),
        outcome.pointer.ok_or(())?,
    )
}

/// Creates one detached unqualified attribute with an empty string value.
pub(super) fn create_attribute(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let Some(pointer) = crate::native::document_create_attribute(
        document.pointer(),
        request.byte_string(0)?,
    ) else {
        return Ok(dom_exception(5));
    };
    register_detached_node(context, document.graph(), pointer)
}

/// Creates one detached namespaced attribute with family-specific QName rules.
pub(super) fn create_attribute_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let outcome = crate::native::document_create_attribute_ns(
        document.pointer(),
        request.optional_byte_string(0)?,
        request.byte_string(1)?,
        document.family() != DocumentFamily::Legacy,
    );
    if outcome.error_code != 0 {
        return Ok(dom_exception(outcome.error_code));
    }
    register_detached_node(
        context,
        document.graph(),
        outcome.pointer.ok_or(())?,
    )
}

/// Creates one detached text node and returns its concrete canonical wrapper.
pub(super) fn create_text_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let pointer =
        crate::native::document_create_text(document.pointer(), request.byte_string(0)?)
            .ok_or(())?;
    let graph = document.graph();
    context.register_detached_root(pointer, Rc::clone(&graph));
    canonical_pointer_result(context, pointer, graph)
}

/// Creates one detached CDATA section and returns its canonical node handle.
pub(super) fn create_cdata_section(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let pointer =
        crate::native::document_create_cdata(document.pointer(), request.byte_string(0)?)
            .ok_or(())?;
    register_detached_node(context, document.graph(), pointer)
}

/// Creates one detached comment and returns its canonical node handle.
pub(super) fn create_comment(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let pointer =
        crate::native::document_create_comment(document.pointer(), request.byte_string(0)?)
            .ok_or(())?;
    register_detached_node(context, document.graph(), pointer)
}

/// Creates one detached empty document fragment and returns its canonical node handle.
pub(super) fn create_document_fragment(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let document = document(context, request.header.receiver)?;
    let pointer =
        crate::native::document_create_fragment(document.pointer()).ok_or(())?;
    register_detached_node(context, document.graph(), pointer)
}

/// Creates one detached processing instruction with legacy optional data semantics.
pub(super) fn create_processing_instruction(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if !(1..=2).contains(&request.values.len()) {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let pointer = crate::native::document_create_pi(
        document.pointer(),
        request.byte_string(0)?,
        optional_bytes(request, 1, b"")?,
    )
    .ok_or(())?;
    register_detached_node(context, document.graph(), pointer)
}

/// Creates one detached entity-reference node and returns its canonical handle.
pub(super) fn create_entity_reference(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let document = document(context, request.header.receiver)?;
    let pointer = crate::native::document_create_entity_reference(
        document.pointer(),
        request.byte_string(0)?,
    )
    .ok_or(())?;
    register_detached_node(context, document.graph(), pointer)
}

/// Returns the current root element or PHP null for an empty document.
pub(super) fn document_element(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let document = document(context, request.header.receiver)?;
    let Some(pointer) = crate::native::document_element(document.pointer()) else {
        return Ok(DispatchResult::null());
    };
    let graph = document.graph();
    canonical_pointer_result(context, pointer, graph)
}

/// Returns PHP's family-specific implementation wrapper with its identity policy.
pub(super) fn implementation(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let document = document(context, request.header.receiver)?;
    let pointer = document.pointer();
    let family = document.family();
    if family != DocumentFamily::Legacy {
        if let Some(handle) = context.implementation_handles.get(&pointer).copied() {
            let valid = context
                .native_objects
                .get(handle, HANDLE_IMPLEMENTATION)
                .ok()
                .and_then(NativeObject::implementation)
                .is_some_and(|implementation| {
                    implementation.family() == family
                        && implementation.associated_document() == Some(pointer)
                });
            if valid {
                return Ok(DispatchResult::bridge_handle(handle));
            }
            context.implementation_handles.remove(&pointer);
        }
    }
    let associated_document = (family != DocumentFamily::Legacy).then_some(pointer);
    let handle = context.native_objects.insert(
        HANDLE_IMPLEMENTATION,
        NativeObject::Implementation(ImplementationObject::new(
            family,
            associated_document,
        )),
    );
    if associated_document.is_some() {
        context.implementation_handles.insert(pointer, handle);
    }
    Ok(DispatchResult::bridge_handle(handle))
}

/// Returns one document's canonical doctype wrapper or PHP null.
pub(super) fn doctype(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = document(context, request.header.receiver)?;
    let graph = target.graph();
    let Some(pointer) = crate::native::document_doctype(target.pointer()) else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, pointer, graph)
}

/// Returns one document's URI with the family-specific absent default.
pub(super) fn uri(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let target = document(context, request.header.receiver)?;
    Ok(match crate::native::document_url(target.pointer()) {
        Some(uri) => DispatchResult::bytes(uri),
        None if target.family() == DocumentFamily::Legacy => {
            DispatchResult::null()
        }
        None => DispatchResult::bytes(b"about:blank".to_vec()),
    })
}

/// Replaces one document's URI, mapping PHP null to the empty string.
pub(super) fn set_uri(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let uri = request.optional_byte_string(0)?.unwrap_or_default();
    let target = document(context, request.header.receiver)?;
    if !crate::native::document_set_url(target.pointer(), uri) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Imports one legacy or modern node into this document without attaching the clone.
pub(super) fn import_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let source_handle = request.bridge_handle(0)?;
    let deep = if request.values.len() == 2 {
        request.boolean(1)?
    } else {
        false
    };
    let target = document(context, request.header.receiver)?;
    let target_pointer = target.pointer();
    let target_graph = target.graph();
    let modern = target.family() != DocumentFamily::Legacy;
    let (source_pointer, source_graph, _) =
        receiver_pointer_and_graph(context, source_handle)?;
    let same_document = Rc::ptr_eq(&target_graph, &source_graph);
    let outcome = crate::native::document_import_node(
        target_pointer,
        source_pointer,
        deep,
        modern,
    );
    match outcome.error_code {
        0 => {
            let imported = outcome.pointer.ok_or(())?;
            if !same_document {
                context.register_detached_root(
                    imported,
                    Rc::clone(&target_graph),
                );
            }
            canonical_pointer_result(context, imported, target_graph)
        }
        9 if modern => Ok(dom_exception(9)),
        9 => Ok(DispatchResult::boolean(false)),
        11 if modern => Ok(dom_exception(11)),
        _ if !modern => Ok(DispatchResult::boolean(false)),
        _ => Err(()),
    }
}

/// Adopts one node into this document and rehomes every existing subtree wrapper.
pub(super) fn adopt_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let source_handle = request.bridge_handle(0)?;
    let target = document(context, request.header.receiver)?;
    let target_pointer = target.pointer();
    let target_graph = target.graph();
    let modern = target.family() != DocumentFamily::Legacy;
    let (source_pointer, _, source_is_document) =
        receiver_pointer_and_graph(context, source_handle)?;
    let outcome = crate::native::document_adopt_node(
        target_pointer,
        source_pointer,
        modern,
    );
    match outcome.error_code {
        0 => {
            if source_is_document || outcome.pointer != Some(source_pointer) {
                return Err(());
            }
            rehome_subtree_handles(
                context,
                source_pointer,
                Rc::clone(&target_graph),
            )?;
            context.attach_detached_root(source_pointer);
            context.register_detached_root(
                source_pointer,
                Rc::clone(&target_graph),
            );
            canonical_pointer_result(context, source_pointer, target_graph)
        }
        9 => Ok(dom_exception(9)),
        11 if modern => Ok(dom_exception(11)),
        _ if !modern => Ok(DispatchResult::boolean(false)),
        _ => Err(()),
    }
}

/// Returns the connected element whose attribute is typed as the requested XML ID.
pub(super) fn element_by_id(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let id = request.byte_string(0)?;
    let target = document(context, request.header.receiver)?;
    let pointer = target.pointer();
    let graph = target.graph();
    let Some(element) =
        crate::native::document_get_element_by_id(pointer, id)
    else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, element, graph)
}

/// Returns one document wrapper's serialization-format flag.
pub(super) fn format_output(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    Ok(DispatchResult::boolean(
        document(context, request.header.receiver)?.format_output(),
    ))
}

/// Updates one document wrapper's serialization-format flag.
pub(super) fn set_format_output(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let value = request.boolean(0)?;
    document_mut(context, request.header.receiver)?.set_format_output(value);
    Ok(DispatchResult::null())
}

/// Inserts one empty native document and returns its generation-checked handle.
fn insert(
    context: &mut Context,
    version: &[u8],
    encoding: &[u8],
    family: DocumentFamily,
) -> Result<DispatchResult, ()> {
    let pointer = crate::native::document_new(version, encoding).ok_or(())?;
    if family == DocumentFamily::ModernXml
        && !crate::native::document_convert_modern_xml(pointer)
    {
        unsafe {
            crate::native::document_free(pointer);
        }
        return Err(());
    }
    Ok(insert_pointer(context, pointer, family))
}

/// Takes ownership of one fully initialized native document and registers its wrapper.
pub(super) fn insert_pointer(
    context: &mut Context,
    pointer: usize,
    family: DocumentFamily,
) -> DispatchResult {
    let handle = context.native_objects.insert(
        HANDLE_DOCUMENT,
        NativeObject::Document(DocumentObject::new(pointer, family)),
    );
    context.document_handles.insert(pointer, handle);
    DispatchResult::bridge_handle(handle)
}
