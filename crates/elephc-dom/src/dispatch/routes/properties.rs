//! Purpose:
//! Routes DOM virtual-property reads and writes to focused bridge handlers.
//! Keeps PHP-visible property semantics independent from method and lifecycle routing.
//!
//! Called from:
//! - `super::dispatch()` for generated `property-get` and `property-set` keys.
//!
//! Key details:
//! - Legacy document configuration flags preserve their distinct mutable storage.
//! - Wrapper-valued reads retain canonical native identity through their handlers.

use crate::context::Context;
use crate::handles::handle_kind;
use crate::objects::{LegacyDocumentFlag, HANDLE_NAMESPACE_NODE};
use crate::request::Request;

use super::super::{
    character_data, collection, document, document_config, document_html,
    document_type, element, element_markup, entity, namespace_node, node, node_values,
    require_no_values, reject_compiler_resident_operation, token_list, xpath,
    DispatchResult,
};

/// Executes one generated DOM virtual-property operation.
pub(super) fn dispatch(
    key: &str,
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let key = namespace_node_property_key(key, request);
    match key {
        "property-get:domnamespacenode::$nodeName" => {
            namespace_node::name(context, request)
        }
        "property-get:domnamespacenode::$nodeValue" => {
            namespace_node::node_value(context, request)
        }
        "property-get:domnamespacenode::$nodeType" => {
            namespace_node::node_type(context, request)
        }
        "property-get:domnamespacenode::$prefix" => {
            namespace_node::prefix(context, request)
        }
        "property-get:domnamespacenode::$localName" => {
            namespace_node::local_name(context, request)
        }
        "property-get:domnamespacenode::$namespaceURI" => {
            namespace_node::namespace_uri(context, request)
        }
        "property-get:domnamespacenode::$isConnected" => {
            namespace_node::is_connected(context, request)
        }
        "property-get:domnamespacenode::$ownerDocument" => {
            namespace_node::owner_document(context, request)
        }
        "property-get:domnamespacenode::$parentNode" => {
            namespace_node::parent_node(context, request)
        }
        "property-get:domnamespacenode::$parentElement" => {
            namespace_node::parent_element(context, request)
        }
        "property-get:domdocument::$version"
        | "property-get:domdocument::$xmlVersion"
        | "property-get:dom\\xmldocument::$xmlVersion" => {
            document::version(context, request)
        }
        "property-get:dom\\adjacentposition::$name"
        | "property-get:dom\\adjacentposition::$value" => {
            reject_compiler_resident_operation(request)
        }
        "property-get:domdocument::$encoding"
        | "property-get:domdocument::$actualEncoding"
        | "property-get:domdocument::$xmlEncoding"
        | "property-get:dom\\xmldocument::$xmlEncoding"
        | "property-get:dom\\document::$characterSet"
        | "property-get:dom\\document::$charset"
        | "property-get:dom\\document::$inputEncoding" => {
            document::encoding(context, request)
        }
        "property-get:dom\\document::$head" => {
            document_html::head(context, request)
        }
        "property-get:dom\\document::$body" => {
            document_html::body(context, request)
        }
        "property-get:dom\\document::$title" => {
            document_html::title(context, request)
        }
        "property-get:domdocument::$standalone"
        | "property-get:domdocument::$xmlStandalone"
        | "property-get:dom\\xmldocument::$xmlStandalone" => {
            document::standalone(context, request)
        }
        "property-get:domdocument::$documentElement"
        | "property-get:dom\\document::$documentElement" => {
            document::document_element(context, request)
        }
        "property-get:domxpath::$document"
        | "property-get:dom\\xpath::$document" => {
            xpath::document_property(context, request)
        }
        "property-get:domxpath::$registerNodeNamespaces"
        | "property-get:dom\\xpath::$registerNodeNamespaces" => {
            xpath::register_node_namespaces(context, request)
        }
        "property-get:domdocument::$implementation"
        | "property-get:dom\\document::$implementation" => {
            document::implementation(context, request)
        }
        "property-get:domdocument::$config" => {
            document_config::deprecated_config(context, request)
        }
        "property-get:domdocument::$doctype"
        | "property-get:dom\\document::$doctype" => {
            document::doctype(context, request)
        }
        "property-get:domdocument::$documentURI"
        | "property-get:dom\\document::$documentURI"
        | "property-get:dom\\document::$URL" => {
            document::uri(context, request)
        }
        "property-get:domdocument::$preserveWhiteSpace" => {
            document_config::get(
                context,
                request,
                LegacyDocumentFlag::PreserveWhitespace,
            )
        }
        "property-get:domdocument::$recover" => document_config::get(
            context,
            request,
            LegacyDocumentFlag::Recover,
        ),
        "property-get:domdocument::$resolveExternals" => {
            document_config::get(
                context,
                request,
                LegacyDocumentFlag::ResolveExternals,
            )
        }
        "property-get:domdocument::$strictErrorChecking" => {
            document_config::get(
                context,
                request,
                LegacyDocumentFlag::StrictErrorChecking,
            )
        }
        "property-get:domdocument::$substituteEntities" => {
            document_config::get(
                context,
                request,
                LegacyDocumentFlag::SubstituteEntities,
            )
        }
        "property-get:domdocument::$validateOnParse" => {
            document_config::get(
                context,
                request,
                LegacyDocumentFlag::ValidateOnParse,
            )
        }
        "property-get:domnode::$nodeName"
        | "property-get:dom\\node::$nodeName" => node::name(context, request),
        "property-get:domnode::$nodeType"
        | "property-get:dom\\node::$nodeType" => node::node_type(context, request),
        "property-get:domnode::$nodeValue"
        | "property-get:dom\\node::$nodeValue" => {
            node_values::node_value(context, request)
        }
        "property-get:domnode::$textContent"
        | "property-get:dom\\node::$textContent" => {
            node::text_content(context, request)
        }
        "property-get:domnode::$ownerDocument"
        | "property-get:dom\\node::$ownerDocument" => {
            node::owner_document(context, request)
        }
        "property-get:domnode::$parentNode"
        | "property-get:dom\\node::$parentNode" => node::parent(context, request),
        "property-get:domnode::$parentElement"
        | "property-get:dom\\node::$parentElement" => {
            node::parent_element(context, request)
        }
        "property-get:domnode::$firstChild"
        | "property-get:dom\\node::$firstChild" => {
            node::relative(context, request, crate::native::node_first_child)
        }
        "property-get:domnode::$lastChild"
        | "property-get:dom\\node::$lastChild" => {
            node::relative(context, request, crate::native::node_last_child)
        }
        "property-get:domnode::$previousSibling"
        | "property-get:dom\\node::$previousSibling" => node::relative(
            context,
            request,
            crate::native::node_previous_sibling,
        ),
        "property-get:domnode::$nextSibling"
        | "property-get:dom\\node::$nextSibling" => {
            node::relative(context, request, crate::native::node_next_sibling)
        }
        "property-get:domnode::$isConnected"
        | "property-get:dom\\node::$isConnected" => {
            node::is_connected(context, request)
        }
        "property-get:domnode::$childNodes"
        | "property-get:dom\\node::$childNodes" => {
            collection::child_nodes(context, request)
        }
        "property-get:domnode::$attributes"
        | "property-get:dom\\element::$attributes" => {
            collection::attributes(context, request)
        }
        "property-get:domattr::$schemaTypeInfo"
        | "property-get:domelement::$schemaTypeInfo" => {
            element::schema_type_info(context, request)
        }
        "property-get:domnode::$namespaceURI"
        | "property-get:dom\\element::$namespaceURI"
        | "property-get:dom\\attr::$namespaceURI" => {
            node_values::namespace_uri(context, request)
        }
        "property-get:domnode::$prefix" => node_values::prefix(context, request),
        "property-get:dom\\element::$prefix"
        | "property-get:dom\\attr::$prefix" => {
            node_values::optional_prefix(context, request)
        }
        "property-get:domnode::$localName"
        | "property-get:dom\\element::$localName"
        | "property-get:dom\\attr::$localName" => {
            node_values::local_name(context, request)
        }
        "property-get:domnode::$baseURI"
        | "property-get:dom\\node::$baseURI" => {
            node_values::base_uri(context, request)
        }
        "property-get:domcharacterdata::$data"
        | "property-get:dom\\characterdata::$data" => {
            character_data::data(context, request)
        }
        "property-get:domcharacterdata::$length"
        | "property-get:dom\\characterdata::$length" => {
            character_data::length(context, request)
        }
        "property-get:domtext::$wholeText"
        | "property-get:dom\\text::$wholeText" => {
            character_data::whole_text(context, request)
        }
        "property-set:domnode::$nodeValue"
        | "property-set:dom\\node::$nodeValue" => {
            node_values::set_node_value(context, request)
        }
        "property-set:domnode::$prefix" => {
            node_values::set_prefix(context, request)
        }
        "property-set:domnode::$textContent"
        | "property-set:dom\\node::$textContent" => {
            node_values::set_text_content(context, request)
        }
        "property-set:domcharacterdata::$data"
        | "property-set:dom\\characterdata::$data" => {
            character_data::set_data(context, request)
        }
        "property-get:domelement::$tagName"
        | "property-get:dom\\element::$tagName" => {
            element::tag_name(context, request)
        }
        "property-get:domelement::$id"
        | "property-get:dom\\element::$id" => element::id(context, request),
        "property-get:domelement::$className"
        | "property-get:dom\\element::$className" => {
            element::class_name(context, request)
        }
        "property-get:dom\\element::$classList" => {
            token_list::class_list(context, request)
        }
        "property-get:dom\\element::$innerHTML" => {
            element_markup::inner_html(context, request)
        }
        "property-get:dom\\element::$outerHTML" => {
            element_markup::outer_html(context, request)
        }
        "property-get:dom\\element::$substitutedNodeValue" => {
            element_markup::substituted_node_value(context, request)
        }
        "property-set:domelement::$id"
        | "property-set:dom\\element::$id" => {
            element::set_attribute_property(context, request, b"id")
        }
        "property-set:domelement::$className"
        | "property-set:dom\\element::$className" => {
            element::set_attribute_property(context, request, b"class")
        }
        "property-set:dom\\element::$innerHTML" => {
            element_markup::set_inner_html(context, request)
        }
        "property-set:dom\\element::$outerHTML" => {
            element_markup::set_outer_html(context, request)
        }
        "property-set:dom\\element::$substitutedNodeValue" => {
            element_markup::set_substituted_node_value(context, request)
        }
        "property-get:domelement::$firstElementChild"
        | "property-get:dom\\element::$firstElementChild"
        | "property-get:domdocument::$firstElementChild"
        | "property-get:dom\\document::$firstElementChild"
        | "property-get:domdocumentfragment::$firstElementChild"
        | "property-get:dom\\documentfragment::$firstElementChild" => element::relative(
            context,
            request,
            crate::native::element_first_child,
        ),
        "property-get:domelement::$lastElementChild"
        | "property-get:dom\\element::$lastElementChild"
        | "property-get:domdocument::$lastElementChild"
        | "property-get:dom\\document::$lastElementChild"
        | "property-get:domdocumentfragment::$lastElementChild"
        | "property-get:dom\\documentfragment::$lastElementChild" => element::relative(
            context,
            request,
            crate::native::element_last_child,
        ),
        "property-get:domelement::$previousElementSibling"
        | "property-get:domcharacterdata::$previousElementSibling"
        | "property-get:dom\\element::$previousElementSibling"
        | "property-get:dom\\characterdata::$previousElementSibling" => {
            element::relative(
                context,
                request,
                crate::native::element_previous_sibling,
            )
        }
        "property-get:domelement::$nextElementSibling"
        | "property-get:domcharacterdata::$nextElementSibling"
        | "property-get:dom\\element::$nextElementSibling"
        | "property-get:dom\\characterdata::$nextElementSibling" => element::relative(
            context,
            request,
            crate::native::element_next_sibling,
        ),
        "property-get:domelement::$childElementCount"
        | "property-get:dom\\element::$childElementCount"
        | "property-get:domdocument::$childElementCount"
        | "property-get:dom\\document::$childElementCount"
        | "property-get:domdocumentfragment::$childElementCount"
        | "property-get:dom\\documentfragment::$childElementCount" => {
            element::child_count(context, request)
        }
        "property-get:dom\\document::$children"
        | "property-get:dom\\documentfragment::$children"
        | "property-get:dom\\element::$children" => {
            collection::children(context, request)
        }
        "property-get:domattr::$name" => {
            node_values::local_name(context, request)
        }
        "property-get:dom\\attr::$name" => node::name(context, request),
        "property-get:domattr::$value"
        | "property-get:dom\\attr::$value" => {
            node_values::node_value(context, request)
        }
        "property-get:domattr::$ownerElement"
        | "property-get:dom\\attr::$ownerElement" => {
            node::parent_element(context, request)
        }
        "property-get:domattr::$specified"
        | "property-get:dom\\attr::$specified" => {
            require_no_values(request)?;
            Ok(DispatchResult::boolean(true))
        }
        "property-get:domdocumenttype::$name"
        | "property-get:dom\\documenttype::$name" => {
            document_type::name(context, request)
        }
        "property-get:domdocumenttype::$publicId"
        | "property-get:dom\\documenttype::$publicId" => {
            document_type::public_id(context, request)
        }
        "property-get:domdocumenttype::$systemId"
        | "property-get:dom\\documenttype::$systemId" => {
            document_type::system_id(context, request)
        }
        "property-get:domdocumenttype::$internalSubset"
        | "property-get:dom\\documenttype::$internalSubset" => {
            document_type::internal_subset(context, request)
        }
        "property-get:domdocumenttype::$entities"
        | "property-get:dom\\documenttype::$entities" => {
            document_type::entities(context, request)
        }
        "property-get:domdocumenttype::$notations"
        | "property-get:dom\\documenttype::$notations" => {
            document_type::notations(context, request)
        }
        "property-get:domprocessinginstruction::$target"
        | "property-get:dom\\processinginstruction::$target" => {
            node::name(context, request)
        }
        "property-get:domprocessinginstruction::$data" => {
            character_data::data(context, request)
        }
        "property-get:domnodelist::$length"
        | "property-get:dom\\nodelist::$length"
        | "property-get:dom\\htmlcollection::$length"
        | "property-get:domnamednodemap::$length"
        | "property-get:dom\\namednodemap::$length"
        | "property-get:dom\\dtdnamednodemap::$length" => {
            collection::length(context, request)
        }
        "property-get:domentity::$publicId"
        | "property-get:dom\\entity::$publicId" => {
            entity::public_id(context, request)
        }
        "property-get:domentity::$systemId"
        | "property-get:dom\\entity::$systemId" => {
            entity::system_id(context, request)
        }
        "property-get:domentity::$notationName"
        | "property-get:dom\\entity::$notationName" => {
            entity::notation_name(context, request)
        }
        "property-get:domentity::$encoding" => {
            entity::encoding(context, request)
        }
        "property-get:domentity::$actualEncoding" => {
            entity::actual_encoding(context, request)
        }
        "property-get:domentity::$version" => {
            entity::version(context, request)
        }
        "property-get:domnotation::$publicId" => {
            entity::notation_public_id(context, request)
        }
        "property-get:domnotation::$systemId" => {
            entity::notation_system_id(context, request)
        }
        "property-get:dom\\notation::$publicId" => {
            entity::modern_notation_uninitialized(request, "publicId")
        }
        "property-get:dom\\notation::$systemId" => {
            entity::modern_notation_uninitialized(request, "systemId")
        }
        "property-get:dom\\tokenlist::$length" => {
            token_list::length(context, request)
        }
        "property-get:dom\\tokenlist::$value" => {
            token_list::value(context, request)
        }
        "property-set:dom\\tokenlist::$value" => {
            token_list::set_value(context, request)
        }
        "property-set:domattr::$value"
        | "property-set:dom\\attr::$value" => {
            node_values::set_attribute_value(context, request)
        }
        "property-set:domdocument::$version"
        | "property-set:domdocument::$xmlVersion"
        | "property-set:dom\\xmldocument::$xmlVersion" => {
            document_config::set_version(context, request)
        }
        "property-set:domdocument::$encoding" => {
            document_config::set_encoding(context, request)
        }
        "property-set:dom\\document::$characterSet"
        | "property-set:dom\\document::$charset"
        | "property-set:dom\\document::$inputEncoding" => {
            document_html::set_encoding(context, request)
        }
        "property-set:dom\\document::$body" => {
            document_html::set_body(context, request)
        }
        "property-set:dom\\document::$title" => {
            document_html::set_title(context, request)
        }
        "property-set:domdocument::$standalone"
        | "property-set:domdocument::$xmlStandalone"
        | "property-set:dom\\xmldocument::$xmlStandalone" => {
            document_config::set_standalone(context, request)
        }
        "property-set:domdocument::$documentURI"
        | "property-set:dom\\document::$documentURI"
        | "property-set:dom\\document::$URL" => {
            document::set_uri(context, request)
        }
        "property-set:domdocument::$preserveWhiteSpace" => {
            document_config::set(
                context,
                request,
                LegacyDocumentFlag::PreserveWhitespace,
            )
        }
        "property-set:domdocument::$recover" => document_config::set(
            context,
            request,
            LegacyDocumentFlag::Recover,
        ),
        "property-set:domdocument::$resolveExternals" => {
            document_config::set(
                context,
                request,
                LegacyDocumentFlag::ResolveExternals,
            )
        }
        "property-set:domdocument::$strictErrorChecking" => {
            document_config::set(
                context,
                request,
                LegacyDocumentFlag::StrictErrorChecking,
            )
        }
        "property-set:domdocument::$substituteEntities" => {
            document_config::set(
                context,
                request,
                LegacyDocumentFlag::SubstituteEntities,
            )
        }
        "property-set:domdocument::$validateOnParse" => {
            document_config::set(
                context,
                request,
                LegacyDocumentFlag::ValidateOnParse,
            )
        }
        "property-set:domxpath::$registerNodeNamespaces"
        | "property-set:dom\\xpath::$registerNodeNamespaces" => {
            xpath::set_register_node_namespaces(context, request)
        }
        "property-set:domprocessinginstruction::$data" => {
            character_data::set_data(context, request)
        }
        "property-get:domdocument::$formatOutput"
        | "property-get:dom\\xmldocument::$formatOutput" => {
            document::format_output(context, request)
        }
        "property-set:domdocument::$formatOutput"
        | "property-set:dom\\xmldocument::$formatOutput" => {
            document::set_format_output(context, request)
        }
        "property-get:domexception::$code"
        | "property-set:domexception::$code"
        | "property-get:libxmlerror::$code"
        | "property-get:libxmlerror::$column"
        | "property-get:libxmlerror::$file"
        | "property-get:libxmlerror::$level"
        | "property-get:libxmlerror::$line"
        | "property-get:libxmlerror::$message"
        | "property-get:dom\\namespaceinfo::$element"
        | "property-get:dom\\namespaceinfo::$namespaceURI"
        | "property-get:dom\\namespaceinfo::$prefix"
        | "property-set:libxmlerror::$code"
        | "property-set:libxmlerror::$column"
        | "property-set:libxmlerror::$file"
        | "property-set:libxmlerror::$level"
        | "property-set:libxmlerror::$line"
        | "property-set:libxmlerror::$message" => {
            reject_compiler_resident_property(request)
        }
        _ => Err(()),
    }
}

