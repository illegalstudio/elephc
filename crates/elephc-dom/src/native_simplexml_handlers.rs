//! Purpose:
//! Wraps native libxml2 helpers used exclusively by SimpleXML object handlers.
//! Keeps native pointer layouts and allocation ownership out of Rust dispatch code.
//!
//! Called from:
//! - `crate::dispatch::simplexml::handlers` for the 12 locked object-handler routes.
//!
//! Key details:
//! - Optional byte slices use nullable pointer/length pairs and preserve embedded NUL bytes.
//! - Owned native string buffers are copied and released before returning to dispatch.

use std::ffi::c_void;

/// Native pointer result with an optional error code.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativePointerResult {
    pointer: *mut c_void,
    error_code: i32,
    reserved: i32,
}

/// Native owned byte buffer with an optional error code.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeBufferResult {
    pointer: *mut u8,
    length: usize,
    error_code: i32,
    reserved: i32,
}

/// Native 32-bit scalar with explicit ABI padding.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeIntResult {
    value: i32,
    reserved: i32,
}

/// Rust-owned bytes copied from one native handler result.
pub(crate) struct BufferOutcome {
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) error_code: i32,
}

unsafe extern "C" {
    fn elephc_dom_native_buffer_free(pointer: *mut u8);
    fn elephc_dom_native_simplexml_handler_view_first(
        node: *mut c_void,
        iter_type: i32,
        iter_name: *const u8,
        iter_name_length: usize,
        namespace_or_prefix: *const u8,
        namespace_or_prefix_length: usize,
        is_prefix: i32,
    ) -> NativePointerResult;
    fn elephc_dom_native_simplexml_handler_view_offset(
        node: *mut c_void,
        iter_type: i32,
        iter_name: *const u8,
        iter_name_length: usize,
        namespace_or_prefix: *const u8,
        namespace_or_prefix_length: usize,
        is_prefix: i32,
        offset: i64,
    ) -> NativePointerResult;
    fn elephc_dom_native_simplexml_handler_view_count(
        node: *mut c_void,
        iter_type: i32,
        iter_name: *const u8,
        iter_name_length: usize,
        namespace_or_prefix: *const u8,
        namespace_or_prefix_length: usize,
        is_prefix: i32,
    ) -> NativeIntResult;
    fn elephc_dom_native_simplexml_handler_view_attribute(
        node: *mut c_void,
        iter_type: i32,
        iter_name: *const u8,
        iter_name_length: usize,
        namespace_or_prefix: *const u8,
        namespace_or_prefix_length: usize,
        is_prefix: i32,
        attribute_name: *const u8,
        attribute_name_length: usize,
    ) -> NativePointerResult;
    fn elephc_dom_native_simplexml_handler_selected_is_empty(
        node: *mut c_void,
    ) -> NativeIntResult;
    fn elephc_dom_native_simplexml_handler_unlink_node(node: *mut c_void);
    fn elephc_dom_native_simplexml_handler_set_node_text(
        node: *mut c_void,
        value: *const u8,
        value_length: usize,
    ) -> NativeIntResult;
    fn elephc_dom_native_simplexml_handler_compare(
        node1: *mut c_void,
        node2: *mut c_void,
        doc1: *mut c_void,
        doc2: *mut c_void,
    ) -> NativeIntResult;
    fn elephc_dom_native_simplexml_handler_cast_string(
        document: *mut c_void,
        node: *mut c_void,
        iter_type: i32,
    ) -> NativeBufferResult;
    fn elephc_dom_native_simplexml_handler_cast_bool(
        document: *mut c_void,
        node: *mut c_void,
        iter_type: i32,
        iter_name: *const u8,
        iter_name_length: usize,
        namespace_or_prefix: *const u8,
        namespace_or_prefix_length: usize,
        is_prefix: i32,
    ) -> NativeIntResult;
    fn elephc_dom_native_simplexml_handler_write_dimension_attr(
        document: *mut c_void,
        node: *mut c_void,
        name: *const u8,
        name_length: usize,
        value: *const u8,
        value_length: usize,
        iter_type: i32,
    ) -> NativeIntResult;
}

/// Converts an optional byte slice into one nullable native pointer/length pair.
fn optional_bytes_pointer(bytes: Option<&[u8]>) -> (*const u8, usize) {
    bytes
        .map(|bytes| (bytes.as_ptr(), bytes.len()))
        .unwrap_or((std::ptr::null(), 0))
}

/// Copies and releases one optional native-owned byte buffer.
fn owned_bytes(pointer: *mut u8, length: usize) -> Option<Vec<u8>> {
    if pointer.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec();
    unsafe {
        elephc_dom_native_buffer_free(pointer);
    }
    Some(bytes)
}

/// Resolves the first node selected by one immutable SimpleXML view.
pub(crate) fn view_first(
    node: usize,
    iter_type: i32,
    iter_name: Option<&[u8]>,
    namespace_or_prefix: Option<&[u8]>,
    is_prefix: bool,
) -> Option<usize> {
    let (iter_name, iter_name_length) = optional_bytes_pointer(iter_name);
    let (namespace_or_prefix, namespace_or_prefix_length) =
        optional_bytes_pointer(namespace_or_prefix);
    let result = unsafe {
        elephc_dom_native_simplexml_handler_view_first(
            node as *mut c_void,
            iter_type,
            iter_name,
            iter_name_length,
            namespace_or_prefix,
            namespace_or_prefix_length,
            i32::from(is_prefix),
        )
    };
    (result.error_code == 0 && !result.pointer.is_null())
        .then_some(result.pointer as usize)
}

