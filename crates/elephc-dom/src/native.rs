//! Purpose:
//! Wraps the pinned libxml2 and PHP-bundled Lexbor engine entry points.
//! Keeps native pointers and engine-specific types out of the stable public bridge ABI.
//!
//! Called from:
//! - DOM bridge operation handlers and focused native-engine tests.
//!
//! Key details:
//! - Byte slices retain their explicit lengths, including when they contain NUL bytes.
//! - Native parse probes return typed outcomes instead of exposing C status constants.

use std::ffi::{c_char, c_void, CStr};

use crate::objects::LibxmlErrorObject;

/// Native pointer-plus-length record returned by engine-owned byte buffers.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeBuffer {
    pointer: *mut u8,
    length: usize,
}

/// Native owned byte buffer with an optional PHP DOMException code.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeBufferResult {
    pointer: *mut u8,
    length: usize,
    error_code: i32,
    reserved: i32,
}

/// Native pointer result with an optional PHP DOMException code.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativePointerResult {
    pointer: *mut c_void,
    error_code: i32,
    reserved: i32,
}

/// Native structured-error record returned by the pinned libxml2 adapter.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeError {
    level: i32,
    domain: i32,
    code: i32,
    line: i32,
    column: i32,
    reserved: i32,
    message: *mut u8,
    message_length: usize,
    file: *mut u8,
    file_length: usize,
}

/// Signals that the native fault-injection bridge reached the allocation-failure branch.
#[cfg(test)]
pub(crate) const TEST_RESOURCE_LOADER_INPUT_CREATION_FAILED: i32 = 1;

/// Native XML parse outcome with independently freed structured errors.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeParseResult {
    document: *mut c_void,
    errors: *mut NativeError,
    error_count: usize,
    allocation_failed: i32,
    status: i32,
}

/// Native document-validation outcome with independently freed structured errors.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeValidationResult {
    errors: *mut NativeError,
    error_count: usize,
    allocation_failed: i32,
    valid: i32,
    status: i32,
    host_status: i32,
}

/// Native XInclude outcome with destroyed-node and structured-error ownership.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeXIncludeResult {
    errors: *mut NativeError,
    error_count: usize,
    invalidated: *mut *mut c_void,
    invalidated_count: usize,
    allocation_failed: i32,
    substitutions: i32,
    host_status: i32,
}

/// Native borrowed pointer-plus-length input for C14N namespace data.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeBytes {
    pointer: *const u8,
    length: usize,
}

/// Native C14N outcome with independently owned output and diagnostic buffers.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeC14nResult {
    bytes: *mut u8,
    length: usize,
    errors: *mut NativeError,
    error_count: usize,
    allocation_failed: i32,
    status: i32,
}

/// Native XPath outcome carrying exactly one scalar or node-set representation.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeXPathResult {
    pointers: *mut *mut c_void,
    pointer_count: usize,
    bytes: *mut u8,
    byte_count: usize,
    errors: *mut NativeError,
    error_count: usize,
    callback_leases: *mut u64,
    callback_lease_count: usize,
    number: f64,
    allocation_failed: i32,
    kind: i32,
    boolean_value: i32,
    status: i32,
    host_status: i32,
}

/// Native selector outcome with independently owned pointer and diagnostic buffers.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeSelectorResult {
    pointers: *mut *mut c_void,
    count: usize,
    matched: i32,
    error_code: i32,
    message: *mut u8,
    message_length: usize,
}

/// One borrowed namespace-info record returned by the native tree adapter.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeNamespaceInfo {
    element: *mut c_void,
    prefix: *const u8,
    prefix_length: usize,
    namespace_uri: *const u8,
    namespace_uri_length: usize,
}

/// Native namespace-info outcome whose flat item allocation is released after copying.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeNamespaceInfoResult {
    items: *mut NativeNamespaceInfo,
    count: usize,
    allocation_failed: i32,
    reserved: i32,
}

/// One owned prefix/URI pair inside a native SimpleXML namespace result.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeSimpleXmlNamespace {
    prefix: *const u8,
    prefix_length: usize,
    namespace_uri: *const u8,
    namespace_uri_length: usize,
}

/// Native SimpleXML namespace result whose item and byte allocations are released together.
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeSimpleXmlNamespaceResult {
    items: *mut NativeSimpleXmlNamespace,
    count: usize,
    allocation_failed: i32,
    reserved: i32,
}

/// Owned Rust parse outcome after copying and releasing every native error buffer.
pub(crate) struct DocumentParseOutcome {
    pub(crate) document: Option<usize>,
    pub(crate) errors: Vec<LibxmlErrorObject>,
    pub(crate) host_status: i32,
}

/// Owned result of parsing and appending one XML balanced chunk.
pub(crate) struct FragmentAppendOutcome {
    pub(crate) appended: bool,
    pub(crate) errors: Vec<LibxmlErrorObject>,
}

/// Owned result of DTD, XML Schema, or Relax NG document validation.
pub(crate) struct ValidationOutcome {
    pub(crate) valid: bool,
    pub(crate) errors: Vec<LibxmlErrorObject>,
    pub(crate) status: i32,
    pub(crate) host_status: i32,
}

/// Owned XInclude result after native buffers have been copied and released.
pub(crate) struct XIncludeOutcome {
    pub(crate) substitutions: i32,
    pub(crate) errors: Vec<LibxmlErrorObject>,
    pub(crate) invalidated: Vec<usize>,
    pub(crate) allocation_failed: bool,
    pub(crate) host_status: i32,
}

/// Rust-owned canonicalization result after every native allocation is released.
pub(crate) struct C14nOutcome {
    pub(crate) bytes: Vec<u8>,
    pub(crate) errors: Vec<LibxmlErrorObject>,
    pub(crate) status: i32,
}

/// Rust-owned XPath value after every temporary native allocation is released.
pub(crate) enum XPathValue {
    Nodes(Vec<usize>),
    Boolean(bool),
    Number(f64),
    Bytes(Vec<u8>),
    Null,
}

/// Rust-owned XPath evaluation result plus ordered libxml diagnostics.
pub(crate) struct XPathOutcome {
    pub(crate) value: XPathValue,
    pub(crate) errors: Vec<LibxmlErrorObject>,
    pub(crate) status: i32,
    pub(crate) host_status: i32,
    pub(crate) callback_error: Vec<u8>,
    _callback_leases: Vec<crate::host::LeasedHostResult>,
}

impl XPathOutcome {
    /// Transfers callback-result leases to a caller that can release them outside a context borrow.
    pub(crate) fn take_callback_leases(
        &mut self,
    ) -> Vec<crate::host::LeasedHostResult> {
        std::mem::take(&mut self._callback_leases)
    }
}

/// Failure classifications specific to HTML parsing before a document exists.
pub(crate) enum HtmlParseError {
    InvalidEncoding,
    Allocation,
}

/// One native pointer outcome whose nonzero error code names a DOMException.
pub(crate) struct PointerOutcome {
    pub(crate) pointer: Option<usize>,
    pub(crate) error_code: i32,
}

/// One native owned byte outcome whose positive error code names a DOMException.
pub(crate) struct BufferOutcome {
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) error_code: i32,
}

/// Rust-owned selector result after all temporary native allocations are released.
pub(crate) struct SelectorOutcome {
    pub(crate) pointers: Vec<usize>,
    pub(crate) matched: bool,
    pub(crate) error_code: i32,
    pub(crate) message: Vec<u8>,
}

/// One copied `Dom\NamespaceInfo` payload tied to a canonical element pointer.
pub(crate) struct NamespaceInfo {
    pub(crate) element: usize,
    pub(crate) prefix: Option<Vec<u8>>,
    pub(crate) namespace_uri: Option<Vec<u8>>,
}

/// Rust-owned SimpleXML prefix-to-URI rows copied from one native result.
pub(crate) struct SimpleXmlNamespaceOutcome {
    pub(crate) items: Vec<(Vec<u8>, Vec<u8>)>,
}

