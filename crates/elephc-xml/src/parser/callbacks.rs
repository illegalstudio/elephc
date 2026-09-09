//! Purpose:
//! The SAX callbacks libxml2 invokes through the shim while `Parser::feed` runs, each
//! translated into one queued `Event` with a snapshot of the position and of the error
//! code (`errNo`) at callback time. They reproduce php-src's
//! `ext/xml/compat.c` handler set: `qualify_namespace`, `start_element_emit_default`'s raw
//! tag capture, the end-tag / PI / comment spellings, and `get_entity`'s routing rules.
//!
//! Called from:
//! - libxml2 (via the shim's `xmlSAXHandler`) during `xmlParseChunk`; the table `SAX` is
//!   handed to `elephc_libxml2_v1_parser_create` by `Parser::new`.
//!
//! Key details:
//! - The `user` pointer is the parser's `Inner`; callbacks reborrow it for their duration
//!   only, and `Parser::feed` holds no Rust reference into it across the FFI call.
//! - Nothing may unwind across the `extern "C"` boundary (that aborts the process), so
//!   every body runs under `guarded` / `guarded_or`: the bodies contain no `unwrap` or
//!   indexing that can panic, and should one panic anyway (an allocation-size overflow
//!   while copying libxml2's bytes), the parser is halted with `PHP_XML_ERROR_NO_MEMORY`
//!   and the round reports `Failed`, the way `Parser::stop` does.
//! - `get_entity` is the one callback libxml2 runs even after `disableSAX` was set, and
//!   it must return the entity pointer for libxml2 to expand it in attribute values.

use std::ffi::{c_char, c_int, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use super::{Attribute, EntityKind, Event, Inner, NamespaceDecl, Pos, Queued, PHP_XML_ERROR_NO_MEMORY};
use crate::ffi::{self, XmlChar, XmlEntity};

/// The callback table handed to the shim (`php_xml_compat_handlers`' subset).
pub(super) static SAX: ffi::Sax = ffi::Sax {
    start_element: Some(start_element),
    end_element: Some(end_element),
    start_element_ns: Some(start_element_ns),
    end_element_ns: Some(end_element_ns),
    characters: Some(characters),
    processing_instruction: Some(processing_instruction),
    comment: Some(comment),
    get_entity: Some(get_entity),
    notation_decl: Some(notation_decl),
    unparsed_entity_decl: Some(unparsed_entity_decl),
};

/// libxml2's live cursor position for `handle` (`XML_GetCurrent*`).
pub(super) fn live_pos(handle: *mut ffi::ShimParser) -> Pos {
    // SAFETY: the caller passes a handle that is valid until the parser is dropped.
    unsafe {
        Pos {
            line: i64::from(ffi::elephc_libxml2_v1_line(handle)),
            col: i64::from(ffi::elephc_libxml2_v1_column(handle)),
            byte: ffi::elephc_libxml2_v1_byte_index(handle),
        }
    }
}

/// Reborrows the parser state from the SAX user pointer.
///
/// # Safety
/// `user` must be the `Inner` pointer given to `elephc_libxml2_v1_parser_create`, with no
/// other live reference to it (true while `Parser::feed` is inside `xmlParseChunk`).
unsafe fn state<'a>(user: *mut c_void) -> &'a mut Inner {
    &mut *(user as *mut Inner)
}

/// Runs a callback body over the parser state under `catch_unwind`, answering `fallback`
/// and halting the parser (`Inner::halt_after_panic`) when the body panics.
///
/// # Safety
/// As for `state`.
unsafe fn guarded_or<T>(user: *mut c_void, fallback: T, body: impl FnOnce(&mut Inner) -> T) -> T {
    match catch_unwind(AssertUnwindSafe(|| body(state(user)))) {
        Ok(value) => value,
        Err(_) => {
            state(user).halt_after_panic();
            fallback
        }
    }
}

/// `guarded_or` for the callbacks that answer nothing.
///
/// # Safety
/// As for `state`.
unsafe fn guarded(user: *mut c_void, body: impl FnOnce(&mut Inner)) {
    guarded_or(user, (), body);
}

impl Inner {
    /// Queues `event` with the position and the error code libxml2 reports right now: a
    /// PHP handler runs at this very point of the parse, so a later error in the same
    /// chunk must stay invisible to it.
    fn push(&mut self, event: Event) {
        #[cfg(all(test, elephc_xml_native))]
        if self.panic_in_callbacks {
            panic!("injected SAX callback panic");
        }
        let pos = live_pos(self.handle);
        // SAFETY: the handle is valid during a callback.
        let error_code = unsafe { ffi::elephc_libxml2_v1_error_code(self.handle) };
        self.events.push_back(Queued { event, pos, error_code });
    }

    /// A callback body panicked: halt libxml2 with `PHP_XML_ERROR_NO_MEMORY` (the only
    /// panic the bodies can reach is an allocation-size overflow), drop the queued
    /// events and make every later `next()` answer `Failed`, exactly like `Parser::stop`
    /// (`feed` then reports the chunk as not well-formed and that code is reported from
    /// then on).
    fn halt_after_panic(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: the handle is valid during a callback.
            unsafe { ffi::elephc_libxml2_v1_stop(self.handle, PHP_XML_ERROR_NO_MEMORY) };
        }
        self.events.clear();
        self.stopped = true;
        if self.frozen_pos.is_none() {
            self.frozen_pos = self.current_pos;
        }
        self.frozen_error = Some(PHP_XML_ERROR_NO_MEMORY);
    }

    /// compat.c `qualify_namespace`: `uri` + the separator's first byte + `name` when the
    /// name is in a namespace, `name` alone otherwise. `xmlStrncat(.., sep, 1)` copies one
    /// byte, so a multi-byte separator is truncated and an empty one joins directly.
    fn qualify(&self, name: &[u8], uri: Option<&[u8]>) -> Vec<u8> {
        match uri {
            Some(uri) => {
                let mut qualified = uri.to_vec();
                if let Some(first) = self.ns_sep.as_ref().and_then(|sep| sep.first()) {
                    qualified.push(*first);
                }
                qualified.extend_from_slice(name);
                qualified
            }
            None => name.to_vec(),
        }
    }

    /// `start_element_emit_default`'s tag text: from the `<` before the cursor to the
    /// cursor inclusive, with an empty-element tag's `/` rewritten to `>`.
    fn current_tag(&self) -> Vec<u8> {
        let mut start: *const c_char = ptr::null();
        // SAFETY: the handle is valid during a callback; the shim writes a pointer into
        // libxml2's input buffer, which is stable until the callback returns.
        let len = unsafe { ffi::elephc_libxml2_v1_current_tag(self.handle, &mut start) };
        if start.is_null() || len <= 0 {
            return Vec::new();
        }
        let mut raw = unsafe { std::slice::from_raw_parts(start as *const u8, len as usize) }.to_vec();
        if let Some(last) = raw.last_mut() {
            if *last == b'/' {
                *last = b'>';
            }
        }
        raw
    }
}

