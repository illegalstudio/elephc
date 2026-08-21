//! Purpose:
//! Implements the exported C entry points for DOM context and result lifecycle operations.
//! Contains panics and translates malformed foreign inputs into stable ABI statuses.
//!
//! Called from:
//! - Target-aware extern calls emitted by Elephc-generated programs.
//!
//! Key details:
//! - Every output is initialized before validation and every mutation happens after complete request decoding.
//! - Result pointers are independent per call and remain valid until explicit release.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use crate::abi::{
    DomClassMetadataEntry, HostVTable, ResultHeader, ABI_VERSION, OPCODE_ABI_PING,
    STATUS_ABI_ERROR, STATUS_INTERNAL_PANIC, STATUS_MALFORMED_REQUEST, STATUS_OK,
    STATUS_THROW, VALUE_NULL,
};
use crate::context::{
    context as find_context, ping_value_tag, register_context, register_result, remove_context,
    Context, Host, ResultFrame,
};
use crate::host::{
    ExternalEntityContext, ExternalEntityResult, HostCallError,
    StreamOpenResult, invoke_external_entity_loader, open_stream,
    read_stream_lease, register_stream_lease, release_callable,
    release_result, release_stream_lease, retain_callable, invoke_xpath_callback,
    resolve_xpath_callable, XPathCallbackArgument, XPathCallbackResult,
};

/// Native resource-loader response consumed synchronously by the pinned libxml2 adapter.
#[repr(C)]
pub(crate) struct HostLoaderResult {
    bytes: *mut u8,
    length: usize,
    resource: u64,
    kind: i32,
    reserved: i32,
}

impl HostLoaderResult {
    /// Builds the default-loader sentinel used before validating any foreign input.
    fn default_loader() -> Self {
        Self {
            bytes: std::ptr::null_mut(),
            length: 0,
            resource: 0,
            kind: 3,
            reserved: 0,
        }
    }
}

/// One libxml XPath argument exposed through the panic-contained Rust callback ABI.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct HostXPathArgument {
    kind: i32,
    boolean_value: i32,
    number: f64,
    bytes: *const u8,
    length: usize,
    nodes: *const *mut std::ffi::c_void,
    node_count: usize,
}

/// One copied native XPath argument before node pointers acquire bridge handles.
enum ForeignXPathArgument {
    Null,
    Boolean(bool),
    Number(f64),
    Bytes(Vec<u8>),
    Nodes(Vec<usize>),
}