/// Remaps a shared `DOMNode` property-read opcode to the dedicated
/// `DOMNameSpaceNode` getter key when the receiver is a standalone
/// namespace-declaration wrapper.
///
/// The compiler lowers direct reads of the ten `DOMNameSpaceNode`
/// properties through the common `DOMNode` property opcode when the
/// static receiver type is a `DOMNode`-rooted union (e.g.
/// `DOMNodeList::item()`). At runtime the handle is a
/// `HANDLE_NAMESPACE_NODE`, whose fake `xmlNode` is not addressable by
/// the generic node accessors, so the read is rerouted to the
/// namespace-aware handlers by rewriting the operation key.
///
/// Returns either the original `key` (for non-namespace-node receivers or
/// properties outside the shared ten) or the matching static
/// `DOMNameSpaceNode` getter key, without allocating: the remapped keys
/// are `&'static str` literals that outlive the borrowed input lifetime.
fn namespace_node_property_key<'a>(key: &'a str, request: &Request) -> &'a str {
    if handle_kind(request.header.receiver).ok() != Some(HANDLE_NAMESPACE_NODE) {
        return key;
    }
    match key {
        "property-get:domnode::$nodeName" => "property-get:domnamespacenode::$nodeName",
        "property-get:domnode::$nodeValue" => "property-get:domnamespacenode::$nodeValue",
        "property-get:domnode::$nodeType" => "property-get:domnamespacenode::$nodeType",
        "property-get:domnode::$prefix" => "property-get:domnamespacenode::$prefix",
        "property-get:domnode::$localName" => "property-get:domnamespacenode::$localName",
        "property-get:domnode::$namespaceURI" => {
            "property-get:domnamespacenode::$namespaceURI"
        }
        "property-get:domnode::$isConnected" => "property-get:domnamespacenode::$isConnected",
        "property-get:domnode::$ownerDocument" => {
            "property-get:domnamespacenode::$ownerDocument"
        }
        "property-get:domnode::$parentNode" => "property-get:domnamespacenode::$parentNode",
        "property-get:domnode::$parentElement" => {
            "property-get:domnamespacenode::$parentElement"
        }
        _ => key,
    }
}

/// Rejects value-object properties that must be executed by ordinary PHP object lowering.
fn reject_compiler_resident_property(
    _request: &Request,
) -> Result<DispatchResult, ()> {
    Err(())
}
