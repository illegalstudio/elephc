//! Purpose:
//! Implements bidirectional DOM and SimpleXML imports over one shared document graph.
//! Enforces php-src's document-wide legacy/modern representation claim.
//!
//! Called from:
//! - `super::super::routes::dispatch()` for the three public interop functions.
//!
//! Key details:
//! - SimpleXML imports always mint fresh wrappers; DOM imports use canonical caches.
//! - Only element/attribute nodes enter DOM, while SimpleXML accepts document roots.
//! - Conflicting DOM API generations fail before native wrapper materialization.

use std::rc::Rc;

use crate::context::Context;
use crate::objects::{
    DocumentGraph, DomApiFamily, DomClaimError, SimpleXmlIteratorState,
    SimpleXmlObject, HANDLE_DOCUMENT, HANDLE_NODE, HANDLE_SIMPLEXML,
};
use crate::request::Request;

use super::super::{
    canonical_document_handle, canonical_pointer_handle, document, node,
    require_no_receiver, DispatchResult,
};

/// libxml2's public element-node type value.
const XML_ELEMENT_NODE: u32 = 1;
/// libxml2's public attribute-node type value.
const XML_ATTRIBUTE_NODE: u32 = 2;

/// Validation outcome for php-src's broader SimpleXML import source surface.
enum SimpleXmlSource {
    Valid(usize, Rc<DocumentGraph>),
    Documentless,
    InvalidNodeType,
    InvalidObject,
}

/// One validated DOM import source, including an existing canonical wrapper.
struct DomSource {
    pointer: usize,
    graph: Rc<DocumentGraph>,
    existing_wrapper: Option<(u64, u64)>,
}

/// Imports one DOM/libxml object into a fresh SimpleXMLElement view.
pub(in crate::dispatch) fn import_dom(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    if request.values.is_empty() || request.values.len() > 2 {
        return Err(());
    }
    let class_kind = match super::resolve_class_kind(
        context,
        super::optional_nullable_bytes(request, 1)?,
        b"simplexml_import_dom()",
    ) {
        Ok(class_kind) => class_kind,
        Err(result) => return Ok(result),
    };
    let Some(handle) = request
        .bridge_handle(0)
        .ok()
        .filter(|handle| crate::handles::handle_kind(*handle).is_ok())
    else {
        return Ok(DispatchResult::type_error(
            b"simplexml_import_dom(): Argument #1 ($node) must be a valid XML node",
        ));
    };
    let (pointer, graph) = match simplexml_source(context, handle)? {
        SimpleXmlSource::Valid(pointer, graph) => (pointer, graph),
        SimpleXmlSource::Documentless => {
            return Ok(DispatchResult::null().with_warning(
                b"Warning: simplexml_import_dom(): Imported Node must have associated Document\n",
            ));
        }
        SimpleXmlSource::InvalidNodeType => {
            return Ok(DispatchResult::null().with_warning(
                b"Warning: simplexml_import_dom(): Invalid Nodetype to import\n",
            ));
        }
        SimpleXmlSource::InvalidObject => {
            return Ok(DispatchResult::type_error(
                b"simplexml_import_dom(): Argument #1 ($node) must be a valid XML node",
            ));
        }
    };
    let object = SimpleXmlObject::new(
        pointer,
        graph,
        class_kind,
        SimpleXmlIteratorState::direct(None, false),
    );
    Ok(super::fresh_result(context, object))
}

/// Resolves one libxml-backed source to the element SimpleXML may expose.
fn simplexml_source(
    context: &Context,
    handle: u64,
) -> Result<SimpleXmlSource, ()> {
    match crate::handles::handle_kind(handle) {
        Ok(HANDLE_DOCUMENT) => {
            let Ok(document) = document(context, handle) else {
                return Ok(SimpleXmlSource::InvalidObject);
            };
            let Some(root) = crate::native::document_element(document.pointer()) else {
                return Ok(SimpleXmlSource::InvalidNodeType);
            };
            Ok(SimpleXmlSource::Valid(root, document.graph()))
        }
        Ok(HANDLE_NODE) => {
            let Ok(node) = node(context, handle) else {
                return Ok(SimpleXmlSource::InvalidObject);
            };
            let pointer = node.pointer();
            if !node.owner_document_exposed()
                || crate::native::node_document(pointer).is_none()
            {
                return Ok(SimpleXmlSource::Documentless);
            }
            if crate::native::node_type(pointer) != XML_ELEMENT_NODE {
                return Ok(SimpleXmlSource::InvalidNodeType);
            }
            Ok(SimpleXmlSource::Valid(pointer, node.document()))
        }
        Ok(HANDLE_SIMPLEXML) => {
            let Ok(simplexml) = super::object(context, handle) else {
                return Ok(SimpleXmlSource::InvalidObject);
            };
            let Some(pointer) = super::exported_pointer(simplexml) else {
                return Ok(SimpleXmlSource::InvalidObject);
            };
            if crate::native::node_document(pointer).is_none() {
                return Ok(SimpleXmlSource::Documentless);
            }
            if crate::native::node_type(pointer) != XML_ELEMENT_NODE {
                return Ok(SimpleXmlSource::InvalidNodeType);
            }
            Ok(SimpleXmlSource::Valid(pointer, simplexml.document()))
        }
        _ => Ok(SimpleXmlSource::InvalidObject),
    }
}

