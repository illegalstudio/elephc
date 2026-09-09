//! Purpose:
//! A resumable SAX pull parser over libxml2 2.15.3's push parser, driven through the
//! Elephc-owned C shim and reproducing what php-src's `ext/xml` (`compat.c`, `xml.c`)
//! observes: one event per `next()`, `NeedMoreData` when the buffer holds no complete
//! item, libxml2 error codes, and libxml2 positions.
//!
//! Called from:
//! - `crate::abi` (the `elephc_xml_parser_*` entry points) and the crate unit tests.
//!
//! Key details:
//! - `feed()` hands the chunk to `xmlParseChunk`; the SAX callbacks (`callbacks`) run
//!   synchronously inside that call and queue `Event`s, each with a snapshot of the
//!   position AND the error code (`errNo`) libxml2 reports at that moment. `next()` pops
//!   one event and makes its snapshot the reported position and error code; once the
//!   queue is empty the LIVE parser values are reported. That is exactly what PHP code
//!   observes: a handler runs inside `xmlParseChunk` and sees the state at its event (a
//!   start handler reads code 0 even when the same chunk later fails with a mismatched
//!   tag), while reads after `xml_parse()` see the chunk's final state. `is_well_formed()`
//!   is never snapshotted: it is the chunk's final outcome, like `xml_parse()`'s return.
//! - Entities are never expanded as markup in content (php-src creates the context with
//!   `wellFormed = 0`, so libxml2 skips expansion): a reference surfaces as
//!   `Event::EntityRef` classified per compat.c's `get_entity`, and the caller decides what
//!   PHP's handler table does with it.
//! - Names are delivered raw (no case folding, no target-encoding conversion, no tag-start
//!   skipping): those are PHP-layer options applied by the ABI layer.
//! - `stop()` is compat.c's external-entity failure path: libxml2 is halted, queued events
//!   PHP would never have dispatched are dropped, the current event's position stays the
//!   reported one, and the stop code is the reported error code from then on (it wins
//!   over the delivered event's snapshot, which is what a handler reading the code right
//!   after the stop must see).

mod callbacks;
mod errors;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::ffi::{c_char, c_void};
use std::ptr;

use crate::ffi;

/// One attribute of a start tag, in document order, values already normalized
/// (entity/character references expanded, whitespace normalized per XML 1.0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    /// Attribute name; in namespace mode `uri<sep>local` for prefixed attributes,
    /// the local name otherwise.
    pub name: Vec<u8>,
    /// Normalized attribute value.
    pub value: Vec<u8>,
}

/// One namespace declaration carried by a start tag (namespace mode only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespaceDecl {
    /// Declared prefix, `None` for the default namespace (`xmlns="..."`).
    pub prefix: Option<Vec<u8>>,
    /// Namespace URI as written.
    pub uri: Vec<u8>,
}

/// What an entity reference in content resolved to (compat.c `get_entity`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityKind {
    /// One of the five predefined entities; `expansion` is the character it stands for.
    Predefined { expansion: Vec<u8> },
    /// An internal general entity declared in the internal subset; `replacement` is the
    /// literal replacement text with character references and nested entity references
    /// still present, exactly as libxml2 keeps it.
    Internal { replacement: Vec<u8> },
    /// An external parsed general entity.
    External {
        system_id: Vec<u8>,
        public_id: Option<Vec<u8>>,
    },
    /// An unparsed (NDATA) entity referenced in content. compat.c hands nothing to any
    /// PHP handler for it; the event exists so the ABI's entity kinds stay complete.
    Unparsed,
    /// A name no declaration matched.
    Undeclared,
}