unsafe extern "C" {
    fn elephc_dom_native_libxml_version() -> u32;
    fn elephc_dom_native_libxml_version_string() -> *const c_char;
    fn elephc_dom_native_lexbor_version_string() -> *const c_char;
    fn elephc_dom_native_validate_name(name: *const u8, name_length: usize) -> i32;
    fn elephc_dom_native_validate_ncname(name: *const u8, name_length: usize) -> i32;
    fn elephc_dom_native_validate_qname(name: *const u8, name_length: usize) -> i32;
    fn elephc_dom_native_parse_xml(bytes: *const u8, length: usize) -> i32;
    fn elephc_dom_native_parse_html(bytes: *const u8, length: usize) -> i32;
    /// Reports whether libxml2 recognizes one document encoding label.
    fn elephc_dom_native_encoding_is_valid(
        encoding: *const u8,
        encoding_length: usize,
    ) -> i32;
    /// Reports whether Lexbor recognizes one WHATWG encoding label.
    fn elephc_dom_native_html_encoding_is_valid(
        encoding: *const u8,
        encoding_length: usize,
    ) -> i32;
    fn elephc_dom_native_document_new(
        version: *const u8,
        version_length: usize,
        encoding: *const u8,
        encoding_length: usize,
    ) -> *mut c_void;
    /// Creates PHP's initial modern HTML document tree.
    fn elephc_dom_native_document_new_html(
        title: *const u8,
        title_length: usize,
    ) -> *mut c_void;
    /// Parses one XML byte stream through libxml2 with an optional source name.
    fn elephc_dom_native_document_parse_xml(
        bytes: *const u8,
        length: usize,
        options: i32,
        override_encoding: *const u8,
        override_encoding_length: usize,
        input_name: *const u8,
        input_name_length: usize,
        host_context: u64,
    ) -> NativeParseResult;
    /// Forces the stream-backed resource-loader input allocation to fail for one test call.
    #[cfg(test)]
    fn elephc_dom_native_test_resource_loader_input_from_io_failure(
        host_context: u64,
    ) -> i32;
    /// Parses one named legacy HTML byte stream through libxml2's HTML4 parser.
    fn elephc_dom_native_document_parse_html4(
        bytes: *const u8,
        length: usize,
        options: i32,
        input_name: *const u8,
        input_name_length: usize,
    ) -> NativeParseResult;
    /// Parses one modern HTML5 byte stream through pinned Lexbor.
    fn elephc_dom_native_document_parse_html5(
        bytes: *const u8,
        length: usize,
        options: u32,
        override_encoding: *const u8,
        override_encoding_length: usize,
        input_name: *const u8,
        input_name_length: usize,
    ) -> NativeParseResult;
    /// Parses one well-formed XML fragment using an element's in-scope namespaces.
    fn elephc_dom_native_parse_xml_fragment(
        context: *mut c_void,
        input: *const u8,
        input_length: usize,
    ) -> NativePointerResult;
    /// Parses one UTF-8 HTML fragment using Lexbor's context-element algorithm.
    fn elephc_dom_native_parse_html_fragment(
        context: *mut c_void,
        input: *const u8,
        input_length: usize,
    ) -> *mut c_void;
    /// Converts parsed namespace declarations into PHP modern-DOM attributes.
    fn elephc_dom_native_document_convert_modern_xml(document: *mut c_void) -> i32;
    /// Validates one document against its DTD.
    fn elephc_dom_native_document_validate(
        document: *mut c_void,
    ) -> NativeValidationResult;
    /// Performs XInclude while reporting every native node PHP must invalidate.
    fn elephc_dom_native_document_xinclude(
        document: *mut c_void,
        flags: i32,
        generic_errors: i32,
        host_context: u64,
    ) -> NativeXIncludeResult;
    /// Canonicalizes one document or node with optional XPath and namespace inputs.
    fn elephc_dom_native_node_c14n(
        document: *mut c_void,
        node: *mut c_void,
        node_is_document: i32,
        modern: i32,
        exclusive: i32,
        with_comments: i32,
        has_xpath: i32,
        query: *const u8,
        query_length: usize,
        namespace_prefixes: *const NativeBytes,
        namespace_uris: *const NativeBytes,
        namespace_count: usize,
        inclusive_prefixes: *const NativeBytes,
        inclusive_prefix_count: usize,
        generic_errors: i32,
    ) -> NativeC14nResult;
    /// Evaluates one XPath expression with persistent and context-node namespaces.
    fn elephc_dom_native_xpath_evaluate(
        document: *mut c_void,
        node: *mut c_void,
        modern: i32,
        register_node_namespaces: i32,
        force_nodeset: i32,
        expression: *const u8,
        expression_length: usize,
        namespace_prefixes: *const NativeBytes,
        namespace_uris: *const NativeBytes,
        namespace_count: usize,
        host_context: u64,
        xpath_handle: u64,
        callback_namespaces: *const NativeBytes,
        callback_names: *const NativeBytes,
        callback_count: usize,
    ) -> NativeXPathResult;
    /// Parses and applies one in-memory W3C XML Schema.
    fn elephc_dom_native_document_schema_validate_source(
        document: *mut c_void,
        source: *const u8,
        source_length: usize,
        flags: i32,
        generic_errors: i32,
        host_context: u64,
    ) -> NativeValidationResult;
    /// Loads and applies one W3C XML Schema file or stream URL.
    fn elephc_dom_native_document_schema_validate_file(
        document: *mut c_void,
        path: *const u8,
        path_length: usize,
        flags: i32,
        generic_errors: i32,
        host_context: u64,
    ) -> NativeValidationResult;
    /// Parses and applies one in-memory Relax NG grammar.
    fn elephc_dom_native_document_relaxng_validate_source(
        document: *mut c_void,
        source: *const u8,
        source_length: usize,
        generic_errors: i32,
        host_context: u64,
    ) -> NativeValidationResult;
    /// Loads and applies one Relax NG file or stream URL.
    fn elephc_dom_native_document_relaxng_validate_file(
        document: *mut c_void,
        path: *const u8,
        path_length: usize,
        generic_errors: i32,
        host_context: u64,
    ) -> NativeValidationResult;
    fn elephc_dom_native_parse_result_free(errors: *mut NativeError, error_count: usize);
    /// Releases both owned arrays returned by the native XInclude adapter.
    fn elephc_dom_native_xinclude_result_free(
        errors: *mut NativeError,
        error_count: usize,
        invalidated: *mut *mut c_void,
    );
    /// Releases one native C14N byte buffer and its structured errors.
    fn elephc_dom_native_c14n_result_free(
        bytes: *mut u8,
        errors: *mut NativeError,
        error_count: usize,
    );
    /// Releases every allocation returned by the native XPath adapter.
    fn elephc_dom_native_xpath_result_free(
        pointers: *mut *mut c_void,
        bytes: *mut u8,
        errors: *mut NativeError,
        error_count: usize,
        callback_leases: *mut u64,
    );
    fn elephc_dom_native_document_free(document: *mut c_void);
    /// Returns the modern HTML document's no-, full-, or limited-quirks mode.
    fn elephc_dom_native_html_document_quirks_mode(
        document: *mut c_void,
    ) -> i32;
    /// Evaluates one parsed selector as query-first, query-all, matches, or closest.
    fn elephc_dom_native_selector_query(
        root: *mut c_void,
        input: *const u8,
        input_length: usize,
        operation: i32,
        quirks: i32,
    ) -> NativeSelectorResult;
    /// Releases both flat allocations returned by one native selector query.
    fn elephc_dom_native_selector_result_free(
        pointers: *mut *mut c_void,
        message: *mut u8,
    );
    /// Serializes one document with the selected family mode and XML save flags.
    fn elephc_dom_native_document_serialize(
        document: *mut c_void,
        encoding: *const u8,
        encoding_length: usize,
        format: i32,
        modern: i32,
        options: i32,
    ) -> NativeBuffer;
    /// Serializes one same-document node with the selected family mode and XML save flags.
    fn elephc_dom_native_document_serialize_node(
        document: *mut c_void,
        node: *mut c_void,
        format: i32,
        mode: i32,
        options: i32,
    ) -> NativeBuffer;
    /// Serializes one SimpleXML subnode with php-src's `xmlNodeDump` behavior.
    fn elephc_dom_native_simplexml_serialize_node(
        document: *mut c_void,
        node: *mut c_void,
    ) -> NativeBuffer;
    /// Copies php-src's inline `xmlNodeListGetString()` value for one SimpleXML node.
    fn elephc_dom_native_simplexml_node_list_content(
        node: *mut c_void,
    ) -> NativeBuffer;
    /// Returns libxml2's borrowed raw node name for SimpleXML property projection.
    fn elephc_dom_native_simplexml_node_name(node: *mut c_void) -> NativeBuffer;
    /// Serializes one modern HTML document or same-document node as HTML5.
    fn elephc_dom_native_document_serialize_html5(
        document: *mut c_void,
        node: *mut c_void,
    ) -> NativeBuffer;
    /// Serializes one modern XML element's inner or outer markup.
    fn elephc_dom_native_element_serialize_xml(
        element: *mut c_void,
        inner: i32,
    ) -> NativeBuffer;
    /// Checks one modern XML element serialization against the well-formedness rules.
    fn elephc_dom_native_element_xml_is_well_formed(
        element: *mut c_void,
        inner: i32,
    ) -> i32;
    /// Serializes one modern HTML element's inner or outer markup as UTF-8.
    fn elephc_dom_native_element_serialize_html5(
        element: *mut c_void,
        inner: i32,
    ) -> NativeBuffer;
    /// Serializes one legacy HTML document or same-document node.
    fn elephc_dom_native_document_serialize_html4(
        document: *mut c_void,
        node: *mut c_void,
        format: i32,
    ) -> NativeBuffer;
    fn elephc_dom_native_buffer_free(pointer: *mut u8);
    fn elephc_dom_native_document_version(document: *mut c_void) -> NativeBuffer;
    fn elephc_dom_native_document_encoding(document: *mut c_void) -> NativeBuffer;
    fn elephc_dom_native_document_doctype(document: *mut c_void) -> *mut c_void;
    fn elephc_dom_native_document_type_new(
        qualified_name: *const u8,
        qualified_name_length: usize,
        public_id: *const u8,
        public_id_length: usize,
        system_id: *const u8,
        system_id_length: usize,
    ) -> *mut c_void;
    /// Creates one validated root for an implementation-level document factory.
    fn elephc_dom_native_document_create_implementation_root(
        document: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        qualified_name: *const u8,
        qualified_name_length: usize,
        modern: i32,
    ) -> NativePointerResult;
    /// Attaches or adopts one DTD into a newly created document.
    fn elephc_dom_native_document_attach_doctype(
        document: *mut c_void,
        doctype: *mut c_void,
        allow_adoption: i32,
    ) -> i32;
    fn elephc_dom_native_document_url(document: *mut c_void) -> NativeBuffer;
    fn elephc_dom_native_document_set_url(
        document: *mut c_void,
        url: *const u8,
        url_length: usize,
    ) -> i32;
    fn elephc_dom_native_document_set_version(
        document: *mut c_void,
        version: *const u8,
        version_length: usize,
    ) -> i32;
    fn elephc_dom_native_document_set_encoding(
        document: *mut c_void,
        encoding: *const u8,
        encoding_length: usize,
    ) -> i32;
    /// Replaces one modern document encoding with Lexbor's canonical label.
    fn elephc_dom_native_document_set_modern_encoding(
        document: *mut c_void,
        encoding: *const u8,
        encoding_length: usize,
    ) -> i32;
    fn elephc_dom_native_document_standalone(document: *mut c_void) -> i32;
    fn elephc_dom_native_document_set_standalone(
        document: *mut c_void,
        standalone: i32,
    ) -> i32;
    /// Creates one libxml2 element associated with an authoritative document.
    fn elephc_dom_native_document_create_element(
        document: *mut c_void,
        name: *const u8,
        name_length: usize,
        value: *const u8,
        value_length: usize,
        html: i32,
    ) -> *mut c_void;
    /// Creates one namespaced element or reports its exact DOMException code.
    fn elephc_dom_native_document_create_element_ns(
        document: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        qualified_name: *const u8,
        qualified_name_length: usize,
        value: *const u8,
        value_length: usize,
        modern: i32,
    ) -> NativePointerResult;
    /// Creates one detached unqualified attribute.
    fn elephc_dom_native_document_create_attribute(
        document: *mut c_void,
        name: *const u8,
        name_length: usize,
    ) -> *mut c_void;
    /// Creates one detached namespaced attribute or reports its DOMException code.
    fn elephc_dom_native_document_create_attribute_ns(
        document: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        qualified_name: *const u8,
        qualified_name_length: usize,
        modern: i32,
    ) -> NativePointerResult;
    /// Creates one libxml2 text node associated with an authoritative document.
    fn elephc_dom_native_document_create_text(
        document: *mut c_void,
        value: *const u8,
        value_length: usize,
    ) -> *mut c_void;
    /// Creates one libxml2 CDATA section associated with an authoritative document.
    fn elephc_dom_native_document_create_cdata(
        document: *mut c_void,
        value: *const u8,
        value_length: usize,
    ) -> *mut c_void;
    /// Creates one libxml2 comment associated with an authoritative document.
    fn elephc_dom_native_document_create_comment(
        document: *mut c_void,
        value: *const u8,
        value_length: usize,
    ) -> *mut c_void;
    /// Creates one empty libxml2 document fragment.
    fn elephc_dom_native_document_create_fragment(document: *mut c_void) -> *mut c_void;
    /// Parses one XML balanced chunk and appends its complete node list to a fragment.
    fn elephc_dom_native_fragment_append_xml(
        fragment: *mut c_void,
        input: *const u8,
        input_length: usize,
    ) -> NativeParseResult;
    /// Creates one libxml2 processing instruction.
    fn elephc_dom_native_document_create_pi(
        document: *mut c_void,
        target: *const u8,
        target_length: usize,
        data: *const u8,
        data_length: usize,
    ) -> *mut c_void;
    /// Creates one libxml2 entity-reference node.
    fn elephc_dom_native_document_create_entity_reference(
        document: *mut c_void,
        name: *const u8,
        name_length: usize,
    ) -> *mut c_void;
    /// Returns the root element of one libxml2 document.
    fn elephc_dom_native_document_element(document: *mut c_void) -> *mut c_void;
    /// Returns one modern document's direct HTML head element.
    fn elephc_dom_native_document_head(document: *mut c_void) -> *mut c_void;
    /// Returns one modern document's direct HTML body or frameset element.
    fn elephc_dom_native_document_body(document: *mut c_void) -> *mut c_void;
    /// Returns one modern document's effective HTML or SVG title element.
    fn elephc_dom_native_document_title_element(
        document: *mut c_void,
    ) -> *mut c_void;
    /// Copies one modern document's collapsed title text.
    fn elephc_dom_native_document_title(document: *mut c_void) -> NativeBuffer;
    /// Replaces or creates one modern document's title text.
    fn elephc_dom_native_document_set_title(
        document: *mut c_void,
        value: *const u8,
        value_length: usize,
    ) -> i32;
    /// Reports whether one element is an HTML body or frameset.
    fn elephc_dom_native_node_is_html_body(node: *mut c_void) -> i32;
    /// Reports whether one element belongs to the HTML namespace.
    fn elephc_dom_native_node_is_html_element(node: *mut c_void) -> i32;
    /// Appends one libxml2 node without coalescing wrapper-visible text identities.
    fn elephc_dom_native_node_append_child(
        parent: *mut c_void,
        child: *mut c_void,
    ) -> *mut c_void;
    /// Returns one libxml2 node's parent pointer.
    fn elephc_dom_native_node_parent(node: *mut c_void) -> *mut c_void;
    /// Returns the document pointer currently assigned to one native node.
    fn elephc_dom_native_node_document(node: *mut c_void) -> *mut c_void;
    /// Returns one libxml2 node's parent only when it is an element.
    fn elephc_dom_native_node_parent_element(node: *mut c_void) -> *mut c_void;
    /// Returns one libxml2 node's first child.
    fn elephc_dom_native_node_first_child(node: *mut c_void) -> *mut c_void;
    /// Returns an element or its private template-content fragment.
    fn elephc_dom_native_element_content_container(
        element: *mut c_void,
        ensure: i32,
    ) -> *mut c_void;
    /// Returns one libxml2 node's last child.
    fn elephc_dom_native_node_last_child(node: *mut c_void) -> *mut c_void;
    /// Returns one libxml2 node's previous sibling.
    fn elephc_dom_native_node_previous_sibling(node: *mut c_void) -> *mut c_void;
    /// Returns one libxml2 node's next sibling.
    fn elephc_dom_native_node_next_sibling(node: *mut c_void) -> *mut c_void;
    /// Returns the topmost ancestor of one libxml2 node.
    fn elephc_dom_native_node_root(node: *mut c_void) -> *mut c_void;
    /// Reports whether one libxml2 node has a document ancestor.
    fn elephc_dom_native_node_is_connected(node: *mut c_void) -> i32;
    /// Reports whether one libxml2 node has at least one child.
    fn elephc_dom_native_node_has_children(node: *mut c_void) -> i32;
    /// Reports whether one libxml2 node contains another, including itself.
    fn elephc_dom_native_node_contains(node: *mut c_void, other: *mut c_void) -> i32;
    /// Compares two nodes using PHP's legacy or modern structural-equality rules.
    fn elephc_dom_native_node_is_equal(
        node: *mut c_void,
        other: *mut c_void,
        modern: i32,
    ) -> i32;
    /// Returns the DOM document-position bitmask between two nodes.
    fn elephc_dom_native_node_compare_document_position(
        node: *mut c_void,
        other: *mut c_void,
    ) -> i64;
    /// Unlinks one direct child from its libxml2 parent.
    fn elephc_dom_native_node_unlink_child(
        parent: *mut c_void,
        child: *mut c_void,
    ) -> i32;
    /// Inserts one libxml2 node before a direct reference child, or appends for null.
    fn elephc_dom_native_node_insert_before(
        parent: *mut c_void,
        child: *mut c_void,
        reference: *mut c_void,
    ) -> *mut c_void;
    /// Renames one modern element or attribute with namespace validation.
    fn elephc_dom_native_node_rename(
        node: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        qualified_name: *const u8,
        qualified_name_length: usize,
    ) -> i32;
    /// Replaces one direct libxml2 child and returns the detached prior node.
    fn elephc_dom_native_node_replace_child(
        parent: *mut c_void,
        child: *mut c_void,
        replaced: *mut c_void,
    ) -> *mut c_void;
    /// Returns one libxml2 node's numeric kind.
    fn elephc_dom_native_node_type(node: *mut c_void) -> u32;
    /// Returns libxml2's raw node kind without PHP's DOM normalization.
    fn elephc_dom_native_node_storage_type(node: *mut c_void) -> u32;
    /// Returns an owned buffer containing one node's PHP-compatible qualified name.
    fn elephc_dom_native_node_name(node: *mut c_void) -> NativeBuffer;
    /// Returns an owned buffer containing one node's recursive text content.
    fn elephc_dom_native_node_content(node: *mut c_void) -> NativeBuffer;
    /// Returns one node's borrowed namespace URI.
    fn elephc_dom_native_node_namespace_uri(node: *mut c_void) -> NativeBuffer;
    /// Returns one node's borrowed namespace prefix.
    fn elephc_dom_native_node_prefix(node: *mut c_void) -> NativeBuffer;
    /// Rebinds one legacy element or attribute to a namespace prefix.
    fn elephc_dom_native_node_set_prefix(
        node: *mut c_void,
        prefix: *const u8,
        prefix_length: usize,
    ) -> i32;
    /// Returns one element or attribute's borrowed local name.
    fn elephc_dom_native_node_local_name(node: *mut c_void) -> NativeBuffer;
    /// Returns one libxml2-owned effective base URI.
    fn elephc_dom_native_node_base_uri(node: *mut c_void) -> NativeBuffer;
    /// Returns one libxml2-owned absolute node path.
    fn elephc_dom_native_node_path(node: *mut c_void) -> NativeBuffer;
    /// Returns one node's source line or zero when unavailable.
    fn elephc_dom_native_node_line(node: *mut c_void) -> i64;
    /// Reports whether one node owns attributes.
    fn elephc_dom_native_node_has_attributes(node: *mut c_void) -> i32;
    /// Reports whether one native attribute is typed as an XML ID.
    fn elephc_dom_native_attribute_is_id(node: *mut c_void) -> i32;
    /// Updates one native attribute's XML ID type marker.
    fn elephc_dom_native_attribute_set_is_id(
        node: *mut c_void,
        is_id: i32,
    ) -> i32;
    /// Returns one doctype's declared name.
    fn elephc_dom_native_document_type_name(node: *mut c_void) -> NativeBuffer;
    /// Returns one doctype's public identifier.
    fn elephc_dom_native_document_type_public_id(node: *mut c_void) -> NativeBuffer;
    /// Returns one doctype's system identifier.
    fn elephc_dom_native_document_type_system_id(node: *mut c_void) -> NativeBuffer;
    /// Serializes one doctype's internal declaration subset.
    fn elephc_dom_native_document_type_internal_subset(
        node: *mut c_void,
    ) -> NativeBuffer;
    /// Returns the libxml2 hash-table size for one doctype entities or notations.
    fn elephc_dom_native_document_type_dtd_table_size(
        node: *mut c_void,
        kind: i32,
    ) -> usize;
    /// Returns the libxml2 payload at one zero-based DTD table index.
    fn elephc_dom_native_document_type_dtd_table_at(
        node: *mut c_void,
        kind: i32,
        index: usize,
    ) -> *mut c_void;
    /// Looks up a libxml2 DTD table payload by its declared name.
    fn elephc_dom_native_document_type_dtd_table_lookup(
        node: *mut c_void,
        kind: i32,
        name: *const u8,
        name_length: usize,
    ) -> *mut c_void;
    /// Synthesizes one fresh notation wrapper node from a libxml2 notation payload.
    fn elephc_dom_native_notation_synthesize(
        payload: *mut c_void,
    ) -> *mut c_void;
    /// Returns one entity's public identifier when the entity is external and unparsed.
    fn elephc_dom_native_entity_external_id(node: *mut c_void) -> NativeBuffer;
    /// Returns one entity's system identifier when the entity is external and unparsed.
    fn elephc_dom_native_entity_system_id(node: *mut c_void) -> NativeBuffer;
    /// Returns one entity's resolved notation name when the entity is external and unparsed.
    fn elephc_dom_native_entity_notation_name(node: *mut c_void) -> NativeBuffer;
    /// Returns one synthesized notation's public identifier or an empty string.
    fn elephc_dom_native_notation_public_id(node: *mut c_void) -> NativeBuffer;
    /// Returns one synthesized notation's system identifier or an empty string.
    fn elephc_dom_native_notation_system_id(node: *mut c_void) -> NativeBuffer;
    /// Replaces one node's content from an explicit-length byte string.
    fn elephc_dom_native_node_set_content(
        node: *mut c_void,
        content: *const u8,
        content_length: usize,
    ) -> i32;
    /// Returns one character-data node's UTF-8 code-point length.
    fn elephc_dom_native_character_data_length(node: *mut c_void) -> i64;
    /// Returns a code-point substring or an `INDEX_SIZE_ERR` outcome.
    fn elephc_dom_native_character_data_substring(
        node: *mut c_void,
        offset: i64,
        count: i64,
        modern: i32,
    ) -> NativeBufferResult;
    /// Appends explicit-length bytes to one character-data node.
    fn elephc_dom_native_character_data_append(
        node: *mut c_void,
        data: *const u8,
        data_length: usize,
    ) -> i32;
    /// Inserts explicit-length bytes at one UTF-8 code-point offset.
    fn elephc_dom_native_character_data_insert(
        node: *mut c_void,
        offset: i64,
        data: *const u8,
        data_length: usize,
        modern: i32,
    ) -> i32;
    /// Deletes one UTF-8 code-point range.
    fn elephc_dom_native_character_data_delete(
        node: *mut c_void,
        offset: i64,
        count: i64,
        modern: i32,
    ) -> i32;
    /// Replaces one UTF-8 code-point range with explicit-length bytes.
    fn elephc_dom_native_character_data_replace(
        node: *mut c_void,
        offset: i64,
        count: i64,
        data: *const u8,
        data_length: usize,
        modern: i32,
    ) -> i32;
    /// Returns the concatenated data of one text node's adjacent text/CDATA run.
    fn elephc_dom_native_text_whole_text(node: *mut c_void) -> NativeBuffer;
    /// Splits one text node at a UTF-8 code-point offset.
    fn elephc_dom_native_text_split(
        node: *mut c_void,
        offset: i64,
    ) -> NativePointerResult;
    /// Reports whether libxml2 classifies one legacy text node as blank.
    fn elephc_dom_native_text_is_blank(node: *mut c_void) -> i32;
    /// Resolves a namespace URI for a nullable prefix.
    fn elephc_dom_native_node_lookup_namespace_uri(
        node: *mut c_void,
        prefix: *const u8,
        prefix_length: usize,
        default_namespace: i32,
    ) -> NativeBuffer;
    /// Resolves one in-scope prefix for a namespace URI.
    fn elephc_dom_native_node_lookup_prefix(
        node: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
    ) -> NativeBuffer;
    /// Clones one complete or shallow document while preserving its DOM family metadata.
    fn elephc_dom_native_document_clone(
        document: *mut c_void,
        deep: i32,
        family: i32,
    ) -> *mut c_void;
    /// Clones one node into its current document without attaching it.
    fn elephc_dom_native_node_clone(
        node: *mut c_void,
        deep: i32,
        modern: i32,
    ) -> *mut c_void;
    /// Imports one node into a target document or reports a DOMException code.
    fn elephc_dom_native_document_import_node(
        document: *mut c_void,
        node: *mut c_void,
        deep: i32,
        modern: i32,
    ) -> NativePointerResult;
    /// Adopts one node into a target document or reports a DOMException code.
    fn elephc_dom_native_document_adopt_node(
        document: *mut c_void,
        node: *mut c_void,
        modern: i32,
    ) -> NativePointerResult;
    /// Locates one connected element whose attribute is typed as an XML ID.
    fn elephc_dom_native_document_get_element_by_id(
        document: *mut c_void,
        id: *const u8,
        id_length: usize,
    ) -> *mut c_void;
    /// Counts one node's direct children of every native node kind.
    fn elephc_dom_native_node_child_count(node: *mut c_void) -> usize;
    /// Returns one node's direct child by zero-based index.
    fn elephc_dom_native_node_child_at(
        node: *mut c_void,
        index: usize,
    ) -> *mut c_void;
    /// Counts descendant elements matching one qualified tag name.
    fn elephc_dom_native_descendant_element_count_name(
        root: *mut c_void,
        name: *const u8,
        name_length: usize,
        match_local_name: i32,
    ) -> usize;
    /// Returns the first descendant element in tree order.
    fn elephc_dom_native_descendant_element_first(
        root: *mut c_void,
    ) -> *mut c_void;
    /// Returns the next descendant element within one trusted root.
    fn elephc_dom_native_descendant_element_next(
        root: *mut c_void,
        current: *mut c_void,
    ) -> *mut c_void;
    /// Collects in-scope namespaces for one element and optionally every descendant.
    fn elephc_dom_native_element_namespace_info(
        element: *mut c_void,
        include_descendants: i32,
    ) -> NativeNamespaceInfoResult;
    /// Releases one native namespace-info item vector after its borrowed bytes are copied.
    fn elephc_dom_native_namespace_info_result_free(
        items: *mut NativeNamespaceInfo,
    );
    /// Returns one descendant element matching a qualified tag name.
    fn elephc_dom_native_descendant_element_at_name(
        root: *mut c_void,
        index: usize,
        name: *const u8,
        name_length: usize,
        match_local_name: i32,
    ) -> *mut c_void;
    /// Counts descendant elements matching namespace URI and local name.
    fn elephc_dom_native_descendant_element_count_ns(
        root: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        local_name: *const u8,
        local_name_length: usize,
    ) -> usize;
    /// Returns one namespace-matching descendant element by zero-based index.
    fn elephc_dom_native_descendant_element_at_ns(
        root: *mut c_void,
        index: usize,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        local_name: *const u8,
        local_name_length: usize,
    ) -> *mut c_void;
    /// Returns one libxml2-owned attribute value.
    fn elephc_dom_native_element_get_attribute(
        element: *mut c_void,
        name: *const u8,
        name_length: usize,
    ) -> NativeBuffer;
    /// Returns one borrowed attribute-node pointer by qualified name.
    fn elephc_dom_native_element_get_attribute_node(
        element: *mut c_void,
        name: *const u8,
        name_length: usize,
    ) -> *mut c_void;
    /// Returns one libxml2-owned attribute value by namespace URI and local name.
    fn elephc_dom_native_element_get_attribute_ns(
        element: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        local_name: *const u8,
        local_name_length: usize,
    ) -> NativeBuffer;
    /// Returns one borrowed attribute pointer by namespace URI and local name.
    fn elephc_dom_native_element_get_attribute_node_ns(
        element: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        local_name: *const u8,
        local_name_length: usize,
    ) -> *mut c_void;
    /// Creates or updates one attribute and returns its native node.
    fn elephc_dom_native_element_set_attribute(
        element: *mut c_void,
        name: *const u8,
        name_length: usize,
        value: *const u8,
        value_length: usize,
    ) -> *mut c_void;
    /// Creates or updates one namespaced attribute and reports DOMException failures.
    fn elephc_dom_native_element_set_attribute_ns(
        element: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        qualified_name: *const u8,
        qualified_name_length: usize,
        value: *const u8,
        value_length: usize,
        modern: i32,
    ) -> NativePointerResult;
    /// Adopts one detached attribute into another authoritative document.
    fn elephc_dom_native_attribute_adopt(
        attribute: *mut c_void,
        document: *mut c_void,
    ) -> i32;
    /// Attaches one attribute node and returns the detached replaced attribute.
    fn elephc_dom_native_element_set_attribute_node(
        element: *mut c_void,
        attribute: *mut c_void,
        use_namespace: i32,
    ) -> *mut c_void;
    /// Detaches one exact attribute node from its current element.
    fn elephc_dom_native_element_remove_attribute_node(
        element: *mut c_void,
        attribute: *mut c_void,
    ) -> i32;
    /// Detaches one named attribute without invalidating an existing wrapper.
    fn elephc_dom_native_element_remove_attribute(
        element: *mut c_void,
        name: *const u8,
        name_length: usize,
    ) -> *mut c_void;
    /// Detaches one namespaced attribute or eliminates one legacy declaration.
    fn elephc_dom_native_element_remove_attribute_ns(
        element: *mut c_void,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        local_name: *const u8,
        local_name_length: usize,
        legacy: i32,
    ) -> *mut c_void;
    /// Counts ordinary attributes and optional legacy namespace declarations.
    fn elephc_dom_native_element_attribute_count(
        element: *mut c_void,
        include_namespace_declarations: i32,
    ) -> usize;
    /// Returns one ordinary attribute pointer by zero-based index.
    fn elephc_dom_native_element_attribute_at(
        element: *mut c_void,
        index: usize,
    ) -> *mut c_void;
    /// Returns one owned qualified attribute name at a stable list index.
    fn elephc_dom_native_element_attribute_name_at(
        element: *mut c_void,
        index: usize,
        include_namespace_declarations: i32,
    ) -> NativeBuffer;
    /// Returns one element's first element child.
    fn elephc_dom_native_element_first_child(element: *mut c_void) -> *mut c_void;
    /// Returns one element's last element child.
    fn elephc_dom_native_element_last_child(element: *mut c_void) -> *mut c_void;
    /// Returns one element's previous element sibling.
    fn elephc_dom_native_element_previous_sibling(element: *mut c_void) -> *mut c_void;
    /// Returns one element's next element sibling.
    fn elephc_dom_native_element_next_sibling(element: *mut c_void) -> *mut c_void;
    /// Counts one element's direct element children.
    fn elephc_dom_native_element_child_count(element: *mut c_void) -> i64;
    /// Frees one detached libxml2 node tree.
    fn elephc_dom_native_node_free(node: *mut c_void);
    /// Frees one standalone fake namespace-declaration node and its namespace binding.
    fn elephc_dom_native_namespace_node_free(node: *mut c_void);
    /// Frees one standalone fake DTD notation node and its copied fields.
    fn elephc_dom_native_notation_node_free(node: *mut c_void);
    /// Clones one standalone fake namespace-declaration node into a fresh allocation.
    fn elephc_dom_native_namespace_node_clone(node: *mut c_void) -> *mut c_void;
    /// Returns the PHP node name for one fake namespace-declaration node.
    fn elephc_dom_native_namespace_node_name(node: *mut c_void) -> NativeBuffer;
    /// Returns the PHP node value for one fake namespace-declaration node.
    fn elephc_dom_native_namespace_node_value(node: *mut c_void) -> NativeBuffer;
    /// Returns the PHP local name for one fake namespace-declaration node.
    fn elephc_dom_native_namespace_node_local_name(
        node: *mut c_void,
    ) -> NativeBuffer;
    /// Appends one SimpleXML child with php-src's QName and namespace behavior.
    fn elephc_dom_native_simplexml_add_child(
        document: *mut c_void,
        node: *mut c_void,
        qualified_name: *const u8,
        qualified_name_length: usize,
        value: *const u8,
        value_length: usize,
        has_value: i32,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        has_namespace: i32,
    ) -> NativePointerResult;
    /// Appends one SimpleXML attribute or returns a stable negative warning code.
    fn elephc_dom_native_simplexml_add_attribute(
        document: *mut c_void,
        node: *mut c_void,
        qualified_name: *const u8,
        qualified_name_length: usize,
        value: *const u8,
        value_length: usize,
        namespace_uri: *const u8,
        namespace_uri_length: usize,
        has_namespace: i32,
    ) -> i32;
    /// Collects the namespaces used by one SimpleXML node subtree.
    fn elephc_dom_native_simplexml_get_namespaces(
        node: *mut c_void,
        recursive: i32,
    ) -> NativeSimpleXmlNamespaceResult;
    /// Collects namespace declarations from the selected document subtree.
    fn elephc_dom_native_simplexml_get_doc_namespaces(
        document: *mut c_void,
        node: *mut c_void,
        recursive: i32,
        from_root: i32,
        include_xmlns_attributes: i32,
    ) -> NativeSimpleXmlNamespaceResult;
    /// Releases one native SimpleXML namespace result and all copied strings.
    fn elephc_dom_native_simplexml_namespace_result_free(
        result: *mut NativeSimpleXmlNamespaceResult,
    );
}

