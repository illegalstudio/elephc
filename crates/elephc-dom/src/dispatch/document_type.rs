//! Purpose:
//! Dispatches scalar metadata properties for legacy and modern document-type nodes.
//! Keeps libxml2 DTD layout details behind the native adapter.
//!
//! Called from:
//! - `super::routes::dispatch()` for `DOMDocumentType` and `Dom\DocumentType`.
//!
//! Key details:
//! - Missing public and system identifiers are PHP empty strings.
//! - An absent internal declaration subset is represented as PHP null.
//! - `$entities` and `$notations` build fresh live `DtdNamedNodeMap` wrappers; entity
//!   members are canonical while notation members are freshly synthesized fake nodes.

use crate::context::Context;
use crate::native::DtdTableKind;
use crate::objects::CollectionKind;
use crate::request::Request;

use super::{
    collection_result, dom_exception, receiver_pointer_and_graph, require_no_values,
    DispatchResult,
};

/// Returns one doctype's declared name.
pub(super) fn name(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    bytes_field(context, request, crate::native::document_type_name)
}

/// Returns one doctype's public identifier.
pub(super) fn public_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    bytes_field(context, request, crate::native::document_type_public_id)
}

/// Returns one doctype's system identifier.
pub(super) fn system_id(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    bytes_field(context, request, crate::native::document_type_system_id)
}

/// Returns one doctype's serialized internal subset or PHP null.
pub(super) fn internal_subset(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(
        match crate::native::document_type_internal_subset(pointer) {
            Some(value) => DispatchResult::bytes(value),
            None => DispatchResult::null(),
        },
    )
}

/// Returns one mandatory byte-string doctype field.
fn bytes_field(
    context: &Context,
    request: &Request,
    accessor: fn(usize) -> Vec<u8>,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, _, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(DispatchResult::bytes(accessor(pointer)))
}

/// Returns one doctype's live `Dom\DtdNamedNodeMap` of declared entities.
pub(super) fn entities(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    dtd_table(context, request, DtdTableKind::Entities)
}

/// Returns one doctype's live `Dom\DtdNamedNodeMap` of declared notations.
pub(super) fn notations(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    dtd_table(context, request, DtdTableKind::Notations)
}

/// Builds one fresh live DTD named-node-map wrapper around the selected table.
fn dtd_table(
    context: &mut Context,
    request: &Request,
    kind: DtdTableKind,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (pointer, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if crate::native::node_document(pointer).is_none() {
        return Ok(dom_exception(11));
    }
    let collection_kind = match kind {
        DtdTableKind::Entities => CollectionKind::DtdEntities,
        DtdTableKind::Notations => CollectionKind::DtdNotations,
    };
    Ok(collection_result(context, pointer, graph, collection_kind))
}
