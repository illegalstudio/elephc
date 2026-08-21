//! Purpose:
//! Marshals native-to-PHP host calls through the versioned flat DOM bridge ABI.
//! Owns strict response validation for retained callable descriptors.
//!
//! Called from:
//! - `crate::context::Context` when native state retains or releases PHP values.
//! - External-entity loader callbacks and leased document stream adapters.
//!
//! Key details:
//! - Host messages use the same 48-byte padded header and 24-byte values as public calls.
//! - A successful retain transfers one host reference to the bridge until a matching release.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::abi::{
    HostCall, RequestHeader, ResultHeader, Value, ABI_VERSION,
    HOST_OPCODE_EMIT_WARNING, HOST_OPCODE_FLUSH_STREAM,
    HOST_OPCODE_INVOKE_EXTERNAL_ENTITY_LOADER, HOST_OPCODE_OPEN_STREAM,
    HOST_OPCODE_INVOKE_XPATH_CALLBACK, HOST_OPCODE_READ_STREAM,
    HOST_OPCODE_RELEASE_CALLABLE, HOST_OPCODE_RELEASE_RESULT,
    HOST_OPCODE_RESOLVE_XPATH_CALLABLE, HOST_OPCODE_RETAIN_CALLABLE,
    HOST_OPCODE_WRITE_STREAM,
    PHP_ERROR_KIND_PENDING_HOST_THROWABLE, STATUS_OK, STATUS_THROW, VALUE_BYTES,
    VALUE_ARRAY, VALUE_BOOL, VALUE_BRIDGE_HANDLE, VALUE_CALLABLE, VALUE_FLOAT,
    VALUE_HOST_HANDLE, VALUE_NULL, VALUE_RESOURCE, REQUEST_FLAG_ARGUMENT_COUNT,
};
use crate::context::Host;

const REQUEST_HEADER_SIZE: usize = 48;
const VALUE_SIZE: usize = std::mem::size_of::<Value>();
const ONE_VALUE_REQUEST_SIZE: usize = REQUEST_HEADER_SIZE + VALUE_SIZE;
const EXTERNAL_LOADER_VALUE_COUNT: usize = 7;
const EXTERNAL_LOADER_VALUES_SIZE: usize =
    EXTERNAL_LOADER_VALUE_COUNT * VALUE_SIZE;
const EXTERNAL_LOADER_FIXED_SIZE: usize =
    REQUEST_HEADER_SIZE + EXTERNAL_LOADER_VALUES_SIZE;
const STREAM_READ_VALUE_COUNT: usize = 2;
const STREAM_READ_REQUEST_SIZE: usize =
    REQUEST_HEADER_SIZE + STREAM_READ_VALUE_COUNT * VALUE_SIZE;
const STREAM_OPEN_VALUE_COUNT: usize = 3;
const STREAM_OPEN_FIXED_SIZE: usize =
    REQUEST_HEADER_SIZE + STREAM_OPEN_VALUE_COUNT * VALUE_SIZE;
const STREAM_WRITE_VALUE_COUNT: usize = 2;
const STREAM_WRITE_FIXED_SIZE: usize =
    REQUEST_HEADER_SIZE + STREAM_WRITE_VALUE_COUNT * VALUE_SIZE;
const WARNING_FIXED_SIZE: usize = REQUEST_HEADER_SIZE + VALUE_SIZE;
const USER_WRAPPER_FD_BASE: u64 = 0x40000000;
const USER_WRAPPER_FD_END: u64 = USER_WRAPPER_FD_BASE + 256;
const PHP_STREAM_BUFFER_SIZE: usize = 8192;

/// Nullable parser metadata passed to PHP's external-entity resolver.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExternalEntityContext<'a> {
    pub directory: Option<&'a [u8]>,
    pub int_sub_name: Option<&'a [u8]>,
    pub ext_sub_uri: Option<&'a [u8]>,
    pub ext_sub_system: Option<&'a [u8]>,
}

/// One successful value returned by PHP's external-entity resolver.
#[derive(Debug)]
pub(crate) enum ExternalEntityResult {
    /// The resolver declined to provide a resource.
    Null,
    /// The resolver returned a URI or string-convertible value.
    Bytes(Vec<u8>),
    /// The resolver returned an Elephc stream resource.
    Resource(LeasedHostResource),
}

/// One libxml XPath argument converted for a retained PHP callback descriptor.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum XPathCallbackArgument {
    /// XPath's empty value maps to PHP null.
    Null,
    /// XPath boolean value.
    Boolean(bool),
    /// XPath IEEE-754 number.
    Number(f64),
    /// XPath string bytes.
    Bytes(Vec<u8>),
    /// XPath node-set converted to canonical PHP DOM wrapper handles.
    Nodes(Vec<XPathCallbackNode>),
}

/// One canonical DOM wrapper carried inside a node-set callback argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XPathCallbackNode {
    /// Generation-checked native bridge handle.
    pub(crate) handle: u64,
    /// Stable concrete PHP DOM wrapper discriminator.
    pub(crate) wrapper_kind: u64,
}

/// One PHP callback result normalized to the XPath types php-src preserves.
#[derive(Debug)]
pub(crate) enum XPathCallbackResult {
    /// PHP boolean results remain XPath booleans.
    Boolean(bool),
    /// Every supported non-boolean scalar result becomes an XPath string.
    Bytes(Vec<u8>),
    /// A returned DOM node kept alive by its leased boxed callback result.
    Node {
        handle: u64,
        lease: LeasedHostResult,
    },
}

impl PartialEq for XPathCallbackResult {
    /// Compares pointer-free callback results used by focused host tests.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Boolean(left), Self::Boolean(right)) => left == right,
            (Self::Bytes(left), Self::Bytes(right)) => left == right,
            _ => false,
        }
    }
}

/// One boxed PHP callback result leased until native XPath materialization completes.
#[derive(Debug)]
pub(crate) struct LeasedHostResult {
    host: Host,
    result_id: u64,
}

impl LeasedHostResult {
    /// Adopts one nonzero host result ID returned by the generated runtime.
    fn new(host: Host, result_id: u64) -> Self {
        debug_assert_ne!(result_id, 0);
        Self { host, result_id }
    }

    /// Transfers this lease ID to native evaluation ownership without releasing it.
    pub(crate) fn into_id(mut self) -> u64 {
        std::mem::take(&mut self.result_id)
    }

    /// Reconstitutes one lease copied back from the synchronous native XPath adapter.
    pub(crate) fn from_id(host: Host, result_id: u64) -> Result<Self, HostCallError> {
        if result_id == 0 {
            return Err(HostCallError::Abi);
        }
        Ok(Self::new(host, result_id))
    }
}

impl Drop for LeasedHostResult {
    /// Releases the boxed callback result unless ownership was transferred onward.
    fn drop(&mut self) {
        if self.result_id != 0 {
            let _ = release_result(self.host, self.result_id);
        }
    }
}

/// One callback-returned stream kept alive by its leased boxed PHP result.
#[derive(Debug)]
pub(crate) struct LeasedHostResource {
    pub resource: u64,
    pub resource_kind: u64,
    pub class_name: Vec<u8>,
    host: Host,
    result_id: u64,
    buffered: Vec<u8>,
}

impl LeasedHostResource {
    /// Releases this lease explicitly so a contained PHP close failure can be propagated.
    pub(crate) fn release(mut self) -> Result<(), HostCallError> {
        let result_id = std::mem::take(&mut self.result_id);
        release_result(self.host, result_id)
    }
}

impl Drop for LeasedHostResource {
    /// Releases the boxed callback result that owns this stream resource.
    fn drop(&mut self) {
        if self.result_id != 0 {
            let _ = release_result(self.host, self.result_id);
        }
    }
}

thread_local! {
    /// Callback-returned streams retained until libxml2 closes its parser input.
    static STREAM_LEASES: RefCell<HashMap<u64, LeasedHostResource>> =
        RefCell::new(HashMap::new());
}

/// Failure returned by one contained native-to-PHP callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostCallError {
    /// The host rejected or malformed the callback ABI exchange.
    Abi,
    /// PHP raised a Throwable that remains stored in the runtime's active exception cell.
    PendingThrowable,
}

/// PHP-visible outcome of one contained stream write callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamWriteResult {
    /// The wrapper returned `false` or another negative integer.
    Failed,
    /// The wrapper accepted no more than the requested number of bytes.
    Written(usize),
    /// The wrapper reported more bytes than were present in the request.
    Oversized(usize),
}

/// PHP-visible failure detail returned when one stream path cannot be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamOpenFailure {
    /// The stream layer failed without a registered-wrapper diagnostic.
    Silent,
    /// The registered wrapper does not implement `url_stat()`.
    MissingUrlStat(Vec<u8>),
    /// The registered wrapper does not implement `stream_open()`.
    MissingStreamOpen(Vec<u8>),
    /// The registered wrapper's `stream_open()` callback returned false.
    StreamOpenFailed(Vec<u8>),
}

/// Result of one contained PHP stream-open host exchange.
#[derive(Debug)]
pub(crate) enum StreamOpenResult {
    /// The host returned one live leased stream.
    Opened(LeasedHostResource),
    /// The host could not open the path and returned optional diagnostic detail.
    Failed(StreamOpenFailure),
}