/// SAX1 `startElement` (plain-mode parser): name plus NULL-terminated name/value pairs.
unsafe extern "C" fn start_element(user: *mut c_void, name: *const XmlChar, attributes: *const *const XmlChar) {
    guarded(user, |inner| {
        let mut attrs = Vec::new();
        if !attributes.is_null() {
            let mut cursor = attributes;
            while !(*cursor).is_null() {
                let name = ffi::bytes(*cursor);
                let value = ffi::bytes(*cursor.add(1));
                attrs.push(Attribute { name, value });
                cursor = cursor.add(2);
            }
        }
        let raw = inner.current_tag();
        inner.push(Event::StartElement {
            name: ffi::bytes(name),
            attributes: attrs,
            namespaces: Vec::new(),
            raw,
        });
    });
}

/// SAX1 `endElement`: the default-handler text is `</name>`.
unsafe extern "C" fn end_element(user: *mut c_void, name: *const XmlChar) {
    guarded(user, |inner| {
        let name = ffi::bytes(name);
        let mut raw = b"</".to_vec();
        raw.extend_from_slice(&name);
        raw.push(b'>');
        inner.push(Event::EndElement { name, raw });
    });
}

/// SAX2 `startElementNs` (namespace parser): `start_element_handler_ns`'s qualification of
/// the element and of prefixed attributes, attribute values from their start/end ranges,
/// namespace declarations as prefix/URI pairs.
unsafe extern "C" fn start_element_ns(
    user: *mut c_void,
    localname: *const XmlChar,
    prefix: *const XmlChar,
    uri: *const XmlChar,
    nb_namespaces: c_int,
    namespaces: *const *const XmlChar,
    nb_attributes: c_int,
    _nb_defaulted: c_int,
    attributes: *const *const XmlChar,
) {
    let _ = prefix;
    guarded(user, |inner| {
        let mut decls = Vec::new();
        if !namespaces.is_null() && nb_namespaces > 0 {
            for index in 0..nb_namespaces as usize {
                let prefix = ffi::opt_bytes(*namespaces.add(2 * index));
                let uri = ffi::bytes(*namespaces.add(2 * index + 1));
                decls.push(NamespaceDecl { prefix, uri });
            }
        }
        let mut attrs = Vec::new();
        if !attributes.is_null() && nb_attributes > 0 {
            for index in 0..nb_attributes as usize {
                let base = attributes.add(5 * index);
                let local = ffi::bytes(*base);
                let name = if (*base.add(1)).is_null() {
                    local
                } else {
                    let attr_uri = ffi::opt_bytes(*base.add(2));
                    inner.qualify(&local, attr_uri.as_deref())
                };
                let start = *base.add(3);
                let end = *base.add(4);
                let value = if start.is_null() || end.is_null() || end < start {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(start, end as usize - start as usize).to_vec()
                };
                attrs.push(Attribute { name, value });
            }
        }
        let element_uri = ffi::opt_bytes(uri);
        let name = inner.qualify(&ffi::bytes(localname), element_uri.as_deref());
        let raw = inner.current_tag();
        inner.push(Event::StartElement {
            name,
            attributes: attrs,
            namespaces: decls,
            raw,
        });
    });
}

