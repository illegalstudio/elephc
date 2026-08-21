//! Purpose:
//! Implements legacy `DOMXPath` and modern `Dom\XPath` bridge operations.
//! Retains document and namespace state while delegating expression evaluation to pinned libxml2.
//!
//! Called from:
//! - `super::routes::dispatch()` and `super::routes::properties::dispatch()`.
//!
//! Key details:
//! - Context-node namespace registration is temporary and never mutates persistent bindings.
//! - Legacy evaluation failures return false while modern failures throw after ordered warnings.

use std::rc::Rc;

use crate::abi::{
    STATUS_ABI_ERROR, STATUS_THROW, VALUE_ARRAY, VALUE_BOOL, VALUE_BYTES,
    VALUE_BRIDGE_HANDLE, VALUE_CALLABLE, VALUE_FLOAT, VALUE_INT, VALUE_MAP,
    VALUE_NULL, VALUE_OBJECT, Value,
};
use crate::context::Context;
use crate::handles::handle_kind;
use crate::objects::{
    CollectionKind, DocumentFamily, DocumentGraph, NativeObject,
    NamespaceNodeAllocation, XPathObject, HANDLE_DOCUMENT, HANDLE_XPATH,
};
use crate::request::Request;

use super::{
    canonical_document_handle, canonical_namespace_node_handle,
    canonical_pointer_handle, document,
    receiver_pointer_and_graph, require_no_values, xpath, xpath_mut, DispatchResult,
    xpath_collection_result,
};

/// One validated XPath evaluation detached from the bridge context borrow.
pub(super) struct PreparedXPathEvaluation {
    graph: Rc<DocumentGraph>,
    host: crate::context::Host,
    context_node: Option<usize>,
    modern: bool,
    register_node_namespaces: bool,
    force_nodeset: bool,
    expression: Vec<u8>,
    namespaces: Vec<(Vec<u8>, Vec<u8>)>,
    callbacks: Vec<(Vec<u8>, Vec<u8>)>,
    method: &'static [u8],
}

/// Either one runnable XPath request or an already complete PHP result.
pub(super) enum XPathEvaluationPreparation {
    Ready(PreparedXPathEvaluation),
    Complete(DispatchResult),
}

/// Native XPath output retained together with its document graph.
pub(super) struct ExecutedXPathEvaluation {
    prepared: PreparedXPathEvaluation,
    outcome: crate::native::XPathOutcome,
}

impl ExecutedXPathEvaluation {
    /// Detaches callback-result releases so DOM finalizers can re-enter after the context borrow ends.
    pub(super) fn take_pending_host_actions(
        &mut self,
    ) -> Vec<crate::context::PendingHostAction> {
        let host = self.prepared.host;
        self.outcome
            .take_callback_leases()
            .into_iter()
            .map(|lease| crate::context::PendingHostAction::ReleaseResult {
                host,
                result_id: lease.into_id(),
            })
            .collect()
    }
}

/// Constructs one legacy or modern XPath context around an authoritative document.
pub(super) fn construct(
    context: &mut Context,
    request: &Request,
    modern: bool,
) -> Result<DispatchResult, ()> {
    if !(1..=2).contains(&request.values.len()) {
        return Err(());
    }
    let document = document(context, request.bridge_handle(0)?)?;
    let family = document.family();
    if modern == matches!(family, DocumentFamily::Legacy) {
        return Err(());
    }
    let register_node_namespaces = if request.values.len() == 2 {
        request.boolean(1)?
    } else {
        true
    };
    let handle = context.native_objects.insert(
        HANDLE_XPATH,
        NativeObject::XPath(XPathObject::new(
            document.graph(),
            register_node_namespaces,
        )),
    );
    Ok(DispatchResult::bridge_handle(handle))
}

/// Returns the canonical document wrapper retained by one XPath context.
pub(super) fn document_property(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let graph = xpath(context, request.header.receiver)?.document();
    Ok(DispatchResult::bridge_handle(canonical_document_handle(
        context, graph,
    )))
}

