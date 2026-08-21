//! Purpose:
//! Maps stable generated DOM/libxml opcodes to their focused bridge dispatchers.
//! Keeps operation routing separate from handle, result, and ownership mechanics.
//!
//! Called from:
//! - `super::dispatch()` after complete flat-message validation.
//!
//! Key details:
//! - Keys are locked by the generated PHP 8.5.8 opcode snapshot.
//! - Unimplemented locked operations fail closed through the bridge ABI.

mod properties;

use crate::context::Context;
use crate::handles::handle_kind;
use crate::objects::{
    HANDLE_NAMESPACE_NODE, HANDLE_NODE, HANDLE_SIMPLEXML, HANDLE_TOKEN_LIST,
    HANDLE_XPATH,
};
use crate::request::Request;

use super::{
    character_data, class_collection, collection, document, document_fragment,
    document_io, document_validation, document_xinclude, element, element_adjacent,
    element_markup, implementation, libxml, lifecycle, namespace_node, node, node_c14n,
    node_constructors, node_mutation, node_rename, node_values,
    register_node_class, reject_compiler_resident_operation, release_wrapper,
    retain_wrapper, selector, simplexml, token_list, xpath, DispatchResult,
};

/// Executes one generated public opcode or rejects an operation not yet implemented.
pub(super) fn dispatch(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let key =
        crate::generated::opcodes::operation_key(request.header.opcode).ok_or(())?;
    let invalid_receiver = match handle_kind(request.header.receiver).ok() {
        Some(HANDLE_NODE) => context
            .native_objects
            .get(request.header.receiver, HANDLE_NODE)
            .is_ok_and(crate::objects::NativeObject::is_invalid_node),
        Some(HANDLE_TOKEN_LIST) => context
            .native_objects
            .get(request.header.receiver, HANDLE_TOKEN_LIST)
            .is_ok_and(crate::objects::NativeObject::is_invalid_token_list),
        Some(HANDLE_NAMESPACE_NODE) => context
            .native_objects
            .get(request.header.receiver, HANDLE_NAMESPACE_NODE)
            .is_ok_and(crate::objects::NativeObject::is_invalid_namespace_node),
        _ => false,
    };
    if key != "internal:bridge.wrapper.release"
        && key != "internal:bridge.wrapper.retain"
        && invalid_receiver
    {
        return Ok(super::dom_exception(11));
    }
    match key {
        "function:simplexml_load_string"
        | "function:simplexml_load_file"
        | "method:simplexmlelement::__construct" => {
            simplexml::loaders::dispatch_borrowed(context, key, request)
        }
        "function:simplexml_import_dom" => {
            simplexml::interop::import_dom(context, request)
        }
        "function:dom_import_simplexml" => {
            simplexml::interop::import_legacy(context, request)
        }
        "function:dom\\import_simplexml" => {
            simplexml::interop::import_modern(context, request)
        }
        "method:simplexmlelement::__debuginfo" => {
            simplexml::methods::debug_info(context, request)
        }
        "method:simplexmlelement::__tostring" => {
            simplexml::methods::to_string(context, request)
        }
        "method:simplexmlelement::addattribute" => {
            simplexml::methods::add_attribute(context, request)
        }
        "method:simplexmlelement::addchild" => {
            simplexml::methods::add_child(context, request)
        }
        "method:simplexmlelement::asxml" => {
            simplexml::methods::as_xml(
                context,
                request,
                "SimpleXMLElement::asXML",
            )
        }
        "method:simplexmlelement::attributes" => {
            simplexml::methods::attributes(context, request)
        }
        "method:simplexmlelement::children" => {
            simplexml::methods::children(context, request)
        }
        "method:simplexmlelement::count" => {
            simplexml::methods::count(context, request)
        }
        "method:simplexmlelement::current" => {
            simplexml::methods::current(context, request)
        }
        "method:simplexmlelement::getchildren" => {
            simplexml::methods::get_children(context, request)
        }
        "method:simplexmlelement::getdocnamespaces" => {
            simplexml::methods::get_doc_namespaces(context, request)
        }
        "method:simplexmlelement::getname" => {
            simplexml::methods::get_name(context, request)
        }
        "method:simplexmlelement::getnamespaces" => {
            simplexml::methods::get_namespaces(context, request)
        }
        "method:simplexmlelement::haschildren" => {
            simplexml::methods::has_children(context, request)
        }
        "method:simplexmlelement::key" => {
            simplexml::methods::key(context, request)
        }
        "method:simplexmlelement::next" => {
            simplexml::methods::next(context, request)
        }
        "method:simplexmlelement::registerxpathnamespace" => {
            simplexml::methods::register_xpath_namespace(context, request)
        }
        "method:simplexmlelement::rewind" => {
            simplexml::methods::rewind(context, request)
        }
        "method:simplexmlelement::savexml" => {
            simplexml::methods::as_xml(
                context,
                request,
                "SimpleXMLElement::saveXML",
            )
        }
        "method:simplexmlelement::valid" => {
            simplexml::methods::valid(context, request)
        }
        "method:simplexmlelement::xpath" => {
            simplexml::methods::xpath(context, request)
        }
        "object-handler:simplexml::cast" => {
            simplexml::handlers::cast(context, request)
        }
        "object-handler:simplexml::compare" => {
            simplexml::handlers::compare_dispatch(context, request)
        }
        "object-handler:simplexml::count" => {
            simplexml::handlers::count_dispatch(context, request)
        }
        "object-handler:simplexml::get_iterator" => {
            simplexml::handlers::get_iterator_dispatch(context, request)
        }
        "object-handler:simplexml::has_dimension" => {
            simplexml::handlers::has_dimension_dispatch(context, request)
        }
        "object-handler:simplexml::has_property" => {
            simplexml::handlers::has_property_dispatch(context, request)
        }
        "object-handler:simplexml::read_dimension" => {
            simplexml::handlers::read_dimension_dispatch(context, request)
        }
        "object-handler:simplexml::read_property" => {
            simplexml::handlers::read_property_dispatch(context, request)
        }
        "object-handler:simplexml::unset_dimension" => {
            simplexml::handlers::unset_dimension_dispatch(context, request)
        }
        "object-handler:simplexml::unset_property" => {
            simplexml::handlers::unset_property_dispatch(context, request)
        }
        "object-handler:simplexml::write_dimension" => {
            simplexml::handlers::write_dimension_dispatch(context, request)
        }
        "object-handler:simplexml::write_property" => {
            simplexml::handlers::write_property_dispatch(context, request)
        }
        "method:domdocument::__construct" => {
            document::construct_legacy(context, request)
        }
        "method:dom\\adjacentposition::cases"
        | "method:dom\\adjacentposition::from"
        | "method:dom\\adjacentposition::tryfrom"
        | "method:dom\\namespaceinfo::__construct"
        | "method:dom\\node::__construct"
        | "method:dom\\tokenlist::__construct"
        | "method:domnodelist::getiterator"
        | "method:domnamednodemap::getiterator"
        | "method:dom\\nodelist::getiterator"
        | "method:dom\\namednodemap::getiterator"
        | "method:dom\\dtdnamednodemap::getiterator"
        | "method:dom\\htmlcollection::getiterator"
        | "method:dom\\tokenlist::getiterator" => {
            reject_compiler_resident_operation(request)
        }
        "method:domnode::__sleep"
        | "method:dom\\node::__sleep" => {
            lifecycle::reject_node_serialization(context, request, true)
        }
        "method:domnode::__wakeup"
        | "method:dom\\node::__wakeup" => {
            lifecycle::reject_node_serialization(context, request, false)
        }
        "method:domelement::__construct" => {
            node_constructors::element(context, request)
        }
        "method:domattr::__construct" => {
            node_constructors::attribute(context, request)
        }
        "method:domtext::__construct" => {
            node_constructors::text(context, request)
        }
        "method:domcomment::__construct" => {
            node_constructors::comment(context, request)
        }
        "method:domcdatasection::__construct" => {
            node_constructors::cdata(context, request)
        }
        "method:domdocumentfragment::__construct" => {
            node_constructors::fragment(context, request)
        }
        "method:domprocessinginstruction::__construct" => {
            node_constructors::processing_instruction(context, request)
        }
        "method:domentityreference::__construct" => {
            node_constructors::entity_reference(context, request)
        }
        "method:domimplementation::hasfeature" => {
            implementation::has_feature(context, request)
        }
        "method:domimplementation::createdocumenttype" => {
            implementation::create_document_type(
                context,
                request,
                crate::objects::DocumentFamily::Legacy,
            )
        }
        "method:dom\\implementation::createdocumenttype" => {
            implementation::create_document_type(
                context,
                request,
                crate::objects::DocumentFamily::ModernXml,
            )
        }
        "method:domimplementation::createdocument" => {
            implementation::create_document(
                context,
                request,
                crate::objects::DocumentFamily::Legacy,
            )
        }
        "method:dom\\implementation::createdocument" => {
            implementation::create_document(
                context,
                request,
                crate::objects::DocumentFamily::ModernXml,
            )
        }
        "method:dom\\implementation::createhtmldocument" => {
            implementation::create_html_document(context, request)
        }
        "method:dom\\xmldocument::createempty" => {
            document::create_empty_modern_xml(context, request)
        }
        "method:dom\\htmldocument::createempty" => {
            document::create_empty_modern_html(context, request)
        }
        "method:dom\\xmldocument::createfromstring" => {
            document::create_modern_xml_from_string(context, request)
        }
        "method:dom\\xmldocument::createfromfile" => {
            document_io::create_modern_xml_from_file(context, request)
        }
        "method:dom\\htmldocument::createfromstring" => {
            document::create_modern_html_from_string(context, request)
        }
        "method:dom\\htmldocument::createfromfile" => {
            document_io::create_modern_html_from_file(context, request)
        }
        "method:domdocument::loadxml" => {
            document::load_legacy_xml(context, request)
        }
        "method:domdocument::loadhtml" => {
            document::load_legacy_html(context, request)
        }
        "method:domdocument::load" => {
            document_io::load_legacy_xml_file(context, request)
        }
        "method:domdocument::loadhtmlfile" => {
            document_io::load_legacy_html_file(context, request)
        }
        "method:domdocument::validate" => document_validation::validate(
            context,
            request,
            b"DOMDocument::validate",
        ),
        "method:dom\\xmldocument::validate" => {
            document_validation::validate(
                context,
                request,
                b"Dom\\XMLDocument::validate",
            )
        }
        "method:domdocument::schemavalidatesource" => {
            document_validation::schema_validate_source(
                context,
                request,
                b"DOMDocument::schemaValidateSource",
            )
        }
        "method:dom\\document::schemavalidatesource" => {
            document_validation::schema_validate_source(
                context,
                request,
                b"Dom\\Document::schemaValidateSource",
            )
        }
        "method:domdocument::schemavalidate" => {
            document_validation::schema_validate_file(
                context,
                request,
                b"DOMDocument::schemaValidate",
            )
        }
        "method:dom\\document::schemavalidate" => {
            document_validation::schema_validate_file(
                context,
                request,
                b"Dom\\Document::schemaValidate",
            )
        }
        "method:domdocument::relaxngvalidatesource" => {
            document_validation::relaxng_validate_source(
                context,
                request,
                b"DOMDocument::relaxNGValidateSource",
            )
        }
        "method:dom\\document::relaxngvalidatesource" => {
            document_validation::relaxng_validate_source(
                context,
                request,
                b"Dom\\Document::relaxNgValidateSource",
            )
        }
        "method:domdocument::relaxngvalidate" => {
            document_validation::relaxng_validate_file(
                context,
                request,
                b"DOMDocument::relaxNGValidate",
            )
        }
        "method:dom\\document::relaxngvalidate" => {
            document_validation::relaxng_validate_file(
                context,
                request,
                b"Dom\\Document::relaxNgValidate",
            )
        }
        "method:domdocument::registernodeclass" => {
            register_node_class::register(context, request, false)
        }
        "method:dom\\document::registernodeclass" => {
            register_node_class::register(context, request, true)
        }
        "method:domdocument::xinclude"
        | "method:dom\\xmldocument::xinclude" => {
            document_xinclude::direct(context, key, request)
        }
        "method:domnode::c14n"
        | "method:dom\\node::c14n" => {
            node_c14n::canonicalize(context, key, request)
        }
        "method:domnode::c14nfile"
        | "method:dom\\node::c14nfile" => {
            node_c14n::canonicalize_file(context, key, request)
        }
        "method:domxpath::__construct" => {
            xpath::construct(context, request, false)
        }
        "method:dom\\xpath::__construct" => {
            xpath::construct(context, request, true)
        }
        "method:domxpath::evaluate" => xpath::evaluate(
            context,
            request,
            false,
            false,
            b"DOMXPath::evaluate",
        ),
        "method:dom\\xpath::evaluate" => xpath::evaluate(
            context,
            request,
            true,
            false,
            b"Dom\\XPath::evaluate",
        ),
        "method:domxpath::query" => xpath::evaluate(
            context,
            request,
            false,
            true,
            b"DOMXPath::query",
        ),
        "method:dom\\xpath::query" => xpath::evaluate(
            context,
            request,
            true,
            true,
            b"Dom\\XPath::query",
        ),
        "method:domxpath::quote"
        | "method:dom\\xpath::quote" => xpath::quote(request),
        "method:domxpath::registernamespace"
        | "method:dom\\xpath::registernamespace" => {
            xpath::register_namespace(context, request)
        }
        "method:domxpath::registerphpfunctionns" => {
            xpath::register_php_function_ns(
                context,
                request,
                b"DOMXPath::registerPhpFunctionNS",
            )
        }
        "method:dom\\xpath::registerphpfunctionns" => {
            xpath::register_php_function_ns(
                context,
                request,
                b"Dom\\XPath::registerPhpFunctionNS",
            )
        }
        "method:domxpath::registerphpfunctions" => {
            xpath::register_php_functions(
                context,
                request,
                b"DOMXPath::registerPhpFunctions",
            )
        }
        "method:dom\\xpath::registerphpfunctions" => {
            xpath::register_php_functions(
                context,
                request,
                b"Dom\\XPath::registerPhpFunctions",
            )
        }
        "method:domdocument::createelement"
        | "method:dom\\document::createelement" => {
            document::create_element(context, request)
        }
        "method:domdocument::createelementns"
        | "method:dom\\document::createelementns" => {
            document::create_element_ns(context, request)
        }
        "method:domdocument::createattribute"
        | "method:dom\\document::createattribute" => {
            document::create_attribute(context, request)
        }
        "method:domdocument::createattributens"
        | "method:dom\\document::createattributens" => {
            document::create_attribute_ns(context, request)
        }
        "method:domdocument::createtextnode"
        | "method:dom\\document::createtextnode" => {
            document::create_text_node(context, request)
        }
        "method:domdocument::createcdatasection"
        | "method:dom\\document::createcdatasection" => {
            document::create_cdata_section(context, request)
        }
        "method:domdocument::createcomment"
        | "method:dom\\document::createcomment" => {
            document::create_comment(context, request)
        }
        "method:domdocument::createdocumentfragment"
        | "method:dom\\document::createdocumentfragment" => {
            document::create_document_fragment(context, request)
        }
        "method:domdocumentfragment::appendxml" => {
            document_fragment::append_xml(
                context,
                request,
                b"DOMDocumentFragment::appendXML",
            )
        }
        "method:dom\\documentfragment::appendxml" => {
            document_fragment::append_xml(
                context,
                request,
                b"Dom\\DocumentFragment::appendXml",
            )
        }
        "method:domdocument::createprocessinginstruction"
        | "method:dom\\document::createprocessinginstruction" => {
            document::create_processing_instruction(context, request)
        }
        "method:domdocument::createentityreference"
        | "method:dom\\xmldocument::createentityreference" => {
            document::create_entity_reference(context, request)
        }
        "method:domdocument::importnode"
        | "method:dom\\document::importnode"
        | "method:dom\\document::importlegacynode" => {
            document::import_node(context, request)
        }
        "method:domdocument::adoptnode"
        | "method:dom\\document::adoptnode" => {
            document::adopt_node(context, request)
        }
        "method:domdocument::getelementbyid"
        | "method:dom\\document::getelementbyid" => {
            document::element_by_id(context, request)
        }
        "method:domdocument::append"
        | "method:domdocumentfragment::append"
        | "method:domelement::append"
        | "method:domparentnode::append"
        | "method:dom\\document::append"
        | "method:dom\\documentfragment::append"
        | "method:dom\\element::append"
        | "method:dom\\parentnode::append" => {
            node_mutation::append(context, request)
        }
        "method:domdocument::prepend"
        | "method:domdocumentfragment::prepend"
        | "method:domelement::prepend"
        | "method:domparentnode::prepend"
        | "method:dom\\document::prepend"
        | "method:dom\\documentfragment::prepend"
        | "method:dom\\element::prepend"
        | "method:dom\\parentnode::prepend" => {
            node_mutation::prepend(context, request)
        }
        "method:domdocument::replacechildren"
        | "method:domdocumentfragment::replacechildren"
        | "method:domelement::replacechildren"
        | "method:domparentnode::replacechildren"
        | "method:dom\\document::replacechildren"
        | "method:dom\\documentfragment::replacechildren"
        | "method:dom\\element::replacechildren"
        | "method:dom\\parentnode::replacechildren" => {
            node_mutation::replace_children(context, request)
        }
        "method:domcharacterdata::before"
        | "method:domchildnode::before"
        | "method:domelement::before"
        | "method:dom\\characterdata::before"
        | "method:dom\\childnode::before"
        | "method:dom\\documenttype::before"
        | "method:dom\\element::before" => {
            node_mutation::before(context, request)
        }
        "method:domcharacterdata::after"
        | "method:domchildnode::after"
        | "method:domelement::after"
        | "method:dom\\characterdata::after"
        | "method:dom\\childnode::after"
        | "method:dom\\documenttype::after"
        | "method:dom\\element::after" => {
            node_mutation::after(context, request)
        }
        "method:domcharacterdata::replacewith"
        | "method:domchildnode::replacewith"
        | "method:domelement::replacewith"
        | "method:dom\\characterdata::replacewith"
        | "method:dom\\childnode::replacewith"
        | "method:dom\\documenttype::replacewith"
        | "method:dom\\element::replacewith" => {
            node_mutation::replace_with(context, request)
        }
        "method:domcharacterdata::remove"
        | "method:domchildnode::remove"
        | "method:domelement::remove"
        | "method:dom\\characterdata::remove"
        | "method:dom\\childnode::remove"
        | "method:dom\\documenttype::remove"
        | "method:dom\\element::remove" => {
            node_mutation::remove(context, request)
        }
        "method:domelement::insertadjacentelement"
        | "method:dom\\element::insertadjacentelement" => {
            element_adjacent::insert_element(context, request)
        }
        "method:domelement::insertadjacenttext"
        | "method:dom\\element::insertadjacenttext" => {
            element_adjacent::insert_text(context, request)
        }
        "method:dom\\element::rename"
        | "method:dom\\attr::rename" => {
            node_rename::rename(context, request)
        }
        "method:dom\\element::insertadjacenthtml" => {
            element_markup::insert_adjacent_html(context, request)
        }
        "method:domnode::appendchild"
        | "method:dom\\node::appendchild" => node::append_child(context, request),
        "method:domnode::insertbefore"
        | "method:dom\\node::insertbefore" => {
            node::insert_before(context, request)
        }
        "method:domnode::removechild"
        | "method:dom\\node::removechild" => node::remove_child(context, request),
        "method:domnode::replacechild"
        | "method:dom\\node::replacechild" => {
            node::replace_child(context, request)
        }
        "method:domnode::haschildnodes"
        | "method:dom\\node::haschildnodes" => {
            node::has_child_nodes(context, request)
        }
        "method:domnode::normalize"
        | "method:dom\\node::normalize"
        | "method:domdocument::normalizedocument" => {
            node::normalize(context, request)
        }
        "method:domnode::issamenode"
        | "method:dom\\node::issamenode" => node::is_same(context, request),
        "method:domnode::isequalnode"
        | "method:dom\\node::isequalnode" => {
            node::is_equal(context, request)
        }
        "method:domnode::comparedocumentposition"
        | "method:dom\\node::comparedocumentposition" => {
            node::compare_document_position(context, request)
        }
        "method:domnode::contains"
        | "method:dom\\node::contains" => node::contains(context, request),
        "method:domnode::getrootnode"
        | "method:dom\\node::getrootnode" => node::root(context, request),
        "method:domnode::clonenode"
        | "method:dom\\node::clonenode" => {
            node_values::clone_node(context, request)
        }
        "method:domnode::getlineno"
        | "method:dom\\node::getlineno" => node_values::line(context, request),
        "method:domnode::getnodepath"
        | "method:dom\\node::getnodepath" => node_values::path(context, request),
        "method:domnode::hasattributes"
        | "method:dom\\element::hasattributes" => {
            node_values::has_attributes(context, request)
        }
        "method:domnode::isdefaultnamespace"
        | "method:dom\\node::isdefaultnamespace" => {
            node_values::is_default_namespace(context, request)
        }
        "method:domnode::issupported" => {
            node_values::is_supported(context, request)
        }
        "method:domattr::isid"
        | "method:dom\\attr::isid" => {
            node_values::attribute_is_id(context, request)
        }
        "method:domnode::lookupnamespaceuri"
        | "method:dom\\node::lookupnamespaceuri" => {
            node_values::lookup_namespace_uri(context, request)
        }
        "method:domnode::lookupprefix"
        | "method:dom\\node::lookupprefix" => {
            node_values::lookup_prefix(context, request)
        }
        "method:domcharacterdata::substringdata"
        | "method:dom\\characterdata::substringdata" => {
            character_data::substring(context, request)
        }
        "method:domcharacterdata::appenddata"
        | "method:dom\\characterdata::appenddata" => {
            character_data::append(context, request)
        }
        "method:domcharacterdata::insertdata"
        | "method:dom\\characterdata::insertdata" => {
            character_data::insert(context, request)
        }
        "method:domcharacterdata::deletedata"
        | "method:dom\\characterdata::deletedata" => {
            character_data::delete(context, request)
        }
        "method:domcharacterdata::replacedata"
        | "method:dom\\characterdata::replacedata" => {
            character_data::replace(context, request)
        }
        "method:domtext::splittext"
        | "method:dom\\text::splittext" => {
            character_data::split_text(context, request)
        }
        "method:domtext::iswhitespaceinelementcontent"
        | "method:domtext::iselementcontentwhitespace" => {
            character_data::is_whitespace(context, request)
        }
        "method:domelement::getattribute"
        | "method:dom\\element::getattribute" => {
            element::get_attribute(context, request)
        }
        "method:domelement::hasattribute"
        | "method:dom\\element::hasattribute" => {
            element::has_attribute(context, request)
        }
        "method:domelement::hasattributens"
        | "method:dom\\element::hasattributens" => {
            element::has_attribute_ns(context, request)
        }
        "method:domelement::getattributenode"
        | "method:dom\\element::getattributenode" => {
            element::get_attribute_node(context, request)
        }
        "method:domelement::getattributenames"
        | "method:dom\\element::getattributenames" => {
            element::get_attribute_names(context, request)
        }
        "method:domelement::getattributens"
        | "method:dom\\element::getattributens" => {
            element::get_attribute_ns(context, request)
        }
        "method:domelement::getattributenodens"
        | "method:dom\\element::getattributenodens" => {
            element::get_attribute_node_ns(context, request)
        }
        "method:domelement::setattribute"
        | "method:dom\\element::setattribute" => {
            element::set_attribute(context, request)
        }
        "method:domelement::setattributens"
        | "method:dom\\element::setattributens" => {
            element::set_attribute_ns(context, request)
        }
        "method:domelement::setattributenode" => {
            element::set_attribute_node(context, request, false)
        }
        "method:domelement::setattributenodens"
        | "method:dom\\element::setattributenode"
        | "method:dom\\element::setattributenodens" => {
            element::set_attribute_node(context, request, true)
        }
        "method:domelement::removeattribute"
        | "method:dom\\element::removeattribute" => {
            element::remove_attribute(context, request)
        }
        "method:domelement::removeattributenode"
        | "method:dom\\element::removeattributenode" => {
            element::remove_attribute_node(context, request)
        }
        "method:domelement::removeattributens"
        | "method:dom\\element::removeattributens" => {
            element::remove_attribute_ns(context, request)
        }
        "method:domelement::toggleattribute"
        | "method:dom\\element::toggleattribute" => {
            element::toggle_attribute(context, request)
        }
        "method:domelement::setidattribute"
        | "method:dom\\element::setidattribute" => {
            element::set_id_attribute(context, request, false)
        }
        "method:domelement::setidattributens"
        | "method:dom\\element::setidattributens" => {
            element::set_id_attribute(context, request, true)
        }
        "method:domelement::setidattributenode"
        | "method:dom\\element::setidattributenode" => {
            element::set_id_attribute_node(context, request)
        }
        "method:domdocument::getelementsbytagname"
        | "method:dom\\document::getelementsbytagname"
        | "method:domelement::getelementsbytagname"
        | "method:dom\\element::getelementsbytagname" => {
            collection::elements_by_tag_name(context, request)
        }
        "method:domdocument::getelementsbytagnamens"
        | "method:dom\\document::getelementsbytagnamens"
        | "method:domelement::getelementsbytagnamens"
        | "method:dom\\element::getelementsbytagnamens" => {
            collection::elements_by_tag_name_ns(context, request)
        }
        "method:dom\\document::getelementsbyclassname"
        | "method:dom\\element::getelementsbyclassname" => {
            class_collection::elements_by_class_name(context, request)
        }
        "method:dom\\document::queryselector"
        | "method:dom\\documentfragment::queryselector"
        | "method:dom\\element::queryselector"
        | "method:dom\\parentnode::queryselector" => {
            selector::query_selector(context, request)
        }
        "method:dom\\document::queryselectorall"
        | "method:dom\\documentfragment::queryselectorall"
        | "method:dom\\element::queryselectorall"
        | "method:dom\\parentnode::queryselectorall" => {
            selector::query_selector_all(context, request)
        }
        "method:dom\\element::matches" => {
            selector::matches(context, request)
        }
        "method:dom\\element::getinscopenamespaces" => {
            element::get_in_scope_namespaces(context, request)
        }
        "method:dom\\element::getdescendantnamespaces" => {
            element::get_descendant_namespaces(context, request)
        }
        "method:dom\\element::closest" => {
            selector::closest(context, request)
        }
        "method:dom\\tokenlist::add" => token_list::add(context, request),
        "method:dom\\tokenlist::contains" => {
            token_list::contains(context, request)
        }
        "method:dom\\tokenlist::count" => token_list::length(context, request),
        "method:dom\\tokenlist::item" => token_list::item(context, request),
        "method:dom\\tokenlist::remove" => {
            token_list::remove(context, request)
        }
        "method:dom\\tokenlist::replace" => {
            token_list::replace(context, request)
        }
        "method:dom\\tokenlist::supports" => {
            token_list::supports(context, request)
        }
        "method:dom\\tokenlist::toggle" => {
            token_list::toggle(context, request)
        }
        "method:domnodelist::count"
        | "method:dom\\nodelist::count"
        | "method:dom\\htmlcollection::count"
        | "method:domnamednodemap::count"
        | "method:dom\\namednodemap::count"
        | "method:dom\\dtdnamednodemap::count" => {
            collection::length(context, request)
        }
        "method:domnodelist::item"
        | "method:dom\\nodelist::item"
        | "method:dom\\htmlcollection::item"
        | "method:domnamednodemap::item"
        | "method:dom\\namednodemap::item"
        | "method:dom\\dtdnamednodemap::item" => {
            collection::item(context, request)
        }
        "method:domnamednodemap::getnameditem"
        | "method:dom\\namednodemap::getnameditem" => {
            collection::get_named_item(context, request)
        }
        "method:dom\\dtdnamednodemap::getnameditem" => {
            collection::dtd_get_named_item(context, request)
        }
        "method:domnamednodemap::getnameditemns"
        | "method:dom\\namednodemap::getnameditemns" => {
            collection::get_named_item_ns(context, request)
        }
        "method:dom\\dtdnamednodemap::getnameditemns" => {
            collection::dtd_get_named_item_ns(context, request)
        }
        "method:dom\\htmlcollection::nameditem" => {
            collection::named_item(context, request)
        }
        "method:domdocument::savexml"
        | "method:dom\\xmldocument::savexml"
        | "method:dom\\htmldocument::savexml" => {
            document::serialize_xml(context, request)
        }
        "method:domdocument::savehtml" => {
            document::serialize_legacy_html(context, request)
        }
        "method:dom\\htmldocument::savehtml" => {
            document::serialize_html(context, request)
        }
        "method:domdocument::save"
        | "method:dom\\xmldocument::savexmlfile"
        | "method:dom\\htmldocument::savexmlfile" => {
            document_io::save_xml_file(context, request)
        }
        "method:domdocument::savehtmlfile"
        | "method:dom\\htmldocument::savehtmlfile" => {
            document_io::save_html_file(context, request)
        }
        "function:libxml_use_internal_errors" => {
            libxml::use_internal_errors(context, request)
        }
        "function:libxml_get_errors" => libxml::get_errors(context, request),
        "function:libxml_get_last_error" => {
            libxml::get_last_error(context, request)
        }
        "function:libxml_clear_errors" => libxml::clear_errors(context, request),
        "function:libxml_disable_entity_loader" => {
            libxml::disable_entity_loader(context, request)
        }
        "function:libxml_set_external_entity_loader" => {
            libxml::set_external_entity_loader(context, request)
        }
        "function:libxml_get_external_entity_loader" => {
            libxml::get_external_entity_loader(context, request)
        }
        "function:libxml_set_streams_context" => {
            libxml::set_streams_context(context, request)
        }
        "method:domnamespacenode::__sleep" => {
            namespace_node::sleep(context, request)
        }
        "method:domnamespacenode::__wakeup" => {
            namespace_node::wakeup(context, request)
        }
        "internal:bridge.object.clone" => {
            match handle_kind(request.header.receiver).map_err(|_| ())? {
                HANDLE_XPATH => xpath::clone_object(context, request),
                HANDLE_NAMESPACE_NODE => {
                    namespace_node::clone_object(context, request)
                }
                HANDLE_SIMPLEXML => {
                    simplexml::clone::clone_object(context, request)
                }
                _ => node_values::clone_object(context, request),
            }
        }
        "internal:bridge.wrapper.release" => release_wrapper(context, request),
        "internal:bridge.wrapper.retain" => retain_wrapper(context, request),
        _ => properties::dispatch(key, context, request),
    }
}
