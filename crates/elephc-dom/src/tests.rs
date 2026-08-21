//! Purpose:
//! Exercises the DOM bridge C ABI boundary, request validation, and independent result lifetimes.
//! Covers malformed inputs that must not mutate bridge context state.
//!
//! Called from:
//! - `cargo test -p elephc-dom`.
//!
//! Key details:
//! - Tests invoke exported functions exactly through their public pointer-based records.
//! - Result bytes are copied only while the matching result ID remains retained.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::abi::{
    DomClassMetadataEntry, HostVTable, RequestHeader, ResultHeader, Value,
    ABI_VERSION, DOM_CLASS_NO_PARENT,
    HOST_OPCODE_OPEN_STREAM, HOST_OPCODE_RELEASE_CALLABLE,
    HOST_OPCODE_RELEASE_RESULT, HOST_OPCODE_RETAIN_CALLABLE, OPCODE_ABI_PING,
    PHP_ERROR_KIND_DOM_EXCEPTION, PHP_ERROR_KIND_ERROR,
    PHP_ERROR_KIND_EXCEPTION, PHP_ERROR_KIND_PENDING_HOST_THROWABLE,
    PHP_ERROR_KIND_TYPE_ERROR, PHP_ERROR_KIND_VALUE_ERROR,
    REQUEST_FLAG_ARGUMENT_COUNT, STATUS_ABI_ERROR, STATUS_MALFORMED_REQUEST,
    STATUS_OK, STATUS_THROW,
    VALUE_ARRAY, VALUE_BOOL, VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_FLOAT,
    VALUE_INT, VALUE_MAP, VALUE_NULL, VALUE_OBJECT, VALUE_RESOURCE,
};

static HOST_RETAINS: AtomicU32 = AtomicU32::new(0);
static HOST_RELEASES: AtomicU32 = AtomicU32::new(0);
static HOST_THROW_OPCODE: AtomicU32 = AtomicU32::new(0);
static HOST_REENTRANT_CONTEXT: AtomicU64 = AtomicU64::new(0);
static HOST_TEST_LOCK: Mutex<()> = Mutex::new(());
static FILE_TEST_ID: AtomicU32 = AtomicU32::new(0);
static INPUT_FROM_IO_FAILURE_CLOSES: AtomicU32 = AtomicU32::new(0);

/// Host callback probe that rejects every callback request.
unsafe extern "C" fn rejecting_host_call(
    _user_data: *mut c_void,
    _request_ptr: *const u8,
    _request_len: u64,
    _out_result: *mut ResultHeader,
) -> u32 {
    STATUS_ABI_ERROR
}

/// Opens a leased stream and records its release from the native input-allocation failure path.
unsafe extern "C" fn input_from_io_failure_host_call(
    _user_data: *mut c_void,
    request_ptr: *const u8,
    _request_len: u64,
    out_result: *mut ResultHeader,
) -> u32 {
    let header = std::ptr::read_unaligned(request_ptr.cast::<RequestHeader>());
    match header.opcode {
        HOST_OPCODE_OPEN_STREAM => {
            *out_result = ResultHeader {
                value_tag: VALUE_RESOURCE,
                result_id: 0x711,
                payload0: 0x822,
                payload1: 3,
                ..ResultHeader::abi_error()
            };
            (*out_result).status = STATUS_OK;
        }
        HOST_OPCODE_RELEASE_RESULT => {
            let value = std::ptr::read_unaligned(
                request_ptr.add(std::mem::size_of::<RequestHeader>()).cast::<Value>(),
            );
            assert_eq!(value.payload0, 0x711);
            INPUT_FROM_IO_FAILURE_CLOSES.fetch_add(1, Ordering::Relaxed);
            *out_result = ResultHeader {
                status: STATUS_OK,
                value_tag: VALUE_NULL,
                ..ResultHeader::abi_error()
            };
        }
        opcode => panic!("unexpected input-allocation failure host opcode {opcode}"),
    }
    STATUS_OK
}

/// Accepts callable ownership requests and returns one valid pointer-free null result.
unsafe extern "C" fn accepting_host_call(
    _user_data: *mut c_void,
    request_ptr: *const u8,
    request_len: u64,
    out_result: *mut ResultHeader,
) -> u32 {
    assert_eq!(request_len, 72);
    let request = std::slice::from_raw_parts(request_ptr, request_len as usize);
    let header = std::ptr::read_unaligned(request.as_ptr().cast::<RequestHeader>());
    let value = std::ptr::read_unaligned(request.as_ptr().add(48).cast::<Value>());
    assert_eq!(header.header_size, 48);
    assert_eq!(value.tag, crate::abi::VALUE_CALLABLE);
    match header.opcode {
        HOST_OPCODE_RETAIN_CALLABLE => {
            HOST_RETAINS.fetch_add(1, Ordering::Relaxed);
        }
        HOST_OPCODE_RELEASE_CALLABLE => {
            HOST_RELEASES.fetch_add(1, Ordering::Relaxed);
        }
        opcode => panic!("unexpected host opcode {opcode}"),
    }
    let reentrant_context = HOST_REENTRANT_CONTEXT.load(Ordering::Relaxed);
    if header.opcode == HOST_OPCODE_RELEASE_CALLABLE && reentrant_context != 0 {
        let request = ping_request();
        let mut nested = ResultHeader::abi_error();
        assert_eq!(
            crate::elephc_dom_call(
                reentrant_context,
                request.as_ptr(),
                request.len() as u64,
                &mut nested,
            ),
            STATUS_OK
        );
        assert_eq!(nested.status, STATUS_OK);
        crate::elephc_dom_result_release(reentrant_context, nested.result_id);
    }
    if HOST_THROW_OPCODE.load(Ordering::Relaxed) == header.opcode {
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
        return STATUS_OK;
    }
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

/// Creates one context with the current host vtable shape.
fn new_context() -> u64 {
    new_context_with_host(Some(rejecting_host_call))
}

/// Creates one context with a selected host callback for ownership-state tests.
fn new_context_with_host(call: Option<crate::abi::HostCall>) -> u64 {
    let host = HostVTable {
        abi_version: ABI_VERSION,
        struct_size: std::mem::size_of::<HostVTable>() as u32,
        user_data: std::ptr::null_mut(),
        call,
    };
    let mut context = 0;
    let status = unsafe { crate::elephc_dom_context_new(&host, &mut context) };
    assert_eq!(status, STATUS_OK);
    assert_ne!(context, 0);
    context
}

/// Encodes a request header followed by values and raw bytes using the public layout.
fn request_bytes(header: RequestHeader, values: &[Value], bytes: &[u8]) -> Vec<u8> {
    let total = std::mem::size_of::<RequestHeader>()
        + values.len() * std::mem::size_of::<Value>()
        + bytes.len();
    let mut request = Vec::with_capacity(total);
    unsafe {
        request.extend_from_slice(std::slice::from_raw_parts(
            (&header as *const RequestHeader).cast::<u8>(),
            std::mem::size_of::<RequestHeader>(),
        ));
        request.extend_from_slice(std::slice::from_raw_parts(
            values.as_ptr().cast::<u8>(),
            std::mem::size_of_val(values),
        ));
    }
    request.extend_from_slice(bytes);
    request
}

/// Builds the canonical empty ABI-ping request.
fn ping_request() -> Vec<u8> {
    request_bytes(
        RequestHeader {
            abi_version: ABI_VERSION,
            header_size: std::mem::size_of::<RequestHeader>() as u32,
            opcode: OPCODE_ABI_PING,
            flags: 0,
            receiver: 0,
            value_count: 0,
            byte_count: 0,
        },
        &[],
        &[],
    )
}

/// Returns the generated stable opcode for one exact operation key.
fn opcode(key: &str) -> u32 {
    crate::generated::opcodes::OPERATIONS
        .iter()
        .find_map(|(opcode, candidate)| (*candidate == key).then_some(*opcode))
        .unwrap_or_else(|| panic!("unknown test operation key: {key}"))
}

/// Invokes one bridge operation through the public flat-message entry point.
fn invoke(
    context: u64,
    operation: &str,
    receiver: u64,
    values: &[Value],
    bytes: &[u8],
) -> (u32, ResultHeader) {
    let request = request_bytes(
        RequestHeader {
            abi_version: ABI_VERSION,
            header_size: std::mem::size_of::<RequestHeader>() as u32,
            opcode: opcode(operation),
            flags: 0,
            receiver,
            value_count: values.len() as u64,
            byte_count: bytes.len() as u64,
        },
        values,
        bytes,
    );
    let mut result = ResultHeader::abi_error();
    let status = unsafe {
        crate::elephc_dom_call(
            context,
            request.as_ptr(),
            request.len() as u64,
            &mut result,
        )
    };
    (status, result)
}

/// Copies result bytes while their matching retained result frame is live.
fn result_bytes(result: &ResultHeader) -> Vec<u8> {
    if result.bytes_ptr.is_null() {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(result.bytes_ptr, result.bytes_len as usize)
        }
        .to_vec()
    }
}

/// Copies flat result values while their matching retained result frame is live.
fn result_values(result: &ResultHeader) -> Vec<Value> {
    if result.values_ptr.is_null() {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(result.values_ptr, result.values_len as usize)
        }
        .to_vec()
    }
}

/// Copies flat diagnostics while their matching retained result frame is live.
fn result_diagnostics(result: &ResultHeader) -> Vec<crate::abi::Diagnostic> {
    if result.diagnostics_ptr.is_null() {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(
                result.diagnostics_ptr,
                result.diagnostics_len as usize,
            )
        }
        .to_vec()
    }
}

/// Verifies nested calls retain independent byte buffers until their own release.
#[test]
fn reentrant_result_frames_have_independent_lifetimes() {
    let context = new_context();
    let request = ping_request();
    let mut first = ResultHeader::abi_error();
    let mut second = ResultHeader::abi_error();

    assert_eq!(
        unsafe {
            crate::elephc_dom_call(
                context,
                request.as_ptr(),
                request.len() as u64,
                &mut first,
            )
        },
        STATUS_OK
    );
    assert_eq!(
        unsafe {
            crate::elephc_dom_call(
                context,
                request.as_ptr(),
                request.len() as u64,
                &mut second,
            )
        },
        STATUS_OK
    );
    assert_ne!(first.result_id, second.result_id);
    assert_eq!(first.value_tag, VALUE_BYTES);
    let first_bytes =
        unsafe { std::slice::from_raw_parts(first.bytes_ptr, first.bytes_len as usize) }.to_vec();
    let second_bytes =
        unsafe { std::slice::from_raw_parts(second.bytes_ptr, second.bytes_len as usize) }.to_vec();
    assert_eq!(first_bytes, second_bytes);

    crate::elephc_dom_result_release(context, second.result_id);
    assert_eq!(
        unsafe { std::slice::from_raw_parts(first.bytes_ptr, first.bytes_len as usize) },
        first_bytes
    );
    crate::elephc_dom_result_release(context, first.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies truncated and version-mismatched messages are rejected without a result frame.
#[test]
fn malformed_requests_return_abi_error_without_mutation() {
    let context = new_context();
    let request = ping_request();
    let mut result = ResultHeader::abi_error();
    assert_eq!(
        unsafe {
            crate::elephc_dom_call(context, request.as_ptr(), 3, &mut result)
        },
        STATUS_ABI_ERROR
    );
    assert_eq!(result.result_id, 0);

    let mut bad_version = ping_request();
    bad_version[0] = 0xff;
    assert_eq!(
        unsafe {
            crate::elephc_dom_call(
                context,
                bad_version.as_ptr(),
                bad_version.len() as u64,
                &mut result,
            )
        },
        STATUS_ABI_ERROR
    );
    assert_eq!(result.result_id, 0);
    crate::elephc_dom_context_free(context);
}

/// Rejects malformed class metadata without replacing the last valid compiler table.
#[test]
fn malformed_class_metadata_returns_malformed_request_without_mutation() {
    let context = new_context();
    let name = b"DOMNode";
    let valid = DomClassMetadataEntry {
        name_ptr: name.as_ptr(),
        name_len: name.len() as u64,
        class_id: 0x30,
        parent_class_id: DOM_CLASS_NO_PARENT,
        is_abstract: 0,
        reserved: 0,
    };
    assert_eq!(
        unsafe { crate::elephc_dom_context_set_class_metadata(context, &valid, 1) },
        STATUS_OK
    );

    let invalid = DomClassMetadataEntry {
        reserved: 1,
        ..valid
    };
    assert_eq!(
        unsafe { crate::elephc_dom_context_set_class_metadata(context, &invalid, 1) },
        STATUS_MALFORMED_REQUEST
    );
    assert_eq!(
        crate::context::context(context)
            .expect("context remains registered")
            .borrow()
            .class_metadata
            .by_name(b"DOMNode")
            .expect("previous metadata remains installed")
            .id,
        valid.class_id
    );

    let duplicate = DomClassMetadataEntry {
        class_id: valid.class_id + 1,
        ..valid
    };
    assert_eq!(
        unsafe {
            crate::elephc_dom_context_set_class_metadata(
                context,
                [valid, duplicate].as_ptr(),
                2,
            )
        },
        STATUS_MALFORMED_REQUEST
    );

    let second_name = b"DOMElement";
    let second = DomClassMetadataEntry {
        name_ptr: second_name.as_ptr(),
        name_len: second_name.len() as u64,
        class_id: valid.class_id + 2,
        ..valid
    };
    assert_eq!(
        unsafe {
            crate::elephc_dom_context_set_class_metadata(
                context,
                [valid, invalid, second].as_ptr(),
                3,
            )
        },
        STATUS_MALFORMED_REQUEST
    );
    assert_eq!(
        crate::context::context(context)
            .expect("context remains registered")
            .borrow()
            .class_metadata
            .by_name(b"DOMNode")
            .expect("invalid metadata never replaces the installed table")
            .id,
        valid.class_id
    );

    assert_eq!(
        unsafe {
            crate::elephc_dom_context_set_class_metadata(
                context,
                std::ptr::null(),
                0,
            )
        },
        STATUS_OK
    );
    assert!(
        crate::context::context(context)
            .expect("context remains registered")
            .borrow()
            .class_metadata
            .by_name(b"DOMNode")
            .is_none(),
        "an empty metadata snapshot clears prior compiler rows"
    );
    crate::elephc_dom_context_free(context);
}

/// Balances a callback stream lease when libxml refuses the newly allocated input object.
#[test]
fn native_resource_loader_closes_stream_when_input_creation_fails() {
    let context = new_context_with_host(Some(input_from_io_failure_host_call));
    INPUT_FROM_IO_FAILURE_CLOSES.store(0, Ordering::Relaxed);

    assert_eq!(
        crate::native::test_resource_loader_input_from_io_failure(context),
        crate::native::TEST_RESOURCE_LOADER_INPUT_CREATION_FAILED,
    );
    assert_eq!(INPUT_FROM_IO_FAILURE_CLOSES.load(Ordering::Relaxed), 1);
    crate::elephc_dom_context_free(context);
}

/// Verifies byte ranges and map value counts reject overflow and out-of-bounds slices.
#[test]
fn flat_value_ranges_are_bounds_checked() {
    let context = new_context();
    let values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 2,
        payload1: u64::MAX,
    }];
    let request = request_bytes(
        RequestHeader {
            abi_version: ABI_VERSION,
            header_size: std::mem::size_of::<RequestHeader>() as u32,
            opcode: OPCODE_ABI_PING,
            flags: 0,
            receiver: 0,
            value_count: 1,
            byte_count: 1,
        },
        &values,
        b"x",
    );
    let mut result = ResultHeader::abi_error();
    assert_eq!(
        unsafe {
            crate::elephc_dom_call(
                context,
                request.as_ptr(),
                request.len() as u64,
                &mut result,
            )
        },
        STATUS_ABI_ERROR
    );
    crate::elephc_dom_context_free(context);
}

/// Verifies root arguments can own nested map and array records without changing public arity.
#[test]
fn nested_request_values_are_validated_and_exposed_from_root_arguments() {
    let bytes = b"query//itempurn:example";
    let values = [
        Value {
            tag: VALUE_MAP,
            flags: 0,
            payload0: 2,
            payload1: 2,
        },
        Value {
            tag: VALUE_ARRAY,
            flags: 0,
            payload0: 6,
            payload1: 1,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 5,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 5,
            payload1: 6,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 11,
            payload1: 1,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 12,
            payload1: 11,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 11,
            payload1: 1,
        },
    ];
    let request = request_bytes(
        RequestHeader {
            abi_version: ABI_VERSION,
            header_size: std::mem::size_of::<RequestHeader>() as u32,
            opcode: OPCODE_ABI_PING,
            flags: REQUEST_FLAG_ARGUMENT_COUNT | 2,
            receiver: 0,
            value_count: values.len() as u64,
            byte_count: bytes.len() as u64,
        },
        &values,
        bytes,
    );

    let decoded = crate::request::decode(request.as_ptr(), request.len() as u64)
        .expect("nested request must validate");
    assert_eq!(decoded.values.len(), 2);
    let map = decoded.map_values(0).expect("first argument is a map");
    assert_eq!(map.len(), 4);
    assert_eq!(
        decoded.bytes_for_value(&map[0]).expect("query key bytes"),
        b"query"
    );
    assert_eq!(
        decoded.bytes_for_value(&map[1]).expect("query value bytes"),
        b"//item"
    );
    let array = decoded.array_values(1).expect("second argument is an array");
    assert_eq!(array.len(), 1);
    assert_eq!(
        decoded.bytes_for_value(&array[0]).expect("prefix bytes"),
        b"p"
    );
}

