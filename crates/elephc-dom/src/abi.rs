//! Purpose:
//! Defines the fixed-width versioned C records shared by generated code and the DOM bridge.
//! Centralizes status, value, diagnostic, and host-callback tags without exposing Rust layouts.
//!
//! Called from:
//! - `crate::exports` for every exported ABI entry point.
//! - `crate::request` for bounds and tag validation.
//!
//! Key details:
//! - Every record is `repr(C)` and contains only fixed-width scalars, pointers, or C function pointers.
//! - Pointer-bearing result records remain valid only until their matching result ID is released.

use std::ffi::c_void;

/// Current Elephc DOM bridge ABI version.
pub const ABI_VERSION: u32 = 1;
/// Request flags marker whose low 31 bits carry the public root-argument count.
pub const REQUEST_FLAG_ARGUMENT_COUNT: u32 = 1 << 31;

/// Successful bridge operation status.
pub const STATUS_OK: u32 = 0;
/// Catchable PHP exception status.
pub const STATUS_THROW: u32 = 1;
/// Uncatchable PHP fatal status.
pub const STATUS_FATAL: u32 = 2;
/// Malformed request, stale handle, or ABI-contract violation status.
pub const STATUS_ABI_ERROR: u32 = 3;
/// Contained Rust/native panic status.
pub const STATUS_INTERNAL_PANIC: u32 = 4;
/// Readable ABI input whose required fields or nested records are malformed.
pub const STATUS_MALFORMED_REQUEST: u32 = 5;

/// PHP error-kind discriminator for a catchable `DOMException`.
pub const PHP_ERROR_KIND_DOM_EXCEPTION: u32 = 1;
/// PHP error-kind discriminator for a catchable `ValueError`.
pub const PHP_ERROR_KIND_VALUE_ERROR: u32 = 2;
/// PHP error-kind discriminator for a catchable base `Error`.
pub const PHP_ERROR_KIND_ERROR: u32 = 3;
/// PHP error-kind discriminator for a catchable base `Exception`.
pub const PHP_ERROR_KIND_EXCEPTION: u32 = 4;
/// PHP error-kind discriminator for a catchable `TypeError`.
pub const PHP_ERROR_KIND_TYPE_ERROR: u32 = 5;
/// PHP error-kind discriminator for a host callback Throwable already stored by the runtime.
pub const PHP_ERROR_KIND_PENDING_HOST_THROWABLE: u32 = 6;

/// Null value tag.
pub const VALUE_NULL: u32 = 0;
/// Boolean value tag.
pub const VALUE_BOOL: u32 = 1;
/// Signed integer value tag.
pub const VALUE_INT: u32 = 2;
/// IEEE-754 floating-point bit-pattern value tag.
pub const VALUE_FLOAT: u32 = 3;
/// Byte-string range value tag.
pub const VALUE_BYTES: u32 = 4;
/// Indexed value-range tag.
pub const VALUE_ARRAY: u32 = 5;
/// Alternating key/value range tag.
pub const VALUE_MAP: u32 = 6;
/// Opaque retained host value handle tag.
pub const VALUE_HOST_HANDLE: u32 = 7;
/// Generation-checked bridge object handle tag.
pub const VALUE_BRIDGE_HANDLE: u32 = 8;
/// Opaque PHP callable handle tag.
pub const VALUE_CALLABLE: u32 = 9;
/// Opaque PHP resource handle tag.
pub const VALUE_RESOURCE: u32 = 10;
/// PHP value-object descriptor backed by a flat field range.
pub const VALUE_OBJECT: u32 = 11;
/// Internal SimpleXML offset marker for PHP's empty append syntax (`[]`).
///
/// This is never a PHP value: the compiler emits it only for the first argument
/// of the locked SimpleXML dimension handlers.
pub const VALUE_SIMPLEXML_APPEND: u32 = 12;

/// Diagnostic message bytes require the compiler to add PHP call-site context.
pub const DIAGNOSTIC_FLAG_CALLSITE_CONTEXT: u32 = 1;
/// Diagnostic message bytes require only PHP call-site file and line decoration.
pub const DIAGNOSTIC_FLAG_CALLSITE_LOCATION: u32 = 1 << 1;

/// Internal ABI health-check opcode used by bridge conformance tests.
pub const OPCODE_ABI_PING: u32 = 1;

