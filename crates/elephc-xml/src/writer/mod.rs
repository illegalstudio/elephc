//! Purpose:
//! PHP's in-memory `XMLWriter` as libxml2 2.15.3's `xmlTextWriter` over an
//! `xmlOutputBufferCreateIO` sink that appends to an owned byte buffer, exactly how
//! `ext/xmlwriter/php_xmlwriter.c` builds `XMLWriter::openMemory()` / `toMemory()`.
//!
//! Called from:
//! - `crate::abi` (the `elephc_xml_writer_*` entry points) and the crate unit tests.
//!
//! Key details:
//! - Every method mirrors one `xmlTextWriter*` call and returns `true` where libxml2
//!   returns anything but -1, which is php-src's own success test.
//! - Strings crossing to C are NUL-terminated copies; a name or content holding an
//!   interior NUL is passed truncated at the NUL, like php-src's C-string arguments.
//! - PHP-level argument validation (`must be a valid element name`), stream handling and
//!   the `outputMemory`/`flush` return shapes live above this module; `is_valid_name` is
//!   the `xmlValidateName` check that layer applies.
//! - `output()` exposes what libxml2 has delivered to the sink so far, `flush()` /
//!   `take_output()` run `xmlTextWriterFlush` first, exactly like `php_xmlwriter_flush`.

mod sink;
#[cfg(all(test, elephc_xml_native))]
mod tests;

use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr;

use crate::ffi::{self, XmlChar, XmlTextWriter};
use sink::Sink;

/// Returns whether `name` is an XML `Name`, exactly like `xmlValidateName(name, 0) == 0`
/// (interior NULs truncate the name first, as they do for php-src's C strings).
pub fn is_valid_name(name: &[u8]) -> bool {
    ffi::init_library();
    let name = cstring(name);
    // SAFETY: `name` is a valid NUL-terminated string for the duration of the call.
    unsafe { ffi::xmlValidateName(name.as_ptr() as *const XmlChar, 0) == 0 }
}

/// A NUL-terminated copy of `bytes`, truncated at the first interior NUL.
fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    // A NUL-free slice always converts.
    CString::new(&bytes[..end]).unwrap_or_default()
}

/// An optional NUL-terminated copy (`None` stays NULL on the C side).
fn opt_cstring(bytes: Option<&[u8]>) -> Option<CString> {
    bytes.map(cstring)
}

/// The `xmlChar *` for an owned C string.
fn xml_ptr(string: &CString) -> *const XmlChar {
    string.as_ptr() as *const XmlChar
}

/// The `xmlChar *` for an optional owned C string, NULL for `None`.
fn opt_xml_ptr(string: &Option<CString>) -> *const XmlChar {
    string.as_ref().map_or(ptr::null(), xml_ptr)
}

/// The `char *` for an optional owned C string, NULL for `None`.
fn opt_char_ptr(string: &Option<CString>) -> *const c_char {
    string.as_ref().map_or(ptr::null(), |s| s.as_ptr())
}

/// The text writer.
pub struct Writer {
    /// The `xmlTextWriter`; null when libxml2 could not allocate it, in which case every
    /// call fails and the output stays empty.
    writer: *mut XmlTextWriter,
    /// The sink the writer's output buffer appends to; owned by that buffer (freed by its
    /// close callback) and valid exactly as long as `writer` is.
    sink: *mut Sink,
}

// A writer is used from whichever thread holds the ABI registry lock; libxml2 writers are
// not tied to the creating thread, and the sink is only reached through the raw pointer.
unsafe impl Send for Writer {}