/// Describes the exact native parser versions embedded in this bridge build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeVersions {
    pub(crate) libxml_number: u32,
    pub(crate) libxml: &'static str,
    pub(crate) lexbor: &'static str,
}

/// Converts one process-lifetime native version literal into UTF-8.
fn static_c_string(pointer: *const c_char) -> &'static str {
    assert!(!pointer.is_null(), "native version pointer must not be null");
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .expect("native version literal must be UTF-8")
}

/// Reports whether one explicit-length byte string is a valid XML name.
pub(crate) fn validate_name(name: &[u8]) -> bool {
    unsafe { elephc_dom_native_validate_name(name.as_ptr(), name.len()) == 1 }
}

/// Reports whether one explicit-length byte string is a valid XML NCName.
pub(crate) fn validate_ncname(name: &[u8]) -> bool {
    unsafe { elephc_dom_native_validate_ncname(name.as_ptr(), name.len()) == 1 }
}

/// Reports whether one explicit-length byte string is a complete XML qualified name.
pub(crate) fn validate_qname(name: &[u8]) -> bool {
    unsafe { elephc_dom_native_validate_qname(name.as_ptr(), name.len()) == 1 }
}

/// Returns the libxml2 and Lexbor identities compiled into the static bridge.
pub(crate) fn versions() -> NativeVersions {
    unsafe {
        NativeVersions {
            libxml_number: elephc_dom_native_libxml_version(),
            libxml: static_c_string(elephc_dom_native_libxml_version_string()),
            lexbor: static_c_string(elephc_dom_native_lexbor_version_string()),
        }
    }
}