/// Returns the current default for registering context-node namespaces.
pub(super) fn register_node_namespaces(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    Ok(DispatchResult::boolean(
        xpath(context, request.header.receiver)?.register_node_namespaces(),
    ))
}

/// Updates the default for registering context-node namespaces.
pub(super) fn set_register_node_namespaces(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.boolean(0)?;
    xpath_mut(context, request.header.receiver)?
        .set_register_node_namespaces(value);
    Ok(DispatchResult::null())
}

/// Registers one persistent prefix-to-namespace binding.
pub(super) fn register_namespace(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let prefix = request.byte_string(0)?.to_vec();
    let namespace_uri = request.byte_string(1)?.to_vec();
    if prefix.is_empty() {
        return Ok(DispatchResult::boolean(false));
    }
    xpath_mut(context, request.header.receiver)?
        .register_namespace(prefix, namespace_uri);
    Ok(DispatchResult::boolean(true))
}

/// Registers one custom namespace function with balanced callable ownership.
pub(super) fn register_php_function_ns(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    if request.values.len() != 3 {
        return Err(());
    }
    let namespace_uri = request.byte_string(0)?.to_vec();
    let name = request.byte_string(1)?.to_vec();
    let descriptor = request.callable_descriptor(2)?;
    if namespace_uri.contains(&0) {
        return Ok(argument_null_byte_error(method, 1, b"namespaceURI"));
    }
    if name.contains(&0) {
        return Ok(argument_null_byte_error(method, 2, b"name"));
    }
    if namespace_uri == b"http://php.net/xpath" {
        let mut message = method.to_vec();
        message.extend_from_slice(
            b"(): Argument #1 ($namespaceURI) must not be \"http://php.net/xpath\" because it is reserved by PHP",
        );
        return Ok(DispatchResult::value_error(&message));
    }
    if !crate::native::validate_ncname(&name) {
        let mut message = method.to_vec();
        message.extend_from_slice(
            b"(): Argument #2 ($name) must be a valid callback name",
        );
        return Ok(DispatchResult::value_error(&message));
    }
    xpath(context, request.header.receiver)?;

    match crate::host::retain_callable(context.host, descriptor) {
        Ok(()) => {}
        Err(crate::host::HostCallError::PendingThrowable) => {
            return Ok(DispatchResult::pending_host_throwable());
        }
        Err(crate::host::HostCallError::Abi) => return Err(()),
    }
    let previous = xpath_mut(context, request.header.receiver)?
        .register_callback(namespace_uri, name, descriptor);
    let mut result = DispatchResult::null();
    if let Some(previous) = previous {
        result = result.with_pending_host_action(
            crate::context::PendingHostAction::ReleaseCallable {
                host: context.host,
                descriptor: previous,
            },
        );
    }
    Ok(result)
}