impl Writer {
    /// Creates a memory writer (`xml_writer_create_in_memory`): an IO output buffer
    /// writing into a fresh sink, wrapped by `xmlNewTextWriter`.
    pub fn new() -> Self {
        ffi::init_library();
        let sink = Box::into_raw(Box::new(Sink::default()));
        // SAFETY: `sink` is a live allocation handed to libxml2, which returns it to the
        // write/close callbacks in `sink` and frees it through `close` exactly once.
        unsafe {
            let out = ffi::xmlOutputBufferCreateIO(
                Some(sink::write),
                Some(sink::close),
                sink as *mut c_void,
                ptr::null_mut(),
            );
            if out.is_null() {
                drop(Box::from_raw(sink));
                return Writer { writer: ptr::null_mut(), sink: ptr::null_mut() };
            }
            let writer = ffi::xmlNewTextWriter(out);
            if writer.is_null() {
                // Closing the buffer runs the close callback, which frees the sink.
                ffi::xmlOutputBufferClose(out);
                return Writer { writer: ptr::null_mut(), sink: ptr::null_mut() };
            }
            Writer { writer, sink }
        }
    }

    /// Runs one `xmlTextWriter*` call and applies php-src's success test (`!= -1`).
    fn call(&mut self, body: impl FnOnce(*mut XmlTextWriter) -> c_int) -> bool {
        if self.writer.is_null() {
            return false;
        }
        body(self.writer) != -1
    }