/// Parses one in-memory XML byte sequence with pinned libxml2 and reports success.
pub(crate) fn parses_xml(bytes: &[u8]) -> bool {
    unsafe { elephc_dom_native_parse_xml(bytes.as_ptr(), bytes.len()) == 1 }
}

/// Parses one in-memory HTML byte sequence with PHP's pinned Lexbor and reports success.
pub(crate) fn parses_html(bytes: &[u8]) -> bool {
    unsafe { elephc_dom_native_parse_html(bytes.as_ptr(), bytes.len()) == 1 }
}

/// Reports whether libxml2 recognizes one XML document encoding label.
pub(crate) fn encoding_is_valid(encoding: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_encoding_is_valid(
            encoding.as_ptr(),
            encoding.len(),
        ) == 1
    }
}

/// Reports whether Lexbor recognizes one HTML document encoding label.
pub(crate) fn html_encoding_is_valid(encoding: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_html_encoding_is_valid(
            encoding.as_ptr(),
            encoding.len(),
        ) == 1
    }
}

/// Confirms both frozen engine identities and one minimal parse through each adapter.
pub(crate) fn self_check() -> bool {
    let versions = versions();
    versions.libxml_number == 21503
        && versions.libxml == "2.15.3"
        && versions.lexbor == "2.7.0"
        && parses_xml(b"<root/>")
        && parses_html(b"<!doctype html><title>x</title>")
}

/// Creates one empty libxml2 document with explicit version and optional encoding.
pub(crate) fn document_new(version: &[u8], encoding: &[u8]) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_new(
            version.as_ptr(),
            version.len(),
            encoding.as_ptr(),
            encoding.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one complete modern HTML document with an optional title element.
pub(crate) fn document_new_html(title: Option<&[u8]>) -> Option<usize> {
    let (title_pointer, title_length) = optional_bytes_pointer(title);
    let pointer = unsafe {
        elephc_dom_native_document_new_html(title_pointer, title_length)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Parses one XML byte sequence into a new libxml2 document.
pub(crate) fn document_parse_xml(
    bytes: &[u8],
    options: i32,
    override_encoding: Option<&[u8]>,
    input_name: Option<&[u8]>,
) -> Result<DocumentParseOutcome, ()> {
    let (encoding_pointer, encoding_length) = override_encoding
        .map(|encoding| (encoding.as_ptr(), encoding.len()))
        .unwrap_or((std::ptr::null(), 0));
    let (input_name_pointer, input_name_length) =
        optional_bytes_pointer(input_name);
    let result = unsafe {
        elephc_dom_native_document_parse_xml(
            bytes.as_ptr(),
            bytes.len(),
            options,
            encoding_pointer,
            encoding_length,
            input_name_pointer,
            input_name_length,
            0,
        )
    };
    let errors = match copy_and_free_native_errors(result.errors, result.error_count) {
        Ok(errors) => errors,
        Err(NativeResultAbiError::MalformedRequest) => {
            if !result.document.is_null() {
                unsafe {
                    elephc_dom_native_document_free(result.document);
                }
            }
            return Err(());
        }
    };
    if result.allocation_failed != 0 {
        if !result.document.is_null() {
            unsafe {
                elephc_dom_native_document_free(result.document);
            }
        }
        return Err(());
    }
    Ok(DocumentParseOutcome {
        document: (!result.document.is_null()).then_some(result.document as usize),
        errors,
        host_status: result.status,
    })
}

/// Parses XML with a context-specific re-entrant PHP resource loader.
pub(crate) fn document_parse_xml_with_host(
    bytes: &[u8],
    options: i32,
    override_encoding: Option<&[u8]>,
    input_name: Option<&[u8]>,
    host_context: u64,
) -> Result<DocumentParseOutcome, ()> {
    let (encoding_pointer, encoding_length) = override_encoding
        .map(|encoding| (encoding.as_ptr(), encoding.len()))
        .unwrap_or((std::ptr::null(), 0));
    let (input_name_pointer, input_name_length) =
        optional_bytes_pointer(input_name);
    let result = unsafe {
        elephc_dom_native_document_parse_xml(
            bytes.as_ptr(),
            bytes.len(),
            options,
            encoding_pointer,
            encoding_length,
            input_name_pointer,
            input_name_length,
            host_context,
        )
    };
    let errors = match copy_and_free_native_errors(result.errors, result.error_count) {
        Ok(errors) => errors,
        Err(NativeResultAbiError::MalformedRequest) => {
            if !result.document.is_null() {
                unsafe {
                    elephc_dom_native_document_free(result.document);
                }
            }
            return Err(());
        }
    };
    if result.allocation_failed != 0 {
        if !result.document.is_null() {
            unsafe {
                elephc_dom_native_document_free(result.document);
            }
        }
        return Err(());
    }
    Ok(DocumentParseOutcome {
        document: (!result.document.is_null()).then_some(result.document as usize),
        errors,
        host_status: result.status,
    })
}

/// Exercises the native resource-loader cleanup path after input allocation failure.
#[cfg(test)]
pub(crate) fn test_resource_loader_input_from_io_failure(
    host_context: u64,
) -> i32 {
    unsafe {
        elephc_dom_native_test_resource_loader_input_from_io_failure(host_context)
    }
}

/// Parses one legacy HTML byte sequence through libxml2's HTML4 parser.
pub(crate) fn document_parse_html4(
    bytes: &[u8],
    options: i32,
    input_name: Option<&[u8]>,
) -> Result<DocumentParseOutcome, ()> {
    let (input_name_pointer, input_name_length) =
        optional_bytes_pointer(input_name);
    let result = unsafe {
        elephc_dom_native_document_parse_html4(
            bytes.as_ptr(),
            bytes.len(),
            options,
            input_name_pointer,
            input_name_length,
        )
    };
    let errors = match copy_and_free_native_errors(result.errors, result.error_count) {
        Ok(errors) => errors,
        Err(NativeResultAbiError::MalformedRequest) => {
            if !result.document.is_null() {
                unsafe {
                    elephc_dom_native_document_free(result.document);
                }
            }
            return Err(());
        }
    };
    if result.allocation_failed != 0 {
        if !result.document.is_null() {
            unsafe {
                elephc_dom_native_document_free(result.document);
            }
        }
        return Err(());
    }
    Ok(DocumentParseOutcome {
        document: (!result.document.is_null()).then_some(result.document as usize),
        errors,
        host_status: 0,
    })
}

/// Parses one HTML5 byte sequence through Lexbor into a modern libxml2 graph.
pub(crate) fn document_parse_html5(
    bytes: &[u8],
    options: u32,
    override_encoding: Option<&[u8]>,
    input_name: &[u8],
) -> Result<DocumentParseOutcome, HtmlParseError> {
    let (encoding_pointer, encoding_length) = override_encoding
        .map(|encoding| (encoding.as_ptr(), encoding.len()))
        .unwrap_or((std::ptr::null(), 0));
    let result = unsafe {
        elephc_dom_native_document_parse_html5(
            bytes.as_ptr(),
            bytes.len(),
            options,
            encoding_pointer,
            encoding_length,
            input_name.as_ptr(),
            input_name.len(),
        )
    };
    let errors = match copy_and_free_native_errors(result.errors, result.error_count) {
        Ok(errors) => errors,
        Err(NativeResultAbiError::MalformedRequest) => {
            if !result.document.is_null() {
                unsafe {
                    elephc_dom_native_document_free(result.document);
                }
            }
            return Err(HtmlParseError::Allocation);
        }
    };
    if result.status == 1 {
        return Err(HtmlParseError::InvalidEncoding);
    }
    if result.allocation_failed != 0 {
        if !result.document.is_null() {
            unsafe {
                elephc_dom_native_document_free(result.document);
            }
        }
        return Err(HtmlParseError::Allocation);
    }
    Ok(DocumentParseOutcome {
        document: (!result.document.is_null()).then_some(result.document as usize),
        errors,
        host_status: 0,
    })
}

/// Parses one XML or HTML fragment against an existing element context.
pub(crate) fn parse_fragment(
    context: usize,
    input: &[u8],
    html: bool,
) -> Result<usize, i32> {
    if html {
        let pointer = unsafe {
            elephc_dom_native_parse_html_fragment(
                context as *mut c_void,
                input.as_ptr(),
                input.len(),
            )
        };
        return (!pointer.is_null())
            .then_some(pointer as usize)
            .ok_or(11);
    }
    let result = unsafe {
        elephc_dom_native_parse_xml_fragment(
            context as *mut c_void,
            input.as_ptr(),
            input.len(),
        )
    };
    if result.error_code != 0 {
        Err(result.error_code)
    } else {
        (!result.pointer.is_null())
            .then_some(result.pointer as usize)
            .ok_or(11)
    }
}

/// Converts one libxml2 document to PHP's modern XML namespace representation.
pub(crate) fn document_convert_modern_xml(document: usize) -> bool {
    unsafe {
        elephc_dom_native_document_convert_modern_xml(document as *mut c_void)
            != 0
    }
}

/// Copies and releases one native validation result into Rust-owned storage.
fn validation_outcome(
    result: NativeValidationResult,
) -> Result<ValidationOutcome, ()> {
    let errors = copy_and_free_native_errors(result.errors, result.error_count)
        .map_err(|_| ())?;
    if result.allocation_failed != 0 {
        return Err(());
    }
    Ok(ValidationOutcome {
        valid: result.valid != 0,
        errors,
        status: result.status,
        host_status: result.host_status,
    })
}

/// Validates one document against its internal or external DTD subset.
pub(crate) fn document_validate(
    document: usize,
) -> Result<ValidationOutcome, ()> {
    validation_outcome(unsafe {
        elephc_dom_native_document_validate(document as *mut c_void)
    })
}

/// Performs XInclude through the pinned engine and copies all invalidated pointers.
pub(crate) fn document_xinclude(
    document: usize,
    flags: i32,
    generic_errors: bool,
    host_context: u64,
) -> XIncludeOutcome {
    let result = unsafe {
        elephc_dom_native_document_xinclude(
            document as *mut c_void,
            flags,
            i32::from(generic_errors),
            host_context,
        )
    };
    let native_errors = copy_and_free_native_errors(result.errors, result.error_count);
    let malformed_errors = native_errors.is_err();
    let malformed_invalidated =
        result.invalidated.is_null() && result.invalidated_count != 0;
    let errors = native_errors.unwrap_or_default();
    let invalidated = if result.invalidated.is_null() {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(
                result.invalidated,
                result.invalidated_count,
            )
        }
        .iter()
        .map(|pointer| *pointer as usize)
        .collect()
    };
    unsafe {
        elephc_dom_native_xinclude_result_free(
            std::ptr::null_mut(),
            0,
            result.invalidated,
        );
    }
    XIncludeOutcome {
        substitutions: result.substitutions,
        errors,
        invalidated,
        allocation_failed: result.allocation_failed != 0
            || malformed_errors
            || malformed_invalidated,
        host_status: result.host_status,
    }
}

/// Canonicalizes one node through pinned libxml2 and owns all returned storage.
#[allow(clippy::too_many_arguments)]
pub(crate) fn node_c14n(
    document: usize,
    node: usize,
    node_is_document: bool,
    modern: bool,
    exclusive: bool,
    with_comments: bool,
    query: Option<&[u8]>,
    namespaces: &[(Vec<u8>, Vec<u8>)],
    inclusive_prefixes: &[Vec<u8>],
    generic_errors: bool,
) -> Result<C14nOutcome, ()> {
    let namespace_prefixes: Vec<NativeBytes> = namespaces
        .iter()
        .map(|(prefix, _)| NativeBytes {
            pointer: prefix.as_ptr(),
            length: prefix.len(),
        })
        .collect();
    let namespace_uris: Vec<NativeBytes> = namespaces
        .iter()
        .map(|(_, uri)| NativeBytes {
            pointer: uri.as_ptr(),
            length: uri.len(),
        })
        .collect();
    let inclusive_prefixes: Vec<NativeBytes> = inclusive_prefixes
        .iter()
        .map(|prefix| NativeBytes {
            pointer: prefix.as_ptr(),
            length: prefix.len(),
        })
        .collect();
    let (query_pointer, query_length, has_xpath) = query.map_or(
        (std::ptr::null(), 0, 0),
        |query| (query.as_ptr(), query.len(), 1),
    );
    let result = unsafe {
        elephc_dom_native_node_c14n(
            document as *mut c_void,
            node as *mut c_void,
            i32::from(node_is_document),
            i32::from(modern),
            i32::from(exclusive),
            i32::from(with_comments),
            has_xpath,
            query_pointer,
            query_length,
            namespace_prefixes.as_ptr(),
            namespace_uris.as_ptr(),
            namespace_prefixes.len(),
            inclusive_prefixes.as_ptr(),
            inclusive_prefixes.len(),
            i32::from(generic_errors),
        )
    };
    let malformed_bytes = result.bytes.is_null() && result.length != 0;
    let native_errors = copy_and_free_native_errors(result.errors, result.error_count);
    let malformed_errors = native_errors.is_err();
    let bytes = copy_native_bytes(result.bytes, result.length);
    let errors = native_errors.unwrap_or_default();
    unsafe {
        elephc_dom_native_c14n_result_free(
            result.bytes,
            std::ptr::null_mut(),
            0,
        );
    }
    if result.allocation_failed != 0 || malformed_bytes || malformed_errors {
        return Err(());
    }
    Ok(C14nOutcome {
        bytes,
        errors,
        status: result.status,
    })
}

/// Evaluates one XPath expression through pinned libxml2 and owns all returned storage.
#[allow(clippy::too_many_arguments)]
pub(crate) fn xpath_evaluate(
    document: usize,
    node: Option<usize>,
    modern: bool,
    register_node_namespaces: bool,
    force_nodeset: bool,
    expression: &[u8],
    namespaces: &[(Vec<u8>, Vec<u8>)],
    host_context: u64,
    host: Option<crate::context::Host>,
    xpath_handle: u64,
    callbacks: &[(Vec<u8>, Vec<u8>)],
) -> Result<XPathOutcome, ()> {
    let namespace_prefixes = namespaces
        .iter()
        .map(|(prefix, _)| NativeBytes {
            pointer: prefix.as_ptr(),
            length: prefix.len(),
        })
        .collect::<Vec<_>>();
    let namespace_uris = namespaces
        .iter()
        .map(|(_, uri)| NativeBytes {
            pointer: uri.as_ptr(),
            length: uri.len(),
        })
        .collect::<Vec<_>>();
    let callback_namespaces = callbacks
        .iter()
        .map(|(namespace_uri, _)| NativeBytes {
            pointer: namespace_uri.as_ptr(),
            length: namespace_uri.len(),
        })
        .collect::<Vec<_>>();
    let callback_names = callbacks
        .iter()
        .map(|(_, name)| NativeBytes {
            pointer: name.as_ptr(),
            length: name.len(),
        })
        .collect::<Vec<_>>();
    let result = unsafe {
        elephc_dom_native_xpath_evaluate(
            document as *mut c_void,
            node.unwrap_or(0) as *mut c_void,
            i32::from(modern),
            i32::from(register_node_namespaces),
            i32::from(force_nodeset),
            expression.as_ptr(),
            expression.len(),
            namespace_prefixes.as_ptr(),
            namespace_uris.as_ptr(),
            namespace_prefixes.len(),
            host_context,
            xpath_handle,
            callback_namespaces.as_ptr(),
            callback_names.as_ptr(),
            callback_namespaces.len(),
        )
    };
    let malformed_pointers =
        result.pointers.is_null() && result.pointer_count != 0;
    let malformed_bytes = result.bytes.is_null() && result.byte_count != 0;
    let native_errors = copy_and_free_native_errors(result.errors, result.error_count);
    let malformed_errors = native_errors.is_err();
    let malformed_callback_leases =
        result.callback_leases.is_null() && result.callback_lease_count != 0;
    let pointers = copy_native_pointers(result.pointers, result.pointer_count);
    let mut bytes = copy_native_bytes(result.bytes, result.byte_count);
    let errors = native_errors.unwrap_or_default();
    let callback_lease_ids = if result.callback_leases.is_null()
        || result.callback_lease_count == 0
    {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(
                result.callback_leases,
                result.callback_lease_count,
            )
        }
        .to_vec()
    };
    unsafe {
        elephc_dom_native_xpath_result_free(
            result.pointers,
            result.bytes,
            std::ptr::null_mut(),
            0,
            result.callback_leases,
        );
    }
    let callback_leases = if callback_lease_ids.is_empty() {
        Vec::new()
    } else {
        let host = host.ok_or(())?;
        callback_lease_ids
            .into_iter()
            .map(|result_id| {
                crate::host::LeasedHostResult::from_id(host, result_id)
                    .map_err(|_| ())
            })
            .collect::<Result<Vec<_>, ()>>()?
    };
    if result.allocation_failed != 0
        || malformed_pointers
        || malformed_bytes
        || malformed_errors
        || malformed_callback_leases
    {
        return Err(());
    }
    let callback_error = if matches!(result.status, 7 | 8) {
        if bytes.is_empty() {
            return Err(());
        }
        std::mem::take(&mut bytes)
    } else {
        Vec::new()
    };
    let value = match result.kind {
        0 | 5 => XPathValue::Null,
        1 => XPathValue::Nodes(pointers),
        2 => XPathValue::Boolean(result.boolean_value != 0),
        3 => XPathValue::Number(result.number),
        4 => XPathValue::Bytes(bytes),
        _ => return Err(()),
    };
    Ok(XPathOutcome {
        value,
        errors,
        status: result.status,
        host_status: result.host_status,
        callback_error,
        _callback_leases: callback_leases,
    })
}

/// Validates an in-memory XML Schema with PHP resource callbacks enabled.
pub(crate) fn document_schema_validate_source_with_host(
    document: usize,
    source: &[u8],
    flags: i32,
    generic_errors: bool,
    host_context: u64,
) -> Result<ValidationOutcome, ()> {
    validation_outcome(unsafe {
        elephc_dom_native_document_schema_validate_source(
            document as *mut c_void,
            source.as_ptr(),
            source.len(),
            flags,
            i32::from(generic_errors),
            host_context,
        )
    })
}

/// Validates one document against a W3C XML Schema file or PHP stream URL.
pub(crate) fn document_schema_validate_file(
    document: usize,
    path: &[u8],
    flags: i32,
    generic_errors: bool,
    host_context: u64,
) -> Result<ValidationOutcome, ()> {
    validation_outcome(unsafe {
        elephc_dom_native_document_schema_validate_file(
            document as *mut c_void,
            path.as_ptr(),
            path.len(),
            flags,
            i32::from(generic_errors),
            host_context,
        )
    })
}

/// Validates an in-memory Relax NG grammar with PHP resource callbacks enabled.
pub(crate) fn document_relaxng_validate_source_with_host(
    document: usize,
    source: &[u8],
    generic_errors: bool,
    host_context: u64,
) -> Result<ValidationOutcome, ()> {
    validation_outcome(unsafe {
        elephc_dom_native_document_relaxng_validate_source(
            document as *mut c_void,
            source.as_ptr(),
            source.len(),
            i32::from(generic_errors),
            host_context,
        )
    })
}

/// Validates one document against a Relax NG file or PHP stream URL.
pub(crate) fn document_relaxng_validate_file(
    document: usize,
    path: &[u8],
    generic_errors: bool,
    host_context: u64,
) -> Result<ValidationOutcome, ()> {
    validation_outcome(unsafe {
        elephc_dom_native_document_relaxng_validate_file(
            document as *mut c_void,
            path.as_ptr(),
            path.len(),
            i32::from(generic_errors),
            host_context,
        )
    })
}

/// Classifies malformed pointer/count ownership pairs returned by the native adapter.
#[derive(Debug, PartialEq, Eq)]
enum NativeResultAbiError {
    MalformedRequest,
}

/// Copies and releases one well-formed native structured-error allocation.
fn copy_and_free_native_errors(
    pointer: *mut NativeError,
    count: usize,
) -> Result<Vec<LibxmlErrorObject>, NativeResultAbiError> {
    let errors = copy_native_errors(pointer, count)?;
    unsafe {
        elephc_dom_native_parse_result_free(pointer, count);
    }
    Ok(errors)
}

/// Copies one native structured-error array before its C allocation is released.
fn copy_native_errors(
    pointer: *const NativeError,
    count: usize,
) -> Result<Vec<LibxmlErrorObject>, NativeResultAbiError> {
    if pointer.is_null() {
        return if count == 0 {
            Ok(Vec::new())
        } else {
            Err(NativeResultAbiError::MalformedRequest)
        };
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    Ok(unsafe { std::slice::from_raw_parts(pointer, count) }
        .iter()
        .map(|error| LibxmlErrorObject {
            level: i64::from(error.level),
            domain: error.domain,
            code: i64::from(error.code),
            line: i64::from(error.line),
            column: i64::from(error.column),
            message: copy_native_bytes(error.message, error.message_length),
            file: copy_native_bytes(error.file, error.file_length),
        })
        .collect())
}

/// Copies one native pointer array before its C allocation is released.
fn copy_native_pointers(pointer: *const *mut c_void, count: usize) -> Vec<usize> {
    if pointer.is_null() || count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(pointer, count) }
            .iter()
            .map(|pointer| *pointer as usize)
            .collect()
    }
}