/// Retains one callable descriptor through the execution context's host vtable.
pub(crate) fn retain_callable(
    host: Host,
    descriptor: u64,
) -> Result<(), HostCallError> {
    call_value(host, HOST_OPCODE_RETAIN_CALLABLE, VALUE_CALLABLE, descriptor)
}

/// Releases one callable descriptor previously retained by `retain_callable`.
pub(crate) fn release_callable(
    host: Host,
    descriptor: u64,
) -> Result<(), HostCallError> {
    call_value(host, HOST_OPCODE_RELEASE_CALLABLE, VALUE_CALLABLE, descriptor)
}

/// Resolves one PHP callable name without retaining the returned runtime descriptor.
pub(crate) fn resolve_xpath_callable(
    host: Host,
    name: &[u8],
) -> Result<Option<u64>, HostCallError> {
    if name.is_empty() {
        return Err(HostCallError::Abi);
    }
    let request_size = ONE_VALUE_REQUEST_SIZE
        .checked_add(name.len())
        .ok_or(HostCallError::Abi)?;
    let mut request = vec![0_u8; request_size];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_RESOLVE_XPATH_CALLABLE,
        flags: REQUEST_FLAG_ARGUMENT_COUNT | 1,
        receiver: 0,
        value_count: 1,
        byte_count: name.len() as u64,
    };
    let value = request_bytes_value(name, &mut request, ONE_VALUE_REQUEST_SIZE, 0)?;
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::write_unaligned(
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE).cast::<Value>(),
            value,
        );
    }
    let result = call_request(host, &request)?;
    if result.result_id != 0
        || !result.bytes_ptr.is_null()
        || result.bytes_len != 0
        || !result.values_ptr.is_null()
        || result.values_len != 0
        || !result.diagnostics_ptr.is_null()
        || result.diagnostics_len != 0
        || result.php_error_kind != 0
        || result.dom_exception_code != 0
        || result.payload1 != 0
    {
        return Err(HostCallError::Abi);
    }
    match result.value_tag {
        VALUE_NULL if result.payload0 == 0 => Ok(None),
        VALUE_CALLABLE if result.payload0 != 0 => Ok(Some(result.payload0)),
        _ => Err(HostCallError::Abi),
    }
}

/// Invokes PHP's current external-entity resolver through the generic host ABI.
pub(crate) fn invoke_external_entity_loader(
    host: Host,
    descriptor: u64,
    public_id: Option<&[u8]>,
    system_id: Option<&[u8]>,
    context: ExternalEntityContext<'_>,
) -> Result<ExternalEntityResult, HostCallError> {
    let byte_count = [
        public_id,
        system_id,
        context.directory,
        context.int_sub_name,
        context.ext_sub_uri,
        context.ext_sub_system,
    ]
    .into_iter()
    .flatten()
    .try_fold(0_usize, |total, bytes| total.checked_add(bytes.len()))
    .ok_or(HostCallError::Abi)?;
    let request_size = EXTERNAL_LOADER_FIXED_SIZE
        .checked_add(byte_count)
        .ok_or(HostCallError::Abi)?;
    let mut request = vec![0_u8; request_size];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_INVOKE_EXTERNAL_ENTITY_LOADER,
        flags: 0,
        receiver: 0,
        value_count: EXTERNAL_LOADER_VALUE_COUNT as u64,
        byte_count: byte_count as u64,
    };
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
    }
    let values = [
        callable_value(descriptor),
        optional_bytes_value(public_id, &mut request, 0)?,
        optional_bytes_value(system_id, &mut request, public_id.map_or(0, <[u8]>::len))?,
        optional_bytes_value(
            context.directory,
            &mut request,
            byte_offset(&[public_id, system_id])?,
        )?,
        optional_bytes_value(
            context.int_sub_name,
            &mut request,
            byte_offset(&[public_id, system_id, context.directory])?,
        )?,
        optional_bytes_value(
            context.ext_sub_uri,
            &mut request,
            byte_offset(&[
                public_id,
                system_id,
                context.directory,
                context.int_sub_name,
            ])?,
        )?,
        optional_bytes_value(
            context.ext_sub_system,
            &mut request,
            byte_offset(&[
                public_id,
                system_id,
                context.directory,
                context.int_sub_name,
                context.ext_sub_uri,
            ])?,
        )?,
    ];
    unsafe {
        std::ptr::copy_nonoverlapping(
            values.as_ptr().cast::<u8>(),
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE),
            EXTERNAL_LOADER_VALUES_SIZE,
        );
    }
    let result = call_request(host, &request)?;
    match result.value_tag {
        VALUE_NULL if result.result_id == 0 => Ok(ExternalEntityResult::Null),
        VALUE_BYTES if result.result_id != 0
            && result.payload0 == 0
            && usize::try_from(result.payload1).ok()
                == usize::try_from(result.bytes_len).ok()
            && result.values_ptr.is_null()
            && result.values_len == 0 =>
        {
            if result.bytes_ptr.is_null() != (result.bytes_len == 0) {
                return Err(HostCallError::Abi);
            }
            let bytes = if result.bytes_len == 0 {
                Vec::new()
            } else {
                let length =
                    usize::try_from(result.bytes_len).map_err(|_| HostCallError::Abi)?;
                unsafe {
                    std::slice::from_raw_parts(result.bytes_ptr, length)
                }
                .to_vec()
            };
            release_result(host, result.result_id)?;
            Ok(ExternalEntityResult::Bytes(bytes))
        }
        VALUE_RESOURCE if result.result_id != 0
            && result.bytes_ptr.is_null()
            && result.bytes_len == 0
            && result.values_ptr.is_null()
            && result.values_len == 0 =>
        {
            Ok(ExternalEntityResult::Resource(LeasedHostResource {
                resource: result.payload0,
                resource_kind: result.payload1,
                class_name: Vec::new(),
                host,
                result_id: result.result_id,
                buffered: Vec::new(),
            }))
        }
        _ => Err(HostCallError::Abi),
    }
}

/// Invokes one retained XPath callback through the generic host ABI.
pub(crate) fn invoke_xpath_callback(
    host: Host,
    descriptor: u64,
    arguments: &[XPathCallbackArgument],
) -> Result<XPathCallbackResult, HostCallError> {
    let root_count = arguments
        .len()
        .checked_add(1)
        .ok_or(HostCallError::Abi)?;
    let descendant_count = arguments.iter().try_fold(0_usize, |total, argument| {
        let count = match argument {
            XPathCallbackArgument::Nodes(nodes) => nodes.len(),
            _ => 0,
        };
        total.checked_add(count).ok_or(HostCallError::Abi)
    })?;
    let value_count = root_count
        .checked_add(descendant_count)
        .ok_or(HostCallError::Abi)?;
    let byte_count = arguments.iter().try_fold(0_usize, |total, argument| {
        let length = match argument {
            XPathCallbackArgument::Bytes(bytes) => bytes.len(),
            XPathCallbackArgument::Nodes(_) => 0,
            XPathCallbackArgument::Null
            | XPathCallbackArgument::Boolean(_)
            | XPathCallbackArgument::Number(_) => 0,
        };
        total.checked_add(length).ok_or(HostCallError::Abi)
    })?;
    let fixed_size = REQUEST_HEADER_SIZE
        .checked_add(
            value_count
                .checked_mul(VALUE_SIZE)
                .ok_or(HostCallError::Abi)?,
        )
        .ok_or(HostCallError::Abi)?;
    let request_size = fixed_size
        .checked_add(byte_count)
        .ok_or(HostCallError::Abi)?;
    let mut request = vec![0_u8; request_size];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_INVOKE_XPATH_CALLBACK,
        flags: REQUEST_FLAG_ARGUMENT_COUNT
            | u32::try_from(root_count).map_err(|_| HostCallError::Abi)?,
        receiver: 0,
        value_count: value_count as u64,
        byte_count: byte_count as u64,
    };
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
    }
    let mut values = Vec::with_capacity(value_count);
    let mut descendants = Vec::with_capacity(descendant_count);
    values.push(callable_value(descriptor));
    let mut byte_offset = 0_usize;
    for argument in arguments {
        let value = match argument {
            XPathCallbackArgument::Null => Value {
                tag: VALUE_NULL,
                flags: 0,
                payload0: 0,
                payload1: 0,
            },
            XPathCallbackArgument::Boolean(value) => Value {
                tag: VALUE_BOOL,
                flags: 0,
                payload0: u64::from(*value),
                payload1: 0,
            },
            XPathCallbackArgument::Number(value) => Value {
                tag: VALUE_FLOAT,
                flags: 0,
                payload0: value.to_bits(),
                payload1: 0,
            },
            XPathCallbackArgument::Bytes(bytes) => {
                let value = request_bytes_value(
                    bytes,
                    &mut request,
                    fixed_size,
                    byte_offset,
                )?;
                byte_offset = byte_offset
                    .checked_add(bytes.len())
                    .ok_or(HostCallError::Abi)?;
                value
            }
            XPathCallbackArgument::Nodes(nodes) => {
                let start = root_count
                    .checked_add(descendants.len())
                    .ok_or(HostCallError::Abi)?;
                for node in nodes {
                    if node.handle == 0 || node.wrapper_kind == 0 {
                        return Err(HostCallError::Abi);
                    }
                    descendants.push(Value {
                        tag: VALUE_BRIDGE_HANDLE,
                        flags: 0,
                        payload0: node.handle,
                        payload1: node.wrapper_kind,
                    });
                }
                Value {
                    tag: VALUE_ARRAY,
                    flags: 0,
                    payload0: u64::try_from(start).map_err(|_| HostCallError::Abi)?,
                    payload1: u64::try_from(nodes.len()).map_err(|_| HostCallError::Abi)?,
                }
            }
        };
        values.push(value);
    }
    values.extend(descendants);
    unsafe {
        std::ptr::copy_nonoverlapping(
            values.as_ptr().cast::<u8>(),
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE),
            value_count * VALUE_SIZE,
        );
    }
    let result = call_request(host, &request)?;
    match result.value_tag {
        VALUE_BOOL
            if result.result_id == 0
                && result.payload0 <= 1
                && result.payload1 == 0
                && result.bytes_ptr.is_null()
                && result.bytes_len == 0
                && result.values_ptr.is_null()
                && result.values_len == 0 =>
        {
            Ok(XPathCallbackResult::Boolean(result.payload0 != 0))
        }
        VALUE_BYTES
            if result.result_id != 0
                && result.payload0 == 0
                && result.payload1 == result.bytes_len
                && result.values_ptr.is_null()
                && result.values_len == 0
                && !(result.bytes_ptr.is_null() && result.bytes_len != 0) =>
        {
            let length = usize::try_from(result.bytes_len)
                .map_err(|_| HostCallError::Abi)?;
            let bytes = if length == 0 {
                Vec::new()
            } else {
                unsafe {
                    std::slice::from_raw_parts(result.bytes_ptr, length)
                }
                .to_vec()
            };
            release_result(host, result.result_id)?;
            Ok(XPathCallbackResult::Bytes(bytes))
        }
        VALUE_BRIDGE_HANDLE
            if result.result_id != 0
                && result.payload0 != 0
                && result.bytes_ptr.is_null()
                && result.bytes_len == 0
                && result.values_ptr.is_null()
                && result.values_len == 0 =>
        {
            Ok(XPathCallbackResult::Node {
                handle: result.payload0,
                lease: LeasedHostResult::new(host, result.result_id),
            })
        }
        _ => Err(HostCallError::Abi),
    }
}