/// One SAX event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A start tag (also emitted for empty-element tags, followed by `EndElement`).
    StartElement {
        /// Element name; in namespace mode `uri<sep>local` when the element is in a
        /// namespace, the local name otherwise.
        name: Vec<u8>,
        /// Attributes in document order; namespace mode omits the `xmlns` declarations.
        attributes: Vec<Attribute>,
        /// Namespace declarations on this tag (namespace mode only), in document order.
        namespaces: Vec<NamespaceDecl>,
        /// The tag's source text from `<` to `>`; for an empty-element tag the `/` is
        /// replaced by `>` and the original `>` dropped (`<e/>` becomes `<e>`), which is
        /// what php-src hands to a default handler.
        raw: Vec<u8>,
    },
    /// An end tag (also synthesized for empty-element tags).
    EndElement {
        /// Element name, qualified like `StartElement::name`.
        name: Vec<u8>,
        /// The end tag text a default handler receives: `</name>` with the ORIGINAL prefix
        /// (`</p:c>`), never the namespace-qualified form.
        raw: Vec<u8>,
    },
    /// Character data (text or a CDATA section), split exactly where libxml2 splits it.
    Characters(Vec<u8>),
    /// A processing instruction; `data` is `None` when the PI has no data at all
    /// (`<?pi?>`) and `Some("")` when it has only whitespace before `?>`.
    ProcessingInstruction {
        target: Vec<u8>,
        data: Option<Vec<u8>>,
    },
    /// A comment's text (without the delimiters), from the prolog, content, epilog or
    /// the internal subset.
    Comment(Vec<u8>),
    /// A general entity reference in content.
    EntityRef { name: Vec<u8>, kind: EntityKind },
    /// A `<!NOTATION>` declaration in the internal subset.
    NotationDecl {
        name: Vec<u8>,
        public_id: Option<Vec<u8>>,
        system_id: Option<Vec<u8>>,
    },
    /// An `<!ENTITY ... NDATA ...>` declaration in the internal subset.
    UnparsedEntityDecl {
        name: Vec<u8>,
        public_id: Option<Vec<u8>>,
        system_id: Vec<u8>,
        notation: Vec<u8>,
    },
}

/// Result of one `Parser::next()` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// One event was produced.
    Event(Event),
    /// The buffer holds no complete item and the final chunk has not been fed yet.
    NeedMoreData,
    /// The final chunk was fully consumed and the document is complete.
    Finished,
    /// An error code is set (see `Parser::error_code`) or the parser was stopped; every
    /// later call returns `Failed` again until a chunk parses cleanly.
    Failed,
}

/// libxml2 `XML_ERR_OK`.
pub const XML_ERR_OK: i32 = 0;
/// php-src's `XML_ERROR_NO_MEMORY` (expat numbering: `xml_error_string(1)` is "No memory"),
/// reported when libxml2 could not allocate the context. libxml2's own `XML_ERR_NO_MEMORY`
/// is 2, which PHP's shifted table would print as "Invalid document start"; php-src never
/// reaches this path because `emalloc` aborts on exhaustion.
pub const PHP_XML_ERROR_NO_MEMORY: i32 = 1;
/// libxml2 `XML_ERR_DOCUMENT_END`.
pub const XML_ERR_DOCUMENT_END: i32 = 5;
/// php-src's `XML_ERROR_EXTERNAL_ENTITY_HANDLING`, stored when an external-entity-ref
/// handler answers false.
pub const XML_ERROR_EXTERNAL_ENTITY_HANDLING: i32 = 21;

/// A position snapshot as libxml2 reports it (`XML_GetCurrentLineNumber` & co.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pos {
    /// 1-based line.
    pub(super) line: i64,
    /// 1-based column, counted in code points.
    pub(super) col: i64,
    /// Bytes consumed (`XML_GetCurrentByteIndex`).
    pub(super) byte: i64,
}

/// An event waiting to be delivered, with the position and error code libxml2 reported
/// during its callback.
#[derive(Debug)]
pub(super) struct Queued {
    pub(super) event: Event,
    pub(super) pos: Pos,
    /// libxml2's `errNo` at callback time: what `XML_GetErrorCode` answers a PHP handler
    /// for this event, before any later error in the same chunk.
    pub(super) error_code: i32,
}

/// The state the SAX callbacks mutate. Heap-allocated behind a raw pointer (never a Rust
/// reference that outlives a call) because libxml2 keeps its address as the SAX user
/// pointer for the life of the parser.
pub(super) struct Inner {
    /// The shim's parser handle; null when libxml2 could not allocate the context.
    pub(super) handle: *mut ffi::ShimParser,
    /// Namespace separator: `Some` enables namespace mode (`xml_parser_create_ns`).
    pub(super) ns_sep: Option<Vec<u8>>,
    /// Events queued by the callbacks during `feed()`, oldest first.
    pub(super) events: VecDeque<Queued>,
    /// Position snapshot of the event being delivered, `None` between events.
    pub(super) current_pos: Option<Pos>,
    /// Position snapshot frozen by `stop()` while an event was being delivered.
    pub(super) frozen_pos: Option<Pos>,
    /// Error code snapshot of the event being delivered, `None` between events.
    pub(super) current_error: Option<i32>,
    /// Error code set by `stop()` (or a halted callback), reported from then on.
    pub(super) frozen_error: Option<i32>,
    /// `XML_OPTION_PARSE_HUGE`, applied before every chunk like `xml_parse_helper`.
    pub(super) huge: bool,
    /// Whether the most recent chunk was fed with `is_final`.
    pub(super) final_fed: bool,
    /// `XML_Parse`'s result for the most recent chunk (true before the first one).
    pub(super) last_parse_ok: bool,
    /// Whether `stop()` (or a panicking callback) halted the parser.
    pub(super) stopped: bool,
    /// Test hook: makes the next queued event panic inside its SAX callback.
    #[cfg(all(test, elephc_xml_native))]
    pub(super) panic_in_callbacks: bool,
}