/// Omits the XML declaration when serializing a complete document.
pub(crate) const XML_SAVE_NO_DECL: i32 = 2;
/// Expands empty XML elements into explicit opening and closing tags.
pub(crate) const XML_SAVE_NO_EMPTY: i32 = 4;

/// Copies one nullable native byte range without interpreting its encoding.
fn copy_native_bytes(pointer: *const u8, length: usize) -> Vec<u8> {
    if pointer.is_null() || length == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec()
    }
}

/// Frees one libxml2 document pointer owned by a bridge handle.
pub(crate) unsafe fn document_free(document: usize) {
    elephc_dom_native_document_free(document as *mut c_void);
}

/// Returns one modern HTML document's Lexbor compatibility-mode constant.
pub(crate) fn html_document_quirks_mode(document: usize) -> i32 {
    unsafe {
        elephc_dom_native_html_document_quirks_mode(
            document as *mut c_void,
        )
    }
}

/// Runs one PHP-pinned CSS selector operation and owns the copied flat result.
pub(crate) fn selector_query(
    root: usize,
    input: &[u8],
    operation: i32,
    quirks: bool,
) -> SelectorOutcome {
    let result = unsafe {
        elephc_dom_native_selector_query(
            root as *mut c_void,
            input.as_ptr(),
            input.len(),
            operation,
            i32::from(quirks),
        )
    };
    let mut malformed_pointers =
        result.count != 0 && result.pointers.is_null();
    let pointers =
        if result.pointers.is_null() || result.count == 0 {
            Vec::new()
        } else {
            unsafe {
                std::slice::from_raw_parts(result.pointers, result.count)
            }
            .iter()
            .map(|pointer| {
                if pointer.is_null() {
                    malformed_pointers = true;
                }
                *pointer as usize
            })
            .collect()
        };
    let message =
        copy_native_bytes(result.message, result.message_length);
    unsafe {
        elephc_dom_native_selector_result_free(
            result.pointers,
            result.message,
        );
    }
    SelectorOutcome {
        pointers,
        matched: result.matched != 0,
        error_code: if malformed_pointers {
            -1
        } else {
            result.error_code
        },
        message,
    }
}

/// Serializes one document using legacy, modern XML, or modern HTML mode.
pub(crate) fn document_serialize(
    document: usize,
    encoding: Option<&[u8]>,
    format: bool,
    mode: i32,
    options: i32,
) -> Option<Vec<u8>> {
    let (encoding_pointer, encoding_length) = encoding
        .map(|encoding| (encoding.as_ptr(), encoding.len()))
        .unwrap_or((std::ptr::null(), 0));
    let buffer = unsafe {
        elephc_dom_native_document_serialize(
            document as *mut c_void,
            encoding_pointer,
            encoding_length,
            i32::from(format),
            mode,
            options,
        )
    };
    if buffer.pointer.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(buffer.pointer, buffer.length) }.to_vec();
    unsafe {
        elephc_dom_native_buffer_free(buffer.pointer);
    }
    Some(bytes)
}

/// Serializes one node using its authoritative document context.
pub(crate) fn document_serialize_node(
    document: usize,
    node: usize,
    format: bool,
    mode: i32,
    options: i32,
) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_document_serialize_node(
            document as *mut c_void,
            node as *mut c_void,
            i32::from(format),
            mode,
            options,
        )
    })
}

/// Serializes one SimpleXML subnode without synthesizing inherited namespace declarations.
pub(crate) fn simplexml_serialize_node(
    document: usize,
    node: usize,
) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_simplexml_serialize_node(
            document as *mut c_void,
            node as *mut c_void,
        )
    })
}

/// Copies direct SimpleXML text while excluding nested element descendants.
pub(crate) fn simplexml_node_list_content(node: usize) -> Vec<u8> {
    owned_buffer(unsafe {
        elephc_dom_native_simplexml_node_list_content(node as *mut c_void)
    })
    .unwrap_or_default()
}

/// Copies libxml2's raw name used by php-src for SimpleXML debug properties.
pub(crate) fn simplexml_node_name(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_simplexml_node_name(node as *mut c_void)
    })
}

/// Serializes one modern HTML document or same-document node with HTML5 rules.
pub(crate) fn document_serialize_html5(
    document: usize,
    node: Option<usize>,
) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_document_serialize_html5(
            document as *mut c_void,
            node.map(|pointer| pointer as *mut c_void)
                .unwrap_or(std::ptr::null_mut()),
        )
    })
}

/// Serializes one modern element's inner or outer markup without a declaration.
pub(crate) fn element_serialize_markup(
    element: usize,
    inner: bool,
    html: bool,
) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        if html {
            elephc_dom_native_element_serialize_html5(
                element as *mut c_void,
                i32::from(inner),
            )
        } else {
            elephc_dom_native_element_serialize_xml(
                element as *mut c_void,
                i32::from(inner),
            )
        }
    })
}

