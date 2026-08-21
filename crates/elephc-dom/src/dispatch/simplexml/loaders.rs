//! Purpose:
//! Implements SimpleXML string/file loading and `SimpleXMLElement::__construct()`.
//! Splits validation, callback-capable I/O/parsing, and context publication for re-entry.
//!
//! Called from:
//! - `super::super::reentrant::dispatch()` for exported parse operations.
//! - `super::super::routes::dispatch()` as a callback-free defensive fallback.
//!
//! Key details:
//! - Parsing completes before any existing constructor receiver is changed.
//! - File streams and external-entity callbacks run without a mutable context borrow.
//! - New parses use unclaimed XML graphs and fresh externally owned view handles.

use crate::abi::{STATUS_ABI_ERROR, STATUS_THROW};
use crate::context::Context;
use crate::host::HostCallError;
use crate::objects::{
    DocumentGraph, SimpleXmlIteratorState, SimpleXmlObject,
};
use crate::request::Request;

use super::super::{
    document_io, libxml::record_errors, require_no_receiver, DispatchResult,
};

/// One validated public loader/constructor variant retained across host callbacks.
#[derive(Clone, Copy)]
enum ParseKind {
    LoadString { class_kind: u64 },
    LoadFile { class_kind: u64 },
    Construct { receiver: u64 },
}

impl ParseKind {
    /// Returns the exact PHP callable spelling used by parser diagnostics.
    fn warning_method(&self) -> &'static [u8] {
        match self {
            Self::LoadString { .. } => b"simplexml_load_string",
            Self::LoadFile { .. } => b"simplexml_load_file",
            Self::Construct { .. } => b"SimpleXMLElement::__construct",
        }
    }

    /// Returns whether parse failure throws instead of returning false.
    fn throws_on_parse_failure(&self) -> bool {
        matches!(self, Self::Construct { .. })
    }

    /// Returns the concrete SimpleXMLElement wrapper discriminator for a new result.
    fn wrapper_kind(&self) -> u64 {
        match self {
            Self::LoadString { class_kind } | Self::LoadFile { class_kind } => {
                *class_kind
            }
            Self::Construct { .. } => 0,
        }
    }
}

/// One validated source retained without borrowing the request byte buffer.
enum ParseSource {
    Bytes(Vec<u8>),
    File(Vec<u8>),
}

/// Validated parse inputs that can cross a callback-capable execution boundary.
pub(in crate::dispatch) struct PreparedParse {
    kind: ParseKind,
    source: ParseSource,
    options: i32,
    namespace_or_prefix: Option<Vec<u8>>,
    is_prefix: bool,
    use_host_loader: bool,
    emit_warnings: bool,
}

/// Either validated parse work or one already complete PHP-visible result.
pub(in crate::dispatch) enum Preparation {
    Ready(PreparedParse),
    Complete(DispatchResult),
}

/// One parsed graph awaiting publication after the bridge context is reacquired.
pub(in crate::dispatch) enum Execution {
    Parsed {
        prepared: PreparedParse,
        outcome: crate::native::DocumentParseOutcome,
        document_uri: Option<Vec<u8>>,
    },
    Complete(DispatchResult),
}

/// Reports whether one operation belongs to the loader/constructor parse tranche.
pub(in crate::dispatch) fn handles(operation_key: &str) -> bool {
    matches!(
        operation_key,
        "function:simplexml_load_string"
            | "function:simplexml_load_file"
            | "method:simplexmlelement::__construct"
    )
}

/// Validates one parse operation while the native context remains borrowed.
pub(in crate::dispatch) fn prepare(
    context: &Context,
    operation_key: &str,
    request: &Request,
) -> Result<Preparation, ()> {
    let use_host_loader =
        context.external_entity_loader.is_some() || context.entity_loader_disabled;
    let emit_warnings = !context.internal_errors;
    match operation_key {
        "function:simplexml_load_string" => {
            prepare_loader(context, request, false, use_host_loader, emit_warnings)
        }
        "function:simplexml_load_file" => {
            prepare_loader(context, request, true, use_host_loader, emit_warnings)
        }
        "method:simplexmlelement::__construct" => {
            prepare_constructor(context, request, use_host_loader, emit_warnings)
        }
        _ => Err(()),
    }
}