/// Host callback opcode retaining one PHP callable descriptor for native state.
pub const HOST_OPCODE_RETAIN_CALLABLE: u32 = 1;
/// Host callback opcode releasing one previously retained PHP callable descriptor.
pub const HOST_OPCODE_RELEASE_CALLABLE: u32 = 2;
/// Host callback opcode invoking the current PHP external-entity loader.
pub const HOST_OPCODE_INVOKE_EXTERNAL_ENTITY_LOADER: u32 = 3;
/// Host callback opcode releasing one leased PHP callback result.
pub const HOST_OPCODE_RELEASE_RESULT: u32 = 4;
/// Host callback opcode reading one bounded chunk from a leased PHP stream.
pub const HOST_OPCODE_READ_STREAM: u32 = 5;
/// Host callback opcode opening one PHP stream path for native document I/O.
pub const HOST_OPCODE_OPEN_STREAM: u32 = 6;
/// Host callback opcode writing one bounded chunk to a leased PHP stream.
pub const HOST_OPCODE_WRITE_STREAM: u32 = 7;
/// Host callback opcode flushing one leased PHP stream before close.
pub const HOST_OPCODE_FLUSH_STREAM: u32 = 8;
/// Host callback opcode emitting one already formatted suppressible PHP warning.
pub const HOST_OPCODE_EMIT_WARNING: u32 = 9;
/// Host callback opcode invoking one retained XPath callable descriptor.
pub const HOST_OPCODE_INVOKE_XPATH_CALLBACK: u32 = 10;
/// Host callback opcode resolving one PHP callable name to its runtime descriptor.
pub const HOST_OPCODE_RESOLVE_XPATH_CALLABLE: u32 = 11;

/// Flat request prefix followed by `value_count` values and `byte_count` bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestHeader {
    pub abi_version: u32,
    pub header_size: u32,
    pub opcode: u32,
    pub flags: u32,
    pub receiver: u64,
    pub value_count: u64,
    pub byte_count: u64,
}

/// One typed input/output value in a flat ABI message.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Value {
    pub tag: u32,
    pub flags: u32,
    pub payload0: u64,
    pub payload1: u64,
}

/// One ordered warning/deprecation/libxml diagnostic.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: u32,
    pub domain: u32,
    pub code: i32,
    /// Diagnostic flags; unknown bits are rejected by the compiler boundary.
    pub reserved: u32,
    pub line: u64,
    pub column: u64,
    pub message_offset: u64,
    pub message_len: u64,
    pub file_offset: u64,
    pub file_len: u64,
}

/// Fixed result record written by `elephc_dom_call`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ResultHeader {
    pub abi_version: u32,
    pub struct_size: u32,
    pub status: u32,
    pub value_tag: u32,
    pub php_error_kind: u32,
    pub dom_exception_code: i32,
    pub result_id: u64,
    pub payload0: u64,
    pub payload1: u64,
    pub bytes_ptr: *const u8,
    pub bytes_len: u64,
    pub values_ptr: *const Value,
    pub values_len: u64,
    pub diagnostics_ptr: *const Diagnostic,
    pub diagnostics_len: u64,
}

impl ResultHeader {
    /// Returns a pointer-free ABI-error result suitable for initializing an output slot.
    pub(crate) fn abi_error() -> Self {
        Self {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            status: STATUS_ABI_ERROR,
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

    /// Returns a pointer-free internal-panic result for a contained unwind.
    pub(crate) fn internal_panic() -> Self {
        let mut result = Self::abi_error();
        result.status = STATUS_INTERNAL_PANIC;
        result
    }
}

/// Generic host-call function used for streams, entities, callbacks, diagnostics, and retains.
pub type HostCall = unsafe extern "C" fn(
    user_data: *mut c_void,
    request_ptr: *const u8,
    request_len: u64,
    out_result: *mut ResultHeader,
) -> u32;

/// Versioned host callback table copied into one bridge execution context.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostVTable {
    pub abi_version: u32,
    pub struct_size: u32,
    pub user_data: *mut c_void,
    pub call: Option<HostCall>,
}

/// Sentinel stored in `DomClassMetadataEntry::parent_class_id` for parentless classes.
pub const DOM_CLASS_NO_PARENT: u64 = u64::MAX;

/// One compiler-emitted PHP class metadata row consumed by the DOM bridge.
///
/// The compiler emits a contiguous table of these rows so the bridge can resolve
/// class-name strings, walk parent chains, and reject abstract classes while
/// validating `registerNodeClass()`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DomClassMetadataEntry {
    /// Pointer to canonical PHP class-name bytes, without a trailing NUL byte.
    pub name_ptr: *const u8,
    /// Length of the class-name byte sequence.
    pub name_len: u64,
    /// Runtime class id assigned by the compiler.
    pub class_id: u64,
    /// Parent class id, or `DOM_CLASS_NO_PARENT` for a parentless class.
    pub parent_class_id: u64,
    /// Non-zero when the class is declared abstract.
    pub is_abstract: u32,
    /// Reserved for future ABI revisions and required to be zero.
    pub reserved: u32,
}
