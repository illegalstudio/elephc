//! Purpose:
//! Implements legacy and modern `C14N()` and `C14NFile()` node methods.
//! Parses PHP's XPath and namespace option arrays before entering pinned libxml2.
//!
//! Called from:
//! - `super::routes::dispatch()` for direct canonicalization and local files.
//! - `super::reentrant::dispatch()` for registered PHP stream output.
//!
//! Key details:
//! - Modern namespace attributes are relinked only inside the native call and restored afterward.
//! - XPath type errors, libxml diagnostics, notices, and false/error results preserve PHP ordering.

use crate::abi::{
    Value, VALUE_ARRAY, VALUE_BOOL, VALUE_BYTES, VALUE_CALLABLE, VALUE_FLOAT,
    VALUE_INT, VALUE_MAP, VALUE_NULL, VALUE_OBJECT, VALUE_RESOURCE,
};
use crate::context::Context;
use crate::objects::{DocumentFamily, LibxmlErrorObject};
use crate::request::Request;

use super::{document_io, libxml, DispatchResult};

/// One canonicalized file payload with diagnostics awaiting final output decoration.
pub(super) struct PreparedC14nFile {
    output: document_io::PreparedFile,
    method: &'static [u8],
    errors: Vec<LibxmlErrorObject>,
    generic_errors: bool,
    inclusive_notice: bool,
}

/// Either one writable canonicalized file or an already complete PHP result.
pub(super) enum C14nFilePreparation {
    Ready(PreparedC14nFile),
    Complete(DispatchResult),
}

/// Fully parsed canonicalization inputs independent from ABI record storage.
struct C14nOptions {
    document: usize,
    node: usize,
    node_is_document: bool,
    modern: bool,
    exclusive: bool,
    with_comments: bool,
    query: Option<Vec<u8>>,
    namespaces: Vec<(Vec<u8>, Vec<u8>)>,
    inclusive_prefixes: Vec<Vec<u8>>,
    inclusive_notice: bool,
    method: &'static [u8],
}

/// Runs one in-memory canonicalization and materializes its PHP result.
pub(super) fn canonicalize(
    context: &mut Context,
    operation_key: &str,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let options = match parse_options(context, operation_key, request)? {
        Ok(options) => options,
        Err(result) => return Ok(result),
    };
    let generic_errors = !context.internal_errors;
    let outcome = run(&options, generic_errors)?;
    finish_outcome(
        context,
        options.method,
        options.inclusive_notice,
        generic_errors,
        outcome,
        false,
    )
}

/// Canonicalizes and writes one plain or local-file URL without host callbacks.
pub(super) fn canonicalize_file(
    context: &mut Context,
    operation_key: &str,
    request: &Request,
) -> Result<DispatchResult, ()> {
    match prepare_file(context, operation_key, request)? {
        C14nFilePreparation::Ready(prepared) => write_local_file(prepared),
        C14nFilePreparation::Complete(result) => Ok(result),
    }
}

/// Prepares one C14N file payload so registered wrappers can run without a context borrow.
pub(super) fn prepare_file(
    context: &mut Context,
    operation_key: &str,
    request: &Request,
) -> Result<C14nFilePreparation, ()> {
    let options = match parse_options(context, operation_key, request)? {
        Ok(options) => options,
        Err(result) => return Ok(C14nFilePreparation::Complete(result)),
    };
    let path = request.byte_string(0)?.to_vec();
    let generic_errors = !context.internal_errors;
    let outcome = run(&options, generic_errors)?;
    libxml::record_errors(context, &outcome.errors);
    let base = outcome_result(
        options.method,
        options.inclusive_notice,
        generic_errors,
        &outcome,
        true,
    );
    if outcome.status != 0 {
        return Ok(C14nFilePreparation::Complete(base));
    }
    Ok(C14nFilePreparation::Ready(PreparedC14nFile {
        output: document_io::PreparedFile {
            path,
            bytes: outcome.bytes,
            method: method_string(options.method)?,
        },
        method: options.method,
        errors: outcome.errors,
        generic_errors,
        inclusive_notice: options.inclusive_notice,
    }))
}

/// Writes one prepared C14N payload through a registered PHP stream wrapper.
pub(super) fn write_host_file(
    host: crate::context::Host,
    stream_context: Option<u64>,
    prepared: PreparedC14nFile,
) -> Result<DispatchResult, ()> {
    for warning in visible_warnings(
        prepared.method,
        prepared.inclusive_notice,
        prepared.generic_errors,
        &prepared.errors,
    ) {
        if let Err(error) = crate::host::emit_warning(host, &warning) {
            return match error {
                crate::host::HostCallError::Abi => Err(()),
                crate::host::HostCallError::PendingThrowable => {
                    Ok(DispatchResult::pending_host_throwable())
                }
            };
        }
    }
    document_io::write_host_stream(host, stream_context, prepared.output)
}