/// Validates one public SimpleXML loader and copies all parse inputs.
fn prepare_loader(
    context: &Context,
    request: &Request,
    file: bool,
    use_host_loader: bool,
    emit_warnings: bool,
) -> Result<Preparation, ()> {
    require_no_receiver(request)?;
    if request.values.is_empty() || request.values.len() > 5 {
        return Err(());
    }
    let callable = if file {
        b"simplexml_load_file()".as_slice()
    } else {
        b"simplexml_load_string()".as_slice()
    };
    let data = request.byte_string(0)?.to_vec();
    if file && data.contains(&0) {
        return Ok(Preparation::Complete(DispatchResult::value_error(
            b"simplexml_load_file(): Argument #1 ($filename) must not contain any null bytes",
        )));
    }
    if !file && data.len() > i32::MAX as usize {
        return Ok(Preparation::Complete(DispatchResult::value_error(
            b"simplexml_load_string(): Argument #1 ($data) is too long",
        )));
    }
    let class_kind = match super::resolve_class_kind(
        context,
        super::optional_nullable_bytes(request, 1)?,
        callable,
    ) {
        Ok(class_kind) => class_kind,
        Err(result) => return Ok(Preparation::Complete(result)),
    };
    let options = if request.values.len() > 2 {
        request.integer(2)?
    } else {
        0
    };
    let namespace_or_prefix = super::optional_bytes(request, 3)?;
    let is_prefix = super::optional_boolean(request, 4)?;
    if !file && namespace_or_prefix.len() > i32::MAX as usize {
        return Ok(Preparation::Complete(DispatchResult::value_error(
            b"simplexml_load_string(): Argument #4 ($namespace_or_prefix) is too long",
        )));
    }
    let Ok(options) = i32::try_from(options) else {
        let message = if file {
            b"simplexml_load_file(): Argument #3 ($options) is too large".as_slice()
        } else {
            b"simplexml_load_string(): Argument #3 ($options) is too large".as_slice()
        };
        return Ok(Preparation::Complete(DispatchResult::value_error(message)));
    };
    let namespace_or_prefix =
        (!namespace_or_prefix.is_empty()).then_some(namespace_or_prefix);
    Ok(Preparation::Ready(PreparedParse {
        kind: if file {
            ParseKind::LoadFile { class_kind }
        } else {
            ParseKind::LoadString { class_kind }
        },
        source: if file {
            ParseSource::File(data)
        } else {
            ParseSource::Bytes(data)
        },
        options,
        namespace_or_prefix,
        is_prefix,
        use_host_loader,
        emit_warnings,
    }))
}

