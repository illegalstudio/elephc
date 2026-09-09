//! Purpose:
//! Raw `extern "C"` declarations for the libxml2 2.15.3 public functions this crate calls
//! and for the Elephc-owned shim (`src/native_deps/recipes/libxml2_shim.c`) that owns
//! every access to libxml2's struct internals, plus the opaque pointer types and the SAX
//! callback table shared with the shim.
//!
//! Called from:
//! - `crate::parser` (shim parser functions, SAX callback table) and `crate::writer`
//!   (`xmlTextWriter*`, `xmlOutputBufferCreateIO`, `xmlValidateName`).
//!
//! Key details:
//! - These symbols are declared, never linked, at `cargo build -p elephc-xml` time (see
//!   `build.rs` and `crates/elephc-xml/Cargo.toml`), exactly like `crates/elephc-curl`
//!   declares libcurl: no bindgen, no `-sys` crate, no headers at cargo build time.
//! - Every libxml2 pointer type is an uninhabited enum: nothing here declares the layout
//!   of `xmlParserCtxt`, `xmlEntity`, `xmlTextWriter` or `xmlOutputBuffer`. When a new
//!   internal field is needed, the shim grows a function; Rust never reaches in.
//! - `Sax` mirrors `elephc_libxml2_v1_sax` field for field, in the same order, with the
//!   C signatures (`xmlChar` is `unsigned char`).
//! - Signatures come from the pinned headers (`include/libxml/{parser,xmlwriter,xmlIO,
//!   tree}.h`) and the shim source; the shim's `elephc_libxml2_v1_version` test pins the
//!   archive to 21503.

use std::ffi::{c_char, c_int, c_void};
use std::sync::Once;

/// libxml2's `xmlChar`: an unsigned byte of UTF-8.
pub(crate) type XmlChar = u8;

/// The opaque handle `elephc_libxml2_v1_parser_create` returns (`void *` in the shim).
pub(crate) enum ShimParser {}

/// libxml2's `xmlEntity`, only ever seen as a pointer handed back by the shim.
pub(crate) enum XmlEntity {}

/// libxml2's `xmlTextWriter`.
pub(crate) enum XmlTextWriter {}

/// libxml2's `xmlOutputBuffer`.
pub(crate) enum XmlOutputBuffer {}

/// libxml2's `xmlCharEncodingHandler`; only ever passed as null.
pub(crate) enum XmlCharEncodingHandler {}

/// `xmlOutputWriteCallback`: appends `len` bytes, returning the count written or -1.
pub(crate) type OutputWriteCallback =
    Option<unsafe extern "C" fn(context: *mut c_void, buffer: *const c_char, len: c_int) -> c_int>;

/// `xmlOutputCloseCallback`: releases the sink, returning 0 on success.
pub(crate) type OutputCloseCallback = Option<unsafe extern "C" fn(context: *mut c_void) -> c_int>;

/// SAX1 `startElement`: name plus a NULL-terminated array of name/value pairs (or NULL).
pub(crate) type StartElementFn =
    unsafe extern "C" fn(user: *mut c_void, name: *const XmlChar, attributes: *const *const XmlChar);

/// SAX1 `endElement`.
pub(crate) type EndElementFn = unsafe extern "C" fn(user: *mut c_void, name: *const XmlChar);

/// SAX2 `startElementNs`: local name, prefix, URI, `nb_namespaces` prefix/URI pairs,
/// `nb_attributes` five-tuples (local name, prefix, URI, value start, value end).
pub(crate) type StartElementNsFn = unsafe extern "C" fn(
    user: *mut c_void,
    localname: *const XmlChar,
    prefix: *const XmlChar,
    uri: *const XmlChar,
    nb_namespaces: c_int,
    namespaces: *const *const XmlChar,
    nb_attributes: c_int,
    nb_defaulted: c_int,
    attributes: *const *const XmlChar,
);

/// SAX2 `endElementNs`.
pub(crate) type EndElementNsFn = unsafe extern "C" fn(
    user: *mut c_void,
    localname: *const XmlChar,
    prefix: *const XmlChar,
    uri: *const XmlChar,
);

/// `characters` / `cdataBlock`: `len` bytes of character data.
pub(crate) type CharactersFn = unsafe extern "C" fn(user: *mut c_void, data: *const XmlChar, len: c_int);