/// Resolves one zero-based member selected by a SimpleXML view.
pub(crate) fn view_offset(
    node: usize,
    iter_type: i32,
    iter_name: Option<&[u8]>,
    namespace_or_prefix: Option<&[u8]>,
    is_prefix: bool,
    offset: i64,
) -> Option<usize> {
    let (iter_name, iter_name_length) = optional_bytes_pointer(iter_name);
    let (namespace_or_prefix, namespace_or_prefix_length) =
        optional_bytes_pointer(namespace_or_prefix);
    let result = unsafe {
        elephc_dom_native_simplexml_handler_view_offset(
            node as *mut c_void,
            iter_type,
            iter_name,
            iter_name_length,
            namespace_or_prefix,
            namespace_or_prefix_length,
            i32::from(is_prefix),
            offset,
        )
    };
    (result.error_code == 0 && !result.pointer.is_null())
        .then_some(result.pointer as usize)
}

/// Counts every live member selected by one SimpleXML view.
pub(crate) fn view_count(
    node: usize,
    iter_type: i32,
    iter_name: Option<&[u8]>,
    namespace_or_prefix: Option<&[u8]>,
    is_prefix: bool,
) -> i32 {
    let (iter_name, iter_name_length) = optional_bytes_pointer(iter_name);
    let (namespace_or_prefix, namespace_or_prefix_length) =
        optional_bytes_pointer(namespace_or_prefix);
    unsafe {
        elephc_dom_native_simplexml_handler_view_count(
            node as *mut c_void,
            iter_type,
            iter_name,
            iter_name_length,
            namespace_or_prefix,
            namespace_or_prefix_length,
            i32::from(is_prefix),
        )
        .value
    }
}

/// Resolves one named attribute selected through a SimpleXML view.
pub(crate) fn view_attribute(
    node: usize,
    iter_type: i32,
    iter_name: Option<&[u8]>,
    namespace_or_prefix: Option<&[u8]>,
    is_prefix: bool,
    attribute_name: &[u8],
) -> Option<usize> {
    let (iter_name, iter_name_length) = optional_bytes_pointer(iter_name);
    let (namespace_or_prefix, namespace_or_prefix_length) =
        optional_bytes_pointer(namespace_or_prefix);
    let result = unsafe {
        elephc_dom_native_simplexml_handler_view_attribute(
            node as *mut c_void,
            iter_type,
            iter_name,
            iter_name_length,
            namespace_or_prefix,
            namespace_or_prefix_length,
            i32::from(is_prefix),
            attribute_name.as_ptr(),
            attribute_name.len(),
        )
    };
    (result.error_code == 0 && !result.pointer.is_null())
        .then_some(result.pointer as usize)
}

/// Reports PHP's `empty()` result for one selected element or attribute.
pub(crate) fn selected_is_empty(node: usize) -> bool {
    unsafe {
        elephc_dom_native_simplexml_handler_selected_is_empty(node as *mut c_void)
            .value
            != 0
    }
}

/// Unlinks one selected node while extant wrappers retain the document graph.
pub(crate) fn unlink_node(node: usize) {
    unsafe {
        elephc_dom_native_simplexml_handler_unlink_node(node as *mut c_void);
    }
}

/// Replaces one selected node's complete text content.
pub(crate) fn set_node_text(node: usize, value: &[u8]) -> i32 {
    unsafe {
        elephc_dom_native_simplexml_handler_set_node_text(
            node as *mut c_void,
            value.as_ptr(),
            value.len(),
        )
        .value
    }
}

/// Compares two SimpleXML identities by php-src node/document pointer rules.
pub(crate) fn compare(
    node1: usize,
    node2: usize,
    doc1: usize,
    doc2: usize,
) -> i32 {
    unsafe {
        elephc_dom_native_simplexml_handler_compare(
            node1 as *mut c_void,
            node2 as *mut c_void,
            doc1 as *mut c_void,
            doc2 as *mut c_void,
        )
        .value
    }
}

/// Applies PHP's native boolean cast to one complete immutable SimpleXML view.
pub(crate) fn cast_bool(
    document: usize,
    node: usize,
    iter_type: i32,
    iter_name: Option<&[u8]>,
    namespace_or_prefix: Option<&[u8]>,
    is_prefix: bool,
) -> bool {
    let (iter_name, iter_name_length) = optional_bytes_pointer(iter_name);
    let (namespace_or_prefix, namespace_or_prefix_length) =
        optional_bytes_pointer(namespace_or_prefix);
    unsafe {
        elephc_dom_native_simplexml_handler_cast_bool(
            document as *mut c_void,
            node as *mut c_void,
            iter_type,
            iter_name,
            iter_name_length,
            namespace_or_prefix,
            namespace_or_prefix_length,
            i32::from(is_prefix),
        )
        .value
            != 0
    }
}

/// Extracts the PHP string value for one already selected node.
pub(crate) fn cast_string(
    document: usize,
    node: usize,
    iter_type: i32,
) -> BufferOutcome {
    let result = unsafe {
        elephc_dom_native_simplexml_handler_cast_string(
            document as *mut c_void,
            node as *mut c_void,
            iter_type,
        )
    };
    BufferOutcome {
        bytes: owned_bytes(result.pointer, result.length),
        error_code: result.error_code,
    }
}

/// Replaces or creates one named attribute's scalar text.
pub(crate) fn write_dimension_attribute(
    document: usize,
    node: usize,
    name: &[u8],
    value: &[u8],
    iter_type: i32,
) -> i32 {
    unsafe {
        elephc_dom_native_simplexml_handler_write_dimension_attr(
            document as *mut c_void,
            node as *mut c_void,
            name.as_ptr(),
            name.len(),
            value.as_ptr(),
            value.len(),
            iter_type,
        )
        .value
    }
}