/// Parses receiver, booleans, XPath maps, and inclusive namespace prefixes.
fn parse_options(
    context: &Context,
    operation_key: &str,
    request: &Request,
) -> Result<Result<C14nOptions, DispatchResult>, ()> {
    let (modern, file_mode, method, max_values) = match operation_key {
        "method:domnode::c14n" => (
            false,
            false,
            b"DOMNode::C14N".as_slice(),
            4,
        ),
        "method:dom\\node::c14n" => (
            true,
            false,
            b"Dom\\Node::C14N".as_slice(),
            4,
        ),
        "method:domnode::c14nfile" => (
            false,
            true,
            b"DOMNode::C14NFile".as_slice(),
            5,
        ),
        "method:dom\\node::c14nfile" => (
            true,
            true,
            b"Dom\\Node::C14NFile".as_slice(),
            5,
        ),
        _ => return Err(()),
    };
    if request.values.len() > max_values || (file_mode && request.values.is_empty()) {
        return Err(());
    }
    if file_mode && request.byte_string(0)?.contains(&0) {
        let mut message = method.to_vec();
        message.extend_from_slice(
            b"(): Argument #1 ($uri) must not contain any null bytes",
        );
        return Ok(Err(DispatchResult::value_error(&message)));
    }
    let first_option = usize::from(file_mode);
    let exclusive = optional_boolean(request, first_option, false)?;
    let with_comments = optional_boolean(request, first_option + 1, false)?;
    let xpath_index = first_option + 2;
    let prefix_index = first_option + 3;
    let xpath_argument = 3 + first_option;
    let (query, namespaces) =
        match parse_xpath(request, xpath_index, method, xpath_argument)? {
            Ok(options) => options,
            Err(result) => return Ok(Err(result)),
        };
    let (inclusive_prefixes, has_prefix_array) =
        parse_inclusive_prefixes(request, prefix_index)?;
    let (node, graph, node_is_document) =
        super::receiver_pointer_and_graph(context, request.header.receiver)?;
    if modern == (graph.family() == DocumentFamily::Legacy) {
        return Err(());
    }
    if !modern && !node_is_document {
        let receiver = super::node(context, request.header.receiver)?;
        if !receiver.owner_document_exposed() {
            return Ok(Err(DispatchResult::error(
                b"Node must be associated with a document",
            )));
        }
    }
    Ok(Ok(C14nOptions {
        document: graph.pointer(),
        node,
        node_is_document,
        modern,
        exclusive,
        with_comments,
        query,
        namespaces,
        inclusive_prefixes,
        inclusive_notice: has_prefix_array && !exclusive,
        method,
    }))
}

/// Reads one optional ABI boolean while accepting an omitted value as its default.
fn optional_boolean(
    request: &Request,
    index: usize,
    default: bool,
) -> Result<bool, ()> {
    if index >= request.values.len() {
        Ok(default)
    } else {
        request.boolean(index)
    }
}

/// Parses PHP's nullable XPath array and exact `query`/`namespaces` options.
fn parse_xpath(
    request: &Request,
    index: usize,
    method: &[u8],
    argument: usize,
) -> Result<Result<(Option<Vec<u8>>, Vec<(Vec<u8>, Vec<u8>)>), DispatchResult>, ()> {
    let Some(xpath) = request.values.get(index) else {
        return Ok(Ok((None, Vec::new())));
    };
    if xpath.tag == VALUE_NULL {
        return Ok(Ok((None, Vec::new())));
    }
    let entries = match xpath.tag {
        VALUE_MAP => request.map_values(index)?,
        VALUE_ARRAY => &[],
        _ => return Err(()),
    };
    let mut query = None;
    let mut namespaces = Vec::new();
    for pair in entries.chunks_exact(2) {
        let key = match request.bytes_for_value(&pair[0]) {
            Ok(key) => key,
            Err(()) => continue,
        };
        if key == b"query" {
            if pair[1].tag != VALUE_BYTES {
                return Ok(Err(query_type_error(
                    request,
                    method,
                    argument,
                    &pair[1],
                )?));
            }
            query = Some(request.bytes_for_value(&pair[1])?.to_vec());
        } else if key == b"namespaces" && pair[1].tag == VALUE_MAP {
            namespaces = parse_xpath_namespaces(request, &pair[1])?;
        }
    }
    let Some(query) = query else {
        let mut message = method.to_vec();
        message.extend_from_slice(b"(): Argument #");
        message.extend_from_slice(argument.to_string().as_bytes());
        message.extend_from_slice(b" ($xpath) must have a \"query\" key");
        return Ok(Err(DispatchResult::value_error(&message)));
    };
    Ok(Ok((Some(query), namespaces)))
}

