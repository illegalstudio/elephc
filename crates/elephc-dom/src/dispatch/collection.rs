//! Purpose:
//! Dispatches DOM `NodeList`, `HTMLCollection`, and `NamedNodeMap` construction and lookup.
//! Re-evaluates live descriptors while preserving static selector snapshots.
//!
//! Called from:
//! - `super::routes::dispatch()` for collection properties and methods.
//!
//! Key details:
//! - Collection wrappers are intentionally fresh objects while member wrappers stay canonical.
//! - Query descriptors retain their authoritative document graph without copying nodes.
//! - Selector snapshots preserve pointer order while canonicalizing wrappers on access.

use crate::context::Context;
use crate::native::DtdTableKind;
use crate::objects::CollectionKind;
use crate::request::Request;

use super::{
    canonical_namespace_node_result, canonical_pointer_result, collection,
    collection_mut, collection_result, notation_pointer_result,
    receiver_pointer_and_graph, require_no_values, DispatchResult,
};

/// Creates one fresh live direct-child `NodeList`.
pub(super) fn child_nodes(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::ChildNodes,
    ))
}

/// Creates one fresh live direct-child element `HTMLCollection`.
pub(super) fn children(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::ChildElements,
    ))
}

/// Creates one live descendant-element query by qualified tag name.
pub(super) fn elements_by_tag_name(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?.to_vec();
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::ElementsByTagName { name },
    ))
}

/// Creates one live descendant-element query by namespace URI and local name.
pub(super) fn elements_by_tag_name_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let namespace_uri = request
        .optional_byte_string(0)?
        .map(|value| value.to_vec());
    let local_name = request.byte_string(1)?.to_vec();
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::ElementsByTagNameNs {
            namespace_uri,
            local_name,
        },
    ))
}

/// Creates one fresh live `NamedNodeMap` for an element's attributes.
pub(super) fn attributes(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (root, graph, _) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if crate::native::node_type(root) != 1 {
        return Ok(DispatchResult::null());
    }
    Ok(collection_result(
        context,
        root,
        graph,
        CollectionKind::Attributes,
    ))
}

/// Returns the collection's current live length.
pub(super) fn length(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let collection = collection(context, request.header.receiver)?;
    let length = collection_length(collection);
    Ok(DispatchResult::integer(
        i64::try_from(length).map_err(|_| ())?,
    ))
}

/// Returns one canonical collection member by zero-based index or PHP null.
pub(super) fn item(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let Ok(index) = usize::try_from(request.integer(0)?) else {
        return Ok(DispatchResult::null());
    };
    let (pointer, graph, allocation) = {
        let collection = collection_mut(context, request.header.receiver)?;
        let allocation = collection.namespace_allocation(index);
        (
            collection_item(collection, index),
            collection.member_document(index),
            allocation,
        )
    };
    let Some(pointer) = pointer else {
        return Ok(DispatchResult::null());
    };
    if let Some(allocation) = allocation {
        return Ok(canonical_namespace_node_result(
            context,
            pointer,
            graph,
            Some(allocation),
        ));
    }
    if matches!(
        collection_kind_of(context, request.header.receiver)?,
        CollectionKind::DtdNotations
    ) {
        return Ok(notation_pointer_result(context, pointer, graph));
    }
    canonical_pointer_result(context, pointer, graph)
}

/// Returns one attribute map member by qualified name or PHP null.
pub(super) fn get_named_item(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if matches!(
        collection_kind_of(context, request.header.receiver)?,
        CollectionKind::DtdEntities | CollectionKind::DtdNotations
    ) {
        return dtd_get_named_item(context, request);
    }
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let (pointer, graph) = {
        let collection = collection(context, request.header.receiver)?;
        if !matches!(collection.kind(), CollectionKind::Attributes) {
            return Err(());
        }
        if collection.root_is_invalidated() {
            return Ok(DispatchResult::null());
        }
        (
            crate::native::element_get_attribute_node(collection.root(), name),
            collection.document(),
        )
    };
    let Some(pointer) = pointer else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, pointer, graph)
}