/// Updates PHP's reserved `php:function*` callback registry with balanced ownership.
pub(super) fn register_php_functions(
    context: &mut Context,
    request: &Request,
    method: &[u8],
) -> Result<DispatchResult, ()> {
    if request.values.len() > 1 {
        return Err(());
    }
    xpath(context, request.header.receiver)?;
    if request.values.is_empty() || request.value(0)?.tag == VALUE_NULL {
        xpath_mut(context, request.header.receiver)?
            .allow_all_php_callbacks();
        return Ok(DispatchResult::null());
    }

    let mut registrations = Vec::<(Vec<u8>, u64)>::new();
    match request.value(0)?.tag {
        VALUE_BYTES => {
            let name = request.byte_string(0)?;
            if !valid_php_callback_name(name) {
                return Ok(register_php_functions_name_error(method, false));
            }
            let Some(descriptor) =
                resolve_php_callback(context.host, name)?
            else {
                return Ok(register_php_functions_callable_error(
                    method,
                    name,
                    false,
                ));
            };
            upsert_registration(
                &mut registrations,
                name.to_vec(),
                descriptor,
            );
        }
        VALUE_BOOL | VALUE_INT | VALUE_FLOAT => {
            let name = registration_scalar_name(request.value(0)?)?;
            if !valid_php_callback_name(&name) {
                return Ok(register_php_functions_name_error(method, false));
            }
            let Some(descriptor) =
                resolve_php_callback(context.host, &name)?
            else {
                return Ok(register_php_functions_callable_error(
                    method,
                    &name,
                    false,
                ));
            };
            upsert_registration(&mut registrations, name, descriptor);
        }
        VALUE_ARRAY => {
            for value in request.array_values(0)? {
                let name = if value.tag == VALUE_BYTES {
                    request.bytes_for_value(value)?
                } else if value.tag == VALUE_CALLABLE {
                    return Ok(DispatchResult::error(
                        b"Object of class Closure could not be converted to string",
                    ));
                } else if let Some(class_name) =
                    nested_object_class(request, value)?
                {
                    return Ok(object_string_conversion_error(class_name));
                } else {
                    return Ok(register_php_functions_array_value_error(
                        method,
                        None,
                    ));
                };
                if !valid_php_callback_name(name) {
                    return Ok(register_php_functions_name_error(method, true));
                }
                let Some(descriptor) =
                    resolve_php_callback(context.host, name)?
                else {
                    return Ok(register_php_functions_callable_error(
                        method,
                        name,
                        true,
                    ));
                };
                upsert_registration(
                    &mut registrations,
                    name.to_vec(),
                    descriptor,
                );
            }
        }
        VALUE_MAP => {
            let values = request.map_values(0)?;
            for pair in values.chunks_exact(2) {
                let key = &pair[0];
                let callable = &pair[1];
                let alias = if key.tag == VALUE_BYTES {
                    request.bytes_for_value(key)?
                } else if key.tag == VALUE_INT {
                    if callable.tag == VALUE_CALLABLE {
                        return Ok(DispatchResult::error(
                            b"Object of class Closure could not be converted to string",
                        ));
                    }
                    if let Some(class_name) =
                        nested_object_class(request, callable)?
                    {
                        return Ok(object_string_conversion_error(class_name));
                    }
                    request.bytes_for_value(callable)?
                } else {
                    return Err(());
                };
                if !valid_php_callback_name(alias) {
                    return Ok(register_php_functions_name_error(method, true));
                }
                let descriptor = if callable.tag == VALUE_CALLABLE {
                    (callable.flags == 0
                        && callable.payload0 != 0
                        && callable.payload1 == 0)
                        .then_some(callable.payload0)
                        .ok_or(())?
                } else if callable.tag == VALUE_BYTES {
                    let callable_name = request.bytes_for_value(callable)?;
                    let Some(descriptor) = resolve_php_callback(
                        context.host,
                        callable_name,
                    )? else {
                        return Ok(register_php_functions_callable_error(
                            method,
                            callable_name,
                            true,
                        ));
                    };
                    descriptor
                } else if let Some(detail) =
                    callable_array_error_detail(request, callable)?
                {
                    return Ok(register_php_functions_array_value_error(
                        method,
                        Some(&detail),
                    ));
                } else {
                    return Ok(register_php_functions_array_value_error(
                        method,
                        None,
                    ));
                };
                upsert_registration(
                    &mut registrations,
                    alias.to_vec(),
                    descriptor,
                );
            }
        }
        VALUE_CALLABLE => {
            let mut message = method.to_vec();
            message.extend_from_slice(
                b"(): Argument #1 ($restrict) must be of type array|string|null, Closure given",
            );
            return Ok(DispatchResult::type_error(&message));
        }
        VALUE_OBJECT => {
            let class_name = nested_object_class(request, request.value(0)?)?
                .ok_or(())?;
            let mut message = method.to_vec();
            message.extend_from_slice(
                b"(): Argument #1 ($restrict) must be of type array|string|null, ",
            );
            message.extend_from_slice(class_name);
            message.extend_from_slice(b" given");
            return Ok(DispatchResult::type_error(&message));
        }
        _ => return Err(()),
    }

    let mut retained = Vec::new();
    for (_, descriptor) in &registrations {
        match crate::host::retain_callable(context.host, *descriptor) {
            Ok(()) => retained.push(*descriptor),
            Err(crate::host::HostCallError::PendingThrowable) => {
                release_retained_callbacks(context.host, &retained);
                return Ok(DispatchResult::pending_host_throwable());
            }
            Err(crate::host::HostCallError::Abi) => {
                release_retained_callbacks(context.host, &retained);
                return Err(());
            }
        }
    }

    let mut result = DispatchResult::null();
    let host = context.host;
    let xpath = xpath_mut(context, request.header.receiver)?;
    xpath.restrict_php_callbacks();
    for (alias, descriptor) in registrations {
        if let Some(previous) =
            xpath.register_php_callback(alias, descriptor)
        {
            result = result.with_pending_host_action(
                crate::context::PendingHostAction::ReleaseCallable {
                    host,
                    descriptor: previous,
                },
            );
        }
    }
    Ok(result)
}