/// SAX2 `endElementNs`: qualified name, default-handler text with the original prefix.
unsafe extern "C" fn end_element_ns(user: *mut c_void, localname: *const XmlChar, prefix: *const XmlChar, uri: *const XmlChar) {
    guarded(user, |inner| {
        let local = ffi::bytes(localname);
        let element_uri = ffi::opt_bytes(uri);
        let name = inner.qualify(&local, element_uri.as_deref());
        let mut raw = b"</".to_vec();
        if let Some(prefix) = ffi::opt_bytes(prefix) {
            raw.extend_from_slice(&prefix);
            raw.push(b':');
        }
        raw.extend_from_slice(&local);
        raw.push(b'>');
        inner.push(Event::EndElement { name, raw });
    });
}

/// `characters` and `cdataBlock`: `len` bytes of text.
unsafe extern "C" fn characters(user: *mut c_void, data: *const XmlChar, len: c_int) {
    guarded(user, |inner| {
        let text = if data.is_null() || len <= 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(data, len as usize).to_vec()
        };
        inner.push(Event::Characters(text));
    });
}

/// `processingInstruction`: `data` is NULL for `<?pi?>`.
unsafe extern "C" fn processing_instruction(user: *mut c_void, target: *const XmlChar, data: *const XmlChar) {
    guarded(user, |inner| {
        inner.push(Event::ProcessingInstruction {
            target: ffi::bytes(target),
            data: ffi::opt_bytes(data),
        });
    });
}

/// `comment`: the text between the delimiters.
unsafe extern "C" fn comment(user: *mut c_void, value: *const XmlChar) {
    guarded(user, |inner| inner.push(Event::Comment(ffi::bytes(value))));
}

/// compat.c `get_entity`: outside the DTD, look the name up (predefined first, then the
/// document's declarations); in content, or when nothing matched, queue the reference
/// classified the way compat.c routes it to PHP's handlers; always hand libxml2 the
/// entity pointer it found (NULL inside the subset), so attribute values still expand.
unsafe extern "C" fn get_entity(user: *mut c_void, name: *const XmlChar) -> *mut XmlEntity {
    guarded_or(user, ptr::null_mut(), |inner| {
        let handle = inner.handle;
        if ffi::elephc_libxml2_v1_in_subset(handle) != 0 {
            return ptr::null_mut();
        }
        let entity = ffi::elephc_libxml2_v1_lookup_entity(handle, name);
        if entity.is_null() || ffi::elephc_libxml2_v1_in_content(handle) != 0 {
            let etype = ffi::elephc_libxml2_v1_entity_type(entity);
            let kind = if entity.is_null() {
                Some(EntityKind::Undeclared)
            } else {
                match etype {
                    ffi::XML_INTERNAL_PREDEFINED_ENTITY => Some(EntityKind::Predefined {
                        expansion: ffi::bytes(ffi::elephc_libxml2_v1_entity_content(entity)),
                    }),
                    ffi::XML_INTERNAL_GENERAL_ENTITY | ffi::XML_INTERNAL_PARAMETER_ENTITY => {
                        Some(EntityKind::Internal {
                            replacement: ffi::bytes(ffi::elephc_libxml2_v1_entity_content(entity)),
                        })
                    }
                    ffi::XML_EXTERNAL_GENERAL_PARSED_ENTITY => Some(EntityKind::External {
                        system_id: ffi::bytes(ffi::elephc_libxml2_v1_entity_system_id(entity)),
                        public_id: ffi::opt_bytes(ffi::elephc_libxml2_v1_entity_external_id(entity)),
                    }),
                    ffi::XML_EXTERNAL_GENERAL_UNPARSED_ENTITY => Some(EntityKind::Unparsed),
                    _ => None,
                }
            };
            if let Some(kind) = kind {
                let name = if matches!(kind, EntityKind::External { .. }) {
                    ffi::bytes(ffi::elephc_libxml2_v1_entity_name(entity))
                } else {
                    ffi::bytes(name)
                };
                inner.push(Event::EntityRef { name, kind });
            }
        }
        entity
    })
}

/// `notationDecl`: ids may be NULL.
unsafe extern "C" fn notation_decl(user: *mut c_void, name: *const XmlChar, public_id: *const XmlChar, system_id: *const XmlChar) {
    guarded(user, |inner| {
        inner.push(Event::NotationDecl {
            name: ffi::bytes(name),
            public_id: ffi::opt_bytes(public_id),
            system_id: ffi::opt_bytes(system_id),
        });
    });
}

/// `unparsedEntityDecl`: an `NDATA` entity always carries a system id and a notation.
unsafe extern "C" fn unparsed_entity_decl(
    user: *mut c_void,
    name: *const XmlChar,
    public_id: *const XmlChar,
    system_id: *const XmlChar,
    notation_name: *const XmlChar,
) {
    guarded(user, |inner| {
        inner.push(Event::UnparsedEntityDecl {
            name: ffi::bytes(name),
            public_id: ffi::opt_bytes(public_id),
            system_id: ffi::bytes(system_id),
            notation: ffi::bytes(notation_name),
        });
    });
}