/// Opens one PHP stream path through the host runtime and leases the resulting resource.
pub(crate) fn open_stream(
    host: Host,
    path: &[u8],
    mode: &[u8],
    stream_context: Option<u64>,
    stat_before_open: bool,
) -> Result<StreamOpenResult, HostCallError> {
    let byte_count = path
        .len()
        .checked_add(mode.len())
        .ok_or(HostCallError::Abi)?;
    let request_size = STREAM_OPEN_FIXED_SIZE
        .checked_add(byte_count)
        .ok_or(HostCallError::Abi)?;
    let mut request = vec![0_u8; request_size];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_OPEN_STREAM,
        flags: u32::from(stat_before_open),
        receiver: 0,
        value_count: STREAM_OPEN_VALUE_COUNT as u64,
        byte_count: byte_count as u64,
    };
    let values = [
        request_bytes_value(path, &mut request, STREAM_OPEN_FIXED_SIZE, 0)?,
        request_bytes_value(
            mode,
            &mut request,
            STREAM_OPEN_FIXED_SIZE,
            path.len(),
        )?,
        stream_context.map_or(
            Value {
                tag: VALUE_NULL,
                flags: 0,
                payload0: 0,
                payload1: 0,
            },
            |resource| Value {
                tag: VALUE_RESOURCE,
                flags: 0,
                payload0: resource,
                payload1: 0,
            },
        ),
    ];
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::copy_nonoverlapping(
            values.as_ptr().cast::<u8>(),
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE),
            STREAM_OPEN_VALUE_COUNT * VALUE_SIZE,
        );
    }
    let result = call_request(host, &request)?;
    match result.value_tag {
        VALUE_NULL
            if result.result_id == 0
                && result.payload1 == 0
                && result.values_ptr.is_null()
                && result.values_len == 0 =>
        {
            if result.bytes_ptr.is_null() != (result.bytes_len == 0) {
                return Err(HostCallError::Abi);
            }
            let class_name = if result.bytes_len == 0 {
                Vec::new()
            } else {
                let length =
                    usize::try_from(result.bytes_len).map_err(|_| HostCallError::Abi)?;
                unsafe { std::slice::from_raw_parts(result.bytes_ptr, length) }.to_vec()
            };
            let failure = match result.payload0 {
                0 | 4 if class_name.is_empty() => StreamOpenFailure::Silent,
                1 if !class_name.is_empty() => {
                    StreamOpenFailure::MissingUrlStat(class_name)
                }
                2 if !class_name.is_empty() => {
                    StreamOpenFailure::MissingStreamOpen(class_name)
                }
                3 if !class_name.is_empty() => {
                    StreamOpenFailure::StreamOpenFailed(class_name)
                }
                _ => return Err(HostCallError::Abi),
            };
            Ok(StreamOpenResult::Failed(failure))
        }
        VALUE_RESOURCE
            if result.result_id != 0
                && result.values_ptr.is_null()
                && result.values_len == 0
                && matches!(result.payload1, 0 | 1 | 3) =>
        {
            if result.bytes_ptr.is_null() && result.bytes_len != 0 {
                return Err(HostCallError::Abi);
            }
            let class_name = if result.bytes_len == 0 {
                Vec::new()
            } else {
                let length =
                    usize::try_from(result.bytes_len).map_err(|_| HostCallError::Abi)?;
                unsafe { std::slice::from_raw_parts(result.bytes_ptr, length) }.to_vec()
            };
            Ok(StreamOpenResult::Opened(LeasedHostResource {
                resource: result.payload0,
                resource_kind: result.payload1,
                class_name,
                host,
                result_id: result.result_id,
                buffered: Vec::new(),
            }))
        }
        _ => Err(HostCallError::Abi),
    }
}

/// Writes one byte slice through a leased PHP stream, preserving false versus zero.
pub(crate) fn write_stream_chunk(
    stream: &LeasedHostResource,
    bytes: &[u8],
) -> Result<StreamWriteResult, HostCallError> {
    if bytes.is_empty() {
        return Ok(StreamWriteResult::Written(0));
    }
    if !matches!(stream.resource_kind, 0 | 1 | 3) {
        return Err(HostCallError::Abi);
    }
    let request_size = STREAM_WRITE_FIXED_SIZE
        .checked_add(bytes.len())
        .ok_or(HostCallError::Abi)?;
    let mut request = vec![0_u8; request_size];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_WRITE_STREAM,
        flags: 0,
        receiver: 0,
        value_count: STREAM_WRITE_VALUE_COUNT as u64,
        byte_count: bytes.len() as u64,
    };
    let values = [
        Value {
            tag: VALUE_RESOURCE,
            flags: 0,
            payload0: stream.resource,
            payload1: stream.resource_kind,
        },
        request_bytes_value(bytes, &mut request, STREAM_WRITE_FIXED_SIZE, 0)?,
    ];
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::copy_nonoverlapping(
            values.as_ptr().cast::<u8>(),
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE),
            STREAM_WRITE_VALUE_COUNT * VALUE_SIZE,
        );
    }
    let result = call_request(stream.host, &request)?;
    if result.value_tag != crate::abi::VALUE_INT
        || result.result_id != 0
        || result.payload1 != 0
        || !result.bytes_ptr.is_null()
        || result.bytes_len != 0
        || !result.values_ptr.is_null()
        || result.values_len != 0
    {
        return Err(HostCallError::Abi);
    }
    let written = result.payload0 as i64;
    if written < 0 {
        return Ok(StreamWriteResult::Failed);
    }
    let written = usize::try_from(written).map_err(|_| HostCallError::Abi)?;
    if written > bytes.len() {
        return Ok(StreamWriteResult::Oversized(written));
    }
    Ok(StreamWriteResult::Written(written))
}

/// Flushes one leased PHP stream while ignoring its PHP-visible boolean result.
pub(crate) fn flush_stream(
    stream: &LeasedHostResource,
) -> Result<(), HostCallError> {
    if !matches!(stream.resource_kind, 0 | 1 | 3) {
        return Err(HostCallError::Abi);
    }
    let mut request = [0_u8; ONE_VALUE_REQUEST_SIZE];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_FLUSH_STREAM,
        flags: 0,
        receiver: 0,
        value_count: 1,
        byte_count: 0,
    };
    let value = Value {
        tag: VALUE_RESOURCE,
        flags: 0,
        payload0: stream.resource,
        payload1: stream.resource_kind,
    };
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::write_unaligned(
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE).cast::<Value>(),
            value,
        );
    }
    let result = call_request(stream.host, &request)?;
    validate_pointer_free_null_result(&result)
}

