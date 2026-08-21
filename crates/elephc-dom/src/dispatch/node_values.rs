//! Purpose:
//! Dispatches scalar DOM node metadata, namespace lookup, content writes, and cloning.
//! Preserves legacy versus modern node-value semantics over the shared libxml2 graph.
//!
//! Called from:
//! - `super::dispatch()` for node scalar properties and methods.
//!
//! Key details:
//! - Nullable native buffers map to PHP null except for legacy prefix's empty string.
//! - Clones remain detached roots while sharing the authoritative document allocation.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{DocumentFamily, LegacyDocumentFlag};
use crate::request::Request;

use super::{
    canonical_pointer_result, receiver_pointer_and_graph, require_no_values,
    DispatchResult,
};

/// Returns one node's namespace URI or PHP null when it has no namespace.
pub(super) fn namespace_uri(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(optional_bytes_result(crate::native::node_namespace_uri(pointer)))
}

/// Returns one legacy node's namespace prefix, using PHP's empty-string fallback.
pub(super) fn prefix(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(
        crate::native::node_prefix(pointer).unwrap_or_default(),
    ))
}

/// Returns one modern element or attribute prefix, or PHP null when unprefixed.
pub(super) fn optional_prefix(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(optional_bytes_result(crate::native::node_prefix(pointer)))
}

/// Rebinds one legacy element or attribute namespace prefix in place.
pub(super) fn set_prefix(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let prefix = request.byte_string(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if graph.family() != DocumentFamily::Legacy {
        return Err(());
    }
    Ok(match crate::native::node_set_prefix(pointer, prefix) {
        0 => DispatchResult::null(),
        14 if !graph.legacy_flag(LegacyDocumentFlag::StrictErrorChecking) => {
            DispatchResult::null()
                .with_warning(b"Warning: Unknown: Namespace Error\n")
        }
        14 | 1401 => DispatchResult::dom_exception(14, b"Namespace Error"),
        _ => return Err(()),
    })
}

/// Returns one legacy element or attribute local name, or PHP null otherwise.
pub(super) fn local_name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(optional_bytes_result(crate::native::node_local_name(pointer)))
}

/// Returns one node's effective base URI or PHP null when libxml2 has none.
pub(super) fn base_uri(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(match graph.family() {
        DocumentFamily::Legacy => {
            optional_bytes_result(crate::native::node_base_uri(pointer))
        }
        DocumentFamily::ModernXml | DocumentFamily::ModernHtml => {
            let base_uri = crate::native::node_base_uri(pointer)
                .or_else(|| crate::native::document_url(graph.pointer()))
                .unwrap_or_else(|| b"about:blank".to_vec());
            DispatchResult::bytes(base_uri)
        }
    })
}

/// Returns PHP's family-specific `nodeValue` for one document or node.
pub(super) fn node_value(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let node_type = crate::native::node_type(pointer);
    let has_value = match graph.family() {
        DocumentFamily::Legacy => matches!(node_type, 1 | 2 | 3 | 4 | 7 | 8),
        DocumentFamily::ModernXml | DocumentFamily::ModernHtml => {
            matches!(node_type, 2 | 3 | 4 | 7 | 8)
        }
    };
    if !has_value {
        return Ok(DispatchResult::null());
    }
    Ok(DispatchResult::bytes(
        crate::native::node_content(pointer).unwrap_or_default(),
    ))
}