/// Validates one constructor invocation without changing its current receiver.
fn prepare_constructor(
    context: &Context,
    request: &Request,
    use_host_loader: bool,
    emit_warnings: bool,
) -> Result<Preparation, ()> {
    if request.values.is_empty() || request.values.len() > 5 {
        return Err(());
    }
    if request.header.receiver != 0 {
        super::object(context, request.header.receiver)?;
    }
    let data = request.byte_string(0)?.to_vec();
    if data.len() > i32::MAX as usize {
        return Ok(Preparation::Complete(DispatchResult::exception(
            b"SimpleXMLElement::__construct(): Argument #1 ($data) is too long",
        )));
    }
    let options = if request.values.len() > 1 {
        request.integer(1)?
    } else {
        0
    };
    let data_is_url = super::optional_boolean(request, 2)?;
    let namespace_or_prefix = super::optional_bytes(request, 3)?;
    let is_prefix = super::optional_boolean(request, 4)?;
    if namespace_or_prefix.len() > i32::MAX as usize {
        return Ok(Preparation::Complete(DispatchResult::exception(
            b"SimpleXMLElement::__construct(): Argument #4 ($namespaceOrPrefix) is too long",
        )));
    }
    let Ok(options) = i32::try_from(options) else {
        return Ok(Preparation::Complete(DispatchResult::exception(
            b"SimpleXMLElement::__construct(): Argument #2 ($options) is invalid",
        )));
    };
    let namespace_or_prefix =
        (!namespace_or_prefix.is_empty()).then_some(namespace_or_prefix);
    Ok(Preparation::Ready(PreparedParse {
        kind: ParseKind::Construct {
            receiver: request.header.receiver,
        },
        source: if data_is_url {
            ParseSource::File(data)
        } else {
            ParseSource::Bytes(data)
        },
        options,
        namespace_or_prefix,
        is_prefix,
        use_host_loader,
        emit_warnings,
    }))
}

/// Reads and parses prepared XML without retaining a mutable bridge-context borrow.
pub(in crate::dispatch) fn execute(
    context_id: u64,
    host: crate::context::Host,
    stream_context: Option<u64>,
    prepared: PreparedParse,
) -> Result<Execution, ()> {
    let (source, input_name, document_uri) = match &prepared.source {
        ParseSource::Bytes(source) => (source.clone(), None, None),
        ParseSource::File(path) => {
            let read = document_io::read_source(
                host,
                stream_context,
                path,
                std::str::from_utf8(prepared.kind.warning_method()).map_err(|_| ())?,
            );
            match read {
                Ok(Some((source, uri, input_name))) => {
                    (source, Some(input_name), Some(uri))
                }
                Ok(None) => {
                    return Ok(Execution::Complete(file_open_failure(&prepared, path)));
                }
                Err(HostCallError::PendingThrowable) => {
                    return Ok(Execution::Complete(
                        DispatchResult::pending_host_throwable(),
                    ));
                }
                Err(HostCallError::Abi) => return Err(()),
            }
        }
    };
    let outcome = if prepared.use_host_loader {
        crate::native::document_parse_xml_with_host(
            &source,
            prepared.options,
            None,
            input_name.as_deref(),
            context_id,
        )?
    } else {
        crate::native::document_parse_xml(
            &source,
            prepared.options,
            None,
            input_name.as_deref(),
        )?
    };
    if prepared.use_host_loader {
        match u32::try_from(outcome.host_status).map_err(|_| ())? {
            0 => {}
            STATUS_THROW => {
                free_document(outcome.document);
                return Ok(Execution::Complete(
                    DispatchResult::pending_host_throwable(),
                ));
            }
            STATUS_ABI_ERROR => {
                free_document(outcome.document);
                return Err(());
            }
            _ => {
                free_document(outcome.document);
                return Err(());
            }
        }
    }
    Ok(Execution::Parsed {
        prepared,
        outcome,
        document_uri,
    })
}