/// Emits one preformatted warning through PHP's suppressible diagnostic channel.
pub(crate) fn emit_warning(
    host: Host,
    warning: &[u8],
) -> Result<(), HostCallError> {
    if warning.is_empty() {
        return Err(HostCallError::Abi);
    }
    let request_size = WARNING_FIXED_SIZE
        .checked_add(warning.len())
        .ok_or(HostCallError::Abi)?;
    let mut request = vec![0_u8; request_size];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_EMIT_WARNING,
        flags: 0,
        receiver: 0,
        value_count: 1,
        byte_count: warning.len() as u64,
    };
    let value = request_bytes_value(warning, &mut request, WARNING_FIXED_SIZE, 0)?;
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::write_unaligned(
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE).cast::<Value>(),
            value,
        );
    }
    let result = call_request(host, &request)?;
    validate_pointer_free_null_result(&result)
}

/// Registers one callback-returned stream until the native parser closes its input.
pub(crate) fn register_stream_lease(resource: LeasedHostResource) -> u64 {
    let lease_id = crate::context::next_id();
    STREAM_LEASES.with(|leases| {
        leases.borrow_mut().insert(lease_id, resource);
    });
    lease_id
}

/// Reads at most `maximum` bytes from one live callback-returned stream.
pub(crate) fn read_stream_lease(
    lease_id: u64,
    maximum: usize,
) -> Result<Vec<u8>, HostCallError> {
    if maximum == 0 {
        return Ok(Vec::new());
    }
    let buffered = STREAM_LEASES.with(|leases| {
        let mut leases = leases.borrow_mut();
        let lease = leases.get_mut(&lease_id).ok_or(HostCallError::Abi)?;
        if lease.buffered.is_empty() {
            Ok(None)
        } else {
            let length = maximum.min(lease.buffered.len());
            Ok(Some(lease.buffered.drain(..length).collect::<Vec<_>>()))
        }
    })?;
    if let Some(buffered) = buffered {
        return Ok(buffered);
    }
    let (host, resource, resource_kind) = STREAM_LEASES.with(|leases| {
        let leases = leases.borrow();
        let lease = leases.get(&lease_id).ok_or(HostCallError::Abi)?;
        Ok((lease.host, lease.resource, lease.resource_kind))
    })?;
    if !matches!(resource_kind, 0 | 1 | 3) {
        return Err(HostCallError::Abi);
    }
    let request_size = if resource_kind == 1
        && (USER_WRAPPER_FD_BASE..USER_WRAPPER_FD_END).contains(&resource)
    {
        PHP_STREAM_BUFFER_SIZE
    } else {
        maximum
    };
    let bytes = read_stream_chunk(
        host,
        resource,
        resource_kind,
        request_size,
    )?;
    if bytes.len() <= maximum {
        return Ok(bytes);
    }
    STREAM_LEASES.with(|leases| {
        let mut leases = leases.borrow_mut();
        let lease = leases.get_mut(&lease_id).ok_or(HostCallError::Abi)?;
        lease.buffered.extend_from_slice(&bytes);
        Ok(lease.buffered.drain(..maximum).collect())
    })
}

/// Removes and releases one parser-owned callback stream lease.
pub(crate) fn release_stream_lease(
    lease_id: u64,
) -> Result<(), HostCallError> {
    let lease = STREAM_LEASES
        .with(|leases| leases.borrow_mut().remove(&lease_id))
        .ok_or(HostCallError::Abi)?;
    lease.release()
}

/// Reads one bounded stream chunk through the generic host callback ABI.
fn read_stream_chunk(
    host: Host,
    resource: u64,
    resource_kind: u64,
    maximum: usize,
) -> Result<Vec<u8>, HostCallError> {
    if maximum == 0 {
        return Ok(Vec::new());
    }
    let mut request = [0_u8; STREAM_READ_REQUEST_SIZE];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode: HOST_OPCODE_READ_STREAM,
        flags: 0,
        receiver: 0,
        value_count: STREAM_READ_VALUE_COUNT as u64,
        byte_count: 0,
    };
    let values = [
        Value {
            tag: VALUE_RESOURCE,
            flags: 0,
            payload0: resource,
            payload1: resource_kind,
        },
        Value {
            tag: crate::abi::VALUE_INT,
            flags: 0,
            payload0: maximum as u64,
            payload1: 0,
        },
    ];
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::copy_nonoverlapping(
            values.as_ptr().cast::<u8>(),
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE),
            STREAM_READ_VALUE_COUNT * VALUE_SIZE,
        );
    }
    let result = call_request(host, &request)?;
    let length = usize::try_from(result.bytes_len).map_err(|_| HostCallError::Abi)?;
    if result.value_tag != VALUE_BYTES
        || result.payload0 != 0
        || usize::try_from(result.payload1).ok() != Some(length)
        || length > maximum
        || !result.values_ptr.is_null()
        || result.values_len != 0
        || (length == 0 && result.result_id != 0)
        || (length != 0 && (result.result_id == 0 || result.bytes_ptr.is_null()))
    {
        return Err(HostCallError::Abi);
    }
    let bytes = if length == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(result.bytes_ptr, length) }.to_vec()
    };
    if result.result_id != 0 {
        release_result(host, result.result_id)?;
    }
    Ok(bytes)
}

/// Returns the total byte-section offset of the supplied optional strings.
fn byte_offset(parts: &[Option<&[u8]>]) -> Result<usize, HostCallError> {
    parts
        .iter()
        .flatten()
        .try_fold(0_usize, |total, bytes| total.checked_add(bytes.len()))
        .ok_or(HostCallError::Abi)
}

/// Builds one callable descriptor value for a host request.
fn callable_value(descriptor: u64) -> Value {
    Value {
        tag: VALUE_CALLABLE,
        flags: 0,
        payload0: descriptor,
        payload1: 0,
    }
}

/// Builds and copies one nullable byte-string host-request value.
fn optional_bytes_value(
    bytes: Option<&[u8]>,
    request: &mut [u8],
    offset: usize,
) -> Result<Value, HostCallError> {
    let Some(bytes) = bytes else {
        return Ok(Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        });
    };
    let end = offset.checked_add(bytes.len()).ok_or(HostCallError::Abi)?;
    let destination = request
        .get_mut(EXTERNAL_LOADER_FIXED_SIZE + offset..EXTERNAL_LOADER_FIXED_SIZE + end)
        .ok_or(HostCallError::Abi)?;
    destination.copy_from_slice(bytes);
    Ok(Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: bytes.len() as u64,
    })
}

/// Copies one required byte string into a host request and describes its range.
fn request_bytes_value(
    bytes: &[u8],
    request: &mut [u8],
    fixed_size: usize,
    offset: usize,
) -> Result<Value, HostCallError> {
    let end = offset.checked_add(bytes.len()).ok_or(HostCallError::Abi)?;
    let destination = request
        .get_mut(fixed_size + offset..fixed_size + end)
        .ok_or(HostCallError::Abi)?;
    destination.copy_from_slice(bytes);
    Ok(Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset as u64,
        payload1: bytes.len() as u64,
    })
}

/// Releases one leased PHP callback result through its opaque host handle.
pub(crate) fn release_result(
    host: Host,
    result_id: u64,
) -> Result<(), HostCallError> {
    call_value(
        host,
        HOST_OPCODE_RELEASE_RESULT,
        VALUE_HOST_HANDLE,
        result_id,
    )
}

/// Sends one scalar host-value ownership request and validates its pointer-free null response.
fn call_value(
    host: Host,
    opcode: u32,
    tag: u32,
    payload: u64,
) -> Result<(), HostCallError> {
    let call: HostCall = host.call.ok_or(HostCallError::Abi)?;
    let mut request = [0_u8; ONE_VALUE_REQUEST_SIZE];
    let header = RequestHeader {
        abi_version: ABI_VERSION,
        header_size: REQUEST_HEADER_SIZE as u32,
        opcode,
        flags: 0,
        receiver: 0,
        value_count: 1,
        byte_count: 0,
    };
    let value = Value {
        tag,
        flags: 0,
        payload0: payload,
        payload1: 0,
    };
    unsafe {
        std::ptr::write_unaligned(request.as_mut_ptr().cast::<RequestHeader>(), header);
        std::ptr::write_unaligned(
            request.as_mut_ptr().add(REQUEST_HEADER_SIZE).cast::<Value>(),
            value,
        );
    }
    let result = call_request_with(host, call, &request)?;
    validate_pointer_free_null_result(&result)
}

/// Sends one complete host request and validates its common status envelope.
fn call_request(host: Host, request: &[u8]) -> Result<ResultHeader, HostCallError> {
    let call: HostCall = host.call.ok_or(HostCallError::Abi)?;
    call_request_with(host, call, request)
}