/// Verifies nested request trees reject cycles, shared descendants, and orphan records.
#[test]
fn nested_request_values_reject_non_tree_layouts() {
    let header = |value_count| RequestHeader {
        abi_version: ABI_VERSION,
        header_size: std::mem::size_of::<RequestHeader>() as u32,
        opcode: OPCODE_ABI_PING,
        flags: REQUEST_FLAG_ARGUMENT_COUNT | 1,
        receiver: 0,
        value_count,
        byte_count: 0,
    };
    let cycle = [
        Value {
            tag: VALUE_ARRAY,
            flags: 0,
            payload0: 1,
            payload1: 1,
        },
        Value {
            tag: VALUE_ARRAY,
            flags: 0,
            payload0: 1,
            payload1: 1,
        },
    ];
    let cycle_request = request_bytes(header(2), &cycle, &[]);
    assert!(crate::request::decode(
        cycle_request.as_ptr(),
        cycle_request.len() as u64
    )
    .is_err());

    let orphan = [
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
    ];
    let orphan_request = request_bytes(header(2), &orphan, &[]);
    assert!(crate::request::decode(
        orphan_request.as_ptr(),
        orphan_request.len() as u64
    )
    .is_err());

    let shared = [
        Value {
            tag: VALUE_ARRAY,
            flags: 0,
            payload0: 2,
            payload1: 2,
        },
        Value {
            tag: VALUE_ARRAY,
            flags: 0,
            payload0: 3,
            payload1: 1,
        },
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
    ];
    let shared_request = request_bytes(
        RequestHeader {
            flags: REQUEST_FLAG_ARGUMENT_COUNT | 2,
            ..header(4)
        },
        &shared,
        &[],
    );
    assert!(crate::request::decode(
        shared_request.as_ptr(),
        shared_request.len() as u64
    )
    .is_err());
}

/// Verifies reset invalidates retained result IDs while preserving the context ID.
#[test]
fn context_reset_releases_all_result_frames() {
    let context = new_context();
    let request = ping_request();
    let mut result = ResultHeader::abi_error();
    assert_eq!(
        unsafe {
            crate::elephc_dom_call(
                context,
                request.as_ptr(),
                request.len() as u64,
                &mut result,
            )
        },
        STATUS_OK
    );
    assert_ne!(result.result_id, 0);
    assert_eq!(crate::elephc_dom_context_reset(context), STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies the bridge embeds the exact native engine versions frozen by the specification.
#[test]
fn native_engine_versions_match_the_source_lock() {
    let versions = crate::native::versions();
    assert_eq!(versions.libxml_number, 21503);
    assert_eq!(versions.libxml, "2.15.3");
    assert_eq!(versions.lexbor, "2.7.0");
}

/// Verifies valid and malformed XML are distinguished by pinned libxml2.
#[test]
fn pinned_libxml_parses_in_memory_xml() {
    assert!(crate::native::parses_xml(b"<root><child/></root>"));
    assert!(!crate::native::parses_xml(b"<root>"));
}

/// Verifies PHP's bundled Lexbor accepts ordinary and embedded-NUL HTML byte strings.
#[test]
fn pinned_lexbor_parses_length_delimited_html() {
    assert!(crate::native::parses_html(
        b"<!doctype html><title>elephc</title>"
    ));
    assert!(crate::native::parses_html(b"<p>a\0b</p>"));
}

/// Verifies semantic DOM failures use the structured throw channel rather than ABI errors.
#[test]
fn hierarchy_failures_return_structured_dom_exception_results() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 4,
    }];
    let (status, root) = invoke(
        context,
        "method:domdocument::createelement",
        document,
        &value,
        b"root",
    );
    assert_eq!(status, STATUS_OK);
    let root_handle = root.payload0;
    crate::elephc_dom_result_release(context, root.result_id);

    let append = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: root_handle,
        payload1: 0,
    }];
    let (status, appended) = invoke(
        context,
        "method:domnode::appendchild",
        document,
        &append,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, appended.result_id);

    let (status, exception) = invoke(
        context,
        "method:domnode::appendchild",
        root_handle,
        &append,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(exception.status, STATUS_THROW);
    assert_eq!(exception.php_error_kind, PHP_ERROR_KIND_DOM_EXCEPTION);
    assert_eq!(exception.dom_exception_code, 3);
    assert_eq!(exception.payload1, b"Hierarchy Request Error".len() as u64);
    assert_eq!(result_bytes(&exception), b"Hierarchy Request Error");
    crate::elephc_dom_result_release(context, exception.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies native argument-domain failures use the structured `ValueError` channel.
#[test]
fn negative_text_split_returns_a_structured_value_error() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let text_bytes = b"value";
    let text_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: text_bytes.len() as u64,
    }];
    let (status, text) = invoke(
        context,
        "method:domdocument::createtextnode",
        document,
        &text_value,
        text_bytes,
    );
    assert_eq!(status, STATUS_OK);
    let text_handle = text.payload0;
    crate::elephc_dom_result_release(context, text.result_id);

    let offset = [Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: (-1_i64) as u64,
        payload1: 0,
    }];
    let (status, exception) = invoke(
        context,
        "method:domtext::splittext",
        text_handle,
        &offset,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(exception.status, STATUS_THROW);
    assert_eq!(exception.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(exception.dom_exception_code, 0);
    assert_eq!(
        exception.payload1,
        b"DOMText::splitText(): Argument #1 ($offset) must be greater than or equal to 0".len()
            as u64
    );
    assert_eq!(
        result_bytes(&exception),
        b"DOMText::splitText(): Argument #1 ($offset) must be greater than or equal to 0"
    );
    crate::elephc_dom_result_release(context, exception.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies concrete modern readonly handlers use the structured base `Error` channel.
#[test]
fn modern_readonly_node_value_returns_a_structured_error() {
    let context = new_context();
    let (status, constructed) = invoke(
        context,
        "method:dom\\xmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 1,
    }];
    let (status, exception) = invoke(
        context,
        "property-set:dom\\node::$nodeValue",
        document,
        &value,
        b"x",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(exception.status, STATUS_THROW);
    assert_eq!(exception.php_error_kind, PHP_ERROR_KIND_ERROR);
    assert_eq!(exception.dom_exception_code, 0);
    assert_eq!(
        result_bytes(&exception),
        b"Cannot modify readonly property Dom\\XMLDocument::$nodeValue"
    );
    crate::elephc_dom_result_release(context, exception.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern nodes use the document URL or the non-null `about:blank` fallback.
#[test]
fn modern_node_base_uri_falls_back_to_about_blank() {
    let context = new_context();
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let name = [bytes_value(0, 4)];
    let (status, element) = invoke(
        context,
        "method:dom\\document::createelement",
        document,
        &name,
        b"html",
    );
    assert_eq!(status, STATUS_OK);
    let element_handle = element.payload0;
    crate::elephc_dom_result_release(context, element.result_id);

    let child = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: element_handle,
        payload1: 0,
    }];
    let (status, appended) = invoke(
        context,
        "method:dom\\node::appendchild",
        document,
        &child,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, appended.result_id);

    let (status, base_uri) = invoke(
        context,
        "property-get:dom\\node::$baseURI",
        element_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(base_uri.value_tag, VALUE_BYTES);
    assert_eq!(result_bytes(&base_uri), b"about:blank");
    crate::elephc_dom_result_release(context, base_uri.result_id);

    let uri = b"file:///tmp/source.html";
    let uri_value = [bytes_value(0, uri.len())];
    let (status, written) = invoke(
        context,
        "property-set:dom\\document::$documentURI",
        document,
        &uri_value,
        uri,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, written.result_id);
    let (status, base_uri) = invoke(
        context,
        "property-get:dom\\node::$baseURI",
        element_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&base_uri), b"file:///tmp/source.html");
    crate::elephc_dom_result_release(context, base_uri.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies the stateless implementation factory returns a navigable detached doctype handle.
#[test]
fn implementation_factory_returns_detached_document_type() {
    let context = new_context();
    let bytes = b"rootpubsys";
    let values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 4,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 4,
            payload1: 3,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 7,
            payload1: 3,
        },
    ];
    let (status, doctype) = invoke(
        context,
        "method:domimplementation::createdocumenttype",
        0,
        &values,
        bytes,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(doctype.value_tag, VALUE_BRIDGE_HANDLE);
    assert_ne!(doctype.payload0, 0);
    let doctype_handle = doctype.payload0;
    crate::elephc_dom_result_release(context, doctype.result_id);

    let (status, name) = invoke(
        context,
        "property-get:domdocumenttype::$name",
        doctype_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&name), b"root");
    crate::elephc_dom_result_release(context, name.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies legacy document creation attaches a doctype once and preserves wrapper identity.
#[test]
fn legacy_implementation_document_factory_matches_php_ownership() {
    let context = new_context();
    let doctype_values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 4,
    }];
    let (status, doctype) = invoke(
        context,
        "method:domimplementation::createdocumenttype",
        0,
        &doctype_values,
        b"root",
    );
    assert_eq!(status, STATUS_OK);
    let doctype_handle = doctype.payload0;
    crate::elephc_dom_result_release(context, doctype.result_id);

    let document_values = [
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 4,
        },
        Value {
            tag: VALUE_BRIDGE_HANDLE,
            flags: 0,
            payload0: doctype_handle,
            payload1: 0,
        },
    ];
    let (status, created) = invoke(
        context,
        "method:domimplementation::createdocument",
        0,
        &document_values,
        b"root",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let (status, retained_type) = invoke(
        context,
        "property-get:domdocument::$doctype",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(retained_type.payload0, doctype_handle);
    crate::elephc_dom_result_release(context, retained_type.result_id);
    let (status, owner) = invoke(
        context,
        "property-get:domnode::$ownerDocument",
        doctype_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(owner.payload0, document);
    crate::elephc_dom_result_release(context, owner.result_id);

    let (status, serialized) = invoke(
        context,
        "method:domdocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<?xml version=\"1.0\"?>\n<!DOCTYPE root>\n<root/>\n"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, reused) = invoke(
        context,
        "method:domimplementation::createdocument",
        0,
        &document_values,
        b"root",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(reused.status, STATUS_THROW);
    assert_eq!(reused.php_error_kind, PHP_ERROR_KIND_DOM_EXCEPTION);
    assert_eq!(reused.dom_exception_code, 4);
    assert_eq!(result_bytes(&reused), b"Wrong Document Error");
    crate::elephc_dom_result_release(context, reused.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern document creation auto-adopts one doctype between document graphs.
#[test]
fn modern_implementation_document_factory_moves_doctype_identity() {
    let context = new_context();
    let doctype_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 4,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 4,
            payload1: 0,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 4,
            payload1: 0,
        },
    ];
    let (status, doctype) = invoke(
        context,
        "method:dom\\implementation::createdocumenttype",
        0,
        &doctype_values,
        b"root",
    );
    assert_eq!(status, STATUS_OK);
    let doctype_handle = doctype.payload0;
    crate::elephc_dom_result_release(context, doctype.result_id);

    let mut document_values = [
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 5,
        },
        Value {
            tag: VALUE_BRIDGE_HANDLE,
            flags: 0,
            payload0: doctype_handle,
            payload1: 0,
        },
    ];
    let (status, first_result) = invoke(
        context,
        "method:dom\\implementation::createdocument",
        0,
        &document_values,
        b"first",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(first_result.status, STATUS_OK);
    let first = first_result.payload0;
    crate::elephc_dom_result_release(context, first_result.result_id);

    document_values[1].payload1 = 6;
    let (status, second_result) = invoke(
        context,
        "method:dom\\implementation::createdocument",
        0,
        &document_values,
        b"second",
    );
    assert_eq!(status, STATUS_OK);
    let second = second_result.payload0;
    crate::elephc_dom_result_release(context, second_result.result_id);

    let (status, first_type) = invoke(
        context,
        "property-get:dom\\document::$doctype",
        first,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(first_type.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, first_type.result_id);
    let (status, second_type) = invoke(
        context,
        "property-get:dom\\document::$doctype",
        second,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(second_type.payload0, doctype_handle);
    crate::elephc_dom_result_release(context, second_type.result_id);
    let (status, owner) = invoke(
        context,
        "property-get:dom\\node::$ownerDocument",
        doctype_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(owner.payload0, second);
    crate::elephc_dom_result_release(context, owner.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern HTML creation distinguishes an omitted title from an empty title.
#[test]
fn modern_implementation_html_factory_builds_php_tree() {
    let context = new_context();
    let (status, without_title) = invoke(
        context,
        "method:dom\\implementation::createhtmldocument",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let without_title_handle = without_title.payload0;
    crate::elephc_dom_result_release(context, without_title.result_id);
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        without_title_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body></body></html>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let empty_title = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, with_title) = invoke(
        context,
        "method:dom\\implementation::createhtmldocument",
        0,
        &empty_title,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let with_title_handle = with_title.payload0;
    crate::elephc_dom_result_release(context, with_title.result_id);
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        with_title_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title></title></head><body></body></html>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies legacy HTML4 loading and document/subtree serialization match PHP.
#[test]
fn legacy_html_string_parse_and_serialization_match_php() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let source =
        b"<!doctype html><title>T</title><p id=x>A&amp;B<br><svg><path/></svg>";
    let values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, loaded) = invoke(
        context,
        "method:domdocument::loadhtml",
        document,
        &values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(loaded.status, STATUS_OK);
    assert_eq!(loaded.value_tag, VALUE_BOOL);
    assert_eq!(loaded.payload0, 1);
    crate::elephc_dom_result_release(context, loaded.result_id);

    let (status, serialized) = invoke(
        context,
        "method:domdocument::savehtml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<!DOCTYPE html>\n<html><head><title>T</title></head><body><p id=\"x\">A&amp;B<br><svg><path></path></svg></p></body></html>\n"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, root) = invoke(
        context,
        "property-get:domdocument::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let root_handle = root.payload0;
    crate::elephc_dom_result_release(context, root.result_id);
    let root_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: root_handle,
        payload1: 0,
    }];
    let (status, serialized) = invoke(
        context,
        "method:domdocument::savehtml",
        document,
        &root_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<html><head><title>T</title></head><body><p id=\"x\">A&amp;B<br><svg><path></path></svg></p></body></html>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let empty = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, rejected) = invoke(
        context,
        "method:domdocument::loadhtml",
        document,
        &empty,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(rejected.status, STATUS_THROW);
    assert_eq!(rejected.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(
        result_bytes(&rejected),
        b"DOMDocument::loadHTML(): Argument #1 ($source) must not be empty"
    );
    crate::elephc_dom_result_release(context, rejected.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies legacy and modern filesystem routes preserve PHP bytes and exception kinds.
#[test]
fn document_file_round_trips_and_exceptions_match_php() {
    let id = FILE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let prefix = format!("elephc-dom-{}-{id}", std::process::id());
    let directory = std::env::temp_dir();
    let xml_input = directory.join(format!("{prefix}-input.xml"));
    let html_input = directory.join(format!("{prefix}-input.html"));
    let modern_xml_output = directory.join(format!("{prefix}-modern.xml"));
    let modern_html_output = directory.join(format!("{prefix}-modern.html"));
    let modern_html_xml_output =
        directory.join(format!("{prefix}-modern-html.xml"));
    let legacy_xml_output = directory.join(format!("{prefix}-legacy.xml"));
    let legacy_html_output = directory.join(format!("{prefix}-legacy.html"));
    let missing_input = directory.join(format!("{prefix}-missing.xml"));
    std::fs::write(&xml_input, b"<root><empty/></root>")
        .expect("XML fixture must be writable");
    std::fs::write(
        &html_input,
        b"<!doctype html><title>T</title><p>A<br>",
    )
    .expect("HTML fixture must be writable");

    let xml_path = xml_input.to_string_lossy().into_owned().into_bytes();
    let html_path = html_input.to_string_lossy().into_owned().into_bytes();
    let modern_xml_path = modern_xml_output
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let modern_html_path = modern_html_output
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let modern_html_xml_path = modern_html_xml_output
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let legacy_xml_path = legacy_xml_output
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let legacy_html_path = legacy_html_output
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let missing_path = missing_input
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let context = new_context();

    let xml_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: xml_path.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\xmldocument::createfromfile",
        0,
        &xml_value,
        &xml_path,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.status, STATUS_OK);
    let modern_xml = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);
    let save_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: modern_xml_path.len() as u64,
    }];
    let (status, saved) = invoke(
        context,
        "method:dom\\xmldocument::savexmlfile",
        modern_xml,
        &save_value,
        &modern_xml_path,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(saved.status, STATUS_OK);
    let expected_modern_xml =
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root><empty/></root>";
    assert_eq!(saved.payload0, expected_modern_xml.len() as u64);
    crate::elephc_dom_result_release(context, saved.result_id);
    assert_eq!(
        std::fs::read(&modern_xml_output).expect("modern XML output exists"),
        expected_modern_xml
    );

    let html_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: html_path.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createfromfile",
        0,
        &html_value,
        &html_path,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.status, STATUS_OK);
    let modern_html = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);
    for (operation, path, path_bytes, expected) in [
        (
            "method:dom\\htmldocument::savehtmlfile",
            &modern_html_output,
            &modern_html_path,
            b"<!DOCTYPE html><html><head><title>T</title></head><body><p>A<br></p></body></html>"
                .as_slice(),
        ),
        (
            "method:dom\\htmldocument::savexmlfile",
            &modern_html_xml_output,
            &modern_html_xml_path,
            b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body><p>A<br /></p></body></html>"
                .as_slice(),
        ),
    ] {
        let value = [Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: path_bytes.len() as u64,
        }];
        let (status, saved) = invoke(
            context,
            operation,
            modern_html,
            &value,
            path_bytes,
        );
        assert_eq!(status, STATUS_OK, "{operation}");
        assert_eq!(saved.status, STATUS_OK, "{operation}");
        assert_eq!(saved.payload0, expected.len() as u64, "{operation}");
        crate::elephc_dom_result_release(context, saved.result_id);
        assert_eq!(
            std::fs::read(path).expect("modern HTML output exists"),
            expected,
            "{operation}"
        );
    }

    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let legacy = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);
    let (status, loaded) = invoke(
        context,
        "method:domdocument::load",
        legacy,
        &xml_value,
        &xml_path,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(loaded.payload0, 1);
    crate::elephc_dom_result_release(context, loaded.result_id);
    let legacy_save_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: legacy_xml_path.len() as u64,
    }];
    let (status, saved) = invoke(
        context,
        "method:domdocument::save",
        legacy,
        &legacy_save_value,
        &legacy_xml_path,
    );
    assert_eq!(status, STATUS_OK);
    let expected_legacy_xml =
        b"<?xml version=\"1.0\"?>\n<root><empty/></root>\n";
    assert_eq!(saved.payload0, expected_legacy_xml.len() as u64);
    crate::elephc_dom_result_release(context, saved.result_id);
    assert_eq!(
        std::fs::read(&legacy_xml_output).expect("legacy XML output exists"),
        expected_legacy_xml
    );

    let (status, loaded) = invoke(
        context,
        "method:domdocument::loadhtmlfile",
        legacy,
        &html_value,
        &html_path,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(loaded.payload0, 1);
    crate::elephc_dom_result_release(context, loaded.result_id);
    let legacy_html_save_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: legacy_html_path.len() as u64,
    }];
    let (status, saved) = invoke(
        context,
        "method:domdocument::savehtmlfile",
        legacy,
        &legacy_html_save_value,
        &legacy_html_path,
    );
    assert_eq!(status, STATUS_OK);
    let expected_legacy_html =
        b"<!DOCTYPE html>\n<html><head><title>T</title></head><body><p>A<br></p></body></html>\n";
    assert_eq!(saved.payload0, expected_legacy_html.len() as u64);
    crate::elephc_dom_result_release(context, saved.result_id);
    assert_eq!(
        std::fs::read(&legacy_html_output).expect("legacy HTML output exists"),
        expected_legacy_html
    );

    let missing_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: missing_path.len() as u64,
    }];
    let (status, missing) = invoke(
        context,
        "method:dom\\xmldocument::createfromfile",
        0,
        &missing_value,
        &missing_path,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(missing.status, STATUS_THROW);
    assert_eq!(missing.php_error_kind, PHP_ERROR_KIND_EXCEPTION);
    let mut expected_message = b"Cannot open file '".to_vec();
    expected_message.extend_from_slice(&missing_path);
    expected_message.push(b'\'');
    assert_eq!(result_bytes(&missing), expected_message);
    crate::elephc_dom_result_release(context, missing.result_id);
    crate::elephc_dom_context_free(context);

    for path in [
        xml_input,
        html_input,
        modern_xml_output,
        modern_html_output,
        modern_html_xml_output,
        legacy_xml_output,
        legacy_html_output,
    ] {
        let _ = std::fs::remove_file(path);
    }
}

/// Verifies Lexbor HTML5 parsing and HTML serialization retain PHP tree semantics.
#[test]
fn modern_html_string_parse_and_serialization_match_php() {
    let context = new_context();
    let source =
        b"<!doctype html><title>T</title><p id=x>A&nbsp;<br><svg><title>S</title></svg>";
    let values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<!DOCTYPE html><html><head><title>T</title></head><body><p id=\"x\">A&nbsp;<br><svg><title>S</title></svg></p></body></html>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body><p id=\"x\">A\u{a0}<br /><svg xmlns=\"http://www.w3.org/2000/svg\"><title>S</title></svg></p></body></html>".as_bytes()
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, body) = invoke(
        context,
        "property-get:dom\\document::$body",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let body_handle = body.payload0;
    crate::elephc_dom_result_release(context, body.result_id);
    let body_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: body_handle,
        payload1: 0,
    }];
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        document,
        &body_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<body><p id=\"x\">A&nbsp;<br><svg><title>S</title></svg></p></body>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies HTML parser flags and override-encoding errors use PHP's exact contracts.
#[test]
fn modern_html_parse_options_match_php() {
    let context = new_context();
    let invalid_options = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 4,
        },
        Value {
            tag: VALUE_INT,
            flags: 0,
            payload0: 1,
            payload1: 0,
        },
    ];
    let (status, invalid) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &invalid_options,
        b"<p>x",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(invalid.status, STATUS_THROW);
    assert_eq!(invalid.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(
        result_bytes(&invalid),
        b"Dom\\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\\HTML_NO_DEFAULT_NS)"
    );
    crate::elephc_dom_result_release(context, invalid.result_id);

    let invalid_encoding = [
        invalid_options[0],
        Value {
            tag: VALUE_INT,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 4,
            payload1: 3,
        },
    ];
    let (status, invalid) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &invalid_encoding,
        b"<p>xbad",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(invalid.status, STATUS_THROW);
    assert_eq!(invalid.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(
        result_bytes(&invalid),
        b"Dom\\HTMLDocument::createFromString(): Argument #3 ($overrideEncoding) must be a valid document encoding"
    );
    crate::elephc_dom_result_release(context, invalid.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies HTML5 parse errors populate ordered ABI diagnostics with bounded messages.
#[test]
fn modern_html_parse_errors_populate_diagnostics() {
    let context = new_context();
    let source = b"<>x</> <!doctype html>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.status, STATUS_OK);
    let diagnostics = result_diagnostics(&created);
    let bytes = result_bytes(&created);
    assert_eq!(diagnostics.len(), 4);
    let messages = diagnostics
        .iter()
        .map(|diagnostic| {
            let start = diagnostic.message_offset as usize;
            let end = start + diagnostic.message_len as usize;
            &bytes[start..end]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        messages,
        [
            b"Warning: Dom\\HTMLDocument::createFromString(): tokenizer error invalid-first-character-of-tag-name in Entity, line: 1, column: 2\n".as_slice(),
            b"Warning: Dom\\HTMLDocument::createFromString(): tokenizer error missing-end-tag-name in Entity, line: 1, column: 6\n".as_slice(),
            b"Warning: Dom\\HTMLDocument::createFromString(): tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 1-7\n".as_slice(),
            b"Warning: Dom\\HTMLDocument::createFromString(): tree error doctype-token-in-body-mode in Entity, line: 1, column: 10-16\n".as_slice(),
        ]
    );
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.column)
            .collect::<Vec<_>>(),
        [2, 6, 1, 10]
    );
    crate::elephc_dom_result_release(context, created.result_id);

    let quiet_values = [
        source_value[0],
        Value {
            tag: VALUE_INT,
            flags: 0,
            payload0: 32,
            payload1: 0,
        },
    ];
    let (status, quiet) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &quiet_values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(quiet.diagnostics_len, 0);
    crate::elephc_dom_result_release(context, quiet.result_id);

    let mut long_source = vec![b'a'; 20_000];
    long_source.extend_from_slice(b"<>");
    let long_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: long_source.len() as u64,
    }];
    let (status, long_result) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &long_value,
        &long_source,
    );
    assert_eq!(status, STATUS_OK);
    let long_diagnostics = result_diagnostics(&long_result);
    let long_bytes = result_bytes(&long_result);
    let long_messages = long_diagnostics
        .iter()
        .map(|diagnostic| {
            let start = diagnostic.message_offset as usize;
            let end = start + diagnostic.message_len as usize;
            String::from_utf8_lossy(&long_bytes[start..end]).into_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(long_diagnostics.len(), 1, "{long_messages:?}");
    assert_eq!(long_diagnostics[0].line, 1);
    assert_eq!(long_diagnostics[0].column, 20_002);
    crate::elephc_dom_result_release(context, long_result.result_id);

    let mut multi_chunk_source = source.to_vec();
    multi_chunk_source.extend(std::iter::repeat(b'a').take(5_000));
    multi_chunk_source.extend_from_slice(b"<>");
    let multi_chunk_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: multi_chunk_source.len() as u64,
    }];
    let (status, multi_chunk) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &multi_chunk_value,
        &multi_chunk_source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_diagnostics(&multi_chunk)
            .iter()
            .map(|diagnostic| diagnostic.column)
            .collect::<Vec<_>>(),
        [2, 6, 1, 10, 5_024]
    );
    crate::elephc_dom_result_release(context, multi_chunk.result_id);

    let no_implied_source = b"<p>x";
    let no_implied_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: no_implied_source.len() as u64,
        },
        Value {
            tag: VALUE_INT,
            flags: 0,
            payload0: 8_192,
            payload1: 0,
        },
    ];
    let (status, no_implied) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &no_implied_values,
        no_implied_source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(no_implied.diagnostics_len, 0);
    crate::elephc_dom_result_release(context, no_implied.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies libxml2 parse errors render once and obey PHP's `LIBXML_NOERROR` flag.
#[test]
fn xml_parse_errors_populate_filtered_diagnostics() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let source = b"<root>";
    let recovering_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: source.len() as u64,
        },
        Value {
            tag: VALUE_INT,
            flags: 0,
            payload0: 1,
            payload1: 0,
        },
    ];
    let (status, recovered) = invoke(
        context,
        "method:domdocument::loadxml",
        document,
        &recovering_values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(recovered.value_tag, VALUE_BOOL);
    assert_eq!(recovered.payload0, 1);
    let diagnostics = result_diagnostics(&recovered);
    let bytes = result_bytes(&recovered);
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics[0];
    let message_start = diagnostic.message_offset as usize;
    let message_end = message_start + diagnostic.message_len as usize;
    assert_eq!(
        &bytes[message_start..message_end],
        b"Warning: DOMDocument::loadXML(): Premature end of data in tag root line 1 in Entity, line: 1\n"
    );
    assert_eq!(diagnostic.level, 3);
    assert_eq!(diagnostic.code, 77);
    assert_eq!(diagnostic.line, 1);
    assert_eq!(diagnostic.column, 7);
    crate::elephc_dom_result_release(context, recovered.result_id);

    let quiet_values = [
        recovering_values[0],
        Value {
            tag: VALUE_INT,
            flags: 0,
            payload0: 32,
            payload1: 0,
        },
    ];
    let (status, quiet) = invoke(
        context,
        "method:domdocument::loadxml",
        document,
        &quiet_values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(quiet.value_tag, VALUE_BOOL);
    assert_eq!(quiet.payload0, 0);
    assert_eq!(quiet.diagnostics_len, 0);
    crate::elephc_dom_result_release(context, quiet.result_id);

    let test_id = FILE_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "elephc-dom-parser-error-{}-{test_id}.xml",
        std::process::id()
    ));
    std::fs::write(&path, source).expect("write malformed XML fixture");
    let path_string = path.to_string_lossy().into_owned();
    let path_bytes = path_string.as_bytes();
    let file_values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: path_bytes.len() as u64,
    }];
    let (status, file_result) = invoke(
        context,
        "method:domdocument::load",
        document,
        &file_values,
        path_bytes,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(file_result.value_tag, VALUE_BOOL);
    assert_eq!(file_result.payload0, 0);
    let file_diagnostics = result_diagnostics(&file_result);
    let file_bytes = result_bytes(&file_result);
    assert_eq!(file_diagnostics.len(), 1);
    let canonical = std::fs::canonicalize(&path)
        .expect("canonicalize malformed XML fixture");
    let canonical_string = canonical.to_string_lossy();
    let file_diagnostic = file_diagnostics[0];
    let file_start = file_diagnostic.file_offset as usize;
    let file_end = file_start + file_diagnostic.file_len as usize;
    assert_eq!(
        &file_bytes[file_start..file_end],
        canonical_string.as_bytes()
    );
    let message_start = file_diagnostic.message_offset as usize;
    let message_end =
        message_start + file_diagnostic.message_len as usize;
    let mut expected =
        b"Warning: DOMDocument::load(): Premature end of data in tag root line 1 in "
            .to_vec();
    expected.extend_from_slice(canonical_string.as_bytes());
    expected.extend_from_slice(b", line: 1\n");
    assert_eq!(&file_bytes[message_start..message_end], expected);
    crate::elephc_dom_result_release(context, file_result.result_id);
    std::fs::remove_file(&path).expect("remove malformed XML fixture");
    crate::elephc_dom_context_free(context);
}