/// Extracts only string-key/string-value entries from a non-packed namespace map.
fn parse_xpath_namespaces(
    request: &Request,
    namespaces: &Value,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, ()> {
    let mut parsed = Vec::new();
    for pair in request.nested_values(namespaces)?.chunks_exact(2) {
        let Ok(prefix) = request.bytes_for_value(&pair[0]) else {
            continue;
        };
        let Ok(uri) = request.bytes_for_value(&pair[1]) else {
            continue;
        };
        parsed.push((prefix.to_vec(), uri.to_vec()));
    }
    Ok(parsed)
}

/// Collects string values from either packed or associative prefix arrays.
fn parse_inclusive_prefixes(
    request: &Request,
    index: usize,
) -> Result<(Vec<Vec<u8>>, bool), ()> {
    let Some(prefixes) = request.values.get(index) else {
        return Ok((Vec::new(), false));
    };
    if prefixes.tag == VALUE_NULL {
        return Ok((Vec::new(), false));
    }
    let values = match prefixes.tag {
        VALUE_ARRAY => request.array_values(index)?,
        VALUE_MAP => request.map_values(index)?,
        _ => return Err(()),
    };
    let mut parsed = Vec::new();
    if prefixes.tag == VALUE_MAP {
        for pair in values.chunks_exact(2) {
            if let Ok(prefix) = request.bytes_for_value(&pair[1]) {
                parsed.push(prefix.to_vec());
            }
        }
    } else {
        for value in values {
            if let Ok(prefix) = request.bytes_for_value(value) {
                parsed.push(prefix.to_vec());
            }
        }
    }
    Ok((parsed, true))
}

/// Builds php-src's exact TypeError for a non-string XPath query option.
fn query_type_error(
    request: &Request,
    method: &[u8],
    argument: usize,
    value: &Value,
) -> Result<DispatchResult, ()> {
    let mut message = method.to_vec();
    message.extend_from_slice(b"(): Argument #");
    message.extend_from_slice(argument.to_string().as_bytes());
    message.extend_from_slice(
        b" ($xpath) \"query\" option must be a string, ",
    );
    message.extend_from_slice(&php_value_name(request, value)?);
    message.extend_from_slice(b" given");
    Ok(DispatchResult::type_error(&message))
}

/// Returns Zend's PHP-visible value name for one serialized query option.
fn php_value_name(request: &Request, value: &Value) -> Result<Vec<u8>, ()> {
    match value.tag {
        VALUE_NULL => Ok(b"null".to_vec()),
        VALUE_BOOL if value.payload0 == 0 => Ok(b"false".to_vec()),
        VALUE_BOOL if value.payload0 == 1 => Ok(b"true".to_vec()),
        VALUE_INT => Ok(b"int".to_vec()),
        VALUE_FLOAT => Ok(b"float".to_vec()),
        VALUE_BYTES => Ok(b"string".to_vec()),
        VALUE_ARRAY | VALUE_MAP => Ok(b"array".to_vec()),
        VALUE_OBJECT => {
            let fields = request.nested_values(value)?;
            fields
                .first()
                .ok_or(())
                .and_then(|field| request.bytes_for_value(field))
                .map(<[u8]>::to_vec)
        }
        VALUE_RESOURCE => Ok(b"resource".to_vec()),
        VALUE_CALLABLE => Ok(b"Closure".to_vec()),
        _ => Err(()),
    }
}

/// Executes the native canonicalization adapter with owned parsed options.
fn run(
    options: &C14nOptions,
    generic_errors: bool,
) -> Result<crate::native::C14nOutcome, ()> {
    crate::native::node_c14n(
        options.document,
        options.node,
        options.node_is_document,
        options.modern,
        options.exclusive,
        options.with_comments,
        options.query.as_deref(),
        &options.namespaces,
        &options.inclusive_prefixes,
        generic_errors,
    )
}

/// Records libxml errors and returns the family-appropriate memory or file result.
fn finish_outcome(
    context: &mut Context,
    method: &[u8],
    inclusive_notice: bool,
    generic_errors: bool,
    outcome: crate::native::C14nOutcome,
    file_mode: bool,
) -> Result<DispatchResult, ()> {
    libxml::record_errors(context, &outcome.errors);
    Ok(outcome_result(
        method,
        inclusive_notice,
        generic_errors,
        &outcome,
        file_mode,
    ))
}

/// Converts one native status into PHP's string, integer placeholder, false, or exception.
fn outcome_result(
    method: &[u8],
    inclusive_notice: bool,
    generic_errors: bool,
    outcome: &crate::native::C14nOutcome,
    file_mode: bool,
) -> DispatchResult {
    let result = match outcome.status {
        0 if file_mode => DispatchResult::integer(0),
        0 => DispatchResult::bytes(outcome.bytes.clone()),
        1 => DispatchResult::error(b"XPath query did not return a nodeset"),
        2 => DispatchResult::boolean(false),
        4 => DispatchResult::dom_exception(
            3,
            b"Canonicalization can only happen on nodes attached to a document.",
        ),
        _ => return DispatchResult::boolean(false),
    };
    decorate_result(
        result,
        method,
        inclusive_notice && outcome.status != 1,
        generic_errors,
        &outcome.errors,
    )
}

/// Attaches visible libxml warnings followed by PHP's inclusive-prefix notice.
fn decorate_result(
    mut result: DispatchResult,
    method: &[u8],
    inclusive_notice: bool,
    generic_errors: bool,
    errors: &[LibxmlErrorObject],
) -> DispatchResult {
    if generic_errors {
        result = result.with_libxml_warnings(method, errors);
    }
    if inclusive_notice {
        let mut notice = b"Notice: ".to_vec();
        notice.extend_from_slice(method);
        notice.extend_from_slice(
            b"(): Inclusive namespace prefixes only allowed in exclusive mode.\n",
        );
        result = result.with_warning(&notice);
    }
    result
}

/// Formats canonicalization warnings and notices in their PHP-observable order.
fn visible_warnings(
    method: &[u8],
    inclusive_notice: bool,
    generic_errors: bool,
    errors: &[LibxmlErrorObject],
) -> Vec<Vec<u8>> {
    let mut warnings = Vec::new();
    if generic_errors {
        warnings.reserve(errors.len());
        for error in errors {
            let mut error_message = error.message.as_slice();
            while let Some(trimmed) = error_message.strip_suffix(b"\n") {
                error_message = trimmed;
            }
            let mut warning = b"Warning: ".to_vec();
            warning.extend_from_slice(method);
            warning.extend_from_slice(b"(): ");
            warning.extend_from_slice(error_message);
            warning.push(b'\n');
            warnings.push(warning);
        }
    }
    if inclusive_notice {
        let mut notice = b"Notice: ".to_vec();
        notice.extend_from_slice(method);
        notice.extend_from_slice(
            b"(): Inclusive namespace prefixes only allowed in exclusive mode.\n",
        );
        warnings.push(notice);
    }
    warnings
}

/// Converts stable method bytes to the stream subsystem's static UTF-8 label.
fn method_string(method: &[u8]) -> Result<&'static str, ()> {
    match method {
        b"DOMNode::C14NFile" => Ok("DOMNode::C14NFile"),
        b"Dom\\Node::C14NFile" => Ok("Dom\\Node::C14NFile"),
        _ => Err(()),
    }
}