/// Calls one already-resolved host entry point and validates its status envelope.
fn call_request_with(
    host: Host,
    call: HostCall,
    request: &[u8],
) -> Result<ResultHeader, HostCallError> {
    let mut result = ResultHeader::abi_error();
    let call_status = unsafe {
        call(
            host.user_data as *mut std::ffi::c_void,
            request.as_ptr(),
            request.len() as u64,
            &mut result,
        )
    };
    if call_status != STATUS_OK
        || result.abi_version != ABI_VERSION
        || usize::try_from(result.struct_size).ok() != Some(std::mem::size_of::<ResultHeader>())
        || !result.diagnostics_ptr.is_null()
        || result.diagnostics_len != 0
    {
        return Err(HostCallError::Abi);
    }
    if result.status == STATUS_THROW
        && result.php_error_kind == PHP_ERROR_KIND_PENDING_HOST_THROWABLE
        && result.dom_exception_code == 0
    {
        return Err(HostCallError::PendingThrowable);
    }
    if result.status != STATUS_OK
        || result.php_error_kind != 0
        || result.dom_exception_code != 0
    {
        return Err(HostCallError::Abi);
    }
    Ok(result)
}

/// Validates the pointer-free null response used by ownership host operations.
fn validate_pointer_free_null_result(
    result: &ResultHeader,
) -> Result<(), HostCallError> {
    if result.value_tag != VALUE_NULL
        || result.result_id != 0
        || result.payload0 != 0
        || result.payload1 != 0
        || !result.bytes_ptr.is_null()
        || result.bytes_len != 0
        || !result.values_ptr.is_null()
        || result.values_len != 0
    {
        return Err(HostCallError::Abi);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::sync::Mutex;

    use super::*;

    static SEEN_OPCODE: AtomicU32 = AtomicU32::new(0);
    static SEEN_PAYLOAD: AtomicU64 = AtomicU64::new(0);
    static RELEASED_RESULT: AtomicU64 = AtomicU64::new(0);
    static RELEASED_XPATH_NODE_RESULT: AtomicU64 = AtomicU64::new(0);
    static RELEASED_STREAM_RESULT: AtomicU64 = AtomicU64::new(0);
    static RELEASED_STREAM_CHUNK: AtomicU64 = AtomicU64::new(0);
    static BUFFERED_STREAM_READS: AtomicU64 = AtomicU64::new(0);
    static RELEASED_BUFFERED_STREAM: AtomicU64 = AtomicU64::new(0);
    static STREAM_WRITE_MODE: AtomicU32 = AtomicU32::new(0);
    static STREAM_WRITE_CALLS: AtomicU64 = AtomicU64::new(0);
    static STREAM_FLUSH_CALLS: AtomicU64 = AtomicU64::new(0);
    static STREAM_WARNING_CALLS: AtomicU64 = AtomicU64::new(0);
    static RELEASED_WRITE_STREAM: AtomicU64 = AtomicU64::new(0);
    static STREAM_OPEN_FAILURE_REASON: AtomicU64 = AtomicU64::new(0);
    static HOST_TEST_LOCK: Mutex<()> = Mutex::new(());
    static LOADER_RESULT_BYTES: &[u8] = b"resolved.dtd";
    static XPATH_RESULT_BYTES: &[u8] = b"callback-result";
    static STREAM_RESULT_BYTES: &[u8] = b"chunk";
    static BUFFERED_STREAM_BYTES: &[u8] = b"abcdefghijkl";
    static STREAM_OPEN_FAILURE_CLASS: &[u8] = b"FailureWrapper";

    /// Accepts one ownership host call while recording its opcode and descriptor payload.
    unsafe extern "C" fn accepting_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
        let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        let value =
            std::ptr::read_unaligned(request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>());
        SEEN_OPCODE.store(header.opcode, Ordering::Relaxed);
        SEEN_PAYLOAD.store(value.payload0, Ordering::Relaxed);
        *out_result = ResultHeader {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<ResultHeader>() as u32,
            status: STATUS_OK,
            value_tag: VALUE_NULL,
            php_error_kind: 0,
            dom_exception_code: 0,
            result_id: 0,
            payload0: 0,
            payload1: 0,
            bytes_ptr: std::ptr::null(),
            bytes_len: 0,
            values_ptr: std::ptr::null(),
            values_len: 0,
            diagnostics_ptr: std::ptr::null(),
            diagnostics_len: 0,
        };
        STATUS_OK
    }

    /// Validates callable-name marshalling and returns a borrowed descriptor for one name.
    unsafe extern "C" fn resolving_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header =
            std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        assert_eq!(header.opcode, HOST_OPCODE_RESOLVE_XPATH_CALLABLE);
        assert_eq!(header.flags, REQUEST_FLAG_ARGUMENT_COUNT | 1);
        assert_eq!(header.value_count, 1);
        assert_eq!(
            request_len as usize,
            ONE_VALUE_REQUEST_SIZE + header.byte_count as usize,
        );
        let value = std::ptr::read_unaligned(
            request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
        );
        assert_eq!(value.tag, VALUE_BYTES);
        assert_eq!(value.flags, 0);
        assert_eq!(value.payload0, 0);
        assert_eq!(value.payload1, header.byte_count);
        let name = std::slice::from_raw_parts(
            request_ptr.add(ONE_VALUE_REQUEST_SIZE),
            header.byte_count as usize,
        );
        *out_result = if name == b"known" {
            ResultHeader {
                value_tag: VALUE_CALLABLE,
                payload0: 0xcafe,
                ..pointer_free_null_result()
            }
        } else {
            pointer_free_null_result()
        };
        STATUS_OK
    }

    /// Returns the pointer-free signal used when a PHP Throwable escaped one host callback.
    unsafe extern "C" fn pending_throwable_host_call(
        _user_data: *mut c_void,
        _request_ptr: *const u8,
        _request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        *out_result = ResultHeader {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<ResultHeader>() as u32,
            status: STATUS_THROW,
            value_tag: VALUE_NULL,
            php_error_kind: PHP_ERROR_KIND_PENDING_HOST_THROWABLE,
            dom_exception_code: 0,
            result_id: 0,
            payload0: 0,
            payload1: 0,
            bytes_ptr: std::ptr::null(),
            bytes_len: 0,
            values_ptr: std::ptr::null(),
            values_len: 0,
            diagnostics_ptr: std::ptr::null(),
            diagnostics_len: 0,
        };
        STATUS_OK
    }

    /// Validates one stream-open request and returns the selected structured failure result.
    unsafe extern "C" fn failed_stream_open_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        assert_eq!(header.opcode, HOST_OPCODE_OPEN_STREAM);
        assert_eq!(header.flags, 1);
        assert_eq!(header.value_count as usize, STREAM_OPEN_VALUE_COUNT);
        assert_eq!(
            request_len as usize,
            STREAM_OPEN_FIXED_SIZE + header.byte_count as usize
        );
        let request = std::slice::from_raw_parts(request_ptr, request_len as usize);
        let path = std::ptr::read_unaligned(
            request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
        );
        let mode = std::ptr::read_unaligned(
            request_ptr
                .add(REQUEST_HEADER_SIZE + VALUE_SIZE)
                .cast::<Value>(),
        );
        let context = std::ptr::read_unaligned(
            request_ptr
                .add(REQUEST_HEADER_SIZE + 2 * VALUE_SIZE)
                .cast::<Value>(),
        );
        assert_eq!(path.tag, VALUE_BYTES);
        assert_eq!(mode.tag, VALUE_BYTES);
        assert_eq!(context.tag, VALUE_NULL);
        let byte_section = &request[STREAM_OPEN_FIXED_SIZE..];
        assert_eq!(
            &byte_section[path.payload0 as usize
                ..(path.payload0 + path.payload1) as usize],
            b"failure://source"
        );
        assert_eq!(
            &byte_section[mode.payload0 as usize
                ..(mode.payload0 + mode.payload1) as usize],
            b"rb"
        );
        let selected = STREAM_OPEN_FAILURE_REASON.load(Ordering::Relaxed);
        let (reason, carries_class) = if selected == 5 {
            (1, false)
        } else {
            (selected, matches!(selected, 1..=3))
        };
        *out_result = ResultHeader {
            payload0: reason,
            bytes_ptr: if carries_class {
                STREAM_OPEN_FAILURE_CLASS.as_ptr()
            } else {
                std::ptr::null()
            },
            bytes_len: if carries_class {
                STREAM_OPEN_FAILURE_CLASS.len() as u64
            } else {
                0
            },
            ..pointer_free_null_result()
        };
        STATUS_OK
    }

    /// Validates a complete external-loader request and returns one leased URI string.
    unsafe extern "C" fn external_loader_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        if header.opcode == HOST_OPCODE_RELEASE_RESULT {
            assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
            let value = std::ptr::read_unaligned(
                request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
            );
            assert_eq!(value.tag, VALUE_HOST_HANDLE);
            RELEASED_RESULT.store(value.payload0, Ordering::Relaxed);
            *out_result = pointer_free_null_result();
            return STATUS_OK;
        }

        assert_eq!(header.opcode, HOST_OPCODE_INVOKE_EXTERNAL_ENTITY_LOADER);
        assert_eq!(header.value_count as usize, EXTERNAL_LOADER_VALUE_COUNT);
        assert_eq!(
            request_len as usize,
            EXTERNAL_LOADER_FIXED_SIZE + header.byte_count as usize
        );
        let request =
            std::slice::from_raw_parts(request_ptr, request_len as usize);
        let values = (0..EXTERNAL_LOADER_VALUE_COUNT)
            .map(|index| {
                std::ptr::read_unaligned(
                    request_ptr
                        .add(REQUEST_HEADER_SIZE + index * VALUE_SIZE)
                        .cast::<Value>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(values[0], callable_value(0xbeef));
        assert_eq!(host_request_bytes(request, values[1]), Some(&b"PUB"[..]));
        assert_eq!(
            host_request_bytes(request, values[2]),
            Some(&b"virtual.dtd"[..])
        );
        assert_eq!(
            host_request_bytes(request, values[3]),
            Some(&b"/tmp"[..])
        );
        assert_eq!(
            host_request_bytes(request, values[4]),
            Some(&b"root"[..])
        );
        assert_eq!(
            host_request_bytes(request, values[5]),
            Some(&b"virtual.dtd"[..])
        );
        assert_eq!(
            host_request_bytes(request, values[6]),
            Some(&b"PUB"[..])
        );
        *out_result = ResultHeader {
            value_tag: VALUE_BYTES,
            result_id: 0x55,
            payload1: LOADER_RESULT_BYTES.len() as u64,
            bytes_ptr: LOADER_RESULT_BYTES.as_ptr(),
            bytes_len: LOADER_RESULT_BYTES.len() as u64,
            ..pointer_free_null_result()
        };
        STATUS_OK
    }

    /// Validates XPath scalar argument marshalling and returns one leased string.
    unsafe extern "C" fn xpath_callback_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header =
            std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        if header.opcode == HOST_OPCODE_RELEASE_RESULT {
            assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
            let value = std::ptr::read_unaligned(
                request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
            );
            assert_eq!(value.tag, VALUE_HOST_HANDLE);
            RELEASED_RESULT.store(value.payload0, Ordering::Relaxed);
            *out_result = pointer_free_null_result();
            return STATUS_OK;
        }

        assert_eq!(header.opcode, HOST_OPCODE_INVOKE_XPATH_CALLBACK);
        assert_eq!(header.value_count, 5);
        let fixed_size = REQUEST_HEADER_SIZE + 5 * VALUE_SIZE;
        assert_eq!(
            request_len as usize,
            fixed_size + header.byte_count as usize
        );
        let values = (0..5)
            .map(|index| {
                std::ptr::read_unaligned(
                    request_ptr
                        .add(REQUEST_HEADER_SIZE + index * VALUE_SIZE)
                        .cast::<Value>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(values[0], callable_value(0xcafe));
        assert_eq!(
            values[1],
            Value {
                tag: VALUE_NULL,
                flags: 0,
                payload0: 0,
                payload1: 0,
            }
        );
        assert_eq!(
            values[2],
            Value {
                tag: VALUE_BOOL,
                flags: 0,
                payload0: 1,
                payload1: 0,
            }
        );
        assert_eq!(
            values[3],
            Value {
                tag: VALUE_FLOAT,
                flags: 0,
                payload0: (-12.5_f64).to_bits(),
                payload1: 0,
            }
        );
        assert_eq!(values[4].tag, VALUE_BYTES);
        assert_eq!(values[4].flags, 0);
        assert_eq!(values[4].payload0, 0);
        assert_eq!(values[4].payload1, 3);
        let payload = std::slice::from_raw_parts(
            request_ptr.add(fixed_size),
            header.byte_count as usize,
        );
        assert_eq!(payload, b"a\0b");
        *out_result = ResultHeader {
            value_tag: VALUE_BYTES,
            result_id: 0x77,
            payload1: XPATH_RESULT_BYTES.len() as u64,
            bytes_ptr: XPATH_RESULT_BYTES.as_ptr(),
            bytes_len: XPATH_RESULT_BYTES.len() as u64,
            ..pointer_free_null_result()
        };
        STATUS_OK
    }

    /// Validates nested XPath node-set marshalling and returns one leased bridge handle.
    unsafe extern "C" fn xpath_node_callback_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header =
            std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        if header.opcode == HOST_OPCODE_RELEASE_RESULT {
            assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
            let value = std::ptr::read_unaligned(
                request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
            );
            assert_eq!(value.tag, VALUE_HOST_HANDLE);
            RELEASED_XPATH_NODE_RESULT
                .store(value.payload0, Ordering::Relaxed);
            *out_result = pointer_free_null_result();
            return STATUS_OK;
        }

        assert_eq!(header.opcode, HOST_OPCODE_INVOKE_XPATH_CALLBACK);
        assert_eq!(header.flags, REQUEST_FLAG_ARGUMENT_COUNT | 2);
        assert_eq!(header.value_count, 4);
        assert_eq!(header.byte_count, 0);
        assert_eq!(
            request_len as usize,
            REQUEST_HEADER_SIZE + 4 * VALUE_SIZE,
        );
        let values = (0..4)
            .map(|index| {
                std::ptr::read_unaligned(
                    request_ptr
                        .add(REQUEST_HEADER_SIZE + index * VALUE_SIZE)
                        .cast::<Value>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(values[0], callable_value(0xd00d));
        assert_eq!(
            values[1],
            Value {
                tag: VALUE_ARRAY,
                flags: 0,
                payload0: 2,
                payload1: 2,
            }
        );
        assert_eq!(
            values[2],
            Value {
                tag: VALUE_BRIDGE_HANDLE,
                flags: 0,
                payload0: 0x111,
                payload1: 201,
            }
        );
        assert_eq!(
            values[3],
            Value {
                tag: VALUE_BRIDGE_HANDLE,
                flags: 0,
                payload0: 0x222,
                payload1: 202,
            }
        );
        *out_result = ResultHeader {
            value_tag: VALUE_BRIDGE_HANDLE,
            result_id: 0x99,
            payload0: 0xbeef,
            ..pointer_free_null_result()
        };
        STATUS_OK
    }

    /// Returns one leased stream, validates its bounded read, and records both releases.
    unsafe extern "C" fn external_loader_stream_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        match header.opcode {
            HOST_OPCODE_INVOKE_EXTERNAL_ENTITY_LOADER => {
                assert_eq!(
                    request_len as usize,
                    EXTERNAL_LOADER_FIXED_SIZE + header.byte_count as usize
                );
                *out_result = ResultHeader {
                    value_tag: VALUE_RESOURCE,
                    result_id: 0x66,
                    payload0: 0,
                    payload1: 3,
                    ..pointer_free_null_result()
                };
            }
            HOST_OPCODE_READ_STREAM => {
                assert_eq!(request_len as usize, STREAM_READ_REQUEST_SIZE);
                let resource = std::ptr::read_unaligned(
                    request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
                );
                let maximum = std::ptr::read_unaligned(
                    request_ptr
                        .add(REQUEST_HEADER_SIZE + VALUE_SIZE)
                        .cast::<Value>(),
                );
                assert_eq!(
                    resource,
                    Value {
                        tag: VALUE_RESOURCE,
                        flags: 0,
                        payload0: 0,
                        payload1: 3,
                    }
                );
                assert_eq!(maximum.tag, crate::abi::VALUE_INT);
                assert_eq!(maximum.payload0, 7);
                *out_result = ResultHeader {
                    value_tag: VALUE_BYTES,
                    result_id: 0x77,
                    payload1: STREAM_RESULT_BYTES.len() as u64,
                    bytes_ptr: STREAM_RESULT_BYTES.as_ptr(),
                    bytes_len: STREAM_RESULT_BYTES.len() as u64,
                    ..pointer_free_null_result()
                };
            }
            HOST_OPCODE_RELEASE_RESULT => {
                assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
                let value = std::ptr::read_unaligned(
                    request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
                );
                assert_eq!(value.tag, VALUE_HOST_HANDLE);
                match value.payload0 {
                    0x66 => RELEASED_STREAM_RESULT.store(value.payload0, Ordering::Relaxed),
                    0x77 => RELEASED_STREAM_CHUNK.store(value.payload0, Ordering::Relaxed),
                    other => panic!("unexpected released stream result {other:#x}"),
                }
                *out_result = pointer_free_null_result();
            }
            opcode => panic!("unexpected stream host opcode {opcode}"),
        }
        STATUS_OK
    }

    /// Emulates PHP's 8192-byte userspace-wrapper read and records result releases.
    unsafe extern "C" fn buffered_wrapper_stream_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        match header.opcode {
            HOST_OPCODE_READ_STREAM => {
                assert_eq!(request_len as usize, STREAM_READ_REQUEST_SIZE);
                let values = request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>();
                let resource = std::ptr::read_unaligned(values);
                let maximum = std::ptr::read_unaligned(values.add(1));
                assert_eq!(
                    resource,
                    Value {
                        tag: VALUE_RESOURCE,
                        flags: 0,
                        payload0: USER_WRAPPER_FD_BASE,
                        payload1: 1,
                    }
                );
                assert_eq!(maximum.tag, crate::abi::VALUE_INT);
                assert_eq!(maximum.payload0, PHP_STREAM_BUFFER_SIZE as u64);
                BUFFERED_STREAM_READS.fetch_add(1, Ordering::Relaxed);
                *out_result = ResultHeader {
                    value_tag: VALUE_BYTES,
                    result_id: 0x88,
                    payload1: BUFFERED_STREAM_BYTES.len() as u64,
                    bytes_ptr: BUFFERED_STREAM_BYTES.as_ptr(),
                    bytes_len: BUFFERED_STREAM_BYTES.len() as u64,
                    ..pointer_free_null_result()
                };
            }
            HOST_OPCODE_RELEASE_RESULT => {
                assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
                let value = std::ptr::read_unaligned(
                    request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
                );
                assert_eq!(value.tag, VALUE_HOST_HANDLE);
                RELEASED_BUFFERED_STREAM.store(value.payload0, Ordering::Relaxed);
                *out_result = pointer_free_null_result();
            }
            opcode => panic!("unexpected buffered stream host opcode {opcode}"),
        }
        STATUS_OK
    }

    /// Validates write and flush host messages while exposing partial, false, and zero results.
    unsafe extern "C" fn write_stream_host_call(
        _user_data: *mut c_void,
        request_ptr: *const u8,
        request_len: u64,
        out_result: *mut ResultHeader,
    ) -> u32 {
        let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
        match header.opcode {
            HOST_OPCODE_WRITE_STREAM => {
                assert_eq!(
                    request_len as usize,
                    STREAM_WRITE_FIXED_SIZE + header.byte_count as usize
                );
                assert_eq!(header.value_count as usize, STREAM_WRITE_VALUE_COUNT);
                let values = request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>();
                let resource = std::ptr::read_unaligned(values);
                let bytes = std::ptr::read_unaligned(values.add(1));
                assert_eq!(
                    resource,
                    Value {
                        tag: VALUE_RESOURCE,
                        flags: 0,
                        payload0: USER_WRAPPER_FD_BASE,
                        payload1: 1,
                    }
                );
                assert_eq!(bytes.tag, VALUE_BYTES);
                assert_eq!(bytes.flags, 0);
                assert_eq!(bytes.payload0, 0);
                assert_eq!(bytes.payload1, header.byte_count);
                let payload = std::slice::from_raw_parts(
                    request_ptr.add(STREAM_WRITE_FIXED_SIZE),
                    header.byte_count as usize,
                );
                assert!(matches!(
                    payload,
                    b"abcdef" | b"false" | b"zero" | b"oversized" | b"negative"
                ));
                STREAM_WRITE_CALLS.fetch_add(1, Ordering::Relaxed);
                let written = match STREAM_WRITE_MODE.load(Ordering::Relaxed) {
                    0 => 3_i64,
                    1 => -1,
                    2 => 0,
                    3 => header.byte_count as i64 + 2,
                    4 => -2,
                    mode => panic!("unexpected stream write mode {mode}"),
                };
                *out_result = ResultHeader {
                    value_tag: crate::abi::VALUE_INT,
                    payload0: written as u64,
                    ..pointer_free_null_result()
                };
            }
            HOST_OPCODE_FLUSH_STREAM => {
                assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
                let value = std::ptr::read_unaligned(
                    request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
                );
                assert_eq!(
                    value,
                    Value {
                        tag: VALUE_RESOURCE,
                        flags: 0,
                        payload0: USER_WRAPPER_FD_BASE,
                        payload1: 1,
                    }
                );
                STREAM_FLUSH_CALLS.fetch_add(1, Ordering::Relaxed);
                *out_result = pointer_free_null_result();
            }
            HOST_OPCODE_EMIT_WARNING => {
                assert_eq!(
                    request_len as usize,
                    WARNING_FIXED_SIZE + header.byte_count as usize
                );
                assert_eq!(header.value_count, 1);
                let value = std::ptr::read_unaligned(
                    request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
                );
                assert_eq!(
                    value,
                    Value {
                        tag: VALUE_BYTES,
                        flags: 0,
                        payload0: 0,
                        payload1: header.byte_count,
                    }
                );
                let payload = std::slice::from_raw_parts(
                    request_ptr.add(WARNING_FIXED_SIZE),
                    header.byte_count as usize,
                );
                assert_eq!(payload, b"Warning: oversized write\n");
                STREAM_WARNING_CALLS.fetch_add(1, Ordering::Relaxed);
                *out_result = pointer_free_null_result();
            }
            HOST_OPCODE_RELEASE_RESULT => {
                assert_eq!(request_len as usize, ONE_VALUE_REQUEST_SIZE);
                let value = std::ptr::read_unaligned(
                    request_ptr.add(REQUEST_HEADER_SIZE).cast::<Value>(),
                );
                assert_eq!(value.tag, VALUE_HOST_HANDLE);
                RELEASED_WRITE_STREAM.store(value.payload0, Ordering::Relaxed);
                *out_result = pointer_free_null_result();
            }
            opcode => panic!("unexpected write stream host opcode {opcode}"),
        }
        STATUS_OK
    }

    /// Builds the canonical successful pointer-free null host result.
    fn pointer_free_null_result() -> ResultHeader {
        ResultHeader {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<ResultHeader>() as u32,
            status: STATUS_OK,
            value_tag: VALUE_NULL,
            php_error_kind: 0,
            dom_exception_code: 0,
            result_id: 0,
            payload0: 0,
            payload1: 0,
            bytes_ptr: std::ptr::null(),
            bytes_len: 0,
            values_ptr: std::ptr::null(),
            values_len: 0,
            diagnostics_ptr: std::ptr::null(),
            diagnostics_len: 0,
        }
    }

    /// Returns one nullable string value from a fully validated host request.
    fn host_request_bytes(request: &[u8], value: Value) -> Option<&[u8]> {
        if value.tag == VALUE_NULL {
            return None;
        }
        assert_eq!(value.tag, VALUE_BYTES);
        let start = EXTERNAL_LOADER_FIXED_SIZE + value.payload0 as usize;
        let end = start + value.payload1 as usize;
        Some(&request[start..end])
    }

    /// Verifies callable ownership requests use the locked padded message layout.
    #[test]
    fn callable_ownership_uses_flat_host_messages() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        let host = Host {
            user_data: 0,
            call: Some(accepting_host_call),
        };
        retain_callable(host, 0x1234).expect("retain succeeds");
        assert_eq!(
            SEEN_OPCODE.load(Ordering::Relaxed),
            HOST_OPCODE_RETAIN_CALLABLE
        );
        assert_eq!(SEEN_PAYLOAD.load(Ordering::Relaxed), 0x1234);
        release_callable(host, 0x5678).expect("release succeeds");
        assert_eq!(
            SEEN_OPCODE.load(Ordering::Relaxed),
            HOST_OPCODE_RELEASE_CALLABLE
        );
        assert_eq!(SEEN_PAYLOAD.load(Ordering::Relaxed), 0x5678);
    }

    /// Verifies callable-name resolution accepts only pointer-free callable or null results.
    #[test]
    fn xpath_callable_resolution_uses_one_flat_byte_root() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        let host = Host {
            user_data: 0,
            call: Some(resolving_host_call),
        };
        assert_eq!(
            resolve_xpath_callable(host, b"known").expect("known resolution"),
            Some(0xcafe),
        );
        assert_eq!(
            resolve_xpath_callable(host, b"missing")
                .expect("missing resolution"),
            None,
        );
        assert_eq!(
            resolve_xpath_callable(host, b""),
            Err(HostCallError::Abi),
        );
    }

    /// Verifies a contained PHP Throwable remains distinct from malformed host traffic.
    #[test]
    fn callable_ownership_preserves_pending_host_throwable() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        let host = Host {
            user_data: 0,
            call: Some(pending_throwable_host_call),
        };
        assert_eq!(
            release_callable(host, 0x5678),
            Err(HostCallError::PendingThrowable)
        );
    }

    /// Verifies stream-open null results preserve every locked PHP warning discriminator.
    #[test]
    fn document_stream_open_marshalling_preserves_failure_details() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        let host = Host {
            user_data: 0,
            call: Some(failed_stream_open_host_call),
        };
        for (reason, expected) in [
            (0, StreamOpenFailure::Silent),
            (1, StreamOpenFailure::MissingUrlStat(
                STREAM_OPEN_FAILURE_CLASS.to_vec(),
            )),
            (2, StreamOpenFailure::MissingStreamOpen(
                STREAM_OPEN_FAILURE_CLASS.to_vec(),
            )),
            (3, StreamOpenFailure::StreamOpenFailed(
                STREAM_OPEN_FAILURE_CLASS.to_vec(),
            )),
            (4, StreamOpenFailure::Silent),
        ] {
            STREAM_OPEN_FAILURE_REASON.store(reason, Ordering::Relaxed);
            let result = open_stream(
                host,
                b"failure://source",
                b"rb",
                None,
                true,
            )
            .expect("structured stream-open failure");
            let StreamOpenResult::Failed(actual) = result else {
                panic!("expected one failed stream-open result");
            };
            assert_eq!(actual, expected);
        }
        STREAM_OPEN_FAILURE_REASON.store(5, Ordering::Relaxed);
        assert!(matches!(
            open_stream(host, b"failure://source", b"rb", None, true),
            Err(HostCallError::Abi)
        ));
    }

    /// Verifies external-loader arguments and leased string results use the locked flat ABI.
    #[test]
    fn external_loader_marshalling_and_result_release_are_balanced() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        RELEASED_RESULT.store(0, Ordering::Relaxed);
        let host = Host {
            user_data: 0,
            call: Some(external_loader_host_call),
        };
        let result = invoke_external_entity_loader(
            host,
            0xbeef,
            Some(b"PUB"),
            Some(b"virtual.dtd"),
            ExternalEntityContext {
                directory: Some(b"/tmp"),
                int_sub_name: Some(b"root"),
                ext_sub_uri: Some(b"virtual.dtd"),
                ext_sub_system: Some(b"PUB"),
            },
        )
        .expect("loader invocation succeeds");
        match result {
            ExternalEntityResult::Bytes(bytes) => {
                assert_eq!(bytes, LOADER_RESULT_BYTES);
            }
            other => panic!("unexpected loader result: {other:?}"),
        }
        assert_eq!(RELEASED_RESULT.load(Ordering::Relaxed), 0x55);
    }

    /// Verifies XPath scalar arguments preserve order, types, bytes, and result ownership.
    #[test]
    fn xpath_callback_marshalling_and_result_release_are_balanced() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        RELEASED_RESULT.store(0, Ordering::Relaxed);
        let host = Host {
            user_data: 0,
            call: Some(xpath_callback_host_call),
        };
        let result = invoke_xpath_callback(
            host,
            0xcafe,
            &[
                XPathCallbackArgument::Null,
                XPathCallbackArgument::Boolean(true),
                XPathCallbackArgument::Number(-12.5),
                XPathCallbackArgument::Bytes(b"a\0b".to_vec()),
            ],
        )
        .expect("XPath callback invocation succeeds");
        assert_eq!(
            result,
            XPathCallbackResult::Bytes(XPATH_RESULT_BYTES.to_vec())
        );
        assert_eq!(RELEASED_RESULT.load(Ordering::Relaxed), 0x77);
    }

    /// Verifies nested node handles and callback-result leases remain ownership-balanced.
    #[test]
    fn xpath_callback_node_sets_and_dom_results_are_balanced() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        RELEASED_XPATH_NODE_RESULT.store(0, Ordering::Relaxed);
        let host = Host {
            user_data: 0,
            call: Some(xpath_node_callback_host_call),
        };
        let result = invoke_xpath_callback(
            host,
            0xd00d,
            &[XPathCallbackArgument::Nodes(vec![
                XPathCallbackNode {
                    handle: 0x111,
                    wrapper_kind: 201,
                },
                XPathCallbackNode {
                    handle: 0x222,
                    wrapper_kind: 202,
                },
            ])],
        )
        .expect("XPath node callback invocation succeeds");
        let XPathCallbackResult::Node { handle, lease } = result else {
            panic!("expected one leased DOM node result");
        };
        assert_eq!(handle, 0xbeef);
        assert_eq!(
            RELEASED_XPATH_NODE_RESULT.load(Ordering::Relaxed),
            0,
        );
        drop(lease);
        assert_eq!(
            RELEASED_XPATH_NODE_RESULT.load(Ordering::Relaxed),
            0x99,
        );
    }

    /// Verifies an escaped resolver Throwable stays distinct from malformed host traffic.
    #[test]
    fn external_loader_preserves_pending_host_throwable() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        let host = Host {
            user_data: 0,
            call: Some(pending_throwable_host_call),
        };
        assert!(matches!(
            invoke_external_entity_loader(
                host,
                0xbeef,
                None,
                Some(b"virtual.dtd"),
                ExternalEntityContext {
                    directory: None,
                    int_sub_name: None,
                    ext_sub_uri: None,
                    ext_sub_system: None,
                },
            ),
            Err(HostCallError::PendingThrowable)
        ));
    }

    /// Verifies a callback-returned stream remains leased through reads and closes exactly once.
    #[test]
    fn external_loader_stream_reads_and_releases_are_balanced() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        RELEASED_STREAM_RESULT.store(0, Ordering::Relaxed);
        RELEASED_STREAM_CHUNK.store(0, Ordering::Relaxed);
        let host = Host {
            user_data: 0,
            call: Some(external_loader_stream_host_call),
        };
        let result = invoke_external_entity_loader(
            host,
            0xbeef,
            None,
            Some(b"virtual.dtd"),
            ExternalEntityContext {
                directory: None,
                int_sub_name: Some(b"root"),
                ext_sub_uri: None,
                ext_sub_system: Some(b"virtual.dtd"),
            },
        )
        .expect("loader invocation succeeds");
        let ExternalEntityResult::Resource(resource) = result else {
            panic!("expected one leased stream resource");
        };
        let lease_id = register_stream_lease(resource);
        assert_ne!(lease_id, 0);
        assert_eq!(
            read_stream_lease(lease_id, 7).expect("stream read succeeds"),
            STREAM_RESULT_BYTES
        );
        assert_eq!(RELEASED_STREAM_CHUNK.load(Ordering::Relaxed), 0x77);
        release_stream_lease(lease_id).expect("stream lease release succeeds");
        assert_eq!(RELEASED_STREAM_RESULT.load(Ordering::Relaxed), 0x66);
        assert_eq!(
            read_stream_lease(lease_id, 7),
            Err(HostCallError::Abi)
        );
    }

    /// Verifies userspace-wrapper reads use PHP's 8192-byte buffer without a second host call.
    #[test]
    fn external_loader_userspace_wrapper_reads_are_buffered_like_php() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        BUFFERED_STREAM_READS.store(0, Ordering::Relaxed);
        RELEASED_BUFFERED_STREAM.store(0, Ordering::Relaxed);
        let host = Host {
            user_data: 0,
            call: Some(buffered_wrapper_stream_host_call),
        };
        let lease_id = register_stream_lease(LeasedHostResource {
            resource: USER_WRAPPER_FD_BASE,
            resource_kind: 1,
            class_name: Vec::new(),
            host,
            result_id: 0x99,
            buffered: Vec::new(),
        });
        assert_eq!(
            read_stream_lease(lease_id, 7).expect("first buffered read succeeds"),
            b"abcdefg"
        );
        assert_eq!(
            read_stream_lease(lease_id, 7).expect("second buffered read succeeds"),
            b"hijkl"
        );
        assert_eq!(BUFFERED_STREAM_READS.load(Ordering::Relaxed), 1);
        assert_eq!(RELEASED_BUFFERED_STREAM.load(Ordering::Relaxed), 0x88);
        release_stream_lease(lease_id).expect("stream lease release succeeds");
        assert_eq!(RELEASED_BUFFERED_STREAM.load(Ordering::Relaxed), 0x99);
    }

    /// Verifies stream writes preserve partial counts, exact false, zero, flush, and release.
    #[test]
    fn document_stream_write_marshalling_preserves_php_results() {
        let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
        STREAM_WRITE_CALLS.store(0, Ordering::Relaxed);
        STREAM_FLUSH_CALLS.store(0, Ordering::Relaxed);
        STREAM_WARNING_CALLS.store(0, Ordering::Relaxed);
        RELEASED_WRITE_STREAM.store(0, Ordering::Relaxed);
        let host = Host {
            user_data: 0,
            call: Some(write_stream_host_call),
        };
        let stream = LeasedHostResource {
            resource: USER_WRAPPER_FD_BASE,
            resource_kind: 1,
            class_name: b"WriteWrapper".to_vec(),
            host,
            result_id: 0xaa,
            buffered: Vec::new(),
        };
        STREAM_WRITE_MODE.store(0, Ordering::Relaxed);
        assert_eq!(
            write_stream_chunk(&stream, b"abcdef").expect("partial write"),
            StreamWriteResult::Written(3)
        );
        STREAM_WRITE_MODE.store(1, Ordering::Relaxed);
        assert_eq!(
            write_stream_chunk(&stream, b"false").expect("false write"),
            StreamWriteResult::Failed
        );
        STREAM_WRITE_MODE.store(2, Ordering::Relaxed);
        assert_eq!(
            write_stream_chunk(&stream, b"zero").expect("zero write"),
            StreamWriteResult::Written(0)
        );
        STREAM_WRITE_MODE.store(3, Ordering::Relaxed);
        assert_eq!(
            write_stream_chunk(&stream, b"oversized").expect("oversized write"),
            StreamWriteResult::Oversized(11)
        );
        STREAM_WRITE_MODE.store(4, Ordering::Relaxed);
        assert_eq!(
            write_stream_chunk(&stream, b"negative").expect("negative write"),
            StreamWriteResult::Failed
        );
        emit_warning(host, b"Warning: oversized write\n")
            .expect("warning emission succeeds");
        flush_stream(&stream).expect("flush succeeds");
        stream.release().expect("stream release succeeds");
        assert_eq!(STREAM_WRITE_CALLS.load(Ordering::Relaxed), 5);
        assert_eq!(STREAM_FLUSH_CALLS.load(Ordering::Relaxed), 1);
        assert_eq!(STREAM_WARNING_CALLS.load(Ordering::Relaxed), 1);
        assert_eq!(RELEASED_WRITE_STREAM.load(Ordering::Relaxed), 0xaa);
    }
}