/// Creates one execution context and writes its opaque ID to `out_context`.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_context_new(
    host_vtable: *const HostVTable,
    out_context: *mut u64,
) -> u32 {
    match catch_unwind(AssertUnwindSafe(|| {
        context_new_impl(host_vtable, out_context)
    })) {
        Ok(status) => status,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Validates foreign context construction inputs and registers one context.
unsafe fn context_new_impl(host_vtable: *const HostVTable, out_context: *mut u64) -> u32 {
    if out_context.is_null() {
        return STATUS_ABI_ERROR;
    }
    out_context.write(0);
    if host_vtable.is_null() {
        return STATUS_ABI_ERROR;
    }
    let host = std::ptr::read_unaligned(host_vtable);
    if host.abi_version != ABI_VERSION
        || usize::try_from(host.struct_size).ok() != Some(std::mem::size_of::<HostVTable>())
    {
        return STATUS_ABI_ERROR;
    }
    if !crate::native::self_check() {
        return STATUS_INTERNAL_PANIC;
    }

    let context_id = register_context(Context::new(Host {
        user_data: host.user_data as usize,
        call: host.call,
    }));
    out_context.write(context_id);
    crate::abi::STATUS_OK
}

/// Resets one context's documents, handles, diagnostics, callbacks, and retained result frames.
#[no_mangle]
pub extern "C" fn elephc_dom_context_reset(context: u64) -> u32 {
    match catch_unwind(AssertUnwindSafe(|| {
        let Some(context) = find_context(context) else {
            return STATUS_ABI_ERROR;
        };
        let Ok(mut context) = context.try_borrow_mut() else {
            return STATUS_ABI_ERROR;
        };
        context.reset();
        crate::abi::STATUS_OK
    })) {
        Ok(status) => status,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Installs compiler-emitted PHP class metadata into one DOM execution context.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_context_set_class_metadata(
    context: u64,
    entries: *const DomClassMetadataEntry,
    count: u64,
) -> u32 {
    match catch_unwind(AssertUnwindSafe(|| {
        let Some(context) = find_context(context) else {
            return STATUS_ABI_ERROR;
        };
        let Ok(mut context) = context.try_borrow_mut() else {
            return STATUS_ABI_ERROR;
        };
        let Some(count) = usize::try_from(count).ok() else {
            return STATUS_ABI_ERROR;
        };
        if count > isize::MAX as usize / std::mem::size_of::<DomClassMetadataEntry>() {
            return STATUS_ABI_ERROR;
        }
        let entries = if count == 0 {
            &[][..]
        } else {
            if entries.is_null() {
                return STATUS_ABI_ERROR;
            }
            unsafe { std::slice::from_raw_parts(entries, count) }
        };
        match context.class_metadata.install(entries) {
            Ok(()) => STATUS_OK,
            Err(()) => STATUS_MALFORMED_REQUEST,
        }
    })) {
        Ok(status) => status,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Frees one context and all result/document/handle state it owns.
#[no_mangle]
pub extern "C" fn elephc_dom_context_free(context: u64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        remove_context(context);
    }));
}

/// Validates and executes one flat request, writing a fixed-size result header.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_call(
    context: u64,
    request_ptr: *const u8,
    request_len: u64,
    out_result: *mut ResultHeader,
) -> u32 {
    if out_result.is_null() {
        return STATUS_ABI_ERROR;
    }
    out_result.write(ResultHeader::abi_error());
    match catch_unwind(AssertUnwindSafe(|| {
        call_impl(context, request_ptr, request_len, out_result)
    })) {
        Ok(status) => status,
        Err(_) => {
            out_result.write(ResultHeader::internal_panic());
            STATUS_INTERNAL_PANIC
        }
    }
}

/// Dispatches one already output-initialized call after complete request validation.
unsafe fn call_impl(
    context_id: u64,
    request_ptr: *const u8,
    request_len: u64,
    out_result: *mut ResultHeader,
) -> u32 {
    let request = match crate::request::decode(request_ptr, request_len) {
        Ok(request) => request,
        Err(error) => return publish_decode_error(context_id, error, out_result),
    };
    let _ = request.values.len();
    let _ = request.bytes.len();
    let Some(context_cell) = find_context(context_id) else {
        return STATUS_ABI_ERROR;
    };

    match request.header.opcode {
        OPCODE_ABI_PING => {
            if request.header.receiver != 0
                || !request.values.is_empty()
                || !request.bytes.is_empty()
            {
                return STATUS_ABI_ERROR;
            }
            let Ok(mut context) = context_cell.try_borrow_mut() else {
                return STATUS_ABI_ERROR;
            };
            let result = register_result(&mut context, ResultFrame::ping(), ping_value_tag());
            out_result.write(result);
            crate::abi::STATUS_OK
        }
        opcode if crate::generated::opcodes::operation_key(opcode).is_some() => {
            let _ = crate::generated::opcodes::MANIFEST_SHA256;
            let operation_key =
                crate::generated::opcodes::operation_key(opcode).expect("guarded opcode");
            let result = match crate::dispatch::dispatch_reentrant(
                context_id,
                operation_key,
                &request,
            ) {
                Ok(Some(result)) => result,
                Ok(None) => {
                    let Ok(mut context) = context_cell.try_borrow_mut() else {
                        return STATUS_ABI_ERROR;
                    };
                    match crate::dispatch::dispatch(&mut context, &request) {
                        Ok(result) => result,
                        Err(()) => return STATUS_ABI_ERROR,
                    }
                }
                Err(()) => return STATUS_ABI_ERROR,
            };
            publish_dispatch_result(&context_cell, result, out_result)
        }
        _ => STATUS_ABI_ERROR,
    }
}

/// Registers one pointer-free result for a rejected ABI request after decoding fails.
unsafe fn publish_decode_error(
    context_id: u64,
    error: crate::request::DecodeError,
    out_result: *mut ResultHeader,
) -> u32 {
    let Some(context) = find_context(context_id) else {
        return STATUS_ABI_ERROR;
    };
    let Ok(mut context) = context.try_borrow_mut() else {
        return STATUS_ABI_ERROR;
    };
    let status = error.status();
    let result = register_result(
        &mut context,
        ResultFrame::abi_status(status),
        VALUE_NULL,
    );
    out_result.write(result);
    status
}

/// Executes deferred host actions and registers one dispatch result after all context borrows end.
unsafe fn publish_dispatch_result(
    context_cell: &std::rc::Rc<std::cell::RefCell<Context>>,
    mut result: crate::dispatch::DispatchResult,
    out_result: *mut ResultHeader,
) -> u32 {
    let pending_host_actions =
        std::mem::take(&mut result.frame.pending_host_actions);
    for action in pending_host_actions {
        match action.execute() {
            Ok(()) => {}
            Err(crate::host::HostCallError::PendingThrowable) => {
                result.frame = ResultFrame::pending_host_throwable();
                result.value_tag = crate::abi::VALUE_NULL;
                break;
            }
            Err(crate::host::HostCallError::Abi) => {
                return STATUS_ABI_ERROR;
            }
        }
    }
    let Ok(mut context) = context_cell.try_borrow_mut() else {
        return STATUS_ABI_ERROR;
    };
    let header = register_result(&mut context, result.frame, result.value_tag);
    out_result.write(header);
    crate::abi::STATUS_OK
}

/// Releases one retained result frame; foreign/double releases are recorded as ABI violations.
#[no_mangle]
pub extern "C" fn elephc_dom_result_release(context: u64, result_id: u64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(context) = find_context(context) else {
            return;
        };
        let Ok(mut context) = context.try_borrow_mut() else {
            return;
        };
        if context.results.remove(&result_id).is_none() {
            context.release_violations = context.release_violations.saturating_add(1);
        }
    }));
}