/// Coerces one weak internal scalar argument to PHP's callback-name string form.
fn registration_scalar_name(value: &crate::abi::Value) -> Result<Vec<u8>, ()> {
    if value.flags != 0 || value.payload1 != 0 {
        return Err(());
    }
    match value.tag {
        VALUE_BOOL => match value.payload0 {
            0 => Ok(Vec::new()),
            1 => Ok(b"1".to_vec()),
            _ => Err(()),
        },
        VALUE_INT => Ok((value.payload0 as i64).to_string().into_bytes()),
        VALUE_FLOAT => Ok(f64::from_bits(value.payload0).to_string().into_bytes()),
        _ => Err(()),
    }
}

/// Builds php-src's invalid class-method detail for an unresolved callable-array value.
fn callable_array_error_detail(
    request: &Request,
    value: &crate::abi::Value,
) -> Result<Option<Vec<u8>>, ()> {
    if value.tag != VALUE_ARRAY || value.payload1 != 2 {
        return Ok(None);
    }
    let values = request.nested_values(value)?;
    let Some(method) = values
        .get(1)
        .filter(|method| method.tag == VALUE_BYTES)
        .map(|method| request.bytes_for_value(method))
        .transpose()?
    else {
        return Ok(None);
    };
    let class_name = if let Some(class_name) =
        values.first().map(|receiver| nested_object_class(request, receiver)).transpose()?.flatten()
    {
        class_name
    } else if let Some(receiver) =
        values.first().filter(|receiver| receiver.tag == VALUE_BYTES)
    {
        request.bytes_for_value(receiver)?
    } else {
        return Ok(None);
    };
    let mut detail = b"class ".to_vec();
    detail.extend_from_slice(class_name);
    detail.extend_from_slice(b" does not have a method \"");
    detail.extend_from_slice(method);
    detail.push(b'"');
    Ok(Some(detail))
}

/// Quotes arbitrary bytes as one XPath string literal or `concat()` expression.
pub(super) fn quote(request: &Request) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let input = request.byte_string(0)?;
    if !input.contains(&b'\'') {
        return Ok(DispatchResult::bytes(surround(input, b'\'')));
    }
    if !input.contains(&b'"') {
        return Ok(DispatchResult::bytes(surround(input, b'"')));
    }

    let mut output = b"concat(".to_vec();
    let mut cursor = 0;
    while cursor < input.len() {
        let remaining = &input[cursor..];
        let single = remaining
            .iter()
            .position(|byte| *byte == b'\'')
            .unwrap_or(remaining.len());
        let double = remaining
            .iter()
            .position(|byte| *byte == b'"')
            .unwrap_or(remaining.len());
        let length = single.max(double);
        let delimiter = if single > double { b'\'' } else { b'"' };
        output.push(delimiter);
        output.extend_from_slice(&remaining[..length]);
        output.push(delimiter);
        output.push(b',');
        cursor += length;
    }
    *output.last_mut().ok_or(())? = b')';
    Ok(DispatchResult::bytes(output))
}