/// Imports one element or attribute into the legacy DOM API.
pub(in crate::dispatch) fn import_legacy(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    import_dom_family(
        context,
        request,
        DomApiFamily::Legacy,
        b"dom_import_simplexml()",
        b"a Dom\\Node",
    )
}

/// Imports one element or attribute into the modern `Dom` API.
pub(in crate::dispatch) fn import_modern(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    import_dom_family(
        context,
        request,
        DomApiFamily::Modern,
        b"Dom\\import_simplexml()",
        b"a DOMNode",
    )
}

/// Claims the source document and returns its canonical DOM wrapper.
fn import_dom_family(
    context: &mut Context,
    request: &Request,
    requested: DomApiFamily,
    callable: &[u8],
    conflicting_class: &[u8],
) -> Result<DispatchResult, ()> {
    require_no_receiver(request)?;
    if request.values.len() != 1 {
        return Err(());
    }
    let Some(handle) = request.bridge_handle(0).ok() else {
        return Ok(invalid_dom_node_type(callable));
    };
    let Some(source) = dom_source(context, handle)? else {
        return Ok(invalid_dom_node_type(callable));
    };
    let family = match source.graph.claim_dom_api(requested) {
        Ok(family) => family,
        Err(DomClaimError::ConflictingFamily) => {
            return Ok(DispatchResult::type_error(
                &[
                    callable,
                    b": Argument #1 ($node) must not be already imported as ",
                    conflicting_class,
                ]
                .concat(),
            ));
        }
        Err(DomClaimError::ModernConversionFailed) => return Err(()),
    };
    debug_assert_eq!(source.graph.family(), family);
    canonical_document_handle(context, Rc::clone(&source.graph));
    if let Some((handle, kind)) = source.existing_wrapper {
        return Ok(DispatchResult::typed_bridge_handle(handle, kind));
    }
    let (handle, kind) =
        canonical_pointer_handle(context, source.pointer, source.graph)?;
    Ok(DispatchResult::typed_bridge_handle(handle, kind))
}

/// Resolves an element/attribute source and its authoritative document graph.
fn dom_source(
    context: &Context,
    handle: u64,
) -> Result<Option<DomSource>, ()> {
    match crate::handles::handle_kind(handle) {
        Ok(HANDLE_NODE) => {
            let Ok(node) = node(context, handle) else {
                return Ok(None);
            };
            let pointer = node.pointer();
            let node_type = crate::native::node_type(pointer);
            if !node.owner_document_exposed()
                || crate::native::node_document(pointer).is_none()
                || !matches!(node_type, XML_ELEMENT_NODE | XML_ATTRIBUTE_NODE)
            {
                return Ok(None);
            }
            let graph = node.document();
            let kind = node.wrapper_kind();
            Ok(Some(DomSource {
                pointer,
                graph,
                existing_wrapper: Some((handle, kind)),
            }))
        }
        Ok(HANDLE_SIMPLEXML) => {
            let Ok(simplexml) = super::object(context, handle) else {
                return Ok(None);
            };
            let Some(pointer) = super::exported_pointer(simplexml) else {
                return Ok(None);
            };
            let node_type = crate::native::node_type(pointer);
            if crate::native::node_document(pointer).is_none()
                || !matches!(node_type, XML_ELEMENT_NODE | XML_ATTRIBUTE_NODE)
            {
                return Ok(None);
            }
            Ok(Some(DomSource {
                pointer,
                graph: simplexml.document(),
                existing_wrapper: None,
            }))
        }
        _ => Ok(None),
    }
}

/// Builds php-src's exact invalid DOM import `TypeError`.
fn invalid_dom_node_type(callable: &[u8]) -> DispatchResult {
    DispatchResult::type_error(
        &[callable, b": Argument #1 ($node) is not a valid node type"].concat(),
    )
}