/// Invokes one context-specific PHP external-entity loader without retaining a context borrow.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_host_external_entity_load(
    context_id: u64,
    public_id: *const u8,
    public_id_length: usize,
    system_id: *const u8,
    system_id_length: usize,
    directory: *const u8,
    directory_length: usize,
    int_sub_name: *const u8,
    int_sub_name_length: usize,
    ext_sub_uri: *const u8,
    ext_sub_uri_length: usize,
    ext_sub_system: *const u8,
    ext_sub_system_length: usize,
    out_result: *mut HostLoaderResult,
) -> u32 {
    if out_result.is_null() {
        return STATUS_ABI_ERROR;
    }
    out_result.write(HostLoaderResult::default_loader());
    match catch_unwind(AssertUnwindSafe(|| {
        host_external_entity_load_impl(
            context_id,
            optional_foreign_bytes(public_id, public_id_length)?,
            optional_foreign_bytes(system_id, system_id_length)?,
            optional_foreign_bytes(directory, directory_length)?,
            optional_foreign_bytes(int_sub_name, int_sub_name_length)?,
            optional_foreign_bytes(ext_sub_uri, ext_sub_uri_length)?,
            optional_foreign_bytes(ext_sub_system, ext_sub_system_length)?,
            out_result,
        )
    })) {
        Ok(Ok(status)) => status,
        Ok(Err(())) => STATUS_ABI_ERROR,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Opens one libxml resource URL through PHP's filesystem and stream-wrapper layer.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_host_resource_open(
    context_id: u64,
    url: *const u8,
    url_length: usize,
    out_result: *mut HostLoaderResult,
) -> u32 {
    if out_result.is_null() {
        return STATUS_ABI_ERROR;
    }
    out_result.write(HostLoaderResult::default_loader());
    match catch_unwind(AssertUnwindSafe(|| {
        let url = optional_foreign_bytes(url, url_length)?
            .ok_or(())?;
        let context_cell = find_context(context_id).ok_or(())?;
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let host = context.host;
        let stream_context = context.stream_context;
        drop(context);
        match open_stream(host, url, b"rb", stream_context, true) {
            Ok(StreamOpenResult::Opened(resource)) => {
                out_result.write(HostLoaderResult {
                    bytes: std::ptr::null_mut(),
                    length: 0,
                    resource: register_stream_lease(resource),
                    kind: 2,
                    reserved: 0,
                });
                Ok(STATUS_OK)
            }
            Ok(StreamOpenResult::Failed(_)) => {
                out_result.write(HostLoaderResult {
                    kind: 0,
                    ..HostLoaderResult::default_loader()
                });
                Ok(STATUS_OK)
            }
            Err(error) => Ok(host_error_status(error)),
        }
    })) {
        Ok(Ok(status)) => status,
        Ok(Err(())) => STATUS_ABI_ERROR,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Resolves and calls the active loader after copying all state out of the context cell.
unsafe fn host_external_entity_load_impl(
    context_id: u64,
    public_id: Option<&[u8]>,
    system_id: Option<&[u8]>,
    directory: Option<&[u8]>,
    int_sub_name: Option<&[u8]>,
    ext_sub_uri: Option<&[u8]>,
    ext_sub_system: Option<&[u8]>,
    out_result: *mut HostLoaderResult,
) -> Result<u32, ()> {
    let context_cell = find_context(context_id).ok_or(())?;
    let context = context_cell.try_borrow().map_err(|_| ())?;
    let host = context.host;
    let disabled = context.entity_loader_disabled;
    let descriptor = context.external_entity_loader;
    drop(context);

    let Some(descriptor) = descriptor else {
        if disabled {
            out_result.write(HostLoaderResult {
                kind: 0,
                ..HostLoaderResult::default_loader()
            });
        }
        return Ok(STATUS_OK);
    };

    if let Err(error) = retain_callable(host, descriptor) {
        return Ok(host_error_status(error));
    }
    let callback_result = invoke_external_entity_loader(
        host,
        descriptor,
        public_id,
        system_id,
        ExternalEntityContext {
            directory,
            int_sub_name,
            ext_sub_uri,
            ext_sub_system,
        },
    );
    let release_result = release_callable(host, descriptor);
    let callback_result = match callback_result {
        Ok(result) => result,
        Err(error) => return Ok(host_error_status(error)),
    };
    if let Err(error) = release_result {
        return Ok(host_error_status(error));
    }

    let result = match callback_result {
        ExternalEntityResult::Null => HostLoaderResult {
            kind: 0,
            ..HostLoaderResult::default_loader()
        },
        ExternalEntityResult::Bytes(bytes) => {
            let length = bytes.len();
            let pointer = if bytes.is_empty() {
                std::ptr::null_mut()
            } else {
                let bytes = bytes.into_boxed_slice();
                Box::into_raw(bytes).cast::<u8>()
            };
            HostLoaderResult {
                bytes: pointer,
                length,
                resource: 0,
                kind: 1,
                reserved: 0,
            }
        }
        ExternalEntityResult::Resource(resource) => {
            let lease_id = register_stream_lease(resource);
            HostLoaderResult {
                bytes: std::ptr::null_mut(),
                length: 0,
                resource: lease_id,
                kind: 2,
                reserved: 0,
            }
        }
    };
    out_result.write(result);
    Ok(STATUS_OK)
}

/// Invokes one registered custom XPath function without retaining a context borrow.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_host_xpath_invoke(
    context_id: u64,
    xpath_handle: u64,
    namespace_uri: *const u8,
    namespace_uri_length: usize,
    name: *const u8,
    name_length: usize,
    arguments: *const HostXPathArgument,
    argument_count: usize,
    out_result: *mut HostLoaderResult,
) -> u32 {
    if out_result.is_null() {
        return STATUS_ABI_ERROR;
    }
    out_result.write(HostLoaderResult::default_loader());
    match catch_unwind(AssertUnwindSafe(|| {
        host_xpath_invoke_impl(
            context_id,
            xpath_handle,
            optional_foreign_bytes(namespace_uri, namespace_uri_length)?
                .ok_or(())?,
            optional_foreign_bytes(name, name_length)?.ok_or(())?,
            foreign_xpath_arguments(arguments, argument_count)?,
            out_result,
        )
    })) {
        Ok(Ok(status)) => status,
        Ok(Err(())) => STATUS_ABI_ERROR,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Releases one callback-result lease when the native adapter cannot retain it.
#[no_mangle]
pub extern "C" fn elephc_dom_host_result_release(
    context_id: u64,
    result_id: u64,
) -> u32 {
    match catch_unwind(AssertUnwindSafe(|| {
        if result_id == 0 {
            return STATUS_ABI_ERROR;
        }
        let Some(context_cell) = find_context(context_id) else {
            return STATUS_ABI_ERROR;
        };
        let host = {
            let Ok(context) = context_cell.try_borrow() else {
                return STATUS_ABI_ERROR;
            };
            context.host
        };
        match release_result(host, result_id) {
            Ok(()) => STATUS_OK,
            Err(error) => host_error_status(error),
        }
    })) {
        Ok(status) => status,
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Resolves, retains, invokes, and balances one XPath callback descriptor.
unsafe fn host_xpath_invoke_impl(
    context_id: u64,
    xpath_handle: u64,
    namespace_uri: &[u8],
    name: &[u8],
    arguments: Vec<ForeignXPathArgument>,
    out_result: *mut HostLoaderResult,
) -> Result<u32, ()> {
    let context_cell = find_context(context_id).ok_or(())?;
    let reserved_php_callback =
        namespace_uri == b"http://php.net/xpath"
            && (name == b"function" || name == b"functionString");
    let (descriptor, host, graph, arguments, dynamic_handler) = {
        let context = context_cell.try_borrow().map_err(|_| ())?;
        let xpath = context
            .native_objects
            .get(xpath_handle, crate::objects::HANDLE_XPATH)
            .map_err(|_| ())?
            .xpath()
            .ok_or(())?;
        let host = context.host;
        let graph = xpath.document();
        if !reserved_php_callback {
            (
                xpath
                .callback_descriptor(namespace_uri, name)
                .ok_or(())?,
                host,
                graph,
                arguments,
                None,
            )
        } else {
            let Some(ForeignXPathArgument::Bytes(handler)) = arguments.first()
            else {
                let (message, kind) = if arguments.is_empty() {
                    (
                        b"Function name must be passed as the first argument".as_slice(),
                        1,
                    )
                } else {
                    (b"Handler name must be a string".as_slice(), 2)
                };
                write_xpath_callback_error(out_result, message, kind);
                return Ok(STATUS_OK);
            };
            let descriptor = match xpath.php_callback_mode() {
                crate::objects::XPathPhpCallbackMode::None => {
                    write_xpath_callback_error(
                        out_result,
                        b"No callbacks were registered",
                        1,
                    );
                    return Ok(STATUS_OK);
                }
                crate::objects::XPathPhpCallbackMode::Set => {
                    let Some(descriptor) =
                        xpath.php_callback_descriptor(handler)
                    else {
                        let mut message = b"No callback handler \"".to_vec();
                        message.extend_from_slice(handler);
                        message.extend_from_slice(b"\" registered");
                        write_xpath_callback_error(
                            out_result,
                            &message,
                            1,
                        );
                        return Ok(STATUS_OK);
                    };
                    descriptor
                }
                crate::objects::XPathPhpCallbackMode::All => 0,
            };
            let dynamic_handler =
                (descriptor == 0).then(|| handler.clone());
            (
                descriptor,
                host,
                graph,
                arguments.into_iter().skip(1).collect(),
                dynamic_handler,
            )
        }
    };
    let descriptor = if descriptor == 0 {
        let handler = dynamic_handler.as_deref().ok_or(())?;
        let Some(descriptor) = resolve_xpath_callable(host, handler)
            .map_err(|_| ())?
        else {
            let mut message = b"Invalid callback ".to_vec();
            message.extend_from_slice(handler);
            message.extend_from_slice(b", function \"");
            message.extend_from_slice(handler);
            message.extend_from_slice(
                b"\" not found or invalid function name",
            );
            write_xpath_callback_error(out_result, &message, 1);
            return Ok(STATUS_OK);
        };
        descriptor
    } else {
        descriptor
    };
    let arguments = {
        let mut context = context_cell.try_borrow_mut().map_err(|_| ())?;
        materialize_xpath_arguments(&mut context, graph, arguments)?
    };
    if let Err(error) = retain_callable(host, descriptor) {
        return Ok(host_error_status(error));
    }
    let callback_result = invoke_xpath_callback(host, descriptor, &arguments);
    let release_result = release_callable(host, descriptor);
    let callback_result = match callback_result {
        Ok(result) => result,
        Err(error) => return Ok(host_error_status(error)),
    };
    if let Err(error) = release_result {
        return Ok(host_error_status(error));
    }

    let result = match callback_result {
        XPathCallbackResult::Boolean(value) => HostLoaderResult {
            bytes: std::ptr::null_mut(),
            length: 0,
            resource: u64::from(value),
            kind: 4,
            reserved: 0,
        },
        XPathCallbackResult::Bytes(bytes) => {
            let length = bytes.len();
            let pointer = if bytes.is_empty() {
                std::ptr::null_mut()
            } else {
                Box::into_raw(bytes.into_boxed_slice()).cast::<u8>()
            };
            HostLoaderResult {
                bytes: pointer,
                length,
                resource: 0,
                kind: 1,
                reserved: 0,
            }
        }
        XPathCallbackResult::Node { handle, lease } => {
            let pointer = {
                let context = context_cell.try_borrow().map_err(|_| ())?;
                crate::dispatch::xpath_callback_pointer(&context, handle)?
            };
            HostLoaderResult {
                bytes: pointer as *mut u8,
                length: usize::try_from(lease.into_id()).map_err(|_| ())?,
                resource: handle,
                kind: 5,
                reserved: 0,
            }
        }
    };
    out_result.write(result);
    Ok(STATUS_OK)
}

/// Publishes one bridge-owned XPath callback Error or TypeError message.
unsafe fn write_xpath_callback_error(
    out_result: *mut HostLoaderResult,
    message: &[u8],
    kind: u64,
) {
    let length = message.len();
    let bytes = message.to_vec().into_boxed_slice();
    out_result.write(HostLoaderResult {
        bytes: Box::into_raw(bytes).cast::<u8>(),
        length,
        resource: kind,
        kind: 6,
        reserved: 0,
    });
}

/// Acquires canonical bridge handles for copied XPath node-set members.
fn materialize_xpath_arguments(
    context: &mut Context,
    graph: Rc<crate::objects::DocumentGraph>,
    arguments: Vec<ForeignXPathArgument>,
) -> Result<Vec<XPathCallbackArgument>, ()> {
    arguments
        .into_iter()
        .map(|argument| match argument {
            ForeignXPathArgument::Null => Ok(XPathCallbackArgument::Null),
            ForeignXPathArgument::Boolean(value) => {
                Ok(XPathCallbackArgument::Boolean(value))
            }
            ForeignXPathArgument::Number(value) => {
                Ok(XPathCallbackArgument::Number(value))
            }
            ForeignXPathArgument::Bytes(bytes) => {
                Ok(XPathCallbackArgument::Bytes(bytes))
            }
            ForeignXPathArgument::Nodes(pointers) => pointers
                .into_iter()
                .map(|pointer| {
                    let (handle, wrapper_kind) =
                        crate::dispatch::xpath_callback_wrapper(
                            context,
                            Rc::clone(&graph),
                            pointer,
                        )?;
                    Ok(crate::host::XPathCallbackNode {
                        handle,
                        wrapper_kind,
                    })
                })
                .collect::<Result<Vec<_>, ()>>()
                .map(XPathCallbackArgument::Nodes),
        })
        .collect()
}

/// Copies and validates one foreign XPath argument vector.
unsafe fn foreign_xpath_arguments(
    arguments: *const HostXPathArgument,
    argument_count: usize,
) -> Result<Vec<ForeignXPathArgument>, ()> {
    if argument_count == 0 {
        if !arguments.is_null() {
            return Ok(Vec::new());
        }
        return Ok(Vec::new());
    }
    if arguments.is_null() {
        return Err(());
    }
    let arguments = std::slice::from_raw_parts(arguments, argument_count);
    arguments
        .iter()
        .map(|argument| match argument.kind {
            0 if argument.boolean_value == 0
                && argument.number.to_bits() == 0
                && argument.bytes.is_null()
                && argument.length == 0
                && argument.nodes.is_null()
                && argument.node_count == 0 =>
            {
                Ok(ForeignXPathArgument::Null)
            }
            1 if matches!(argument.boolean_value, 0 | 1)
                && argument.number.to_bits() == 0
                && argument.bytes.is_null()
                && argument.length == 0
                && argument.nodes.is_null()
                && argument.node_count == 0 =>
            {
                Ok(ForeignXPathArgument::Boolean(
                    argument.boolean_value != 0,
                ))
            }
            2 if argument.boolean_value == 0
                && argument.bytes.is_null()
                && argument.length == 0
                && argument.nodes.is_null()
                && argument.node_count == 0 =>
            {
                Ok(ForeignXPathArgument::Number(argument.number))
            }
            3 if argument.boolean_value == 0
                && argument.number.to_bits() == 0
                && argument.nodes.is_null()
                && argument.node_count == 0 =>
            {
                let bytes = optional_foreign_bytes(
                    argument.bytes,
                    argument.length,
                )?
                .unwrap_or_default()
                .to_vec();
                Ok(ForeignXPathArgument::Bytes(bytes))
            }
            4 if argument.boolean_value == 0
                && argument.number.to_bits() == 0
                && argument.bytes.is_null()
                && argument.length == 0 =>
            {
                if argument.node_count == 0 {
                    if !argument.nodes.is_null() {
                        return Ok(ForeignXPathArgument::Nodes(Vec::new()));
                    }
                    return Ok(ForeignXPathArgument::Nodes(Vec::new()));
                }
                if argument.nodes.is_null() {
                    return Err(());
                }
                let pointers = std::slice::from_raw_parts(
                    argument.nodes,
                    argument.node_count,
                );
                if pointers.iter().any(|pointer| pointer.is_null()) {
                    return Err(());
                }
                Ok(ForeignXPathArgument::Nodes(
                    pointers
                        .iter()
                        .map(|pointer| *pointer as usize)
                        .collect(),
                ))
            }
            _ => Err(()),
        })
        .collect()
}

/// Releases one non-empty Rust byte slice transferred to the synchronous C loader.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_host_loader_bytes_free(
    bytes: *mut u8,
    length: usize,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if bytes.is_null() {
            return;
        }
        let slice = std::ptr::slice_from_raw_parts_mut(bytes, length);
        drop(Box::from_raw(slice));
    }));
}

/// Reads one bounded chunk from a callback-returned stream retained for libxml2.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_host_stream_read(
    lease_id: u64,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> u32 {
    if out_length.is_null() || (buffer.is_null() && capacity != 0) {
        return STATUS_ABI_ERROR;
    }
    out_length.write(0);
    match catch_unwind(AssertUnwindSafe(|| {
        let bytes = read_stream_lease(lease_id, capacity)?;
        if bytes.len() > capacity {
            return Err(HostCallError::Abi);
        }
        if !bytes.is_empty() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
        }
        out_length.write(bytes.len());
        Ok::<(), HostCallError>(())
    })) {
        Ok(Ok(())) => STATUS_OK,
        Ok(Err(error)) => host_error_status(error),
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Closes one callback-returned parser stream and balances its leased PHP result.
#[no_mangle]
pub extern "C" fn elephc_dom_host_stream_close(lease_id: u64) -> u32 {
    match catch_unwind(AssertUnwindSafe(|| {
        release_stream_lease(lease_id)
    })) {
        Ok(Ok(())) => STATUS_OK,
        Ok(Err(error)) => host_error_status(error),
        Err(_) => STATUS_INTERNAL_PANIC,
    }
}

/// Converts one nullable foreign pointer and length into a request-scoped byte slice.
unsafe fn optional_foreign_bytes<'a>(
    pointer: *const u8,
    length: usize,
) -> Result<Option<&'a [u8]>, ()> {
    if pointer.is_null() {
        return if length == 0 { Ok(None) } else { Err(()) };
    }
    Ok(Some(std::slice::from_raw_parts(pointer, length)))
}

/// Maps a contained PHP callback failure onto the stable public status namespace.
fn host_error_status(error: HostCallError) -> u32 {
    match error {
        HostCallError::PendingThrowable => STATUS_THROW,
        HostCallError::Abi => STATUS_ABI_ERROR,
    }
}