/// Publishes a completely parsed XML document into one fresh or existing view.
pub(in crate::dispatch) fn finish(
    context: &mut Context,
    execution: Execution,
) -> Result<DispatchResult, ()> {
    let Execution::Parsed {
        prepared,
        outcome,
        document_uri,
    } = execution
    else {
        return Err(());
    };
    let emit_warnings = !context.internal_errors;
    record_errors(context, &outcome.errors);
    let Some(pointer) = outcome.document else {
        let result = parse_failure(&prepared);
        return Ok(with_parse_warnings(
            result,
            &prepared,
            &outcome.errors,
            emit_warnings,
        ));
    };
    let root = crate::native::document_element(pointer);
    if let Some(uri) = document_uri.as_deref() {
        if !crate::native::document_set_url(pointer, uri) {
            unsafe {
                crate::native::document_free(pointer);
            }
            return Err(());
        }
    }
    let graph = DocumentGraph::new_unclaimed_xml(pointer);
    let iterator = SimpleXmlIteratorState::direct(
        prepared.namespace_or_prefix.clone(),
        prepared.is_prefix,
    );
    let result = match prepared.kind {
        ParseKind::Construct { receiver } if receiver != 0 => {
            if context.clear_simplexml_iterator_current(receiver).is_err() {
                drop(graph);
                return Err(());
            }
            let Ok(simplexml) = super::object_mut(context, receiver) else {
                drop(graph);
                return Err(());
            };
            if let Some(root) = root {
                simplexml.replace_parsed_view(root, graph, iterator);
            } else {
                simplexml.replace_parsed_document_without_node(graph, iterator);
            }
            DispatchResult::null()
        }
        kind => {
            let object = if let Some(root) = root {
                SimpleXmlObject::new(
                    root,
                    graph,
                    kind.wrapper_kind(),
                    iterator,
                )
            } else {
                SimpleXmlObject::new_without_node(
                    graph,
                    kind.wrapper_kind(),
                    iterator,
                )
            };
            super::fresh_result(context, object)
        }
    };
    Ok(with_parse_warnings(
        result,
        &prepared,
        &outcome.errors,
        emit_warnings,
    ))
}

/// Runs one callback-free parse directly for defensive non-reentrant dispatch.
pub(in crate::dispatch) fn dispatch_borrowed(
    context: &mut Context,
    operation_key: &str,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let prepared = match prepare(context, operation_key, request)? {
        Preparation::Ready(prepared) => prepared,
        Preparation::Complete(result) => return Ok(result),
    };
    if prepared.use_host_loader
        || matches!(&prepared.source, ParseSource::File(path) if document_io::requires_host_stream(path))
    {
        return Err(());
    }
    let execution = execute(0, context.host, context.stream_context, prepared)?;
    match execution {
        Execution::Parsed { .. } => finish(context, execution),
        Execution::Complete(result) => Ok(result),
    }
}

/// Frees an uncommitted parsed document if context re-entry prevents publication.
pub(in crate::dispatch) fn free_execution(execution: Execution) {
    if let Execution::Parsed { outcome, .. } = execution {
        free_document(outcome.document);
    }
}

/// Produces the loader false or constructor exception parse-failure result.
fn parse_failure(prepared: &PreparedParse) -> DispatchResult {
    if prepared.kind.throws_on_parse_failure() {
        DispatchResult::exception(b"String could not be parsed as XML")
    } else {
        DispatchResult::boolean(false)
    }
}

/// Attaches php-src-compatible libxml warnings when internal errors are disabled.
fn with_parse_warnings(
    result: DispatchResult,
    prepared: &PreparedParse,
    errors: &[crate::objects::LibxmlErrorObject],
    emit_warnings: bool,
) -> DispatchResult {
    if emit_warnings {
        result.with_libxml_parser_warnings(
            prepared.kind.warning_method(),
            errors,
            prepared.options,
        )
    } else {
        result
    }
}

/// Builds the file-open warning plus the loader/constructor failure contract.
fn file_open_failure(prepared: &PreparedParse, path: &[u8]) -> DispatchResult {
    let mut warning = b"Warning: ".to_vec();
    warning.extend_from_slice(prepared.kind.warning_method());
    warning.extend_from_slice(b"(): I/O warning : failed to load external entity \"");
    warning.extend_from_slice(path);
    warning.extend_from_slice(b"\"\n");
    if prepared.emit_warnings {
        parse_failure(prepared).with_warning(&warning)
    } else {
        parse_failure(prepared)
    }
}

/// Frees one optional native document that did not enter a shared graph.
fn free_document(document: Option<usize>) {
    if let Some(document) = document {
        unsafe {
            crate::native::document_free(document);
        }
    }
}