/// Evaluates one expression and returns its native scalar or static node-set value.
pub(super) fn evaluate(
    context: &mut Context,
    request: &Request,
    modern: bool,
    force_nodeset: bool,
    method: &'static [u8],
) -> Result<DispatchResult, ()> {
    let prepared = match prepare_evaluation(
        context,
        request,
        modern,
        force_nodeset,
        method,
    )? {
        XPathEvaluationPreparation::Ready(prepared) => prepared,
        XPathEvaluationPreparation::Complete(result) => return Ok(result),
    };
    let executed = execute_evaluation(
        0,
        request.header.receiver,
        prepared,
        false,
    )?;
    finish_evaluation(context, executed)
}

/// Validates and snapshots one XPath evaluation before any PHP callback can run.
pub(super) fn prepare_evaluation(
    context: &Context,
    request: &Request,
    modern: bool,
    force_nodeset: bool,
    method: &'static [u8],
) -> Result<XPathEvaluationPreparation, ()> {
    if !(1..=4).contains(&request.values.len()) {
        return Err(());
    }
    let expression = request.byte_string(0)?.to_vec();
    let (graph, register_node_namespaces, namespaces, mut callbacks) = {
        let xpath = xpath(context, request.header.receiver)?;
        let graph = xpath.document();
        if modern == matches!(graph.family(), DocumentFamily::Legacy) {
            return Err(());
        }
        (
            graph,
            xpath.register_node_namespaces(),
            xpath.namespaces().to_vec(),
            xpath
                .callbacks()
                .iter()
                .map(|callback| {
                    (
                        XPathObject::callback_namespace(callback).to_vec(),
                        XPathObject::callback_name(callback).to_vec(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    };
    callbacks.push((
        b"http://php.net/xpath".to_vec(),
        b"functionString".to_vec(),
    ));
    callbacks.push((
        b"http://php.net/xpath".to_vec(),
        b"function".to_vec(),
    ));
    let context_node = if request.values.len() >= 2 {
        match request.optional_bridge_handle(1)? {
            Some(handle) => {
                if !matches!(
                    handle_kind(handle).map_err(|_| ())?,
                    HANDLE_DOCUMENT | crate::objects::HANDLE_NODE
                ) {
                    return Err(());
                }
                let (pointer, node_graph, _) =
                    receiver_pointer_and_graph(context, handle)?;
                if node_graph.pointer() != graph.pointer() {
                    return Ok(XPathEvaluationPreparation::Complete(
                        DispatchResult::error(b"Node from wrong document"),
                    ));
                }
                Some(pointer)
            }
            None => None,
        }
    } else {
        None
    };
    let register_node_namespaces = match request.values.len() {
        3 => request.boolean(2)?,
        4 if request.boolean(3)? => request.boolean(2)?,
        4 => register_node_namespaces,
        _ => register_node_namespaces,
    };
    Ok(XPathEvaluationPreparation::Ready(PreparedXPathEvaluation {
        graph,
        host: context.host,
        context_node,
        modern,
        register_node_namespaces,
        force_nodeset,
        expression,
        namespaces,
        callbacks,
        method,
    }))
}

/// Reports whether the prepared evaluation needs the re-entrant host path.
pub(super) fn evaluation_has_callbacks(
    prepared: &PreparedXPathEvaluation,
) -> bool {
    !prepared.callbacks.is_empty()
}

/// Runs pinned-libxml XPath evaluation without retaining a context borrow.
pub(super) fn execute_evaluation(
    context_id: u64,
    xpath_handle: u64,
    prepared: PreparedXPathEvaluation,
    enable_callbacks: bool,
) -> Result<ExecutedXPathEvaluation, ()> {
    let callbacks = if enable_callbacks {
        prepared.callbacks.as_slice()
    } else {
        &[]
    };
    let outcome = crate::native::xpath_evaluate(
        prepared.graph.pointer(),
        prepared.context_node,
        prepared.modern,
        prepared.register_node_namespaces,
        prepared.force_nodeset,
        &prepared.expression,
        &prepared.namespaces,
        context_id,
        Some(prepared.host),
        xpath_handle,
        callbacks,
    )?;
    Ok(ExecutedXPathEvaluation { prepared, outcome })
}

/// Materializes callback status, diagnostics, and the family-specific XPath result.
pub(super) fn finish_evaluation(
    context: &mut Context,
    executed: ExecutedXPathEvaluation,
) -> Result<DispatchResult, ()> {
    let ExecutedXPathEvaluation { prepared, outcome } = executed;
    match u32::try_from(outcome.host_status).map_err(|_| ())? {
        0 => {}
        STATUS_THROW => return Ok(DispatchResult::pending_host_throwable()),
        STATUS_ABI_ERROR => return Err(()),
        _ => return Err(()),
    }
    let result = match outcome.status {
        0 => xpath_value_result(
            context,
            Rc::clone(&prepared.graph),
            outcome.value,
        )?,
        2 => DispatchResult::error(b"Node from wrong document"),
        3 if prepared.modern => {
            DispatchResult::error(b"Could not evaluate XPath expression")
        }
        3 => DispatchResult::boolean(false),
        4 => DispatchResult::dom_exception(
            9,
            b"The namespace axis is not well-defined in the living DOM specification. Use Dom\\Element::getInScopeNamespaces() or Dom\\Element::getDescendantNamespaces() instead.",
        ),
        5 => DispatchResult::error(
            b"Legacy XPath namespace-node results are not implemented",
        ),
        6 => return Err(()),
        7 => DispatchResult::error(&outcome.callback_error),
        8 => DispatchResult::type_error(&outcome.callback_error),
        _ => return Err(()),
    };
    Ok(result.with_libxml_warnings(
        prepared.method,
        &outcome.errors,
    ))
}

/// Clones one XPath context while retaining the same authoritative document graph.
pub(super) fn clone_object(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let host = context.host;
    let xpath = xpath(context, request.header.receiver)?;
    let mut retained = Vec::new();
    for descriptor in xpath.callback_descriptors() {
        match crate::host::retain_callable(host, descriptor) {
            Ok(()) => retained.push(descriptor),
            Err(crate::host::HostCallError::PendingThrowable) => {
                release_retained_callbacks(host, &retained);
                return Ok(DispatchResult::pending_host_throwable());
            }
            Err(crate::host::HostCallError::Abi) => {
                release_retained_callbacks(host, &retained);
                return Err(());
            }
        }
    }
    let cloned = xpath.clone_with_retained_callbacks();
    let handle = context
        .native_objects
        .insert(HANDLE_XPATH, NativeObject::XPath(cloned));
    Ok(DispatchResult::bridge_handle(handle))
}

/// Builds one exact path-parameter NUL diagnostic.
fn argument_null_byte_error(
    method: &[u8],
    argument: usize,
    parameter: &[u8],
) -> DispatchResult {
    let mut message = method.to_vec();
    message.extend_from_slice(b"(): Argument #");
    message.extend_from_slice(argument.to_string().as_bytes());
    message.extend_from_slice(b" ($");
    message.extend_from_slice(parameter);
    message.extend_from_slice(b") must not contain any null bytes");
    DispatchResult::value_error(&message)
}

/// Releases descriptors retained during a failed XPath clone attempt.
fn release_retained_callbacks(
    host: crate::context::Host,
    descriptors: &[u64],
) {
    for descriptor in descriptors.iter().rev() {
        let _ = crate::host::release_callable(host, *descriptor);
    }
}

/// Resolves one callback name through the generated runtime host dispatcher.
fn resolve_php_callback(
    host: crate::context::Host,
    name: &[u8],
) -> Result<Option<u64>, ()> {
    match crate::host::resolve_xpath_callable(host, name) {
        Ok(descriptor) => Ok(descriptor),
        Err(crate::host::HostCallError::PendingThrowable) => Err(()),
        Err(crate::host::HostCallError::Abi) => Err(()),
    }
}

/// Reports whether one handler name satisfies php-src's empty/NUL validation.
fn valid_php_callback_name(name: &[u8]) -> bool {
    !name.is_empty() && !name.contains(&0)
}

/// Returns one serialized object's exact concrete PHP class name.
fn nested_object_class<'a>(
    request: &'a Request,
    value: &crate::abi::Value,
) -> Result<Option<&'a [u8]>, ()> {
    if value.tag != VALUE_OBJECT {
        return Ok(None);
    }
    let fields = request.nested_values(value)?;
    if fields.len() != 1 {
        return Err(());
    }
    request.bytes_for_value(&fields[0]).map(Some)
}

/// Builds PHP's object-to-string conversion Error for an indexed callback entry.
fn object_string_conversion_error(class_name: &[u8]) -> DispatchResult {
    let mut message = b"Object of class ".to_vec();
    message.extend_from_slice(class_name);
    message.extend_from_slice(b" could not be converted to string");
    DispatchResult::error(&message)
}

/// Replaces an earlier registration for the same exact alias or appends it.
fn upsert_registration(
    registrations: &mut Vec<(Vec<u8>, u64)>,
    alias: Vec<u8>,
    descriptor: u64,
) {
    if let Some((_, current)) = registrations
        .iter_mut()
        .find(|(current_alias, _)| *current_alias == alias)
    {
        *current = descriptor;
    } else {
        registrations.push((alias, descriptor));
    }
}

/// Builds php-src's direct or array callback-name validation error.
fn register_php_functions_name_error(
    method: &[u8],
    array_entry: bool,
) -> DispatchResult {
    let mut message = method.to_vec();
    if array_entry {
        message.extend_from_slice(
            b"(): Argument #1 ($restrict) must be an array containing valid callback names",
        );
    } else {
        message.extend_from_slice(
            b"(): Argument #1 ($restrict) must be a valid callback name",
        );
    }
    DispatchResult::value_error(&message)
}

/// Builds php-src's invalid callable diagnostic for direct or array registration.
fn register_php_functions_callable_error(
    method: &[u8],
    callable: &[u8],
    array_entry: bool,
) -> DispatchResult {
    let mut message = method.to_vec();
    if array_entry {
        message.extend_from_slice(
            b"(): Argument #1 ($restrict) must be an array with valid callbacks as values, function \"",
        );
    } else {
        message.extend_from_slice(
            b"(): Argument #1 ($restrict) must be a callable, function \"",
        );
    }
    message.extend_from_slice(callable);
    message.extend_from_slice(b"\" not found or invalid function name");
    DispatchResult::type_error(&message)
}

/// Builds the generic invalid array callback-value diagnostic.
fn register_php_functions_array_value_error(
    method: &[u8],
    detail: Option<&[u8]>,
) -> DispatchResult {
    let mut message = method.to_vec();
    message.extend_from_slice(
        b"(): Argument #1 ($restrict) must be an array with valid callbacks as values",
    );
    if let Some(detail) = detail {
        message.extend_from_slice(b", ");
        message.extend_from_slice(detail);
    }
    DispatchResult::type_error(&message)
}

/// Converts one native XPath value into the matching flat bridge result.
fn xpath_value_result(
    context: &mut Context,
    graph: std::rc::Rc<crate::objects::DocumentGraph>,
    value: crate::native::XPathValue,
) -> Result<DispatchResult, ()> {
    Ok(match value {
       crate::native::XPathValue::Nodes(pointers) => {
            let member_documents = pointers
                .iter()
                .map(|pointer| xpath_member_document(context, &graph, *pointer))
                .collect::<Result<Vec<_>, ()>>()?;
           let pointers: Vec<Option<usize>> = pointers.into_iter().map(Some).collect();
            let namespace_allocations: Vec<Option<Rc<NamespaceNodeAllocation>>> = pointers
                .iter()
                .map(|member| match member {
                    Some(pointer) if crate::native::node_type(*pointer) == 18 => {
                        let parent = crate::native::node_parent(*pointer).unwrap_or(0);
                        Some(Rc::new(NamespaceNodeAllocation::new(*pointer, parent)))
                    }
                    _ => None,
                })
                .collect();
            let mut eager_wrappers = Vec::new();
            for ((pointer, member_document), allocation) in pointers
                .iter()
                .zip(&member_documents)
                .zip(&namespace_allocations)
            {
                let Some(pointer) = pointer else {
                    continue;
                };
                let member_graph = member_document
                    .as_ref()
                    .map_or_else(|| Rc::clone(&graph), Rc::clone);
                if let Some(allocation) = allocation {
                    let (parent_handle, parent_kind) = canonical_pointer_handle(
                        context,
                        allocation.parent(),
                        Rc::clone(&member_graph),
                    )?;
                    eager_wrappers.push(Value {
                        tag: VALUE_BRIDGE_HANDLE,
                        flags: 1,
                        payload0: parent_handle,
                        payload1: parent_kind,
                    });
                    let handle = canonical_namespace_node_handle(
                        context,
                        *pointer,
                        member_graph,
                        Some(Rc::clone(allocation)),
                    );
                    eager_wrappers.push(Value {
                        tag: VALUE_BRIDGE_HANDLE,
                        flags: 0,
                        payload0: handle,
                        payload1: 118,
                    });
                } else {
                    let (handle, kind) = canonical_pointer_handle(
                        context,
                        *pointer,
                        member_graph,
                    )?;
                    eager_wrappers.push(Value {
                        tag: VALUE_BRIDGE_HANDLE,
                        flags: 0,
                        payload0: handle,
                        payload1: kind,
                    });
                }
            }
            xpath_collection_result(
                context,
                graph.pointer(),
                graph,
                CollectionKind::Snapshot {
                    pointers,
                    member_documents,
                    namespace_allocations,
                },
                eager_wrappers,
            )
        }
        crate::native::XPathValue::Boolean(value) => {
            DispatchResult::boolean(value)
        }
        crate::native::XPathValue::Number(value) => DispatchResult::float(value),
        crate::native::XPathValue::Bytes(value) => DispatchResult::bytes(value),
        crate::native::XPathValue::Null => DispatchResult::null(),
    })
}

/// Resolves a foreign callback-returned node to the graph retained by its live wrapper.
///
/// Native XPath normally returns members from the context document, but PHP callbacks
/// may return a node from another document. The callback-result lease keeps that wrapper
/// alive until this function snapshots its graph; the resulting node list then retains
/// the graph independently after the lease is released.
fn xpath_member_document(
    context: &Context,
    context_graph: &Rc<DocumentGraph>,
    pointer: usize,
) -> Result<Option<Rc<DocumentGraph>>, ()> {
    if pointer == context_graph.pointer()
        || crate::native::node_document(pointer) == Some(context_graph.pointer())
    {
        return Ok(None);
    }
    let handle = context
        .node_handles
        .get(&pointer)
        .or_else(|| context.document_handles.get(&pointer))
        .copied()
        .ok_or(())?;
    let (wrapper_pointer, graph, _) = receiver_pointer_and_graph(context, handle)?;
    if wrapper_pointer != pointer {
        return Err(());
    }
    let native_document = crate::native::node_document(pointer);
    if pointer != graph.pointer() && native_document != Some(graph.pointer()) {
        return Err(());
    }
    Ok(Some(graph))
}

/// Surrounds one byte string with a selected XPath quote delimiter.
fn surround(input: &[u8], delimiter: u8) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() + 2);
    output.push(delimiter);
    output.extend_from_slice(input);
    output.push(delimiter);
    output
}
