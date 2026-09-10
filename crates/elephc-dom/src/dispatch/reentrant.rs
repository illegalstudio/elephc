//! Purpose:
//! Dispatches DOM operations whose native parser can synchronously call back into PHP.
//! Keeps the per-thread context `RefCell` unborrowed across every host callback boundary.
//!
//! Called from:
//! - `crate::exports::elephc_dom_call()` before ordinary mutable-context dispatch.
//!
//! Key details:
//! - Request and receiver state are validated and snapshotted before callbacks start.
//! - A callback may re-enter DOM; parsing and stream output never retain a context borrow.

use crate::abi::{STATUS_ABI_ERROR, STATUS_THROW, VALUE_INT};
use crate::context::context as find_context;
use crate::objects::DocumentFamily;
use crate::request::Request;

use super::{
    document,
    document_io::{
        FilePreparation, HostFileReadExecution, HostFileReadPreparation,
    },
    document_mut,
    document_validation::FileValidationPreparation,
    document_validation::SourceValidationPreparation,
    document_xinclude::XIncludePreparation,
    libxml::record_errors,
    node_c14n::C14nFilePreparation,
    optional_i32,
    xpath::XPathEvaluationPreparation,
    DispatchResult,
};

/// Routes one callback-capable operation or declines when ordinary dispatch is sufficient.
pub(super) fn dispatch(
    context_id: u64,
    operation_key: &str,
    request: &Request,
) -> Result<Option<DispatchResult>, ()> {
    if super::simplexml::loaders::handles(operation_key) {
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let prepared = super::simplexml::loaders::prepare(
            &context,
            operation_key,
            request,
        )?;
        let host = context.host;
        let stream_context = context.stream_context;
        drop(context);
        let prepared = match prepared {
            super::simplexml::loaders::Preparation::Ready(prepared) => prepared,
            super::simplexml::loaders::Preparation::Complete(result) => {
                return Ok(Some(result));
            }
        };
        let execution = super::simplexml::loaders::execute(
            context_id,
            host,
            stream_context,
            prepared,
        )?;
        let execution = match execution {
            super::simplexml::loaders::Execution::Parsed { .. } => execution,
            super::simplexml::loaders::Execution::Complete(result) => {
                return Ok(Some(result));
            }
        };
        let mut context = match context_cell.try_borrow_mut() {
            Ok(context) => context,
            Err(_) => {
                super::simplexml::loaders::free_execution(execution);
                return Err(());
            }
        };
        let result = super::simplexml::loaders::finish(&mut context, execution)?;
        return Ok(Some(result));
    }

    let xpath_evaluation = match operation_key {
        "method:domxpath::evaluate" => {
            Some((false, false, b"DOMXPath::evaluate".as_slice()))
        }
        "method:dom\\xpath::evaluate" => {
            Some((true, false, b"Dom\\XPath::evaluate".as_slice()))
        }
        "method:domxpath::query" => {
            Some((false, true, b"DOMXPath::query".as_slice()))
        }
        "method:dom\\xpath::query" => {
            Some((true, true, b"Dom\\XPath::query".as_slice()))
        }
        _ => None,
    };
    if let Some((modern, force_nodeset, method)) = xpath_evaluation {
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let prepared = super::xpath::prepare_evaluation(
            &context,
            request,
            modern,
            force_nodeset,
            method,
        )?;
        drop(context);
        let prepared = match prepared {
            XPathEvaluationPreparation::Ready(prepared) => prepared,
            XPathEvaluationPreparation::Complete(result) => {
                return Ok(Some(result));
            }
        };
        debug_assert!(
            super::xpath::evaluation_has_callbacks(&prepared),
            "re-entrant XPath routing requires native callback registrations",
        );
        let mut executed = super::xpath::execute_evaluation(
            context_id,
            request.header.receiver,
            prepared,
            true,
        )?;
        let pending_host_actions =
            executed.take_pending_host_actions();
        let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
        let result =
            super::xpath::finish_evaluation(&mut context, executed);
        drop(context);
        let mut result = result?;
        for action in pending_host_actions {
            match action.execute() {
                Ok(()) => {}
                Err(crate::host::HostCallError::PendingThrowable) => {
                    result = DispatchResult::pending_host_throwable();
                    break;
                }
                Err(crate::host::HostCallError::Abi) => return Err(()),
            }
        }
        return Ok(Some(result));
    }

    if matches!(
        operation_key,
        "method:domdocument::xinclude"
            | "method:dom\\xmldocument::xinclude"
    ) {
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let prepared =
            super::document_xinclude::prepare(&context, operation_key, request)?;
        drop(context);
        let prepared = match prepared {
            XIncludePreparation::Ready(prepared) => prepared,
            XIncludePreparation::Complete(result) => {
                return Ok(Some(result));
            }
        };
        let executed =
            super::document_xinclude::execute(context_id, prepared);
        let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
        let result =
            super::document_xinclude::finish(&mut context, executed)?;
        return Ok(Some(result));
    }

    if matches!(
        operation_key,
        "method:domnode::c14nfile"
            | "method:dom\\node::c14nfile"
    ) {
        let path = request.byte_string(0)?;
        if !super::document_io::requires_host_stream(path) {
            return Ok(None);
        }
        let context_cell = find_context(context_id).ok_or(())?;
        let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
        let prepared =
            super::node_c14n::prepare_file(&mut context, operation_key, request)?;
        let host = context.host;
        let stream_context = context.stream_context;
        drop(context);
        let result = match prepared {
            C14nFilePreparation::Ready(prepared) => {
                super::node_c14n::write_host_file(
                    host,
                    stream_context,
                    prepared,
                )?
            }
            C14nFilePreparation::Complete(result) => result,
        };
        return Ok(Some(result));
    }

    let source_validation = matches!(
        operation_key,
        "method:domdocument::schemavalidatesource"
            | "method:dom\\document::schemavalidatesource"
            | "method:domdocument::relaxngvalidatesource"
            | "method:dom\\document::relaxngvalidatesource"
    );
    if source_validation {
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let prepared =
            super::document_validation::prepare_source_validation(
                &context,
                operation_key,
                request,
            )?;
        drop(context);
        let prepared = match prepared {
            SourceValidationPreparation::Ready(prepared) => prepared,
            SourceValidationPreparation::Complete(result) => {
                return Ok(Some(result));
            }
        };
        let executed =
            super::document_validation::execute_source_validation(
                context_id,
                prepared,
            )?;
        let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
        let result = super::document_validation::finish_source_validation(
            &mut context,
            executed,
        )?;
        return Ok(Some(result));
    }

    let file_validation = matches!(
        operation_key,
        "method:domdocument::schemavalidate"
            | "method:dom\\document::schemavalidate"
            | "method:domdocument::relaxngvalidate"
            | "method:dom\\document::relaxngvalidate"
    );
    if file_validation {
        let path = request.byte_string(0)?;
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let requires_callback = super::document_io::requires_host_stream(path)
            || context.external_entity_loader.is_some()
            || context.entity_loader_disabled;
        if requires_callback {
            let prepared =
                super::document_validation::prepare_file_validation(
                    &context,
                    operation_key,
                    request,
                )?;
            drop(context);
            let prepared = match prepared {
                FileValidationPreparation::Ready(prepared) => prepared,
                FileValidationPreparation::Complete(result) => {
                    return Ok(Some(result));
                }
            };
            let executed =
                super::document_validation::execute_file_validation(
                    context_id,
                    prepared,
                )?;
            let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
            let result =
                super::document_validation::finish_file_validation(
                    &mut context,
                    executed,
                )?;
            return Ok(Some(result));
        }
    }

    let host_file_read = matches!(
        operation_key,
        "method:dom\\xmldocument::createfromfile"
            | "method:dom\\htmldocument::createfromfile"
            | "method:domdocument::load"
            | "method:domdocument::loadhtmlfile"
    );
    if host_file_read {
        let path = request.byte_string(0)?;
        if super::document_io::requires_host_stream(path) {
            let context_cell = find_context(context_id).ok_or(())?;
            let context = context_cell.try_borrow().map_err(|_| ())?;
            let prepared = super::document_io::prepare_host_file_read(
                &context,
                operation_key,
                request,
            )?;
            let host = context.host;
            let stream_context = context.stream_context;
            drop(context);
            let prepared = match prepared {
                HostFileReadPreparation::Ready(prepared) => prepared,
                HostFileReadPreparation::Complete(result) => {
                    return Ok(Some(result));
                }
            };
            let execution = super::document_io::execute_host_file_read(
                host,
                stream_context,
                prepared,
            )?;
            let execution = match execution {
                HostFileReadExecution::Parsed { .. } => execution,
                HostFileReadExecution::Complete(result) => {
                    return Ok(Some(result));
                }
            };
            let mut context = match context_cell.try_borrow_mut() {
                Ok(context) => context,
                Err(_) => {
                    super::document_io::free_host_file_read(execution);
                    return Err(());
                }
            };
            let result =
                super::document_io::finish_host_file_read(&mut context, execution)?;
            return Ok(Some(result));
        }
    }

    let simplexml_save = match operation_key {
        "method:simplexmlelement::asxml" => Some("SimpleXMLElement::asXML"),
        "method:simplexmlelement::savexml" => Some("SimpleXMLElement::saveXML"),
        _ => None,
    };
    if let Some(method) = simplexml_save {
        if request.values.len() != 1 {
            return Ok(None);
        }
        let Some(path) = request.optional_byte_string(0)? else {
            return Ok(None);
        };
        if !super::document_io::requires_host_stream(path) {
            return Ok(None);
        }
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let prepared = super::simplexml::methods::prepare_as_xml(
            &context,
            request,
            method,
        )?;
        let host = context.host;
        let stream_context = context.stream_context;
        drop(context);
        let result = match prepared {
            FilePreparation::Ready(prepared) => {
                super::document_io::write_host_stream(
                    host,
                    stream_context,
                    prepared,
                )?
            }
            FilePreparation::Complete(result) => result,
        };
        return Ok(Some(if result.value_tag == VALUE_INT {
            DispatchResult::boolean(true)
        } else {
            result
        }));
    }

    let save_as_xml = match operation_key {
        "method:domdocument::save"
        | "method:dom\\xmldocument::savexmlfile"
        | "method:dom\\htmldocument::savexmlfile" => Some(true),
        "method:domdocument::savehtmlfile"
        | "method:dom\\htmldocument::savehtmlfile" => Some(false),
        _ => None,
    };
    if let Some(save_as_xml) = save_as_xml {
        let path = request.byte_string(0)?;
        if !super::document_io::requires_host_stream(path) {
            return Ok(None);
        }
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let prepared = if save_as_xml {
            super::document_io::prepare_xml_file(&context, request)?
        } else {
            super::document_io::prepare_html_file(&context, request)?
        };
        let host = context.host;
        let stream_context = context.stream_context;
        drop(context);
        let result = match prepared {
            FilePreparation::Ready(prepared) => {
                super::document_io::write_host_stream(
                    host,
                    stream_context,
                    prepared,
                )?
            }
            FilePreparation::Complete(result) => result,
        };
        return Ok(Some(result));
    }

    if operation_key != "method:domdocument::loadxml" {
        return Ok(None);
    }

    let context_cell = find_context(context_id).ok_or(())?;
    let context = context_cell.try_borrow().map_err(|_| ())?;
    if context.external_entity_loader.is_none() && !context.entity_loader_disabled {
        return Ok(None);
    }
    let source = request.byte_string(0)?.to_vec();
    let explicit_options = optional_i32(request, 1, 0)?;
    let target = document(&context, request.header.receiver)?;
    if target.family() != DocumentFamily::Legacy {
        return Err(());
    }
    let options = explicit_options | target.legacy_parser_options();
    drop(context);

    let outcome = crate::native::document_parse_xml_with_host(
        &source,
        options,
        None,
        None,
        context_id,
    )?;
    match u32::try_from(outcome.host_status).map_err(|_| ())? {
        0 => {}
        STATUS_THROW => {
            free_parse_document(outcome.document);
            return Ok(Some(DispatchResult::pending_host_throwable()));
        }
        STATUS_ABI_ERROR => {
            free_parse_document(outcome.document);
            return Err(());
        }
        _ => {
            free_parse_document(outcome.document);
            return Err(());
        }
    }

    let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
    let emit_warnings = !context.internal_errors;
    record_errors(&mut context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = DispatchResult::boolean(false);
        return Ok(Some(if emit_warnings {
            result.with_libxml_parser_warnings(
                b"DOMDocument::loadXML",
                &outcome.errors,
                options,
            )
        } else {
            result
        }));
    };
    let document = document_mut(&mut context, request.header.receiver)?;
    let previous_pointer = document.pointer();
    document.replace_pointer(pointer);
    context.document_handles.remove(&previous_pointer);
    context
        .document_handles
        .insert(pointer, request.header.receiver);
    let result = DispatchResult::boolean(true);
    Ok(Some(if emit_warnings {
        result.with_libxml_parser_warnings(
            b"DOMDocument::loadXML",
            &outcome.errors,
            options,
        )
    } else {
        result
    }))
}

/// Frees a native parse document when a host failure prevents it from entering a context.
fn free_parse_document(document: Option<usize>) {
    if let Some(document) = document {
        unsafe {
            crate::native::document_free(document);
        }
    }
}