/// Replaces one writable node value with family- and concrete-wrapper semantics.
pub(super) fn set_node_value(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.optional_byte_string(0)?.unwrap_or_default();
    let (pointer, graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let node_type = crate::native::node_type(pointer);
    if graph.family() != DocumentFamily::Legacy
        && (is_document || !matches!(node_type, 2 | 3 | 4 | 7 | 8))
    {
        return Ok(readonly_property_error(
            graph.family(),
            node_type,
            is_document,
            "nodeValue",
        ));
    }
    if matches!(node_type, 1 | 2 | 3 | 4 | 7 | 8)
        && !crate::native::node_set_content(pointer, value)
    {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Replaces one attribute's writable value for both DOM API families.
pub(super) fn set_attribute_value(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.byte_string(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if crate::native::node_type(pointer) != 2
        || !crate::native::node_set_content(pointer, value)
    {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Replaces one writable node's text content while legacy documents remain no-ops.
pub(super) fn set_text_content(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.optional_byte_string(0)?.unwrap_or_default();
    let (pointer, graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let node_type = crate::native::node_type(pointer);
    if graph.family() == DocumentFamily::Legacy
        && matches!(node_type, 12 | 17)
    {
        return Ok(DispatchResult::null());
    }
    if graph.family() != DocumentFamily::Legacy
        && (is_document || !matches!(node_type, 1 | 2 | 3 | 4 | 7 | 8 | 11))
    {
        return Ok(readonly_property_error(
            graph.family(),
            node_type,
            is_document,
            "textContent",
        ));
    }
    if !is_document && !crate::native::node_set_content(pointer, value) {
        return Err(());
    }
    Ok(DispatchResult::null())
}

/// Builds php-src's concrete-wrapper message for one modern readonly node property.
fn readonly_property_error(
    family: DocumentFamily,
    node_type: u32,
    is_document: bool,
    property: &str,
) -> DispatchResult {
    let class = if is_document {
        match family {
            DocumentFamily::ModernXml => "Dom\\XMLDocument",
            DocumentFamily::ModernHtml => "Dom\\HTMLDocument",
            DocumentFamily::Legacy => "DOMDocument",
        }
    } else {
        match (family, node_type) {
            (DocumentFamily::ModernHtml, 1) => "Dom\\HTMLElement",
            (_, 1) => "Dom\\Element",
            (_, 2) => "Dom\\Attr",
            (_, 3) => "Dom\\Text",
            (_, 4) => "Dom\\CDATASection",
            (_, 5) => "Dom\\EntityReference",
            (_, 7) => "Dom\\ProcessingInstruction",
            (_, 8) => "Dom\\Comment",
            (_, 10 | 14) => "Dom\\DocumentType",
            (_, 11) => "Dom\\DocumentFragment",
            (_, 12) => "Dom\\Notation",
            (_, 15 | 17) => "Dom\\Entity",
            _ => "Dom\\Node",
        }
    };
    DispatchResult::error(
        format!("Cannot modify readonly property {class}::${property}").as_bytes(),
    )
}

/// Returns one node's parser source line number.
pub(super) fn line(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if matches!(crate::native::node_type(pointer), 12 | 17) {
        return Ok(DispatchResult::integer(-1));
    }
    Ok(DispatchResult::integer(crate::native::node_line(pointer)))
}

/// Returns one node's absolute libxml2 path, with family-specific null fallback.
pub(super) fn path(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(match crate::native::node_path(pointer) {
        Some(path) => DispatchResult::bytes(path),
        None if graph.family() == DocumentFamily::Legacy => DispatchResult::null(),
        None => DispatchResult::dom_exception(11, b"Invalid State Error"),
    })
}

/// Reports whether one legacy node currently owns attributes.
pub(super) fn has_attributes(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if matches!(crate::native::node_type(pointer), 12 | 17) {
        return Ok(DispatchResult::boolean(false));
    }
    Ok(DispatchResult::boolean(
        crate::native::node_has_attributes(pointer),
    ))
}

/// Reports whether one attribute is typed as an XML ID.
pub(super) fn attribute_is_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::boolean(crate::native::attribute_is_id(pointer)))
}

/// Resolves an in-scope namespace URI for one nullable prefix.
pub(super) fn lookup_namespace_uri(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let prefix = request.optional_byte_string(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if matches!(crate::native::node_type(pointer), 12 | 17) {
        return Ok(DispatchResult::null());
    }
    Ok(optional_bytes_result(
        crate::native::node_lookup_namespace_uri(pointer, prefix),
    ))
}

/// Resolves one in-scope prefix for the supplied namespace URI.
pub(super) fn lookup_prefix(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let namespace_uri = request.byte_string(0)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if matches!(crate::native::node_type(pointer), 12 | 17) {
        return Ok(DispatchResult::null());
    }
    Ok(optional_bytes_result(crate::native::node_lookup_prefix(
        pointer,
        namespace_uri,
    )))
}

/// Reports whether the receiver's default namespace equals one supplied URI.
pub(super) fn is_default_namespace(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let namespace_uri = request.optional_byte_string(0)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    let deprecated_null =
        graph.family() == DocumentFamily::Legacy && namespace_uri.is_none();
    if matches!(crate::native::node_type(pointer), 12 | 17) {
        let result = DispatchResult::boolean(
            graph.family() != DocumentFamily::Legacy
                && namespace_uri.is_none(),
        );
        return Ok(if deprecated_null {
            result.with_warning(
                b"Deprecated: DOMNode::isDefaultNamespace(): Passing null to parameter #1 ($namespace) of type string is deprecated\n",
            )
        } else {
            result
        });
    }
    let current = crate::native::node_lookup_namespace_uri(pointer, None);
    let result = DispatchResult::boolean(
        current.as_deref() == namespace_uri,
    );
    Ok(if deprecated_null {
        result.with_warning(
            b"Deprecated: DOMNode::isDefaultNamespace(): Passing null to parameter #1 ($namespace) of type string is deprecated\n",
        )
    } else {
        result
    })
}

/// Implements the legacy DOM Level feature probe retained by PHP.
pub(super) fn is_supported(
    _context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let feature = request.byte_string(0)?;
    let version = request.byte_string(1)?;
    let xml_feature = feature.eq_ignore_ascii_case(b"xml");
    let supported_version =
        version.is_empty() || version == b"1.0" || version == b"2.0";
    Ok(DispatchResult::boolean(xml_feature && supported_version))
}

/// Clones one node shallowly or deeply and returns its concrete detached wrapper.
pub(super) fn clone_node(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    let deep = if request.values.is_empty() {
        false
    } else {
        request.boolean(0)?
    };
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if matches!(crate::native::node_type(pointer), 12 | 17) {
        return Ok(DispatchResult::boolean(false));
    }
    clone_receiver(context, request.header.receiver, deep)
}

/// Implements PHP object cloning for native document and node wrappers.
pub(super) fn clone_object(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    clone_receiver(context, request.header.receiver, true)
}

/// Clones one native receiver with document-specific graph ownership.
fn clone_receiver(
    context: &mut Context,
    receiver: u64,
    deep: bool,
) -> Result<DispatchResult, ()> {
    let (pointer, graph, is_document) =
        receiver_pointer_and_graph(context, receiver)?;
    if is_document {
        let clone =
            crate::native::document_clone(pointer, deep, graph.family())
                .ok_or(())?;
        let clone_graph = Rc::new(graph.replacement(clone));
        return canonical_pointer_result(context, clone, clone_graph);
    }
    let modern = graph.family() != DocumentFamily::Legacy;
    let clone = crate::native::node_clone(pointer, deep, modern).ok_or(())?;
    context.register_detached_root(clone, Rc::clone(&graph));
    canonical_pointer_result(context, clone, graph)
}

/// Maps one optional native byte buffer to a PHP string or null result.
fn optional_bytes_result(value: Option<Vec<u8>>) -> DispatchResult {
    match value {
        Some(value) => DispatchResult::bytes(value),
        None => DispatchResult::null(),
    }
}