/// Returns one attribute map member by namespace URI and local name or PHP null.
pub(super) fn get_named_item_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if matches!(
        collection_kind_of(context, request.header.receiver)?,
        CollectionKind::DtdEntities | CollectionKind::DtdNotations
    ) {
        return dtd_get_named_item_ns(context, request);
    }
    if request.values.len() != 2 {
        return Err(());
    }
    let namespace_uri = request.optional_byte_string(0)?;
    let local_name = request.byte_string(1)?;
    let (pointer, graph) = {
        let collection = collection(context, request.header.receiver)?;
        if !matches!(collection.kind(), CollectionKind::Attributes) {
            return Err(());
        }
        if collection.root_is_invalidated() {
            return Ok(DispatchResult::null());
        }
        (
            crate::native::element_get_attribute_node_ns(
                collection.root(),
                namespace_uri,
                local_name,
            ),
            collection.document(),
        )
    };
    let Some(pointer) = pointer else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, pointer, graph)
}

/// Returns the first HTML-collection element whose `id` or `name` matches.
pub(super) fn named_item(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    if name.is_empty() {
        return Ok(DispatchResult::null());
    }
    let (pointer, graph) = {
        let collection = collection(context, request.header.receiver)?;
        let pointer = (0..collection_length(collection))
            .filter_map(|index| collection_item(collection, index))
            .find(|pointer| {
                let id_matches =
                    crate::native::element_get_attribute(*pointer, b"id")
                        .is_some_and(|value| value == name);
                let html_name_matches =
                    crate::native::node_namespace_uri(*pointer).as_deref()
                        == Some(b"http://www.w3.org/1999/xhtml")
                    && crate::native::element_get_attribute(*pointer, b"name")
                        .is_some_and(|value| value == name);
                id_matches || html_name_matches
            });
        (pointer, collection.document())
    };
    let Some(pointer) = pointer else {
        return Ok(DispatchResult::null());
    };
    canonical_pointer_result(context, pointer, graph)
}

/// Computes one live descriptor's current member count.
fn collection_length(collection: &crate::objects::CollectionObject) -> usize {
    if collection.root_is_invalidated()
        && !matches!(collection.kind(), CollectionKind::Snapshot { .. })
    {
        return 0;
    }
    match collection.kind() {
        CollectionKind::ChildNodes => {
            crate::native::node_child_count(collection.root())
        }
        CollectionKind::ChildElements => usize::try_from(
            crate::native::element_child_count(collection.root()),
        )
        .unwrap_or(0),
        CollectionKind::ElementsByTagName { name } => {
            crate::native::descendant_element_count_name(
                collection.root(),
                name,
                collection.family() == crate::objects::DocumentFamily::Legacy,
            )
        }
        CollectionKind::ElementsByTagNameNs {
            namespace_uri,
            local_name,
        } => crate::native::descendant_element_count_ns(
            collection.root(),
            namespace_uri.as_deref(),
            local_name,
        ),
        CollectionKind::ElementsByClassName {
            names,
            full_quirks,
        } => super::class_collection::length(
            collection.root(),
            names,
            *full_quirks,
        ),
        CollectionKind::Snapshot { pointers, .. } => pointers.len(),
        CollectionKind::Attributes => {
            crate::native::element_attribute_count(collection.root())
        }
        CollectionKind::DtdEntities => crate::native::document_type_dtd_table_size(
            collection.root(),
            crate::native::DtdTableKind::Entities,
        ),
        CollectionKind::DtdNotations => crate::native::document_type_dtd_table_size(
            collection.root(),
            crate::native::DtdTableKind::Notations,
        ),
    }
}