    /// `xmlTextWriterSetIndent`.
    pub fn set_indent(&mut self, indent: bool) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterSetIndent(w, c_int::from(indent)) })
    }

    /// `xmlTextWriterSetIndentString`.
    pub fn set_indent_string(&mut self, indent: &[u8]) -> bool {
        let indent = cstring(indent);
        self.call(|w| unsafe { ffi::xmlTextWriterSetIndentString(w, xml_ptr(&indent)) })
    }

    /// `xmlTextWriterStartDocument`; an encoding libxml2 has no handler for fails.
    pub fn start_document(
        &mut self,
        version: Option<&[u8]>,
        encoding: Option<&[u8]>,
        standalone: Option<&[u8]>,
    ) -> bool {
        let version = opt_cstring(version);
        let encoding = opt_cstring(encoding);
        let standalone = opt_cstring(standalone);
        self.call(|w| unsafe {
            ffi::xmlTextWriterStartDocument(
                w,
                opt_char_ptr(&version),
                opt_char_ptr(&encoding),
                opt_char_ptr(&standalone),
            )
        })
    }

    /// `xmlTextWriterEndDocument`.
    pub fn end_document(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndDocument(w) })
    }

    /// `xmlTextWriterStartComment`.
    pub fn start_comment(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterStartComment(w) })
    }

    /// `xmlTextWriterEndComment`.
    pub fn end_comment(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndComment(w) })
    }

    /// `xmlTextWriterWriteComment`.
    pub fn write_comment(&mut self, content: &[u8]) -> bool {
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteComment(w, xml_ptr(&content)) })
    }

    /// `xmlTextWriterStartElement`.
    pub fn start_element(&mut self, name: &[u8]) -> bool {
        let name = cstring(name);
        self.call(|w| unsafe { ffi::xmlTextWriterStartElement(w, xml_ptr(&name)) })
    }

    /// `xmlTextWriterStartElementNS`.
    pub fn start_element_ns(
        &mut self,
        prefix: Option<&[u8]>,
        name: &[u8],
        namespace_uri: Option<&[u8]>,
    ) -> bool {
        let prefix = opt_cstring(prefix);
        let name = cstring(name);
        let uri = opt_cstring(namespace_uri);
        self.call(|w| unsafe {
            ffi::xmlTextWriterStartElementNS(w, opt_xml_ptr(&prefix), xml_ptr(&name), opt_xml_ptr(&uri))
        })
    }

    /// `xmlTextWriterEndElement`.
    pub fn end_element(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndElement(w) })
    }

    /// `xmlTextWriterFullEndElement`.
    pub fn full_end_element(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterFullEndElement(w) })
    }

    /// `xmlwriter_write_element`: `None` content writes a start tag immediately closed
    /// (php-src calls `xmlTextWriterStartElement` + `xmlTextWriterEndElement`), otherwise
    /// `xmlTextWriterWriteElement`.
    pub fn write_element(&mut self, name: &[u8], content: Option<&[u8]>) -> bool {
        match content {
            None => self.start_element(name) && self.end_element(),
            Some(content) => {
                let name = cstring(name);
                let content = cstring(content);
                self.call(|w| unsafe { ffi::xmlTextWriterWriteElement(w, xml_ptr(&name), xml_ptr(&content)) })
            }
        }
    }

    /// `xmlwriter_write_element_ns`: like `write_element`, through the NS variants.
    pub fn write_element_ns(
        &mut self,
        prefix: Option<&[u8]>,
        name: &[u8],
        namespace_uri: Option<&[u8]>,
        content: Option<&[u8]>,
    ) -> bool {
        match content {
            None => self.start_element_ns(prefix, name, namespace_uri) && self.end_element(),
            Some(content) => {
                let prefix = opt_cstring(prefix);
                let name = cstring(name);
                let uri = opt_cstring(namespace_uri);
                let content = cstring(content);
                self.call(|w| unsafe {
                    ffi::xmlTextWriterWriteElementNS(
                        w,
                        opt_xml_ptr(&prefix),
                        xml_ptr(&name),
                        opt_xml_ptr(&uri),
                        xml_ptr(&content),
                    )
                })
            }
        }
    }

    /// `xmlTextWriterStartAttribute`.
    pub fn start_attribute(&mut self, name: &[u8]) -> bool {
        let name = cstring(name);
        self.call(|w| unsafe { ffi::xmlTextWriterStartAttribute(w, xml_ptr(&name)) })
    }

    /// `xmlTextWriterStartAttributeNS`.
    pub fn start_attribute_ns(
        &mut self,
        prefix: Option<&[u8]>,
        name: &[u8],
        namespace_uri: Option<&[u8]>,
    ) -> bool {
        let prefix = opt_cstring(prefix);
        let name = cstring(name);
        let uri = opt_cstring(namespace_uri);
        self.call(|w| unsafe {
            ffi::xmlTextWriterStartAttributeNS(w, opt_xml_ptr(&prefix), xml_ptr(&name), opt_xml_ptr(&uri))
        })
    }

    /// `xmlTextWriterEndAttribute`.
    pub fn end_attribute(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndAttribute(w) })
    }

    /// `xmlTextWriterWriteAttribute`.
    pub fn write_attribute(&mut self, name: &[u8], value: &[u8]) -> bool {
        let name = cstring(name);
        let value = cstring(value);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteAttribute(w, xml_ptr(&name), xml_ptr(&value)) })
    }

    /// `xmlTextWriterWriteAttributeNS`.
    pub fn write_attribute_ns(
        &mut self,
        prefix: Option<&[u8]>,
        name: &[u8],
        namespace_uri: Option<&[u8]>,
        value: &[u8],
    ) -> bool {
        let prefix = opt_cstring(prefix);
        let name = cstring(name);
        let uri = opt_cstring(namespace_uri);
        let value = cstring(value);
        self.call(|w| unsafe {
            ffi::xmlTextWriterWriteAttributeNS(
                w,
                opt_xml_ptr(&prefix),
                xml_ptr(&name),
                opt_xml_ptr(&uri),
                xml_ptr(&value),
            )
        })
    }

    /// `xmlTextWriterStartPI`.
    pub fn start_pi(&mut self, target: &[u8]) -> bool {
        let target = cstring(target);
        self.call(|w| unsafe { ffi::xmlTextWriterStartPI(w, xml_ptr(&target)) })
    }

    /// `xmlTextWriterEndPI`.
    pub fn end_pi(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndPI(w) })
    }

    /// `xmlTextWriterWritePI`.
    pub fn write_pi(&mut self, target: &[u8], content: &[u8]) -> bool {
        let target = cstring(target);
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWritePI(w, xml_ptr(&target), xml_ptr(&content)) })
    }

    /// `xmlTextWriterStartCDATA`.
    pub fn start_cdata(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterStartCDATA(w) })
    }

    /// `xmlTextWriterEndCDATA`.
    pub fn end_cdata(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndCDATA(w) })
    }

    /// `xmlTextWriterWriteCDATA`.
    pub fn write_cdata(&mut self, content: &[u8]) -> bool {
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteCDATA(w, xml_ptr(&content)) })
    }

    /// `xmlTextWriterWriteString` (PHP `text()`), escaping per the current state.
    pub fn write_string(&mut self, content: &[u8]) -> bool {
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteString(w, xml_ptr(&content)) })
    }

    /// `xmlTextWriterWriteRaw`.
    pub fn write_raw(&mut self, content: &[u8]) -> bool {
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteRaw(w, xml_ptr(&content)) })
    }

    /// `xmlTextWriterStartDTD`.
    pub fn start_dtd(
        &mut self,
        name: &[u8],
        public_id: Option<&[u8]>,
        system_id: Option<&[u8]>,
    ) -> bool {
        let name = cstring(name);
        let public_id = opt_cstring(public_id);
        let system_id = opt_cstring(system_id);
        self.call(|w| unsafe {
            ffi::xmlTextWriterStartDTD(w, xml_ptr(&name), opt_xml_ptr(&public_id), opt_xml_ptr(&system_id))
        })
    }

    /// `xmlTextWriterEndDTD`.
    pub fn end_dtd(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndDTD(w) })
    }

    /// `xmlTextWriterWriteDTD`.
    pub fn write_dtd(
        &mut self,
        name: &[u8],
        public_id: Option<&[u8]>,
        system_id: Option<&[u8]>,
        subset: Option<&[u8]>,
    ) -> bool {
        let name = cstring(name);
        let public_id = opt_cstring(public_id);
        let system_id = opt_cstring(system_id);
        let subset = opt_cstring(subset);
        self.call(|w| unsafe {
            ffi::xmlTextWriterWriteDTD(
                w,
                xml_ptr(&name),
                opt_xml_ptr(&public_id),
                opt_xml_ptr(&system_id),
                opt_xml_ptr(&subset),
            )
        })
    }

    /// `xmlTextWriterStartDTDElement`.
    pub fn start_dtd_element(&mut self, name: &[u8]) -> bool {
        let name = cstring(name);
        self.call(|w| unsafe { ffi::xmlTextWriterStartDTDElement(w, xml_ptr(&name)) })
    }

    /// `xmlTextWriterEndDTDElement`.
    pub fn end_dtd_element(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndDTDElement(w) })
    }

    /// `xmlTextWriterWriteDTDElement`.
    pub fn write_dtd_element(&mut self, name: &[u8], content: &[u8]) -> bool {
        let name = cstring(name);
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteDTDElement(w, xml_ptr(&name), xml_ptr(&content)) })
    }

    /// `xmlTextWriterStartDTDAttlist`.
    pub fn start_dtd_attlist(&mut self, name: &[u8]) -> bool {
        let name = cstring(name);
        self.call(|w| unsafe { ffi::xmlTextWriterStartDTDAttlist(w, xml_ptr(&name)) })
    }

    /// `xmlTextWriterEndDTDAttlist`.
    pub fn end_dtd_attlist(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndDTDAttlist(w) })
    }

    /// `xmlTextWriterWriteDTDAttlist`.
    pub fn write_dtd_attlist(&mut self, name: &[u8], content: &[u8]) -> bool {
        let name = cstring(name);
        let content = cstring(content);
        self.call(|w| unsafe { ffi::xmlTextWriterWriteDTDAttlist(w, xml_ptr(&name), xml_ptr(&content)) })
    }

    /// `xmlTextWriterStartDTDEntity`.
    pub fn start_dtd_entity(&mut self, name: &[u8], is_parameter: bool) -> bool {
        let name = cstring(name);
        self.call(|w| unsafe { ffi::xmlTextWriterStartDTDEntity(w, c_int::from(is_parameter), xml_ptr(&name)) })
    }

    /// `xmlTextWriterEndDTDEntity`.
    pub fn end_dtd_entity(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterEndDTDEntity(w) })
    }

    /// `xmlTextWriterWriteDTDInternalEntity`.
    pub fn write_dtd_internal_entity(
        &mut self,
        name: &[u8],
        content: &[u8],
        is_parameter: bool,
    ) -> bool {
        let name = cstring(name);
        let content = cstring(content);
        self.call(|w| unsafe {
            ffi::xmlTextWriterWriteDTDInternalEntity(w, c_int::from(is_parameter), xml_ptr(&name), xml_ptr(&content))
        })
    }

    /// `xmlTextWriterWriteDTDExternalEntity`.
    pub fn write_dtd_external_entity(
        &mut self,
        name: &[u8],
        public_id: Option<&[u8]>,
        system_id: Option<&[u8]>,
        notation_data: Option<&[u8]>,
        is_parameter: bool,
    ) -> bool {
        let name = cstring(name);
        let public_id = opt_cstring(public_id);
        let system_id = opt_cstring(system_id);
        let notation = opt_cstring(notation_data);
        self.call(|w| unsafe {
            ffi::xmlTextWriterWriteDTDExternalEntity(
                w,
                c_int::from(is_parameter),
                xml_ptr(&name),
                opt_xml_ptr(&public_id),
                opt_xml_ptr(&system_id),
                opt_xml_ptr(&notation),
            )
        })
    }

    /// `xmlTextWriterWriteDTDEntity`, the dispatcher PHP's `xmlwriter_write_dtd_entity`
    /// calls: an external entity when a public or system id is given (a notation alone
    /// is ignored and the value is written as an internal entity), a parameter entity
    /// may not carry a notation, and at least a value or an id must be present.
    pub fn write_dtd_entity(
        &mut self,
        name: &[u8],
        content: Option<&[u8]>,
        is_parameter: bool,
        public_id: Option<&[u8]>,
        system_id: Option<&[u8]>,
        notation_data: Option<&[u8]>,
    ) -> bool {
        let name = cstring(name);
        let content = opt_cstring(content);
        let public_id = opt_cstring(public_id);
        let system_id = opt_cstring(system_id);
        let notation = opt_cstring(notation_data);
        self.call(|w| unsafe {
            ffi::xmlTextWriterWriteDTDEntity(
                w,
                c_int::from(is_parameter),
                xml_ptr(&name),
                opt_xml_ptr(&public_id),
                opt_xml_ptr(&system_id),
                opt_xml_ptr(&notation),
                opt_xml_ptr(&content),
            )
        })
    }

    /// `xmlTextWriterFlush`: converts and delivers everything pending to the sink; `false`
    /// when the output channel is (or ends up) in error.
    pub fn flush(&mut self) -> bool {
        self.call(|w| unsafe { ffi::xmlTextWriterFlush(w) })
    }

    /// Bytes delivered to the sink so far and not yet taken (call `flush` first to include
    /// pending conversion work).
    pub fn output(&self) -> &[u8] {
        if self.sink.is_null() {
            return &[];
        }
        // SAFETY: the sink lives as long as the writer; no libxml2 call runs while this
        // shared borrow of `self` is alive.
        unsafe { &(*self.sink).buffer }
    }

    /// Flushes, then takes (and clears) the delivered output; a failed flush still hands
    /// back what was delivered before it, as PHP's `outputMemory()` does.
    pub fn take_output(&mut self) -> Vec<u8> {
        let _ = self.flush();
        if self.sink.is_null() {
            return Vec::new();
        }
        // SAFETY: as in `output`; the flush above has returned.
        unsafe { std::mem::take(&mut (*self.sink).buffer) }
    }
}

impl Default for Writer {
    /// Same as `Writer::new()`.
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Writer {
    /// `xmlFreeTextWriter`, which closes the output buffer and thereby frees the sink.
    fn drop(&mut self) {
        if !self.writer.is_null() {
            // SAFETY: the writer was created by `new` and is freed exactly once; the sink
            // is released by the buffer's close callback, never touched again here.
            unsafe { ffi::xmlFreeTextWriter(self.writer) };
            self.writer = ptr::null_mut();
            self.sink = ptr::null_mut();
        }
    }
}