/// `processingInstruction`: target plus data (NULL when the PI has none).
pub(crate) type ProcessingInstructionFn =
    unsafe extern "C" fn(user: *mut c_void, target: *const XmlChar, data: *const XmlChar);

/// `comment`: the comment text without delimiters.
pub(crate) type CommentFn = unsafe extern "C" fn(user: *mut c_void, value: *const XmlChar);

/// `getEntity`: resolves a name to an entity pointer (or NULL).
pub(crate) type GetEntityFn =
    unsafe extern "C" fn(user: *mut c_void, name: *const XmlChar) -> *mut XmlEntity;

/// `notationDecl`: name, public id, system id (each id may be NULL).
pub(crate) type NotationDeclFn = unsafe extern "C" fn(
    user: *mut c_void,
    name: *const XmlChar,
    public_id: *const XmlChar,
    system_id: *const XmlChar,
);

/// `unparsedEntityDecl`: name, public id, system id, notation name.
pub(crate) type UnparsedEntityDeclFn = unsafe extern "C" fn(
    user: *mut c_void,
    name: *const XmlChar,
    public_id: *const XmlChar,
    system_id: *const XmlChar,
    notation_name: *const XmlChar,
);

/// `elephc_libxml2_v1_sax`: the compat.c SAX subset the bridge implements. Field order is
/// the shim's, verbatim; every callback receives the `user` pointer given at creation.
#[repr(C)]
pub(crate) struct Sax {
    pub(crate) start_element: Option<StartElementFn>,
    pub(crate) end_element: Option<EndElementFn>,
    pub(crate) start_element_ns: Option<StartElementNsFn>,
    pub(crate) end_element_ns: Option<EndElementNsFn>,
    pub(crate) characters: Option<CharactersFn>,
    pub(crate) processing_instruction: Option<ProcessingInstructionFn>,
    pub(crate) comment: Option<CommentFn>,
    pub(crate) get_entity: Option<GetEntityFn>,
    pub(crate) notation_decl: Option<NotationDeclFn>,
    pub(crate) unparsed_entity_decl: Option<UnparsedEntityDeclFn>,
}

// The table is only ever read by the shim, which copies the pointers into its own
// `xmlSAXHandler`; sharing it between threads is therefore sound.
unsafe impl Sync for Sax {}