/// Resolves one live descriptor's member pointer by zero-based index.
fn collection_item(
    collection: &crate::objects::CollectionObject,
    index: usize,
) -> Option<usize> {
    if collection.root_is_invalidated()
        && !matches!(collection.kind(), CollectionKind::Snapshot { .. })
    {
        return None;
    }
    match collection.kind() {
        CollectionKind::ChildNodes => {
            crate::native::node_child_at(collection.root(), index)
        }
        CollectionKind::ChildElements => {
            let mut element =
                crate::native::element_first_child(collection.root());
            for _ in 0..index {
                element = element.and_then(crate::native::element_next_sibling);
            }
            element
        }
        CollectionKind::ElementsByTagName { name } => {
            crate::native::descendant_element_at_name(
                collection.root(),
                index,
                name,
                collection.family() == crate::objects::DocumentFamily::Legacy,
            )
        }
        CollectionKind::ElementsByTagNameNs {
            namespace_uri,
            local_name,
        } => crate::native::descendant_element_at_ns(
            collection.root(),
            index,
            namespace_uri.as_deref(),
            local_name,
        ),
        CollectionKind::ElementsByClassName {
            names,
            full_quirks,
        } => super::class_collection::item(
            collection.root(),
            names,
            *full_quirks,
            index,
        ),
        CollectionKind::Snapshot { pointers, .. } => {
            pointers.get(index).copied().flatten()
        }
        CollectionKind::Attributes => {
            crate::native::element_attribute_at(collection.root(), index)
        }
        CollectionKind::DtdEntities => crate::native::document_type_dtd_table_at(
            collection.root(),
            crate::native::DtdTableKind::Entities,
            index,
        ),
        CollectionKind::DtdNotations => {
            let raw = crate::native::document_type_dtd_table_at(
                collection.root(),
                crate::native::DtdTableKind::Notations,
                index,
            );
            raw.and_then(crate::native::notation_synthesize)
        }
    }
}

/// Returns one DTD named-node-map member by its declared name or PHP null.
///
/// The libxml2 DTD table lookup returns canonical entity nodes and synthesizes
/// a fresh fake `XML_NOTATION_NODE` wrapper for every notation result.
pub(super) fn dtd_get_named_item(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let name = request.byte_string(0)?;
    let (kind, pointer, graph) = {
        let collection = collection(context, request.header.receiver)?;
        let kind = dtd_table_kind(collection.kind())?;
        if collection.root_is_invalidated() {
            return Ok(DispatchResult::null());
        }
        let raw = crate::native::document_type_dtd_table_lookup(
            collection.root(),
            kind,
            name,
        );
        let pointer = if kind == DtdTableKind::Notations {
            raw.and_then(crate::native::notation_synthesize)
        } else {
            raw
        };
        (kind, pointer, collection.document())
    };
    let Some(pointer) = pointer else {
        return Ok(DispatchResult::null());
    };
    if kind == DtdTableKind::Notations {
        return Ok(notation_pointer_result(context, pointer, graph));
    }
    canonical_pointer_result(context, pointer, graph)
}

/// Returns one DTD named-node-map member by namespace URI and local name or PHP null.
pub(super) fn dtd_get_named_item_ns(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let namespace_uri = request.optional_byte_string(0)?;
    let local_name = request.byte_string(1)?;
    let _ = namespace_uri;
    let (kind, pointer, graph) = {
        let collection = collection(context, request.header.receiver)?;
        let kind = dtd_table_kind(collection.kind())?;
        if collection.root_is_invalidated() {
            return Ok(DispatchResult::null());
        }
        let raw = crate::native::document_type_dtd_table_lookup(
            collection.root(),
            kind,
            local_name,
        );
        let pointer = if kind == DtdTableKind::Notations {
            raw.and_then(crate::native::notation_synthesize)
        } else {
            raw
        };
        (kind, pointer, collection.document())
    };
    let Some(pointer) = pointer else {
        return Ok(DispatchResult::null());
    };
    if kind == DtdTableKind::Notations {
        return Ok(notation_pointer_result(context, pointer, graph));
    }
    canonical_pointer_result(context, pointer, graph)
}

/// Maps one supported DTD named-node-map kind to its underlying libxml2 table kind.
fn dtd_table_kind(kind: &CollectionKind) -> Result<DtdTableKind, ()> {
    match kind {
        CollectionKind::DtdEntities => Ok(DtdTableKind::Entities),
        CollectionKind::DtdNotations => Ok(DtdTableKind::Notations),
        _ => Err(()),
    }
}

/// Borrows one validated DTD named-node-map collection and returns its kind.
fn collection_kind_of(
    context: &Context,
    handle: u64,
) -> Result<&CollectionKind, ()> {
    Ok(collection(context, handle)?.kind())
}