/// Checks whether one modern XML inner or outer serialization may be emitted.
pub(crate) fn element_xml_is_well_formed(
    element: usize,
    inner: bool,
) -> bool {
    unsafe {
        elephc_dom_native_element_xml_is_well_formed(
            element as *mut c_void,
            i32::from(inner),
        ) != 0
    }
}

/// Serializes one legacy HTML document or same-document node with libxml2 HTML rules.
pub(crate) fn document_serialize_html4(
    document: usize,
    node: Option<usize>,
    format: bool,
) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_document_serialize_html4(
            document as *mut c_void,
            node.map(|pointer| pointer as *mut c_void)
                .unwrap_or(std::ptr::null_mut()),
            i32::from(format),
        )
    })
}

/// Copies the document's current XML version bytes.
pub(crate) fn document_version(document: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_document_version(document as *mut c_void)
    })
}

/// Copies the document's current encoding bytes when libxml2 records one.
pub(crate) fn document_encoding(document: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_document_encoding(document as *mut c_void)
    })
}

/// Returns one document's internal doctype pointer.
pub(crate) fn document_doctype(document: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_document_doctype(document as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one detached document type with optional public and system identifiers.
pub(crate) fn document_type_new(
    qualified_name: &[u8],
    public_id: &[u8],
    system_id: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_type_new(
            qualified_name.as_ptr(),
            qualified_name.len(),
            public_id.as_ptr(),
            public_id.len(),
            system_id.as_ptr(),
            system_id.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates the root element required by an implementation-level document factory.
pub(crate) fn document_create_implementation_root(
    document: usize,
    namespace_uri: Option<&[u8]>,
    qualified_name: &[u8],
    modern: bool,
) -> PointerOutcome {
    let (namespace_pointer, namespace_length) =
        optional_bytes_pointer(namespace_uri);
    pointer_outcome(unsafe {
        elephc_dom_native_document_create_implementation_root(
            document as *mut c_void,
            namespace_pointer,
            namespace_length,
            qualified_name.as_ptr(),
            qualified_name.len(),
            i32::from(modern),
        )
    })
}

/// Attaches or modern-adopts a document type into a newly created document.
pub(crate) fn document_attach_doctype(
    document: usize,
    doctype: usize,
    allow_adoption: bool,
) -> i32 {
    unsafe {
        elephc_dom_native_document_attach_doctype(
            document as *mut c_void,
            doctype as *mut c_void,
            i32::from(allow_adoption),
        )
    }
}

/// Copies one document's current URL bytes.
pub(crate) fn document_url(document: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_document_url(document as *mut c_void)
    })
}

/// Replaces one document's URL from an explicit byte string.
pub(crate) fn document_set_url(document: usize, url: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_document_set_url(
            document as *mut c_void,
            url.as_ptr(),
            url.len(),
        ) == 1
    }
}

/// Replaces one document's XML version bytes.
pub(crate) fn document_set_version(document: usize, version: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_document_set_version(
            document as *mut c_void,
            version.as_ptr(),
            version.len(),
        ) == 1
    }
}

/// Replaces one document's encoding or reports invalid encoding as `-1`.
pub(crate) fn document_set_encoding(document: usize, encoding: &[u8]) -> i32 {
    unsafe {
        elephc_dom_native_document_set_encoding(
            document as *mut c_void,
            encoding.as_ptr(),
            encoding.len(),
        )
    }
}

/// Replaces one modern document encoding with a canonical WHATWG label.
pub(crate) fn document_set_modern_encoding(
    document: usize,
    encoding: &[u8],
) -> i32 {
    unsafe {
        elephc_dom_native_document_set_modern_encoding(
            document as *mut c_void,
            encoding.as_ptr(),
            encoding.len(),
        )
    }
}

/// Returns libxml2's standalone flag for one document.
pub(crate) fn document_standalone(document: usize) -> i32 {
    unsafe { elephc_dom_native_document_standalone(document as *mut c_void) }
}

/// Updates libxml2's standalone flag and reports whether the pointer/value were valid.
pub(crate) fn document_set_standalone(document: usize, standalone: i32) -> bool {
    unsafe {
        elephc_dom_native_document_set_standalone(
            document as *mut c_void,
            standalone,
        ) == 1
    }
}