extern "C" {
    // --- libxml2 public API (include/libxml/parser.h, tree.h, xmlIO.h, xmlwriter.h) ---

    /// `xmlInitParser`: one-time global initialization; idempotent and thread-safe.
    pub(crate) fn xmlInitParser();
    /// `xmlValidateName`: 0 when `value` is an XML `Name` (`space` 0 forbids whitespace).
    pub(crate) fn xmlValidateName(value: *const XmlChar, space: c_int) -> c_int;
    /// `xmlOutputBufferCreateIO`: an output buffer writing through callbacks.
    pub(crate) fn xmlOutputBufferCreateIO(
        iowrite: OutputWriteCallback,
        ioclose: OutputCloseCallback,
        ioctx: *mut c_void,
        encoder: *mut XmlCharEncodingHandler,
    ) -> *mut XmlOutputBuffer;
    /// `xmlOutputBufferClose`: flushes, invokes the close callback and frees the buffer.
    pub(crate) fn xmlOutputBufferClose(out: *mut XmlOutputBuffer) -> c_int;
    /// `xmlNewTextWriter`: a writer owning `out` (freed with the writer).
    pub(crate) fn xmlNewTextWriter(out: *mut XmlOutputBuffer) -> *mut XmlTextWriter;
    /// `xmlFreeTextWriter`: frees the writer and closes its output buffer.
    pub(crate) fn xmlFreeTextWriter(writer: *mut XmlTextWriter);
    /// `xmlTextWriterFlush`: bytes flushed, or -1 when the output buffer is in error.
    pub(crate) fn xmlTextWriterFlush(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterSetIndent`.
    pub(crate) fn xmlTextWriterSetIndent(writer: *mut XmlTextWriter, indent: c_int) -> c_int;
    /// `xmlTextWriterSetIndentString`.
    pub(crate) fn xmlTextWriterSetIndentString(writer: *mut XmlTextWriter, string: *const XmlChar) -> c_int;
    /// `xmlTextWriterStartDocument`; every string may be NULL.
    pub(crate) fn xmlTextWriterStartDocument(
        writer: *mut XmlTextWriter,
        version: *const c_char,
        encoding: *const c_char,
        standalone: *const c_char,
    ) -> c_int;
    /// `xmlTextWriterEndDocument`.
    pub(crate) fn xmlTextWriterEndDocument(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterStartComment`.
    pub(crate) fn xmlTextWriterStartComment(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterEndComment`.
    pub(crate) fn xmlTextWriterEndComment(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteComment`.
    pub(crate) fn xmlTextWriterWriteComment(writer: *mut XmlTextWriter, content: *const XmlChar) -> c_int;
    /// `xmlTextWriterStartElement`.
    pub(crate) fn xmlTextWriterStartElement(writer: *mut XmlTextWriter, name: *const XmlChar) -> c_int;
    /// `xmlTextWriterStartElementNS`; prefix and URI may be NULL.
    pub(crate) fn xmlTextWriterStartElementNS(
        writer: *mut XmlTextWriter,
        prefix: *const XmlChar,
        name: *const XmlChar,
        namespace_uri: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterEndElement`.
    pub(crate) fn xmlTextWriterEndElement(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterFullEndElement`.
    pub(crate) fn xmlTextWriterFullEndElement(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteElement`.
    pub(crate) fn xmlTextWriterWriteElement(
        writer: *mut XmlTextWriter,
        name: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterWriteElementNS`.
    pub(crate) fn xmlTextWriterWriteElementNS(
        writer: *mut XmlTextWriter,
        prefix: *const XmlChar,
        name: *const XmlChar,
        namespace_uri: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterStartAttribute`.
    pub(crate) fn xmlTextWriterStartAttribute(writer: *mut XmlTextWriter, name: *const XmlChar) -> c_int;
    /// `xmlTextWriterStartAttributeNS`.
    pub(crate) fn xmlTextWriterStartAttributeNS(
        writer: *mut XmlTextWriter,
        prefix: *const XmlChar,
        name: *const XmlChar,
        namespace_uri: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterEndAttribute`.
    pub(crate) fn xmlTextWriterEndAttribute(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteAttribute`.
    pub(crate) fn xmlTextWriterWriteAttribute(
        writer: *mut XmlTextWriter,
        name: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterWriteAttributeNS`.
    pub(crate) fn xmlTextWriterWriteAttributeNS(
        writer: *mut XmlTextWriter,
        prefix: *const XmlChar,
        name: *const XmlChar,
        namespace_uri: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterStartPI`.
    pub(crate) fn xmlTextWriterStartPI(writer: *mut XmlTextWriter, target: *const XmlChar) -> c_int;
    /// `xmlTextWriterEndPI`.
    pub(crate) fn xmlTextWriterEndPI(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWritePI`.
    pub(crate) fn xmlTextWriterWritePI(
        writer: *mut XmlTextWriter,
        target: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterStartCDATA`.
    pub(crate) fn xmlTextWriterStartCDATA(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterEndCDATA`.
    pub(crate) fn xmlTextWriterEndCDATA(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteCDATA`.
    pub(crate) fn xmlTextWriterWriteCDATA(writer: *mut XmlTextWriter, content: *const XmlChar) -> c_int;
    /// `xmlTextWriterWriteString`.
    pub(crate) fn xmlTextWriterWriteString(writer: *mut XmlTextWriter, content: *const XmlChar) -> c_int;
    /// `xmlTextWriterWriteRaw`.
    pub(crate) fn xmlTextWriterWriteRaw(writer: *mut XmlTextWriter, content: *const XmlChar) -> c_int;
    /// `xmlTextWriterStartDTD`; public and system id may be NULL.
    pub(crate) fn xmlTextWriterStartDTD(
        writer: *mut XmlTextWriter,
        name: *const XmlChar,
        pubid: *const XmlChar,
        sysid: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterEndDTD`.
    pub(crate) fn xmlTextWriterEndDTD(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteDTD`; ids and subset may be NULL.
    pub(crate) fn xmlTextWriterWriteDTD(
        writer: *mut XmlTextWriter,
        name: *const XmlChar,
        pubid: *const XmlChar,
        sysid: *const XmlChar,
        subset: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterStartDTDElement`.
    pub(crate) fn xmlTextWriterStartDTDElement(writer: *mut XmlTextWriter, name: *const XmlChar) -> c_int;
    /// `xmlTextWriterEndDTDElement`.
    pub(crate) fn xmlTextWriterEndDTDElement(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteDTDElement`.
    pub(crate) fn xmlTextWriterWriteDTDElement(
        writer: *mut XmlTextWriter,
        name: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterStartDTDAttlist`.
    pub(crate) fn xmlTextWriterStartDTDAttlist(writer: *mut XmlTextWriter, name: *const XmlChar) -> c_int;
    /// `xmlTextWriterEndDTDAttlist`.
    pub(crate) fn xmlTextWriterEndDTDAttlist(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteDTDAttlist`.
    pub(crate) fn xmlTextWriterWriteDTDAttlist(
        writer: *mut XmlTextWriter,
        name: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterStartDTDEntity`; `pe` selects a parameter entity.
    pub(crate) fn xmlTextWriterStartDTDEntity(
        writer: *mut XmlTextWriter,
        pe: c_int,
        name: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterEndDTDEntity`.
    pub(crate) fn xmlTextWriterEndDTDEntity(writer: *mut XmlTextWriter) -> c_int;
    /// `xmlTextWriterWriteDTDInternalEntity`.
    pub(crate) fn xmlTextWriterWriteDTDInternalEntity(
        writer: *mut XmlTextWriter,
        pe: c_int,
        name: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterWriteDTDExternalEntity`; ids and notation may be NULL.
    pub(crate) fn xmlTextWriterWriteDTDExternalEntity(
        writer: *mut XmlTextWriter,
        pe: c_int,
        name: *const XmlChar,
        pubid: *const XmlChar,
        sysid: *const XmlChar,
        ndataid: *const XmlChar,
    ) -> c_int;
    /// `xmlTextWriterWriteDTDEntity`: the internal/external dispatcher PHP calls.
    pub(crate) fn xmlTextWriterWriteDTDEntity(
        writer: *mut XmlTextWriter,
        pe: c_int,
        name: *const XmlChar,
        pubid: *const XmlChar,
        sysid: *const XmlChar,
        ndataid: *const XmlChar,
        content: *const XmlChar,
    ) -> c_int;

    // --- Elephc-owned shim (src/native_deps/recipes/libxml2_shim.c) ---

    /// `LIBXML_VERSION` the shim was compiled against (21503 for 2.15.3); only the
    /// version-pin test reads it, production code has no use for it.
    #[cfg_attr(not(all(test, elephc_xml_native)), allow(dead_code))]
    pub(crate) fn elephc_libxml2_v1_version() -> i32;
    /// `XML_ParserCreate_MM`: a push parser with compat.c's options; NULL on allocation
    /// failure. `user` is what every callback in `callbacks` receives.
    pub(crate) fn elephc_libxml2_v1_parser_create(
        callbacks: *const Sax,
        user: *mut c_void,
        use_namespaces: i32,
    ) -> *mut ShimParser;
    /// `XML_ParserFree`.
    pub(crate) fn elephc_libxml2_v1_parser_free(handle: *mut ShimParser);
    /// `xml_parse_helper`'s per-chunk `XML_OPTION_PARSE_HUGE` application.
    pub(crate) fn elephc_libxml2_v1_set_huge(handle: *mut ShimParser, huge: i32);
    /// `XML_Parse`: 1 when the chunk parsed without an error above warning level.
    pub(crate) fn elephc_libxml2_v1_parse_chunk(
        handle: *mut ShimParser,
        data: *const c_char,
        len: i32,
        terminate: i32,
    ) -> i32;
    /// `XML_GetErrorCode` (`ctxt->errNo`).
    pub(crate) fn elephc_libxml2_v1_error_code(handle: *mut ShimParser) -> i32;
    /// compat.c's external-entity failure path: `xmlStopParser`, then `errNo = code`.
    pub(crate) fn elephc_libxml2_v1_stop(handle: *mut ShimParser, error_code: i32);
    /// `XML_GetCurrentLineNumber`.
    pub(crate) fn elephc_libxml2_v1_line(handle: *mut ShimParser) -> i32;
    /// `XML_GetCurrentColumnNumber`.
    pub(crate) fn elephc_libxml2_v1_column(handle: *mut ShimParser) -> i32;
    /// `XML_GetCurrentByteIndex`.
    pub(crate) fn elephc_libxml2_v1_byte_index(handle: *mut ShimParser) -> i64;
    /// The source text of the tag being reported (`<` up to the cursor, inclusive), as
    /// `start_element_emit_default` reads it; the length written to `*out_start`.
    pub(crate) fn elephc_libxml2_v1_current_tag(handle: *mut ShimParser, out_start: *mut *const c_char) -> i64;
    /// `ctxt->inSubset`.
    pub(crate) fn elephc_libxml2_v1_in_subset(handle: *mut ShimParser) -> i32;
    /// `ctxt->instate == XML_PARSER_CONTENT`.
    pub(crate) fn elephc_libxml2_v1_in_content(handle: *mut ShimParser) -> i32;
    /// `xmlGetPredefinedEntity`, then `xmlGetDocEntity` on the parser's document.
    pub(crate) fn elephc_libxml2_v1_lookup_entity(handle: *mut ShimParser, name: *const XmlChar) -> *mut XmlEntity;
    /// `xmlEntity->etype` (`xmlEntityType` numbering), 0 for NULL.
    pub(crate) fn elephc_libxml2_v1_entity_type(entity: *mut XmlEntity) -> i32;
    /// `xmlEntity->name`.
    pub(crate) fn elephc_libxml2_v1_entity_name(entity: *mut XmlEntity) -> *const XmlChar;
    /// `xmlEntity->content`.
    pub(crate) fn elephc_libxml2_v1_entity_content(entity: *mut XmlEntity) -> *const XmlChar;
    /// `xmlEntity->SystemID`.
    pub(crate) fn elephc_libxml2_v1_entity_system_id(entity: *mut XmlEntity) -> *const XmlChar;
    /// `xmlEntity->ExternalID`.
    pub(crate) fn elephc_libxml2_v1_entity_external_id(entity: *mut XmlEntity) -> *const XmlChar;
}

/// `xmlEntityType::XML_INTERNAL_GENERAL_ENTITY`.
pub(crate) const XML_INTERNAL_GENERAL_ENTITY: i32 = 1;
/// `xmlEntityType::XML_EXTERNAL_GENERAL_PARSED_ENTITY`.
pub(crate) const XML_EXTERNAL_GENERAL_PARSED_ENTITY: i32 = 2;
/// `xmlEntityType::XML_EXTERNAL_GENERAL_UNPARSED_ENTITY`.
pub(crate) const XML_EXTERNAL_GENERAL_UNPARSED_ENTITY: i32 = 3;
/// `xmlEntityType::XML_INTERNAL_PARAMETER_ENTITY`.
pub(crate) const XML_INTERNAL_PARAMETER_ENTITY: i32 = 4;
/// `xmlEntityType::XML_INTERNAL_PREDEFINED_ENTITY`.
pub(crate) const XML_INTERNAL_PREDEFINED_ENTITY: i32 = 6;

/// Guards the one-time `xmlInitParser` call.
static INIT: Once = Once::new();

/// Runs libxml2's global initialization exactly once per process. Called before the first
/// parser or writer is created; later calls are free.
pub(crate) fn init_library() {
    INIT.call_once(|| unsafe { xmlInitParser() });
}

/// Copies a NUL-terminated `xmlChar` string; `None` for a null pointer.
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated byte string that outlives the call.
pub(crate) unsafe fn opt_bytes(ptr: *const XmlChar) -> Option<Vec<u8>> {
    if ptr.is_null() {
        None
    } else {
        Some(std::ffi::CStr::from_ptr(ptr as *const c_char).to_bytes().to_vec())
    }
}

/// Copies a NUL-terminated `xmlChar` string; empty for a null pointer.
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated byte string that outlives the call.
pub(crate) unsafe fn bytes(ptr: *const XmlChar) -> Vec<u8> {
    opt_bytes(ptr).unwrap_or_default()
}