/// Writes one prepared payload to a local path and attaches visible diagnostics.
fn write_local_file(prepared: PreparedC14nFile) -> Result<DispatchResult, ()> {
    let path = match document_io::resolve_path(&prepared.output.path) {
        Some(path) => path,
        None => return Ok(DispatchResult::boolean(false)),
    };
    let base = decorate_result(
        DispatchResult::boolean(false),
        prepared.method,
        prepared.inclusive_notice,
        prepared.generic_errors,
        &prepared.errors,
    );
    match std::fs::write(&path, &prepared.output.bytes) {
        Ok(()) => Ok(decorate_result(
            DispatchResult::integer(
                i64::try_from(prepared.output.bytes.len()).map_err(|_| ())?,
            ),
            prepared.method,
            prepared.inclusive_notice,
            prepared.generic_errors,
            &prepared.errors,
        )),
        Err(error) => {
            let mut warning = b"Warning: ".to_vec();
            warning.extend_from_slice(prepared.method);
            warning.push(b'(');
            warning.extend_from_slice(&prepared.output.path);
            warning.extend_from_slice(b"): Failed to open stream: ");
            warning.extend_from_slice(local_io_error(&error).as_bytes());
            warning.push(b'\n');
            Ok(base.with_warning(&warning))
        }
    }
}

/// Normalizes Rust's OS suffix away from PHP's local stream-open message.
fn local_io_error(error: &std::io::Error) -> String {
    let message = error.to_string();
    match message.rfind(" (os error ") {
        Some(index) => message[..index].to_string(),
        None => message,
    }
}