/// The push parser.
pub struct Parser {
    /// Owned via `Box::into_raw`; freed in `Drop`.
    inner: *mut Inner,
}

// The parser is used from whichever thread holds the ABI registry lock; libxml2 contexts
// are not tied to the creating thread, and `Inner` is only ever reached through `inner`.
unsafe impl Send for Parser {}

impl Parser {
    /// Creates a parser; `namespace_separator` enables namespace mode and gives the
    /// bytes joining URI and local name (`xml_parser_create_ns`), `None` is plain mode.
    /// Only the separator's FIRST byte is used, exactly like compat.c's
    /// `qualify_namespace`; an empty separator joins URI and name directly.
    pub fn new(namespace_separator: Option<Vec<u8>>) -> Self {
        ffi::init_library();
        let use_namespaces = i32::from(namespace_separator.is_some());
        let inner = Box::into_raw(Box::new(Inner {
            handle: ptr::null_mut(),
            ns_sep: namespace_separator,
            events: VecDeque::new(),
            current_pos: None,
            frozen_pos: None,
            current_error: None,
            frozen_error: None,
            huge: false,
            final_fed: false,
            last_parse_ok: true,
            stopped: false,
            #[cfg(all(test, elephc_xml_native))]
            panic_in_callbacks: false,
        }));
        // SAFETY: `inner` is a live heap allocation; the shim stores the pointer and hands
        // it back unchanged to every callback, which is the only other place it is read.
        unsafe {
            (*inner).handle = ffi::elephc_libxml2_v1_parser_create(
                &callbacks::SAX,
                inner as *mut c_void,
                use_namespaces,
            );
        }
        Parser { inner }
    }

    /// Lifts libxml2's resource limits (`XML_PARSE_HUGE`); applied at the next `feed`.
    pub fn set_parse_huge(&mut self, huge: bool) {
        self.state().huge = huge;
    }

    /// Appends one chunk (`xml_parse` with `is_final` as its third argument): applies the
    /// huge option, runs `xmlParseChunk`, and records `XML_Parse`'s status. Every SAX
    /// event the chunk produces is queued before this returns.
    pub fn feed(&mut self, chunk: &[u8], is_final: bool) {
        let inner = self.inner;
        // SAFETY: `inner` is live for the parser's lifetime; no Rust reference into it is
        // held across the FFI call, during which the callbacks reborrow it.
        let handle = unsafe { (*inner).handle };
        if handle.is_null() {
            let state = self.state();
            state.last_parse_ok = false;
            state.final_fed = is_final;
            return;
        }
        let huge = unsafe { (*inner).huge };
        let mut ok = 1;
        unsafe {
            ffi::elephc_libxml2_v1_set_huge(handle, i32::from(huge));
            if chunk.is_empty() {
                ok = ffi::elephc_libxml2_v1_parse_chunk(handle, ptr::null(), 0, i32::from(is_final));
            } else {
                // `XML_Parse` takes an `int` length; PHP data larger than that is fed in
                // pieces, terminating only with the last one.
                let pieces: Vec<&[u8]> = chunk.chunks(i32::MAX as usize).collect();
                let last = pieces.len() - 1;
                for (index, piece) in pieces.iter().enumerate() {
                    let terminate = i32::from(is_final && index == last);
                    ok = ffi::elephc_libxml2_v1_parse_chunk(
                        handle,
                        piece.as_ptr() as *const c_char,
                        piece.len() as i32,
                        terminate,
                    );
                }
            }
        }
        let state = self.state();
        // A callback that panicked halted the parser from inside `xmlParseChunk`; the
        // chunk is then reported like one fed after `stop()`.
        state.last_parse_ok = ok == 1 && !state.stopped;
        state.final_fed = is_final;
    }

    /// Test hook: every later SAX callback panics instead of queueing its event.
    #[cfg(all(test, elephc_xml_native))]
    pub(super) fn inject_callback_panic(&mut self) {
        self.state().panic_in_callbacks = true;
    }

    /// Delivers the next queued event, or the round's outcome once the queue is empty:
    /// `NeedMoreData` while the final chunk is pending, `Finished` when it was fed and no
    /// error is set, `Failed` when an error code is set or the parser was stopped.
    /// (Named after `xml_parse`'s pull shape on purpose; it is not an `Iterator`, since
    /// the non-event outcomes are part of the answer.)
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Step {
        let state = self.state();
        if let Some(queued) = state.events.pop_front() {
            state.current_pos = Some(queued.pos);
            state.current_error = Some(queued.error_code);
            return Step::Event(queued.event);
        }
        state.current_pos = None;
        state.current_error = None;
        if state.stopped || state.handle.is_null() || !state.last_parse_ok {
            return Step::Failed;
        }
        if state.final_fed {
            Step::Finished
        } else {
            Step::NeedMoreData
        }
    }