/// Creates one detached element associated with an authoritative document.
pub(crate) fn document_create_element(
    document: usize,
    name: &[u8],
    value: Option<&[u8]>,
    html: bool,
) -> Option<usize> {
    let (value_pointer, value_length) = value
        .map(|bytes| (bytes.as_ptr(), bytes.len()))
        .unwrap_or((std::ptr::null(), 0));
    let pointer = unsafe {
        elephc_dom_native_document_create_element(
            document as *mut c_void,
            name.as_ptr(),
            name.len(),
            value_pointer,
            value_length,
            i32::from(html),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one namespaced detached element with family-specific QName validation.
pub(crate) fn document_create_element_ns(
    document: usize,
    namespace_uri: Option<&[u8]>,
    qualified_name: &[u8],
    value: Option<&[u8]>,
    modern: bool,
) -> PointerOutcome {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    let (value_pointer, value_length) = optional_bytes_pointer(value);
    pointer_outcome(unsafe {
        elephc_dom_native_document_create_element_ns(
            document as *mut c_void,
            namespace_pointer,
            namespace_length,
            qualified_name.as_ptr(),
            qualified_name.len(),
            value_pointer,
            value_length,
            i32::from(modern),
        )
    })
}

/// Creates one detached unqualified attribute associated with a document.
pub(crate) fn document_create_attribute(
    document: usize,
    name: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_create_attribute(
            document as *mut c_void,
            name.as_ptr(),
            name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one detached namespaced attribute with family-specific QName validation.
pub(crate) fn document_create_attribute_ns(
    document: usize,
    namespace_uri: Option<&[u8]>,
    qualified_name: &[u8],
    modern: bool,
) -> PointerOutcome {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    pointer_outcome(unsafe {
        elephc_dom_native_document_create_attribute_ns(
            document as *mut c_void,
            namespace_pointer,
            namespace_length,
            qualified_name.as_ptr(),
            qualified_name.len(),
            i32::from(modern),
        )
    })
}

/// Creates one detached text node associated with an authoritative document.
pub(crate) fn document_create_text(document: usize, value: &[u8]) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_create_text(
            document as *mut c_void,
            value.as_ptr(),
            value.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one detached CDATA section associated with an authoritative document.
pub(crate) fn document_create_cdata(document: usize, value: &[u8]) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_create_cdata(
            document as *mut c_void,
            value.as_ptr(),
            value.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one detached comment associated with an authoritative document.
pub(crate) fn document_create_comment(document: usize, value: &[u8]) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_create_comment(
            document as *mut c_void,
            value.as_ptr(),
            value.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one detached empty document fragment.
pub(crate) fn document_create_fragment(document: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_document_create_fragment(document as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Parses and appends one XML balanced chunk while copying every libxml error.
pub(crate) fn fragment_append_xml(
    fragment: usize,
    input: &[u8],
) -> Result<FragmentAppendOutcome, ()> {
    let result = unsafe {
        elephc_dom_native_fragment_append_xml(
            fragment as *mut c_void,
            input.as_ptr(),
            input.len(),
        )
    };
    let errors = copy_and_free_native_errors(result.errors, result.error_count)
        .map_err(|_| ())?;
    if result.allocation_failed != 0 || result.status != 0 {
        return Err(());
    }
    Ok(FragmentAppendOutcome {
        appended: !result.document.is_null(),
        errors,
    })
}

/// Creates one detached processing instruction associated with a document.
pub(crate) fn document_create_pi(
    document: usize,
    target: &[u8],
    data: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_create_pi(
            document as *mut c_void,
            target.as_ptr(),
            target.len(),
            data.as_ptr(),
            data.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates one detached entity-reference node associated with a document.
pub(crate) fn document_create_entity_reference(
    document: usize,
    name: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_create_entity_reference(
            document as *mut c_void,
            name.as_ptr(),
            name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns the document element pointer when the graph currently has one.
pub(crate) fn document_element(document: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_document_element(document as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one modern document's direct HTML head element.
pub(crate) fn document_head(document: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_document_head(document as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one modern document's direct HTML body or frameset element.
pub(crate) fn document_body(document: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_document_body(document as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one modern document's effective HTML or SVG title element.
pub(crate) fn document_title_element(document: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_title_element(document as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Copies one modern document's collapsed HTML or SVG title.
pub(crate) fn document_title(document: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_document_title(document as *mut c_void)
    })
}

/// Replaces or creates one modern document's HTML or SVG title text.
pub(crate) fn document_set_title(document: usize, value: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_document_set_title(
            document as *mut c_void,
            value.as_ptr(),
            value.len(),
        ) == 1
    }
}

/// Reports whether one element is an HTML body or frameset.
pub(crate) fn node_is_html_body(node: usize) -> bool {
    unsafe {
        elephc_dom_native_node_is_html_body(node as *mut c_void) == 1
    }
}

/// Reports whether one element belongs to the HTML namespace.
pub(crate) fn node_is_html_element(node: usize) -> bool {
    unsafe {
        elephc_dom_native_node_is_html_element(node as *mut c_void) == 1
    }
}

/// Appends one node and returns libxml2's authoritative resulting node pointer.
pub(crate) fn node_append_child(parent: usize, child: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_append_child(
            parent as *mut c_void,
            child as *mut c_void,
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's parent pointer, including a document parent.
pub(crate) fn node_parent(node: usize) -> Option<usize> {
    let pointer = unsafe { elephc_dom_native_node_parent(node as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns the native document currently assigned to one node, if any.
pub(crate) fn node_document(node: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_node_document(node as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's parent only when it is an element.
pub(crate) fn node_parent_element(node: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_parent_element(node as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's first child.
pub(crate) fn node_first_child(node: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_first_child(node as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns an element's ordinary child container or private template fragment.
pub(crate) fn element_content_container(
    element: usize,
    ensure: bool,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_content_container(
            element as *mut c_void,
            i32::from(ensure),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's last child.
pub(crate) fn node_last_child(node: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_last_child(node as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's previous sibling.
pub(crate) fn node_previous_sibling(node: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_previous_sibling(node as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's next sibling.
pub(crate) fn node_next_sibling(node: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_next_sibling(node as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one node's topmost ancestor, which may be a document or detached node.
pub(crate) fn node_root(node: usize) -> Option<usize> {
    let pointer = unsafe { elephc_dom_native_node_root(node as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Reports whether one node currently has a document ancestor.
pub(crate) fn node_is_connected(node: usize) -> bool {
    unsafe { elephc_dom_native_node_is_connected(node as *mut c_void) == 1 }
}

/// Reports whether one node currently has at least one child.
pub(crate) fn node_has_children(node: usize) -> bool {
    unsafe { elephc_dom_native_node_has_children(node as *mut c_void) == 1 }
}

/// Reports whether one node contains another node, including identity.
pub(crate) fn node_contains(node: usize, other: usize) -> bool {
    unsafe {
        elephc_dom_native_node_contains(
            node as *mut c_void,
            other as *mut c_void,
        ) == 1
    }
}

/// Reports PHP-compatible structural equality for two DOM nodes.
pub(crate) fn node_is_equal(node: usize, other: usize, modern: bool) -> bool {
    unsafe {
        elephc_dom_native_node_is_equal(
            node as *mut c_void,
            other as *mut c_void,
            i32::from(modern),
        ) == 1
    }
}

/// Returns one DOM document-position relation bitmask.
pub(crate) fn node_compare_document_position(
    node: usize,
    other: usize,
) -> Option<i64> {
    let position = unsafe {
        elephc_dom_native_node_compare_document_position(
            node as *mut c_void,
            other as *mut c_void,
        )
    };
    (position >= 0).then_some(position)
}

/// Unlinks one direct child and reports whether the relationship was valid.
pub(crate) fn node_unlink_child(parent: usize, child: usize) -> bool {
    unsafe {
        elephc_dom_native_node_unlink_child(
            parent as *mut c_void,
            child as *mut c_void,
        ) == 1
    }
}

/// Inserts one node before a direct reference child, or appends when the reference is absent.
pub(crate) fn node_insert_before(
    parent: usize,
    child: usize,
    reference: Option<usize>,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_insert_before(
            parent as *mut c_void,
            child as *mut c_void,
            reference.unwrap_or_default() as *mut c_void,
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Renames one modern element or attribute and returns its detailed native status.
pub(crate) fn node_rename(
    node: usize,
    namespace_uri: Option<&[u8]>,
    qualified_name: &[u8],
) -> i32 {
    let (namespace_pointer, namespace_length) =
        optional_bytes_pointer(namespace_uri);
    unsafe {
        elephc_dom_native_node_rename(
            node as *mut c_void,
            namespace_pointer,
            namespace_length,
            qualified_name.as_ptr(),
            qualified_name.len(),
        )
    }
}

/// Replaces one direct child and returns the detached prior node pointer.
pub(crate) fn node_replace_child(
    parent: usize,
    child: usize,
    replaced: usize,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_replace_child(
            parent as *mut c_void,
            child as *mut c_void,
            replaced as *mut c_void,
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns the DOM node-type integer for one native node or document.
pub(crate) fn node_type(node: usize) -> u32 {
    unsafe { elephc_dom_native_node_type(node as *mut c_void) }
}

/// Returns libxml2's unnormalized storage type for internal DTD validation.
pub(crate) fn node_storage_type(node: usize) -> u32 {
    unsafe { elephc_dom_native_node_storage_type(node as *mut c_void) }
}

/// Copies and frees one native node's PHP-compatible qualified-name bytes.
pub(crate) fn node_name(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_node_name(node as *mut c_void)
    })
}

/// Copies and releases one native node's recursively computed text content.
pub(crate) fn node_content(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_node_content(node as *mut c_void)
    })
}

/// Copies one node's namespace URI when it has a namespace binding.
pub(crate) fn node_namespace_uri(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_node_namespace_uri(node as *mut c_void)
    })
}

/// Copies one node's namespace prefix when its namespace has one.
pub(crate) fn node_prefix(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe { elephc_dom_native_node_prefix(node as *mut c_void) })
}

/// Rebinds one legacy element or attribute to a namespace prefix.
pub(crate) fn node_set_prefix(node: usize, prefix: &[u8]) -> i32 {
    unsafe {
        elephc_dom_native_node_set_prefix(
            node as *mut c_void,
            prefix.as_ptr(),
            prefix.len(),
        )
    }
}

/// Copies one element or attribute node's local name.
pub(crate) fn node_local_name(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_node_local_name(node as *mut c_void)
    })
}

/// Copies and frees one node's effective base URI.
pub(crate) fn node_base_uri(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe { elephc_dom_native_node_base_uri(node as *mut c_void) })
}

/// Copies and frees one node's absolute libxml2 path.
pub(crate) fn node_path(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe { elephc_dom_native_node_path(node as *mut c_void) })
}

/// Returns one node's parser source line, or zero when unavailable.
pub(crate) fn node_line(node: usize) -> i64 {
    unsafe { elephc_dom_native_node_line(node as *mut c_void) }
}

/// Reports whether one node currently owns at least one attribute.
pub(crate) fn node_has_attributes(node: usize) -> bool {
    unsafe { elephc_dom_native_node_has_attributes(node as *mut c_void) == 1 }
}

/// Reports whether one native attribute is typed as an XML ID.
pub(crate) fn attribute_is_id(node: usize) -> bool {
    unsafe { elephc_dom_native_attribute_is_id(node as *mut c_void) == 1 }
}

/// Updates one native attribute's XML ID type marker.
pub(crate) fn attribute_set_is_id(node: usize, is_id: bool) -> bool {
    unsafe {
        elephc_dom_native_attribute_set_is_id(
            node as *mut c_void,
            i32::from(is_id),
        ) == 1
    }
}

/// Copies one doctype's declared name, using an empty string when absent.
pub(crate) fn document_type_name(node: usize) -> Vec<u8> {
    borrowed_buffer(unsafe {
        elephc_dom_native_document_type_name(node as *mut c_void)
    })
    .unwrap_or_default()
}

/// Copies one doctype's public identifier, using an empty string when absent.
pub(crate) fn document_type_public_id(node: usize) -> Vec<u8> {
    borrowed_buffer(unsafe {
        elephc_dom_native_document_type_public_id(node as *mut c_void)
    })
    .unwrap_or_default()
}

/// Copies one doctype's system identifier, using an empty string when absent.
pub(crate) fn document_type_system_id(node: usize) -> Vec<u8> {
    borrowed_buffer(unsafe {
        elephc_dom_native_document_type_system_id(node as *mut c_void)
    })
    .unwrap_or_default()
}

/// Serializes one doctype's internal subset or returns null when it is absent.
pub(crate) fn document_type_internal_subset(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_document_type_internal_subset(node as *mut c_void)
    })
}
/// Identifies one libxml2 DTD hash table by its declared semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DtdTableKind {
    Entities,
    Notations,
}

impl DtdTableKind {
    /// Returns the C integer tag expected by the native bridge.
    fn as_raw(self) -> i32 {
        match self {
            DtdTableKind::Entities => 0,
            DtdTableKind::Notations => 1,
        }
    }
}

/// Returns the libxml2 hash-table size for one doctype's entities or notations.
pub(crate) fn document_type_dtd_table_size(
    node: usize,
    kind: DtdTableKind,
) -> usize {
    unsafe {
        elephc_dom_native_document_type_dtd_table_size(
            node as *mut c_void,
            kind.as_raw(),
        )
    }
}

/// Returns the libxml2 payload at one zero-based DTD table index, or null.
pub(crate) fn document_type_dtd_table_at(
    node: usize,
    kind: DtdTableKind,
    index: usize,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_type_dtd_table_at(
            node as *mut c_void,
            kind.as_raw(),
            index,
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Looks up one libxml2 DTD table payload by its declared name, or null.
pub(crate) fn document_type_dtd_table_lookup(
    node: usize,
    kind: DtdTableKind,
    name: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_type_dtd_table_lookup(
            node as *mut c_void,
            kind.as_raw(),
            name.as_ptr(),
            name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Synthesizes one fresh notation wrapper node from a libxml2 notation payload.
///
/// `xmlNotation` values are not `xmlNode` records, so the bridge cannot use
/// them directly. The returned pointer is an independently owned fake
/// `xmlEntity` whose `type` is `XML_NOTATION_NODE`; its owning `NodeObject`
/// releases it as soon as the wrapper handle goes out of scope.
pub(crate) fn notation_synthesize(payload: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_notation_synthesize(payload as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one entity's public identifier when the entity is external and unparsed.
pub(crate) fn entity_public_id(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_entity_external_id(node as *mut c_void)
    })
}

/// Returns one entity's system identifier when the entity is external and unparsed.
pub(crate) fn entity_system_id(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_entity_system_id(node as *mut c_void)
    })
}

/// Returns one entity's resolved notation name when the entity is external and unparsed.
pub(crate) fn entity_notation_name(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_entity_notation_name(node as *mut c_void)
    })
}

/// Returns one synthesized notation's public identifier or an empty string.
pub(crate) fn notation_public_id(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_notation_public_id(node as *mut c_void)
    })
}

/// Returns one synthesized notation's system identifier or an empty string.
pub(crate) fn notation_system_id(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_notation_system_id(node as *mut c_void)
    })
}


/// Replaces one node's content with an explicit-length byte string.
pub(crate) fn node_set_content(node: usize, content: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_node_set_content(
            node as *mut c_void,
            content.as_ptr(),
            content.len(),
        ) == 1
    }
}

/// Returns one character-data node's UTF-8 code-point length.
pub(crate) fn character_data_length(node: usize) -> Option<i64> {
    let length =
        unsafe { elephc_dom_native_character_data_length(node as *mut c_void) };
    (length >= 0).then_some(length)
}

/// Returns one character-data code-point substring or its DOMException code.
pub(crate) fn character_data_substring(
    node: usize,
    offset: i64,
    count: i64,
    modern: bool,
) -> BufferOutcome {
    buffer_outcome(unsafe {
        elephc_dom_native_character_data_substring(
            node as *mut c_void,
            offset,
            count,
            i32::from(modern),
        )
    })
}

/// Appends exact bytes to one character-data node.
pub(crate) fn character_data_append(node: usize, data: &[u8]) -> bool {
    unsafe {
        elephc_dom_native_character_data_append(
            node as *mut c_void,
            data.as_ptr(),
            data.len(),
        ) == 0
    }
}

/// Inserts exact bytes at one UTF-8 code-point offset.
pub(crate) fn character_data_insert(
    node: usize,
    offset: i64,
    data: &[u8],
    modern: bool,
) -> i32 {
    unsafe {
        elephc_dom_native_character_data_insert(
            node as *mut c_void,
            offset,
            data.as_ptr(),
            data.len(),
            i32::from(modern),
        )
    }
}

/// Deletes one UTF-8 code-point range.
pub(crate) fn character_data_delete(
    node: usize,
    offset: i64,
    count: i64,
    modern: bool,
) -> i32 {
    unsafe {
        elephc_dom_native_character_data_delete(
            node as *mut c_void,
            offset,
            count,
            i32::from(modern),
        )
    }
}

/// Replaces one UTF-8 code-point range with exact bytes.
pub(crate) fn character_data_replace(
    node: usize,
    offset: i64,
    count: i64,
    data: &[u8],
    modern: bool,
) -> i32 {
    unsafe {
        elephc_dom_native_character_data_replace(
            node as *mut c_void,
            offset,
            count,
            data.as_ptr(),
            data.len(),
            i32::from(modern),
        )
    }
}

/// Copies one text node's complete adjacent text/CDATA run.
pub(crate) fn text_whole_text(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_text_whole_text(node as *mut c_void)
    })
}

/// Splits one text node at a UTF-8 code-point offset.
pub(crate) fn text_split(node: usize, offset: i64) -> PointerOutcome {
    pointer_outcome(unsafe {
        elephc_dom_native_text_split(node as *mut c_void, offset)
    })
}

/// Reports whether libxml2 classifies one legacy text node as whitespace-only.
pub(crate) fn text_is_blank(node: usize) -> bool {
    unsafe { elephc_dom_native_text_is_blank(node as *mut c_void) == 1 }
}

/// Resolves one namespace URI for a nullable in-scope prefix.
pub(crate) fn node_lookup_namespace_uri(
    node: usize,
    prefix: Option<&[u8]>,
) -> Option<Vec<u8>> {
    let (pointer, length, default_namespace) = match prefix {
        Some(prefix) => (prefix.as_ptr(), prefix.len(), 0),
        None => (std::ptr::null(), 0, 1),
    };
    borrowed_buffer(unsafe {
        elephc_dom_native_node_lookup_namespace_uri(
            node as *mut c_void,
            pointer,
            length,
            default_namespace,
        )
    })
}

/// Resolves one in-scope prefix for a namespace URI.
pub(crate) fn node_lookup_prefix(node: usize, namespace_uri: &[u8]) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_node_lookup_prefix(
            node as *mut c_void,
            namespace_uri.as_ptr(),
            namespace_uri.len(),
        )
    })
}

/// Clones one node inside its current document and returns the detached clone root.
pub(crate) fn node_clone(
    node: usize,
    deep: bool,
    modern: bool,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_clone(
            node as *mut c_void,
            i32::from(deep),
            i32::from(modern),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Clones one document while retaining modern XML or HTML native metadata.
pub(crate) fn document_clone(
    document: usize,
    deep: bool,
    family: crate::objects::DocumentFamily,
) -> Option<usize> {
    let family = match family {
        crate::objects::DocumentFamily::Legacy => 0,
        crate::objects::DocumentFamily::ModernXml => 1,
        crate::objects::DocumentFamily::ModernHtml => 2,
    };
    let pointer = unsafe {
        elephc_dom_native_document_clone(
            document as *mut c_void,
            i32::from(deep),
            family,
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Imports one node into a target document.
pub(crate) fn document_import_node(
    document: usize,
    node: usize,
    deep: bool,
    modern: bool,
) -> PointerOutcome {
    pointer_outcome(unsafe {
        elephc_dom_native_document_import_node(
            document as *mut c_void,
            node as *mut c_void,
            i32::from(deep),
            i32::from(modern),
        )
    })
}

/// Adopts one node into a target document.
pub(crate) fn document_adopt_node(
    document: usize,
    node: usize,
    modern: bool,
) -> PointerOutcome {
    pointer_outcome(unsafe {
        elephc_dom_native_document_adopt_node(
            document as *mut c_void,
            node as *mut c_void,
            i32::from(modern),
        )
    })
}

/// Returns one connected element whose attribute is typed as the requested XML ID.
pub(crate) fn document_get_element_by_id(
    document: usize,
    id: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_document_get_element_by_id(
            document as *mut c_void,
            id.as_ptr(),
            id.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Counts one node's direct children of every native node kind.
pub(crate) fn node_child_count(node: usize) -> usize {
    unsafe { elephc_dom_native_node_child_count(node as *mut c_void) }
}

/// Returns one node's direct child by zero-based index.
pub(crate) fn node_child_at(node: usize, index: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_node_child_at(node as *mut c_void, index)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Counts descendant elements matching one qualified tag name.
pub(crate) fn descendant_element_count_name(
    root: usize,
    name: &[u8],
    match_local_name: bool,
) -> usize {
    unsafe {
        elephc_dom_native_descendant_element_count_name(
            root as *mut c_void,
            name.as_ptr(),
            name.len(),
            i32::from(match_local_name),
        )
    }
}

/// Returns the first descendant element in tree order.
pub(crate) fn descendant_element_first(root: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_descendant_element_first(root as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns the next descendant element in tree order within one root.
pub(crate) fn descendant_element_next(
    root: usize,
    current: usize,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_descendant_element_next(
            root as *mut c_void,
            current as *mut c_void,
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Copies PHP's ordered in-scope namespace records for one element subtree.
pub(crate) fn element_namespace_info(
    element: usize,
    include_descendants: bool,
) -> Result<Vec<NamespaceInfo>, ()> {
    let result = unsafe {
        elephc_dom_native_element_namespace_info(
            element as *mut c_void,
            i32::from(include_descendants),
        )
    };
    let malformed_items = result.count != 0 && result.items.is_null();
    let native_items = if malformed_items || result.count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(result.items, result.count) }
    };
    let malformed_record = native_items.iter().any(|item| {
        item.element.is_null()
            || (item.prefix.is_null() && item.prefix_length != 0)
            || (item.namespace_uri.is_null()
                && item.namespace_uri_length != 0)
    });
    let items = native_items
        .iter()
        .map(|item| NamespaceInfo {
            element: item.element as usize,
            prefix: (!item.prefix.is_null())
                .then(|| copy_native_bytes(item.prefix, item.prefix_length)),
            namespace_uri: (!item.namespace_uri.is_null()).then(|| {
                copy_native_bytes(item.namespace_uri, item.namespace_uri_length)
            }),
        })
        .collect::<Vec<_>>();
    unsafe {
        elephc_dom_native_namespace_info_result_free(result.items);
    }
    if result.allocation_failed != 0 || malformed_items || malformed_record {
        return Err(());
    }
    Ok(items)
}

/// Returns one qualified-name-matching descendant element by zero-based index.
pub(crate) fn descendant_element_at_name(
    root: usize,
    index: usize,
    name: &[u8],
    match_local_name: bool,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_descendant_element_at_name(
            root as *mut c_void,
            index,
            name.as_ptr(),
            name.len(),
            i32::from(match_local_name),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Counts descendant elements matching namespace URI and local name.
pub(crate) fn descendant_element_count_ns(
    root: usize,
    namespace_uri: Option<&[u8]>,
    local_name: &[u8],
) -> usize {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    unsafe {
        elephc_dom_native_descendant_element_count_ns(
            root as *mut c_void,
            namespace_pointer,
            namespace_length,
            local_name.as_ptr(),
            local_name.len(),
        )
    }
}

/// Returns one namespace-matching descendant element by zero-based index.
pub(crate) fn descendant_element_at_ns(
    root: usize,
    index: usize,
    namespace_uri: Option<&[u8]>,
    local_name: &[u8],
) -> Option<usize> {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    let pointer = unsafe {
        elephc_dom_native_descendant_element_at_ns(
            root as *mut c_void,
            index,
            namespace_pointer,
            namespace_length,
            local_name.as_ptr(),
            local_name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one element's attribute value by qualified name.
pub(crate) fn element_get_attribute(element: usize, name: &[u8]) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_element_get_attribute(
            element as *mut c_void,
            name.as_ptr(),
            name.len(),
        )
    })
}

/// Returns one element's borrowed attribute-node pointer by qualified name.
pub(crate) fn element_get_attribute_node(element: usize, name: &[u8]) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_get_attribute_node(
            element as *mut c_void,
            name.as_ptr(),
            name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one element's attribute value selected by namespace URI and local name.
pub(crate) fn element_get_attribute_ns(
    element: usize,
    namespace_uri: Option<&[u8]>,
    local_name: &[u8],
) -> Option<Vec<u8>> {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    owned_buffer(unsafe {
        elephc_dom_native_element_get_attribute_ns(
            element as *mut c_void,
            namespace_pointer,
            namespace_length,
            local_name.as_ptr(),
            local_name.len(),
        )
    })
}

/// Returns one borrowed attribute pointer selected by namespace URI and local name.
pub(crate) fn element_get_attribute_node_ns(
    element: usize,
    namespace_uri: Option<&[u8]>,
    local_name: &[u8],
) -> Option<usize> {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    let pointer = unsafe {
        elephc_dom_native_element_get_attribute_node_ns(
            element as *mut c_void,
            namespace_pointer,
            namespace_length,
            local_name.as_ptr(),
            local_name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates or updates one attribute and returns its native node pointer.
pub(crate) fn element_set_attribute(
    element: usize,
    name: &[u8],
    value: &[u8],
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_set_attribute(
            element as *mut c_void,
            name.as_ptr(),
            name.len(),
            value.as_ptr(),
            value.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Creates or updates one namespaced attribute with exact QName error reporting.
pub(crate) fn element_set_attribute_ns(
    element: usize,
    namespace_uri: Option<&[u8]>,
    qualified_name: &[u8],
    value: &[u8],
    modern: bool,
) -> PointerOutcome {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    pointer_outcome(unsafe {
        elephc_dom_native_element_set_attribute_ns(
            element as *mut c_void,
            namespace_pointer,
            namespace_length,
            qualified_name.as_ptr(),
            qualified_name.len(),
            value.as_ptr(),
            value.len(),
            i32::from(modern),
        )
    })
}

/// Adopts one detached attribute into another document without changing its pointer.
pub(crate) fn attribute_adopt(attribute: usize, document: usize) -> bool {
    unsafe {
        elephc_dom_native_attribute_adopt(
            attribute as *mut c_void,
            document as *mut c_void,
        ) == 1
    }
}

/// Attaches one attribute node and returns a replaced attribute when present.
pub(crate) fn element_set_attribute_node(
    element: usize,
    attribute: usize,
    use_namespace: bool,
) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_set_attribute_node(
            element as *mut c_void,
            attribute as *mut c_void,
            i32::from(use_namespace),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Detaches one exact attribute node from its current element.
pub(crate) fn element_remove_attribute_node(element: usize, attribute: usize) -> bool {
    unsafe {
        elephc_dom_native_element_remove_attribute_node(
            element as *mut c_void,
            attribute as *mut c_void,
        ) == 1
    }
}

/// Detaches one attribute and returns its stable native pointer.
pub(crate) fn element_remove_attribute(element: usize, name: &[u8]) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_remove_attribute(
            element as *mut c_void,
            name.as_ptr(),
            name.len(),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Detaches one namespaced attribute or eliminates one legacy declaration.
pub(crate) fn element_remove_attribute_ns(
    element: usize,
    namespace_uri: Option<&[u8]>,
    local_name: &[u8],
    legacy: bool,
) -> Option<usize> {
    let (namespace_pointer, namespace_length) = optional_bytes_pointer(namespace_uri);
    let pointer = unsafe {
        elephc_dom_native_element_remove_attribute_ns(
            element as *mut c_void,
            namespace_pointer,
            namespace_length,
            local_name.as_ptr(),
            local_name.len(),
            i32::from(legacy),
        )
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns every qualified attribute name in PHP's stable native order.
pub(crate) fn element_attribute_names(
    element: usize,
    include_namespace_declarations: bool,
) -> Option<Vec<Vec<u8>>> {
    let include = i32::from(include_namespace_declarations);
    let count = unsafe {
        elephc_dom_native_element_attribute_count(
            element as *mut c_void,
            include,
        )
    };
    let mut names = Vec::with_capacity(count);
    for index in 0..count {
        names.push(owned_buffer(unsafe {
            elephc_dom_native_element_attribute_name_at(
                element as *mut c_void,
                index,
                include,
            )
        })?);
    }
    Some(names)
}

/// Counts one element's ordinary attributes without allocating their names.
pub(crate) fn element_attribute_count(element: usize) -> usize {
    unsafe {
        elephc_dom_native_element_attribute_count(
            element as *mut c_void,
            0,
        )
    }
}

/// Returns one ordinary attribute pointer by zero-based index.
pub(crate) fn element_attribute_at(element: usize, index: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_attribute_at(element as *mut c_void, index)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one element's first direct element child.
pub(crate) fn element_first_child(element: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_first_child(element as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one element's last direct element child.
pub(crate) fn element_last_child(element: usize) -> Option<usize> {
    let pointer =
        unsafe { elephc_dom_native_element_last_child(element as *mut c_void) };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one element's previous element sibling.
pub(crate) fn element_previous_sibling(element: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_previous_sibling(element as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Returns one element's next element sibling.
pub(crate) fn element_next_sibling(element: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_element_next_sibling(element as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Counts one element's direct element children.
pub(crate) fn element_child_count(element: usize) -> i64 {
    unsafe { elephc_dom_native_element_child_count(element as *mut c_void) }
}

/// Frees one detached libxml2 node tree owned by context cleanup.
pub(crate) unsafe fn node_free(node: usize) {
    elephc_dom_native_node_free(node as *mut c_void);
}

/// Frees one standalone fake namespace-declaration node owned by a released wrapper.
pub(crate) unsafe fn namespace_node_free(node: usize) {
    elephc_dom_native_namespace_node_free(node as *mut c_void);
}

/// Frees one synthesized DTD notation node owned by its released wrapper.
pub(crate) unsafe fn notation_node_free(node: usize) {
    elephc_dom_native_notation_node_free(node as *mut c_void);
}

/// Clones one fake namespace-declaration node into a fresh standalone allocation.
pub(crate) fn namespace_node_clone(node: usize) -> Option<usize> {
    let pointer = unsafe {
        elephc_dom_native_namespace_node_clone(node as *mut c_void)
    };
    (!pointer.is_null()).then_some(pointer as usize)
}

/// Copies and frees one fake namespace-declaration node's PHP node name.
pub(crate) fn namespace_node_name(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_namespace_node_name(node as *mut c_void)
    })
}

/// Copies and frees one fake namespace-declaration node's PHP node value.
pub(crate) fn namespace_node_value(node: usize) -> Option<Vec<u8>> {
    owned_buffer(unsafe {
        elephc_dom_native_namespace_node_value(node as *mut c_void)
    })
}

/// Copies one fake namespace-declaration node's PHP local name from live storage.
pub(crate) fn namespace_node_local_name(node: usize) -> Option<Vec<u8>> {
    borrowed_buffer(unsafe {
        elephc_dom_native_namespace_node_local_name(node as *mut c_void)
    })
}

/// Copies a borrowed native buffer whose storage remains engine-owned.
fn borrowed_buffer(buffer: NativeBuffer) -> Option<Vec<u8>> {
    if buffer.pointer.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(buffer.pointer, buffer.length) }.to_vec())
}

/// Copies and frees one libxml2-owned native buffer.
fn owned_buffer(buffer: NativeBuffer) -> Option<Vec<u8>> {
    if buffer.pointer.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(buffer.pointer, buffer.length) }.to_vec();
    unsafe {
        elephc_dom_native_buffer_free(buffer.pointer);
    }
    Some(bytes)
}

/// Copies and frees one native owned byte/error record.
fn buffer_outcome(result: NativeBufferResult) -> BufferOutcome {
    let bytes = owned_buffer(NativeBuffer {
        pointer: result.pointer,
        length: result.length,
    });
    BufferOutcome {
        bytes,
        error_code: result.error_code,
    }
}

/// Converts one optional byte slice into the nullable pointer/length native pair.
fn optional_bytes_pointer(bytes: Option<&[u8]>) -> (*const u8, usize) {
    bytes
        .map(|bytes| (bytes.as_ptr(), bytes.len()))
        .unwrap_or((std::ptr::null(), 0))
}

/// Copies one native pointer/error record into a Rust-owned scalar outcome.
fn pointer_outcome(result: NativePointerResult) -> PointerOutcome {
    PointerOutcome {
        pointer: (!result.pointer.is_null()).then_some(result.pointer as usize),
        error_code: result.error_code,
    }
}

/// Appends one child through SimpleXML's permissive QName and namespace rules.
pub(crate) fn simplexml_add_child(
    document: usize,
    node: usize,
    qualified_name: &[u8],
    value: Option<&[u8]>,
    namespace_uri: Option<&[u8]>,
) -> PointerOutcome {
    let (value_pointer, value_length) = optional_bytes_pointer(value);
    let (namespace_pointer, namespace_length) =
        optional_bytes_pointer(namespace_uri);
    pointer_outcome(unsafe {
        elephc_dom_native_simplexml_add_child(
            document as *mut c_void,
            node as *mut c_void,
            qualified_name.as_ptr(),
            qualified_name.len(),
            value_pointer,
            value_length,
            i32::from(value.is_some()),
            namespace_pointer,
            namespace_length,
            i32::from(namespace_uri.is_some()),
        )
    })
}

/// Appends one attribute and returns php-src-compatible warning status codes.
pub(crate) fn simplexml_add_attribute(
    document: usize,
    node: usize,
    qualified_name: &[u8],
    value: &[u8],
    namespace_uri: Option<&[u8]>,
) -> i32 {
    let (namespace_pointer, namespace_length) =
        optional_bytes_pointer(namespace_uri);
    unsafe {
        elephc_dom_native_simplexml_add_attribute(
            document as *mut c_void,
            node as *mut c_void,
            qualified_name.as_ptr(),
            qualified_name.len(),
            value.as_ptr(),
            value.len(),
            namespace_pointer,
            namespace_length,
            i32::from(namespace_uri.is_some()),
        )
    }
}

/// Collects the first prefix-to-URI binding for each namespace used by a subtree.
pub(crate) fn simplexml_get_namespaces(
    node: usize,
    recursive: bool,
) -> Result<SimpleXmlNamespaceOutcome, ()> {
    copy_simplexml_namespace_result(unsafe {
        elephc_dom_native_simplexml_get_namespaces(
            node as *mut c_void,
            i32::from(recursive),
        )
    })
}

/// Collects the first prefix-to-URI binding for each selected namespace declaration.
pub(crate) fn simplexml_get_doc_namespaces(
    document: usize,
    node: Option<usize>,
    recursive: bool,
    from_root: bool,
    include_xmlns_attributes: bool,
) -> Result<SimpleXmlNamespaceOutcome, ()> {
    copy_simplexml_namespace_result(unsafe {
        elephc_dom_native_simplexml_get_doc_namespaces(
            document as *mut c_void,
            node.unwrap_or(0) as *mut c_void,
            i32::from(recursive),
            i32::from(from_root),
            i32::from(include_xmlns_attributes),
        )
    })
}

/// Copies and releases one native SimpleXML namespace result atomically.
fn copy_simplexml_namespace_result(
    mut result: NativeSimpleXmlNamespaceResult,
) -> Result<SimpleXmlNamespaceOutcome, ()> {
    let malformed_items = result.count != 0 && result.items.is_null();
    let items = if malformed_items || result.count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(result.items, result.count) }
    };
    let malformed_row = items.iter().any(|item| {
        (item.prefix.is_null() && item.prefix_length != 0)
            || (item.namespace_uri.is_null()
                && item.namespace_uri_length != 0)
    });
    let copied = items
        .iter()
        .map(|item| {
            (
                copy_native_bytes(item.prefix, item.prefix_length),
                copy_native_bytes(
                    item.namespace_uri,
                    item.namespace_uri_length,
                ),
            )
        })
        .collect::<Vec<_>>();
    unsafe {
        elephc_dom_native_simplexml_namespace_result_free(&mut result);
    }
    if result.allocation_failed != 0 || malformed_items || malformed_row {
        return Err(());
    }
    Ok(SimpleXmlNamespaceOutcome { items: copied })
}

#[cfg(test)]
mod tests {
    use super::{
        copy_and_free_native_errors, copy_native_errors, NativeError,
        NativeResultAbiError,
    };
    use crate::objects::LibxmlErrorObject;

    /// Rejects a nonzero native error count without an owned error-array pointer.
    #[test]
    fn malformed_null_error_array_is_not_copied_or_freed() {
        assert_eq!(
            copy_and_free_native_errors(std::ptr::null_mut(), 1),
            Err(NativeResultAbiError::MalformedRequest)
        );
    }

    /// Preserves the C error-record layout while copying every borrowed byte range.
    #[test]
    fn native_error_records_copy_all_fields_from_the_c_layout() {
        let mut message = b"native-message".to_vec();
        let mut file = b"native-file.xml".to_vec();
        let error = NativeError {
            level: 2,
            domain: 3,
            code: 4,
            line: 5,
            column: 6,
            reserved: 0,
            message: message.as_mut_ptr(),
            message_length: message.len(),
            file: file.as_mut_ptr(),
            file_length: file.len(),
        };

        assert_eq!(std::mem::size_of::<NativeError>(), 56);
        assert_eq!(
            copy_native_errors(&error, 1),
            Ok(vec![LibxmlErrorObject {
                level: 2,
                domain: 3,
                code: 4,
                line: 5,
                column: 6,
                message,
                file,
            }])
        );
    }
}