/// Verifies HTML serialization transcodes text and attributes into the document encoding.
#[test]
fn modern_html_serialization_uses_document_encoding() {
    let context = new_context();
    let source =
        "<!doctype html><p title=\"é € 😀\">é € 😀</p>".as_bytes();
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let encoding = b"windows-1252";
    let encoding_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: encoding.len() as u64,
    }];
    let (status, set) = invoke(
        context,
        "property-set:dom\\document::$characterSet",
        document,
        &encoding_value,
        encoding,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(set.status, STATUS_OK);
    crate::elephc_dom_result_release(context, set.result_id);

    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(serialized.status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<!DOCTYPE html><html><head></head><body><p title=\"\xe9 \x80 ?\">\xe9 \x80 ?</p></body></html>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies nested template fragments remain private while both serializers include them.
#[test]
fn modern_html_template_content_is_private_and_serializable() {
    let context = new_context();
    let source =
        b"<!doctype html><template><p>A</p><template><b>B</b></template></template>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let (status, html) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&html),
        b"<!DOCTYPE html><html><head><template><p>A</p><template><b>B</b></template></template></head><body></body></html>"
    );
    crate::elephc_dom_result_release(context, html.result_id);

    let (status, xml) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&xml),
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><template><p>A</p><template><b>B</b></template></template></head><body></body></html>"
    );
    crate::elephc_dom_result_release(context, xml.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies substituted content and PHP-native template/document cloning through the flat ABI.
#[test]
fn modern_template_graph_copy_operations_match_php() {
    let context = new_context();
    let source =
        b"<!doctype html><template><b>x</b><template><i>n</i></template></template>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let (status, head_result) = invoke(
        context,
        "property-get:dom\\document::$head",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let head = head_result.payload0;
    crate::elephc_dom_result_release(context, head_result.result_id);
    let (status, template_result) = invoke(
        context,
        "property-get:dom\\node::$firstChild",
        head,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let template = template_result.payload0;
    crate::elephc_dom_result_release(context, template_result.result_id);

    let substituted = b"&lt;y&gt;";
    let substituted_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: substituted.len() as u64,
    }];
    let (status, set) = invoke(
        context,
        "property-set:dom\\element::$substitutedNodeValue",
        template,
        &substituted_value,
        substituted,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, set.result_id);
    let (status, value) = invoke(
        context,
        "property-get:dom\\element::$substitutedNodeValue",
        template,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&value), b"<y>");
    crate::elephc_dom_result_release(context, value.result_id);

    let (status, cloned_template_result) = invoke(
        context,
        "internal:bridge.object.clone",
        template,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let cloned_template = cloned_template_result.payload0;
    crate::elephc_dom_result_release(
        context,
        cloned_template_result.result_id,
    );
    let cloned_template_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: cloned_template,
        payload1: 0,
    }];
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        document,
        &cloned_template_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&serialized), b"<template></template>");
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, cloned_document_result) = invoke(
        context,
        "internal:bridge.object.clone",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let cloned_document = cloned_document_result.payload0;
    crate::elephc_dom_result_release(
        context,
        cloned_document_result.result_id,
    );
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        cloned_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<!DOCTYPE html><html><head><template></template></head><body></body></html>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let shallow_value = [Value {
        tag: VALUE_BOOL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, shallow_document_result) = invoke(
        context,
        "method:dom\\node::clonenode",
        document,
        &shallow_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let shallow_document = shallow_document_result.payload0;
    crate::elephc_dom_result_release(
        context,
        shallow_document_result.result_id,
    );
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savehtml",
        shallow_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert!(result_bytes(&serialized).is_empty());
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern encoding aliases canonicalize labels and reject unknown names.
#[test]
fn modern_document_encoding_aliases_match_php() {
    let context = new_context();
    let (status, created) = invoke(
        context,
        "method:dom\\implementation::createhtmldocument",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let latin1 = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 6,
    }];
    let (status, set) = invoke(
        context,
        "property-set:dom\\document::$characterSet",
        document,
        &latin1,
        b"latin1",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(set.status, STATUS_OK);
    crate::elephc_dom_result_release(context, set.result_id);
    for operation in [
        "property-get:dom\\document::$characterSet",
        "property-get:dom\\document::$charset",
        "property-get:dom\\document::$inputEncoding",
    ] {
        let (status, encoding) =
            invoke(context, operation, document, &[], &[]);
        assert_eq!(status, STATUS_OK, "{operation}");
        assert_eq!(result_bytes(&encoding), b"windows-1252", "{operation}");
        crate::elephc_dom_result_release(context, encoding.result_id);
    }

    let invalid = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 7,
    }];
    let (status, rejected) = invoke(
        context,
        "property-set:dom\\document::$charset",
        document,
        &invalid,
        b"invalid",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(rejected.status, STATUS_THROW);
    assert_eq!(rejected.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(result_bytes(&rejected), b"Invalid document encoding");
    crate::elephc_dom_result_release(context, rejected.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern body replacement auto-adopts identity and preserves detached wrappers.
#[test]
fn modern_document_body_replacement_auto_adopts_identity() {
    let context = new_context();
    let (status, source_result) = invoke(
        context,
        "method:dom\\implementation::createhtmldocument",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let source_document = source_result.payload0;
    crate::elephc_dom_result_release(context, source_result.result_id);
    let (status, target_result) = invoke(
        context,
        "method:dom\\implementation::createhtmldocument",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let target_document = target_result.payload0;
    crate::elephc_dom_result_release(context, target_result.result_id);

    let (status, source_body_result) = invoke(
        context,
        "property-get:dom\\document::$body",
        source_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(source_body_result.payload1, 301);
    let source_body = source_body_result.payload0;
    crate::elephc_dom_result_release(context, source_body_result.result_id);
    let (status, old_target_body_result) = invoke(
        context,
        "property-get:dom\\document::$body",
        target_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let old_target_body = old_target_body_result.payload0;
    crate::elephc_dom_result_release(context, old_target_body_result.result_id);

    let replacement = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: source_body,
        payload1: 0,
    }];
    let (status, set) = invoke(
        context,
        "property-set:dom\\document::$body",
        target_document,
        &replacement,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(set.status, STATUS_OK);
    crate::elephc_dom_result_release(context, set.result_id);

    let (status, source_empty) = invoke(
        context,
        "property-get:dom\\document::$body",
        source_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(source_empty.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, source_empty.result_id);
    let (status, target_body) = invoke(
        context,
        "property-get:dom\\document::$body",
        target_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(target_body.payload0, source_body);
    assert_eq!(target_body.payload1, 301);
    crate::elephc_dom_result_release(context, target_body.result_id);
    let (status, owner) = invoke(
        context,
        "property-get:dom\\node::$ownerDocument",
        source_body,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(owner.payload0, target_document);
    crate::elephc_dom_result_release(context, owner.result_id);
    let (status, detached_parent) = invoke(
        context,
        "property-get:dom\\node::$parentNode",
        old_target_body,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(detached_parent.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, detached_parent.result_id);

    let frameset_name = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 8,
    }];
    let (status, frameset_result) = invoke(
        context,
        "method:dom\\document::createelement",
        target_document,
        &frameset_name,
        b"FRAMESET",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(frameset_result.payload1, 301);
    let frameset = frameset_result.payload0;
    crate::elephc_dom_result_release(context, frameset_result.result_id);
    let (status, frameset_node_name) = invoke(
        context,
        "property-get:dom\\node::$nodeName",
        frameset,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&frameset_node_name), b"FRAMESET");
    crate::elephc_dom_result_release(context, frameset_node_name.result_id);
    let frameset_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: frameset,
        payload1: 0,
    }];
    let (status, set_frameset) = invoke(
        context,
        "property-set:dom\\document::$body",
        target_document,
        &frameset_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(set_frameset.status, STATUS_OK);
    crate::elephc_dom_result_release(context, set_frameset.result_id);
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        target_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert!(
        result_bytes(&serialized)
            .windows(b"<frameset></frameset>".len())
            .any(|window| window == b"<frameset></frameset>")
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let null_body = [Value {
        tag: VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, rejected) = invoke(
        context,
        "property-set:dom\\document::$body",
        target_document,
        &null_body,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(rejected.status, STATUS_THROW);
    assert_eq!(rejected.dom_exception_code, 3);
    assert_eq!(
        result_bytes(&rejected),
        b"The new body must either be a body or a frameset tag"
    );
    crate::elephc_dom_result_release(context, rejected.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies title writes preserve source text while reads collapse direct text whitespace.
#[test]
fn modern_document_title_matches_php_child_text_rules() {
    let context = new_context();
    let (status, created) = invoke(
        context,
        "method:dom\\implementation::createhtmldocument",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let source = b"  A \t B\n C  ";
    let value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, set) = invoke(
        context,
        "property-set:dom\\document::$title",
        document,
        &value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(set.status, STATUS_OK);
    crate::elephc_dom_result_release(context, set.result_id);
    let (status, title) = invoke(
        context,
        "property-get:dom\\document::$title",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&title), b"A B C");
    crate::elephc_dom_result_release(context, title.result_id);
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert!(
        result_bytes(&serialized)
            .windows(b"<title>  A \t B\n C  </title>".len())
            .any(|window| window == b"<title>  A \t B\n C  </title>")
    );
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, head) = invoke(
        context,
        "property-get:dom\\document::$head",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(head.payload1, 301);
    let head_handle = head.payload0;
    crate::elephc_dom_result_release(context, head.result_id);
    let (status, title_element) = invoke(
        context,
        "property-get:dom\\node::$firstChild",
        head_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let title_handle = title_element.payload0;
    crate::elephc_dom_result_release(context, title_element.result_id);
    let (status, old_text_result) = invoke(
        context,
        "property-get:dom\\node::$firstChild",
        title_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let old_text = old_text_result.payload0;
    crate::elephc_dom_result_release(context, old_text_result.result_id);

    let empty = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, set_empty) = invoke(
        context,
        "property-set:dom\\document::$title",
        document,
        &empty,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, set_empty.result_id);
    let (status, old_parent) = invoke(
        context,
        "property-get:dom\\node::$parentNode",
        old_text,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(old_parent.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, old_parent.result_id);
    let (status, old_value) = invoke(
        context,
        "property-get:dom\\node::$nodeValue",
        old_text,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&old_value), source);
    crate::elephc_dom_result_release(context, old_value.result_id);
    let (status, children) = invoke(
        context,
        "property-get:dom\\node::$childNodes",
        title_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let children_handle = children.payload0;
    crate::elephc_dom_result_release(context, children.result_id);
    let (status, length) = invoke(
        context,
        "property-get:dom\\nodelist::$length",
        children_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(length.payload0, 1);
    crate::elephc_dom_result_release(context, length.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies a legacy document survives parse, property reads, and serialization by handle.
#[test]
fn legacy_document_round_trips_through_public_bridge_operations() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    assert_eq!(constructed.value_tag, crate::abi::VALUE_BRIDGE_HANDLE);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let source = b"<root><child>value</child></root>";
    let values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, loaded) = invoke(
        context,
        "method:domdocument::loadxml",
        document,
        &values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(loaded.value_tag, crate::abi::VALUE_BOOL);
    assert_eq!(loaded.payload0, 1);
    crate::elephc_dom_result_release(context, loaded.result_id);

    let (status, version) = invoke(
        context,
        "property-get:domdocument::$xmlVersion",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&version), b"1.0");
    crate::elephc_dom_result_release(context, version.result_id);

    let (status, serialized) = invoke(
        context,
        "method:domdocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let output = result_bytes(&serialized);
    assert!(output.starts_with(b"<?xml version=\"1.0\"?>"));
    assert!(output
        .windows(b"<root><child>value</child></root>".len())
        .any(|window| window == b"<root><child>value</child></root>"));
    crate::elephc_dom_result_release(context, serialized.result_id);

    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);
    let (status, _) = invoke(
        context,
        "property-get:domdocument::$xmlVersion",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_ABI_ERROR);
    crate::elephc_dom_context_free(context);
}

/// Verifies legacy prefix writes expose native rebinding, warnings, and forced conflicts.
#[test]
fn legacy_node_prefix_writes_round_trip_through_public_bridge_operations() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let source = b"<p:root xmlns:p=\"urn:x\" xmlns:a=\"urn:y\"/>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, loaded) = invoke(
        context,
        "method:domdocument::loadxml",
        document,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, loaded.result_id);
    let (status, root) = invoke(
        context,
        "property-get:domdocument::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let root_handle = root.payload0;
    crate::elephc_dom_result_release(context, root.result_id);

    let prefix = b"q";
    let prefix_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: prefix.len() as u64,
    }];
    let (status, written) = invoke(
        context,
        "property-set:domnode::$prefix",
        root_handle,
        &prefix_value,
        prefix,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(written.status, STATUS_OK);
    crate::elephc_dom_result_release(context, written.result_id);
    let (status, read) = invoke(
        context,
        "property-get:domnode::$prefix",
        root_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&read), prefix);
    crate::elephc_dom_result_release(context, read.result_id);

    let loose_value = [Value {
        tag: VALUE_BOOL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, configured) = invoke(
        context,
        "property-set:domdocument::$strictErrorChecking",
        document,
        &loose_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, configured.result_id);
    let invalid_prefix = b"xml";
    let invalid_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: invalid_prefix.len() as u64,
    }];
    let (status, warned) = invoke(
        context,
        "property-set:domnode::$prefix",
        root_handle,
        &invalid_value,
        invalid_prefix,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(warned.status, STATUS_OK);
    let diagnostics = result_diagnostics(&warned);
    assert_eq!(diagnostics.len(), 1);
    let bytes = result_bytes(&warned);
    let start = diagnostics[0].message_offset as usize;
    let end = start + diagnostics[0].message_len as usize;
    assert_eq!(&bytes[start..end], b"Warning: Unknown: Namespace Error\n");
    crate::elephc_dom_result_release(context, warned.result_id);

    let conflicting_prefix = b"a";
    let conflicting_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: conflicting_prefix.len() as u64,
    }];
    let (status, rejected) = invoke(
        context,
        "property-set:domnode::$prefix",
        root_handle,
        &conflicting_value,
        conflicting_prefix,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(rejected.status, STATUS_THROW);
    assert_eq!(rejected.php_error_kind, PHP_ERROR_KIND_DOM_EXCEPTION);
    assert_eq!(rejected.dom_exception_code, 14);
    assert_eq!(result_bytes(&rejected), b"Namespace Error");
    crate::elephc_dom_result_release(context, rejected.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies adjacent element and text mutations consume the flat public bridge values.
#[test]
fn adjacent_mutations_round_trip_through_public_bridge_operations() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let source = b"<root><a/></root>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, loaded) = invoke(
        context,
        "method:domdocument::loadxml",
        document,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, loaded.result_id);

    let (status, root) = invoke(
        context,
        "property-get:domdocument::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let root_handle = root.payload0;
    crate::elephc_dom_result_release(context, root.result_id);
    let (status, first) = invoke(
        context,
        "property-get:domelement::$firstElementChild",
        root_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let first_handle = first.payload0;
    crate::elephc_dom_result_release(context, first.result_id);

    let name = b"x";
    let name_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: name.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:domdocument::createelement",
        document,
        &name_value,
        name,
    );
    assert_eq!(status, STATUS_OK);
    let inserted_handle = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let position = b"beforebegin";
    let adjacent_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: position.len() as u64,
        },
        Value {
            tag: VALUE_BRIDGE_HANDLE,
            flags: 0,
            payload0: inserted_handle,
            payload1: 0,
        },
    ];
    let (status, inserted) = invoke(
        context,
        "method:domelement::insertadjacentelement",
        first_handle,
        &adjacent_values,
        position,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(inserted.payload0, inserted_handle);
    crate::elephc_dom_result_release(context, inserted.result_id);

    let text_position = b"afterbegin";
    let text = b"T&";
    let mut text_bytes = Vec::from(text_position.as_slice());
    text_bytes.extend_from_slice(text);
    let text_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: text_position.len() as u64,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: text_position.len() as u64,
            payload1: text.len() as u64,
        },
    ];
    let (status, inserted) = invoke(
        context,
        "method:domelement::insertadjacenttext",
        first_handle,
        &text_values,
        &text_bytes,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(inserted.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, inserted.result_id);

    let root_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: root_handle,
        payload1: 0,
    }];
    let (status, serialized) = invoke(
        context,
        "method:domdocument::savexml",
        document,
        &root_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&serialized), b"<root><x/><a>T&amp;</a></root>");
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern rename preserves identity and serializes colliding prefixes like PHP.
#[test]
fn modern_node_rename_round_trips_through_public_bridge_operations() {
    let context = new_context();
    let source = b"<a:root xmlns:a=\"urn:old\"/>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\xmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let (status, root) = invoke(
        context,
        "property-get:dom\\document::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let root_handle = root.payload0;
    crate::elephc_dom_result_release(context, root.result_id);

    let namespace = b"urn:new";
    let qualified_name = b"a:renamed";
    let mut rename_bytes = Vec::from(namespace.as_slice());
    rename_bytes.extend_from_slice(qualified_name);
    let rename_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: namespace.len() as u64,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: namespace.len() as u64,
            payload1: qualified_name.len() as u64,
        },
    ];
    let (status, renamed) = invoke(
        context,
        "method:dom\\element::rename",
        root_handle,
        &rename_values,
        &rename_bytes,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(renamed.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, renamed.result_id);

    let (status, name) = invoke(
        context,
        "property-get:dom\\node::$nodeName",
        root_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&name), qualified_name);
    crate::elephc_dom_result_release(context, name.result_id);

    let (status, serialized) = invoke(
        context,
        "method:dom\\xmldocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ns1:renamed xmlns:ns1=\"urn:new\" xmlns:a=\"urn:old\"/>"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern namespace-info arrays preserve php-src order and element identity.
#[test]
fn modern_namespace_info_round_trips_through_public_bridge_operations() {
    let context = new_context();
    let source = b"<r xmlns:a=\"urn:u1\" xmlns:b=\"urn:u2\"><c xmlns:c=\"urn:u3\" xmlns:a=\"urn:u4\"><g xmlns=\"\"/></c></r>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\xmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let (status, root) = invoke(
        context,
        "property-get:dom\\document::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let root_handle = root.payload0;
    crate::elephc_dom_result_release(context, root.result_id);
    let (status, child) = invoke(
        context,
        "property-get:dom\\element::$firstElementChild",
        root_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let child_handle = child.payload0;
    crate::elephc_dom_result_release(context, child.result_id);
    let (status, grandchild) = invoke(
        context,
        "property-get:dom\\element::$firstElementChild",
        child_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let grandchild_handle = grandchild.payload0;
    crate::elephc_dom_result_release(context, grandchild.result_id);

    let (status, in_scope) = invoke(
        context,
        "method:dom\\element::getinscopenamespaces",
        child_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(in_scope.value_tag, VALUE_ARRAY);
    assert_eq!(in_scope.payload1, 3);
    let values = result_values(&in_scope);
    let bytes = result_bytes(&in_scope);
    let fields = values
        .iter()
        .take(3)
        .map(|value| {
            assert_eq!(value.tag, VALUE_OBJECT);
            assert_eq!(
                value.flags,
                crate::objects::VALUE_OBJECT_NAMESPACE_INFO
            );
            let start = value.payload0 as usize;
            let end = start + value.payload1 as usize;
            &values[start..end]
        })
        .collect::<Vec<_>>();
    let copied = fields
        .iter()
        .map(|fields| {
            let prefix = if fields[0].tag == VALUE_NULL {
                None
            } else {
                let start = fields[0].payload0 as usize;
                let end = start + fields[0].payload1 as usize;
                Some(bytes[start..end].to_vec())
            };
            let uri = if fields[1].tag == VALUE_NULL {
                None
            } else {
                let start = fields[1].payload0 as usize;
                let end = start + fields[1].payload1 as usize;
                Some(bytes[start..end].to_vec())
            };
            assert_eq!(fields[2].tag, VALUE_BRIDGE_HANDLE);
            assert_eq!(fields[2].payload0, child_handle);
            (prefix, uri)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        copied,
        vec![
            (Some(b"b".to_vec()), Some(b"urn:u2".to_vec())),
            (Some(b"c".to_vec()), Some(b"urn:u3".to_vec())),
            (Some(b"a".to_vec()), Some(b"urn:u4".to_vec())),
        ]
    );
    crate::elephc_dom_result_release(context, in_scope.result_id);

    let (status, descendants) = invoke(
        context,
        "method:dom\\element::getdescendantnamespaces",
        root_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(descendants.value_tag, VALUE_ARRAY);
    assert_eq!(descendants.payload1, 8);
    let values = result_values(&descendants);
    let element_handles = values
        .iter()
        .take(8)
        .map(|value| values[value.payload0 as usize + 2].payload0)
        .collect::<Vec<_>>();
    assert_eq!(
        element_handles,
        vec![
            root_handle,
            root_handle,
            child_handle,
            child_handle,
            child_handle,
            grandchild_handle,
            grandchild_handle,
            grandchild_handle,
        ]
    );
    crate::elephc_dom_result_release(context, descendants.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies canonical node handles preserve tree identity, navigation, and content.
#[test]
fn legacy_document_tree_operations_preserve_native_identity() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let element_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 4,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 4,
            payload1: 5,
        },
    ];
    let (status, element) = invoke(
        context,
        "method:domdocument::createelement",
        document,
        &element_values,
        b"rootvalue",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(element.value_tag, VALUE_BRIDGE_HANDLE);
    let element_handle = element.payload0;
    crate::elephc_dom_result_release(context, element.result_id);

    let text_values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 4,
    }];
    let (status, text) = invoke(
        context,
        "method:domdocument::createtextnode",
        document,
        &text_values,
        b"tail",
    );
    assert_eq!(status, STATUS_OK);
    let text_handle = text.payload0;
    crate::elephc_dom_result_release(context, text.result_id);

    let append_element = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: element_handle,
        payload1: 0,
    }];
    let (status, appended) = invoke(
        context,
        "method:domnode::appendchild",
        document,
        &append_element,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(appended.payload0, element_handle);
    crate::elephc_dom_result_release(context, appended.result_id);

    let (status, root) = invoke(
        context,
        "property-get:domdocument::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(root.payload0, element_handle);
    crate::elephc_dom_result_release(context, root.result_id);

    let append_text = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: text_handle,
        payload1: 0,
    }];
    let (status, appended) = invoke(
        context,
        "method:domnode::appendchild",
        element_handle,
        &append_text,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(appended.payload0, text_handle);
    crate::elephc_dom_result_release(context, appended.result_id);

    let (status, name) = invoke(
        context,
        "property-get:domnode::$nodeName",
        element_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&name), b"root");
    crate::elephc_dom_result_release(context, name.result_id);

    let (status, node_type) = invoke(
        context,
        "property-get:domnode::$nodeType",
        text_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(node_type.payload0, 3);
    crate::elephc_dom_result_release(context, node_type.result_id);

    let (status, content) = invoke(
        context,
        "property-get:domnode::$textContent",
        element_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&content), b"valuetail");
    crate::elephc_dom_result_release(context, content.result_id);

    let (status, parent) = invoke(
        context,
        "property-get:domnode::$parentNode",
        text_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(parent.payload0, element_handle);
    crate::elephc_dom_result_release(context, parent.result_id);

    let (status, owner) = invoke(
        context,
        "property-get:domnode::$ownerDocument",
        text_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(owner.payload0, document);
    crate::elephc_dom_result_release(context, owner.result_id);

    let (status, serialized) = invoke(
        context,
        "method:domdocument::savexml",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert!(result_bytes(&serialized)
        .windows(b"<root>valuetail</root>".len())
        .any(|window| window == b"<root>valuetail</root>"));
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies every core document node factory publishes the matching libxml node kind.
#[test]
fn document_node_factories_publish_php_node_kinds() {
    let context = new_context();
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let one_string = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 1,
    }];
    let two_strings = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 2,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 2,
            payload1: 1,
        },
    ];
    let cases = [
        (
            "method:domdocument::createcdatasection",
            one_string.as_slice(),
            b"a".as_slice(),
            4,
            b"#cdata-section".as_slice(),
        ),
        (
            "method:domdocument::createcomment",
            one_string.as_slice(),
            b"b".as_slice(),
            8,
            b"#comment".as_slice(),
        ),
        (
            "method:domdocument::createdocumentfragment",
            &[] as &[Value],
            &[] as &[u8],
            11,
            b"#document-fragment".as_slice(),
        ),
        (
            "method:domdocument::createprocessinginstruction",
            two_strings.as_slice(),
            b"pic".as_slice(),
            7,
            b"pi".as_slice(),
        ),
        (
            "method:domdocument::createentityreference",
            one_string.as_slice(),
            b"e".as_slice(),
            5,
            b"e".as_slice(),
        ),
    ];

    for (operation, values, bytes, expected_type, expected_name) in cases {
        let (status, created) = invoke(context, operation, document, values, bytes);
        assert_eq!(status, STATUS_OK, "{operation}");
        assert_eq!(created.value_tag, VALUE_BRIDGE_HANDLE, "{operation}");
        let handle = created.payload0;
        crate::elephc_dom_result_release(context, created.result_id);

        let (status, node_type) = invoke(
            context,
            "property-get:domnode::$nodeType",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK, "{operation}");
        assert_eq!(node_type.payload0, expected_type, "{operation}");
        crate::elephc_dom_result_release(context, node_type.result_id);

        let (status, node_name) = invoke(
            context,
            "property-get:domnode::$nodeName",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK, "{operation}");
        assert_eq!(result_bytes(&node_name), expected_name, "{operation}");
        crate::elephc_dom_result_release(context, node_name.result_id);
    }

    crate::elephc_dom_context_free(context);
}

/// Verifies the compiler-planned explicit legacy constructor defaults match the flat ABI.
#[test]
fn legacy_document_accepts_explicit_default_constructor_values() {
    let context = new_context();
    let values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 3,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 3,
            payload1: 0,
        },
    ];
    let (status, constructed) = invoke(
        context,
        "method:domdocument::__construct",
        0,
        &values,
        b"1.0",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(constructed.value_tag, crate::abi::VALUE_BRIDGE_HANDLE);
    crate::elephc_dom_result_release(context, constructed.result_id);
    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        constructed.payload0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies manual legacy constructors keep bridge handles while replacing native resources.
#[test]
fn legacy_manual_constructors_rebind_existing_handles() {
    let context = new_context();
    let initial_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 1,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 1,
            payload1: 3,
        },
    ];
    let (status, constructed) = invoke(
        context,
        "method:domelement::__construct",
        0,
        &initial_values,
        b"aold",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(constructed.value_tag, VALUE_BRIDGE_HANDLE);
    let element = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let replacement_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 1,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 1,
            payload1: 3,
        },
    ];
    let (status, replaced) = invoke(
        context,
        "method:domelement::__construct",
        element,
        &replacement_values,
        b"bnew",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(replaced.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, replaced.result_id);

    let (status, name) = invoke(
        context,
        "property-get:domnode::$nodeName",
        element,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&name), b"b");
    crate::elephc_dom_result_release(context, name.result_id);
    let (status, content) = invoke(
        context,
        "property-get:domnode::$textContent",
        element,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&content), b"new");
    crate::elephc_dom_result_release(context, content.result_id);

    let (status, constructed_fragment) = invoke(
        context,
        "method:domdocumentfragment::__construct",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let fragment = constructed_fragment.payload0;
    crate::elephc_dom_result_release(context, constructed_fragment.result_id);
    let (status, reconstructed_fragment) = invoke(
        context,
        "method:domdocumentfragment::__construct",
        fragment,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(reconstructed_fragment.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, reconstructed_fragment.result_id);
    let fragment_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: fragment,
        payload1: 0,
    }];
    let (status, empty_append) = invoke(
        context,
        "method:domnode::appendchild",
        element,
        &fragment_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(empty_append.value_tag, VALUE_BOOL);
    assert_eq!(empty_append.payload0, 0);
    assert_eq!(empty_append.diagnostics_len, 1);
    assert_eq!(
        result_bytes(&empty_append),
        b"Warning: DOMNode::appendChild(): Document Fragment is empty"
    );
    crate::elephc_dom_result_release(context, empty_append.result_id);

    let (status, constructed_document) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document = constructed_document.payload0;
    crate::elephc_dom_result_release(context, constructed_document.result_id);
    let document_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: 3,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 3,
            payload1: 5,
        },
    ];
    let (status, reconstructed) = invoke(
        context,
        "method:domdocument::__construct",
        document,
        &document_values,
        b"1.1UTF-8",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(reconstructed.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, reconstructed.result_id);
    let (status, version) = invoke(
        context,
        "property-get:domdocument::$xmlVersion",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&version), b"1.1");
    crate::elephc_dom_result_release(context, version.result_id);

    crate::elephc_dom_context_free(context);
}

/// Verifies modern XML/HTML empty factories retain PHP's frozen default metadata.
#[test]
fn modern_empty_document_factories_use_php_defaults() {
    let context = new_context();
    let (status, xml) = invoke(
        context,
        "method:dom\\xmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let xml_document = xml.payload0;
    crate::elephc_dom_result_release(context, xml.result_id);
    let (status, encoding) = invoke(
        context,
        "property-get:dom\\xmldocument::$xmlEncoding",
        xml_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&encoding), b"UTF-8");
    crate::elephc_dom_result_release(context, encoding.result_id);

    let (status, html) = invoke(
        context,
        "method:dom\\htmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let html_document = html.payload0;
    crate::elephc_dom_result_release(context, html.result_id);
    let (status, serialized) = invoke(
        context,
        "method:dom\\htmldocument::savexml",
        html_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(
        result_bytes(&serialized),
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n"
    );
    crate::elephc_dom_result_release(context, serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies libxml error-mode queries, toggles, empty reads, and clearing use context-local state.
#[test]
fn libxml_error_mode_and_empty_result_state_match_php() {
    let context = new_context();
    let null = [Value {
        tag: crate::abi::VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let enabled = [Value {
        tag: crate::abi::VALUE_BOOL,
        flags: 0,
        payload0: 1,
        payload1: 0,
    }];

    let (status, queried) = invoke(
        context,
        "function:libxml_use_internal_errors",
        0,
        &null,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(queried.value_tag, crate::abi::VALUE_BOOL);
    assert_eq!(queried.payload0, 0);
    crate::elephc_dom_result_release(context, queried.result_id);

    let (status, previous) = invoke(
        context,
        "function:libxml_use_internal_errors",
        0,
        &enabled,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(previous.payload0, 0);
    crate::elephc_dom_result_release(context, previous.result_id);

    let (status, errors) = invoke(
        context,
        "function:libxml_get_errors",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(errors.value_tag, crate::abi::VALUE_ARRAY);
    assert_eq!(errors.values_len, 0);
    crate::elephc_dom_result_release(context, errors.result_id);

    let (status, last) = invoke(
        context,
        "function:libxml_get_last_error",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(last.value_tag, crate::abi::VALUE_BOOL);
    assert_eq!(last.payload0, 0);
    crate::elephc_dom_result_release(context, last.result_id);

    let (status, cleared) = invoke(
        context,
        "function:libxml_clear_errors",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(cleared.value_tag, crate::abi::VALUE_NULL);
    crate::elephc_dom_result_release(context, cleared.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies external-loader retain/get/clear and stream-context overwrite state.
#[test]
fn libxml_host_values_have_balanced_context_ownership() {
    let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
    HOST_RETAINS.store(0, Ordering::Relaxed);
    HOST_RELEASES.store(0, Ordering::Relaxed);
    HOST_THROW_OPCODE.store(0, Ordering::Relaxed);
    let context = new_context_with_host(Some(accepting_host_call));

    let (status, empty) = invoke(
        context,
        "function:libxml_get_external_entity_loader",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(empty.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, empty.result_id);

    let callable = [Value {
        tag: crate::abi::VALUE_CALLABLE,
        flags: 0,
        payload0: 0x1234,
        payload1: 0,
    }];
    let (status, set) = invoke(
        context,
        "function:libxml_set_external_entity_loader",
        0,
        &callable,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(set.value_tag, crate::abi::VALUE_BOOL);
    assert_eq!(set.payload0, 1);
    assert_eq!(HOST_RETAINS.load(Ordering::Relaxed), 1);
    crate::elephc_dom_result_release(context, set.result_id);

    let (status, fetched) = invoke(
        context,
        "function:libxml_get_external_entity_loader",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(fetched.value_tag, crate::abi::VALUE_CALLABLE);
    assert_eq!(fetched.payload0, 0x1234);
    assert_eq!(HOST_RETAINS.load(Ordering::Relaxed), 2);
    crate::elephc_dom_result_release(context, fetched.result_id);

    let null = [Value {
        tag: VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, cleared) = invoke(
        context,
        "function:libxml_set_external_entity_loader",
        0,
        &null,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(HOST_RELEASES.load(Ordering::Relaxed), 1);
    crate::elephc_dom_result_release(context, cleared.result_id);

    let stream_context = [Value {
        tag: crate::abi::VALUE_RESOURCE,
        flags: 0,
        payload0: 77,
        payload1: 0,
    }];
    let (status, stored) = invoke(
        context,
        "function:libxml_set_streams_context",
        0,
        &stream_context,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(stored.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, stored.result_id);
    assert_eq!(
        crate::context::context(context)
            .expect("context remains registered")
            .borrow()
            .stream_context,
        Some(77)
    );

    crate::elephc_dom_context_free(context);
    assert_eq!(HOST_RELEASES.load(Ordering::Relaxed), 1);
}

/// Verifies malformed XML populates detached `LibXMLError` value-object descriptors.
#[test]
fn malformed_xml_populates_libxml_error_value_objects() {
    let context = new_context();
    let enabled = [Value {
        tag: crate::abi::VALUE_BOOL,
        flags: 0,
        payload0: 1,
        payload1: 0,
    }];
    let (status, previous) = invoke(
        context,
        "function:libxml_use_internal_errors",
        0,
        &enabled,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, previous.result_id);

    let (status, document) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let document_handle = document.payload0;
    crate::elephc_dom_result_release(context, document.result_id);

    let source = b"<root>";
    let load_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: source.len() as u64,
        },
        Value {
            tag: crate::abi::VALUE_INT,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
    ];
    let (status, loaded) = invoke(
        context,
        "method:domdocument::loadxml",
        document_handle,
        &load_values,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(loaded.value_tag, crate::abi::VALUE_BOOL);
    assert_eq!(loaded.payload0, 0);
    crate::elephc_dom_result_release(context, loaded.result_id);

    let (status, errors) = invoke(
        context,
        "function:libxml_get_errors",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(errors.value_tag, crate::abi::VALUE_ARRAY);
    assert_eq!(errors.payload1, 1);
    assert_eq!(errors.values_len, 7);
    let values = result_values(&errors);
    let bytes = result_bytes(&errors);
    let error_value = values[0];
    assert_eq!(error_value.tag, crate::abi::VALUE_OBJECT);
    assert_eq!(
        error_value.flags,
        crate::objects::VALUE_OBJECT_LIBXML_ERROR
    );
    assert_eq!(error_value.payload0, 1);
    assert_eq!(error_value.payload1, 6);
    assert_eq!(values[1].tag, crate::abi::VALUE_INT);
    assert_eq!(values[1].payload0, 3);
    assert_eq!(values[2].tag, crate::abi::VALUE_INT);
    assert_eq!(values[2].payload0, 77);
    assert_eq!(values[3].tag, crate::abi::VALUE_INT);
    assert_eq!(values[3].payload0, 7);
    assert_eq!(values[4].tag, crate::abi::VALUE_BYTES);
    assert_eq!(
        &bytes[values[4].payload0 as usize
            ..(values[4].payload0 + values[4].payload1) as usize],
        b"Premature end of data in tag root line 1\n"
    );
    assert_eq!(values[5].tag, crate::abi::VALUE_BYTES);
    assert_eq!(values[5].payload1, 0);
    assert_eq!(values[6].tag, crate::abi::VALUE_INT);
    assert_eq!(values[6].payload0, 1);
    crate::elephc_dom_result_release(context, errors.result_id);

    let (status, last) = invoke(
        context,
        "function:libxml_get_last_error",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(last.value_tag, crate::abi::VALUE_OBJECT);
    assert_eq!(last.payload0, 0);
    assert_eq!(last.payload1, 6);
    let last_values = result_values(&last);
    let last_bytes = result_bytes(&last);
    assert_eq!(last_values[1].payload0, 77);
    assert_eq!(
        &last_bytes[last_values[3].payload0 as usize
            ..(last_values[3].payload0 + last_values[3].payload1) as usize],
        b"Premature end of data in tag root line 1\n"
    );
    crate::elephc_dom_result_release(context, last.result_id);

    let (status, released_document) = invoke(
        context,
        "internal:bridge.wrapper.release",
        document_handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released_document.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies both fragment XML routes preserve php-src parsing, errors, and binding rules.
#[test]
fn document_fragment_append_xml_matches_php_balanced_chunk_semantics() {
    let context = new_context();
    let (status, standalone) = invoke(
        context,
        "method:domdocumentfragment::__construct",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let standalone_fragment = standalone.payload0;
    crate::elephc_dom_result_release(context, standalone.result_id);
    let source = b"<foo id=\"baz\">bar</foo><tail/>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, unbound) = invoke(
        context,
        "method:domdocumentfragment::appendxml",
        standalone_fragment,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(unbound.status, STATUS_THROW);
    assert_eq!(unbound.php_error_kind, PHP_ERROR_KIND_DOM_EXCEPTION);
    assert_eq!(unbound.dom_exception_code, 7);
    assert_eq!(result_bytes(&unbound), b"No Modification Allowed Error");
    crate::elephc_dom_result_release(context, unbound.result_id);

    let (status, legacy) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    let legacy_document = legacy.payload0;
    crate::elephc_dom_result_release(context, legacy.result_id);
    let element_name = b"not-a-fragment";
    let element_name_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: element_name.len() as u64,
    }];
    let (status, element_result) = invoke(
        context,
        "method:domdocument::createelement",
        legacy_document,
        &element_name_value,
        element_name,
    );
    assert_eq!(status, STATUS_OK);
    let element = element_result.payload0;
    crate::elephc_dom_result_release(context, element_result.result_id);
    let (wrong_receiver_status, _) = invoke(
        context,
        "method:domdocumentfragment::appendxml",
        element,
        &source_value,
        source,
    );
    assert_eq!(wrong_receiver_status, STATUS_ABI_ERROR);
    let (status, fragment) = invoke(
        context,
        "method:domdocument::createdocumentfragment",
        legacy_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let legacy_fragment = fragment.payload0;
    crate::elephc_dom_result_release(context, fragment.result_id);
    let (status, appended) = invoke(
        context,
        "method:domdocumentfragment::appendxml",
        legacy_fragment,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(appended.value_tag, VALUE_BOOL);
    assert_eq!(appended.payload0, 1);
    assert_eq!(appended.diagnostics_len, 0);
    crate::elephc_dom_result_release(context, appended.result_id);

    let fragment_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: legacy_fragment,
        payload1: 0,
    }];
    let (status, serialized) = invoke(
        context,
        "method:domdocument::savexml",
        legacy_document,
        &fragment_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&serialized), source);
    crate::elephc_dom_result_release(context, serialized.result_id);

    let enabled = [Value {
        tag: VALUE_BOOL,
        flags: 0,
        payload0: 1,
        payload1: 0,
    }];
    let (status, previous) = invoke(
        context,
        "function:libxml_use_internal_errors",
        0,
        &enabled,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, previous.result_id);
    let (status, malformed_fragment_result) = invoke(
        context,
        "method:domdocument::createdocumentfragment",
        legacy_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let malformed_fragment = malformed_fragment_result.payload0;
    crate::elephc_dom_result_release(
        context,
        malformed_fragment_result.result_id,
    );
    let malformed = b"<foo>is<bar>great</foo>";
    let malformed_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: malformed.len() as u64,
    }];
    let (status, rejected) = invoke(
        context,
        "method:domdocumentfragment::appendxml",
        malformed_fragment,
        &malformed_value,
        malformed,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(rejected.value_tag, VALUE_BOOL);
    assert_eq!(rejected.payload0, 0);
    assert_eq!(rejected.diagnostics_len, 0);
    crate::elephc_dom_result_release(context, rejected.result_id);
    let retained_errors = crate::context::context(context)
        .expect("context remains registered")
        .borrow()
        .errors
        .clone();
    assert_eq!(
        retained_errors
            .iter()
            .map(|error| error.code)
            .collect::<Vec<_>>(),
        [76]
    );
    assert!(retained_errors
        .iter()
        .all(|error| error.line == 1 && error.column == 24));

    let empty = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, empty_result) = invoke(
        context,
        "method:domdocumentfragment::appendxml",
        malformed_fragment,
        &empty,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(empty_result.payload0, 1);
    crate::elephc_dom_result_release(context, empty_result.result_id);

    let (status, modern) = invoke(
        context,
        "method:dom\\xmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let modern_document = modern.payload0;
    crate::elephc_dom_result_release(context, modern.result_id);
    let (status, modern_fragment_result) = invoke(
        context,
        "method:dom\\document::createdocumentfragment",
        modern_document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let modern_fragment = modern_fragment_result.payload0;
    crate::elephc_dom_result_release(
        context,
        modern_fragment_result.result_id,
    );
    let nul_source = b"<modern/>\0<ignored/>";
    let nul_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: nul_source.len() as u64,
    }];
    let (status, modern_appended) = invoke(
        context,
        "method:dom\\documentfragment::appendxml",
        modern_fragment,
        &nul_value,
        nul_source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(modern_appended.value_tag, VALUE_BOOL);
    assert_eq!(modern_appended.payload0, 1);
    crate::elephc_dom_result_release(context, modern_appended.result_id);
    let modern_fragment_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: modern_fragment,
        payload1: 0,
    }];
    let (status, modern_serialized) = invoke(
        context,
        "method:dom\\xmldocument::savexml",
        modern_document,
        &modern_fragment_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result_bytes(&modern_serialized), b"<modern/>");
    crate::elephc_dom_result_release(context, modern_serialized.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies pinned libxml validates DTD, XSD, Relax NG, and modern QName values.
#[test]
fn native_document_validation_matches_php_and_modern_namespace_rules() {
    let dtd_source = br#"<!DOCTYPE root [
<!ELEMENT root (child)>
<!ELEMENT child EMPTY>
]><root/>"#;
    let dtd = crate::native::document_parse_xml(dtd_source, 0, None, None)
        .expect("DTD document parses");
    let dtd_document = dtd.document.expect("DTD document exists");
    let dtd_result =
        crate::native::document_validate(dtd_document).expect("DTD validation runs");
    assert!(!dtd_result.valid);
    assert_eq!(dtd_result.status, 0);
    assert_eq!(
        dtd_result
            .errors
            .iter()
            .map(|error| error.code)
            .collect::<Vec<_>>(),
        [504]
    );
    unsafe {
        crate::native::document_free(dtd_document);
    }

    let xml = br#"<root xmlns="urn:test" xmlns:ref="urn:other">
  <item target="ref:something"/>
</root>"#;
    let xsd = br#"<schema xmlns="http://www.w3.org/2001/XMLSchema"
        targetNamespace="urn:test" elementFormDefault="qualified">
  <element name="root">
    <complexType>
      <sequence>
        <element name="item">
          <complexType>
            <attribute name="target" type="QName"/>
          </complexType>
        </element>
      </sequence>
    </complexType>
  </element>
</schema>"#;
    let modern = crate::native::document_parse_xml(xml, 0, None, None)
        .expect("modern source parses");
    let modern_document = modern.document.expect("modern document exists");
    assert!(crate::native::document_convert_modern_xml(modern_document));
    let schema_result =
        crate::native::document_schema_validate_source_with_host(
            modern_document,
            xsd,
            0,
            false,
            0,
        )
        .expect("schema validation runs");
    assert!(schema_result.valid, "{:?}", schema_result.errors.len());
    assert!(schema_result.errors.is_empty());
    let malformed_schema =
        crate::native::document_schema_validate_source_with_host(
            modern_document,
            b"string that is not a schema",
            0,
            true,
            0,
        )
        .expect("malformed schema validation runs");
    assert_eq!(
        malformed_schema
            .errors
            .iter()
            .map(|error| error.message.as_slice())
            .collect::<Vec<_>>(),
        [
            b"Entity: line 1: parser error : Start tag expected, '<' not found"
                .as_slice(),
            b"string that is not a schema".as_slice(),
            b"^".as_slice(),
            b"Failed to parse the XML resource 'in_memory_buffer'.".as_slice(),
        ]
    );
    unsafe {
        crate::native::document_free(modern_document);
    }

    let relax_document = crate::native::document_parse_xml(
        b"<root><child/></root>",
        0,
        None,
        None,
    )
    .expect("Relax NG document parses")
    .document
    .expect("Relax NG document exists");
    let relax = br#"<element name="root"
        xmlns="http://relaxng.org/ns/structure/1.0">
  <element name="child"><empty/></element>
</element>"#;
    let relax_result =
        crate::native::document_relaxng_validate_source_with_host(
            relax_document,
            relax,
            false,
            0,
        )
        .expect("Relax NG validation runs");
    assert!(relax_result.valid);
    assert!(relax_result.errors.is_empty());
    unsafe {
        crate::native::document_free(relax_document);
    }
}

/// Verifies pinned libxml XInclude returns substitutions and destroyed pointers.
#[test]
fn native_document_xinclude_reports_destroyed_subtrees() {
    let parsed = crate::native::document_parse_xml(
        br#"<root xmlns:xi="http://www.w3.org/2001/XInclude">
  <xi:include href="missing.xml">
    <xi:fallback><included attr="value">text</included></xi:fallback>
  </xi:include>
</root>"#,
        0,
        None,
        None,
    )
    .expect("XInclude fixture parses");
    let document = parsed.document.expect("XInclude document exists");
    let outcome = crate::native::document_xinclude(document, 0, false, 0);
    assert!(!outcome.allocation_failed);
    assert_eq!(outcome.substitutions, 1);
    assert!(outcome.invalidated.len() >= 6);
    assert!(!outcome.errors.is_empty());
    unsafe {
        crate::native::document_free(document);
    }
}

/// Verifies a throwing host release is returned as the original pending Throwable signal.
#[test]
fn libxml_host_release_throw_preserves_new_loader_state() {
    let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
    HOST_RETAINS.store(0, Ordering::Relaxed);
    HOST_RELEASES.store(0, Ordering::Relaxed);
    HOST_THROW_OPCODE.store(0, Ordering::Relaxed);
    HOST_REENTRANT_CONTEXT.store(0, Ordering::Relaxed);
    let context = new_context_with_host(Some(accepting_host_call));
    let callable = [Value {
        tag: crate::abi::VALUE_CALLABLE,
        flags: 0,
        payload0: 0x1234,
        payload1: 0,
    }];
    let (status, set) = invoke(
        context,
        "function:libxml_set_external_entity_loader",
        0,
        &callable,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, set.result_id);

    HOST_THROW_OPCODE.store(HOST_OPCODE_RELEASE_CALLABLE, Ordering::Relaxed);
    HOST_REENTRANT_CONTEXT.store(context, Ordering::Relaxed);
    let null = [Value {
        tag: VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }];
    let (status, thrown) = invoke(
        context,
        "function:libxml_set_external_entity_loader",
        0,
        &null,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(thrown.status, STATUS_THROW);
    assert_eq!(
        thrown.php_error_kind,
        PHP_ERROR_KIND_PENDING_HOST_THROWABLE
    );
    assert_eq!(
        crate::context::context(context)
            .expect("context remains registered")
            .borrow()
            .external_entity_loader,
        None
    );
    crate::elephc_dom_result_release(context, thrown.result_id);
    HOST_REENTRANT_CONTEXT.store(0, Ordering::Relaxed);
    HOST_THROW_OPCODE.store(0, Ordering::Relaxed);
    crate::elephc_dom_context_free(context);
}

/// Verifies the native XML fragment adapter returns every parsed top-level child.
#[test]
fn xml_fragment_parser_preserves_context_children() {
    let parsed = crate::native::document_parse_xml(
        b"<root xmlns:x=\"urn:x\"><div>old</div></root>",
        0,
        None,
        None,
    )
    .expect("XML document parsing succeeds");
    let document = parsed.document.expect("parsed document exists");
    assert!(crate::native::document_convert_modern_xml(document));
    let root =
        crate::native::document_element(document).expect("root exists");
    let div =
        crate::native::element_first_child(root).expect("div exists");
    let fragment = crate::native::parse_fragment(
        div,
        b"<x:item/>text&amp;",
        false,
    )
    .expect("fragment parses");
    let element = crate::native::node_first_child(fragment)
        .expect("fragment element exists");
    assert_eq!(crate::native::node_type(element), 1);
    let text = crate::native::node_next_sibling(element)
        .expect("fragment text exists");
    assert_eq!(crate::native::node_type(text), 3);
    unsafe {
        crate::native::node_free(fragment);
        crate::native::document_free(document);
    }
}

/// Verifies the pinned PHP selector adapter returns ordered nodes and exact failures.
#[test]
fn native_selector_adapter_matches_php_semantics() {
    let parsed = crate::native::document_parse_xml(
        b"<root><section id=\"s\"><p class=\"hot\"/><p><b id=\"leaf\"/></p></section><p class=\"hot\"/></root>",
        0,
        None,
        None,
    )
    .expect("XML document parsing succeeds");
    let document = parsed.document.expect("parsed document exists");
    assert!(crate::native::document_convert_modern_xml(document));
    let hot = crate::native::selector_query(
        document,
        b"p.hot, #leaf",
        1,
        false,
    );
    assert_eq!(hot.error_code, 0);
    assert_eq!(hot.pointers.len(), 3);
    assert_eq!(
        crate::native::node_name(hot.pointers[0]).as_deref(),
        Some(b"p".as_slice())
    );
    assert_eq!(
        crate::native::node_name(hot.pointers[1]).as_deref(),
        Some(b"b".as_slice())
    );

    let section = crate::native::selector_query(
        document,
        b"section",
        0,
        false,
    )
    .pointers[0];
    let matches =
        crate::native::selector_query(section, b"#s", 2, false);
    assert_eq!(matches.error_code, 0);
    assert!(matches.matched);
    let leaf = hot.pointers[1];
    let closest =
        crate::native::selector_query(leaf, b"section", 3, false);
    assert_eq!(closest.pointers, vec![section]);

    let invalid = crate::native::selector_query(
        document,
        b"@invalid",
        0,
        false,
    );
    assert_eq!(invalid.error_code, 12);
    assert_eq!(
        invalid.message,
        b"Invalid selector (Selectors. Unexpected token: @invalid)"
    );
    let blank =
        crate::native::selector_query(section, b":blank", 2, false);
    assert_eq!(blank.error_code, 9);
    assert_eq!(
        blank.message,
        b":blank selector is not implemented because CSSWG has not yet decided its semantics (https://github.com/w3c/csswg-drafts/issues/1967)"
    );
    unsafe {
        crate::native::document_free(document);
    }
}

/// Verifies XPath context-node namespaces are registered only when explicitly enabled.
#[test]
fn native_xpath_context_namespace_registration_is_per_call() {
    let parsed = crate::native::document_parse_xml(
        b"<root><scope xmlns:p=\"urn:one\"><p:item/></scope></root>",
        0,
        None,
        None,
    )
    .expect("XML document parsing succeeds");
    let document = parsed.document.expect("parsed document exists");
    assert!(crate::native::document_convert_modern_xml(document));
    let root =
        crate::native::document_element(document).expect("root exists");
    let scope =
        crate::native::element_first_child(root).expect("scope exists");

    let enabled = crate::native::xpath_evaluate(
        document,
        Some(scope),
        true,
        true,
        true,
        b"//p:item",
        &[],
        0,
        None,
        0,
        &[],
    )
    .expect("enabled XPath evaluation returns an outcome");
    assert_eq!(enabled.status, 0);
    assert!(matches!(
        enabled.value,
        crate::native::XPathValue::Nodes(ref pointers)
            if pointers.len() == 1
    ));

    let disabled = crate::native::xpath_evaluate(
        document,
        Some(scope),
        true,
        false,
        true,
        b"//p:item",
        &[],
        0,
        None,
        0,
        &[],
    )
    .expect("disabled XPath evaluation returns an outcome");
    assert_eq!(disabled.status, 3);
    assert!(!disabled.errors.is_empty());

    unsafe {
        crate::native::document_free(document);
    }
}

/// Verifies modern XPath state, scalar results, and node-list snapshots through the public ABI.
#[test]
fn xpath_round_trips_through_public_bridge_operations() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"><p:item/><p:item/></root>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\xmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(created.value_tag, VALUE_BRIDGE_HANDLE);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let document_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: document,
        payload1: 0,
    }];
    let (status, constructed) = invoke(
        context,
        "method:dom\\xpath::__construct",
        0,
        &document_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(constructed.value_tag, VALUE_BRIDGE_HANDLE);
    let xpath = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let (status, retained_document) = invoke(
        context,
        "property-get:dom\\xpath::$document",
        xpath,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(retained_document.payload0, document);
    crate::elephc_dom_result_release(
        context,
        retained_document.result_id,
    );

    let count_expression = b"count(//p:item)";
    let count_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: count_expression.len() as u64,
    }];
    let (status, count) = invoke(
        context,
        "method:dom\\xpath::evaluate",
        xpath,
        &count_value,
        count_expression,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(count.value_tag, VALUE_FLOAT);
    assert_eq!(f64::from_bits(count.payload0), 2.0);
    crate::elephc_dom_result_release(context, count.result_id);

    let query_expression = b"//p:item";
    let query_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: query_expression.len() as u64,
    }];
    let (status, queried) = invoke(
        context,
        "method:dom\\xpath::query",
        xpath,
        &query_value,
        query_expression,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(queried.value_tag, VALUE_BRIDGE_HANDLE);
    let eager_wrappers = result_values(&queried);
    assert_eq!(eager_wrappers.len(), 2);
    for wrapper in &eager_wrappers {
        assert_eq!(wrapper.tag, VALUE_BRIDGE_HANDLE);
        assert_eq!(wrapper.flags, 0);
        assert_eq!(wrapper.payload1, 201);
    }
    let node_list = queried.payload0;
    crate::elephc_dom_result_release(context, queried.result_id);

    let (status, length) = invoke(
        context,
        "property-get:dom\\nodelist::$length",
        node_list,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(length.value_tag, VALUE_INT);
    assert_eq!(length.payload0, 2);
    crate::elephc_dom_result_release(context, length.result_id);

    let dynamic_omission_values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: query_expression.len() as u64,
        },
        Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_BOOL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
        Value {
            tag: VALUE_BOOL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        },
    ];
    let (status, dynamically_omitted) = invoke(
        context,
        "method:dom\\xpath::query",
        xpath,
        &dynamic_omission_values,
        query_expression,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(dynamically_omitted.value_tag, VALUE_BRIDGE_HANDLE);
    crate::elephc_dom_result_release(context, dynamically_omitted.result_id);

    crate::elephc_dom_context_free(context);
}

/// Verifies custom XPath callback replacement, cloning, and release balance host ownership.
#[test]
fn xpath_custom_callback_ownership_is_balanced() {
    let _guard = HOST_TEST_LOCK.lock().expect("host test lock");
    HOST_RETAINS.store(0, Ordering::Relaxed);
    HOST_RELEASES.store(0, Ordering::Relaxed);
    HOST_THROW_OPCODE.store(0, Ordering::Relaxed);
    HOST_REENTRANT_CONTEXT.store(0, Ordering::Relaxed);
    let context = new_context_with_host(Some(accepting_host_call));
    let source = b"<root/>";
    let source_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, created) = invoke(
        context,
        "method:dom\\xmldocument::createfromstring",
        0,
        &source_value,
        source,
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let document_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: document,
        payload1: 0,
    }];
    let (status, constructed) = invoke(
        context,
        "method:dom\\xpath::__construct",
        0,
        &document_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let xpath = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let namespace = b"urn:callback";
    let name = b"render";
    let mut bytes = namespace.to_vec();
    bytes.extend_from_slice(name);
    let mut values = [
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: 0,
            payload1: namespace.len() as u64,
        },
        Value {
            tag: VALUE_BYTES,
            flags: 0,
            payload0: namespace.len() as u64,
            payload1: name.len() as u64,
        },
        Value {
            tag: crate::abi::VALUE_CALLABLE,
            flags: 0,
            payload0: 0x1111,
            payload1: 0,
        },
    ];
    let (status, registered) = invoke(
        context,
        "method:dom\\xpath::registerphpfunctionns",
        xpath,
        &values,
        &bytes,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, registered.result_id);
    assert_eq!(HOST_RETAINS.load(Ordering::Relaxed), 1);
    assert_eq!(HOST_RELEASES.load(Ordering::Relaxed), 0);

    values[2].payload0 = 0x2222;
    let (status, replaced) = invoke(
        context,
        "method:dom\\xpath::registerphpfunctionns",
        xpath,
        &values,
        &bytes,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, replaced.result_id);
    assert_eq!(HOST_RETAINS.load(Ordering::Relaxed), 2);
    assert_eq!(HOST_RELEASES.load(Ordering::Relaxed), 1);

    let (status, cloned) = invoke(
        context,
        "internal:bridge.object.clone",
        xpath,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let clone = cloned.payload0;
    crate::elephc_dom_result_release(context, cloned.result_id);
    assert_eq!(HOST_RETAINS.load(Ordering::Relaxed), 3);

    for handle in [xpath, clone] {
        let (status, released) = invoke(
            context,
            "internal:bridge.wrapper.release",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK);
        crate::elephc_dom_result_release(context, released.result_id);
    }
    assert_eq!(HOST_RELEASES.load(Ordering::Relaxed), 3);
    crate::elephc_dom_context_free(context);
}

/// Builds one legacy `DOMDocument` loaded from `source` and returns its handle.
fn legacy_document_with(source: &[u8], context: u64) -> u64 {
    let (status, constructed) =
        invoke(context, "method:domdocument::__construct", 0, &[], &[]);
    assert_eq!(status, STATUS_OK);
    assert_eq!(constructed.value_tag, VALUE_BRIDGE_HANDLE);
    let document = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let values = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: source.len() as u64,
    }];
    let (status, loaded) =
        invoke(context, "method:domdocument::loadxml", document, &values, source);
    assert_eq!(status, STATUS_OK);
    assert_eq!(loaded.value_tag, VALUE_BOOL);
    assert_eq!(loaded.payload0, 1);
    crate::elephc_dom_result_release(context, loaded.result_id);
    document
}

/// Materializes one `DOMNameSpaceNode` from a legacy XPath `namespace::*` query.
///
/// `index` selects the snapshot member. The returned tuple is the namespace-node
/// bridge handle and the owning nodelist handle (kept alive by the caller).
fn namespace_node_at(context: u64, document: u64, index: i64) -> (u64, u64) {
    let document_value = [Value {
        tag: VALUE_BRIDGE_HANDLE,
        flags: 0,
        payload0: document,
        payload1: 0,
    }];
    let (status, constructed) = invoke(
        context,
        "method:domxpath::__construct",
        0,
        &document_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(constructed.value_tag, VALUE_BRIDGE_HANDLE);
    let xpath = constructed.payload0;
    crate::elephc_dom_result_release(context, constructed.result_id);

    let expression = b"//namespace::*";
    let expression_value = [Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: 0,
        payload1: expression.len() as u64,
    }];
    let (status, queried) = invoke(
        context,
        "method:domxpath::query",
        xpath,
        &expression_value,
        expression,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(queried.value_tag, VALUE_BRIDGE_HANDLE);
    let eager_wrappers = result_values(&queried);
    assert!(!eager_wrappers.is_empty());
    assert_eq!(eager_wrappers.len() % 2, 0);
    for pair in eager_wrappers.chunks_exact(2) {
        assert_eq!(pair[0].tag, VALUE_BRIDGE_HANDLE);
        assert_eq!(pair[0].flags, 1);
        assert_eq!(pair[0].payload1, 101);
        assert_eq!(pair[1].tag, VALUE_BRIDGE_HANDLE);
        assert_eq!(pair[1].flags, 0);
        assert_eq!(pair[1].payload1, 118);
    }
    let node_list = queried.payload0;
    crate::elephc_dom_result_release(context, queried.result_id);

    let index_value = [Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: index as u64,
        payload1: 0,
    }];
    let (status, item) = invoke(
        context,
        "method:domnodelist::item",
        node_list,
        &index_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(item.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(item.payload1, 118);
    assert_eq!(
        item.payload0,
        eager_wrappers[index as usize * 2 + 1].payload0,
    );
    let namespace_node = item.payload0;
    crate::elephc_dom_result_release(context, item.result_id);

    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        xpath,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);

    (namespace_node, node_list)
}

/// Reads one string-valued namespace-node property.
fn namespace_property(context: u64, handle: u64, property: &str) -> Vec<u8> {
    let (status, result) = invoke(context, property, handle, &[], &[]);
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.value_tag, VALUE_BYTES);
    let bytes = result_bytes(&result);
    crate::elephc_dom_result_release(context, result.result_id);
    bytes
}

/// Releases one bridge wrapper handle and its result frame.
fn release_handle(context: u64, handle: u64) {
    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        handle,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);
}

/// Verifies the ten `DOMNameSpaceNode` property reads match the PHP 8.5 oracle.
#[test]
fn namespace_node_property_reads_match_php_oracle() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"><p:child/></root>";
    let document = legacy_document_with(source, context);
    let (namespace_node, node_list) = namespace_node_at(context, document, 1);

    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeName"),
        b"xmlns:p"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeValue"),
        b"urn:p"
    );
    let (status, node_type) = invoke(
        context,
        "property-get:domnamespacenode::$nodeType",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(node_type.value_tag, VALUE_INT);
    assert_eq!(node_type.payload0, 18);
    crate::elephc_dom_result_release(context, node_type.result_id);

    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$prefix"),
        b"p"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$localName"),
        b"p"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$namespaceURI"),
        b"urn:p"
    );

    let (status, connected) = invoke(
        context,
        "property-get:domnamespacenode::$isConnected",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(connected.value_tag, VALUE_BOOL);
    assert_eq!(connected.payload0, 1);
    crate::elephc_dom_result_release(context, connected.result_id);

    let (status, owner) = invoke(
        context,
        "property-get:domnamespacenode::$ownerDocument",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(owner.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(owner.payload0, document);
    assert_eq!(owner.payload1, 109);
    crate::elephc_dom_result_release(context, owner.result_id);

    let (status, parent) = invoke(
        context,
        "property-get:domnamespacenode::$parentNode",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(parent.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(parent.payload1, 101);
    let parent_handle = parent.payload0;
    crate::elephc_dom_result_release(context, parent.result_id);

    let (status, parent_element) = invoke(
        context,
        "property-get:domnamespacenode::$parentElement",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(parent_element.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(parent_element.payload0, parent_handle);
    assert_eq!(parent_element.payload1, 101);
    crate::elephc_dom_result_release(context, parent_element.result_id);

    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);
    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        node_list,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);
    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies the default-namespace `DOMNameSpaceNode` reports the `xmlns` name.
#[test]
fn namespace_node_default_namespace_name_is_xmlns() {
    let context = new_context();
    let source = b"<root xmlns=\"urn:def\"/>";
    let document = legacy_document_with(source, context);
    let (namespace_node, node_list) = namespace_node_at(context, document, 1);

    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeName"),
        b"xmlns"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeValue"),
        b"urn:def"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$prefix"),
        b""
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$localName"),
        b"xmlns"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$namespaceURI"),
        b"urn:def"
    );

    for handle in [namespace_node, node_list, document] {
        let (status, released) = invoke(
            context,
            "internal:bridge.wrapper.release",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK);
        crate::elephc_dom_result_release(context, released.result_id);
    }
    crate::elephc_dom_context_free(context);
}

/// Verifies `__sleep` and `__wakeup` reject serialization with PHP's exact exception.
#[test]
fn namespace_node_sleep_and_wakeup_reject_serialization() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"/>";
    let document = legacy_document_with(source, context);
    let (namespace_node, node_list) = namespace_node_at(context, document, 1);

    let (status, sleep) = invoke(
        context,
        "method:domnamespacenode::__sleep",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(sleep.status, STATUS_THROW);
    assert_eq!(sleep.php_error_kind, PHP_ERROR_KIND_EXCEPTION);
    assert_eq!(
        result_bytes(&sleep),
        b"Serialization of 'DOMNameSpaceNode' is not allowed, unless serialization methods are implemented in a subclass",
    );
    crate::elephc_dom_result_release(context, sleep.result_id);

    let (status, wakeup) = invoke(
        context,
        "method:domnamespacenode::__wakeup",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(wakeup.status, STATUS_THROW);
    assert_eq!(wakeup.php_error_kind, PHP_ERROR_KIND_EXCEPTION);
    assert_eq!(
        result_bytes(&wakeup),
        b"Unserialization of 'DOMNameSpaceNode' is not allowed, unless unserialization methods are implemented in a subclass",
    );
    crate::elephc_dom_result_release(context, wakeup.result_id);

    for handle in [namespace_node, node_list, document] {
        let (status, released) = invoke(
            context,
            "internal:bridge.wrapper.release",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK);
        crate::elephc_dom_result_release(context, released.result_id);
    }
    crate::elephc_dom_context_free(context);
}

/// Verifies repeated `item()` returns the same canonical wrapper handle.
#[test]
fn namespace_node_item_is_canonical_across_reads() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"/>";
    let document = legacy_document_with(source, context);
    let (first, node_list) = namespace_node_at(context, document, 1);

    let index_value = [Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: 1,
        payload1: 0,
    }];
    let (status, second) = invoke(
        context,
        "method:domnodelist::item",
        node_list,
        &index_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(second.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(second.payload1, 118);
    assert_eq!(second.payload0, first);
    crate::elephc_dom_result_release(context, second.result_id);

    for handle in [first, node_list, document] {
        let (status, released) = invoke(
            context,
            "internal:bridge.wrapper.release",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK);
        crate::elephc_dom_result_release(context, released.result_id);
    }
    crate::elephc_dom_context_free(context);
}

/// Verifies the snapshot recreates a namespace-node wrapper after its prior wrapper was
/// released, matching PHP 8.5 where `DOMNodeList::item()` never returns null for a live
/// namespace slot. The shared allocation keeps the fake node alive in the slot, so a
/// fresh wrapper is materialized with the same binding and a distinct handle.
#[test]
fn namespace_node_recreates_after_release_while_list_lives() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"/>";
    let document = legacy_document_with(source, context);
    let (first, node_list) = namespace_node_at(context, document, 1);

    release_handle(context, first);

    let index_value = [Value {
        tag: VALUE_INT,
        flags: 0,
        payload0: 1,
        payload1: 0,
    }];
    let (status, item) = invoke(
        context,
        "method:domnodelist::item",
        node_list,
        &index_value,
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(item.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(item.payload1, 118);
    let second = item.payload0;
    assert_ne!(second, first);
    crate::elephc_dom_result_release(context, item.result_id);

    assert_eq!(
        namespace_property(context, second, "property-get:domnamespacenode::$nodeName"),
        b"xmlns:p"
    );

    release_handle(context, second);
    release_handle(context, node_list);
    release_handle(context, document);
    crate::elephc_dom_context_free(context);
}

/// Verifies a namespace-node wrapper stays valid after its originating snapshot, XPath
/// context, and document are released: the shared allocation keeps the fake node alive
/// while any wrapper retains it, matching PHP 8.5 wrapper lifetimes.
#[test]
fn namespace_node_survives_snapshot_release() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"/>";
    let document = legacy_document_with(source, context);
    let (namespace_node, node_list) = namespace_node_at(context, document, 1);

    release_handle(context, node_list);
    release_handle(context, document);

    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeName"),
        b"xmlns:p"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$prefix"),
        b"p"
    );
    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeValue"),
        b"urn:p"
    );

    release_handle(context, namespace_node);
    crate::elephc_dom_context_free(context);
}

/// Verifies a namespace snapshot releases fake nodes before its final document retain.
#[test]
fn namespace_snapshot_drops_allocations_before_document_graph() {
    let context = new_context();
    let document = legacy_document_with(b"<root xmlns:p=\"urn:p\"/>", context);
    let (namespace_node, node_list) = namespace_node_at(context, document, 1);

    release_handle(context, namespace_node);
    release_handle(context, document);
    release_handle(context, node_list);
    crate::elephc_dom_context_free(context);
}

/// Verifies cloning a namespace-node wrapper yields an independent wrapper.
#[test]
fn namespace_node_clone_is_independent() {
    let context = new_context();
    let source = b"<root xmlns:p=\"urn:p\"/>";
    let document = legacy_document_with(source, context);
    let (namespace_node, node_list) = namespace_node_at(context, document, 1);

    let (status, cloned) = invoke(
        context,
        "internal:bridge.object.clone",
        namespace_node,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(cloned.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(cloned.payload1, 118);
    let clone = cloned.payload0;
    assert_ne!(clone, namespace_node);
    crate::elephc_dom_result_release(context, cloned.result_id);

    assert_eq!(
        namespace_property(context, clone, "property-get:domnamespacenode::$nodeName"),
        b"xmlns:p"
    );
    assert_eq!(
        namespace_property(context, clone, "property-get:domnamespacenode::$prefix"),
        b"p"
    );

    let (status, released) = invoke(
        context,
        "internal:bridge.wrapper.release",
        clone,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, released.result_id);

    assert_eq!(
        namespace_property(context, namespace_node, "property-get:domnamespacenode::$nodeName"),
        b"xmlns:p"
    );

    for handle in [namespace_node, node_list, document] {
        let (status, released) = invoke(
            context,
            "internal:bridge.wrapper.release",
            handle,
            &[],
            &[],
        );
        assert_eq!(status, STATUS_OK);
        crate::elephc_dom_result_release(context, released.result_id);
    }
    crate::elephc_dom_context_free(context);
}

// ---------------------------------------------------------------------------
// registerNodeClass: bridge validation, classmap storage, and override.
// ---------------------------------------------------------------------------

/// Legacy DOM root-node class id used by the synthetic hierarchy.
const RNC_DOMNODE: u64 = 1;
/// Legacy DOM element class id used by the synthetic hierarchy.
const RNC_DOMELEMENT: u64 = 2;
/// Legacy DOM attribute class id used by the synthetic hierarchy.
const RNC_DOMATTR: u64 = 3;
/// Legacy DOM document class id used by the synthetic hierarchy.
const RNC_DOMDOCUMENT: u64 = 4;
/// Userland legacy element subclass id used by successful mappings.
const RNC_MY_ELEMENT: u64 = 10;
/// Userland legacy attribute subclass id used by hierarchy validation.
const RNC_MY_ATTR: u64 = 11;
/// Legacy node subclass id that does not derive from `DOMElement`.
const RNC_BAD_NODE: u64 = 12;
/// Abstract legacy element subclass id used by validation failures.
const RNC_ABSTRACT_ELEMENT: u64 = 13;
/// Abstract legacy node subclass id used by validation failures.
const RNC_ABSTRACT_NODE: u64 = 14;
/// Modern DOM root-node class id used by the synthetic hierarchy.
const RNC_MODERN_NODE: u64 = 20;
/// Modern DOM element class id used by the synthetic hierarchy.
const RNC_MODERN_ELEMENT: u64 = 21;
/// Abstract modern document class id used by the synthetic hierarchy.
const RNC_MODERN_DOCUMENT: u64 = 22;
/// Modern HTML document class id used by cross-family validation.
const RNC_HTML_DOCUMENT: u64 = 23;
/// Userland modern element subclass id used by successful mappings.
const RNC_MODERN_MY_ELEMENT: u64 = 24;
/// Alternate userland legacy element subclass id used by replacement tests.
const RNC_ANOTHER_ELEMENT: u64 = 25;
/// Native modern HTML element class id used by specialized-wrapper tests.
const RNC_HTML_ELEMENT: u64 = 26;

/// Installs a synthetic PHP class hierarchy for register-node-class validation.
fn install_register_node_class_classes(context: u64) {
    let rows: &[(&str, u64, u64, u32)] = &[
        ("DOMNode", RNC_DOMNODE, DOM_CLASS_NO_PARENT, 0),
        ("DOMElement", RNC_DOMELEMENT, RNC_DOMNODE, 0),
        ("DOMAttr", RNC_DOMATTR, RNC_DOMNODE, 0),
        ("DOMDocument", RNC_DOMDOCUMENT, RNC_DOMNODE, 0),
        ("MyElement", RNC_MY_ELEMENT, RNC_DOMELEMENT, 0),
        ("MyAttr", RNC_MY_ATTR, RNC_DOMATTR, 0),
        ("BadClass", RNC_BAD_NODE, RNC_DOMNODE, 0),
        ("AbstractElement", RNC_ABSTRACT_ELEMENT, RNC_DOMELEMENT, 1),
        ("AbstractNode", RNC_ABSTRACT_NODE, RNC_DOMNODE, 1),
        ("Dom\\Node", RNC_MODERN_NODE, DOM_CLASS_NO_PARENT, 0),
        ("Dom\\Element", RNC_MODERN_ELEMENT, RNC_MODERN_NODE, 0),
        ("Dom\\Document", RNC_MODERN_DOCUMENT, RNC_MODERN_NODE, 1),
        ("Dom\\HTMLDocument", RNC_HTML_DOCUMENT, RNC_MODERN_NODE, 0),
        (
            "Dom\\HTMLElement",
            RNC_HTML_ELEMENT,
            RNC_MODERN_ELEMENT,
            0,
        ),
        (
            "ModernElement",
            RNC_MODERN_MY_ELEMENT,
            RNC_MODERN_ELEMENT,
            0,
        ),
        (
            "AnotherElement",
            RNC_ANOTHER_ELEMENT,
            RNC_DOMELEMENT,
            0,
        ),
    ];
    let names = rows
        .iter()
        .map(|(name, _, _, _)| name.as_bytes().to_vec())
        .collect::<Vec<_>>();
    let entries = rows
        .iter()
        .zip(names.iter())
        .map(|((_, id, parent, abstract_flag), name)| DomClassMetadataEntry {
            name_ptr: name.as_ptr(),
            name_len: name.len() as u64,
            class_id: *id,
            parent_class_id: *parent,
            is_abstract: *abstract_flag,
            reserved: 0,
        })
        .collect::<Vec<_>>();
    let status = unsafe {
        crate::elephc_dom_context_set_class_metadata(
            context,
            entries.as_ptr(),
            entries.len() as u64,
        )
    };
    assert_eq!(status, STATUS_OK);
}

/// Builds one byte-string value positioned at `offset` with `length` bytes.
fn bytes_value(offset: u64, length: usize) -> Value {
    Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset,
        payload1: length as u64,
    }
}

/// Builds one null value for a nullable argument.
fn null_value() -> Value {
    Value {
        tag: VALUE_NULL,
        flags: 0,
        payload0: 0,
        payload1: 0,
    }
}

/// Calls the legacy `registerNodeClass()` route with one optional extended class.
fn register_node_class_call(
    context: u64,
    document: u64,
    base: &[u8],
    extended: Option<&[u8]>,
) -> (u32, ResultHeader) {
    let mut bytes = base.to_vec();
    let extended_value = match extended {
        Some(name) => {
            let offset = bytes.len() as u64;
            bytes.extend_from_slice(name);
            bytes_value(offset, name.len())
        }
        None => null_value(),
    };
    let values = [bytes_value(0, base.len()), extended_value];
    invoke(
        context,
        "method:domdocument::registernodeclass",
        document,
        &values,
        &bytes,
    )
}

/// Reads the document element's wrapper discriminator from a legacy document.
fn legacy_document_element_kind(context: u64, document: u64) -> u64 {
    let (status, result) = invoke(
        context,
        "property-get:domdocument::$documentElement",
        document,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.value_tag, VALUE_BRIDGE_HANDLE);
    let kind = result.payload1;
    crate::elephc_dom_result_release(context, result.result_id);
    kind
}

/// Verifies a successful legacy registration returns `true`.
#[test]
fn legacy_register_node_class_returns_true_on_success() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"DOMElement", Some(b"MyElement"));
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.value_tag, VALUE_BOOL);
    assert_eq!(result.payload0, 1);
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies a null extended class resets the mapping and returns `true`.
#[test]
fn legacy_register_node_class_null_resets_the_mapping() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"DOMElement", Some(b"MyElement"));
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    let (status, result) =
        register_node_class_call(context, document, b"DOMElement", None);
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.value_tag, VALUE_BOOL);
    assert_eq!(result.payload0, 1);
    crate::elephc_dom_result_release(context, result.result_id);
    assert_eq!(legacy_document_element_kind(context, document), 101);
    crate::elephc_dom_context_free(context);
}

/// Verifies a registered class overrides a materialized legacy element kind.
#[test]
fn legacy_register_node_class_overrides_the_document_element_wrapper_kind() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"DOMElement", Some(b"MyElement"));
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    assert_eq!(
        legacy_document_element_kind(context, document),
        RNC_MY_ELEMENT | (1u64 << 63)
    );
    crate::elephc_dom_context_free(context);
}

/// Verifies PHP class names are matched case-insensitively.
#[test]
fn legacy_register_node_class_matches_class_names_case_insensitively() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"domelement", Some(b"myelement"));
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    assert_eq!(
        legacy_document_element_kind(context, document),
        RNC_MY_ELEMENT | (1u64 << 63)
    );
    crate::elephc_dom_context_free(context);
}

/// Verifies php-src accepts a native class as the legacy mapping target.
#[test]
fn legacy_register_node_class_accepts_a_native_extended_class() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) = register_node_class_call(
        context,
        document,
        b"DOMElement",
        Some(b"DOMElement"),
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.value_tag, VALUE_BOOL);
    crate::elephc_dom_result_release(context, result.result_id);
    assert_eq!(
        legacy_document_element_kind(context, document),
        RNC_DOMELEMENT | (1u64 << 63)
    );
    crate::elephc_dom_context_free(context);
}

/// Verifies the modern route returns void and overrides a created element kind.
#[test]
fn modern_register_node_class_returns_void_and_overrides_element_wrapper_kind() {
    let context = new_context();
    install_register_node_class_classes(context);
    let (status, created) = invoke(
        context,
        "method:dom\\xmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);

    let bytes = b"Dom\\ElementModernElement";
    let values = [bytes_value(0, b"Dom\\Element".len()), bytes_value(11, 13)];
    let (status, registered) = invoke(
        context,
        "method:dom\\document::registernodeclass",
        document,
        &values,
        bytes,
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(registered.value_tag, VALUE_NULL);
    crate::elephc_dom_result_release(context, registered.result_id);

    let root_name = [bytes_value(0, 4)];
    let (status, element) = invoke(
        context,
        "method:dom\\document::createelement",
        document,
        &root_name,
        b"root",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(element.value_tag, VALUE_BRIDGE_HANDLE);
    assert_eq!(
        element.payload1,
        RNC_MODERN_MY_ELEMENT | (1u64 << 63)
    );
    crate::elephc_dom_result_release(context, element.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies a modern mapping accepts the native HTML element subclass.
#[test]
fn modern_register_node_class_accepts_a_native_html_element_target() {
    let context = new_context();
    install_register_node_class_classes(context);
    let (status, created) = invoke(
        context,
        "method:dom\\htmldocument::createempty",
        0,
        &[],
        &[],
    );
    assert_eq!(status, STATUS_OK);
    let document = created.payload0;
    crate::elephc_dom_result_release(context, created.result_id);
    let bytes = b"Dom\\ElementDom\\HTMLElement";
    let values = [
        bytes_value(0, b"Dom\\Element".len()),
        bytes_value(11, b"Dom\\HTMLElement".len()),
    ];
    let (status, registered) = invoke(
        context,
        "method:dom\\document::registernodeclass",
        document,
        &values,
        bytes,
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, registered.result_id);
    let name = [bytes_value(0, 3)];
    let (status, element) = invoke(
        context,
        "method:dom\\document::createelement",
        document,
        &name,
        b"div",
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(element.payload1, 301);
    crate::elephc_dom_result_release(context, element.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies a second registration replaces the previous mapping.
#[test]
fn legacy_register_node_class_replaces_the_previous_mapping() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"DOMElement", Some(b"MyElement"));
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    let (status, result) = register_node_class_call(
        context,
        document,
        b"DOMElement",
        Some(b"AnotherElement"),
    );
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    assert_eq!(
        legacy_document_element_kind(context, document),
        RNC_ANOTHER_ELEMENT | (1u64 << 63)
    );
    crate::elephc_dom_context_free(context);
}

/// Verifies mappings remain isolated to their authoritative document graph.
#[test]
fn legacy_register_node_class_is_isolated_per_document() {
    let context = new_context();
    install_register_node_class_classes(context);
    let first = legacy_document_with(b"<root/>", context);
    let second = legacy_document_with(b"<other/>", context);
    let (status, result) =
        register_node_class_call(context, first, b"DOMElement", Some(b"MyElement"));
    assert_eq!(status, STATUS_OK);
    crate::elephc_dom_result_release(context, result.result_id);
    assert_eq!(
        legacy_document_element_kind(context, first),
        RNC_MY_ELEMENT | (1u64 << 63)
    );
    assert_eq!(legacy_document_element_kind(context, second), 101);
    crate::elephc_dom_context_free(context);
}

/// Verifies argument one rejects a class outside the legacy DOM family.
#[test]
fn legacy_register_node_class_rejects_a_non_derived_base_class() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) = register_node_class_call(
        context,
        document,
        b"Dom\\HTMLDocument",
        Some(b"DOMDocument"),
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.status, STATUS_THROW);
    assert_eq!(result.php_error_kind, PHP_ERROR_KIND_TYPE_ERROR);
    assert_eq!(
        result_bytes(&result),
        b"DOMDocument::registerNodeClass(): Argument #1 ($baseClass) must be a class name \
         derived from DOMNode, Dom\\HTMLDocument given"
    );
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies an unknown base class uses php-src's derived-class diagnostic.
#[test]
fn legacy_register_node_class_rejects_an_unknown_base_class_as_non_derived() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"NoSuch", Some(b"MyElement"));
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.status, STATUS_THROW);
    assert_eq!(result.php_error_kind, PHP_ERROR_KIND_TYPE_ERROR);
    assert_eq!(
        result_bytes(&result),
        b"DOMDocument::registerNodeClass(): Argument #1 ($baseClass) must be a class name \
         derived from DOMNode, NoSuch given"
    );
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies argument two rejects a class not derived from the selected base.
#[test]
fn legacy_register_node_class_rejects_a_non_derived_extended_class() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"domelement", Some(b"badclass"));
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.status, STATUS_THROW);
    assert_eq!(result.php_error_kind, PHP_ERROR_KIND_ERROR);
    assert_eq!(
        result_bytes(&result),
        b"DOMDocument::registerNodeClass(): Argument #2 ($extendedClass) must be a class \
         name derived from DOMElement or null, BadClass given"
    );
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies an unknown extended class uses php-src's distinct `TypeError`.
#[test]
fn legacy_register_node_class_rejects_an_unknown_extended_class() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) =
        register_node_class_call(context, document, b"DOMElement", Some(b"NoSuch"));
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.status, STATUS_THROW);
    assert_eq!(result.php_error_kind, PHP_ERROR_KIND_TYPE_ERROR);
    assert_eq!(
        result_bytes(&result),
        b"DOMDocument::registerNodeClass(): Argument #2 ($extendedClass) must be a valid class name or null, NoSuch given"
    );
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies argument one rejects an abstract base class.
#[test]
fn legacy_register_node_class_rejects_an_abstract_base_class() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) = register_node_class_call(
        context,
        document,
        b"AbstractNode",
        Some(b"MyElement"),
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.status, STATUS_THROW);
    assert_eq!(result.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(
        result_bytes(&result),
        b"DOMDocument::registerNodeClass(): Argument #1 ($baseClass) must not be an abstract class"
    );
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies argument two rejects an abstract extended class.
#[test]
fn legacy_register_node_class_rejects_an_abstract_extended_class() {
    let context = new_context();
    install_register_node_class_classes(context);
    let document = legacy_document_with(b"<root/>", context);
    let (status, result) = register_node_class_call(
        context,
        document,
        b"DOMElement",
        Some(b"AbstractElement"),
    );
    assert_eq!(status, STATUS_OK);
    assert_eq!(result.status, STATUS_THROW);
    assert_eq!(result.php_error_kind, PHP_ERROR_KIND_VALUE_ERROR);
    assert_eq!(
        result_bytes(&result),
        b"DOMDocument::registerNodeClass(): Argument #2 ($extendedClass) must not be an abstract class"
    );
    crate::elephc_dom_result_release(context, result.result_id);
    crate::elephc_dom_context_free(context);
}

/// Verifies modern tree walks, clones, imports, and stale DTD reads treat entity declarations as metadata.
#[test]
fn modern_entity_references_do_not_recurse_into_dtd_declarations() {
    let source = crate::native::document_parse_xml(
        br#"<!DOCTYPE root [<!ENTITY foo "bar">]><root>&foo;</root>"#,
        0,
        None,
        None,
    )
    .expect("entity document parses")
    .document
    .expect("entity document exists");
    assert!(crate::native::document_convert_modern_xml(source));
    let root = crate::native::document_element(source).expect("root exists");
    let reference = crate::native::node_first_child(root).expect("reference exists");
    assert_eq!(crate::native::node_type(reference), 5);
    assert_eq!(
        crate::native::node_type(
            crate::native::node_first_child(reference)
                .expect("declaration is visible in the source document"),
        ),
        17
    );

    let clone = crate::native::node_clone(root, true, true)
        .expect("modern deep clone terminates");
    let cloned_reference =
        crate::native::node_first_child(clone).expect("clone keeps reference");
    assert_eq!(crate::native::node_type(cloned_reference), 5);
    assert!(crate::native::node_first_child(cloned_reference).is_some());

    let target = crate::native::document_new(b"1.0", b"UTF-8")
        .expect("target document exists");
    assert!(crate::native::document_convert_modern_xml(target));
    let imported = crate::native::document_import_node(target, root, true, true);
    assert_eq!(imported.error_code, 0);
    let imported = imported.pointer.expect("modern import terminates");
    let imported_reference = crate::native::node_first_child(imported)
        .expect("import keeps reference node");
    assert_eq!(crate::native::node_type(imported_reference), 5);
    assert_eq!(crate::native::node_first_child(imported_reference), None);

    let doctype = crate::native::document_doctype(source).expect("DTD exists");
    assert!(crate::native::node_unlink_child(source, doctype));
    assert_eq!(crate::native::node_first_child(reference), None);
    assert_eq!(crate::native::node_last_child(reference), None);
    assert_eq!(crate::native::node_child_count(reference), 0);

    unsafe {
        crate::native::node_free(imported);
        crate::native::node_free(clone);
        crate::native::node_free(doctype);
        crate::native::document_free(target);
        crate::native::document_free(source);
    }
}

/// Verifies legacy adoption clears declaration links before libxml2 changes documents.
#[test]
fn legacy_entity_reference_adoption_terminates_and_rebinds_declaration() {
    let source = crate::native::document_parse_xml(
        br#"<!DOCTYPE root [<!ENTITY foo "source">]><root>&foo;</root>"#,
        0,
        None,
        None,
    )
    .expect("source parses")
    .document
    .expect("source exists");
    let target = crate::native::document_parse_xml(
        br#"<!DOCTYPE root [<!ENTITY foo "target">]><root/>"#,
        0,
        None,
        None,
    )
    .expect("target parses")
    .document
    .expect("target exists");
    let root = crate::native::document_element(source).expect("root exists");
    let reference = crate::native::node_first_child(root).expect("reference exists");
    let adopted = crate::native::document_adopt_node(target, reference, false);
    assert_eq!(adopted.error_code, 0);
    assert_eq!(adopted.pointer, Some(reference));
    assert_eq!(crate::native::node_document(reference), Some(target));
    assert_eq!(
        crate::native::node_type(
            crate::native::node_first_child(reference)
                .expect("target declaration is rebound"),
        ),
        17
    );

    unsafe {
        crate::native::node_free(reference);
        crate::native::document_free(target);
        crate::native::document_free(source);
    }
}

/// Verifies legacy namespace declaration removal is confined to the owning subtree.
#[test]
fn legacy_remove_attribute_ns_eliminates_local_namespace_declaration() {
    let document = crate::native::document_parse_xml(
        br#"<container><child1 xmlns:x="urn:x"><x:foo x:bar=""/></child1><child2 xmlns:x="urn:x"><x:foo x:bar=""/></child2></container>"#,
        0,
        None,
        None,
    )
    .expect("document parses")
    .document
    .expect("document exists");
    let root = crate::native::document_element(document).expect("root exists");
    let child = crate::native::element_first_child(root).expect("first child exists");

    assert_eq!(
        crate::native::element_remove_attribute_ns(
            child,
            Some(b"urn:x"),
            b"x",
            true,
        ),
        None,
    );
    assert_eq!(
        crate::native::document_serialize(document, None, false, 0, 0),
        Some(
            br#"<?xml version="1.0"?>
<container><child1><foo bar=""/></child1><child2 xmlns:x="urn:x"><x:foo x:bar=""/></child2></container>
"#
            .to_vec(),
        ),
    );

    unsafe {
        crate::native::document_free(document);
    }
}