    /// The libxml2 error code to report (`xml_get_error_code`), 0 when none: the code
    /// `stop()` set, else the delivered event's snapshot (what a handler sees), else
    /// libxml2's live `errNo` (what a read after `xml_parse()` sees).
    pub fn error_code(&self) -> i32 {
        let state = self.state_ref();
        if state.handle.is_null() {
            return PHP_XML_ERROR_NO_MEMORY;
        }
        if let Some(code) = state.frozen_error.or(state.current_error) {
            return code;
        }
        // SAFETY: a non-null handle is valid until `Drop`.
        unsafe { ffi::elephc_libxml2_v1_error_code(state.handle) }
    }

    /// `XML_Parse`'s status for the most recent chunk: true while no error above warning
    /// level has been recorded (libxml2 keeps `errNo` sticky, so one error fails every
    /// later chunk too), false after `stop()` set an error code. This is what `xml_parse`
    /// returns.
    pub fn is_well_formed(&self) -> bool {
        self.state_ref().last_parse_ok
    }

    /// Current line (1-based): the delivered event's snapshot, else libxml2's live line.
    pub fn line(&self) -> i64 {
        self.reported_pos().line
    }

    /// Current column (1-based, code points), snapshot or live like `line`.
    pub fn column(&self) -> i64 {
        self.reported_pos().col
    }

    /// Bytes consumed so far (`XML_GetCurrentByteIndex`), snapshot or live like `line`.
    pub fn byte_index(&self) -> i64 {
        self.reported_pos().byte
    }

    /// Stops the parser (compat.c's `external_entity_ref_handler` failure path:
    /// `xmlStopParser`, then `errNo = code`); `error_code` 0 keeps the code currently
    /// reported (the delivered event's snapshot inside a handler), anything else becomes
    /// the reported code. Either way that code is reported from then on, over the
    /// snapshot of the event still being delivered and over libxml2's live `errNo`.
    /// Events still queued are dropped (PHP would never have dispatched them), the
    /// current event's position stays the reported one, and every later `next()`
    /// answers `Failed`.
    pub fn stop(&mut self, error_code: i32) {
        let effective = if error_code != XML_ERR_OK {
            error_code
        } else {
            self.error_code()
        };
        let state = self.state();
        if !state.handle.is_null() {
            // SAFETY: a non-null handle is valid until `Drop`.
            unsafe { ffi::elephc_libxml2_v1_stop(state.handle, effective) };
        }
        state.events.clear();
        state.stopped = true;
        if state.frozen_pos.is_none() {
            state.frozen_pos = state.current_pos;
        }
        state.frozen_error = Some(effective);
        if effective != XML_ERR_OK {
            state.last_parse_ok = false;
        }
    }

    /// Mutable access to the callback state; never held across an FFI call.
    fn state(&mut self) -> &mut Inner {
        // SAFETY: `inner` is a live `Box::into_raw` allocation until `Drop`, and no
        // parse is running while a `&mut Parser` method executes.
        unsafe { &mut *self.inner }
    }

    /// Shared access to the callback state.
    fn state_ref(&self) -> &Inner {
        // SAFETY: as in `state`.
        unsafe { &*self.inner }
    }

    /// The position to report: the stop-frozen snapshot, else the delivered event's, else
    /// libxml2's live cursor.
    fn reported_pos(&self) -> Pos {
        let state = self.state_ref();
        if let Some(pos) = state.frozen_pos.or(state.current_pos) {
            return pos;
        }
        if state.handle.is_null() {
            return Pos { line: 0, col: 0, byte: 0 };
        }
        callbacks::live_pos(state.handle)
    }
}

impl Drop for Parser {
    /// Frees the libxml2 context (`XML_ParserFree`) and the callback state.
    fn drop(&mut self) {
        // SAFETY: `inner` came from `Box::into_raw` in `new` and is freed exactly once;
        // the shim never touches the user pointer after `parser_free`.
        unsafe {
            let inner = Box::from_raw(self.inner);
            if !inner.handle.is_null() {
                ffi::elephc_libxml2_v1_parser_free(inner.handle);
            }
        }
    }
}

/// php-src's `XML_ErrorString` table: the message for a libxml2 error code, `"Unknown"`
/// outside the table.
pub fn error_string(code: i32) -> &'static str {
    errors::error_string(code)
}
