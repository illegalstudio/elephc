//! Purpose:
//! The stable, panic-free C ABI of the `elephc_xml` bridge: id-keyed parser and writer
//! registries behind `elephc_xml_parser_*` / `elephc_xml_writer_*` entry points, plus the
//! PHP-layer string policies (case folding, target encoding) applied on the way out.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_xml"` block the compiler's xml
//!   prelude declares (`elephc::xml_prelude`); every function here has one declaration there.
//!
//! Key details:
//! - Handles are positive integers minted by process-global registries; `0` is never a valid
//!   handle and every accessor on an unknown handle fails closed (0 / empty string).
//! - Strings arrive as NUL-terminated C strings (the compiler's `string` extern parameters
//!   are `char*` copies of the PHP bytes), so writer inputs stop at the first NUL exactly
//!   like php-src's own C-string arguments. Parser input carries an explicit length because
//!   PHP data may contain NUL bytes and libxml2 reports them as an invalid character.
//! - Returned strings live in `thread_local!` cells, one per entry point, valid until the
//!   next call of the same entry point on the same thread; the compiler copies the bytes
//!   immediately (`__rt_cstr_to_str`), which is the FFI buffer-hygiene contract.
//! - Nothing here unwinds: every entry runs its body under `catch_unwind` and answers the
//!   failure value instead, and the thread-local stash never panics (`try_with`).
//! - Writer output can carry NUL bytes (UTF-16 / UCS-4 documents), which a C string cannot:
//!   `elephc_xml_writer_output_has_nul` tells the prelude to read such output through
//!   `elephc_xml_writer_output_hex` instead of the plain string entry points.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};

use crate::parser::{self, EntityKind, Event, Parser, Step};
use crate::writer::Writer;

/// `next()` answer: the buffer holds no complete item yet (`Step::NeedMoreData`).
pub const EVENT_NEED_MORE: i64 = 0;
/// `next()` answer: the document is complete (`Step::Finished`).
pub const EVENT_FINISHED: i64 = -1;
/// `next()` answer: the parser is stopped or failed (`Step::Failed`).
pub const EVENT_FAILED: i64 = -2;
/// `next()` answer: a start tag.
pub const EVENT_START_ELEMENT: i64 = 1;
/// `next()` answer: an end tag.
pub const EVENT_END_ELEMENT: i64 = 2;
/// `next()` answer: character data.
pub const EVENT_CHARACTERS: i64 = 3;
/// `next()` answer: a processing instruction.
pub const EVENT_PI: i64 = 4;
/// `next()` answer: a comment.
pub const EVENT_COMMENT: i64 = 5;
/// `next()` answer: a general entity reference in content.
pub const EVENT_ENTITY_REF: i64 = 6;
/// `next()` answer: a notation declaration.
pub const EVENT_NOTATION_DECL: i64 = 7;
/// `next()` answer: an unparsed entity declaration.
pub const EVENT_UNPARSED_ENTITY_DECL: i64 = 8;

/// `XML_OPTION_CASE_FOLDING`.
pub const OPTION_CASE_FOLDING: i64 = 1;
/// `XML_OPTION_TARGET_ENCODING` (set through `elephc_xml_parser_set_target_encoding`).
pub const OPTION_TARGET_ENCODING: i64 = 2;
/// `XML_OPTION_SKIP_TAGSTART` (stored for `xml_parser_get_option`, applied by the prelude).
pub const OPTION_SKIP_TAGSTART: i64 = 3;
/// `XML_OPTION_SKIP_WHITE` (stored for `xml_parser_get_option`, applied by the prelude).
pub const OPTION_SKIP_WHITE: i64 = 4;
/// `XML_OPTION_PARSE_HUGE`.
pub const OPTION_PARSE_HUGE: i64 = 5;

/// Event string field: element name, PI target, text, entity/notation name.
pub const FIELD_NAME: i64 = 0;
/// Event string field: raw tag text, PI data, entity replacement / system id.
pub const FIELD_SECOND: i64 = 1;
/// Event string field: public id.
pub const FIELD_THIRD: i64 = 2;
/// Event string field: notation name of an unparsed entity declaration.
pub const FIELD_FOURTH: i64 = 3;

/// Event int field: attribute count of a start tag.
pub const INT_ATTR_COUNT: i64 = 0;
/// Event int field: namespace-declaration count of a start tag.
pub const INT_NS_COUNT: i64 = 1;
/// Event int field: entity kind (`ENTITY_*`).
pub const INT_ENTITY_KIND: i64 = 2;
/// Event int field: presence flags (`FLAG_*`).
pub const INT_FLAGS: i64 = 3;

/// Entity kind: one of the five predefined entities.
pub const ENTITY_PREDEFINED: i64 = 0;
/// Entity kind: internal general entity from the internal subset.
pub const ENTITY_INTERNAL: i64 = 1;
/// Entity kind: external parsed general entity.
pub const ENTITY_EXTERNAL: i64 = 2;
/// Entity kind: unparsed (NDATA) entity.
pub const ENTITY_UNPARSED: i64 = 3;
/// Entity kind: no declaration matched.
pub const ENTITY_UNDECLARED: i64 = 4;

/// Flag bit: the PI carries data (`<?pi data?>` rather than `<?pi?>`).
pub const FLAG_HAS_PI_DATA: i64 = 1;
/// Flag bit: a public id is present.
pub const FLAG_HAS_PUBLIC_ID: i64 = 2;
/// Flag bit: a system id is present.
pub const FLAG_HAS_SYSTEM_ID: i64 = 4;

/// PHP's `xml_parser_create()` target-encoding names, in `xml_encodings[]` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetEncoding {
    /// `ISO-8859-1`: code points above 0xFF become `?`.
    Iso88591,
    /// `US-ASCII`: code points above 0x7F become `?`.
    UsAscii,
    /// `UTF-8`: bytes pass through.
    Utf8,
}

impl TargetEncoding {
    /// Resolves a PHP encoding name case-insensitively (`xml_get_encoding`).
    fn parse(name: &[u8]) -> Option<Self> {
        if name.eq_ignore_ascii_case(b"ISO-8859-1") {
            Some(Self::Iso88591)
        } else if name.eq_ignore_ascii_case(b"US-ASCII") {
            Some(Self::UsAscii)
        } else if name.eq_ignore_ascii_case(b"UTF-8") {
            Some(Self::Utf8)
        } else {
            None
        }
    }

    /// The canonical spelling `xml_parser_get_option()` reports.
    fn name(self) -> &'static str {
        match self {
            Self::Iso88591 => "ISO-8859-1",
            Self::UsAscii => "US-ASCII",
            Self::Utf8 => "UTF-8",
        }
    }

    /// `xml_utf8_decode`: converts parser output (UTF-8) into this encoding, replacing
    /// unrepresentable code points with `?`.
    fn decode(self, utf8: &[u8]) -> Vec<u8> {
        match self {
            Self::Utf8 => utf8.to_vec(),
            Self::Iso88591 | Self::UsAscii => {
                let limit = if self == Self::Iso88591 { 0xFF } else { 0x7F };
                let mut out = Vec::with_capacity(utf8.len());
                for ch in String::from_utf8_lossy(utf8).chars() {
                    let code = ch as u32;
                    out.push(if code > limit { b'?' } else { code as u8 });
                }
                out
            }
        }
    }
}

/// One registered parser with its PHP-layer options.
struct ParserEntry {
    /// The engine.
    parser: Parser,
    /// The most recent event, kept for the field accessors.
    event: Option<Event>,
    /// `XML_OPTION_CASE_FOLDING`.
    case_folding: bool,
    /// `XML_OPTION_SKIP_TAGSTART`, stored for `xml_parser_get_option` only.
    skip_tagstart: i64,
    /// `XML_OPTION_SKIP_WHITE`, stored for `xml_parser_get_option` only.
    skip_white: bool,
    /// `XML_OPTION_PARSE_HUGE`.
    parse_huge: bool,
    /// Target encoding of every string handed to PHP.
    target_encoding: TargetEncoding,
}

impl ParserEntry {
    /// Applies the target encoding to one output string.
    fn encode(&self, bytes: &[u8]) -> Vec<u8> {
        self.target_encoding.decode(bytes)
    }

    /// Applies the target encoding and case folding to one element or attribute name
    /// (`xml_decode_tag`: decode first, then ASCII uppercase).
    fn encode_tag(&self, bytes: &[u8]) -> Vec<u8> {
        let mut decoded = self.encode(bytes);
        if self.case_folding {
            decoded.make_ascii_uppercase();
        }
        decoded
    }
}

/// Process-global parser registry keyed by handle.
static PARSERS: OnceLock<Mutex<HashMap<i64, ParserEntry>>> = OnceLock::new();
/// Process-global writer registry keyed by handle.
static WRITERS: OnceLock<Mutex<HashMap<i64, Writer>>> = OnceLock::new();
/// Next handle for either registry; handles are never reused within a process.
static NEXT_HANDLE: Mutex<i64> = Mutex::new(1);

/// Returns the parser registry, creating it on first use.
fn parsers() -> &'static Mutex<HashMap<i64, ParserEntry>> {
    PARSERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the writer registry, creating it on first use.
fn writers() -> &'static Mutex<HashMap<i64, Writer>> {
    WRITERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Mints a fresh positive handle.
fn next_handle() -> i64 {
    let mut next = NEXT_HANDLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let handle = *next;
    *next += 1;
    handle
}

/// Runs `body` on the parser registry entry for `handle`, answering `fallback` when the
/// handle is unknown or the body panics.
fn with_parser<T>(handle: i64, fallback: T, body: impl FnOnce(&mut ParserEntry) -> T) -> T {
    catch_unwind(AssertUnwindSafe(|| {
        let mut registry = parsers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match registry.get_mut(&handle) {
            Some(entry) => Some(body(entry)),
            None => None,
        }
    }))
    .ok()
    .flatten()
    .unwrap_or(fallback)
}

/// Runs `body` on the writer registry entry for `handle`, answering `fallback` when the
/// handle is unknown or the body panics.
fn with_writer<T>(handle: i64, fallback: T, body: impl FnOnce(&mut Writer) -> T) -> T {
    catch_unwind(AssertUnwindSafe(|| {
        let mut registry = writers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match registry.get_mut(&handle) {
            Some(entry) => Some(body(entry)),
            None => None,
        }
    }))
    .ok()
    .flatten()
    .unwrap_or(fallback)
}

/// Reads a NUL-terminated C string argument; a null pointer reads as empty.
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated byte string.
unsafe fn c_bytes<'a>(ptr: *const c_char) -> &'a [u8] {
    if ptr.is_null() {
        &[]
    } else {
        CStr::from_ptr(ptr).to_bytes()
    }
}

/// Reads an optional C string argument: `present == 0` is `None`.
///
/// # Safety
/// `ptr` must be null or point at a NUL-terminated byte string.
unsafe fn c_optional<'a>(present: i64, ptr: *const c_char) -> Option<&'a [u8]> {
    if present == 0 {
        None
    } else {
        Some(c_bytes(ptr))
    }
}

/// The pointer every string entry point answers when its thread-local cell is unusable
/// (thread teardown): a static empty C string, never null.
static EMPTY_C_STRING: [c_char; 1] = [0];

/// Stores `bytes` in a thread-local cell and returns a pointer valid until the next call
/// that uses the same cell on this thread. Interior NULs are dropped rather than truncating
/// the buffer; the only engine output that can contain them (a writer's multi-byte-encoded
/// document) is read through `elephc_xml_writer_output_hex` instead.
fn stash(cell: &'static std::thread::LocalKey<RefCell<CString>>, bytes: Vec<u8>) -> *const c_char {
    let clean: Vec<u8> = bytes.into_iter().filter(|byte| *byte != 0).collect();
    let owned = CString::new(clean).unwrap_or_default();
    cell.try_with(|slot| {
        *slot.borrow_mut() = owned;
        slot.borrow().as_ptr()
    })
    .unwrap_or(EMPTY_C_STRING.as_ptr())
}

/// Runs `body` under `catch_unwind`, answering `fallback` when it panics.
fn guarded<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

/// Converts a Rust boolean into the ABI's `1` / `0`.
fn flag(value: bool) -> i64 {
    i64::from(value)
}

thread_local! {
    static PARSER_EVENT_STRING: RefCell<CString> = RefCell::new(CString::default());
    static PARSER_ATTR_NAME: RefCell<CString> = RefCell::new(CString::default());
    static PARSER_ATTR_VALUE: RefCell<CString> = RefCell::new(CString::default());
    static PARSER_NS_PREFIX: RefCell<CString> = RefCell::new(CString::default());
    static PARSER_NS_URI: RefCell<CString> = RefCell::new(CString::default());
    static PARSER_TARGET_ENCODING: RefCell<CString> = RefCell::new(CString::default());
    static ERROR_STRING: RefCell<CString> = RefCell::new(CString::default());
    static WRITER_OUTPUT: RefCell<CString> = RefCell::new(CString::default());
    static WRITER_TAKEN: RefCell<CString> = RefCell::new(CString::default());
    static WRITER_HEX: RefCell<CString> = RefCell::new(CString::default());
}

// ---------------------------------------------------------------------------------------
// Parser entry points
// ---------------------------------------------------------------------------------------

/// C ABI: creates a parser (`xml_parser_create` / `xml_parser_create_ns`). `namespaces`
/// selects namespace mode with `separator` as the URI/local joiner. Returns the handle.
///
/// # Safety
/// `separator` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_parser_create(namespaces: i64, separator: *const c_char) -> i64 {
    let separator = if namespaces != 0 {
        Some(c_bytes(separator).to_vec())
    } else {
        None
    };
    catch_unwind(AssertUnwindSafe(|| {
        let handle = next_handle();
        let entry = ParserEntry {
            parser: Parser::new(separator),
            event: None,
            case_folding: true,
            skip_tagstart: 0,
            skip_white: false,
            parse_huge: false,
            target_encoding: TargetEncoding::Utf8,
        };
        parsers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(handle, entry);
        handle
    }))
    .unwrap_or(0)
}

/// C ABI: destroys a parser; answers 1 when the handle existed.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_free(handle: i64) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        flag(
            parsers()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&handle)
                .is_some(),
        )
    }))
    .unwrap_or(0)
}

/// C ABI: sets an integer/boolean `XML_OPTION_*`; answers 1, or 0 for an unknown option.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_set_option(handle: i64, option: i64, value: i64) -> i64 {
    with_parser(handle, 0, |entry| match option {
        OPTION_CASE_FOLDING => {
            entry.case_folding = value != 0;
            1
        }
        OPTION_SKIP_TAGSTART => {
            entry.skip_tagstart = value;
            1
        }
        OPTION_SKIP_WHITE => {
            entry.skip_white = value != 0;
            1
        }
        OPTION_PARSE_HUGE => {
            entry.parse_huge = value != 0;
            entry.parser.set_parse_huge(value != 0);
            1
        }
        _ => 0,
    })
}

/// C ABI: reads an integer/boolean `XML_OPTION_*` (booleans as 0/1); 0 for unknown options.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_get_option(handle: i64, option: i64) -> i64 {
    with_parser(handle, 0, |entry| match option {
        OPTION_CASE_FOLDING => flag(entry.case_folding),
        OPTION_SKIP_TAGSTART => entry.skip_tagstart,
        OPTION_SKIP_WHITE => flag(entry.skip_white),
        OPTION_PARSE_HUGE => flag(entry.parse_huge),
        _ => 0,
    })
}

/// C ABI: sets `XML_OPTION_TARGET_ENCODING`; answers 1, or 0 for an unsupported name.
///
/// # Safety
/// `name` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_parser_set_target_encoding(
    handle: i64,
    name: *const c_char,
) -> i64 {
    let Some(encoding) = guarded(None, || TargetEncoding::parse(c_bytes(name))) else {
        return 0;
    };
    with_parser(handle, 0, |entry| {
        entry.target_encoding = encoding;
        1
    })
}

/// C ABI: answers 1 when `name` is a supported source/target encoding name.
///
/// # Safety
/// `name` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_encoding_supported(name: *const c_char) -> i64 {
    guarded(0, || flag(TargetEncoding::parse(c_bytes(name)).is_some()))
}

/// C ABI: the canonical target-encoding name (`xml_parser_get_option`).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_target_encoding(handle: i64) -> *const c_char {
    let name = with_parser(handle, "", |entry| entry.target_encoding.name());
    stash(&PARSER_TARGET_ENCODING, name.as_bytes().to_vec())
}

/// C ABI: appends `len` bytes of input; `is_final` marks the end of the document.
///
/// # Safety
/// `data` must point at `len` readable bytes (or be null with `len == 0`).
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_parser_feed(
    handle: i64,
    data: *const c_char,
    len: i64,
    is_final: i64,
) -> i64 {
    let chunk: &[u8] = guarded(&[], || {
        if data.is_null() || len <= 0 {
            &[]
        } else {
            std::slice::from_raw_parts(data as *const u8, len as usize)
        }
    });
    with_parser(handle, 0, |entry| {
        entry.parser.feed(chunk, is_final != 0);
        1
    })
}

/// C ABI: parses the next item and answers its `EVENT_*` code.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_next(handle: i64) -> i64 {
    with_parser(handle, EVENT_FAILED, |entry| match entry.parser.next() {
        Step::NeedMoreData => {
            entry.event = None;
            EVENT_NEED_MORE
        }
        Step::Finished => {
            entry.event = None;
            EVENT_FINISHED
        }
        Step::Failed => {
            entry.event = None;
            EVENT_FAILED
        }
        Step::Event(event) => {
            let code = match &event {
                Event::StartElement { .. } => EVENT_START_ELEMENT,
                Event::EndElement { .. } => EVENT_END_ELEMENT,
                Event::Characters(_) => EVENT_CHARACTERS,
                Event::ProcessingInstruction { .. } => EVENT_PI,
                Event::Comment(_) => EVENT_COMMENT,
                Event::EntityRef { .. } => EVENT_ENTITY_REF,
                Event::NotationDecl { .. } => EVENT_NOTATION_DECL,
                Event::UnparsedEntityDecl { .. } => EVENT_UNPARSED_ENTITY_DECL,
            };
            entry.event = Some(event);
            code
        }
    })
}

/// C ABI: one string field of the current event (`FIELD_*`), already case-folded (names)
/// and converted to the target encoding.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_string(handle: i64, field: i64) -> *const c_char {
    let bytes = with_parser(handle, Vec::new(), |entry| {
        let Some(event) = entry.event.as_ref() else {
            return Vec::new();
        };
        let raw: Option<&[u8]> = match (event, field) {
            (Event::StartElement { name, .. }, FIELD_NAME)
            | (Event::EndElement { name, .. }, FIELD_NAME) => {
                return entry.encode_tag(name);
            }
            (Event::StartElement { raw, .. }, FIELD_SECOND)
            | (Event::EndElement { raw, .. }, FIELD_SECOND) => Some(raw),
            (Event::Characters(text), FIELD_NAME) => Some(text),
            (Event::ProcessingInstruction { target, .. }, FIELD_NAME) => Some(target),
            (Event::ProcessingInstruction { data, .. }, FIELD_SECOND) => data.as_deref(),
            (Event::Comment(text), FIELD_NAME) => Some(text),
            (Event::EntityRef { name, .. }, FIELD_NAME) => Some(name),
            (Event::EntityRef { kind, .. }, FIELD_SECOND) => match kind {
                EntityKind::Predefined { expansion } => Some(expansion),
                EntityKind::Internal { replacement } => Some(replacement),
                EntityKind::External { system_id, .. } => Some(system_id),
                EntityKind::Unparsed | EntityKind::Undeclared => None,
            },
            (Event::EntityRef { kind, .. }, FIELD_THIRD) => match kind {
                EntityKind::External { public_id, .. } => public_id.as_deref(),
                _ => None,
            },
            (Event::NotationDecl { name, .. }, FIELD_NAME) => Some(name),
            (Event::NotationDecl { system_id, .. }, FIELD_SECOND) => system_id.as_deref(),
            (Event::NotationDecl { public_id, .. }, FIELD_THIRD) => public_id.as_deref(),
            (Event::UnparsedEntityDecl { name, .. }, FIELD_NAME) => Some(name),
            (Event::UnparsedEntityDecl { system_id, .. }, FIELD_SECOND) => Some(system_id),
            (Event::UnparsedEntityDecl { public_id, .. }, FIELD_THIRD) => public_id.as_deref(),
            (Event::UnparsedEntityDecl { notation, .. }, FIELD_FOURTH) => Some(notation),
            _ => None,
        };
        raw.map(|bytes| entry.encode(bytes)).unwrap_or_default()
    });
    stash(&PARSER_EVENT_STRING, bytes)
}

/// C ABI: one integer field of the current event (`INT_*`).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_int(handle: i64, field: i64) -> i64 {
    with_parser(handle, 0, |entry| {
        let Some(event) = entry.event.as_ref() else {
            return 0;
        };
        match (event, field) {
            (Event::StartElement { attributes, .. }, INT_ATTR_COUNT) => attributes.len() as i64,
            (Event::StartElement { namespaces, .. }, INT_NS_COUNT) => namespaces.len() as i64,
            (Event::EntityRef { kind, .. }, INT_ENTITY_KIND) => match kind {
                EntityKind::Predefined { .. } => ENTITY_PREDEFINED,
                EntityKind::Internal { .. } => ENTITY_INTERNAL,
                EntityKind::External { .. } => ENTITY_EXTERNAL,
                EntityKind::Unparsed => ENTITY_UNPARSED,
                EntityKind::Undeclared => ENTITY_UNDECLARED,
            },
            (Event::EntityRef { kind, .. }, INT_FLAGS) => match kind {
                EntityKind::External { public_id, .. } => {
                    FLAG_HAS_SYSTEM_ID | if public_id.is_some() { FLAG_HAS_PUBLIC_ID } else { 0 }
                }
                _ => 0,
            },
            (Event::ProcessingInstruction { data, .. }, INT_FLAGS) => {
                if data.is_some() {
                    FLAG_HAS_PI_DATA
                } else {
                    0
                }
            }
            (
                Event::NotationDecl {
                    public_id,
                    system_id,
                    ..
                },
                INT_FLAGS,
            ) => {
                (if public_id.is_some() { FLAG_HAS_PUBLIC_ID } else { 0 })
                    | (if system_id.is_some() { FLAG_HAS_SYSTEM_ID } else { 0 })
            }
            (Event::UnparsedEntityDecl { public_id, .. }, INT_FLAGS) => {
                FLAG_HAS_SYSTEM_ID | if public_id.is_some() { FLAG_HAS_PUBLIC_ID } else { 0 }
            }
            _ => 0,
        }
    })
}

/// C ABI: the case-folded, target-encoded name of attribute `index` of the current start tag.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_attr_name(handle: i64, index: i64) -> *const c_char {
    let bytes = with_parser(handle, Vec::new(), |entry| match entry.event.as_ref() {
        Some(Event::StartElement { attributes, .. }) => attributes
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|attribute| entry.encode_tag(&attribute.name))
            .unwrap_or_default(),
        _ => Vec::new(),
    });
    stash(&PARSER_ATTR_NAME, bytes)
}

/// C ABI: the target-encoded value of attribute `index` of the current start tag.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_attr_value(handle: i64, index: i64) -> *const c_char {
    let bytes = with_parser(handle, Vec::new(), |entry| match entry.event.as_ref() {
        Some(Event::StartElement { attributes, .. }) => attributes
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|attribute| entry.encode(&attribute.value))
            .unwrap_or_default(),
        _ => Vec::new(),
    });
    stash(&PARSER_ATTR_VALUE, bytes)
}

/// C ABI: answers 1 when namespace declaration `index` of the current start tag has a
/// prefix (the default namespace has none and PHP reports `false` for it).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_ns_has_prefix(handle: i64, index: i64) -> i64 {
    with_parser(handle, 0, |entry| match entry.event.as_ref() {
        Some(Event::StartElement { namespaces, .. }) => namespaces
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|decl| flag(decl.prefix.is_some()))
            .unwrap_or(0),
        _ => 0,
    })
}

/// C ABI: the prefix of namespace declaration `index` of the current start tag.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_ns_prefix(handle: i64, index: i64) -> *const c_char {
    let bytes = with_parser(handle, Vec::new(), |entry| match entry.event.as_ref() {
        Some(Event::StartElement { namespaces, .. }) => namespaces
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .and_then(|decl| decl.prefix.as_ref())
            .map(|prefix| entry.encode(prefix))
            .unwrap_or_default(),
        _ => Vec::new(),
    });
    stash(&PARSER_NS_PREFIX, bytes)
}

/// C ABI: the URI of namespace declaration `index` of the current start tag.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_event_ns_uri(handle: i64, index: i64) -> *const c_char {
    let bytes = with_parser(handle, Vec::new(), |entry| match entry.event.as_ref() {
        Some(Event::StartElement { namespaces, .. }) => namespaces
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|decl| entry.encode(&decl.uri))
            .unwrap_or_default(),
        _ => Vec::new(),
    });
    stash(&PARSER_NS_URI, bytes)
}

/// C ABI: the last libxml2 error code (`xml_get_error_code`).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_error_code(handle: i64) -> i64 {
    with_parser(handle, 0, |entry| i64::from(entry.parser.error_code()))
}

/// C ABI: 1 while no error has been recorded (`xml_parse`'s status).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_well_formed(handle: i64) -> i64 {
    with_parser(handle, 0, |entry| flag(entry.parser.is_well_formed()))
}

/// C ABI: current line (`xml_get_current_line_number`).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_line(handle: i64) -> i64 {
    with_parser(handle, 0, |entry| entry.parser.line())
}

/// C ABI: current column (`xml_get_current_column_number`).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_column(handle: i64) -> i64 {
    with_parser(handle, 0, |entry| entry.parser.column())
}

/// C ABI: bytes consumed (`xml_get_current_byte_index`).
#[no_mangle]
pub extern "C" fn elephc_xml_parser_byte_index(handle: i64) -> i64 {
    with_parser(handle, 0, |entry| entry.parser.byte_index())
}

/// C ABI: stops the parser (`xmlStopParser`); `code` 0 keeps the current error code.
#[no_mangle]
pub extern "C" fn elephc_xml_parser_stop(handle: i64, code: i64) -> i64 {
    with_parser(handle, 0, |entry| {
        entry
            .parser
            .stop(i32::try_from(code).unwrap_or(parser::XML_ERR_OK));
        1
    })
}

/// C ABI: PHP's `xml_error_string()` text for a libxml2 error code.
#[no_mangle]
pub extern "C" fn elephc_xml_error_string(code: i64) -> *const c_char {
    // php-src hands the PHP integer to a C `int` parameter, so the code wraps to 32 bits.
    let text = guarded("Unknown", || parser::error_string(code as i32));
    stash(&ERROR_STRING, text.as_bytes().to_vec())
}

// ---------------------------------------------------------------------------------------
// Writer entry points
// ---------------------------------------------------------------------------------------

/// C ABI: creates a memory writer and returns its handle.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_create() -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let handle = next_handle();
        writers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(handle, Writer::new());
        handle
    }))
    .unwrap_or(0)
}

/// C ABI: destroys a writer; answers 1 when the handle existed.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_free(handle: i64) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        flag(
            writers()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&handle)
                .is_some(),
        )
    }))
    .unwrap_or(0)
}

/// C ABI: `xmlValidateName` on a NUL-terminated name; PHP's `XMLW_NAME_CHK` gate.
///
/// # Safety
/// `name` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_valid_name(name: *const c_char) -> i64 {
    guarded(0, || flag(crate::writer::is_valid_name(c_bytes(name))))
}

/// C ABI: `XMLWriter::setIndent`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_set_indent(handle: i64, enable: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.set_indent(enable != 0)))
}

/// C ABI: `XMLWriter::setIndentString`.
///
/// # Safety
/// `indent` must be null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_set_indent_string(
    handle: i64,
    indent: *const c_char,
) -> i64 {
    let indent = c_bytes(indent);
    with_writer(handle, 0, |writer| flag(writer.set_indent_string(indent)))
}

/// C ABI: `XMLWriter::startDocument`; each optional string travels as a presence flag plus
/// the text.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_document(
    handle: i64,
    has_version: i64,
    version: *const c_char,
    has_encoding: i64,
    encoding: *const c_char,
    has_standalone: i64,
    standalone: *const c_char,
) -> i64 {
    let version = c_optional(has_version, version);
    let encoding = c_optional(has_encoding, encoding);
    let standalone = c_optional(has_standalone, standalone);
    with_writer(handle, 0, |writer| {
        flag(writer.start_document(version, encoding, standalone))
    })
}

/// C ABI: `XMLWriter::endDocument`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_document(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_document()))
}

/// C ABI: `XMLWriter::startComment`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_start_comment(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.start_comment()))
}

/// C ABI: `XMLWriter::endComment`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_comment(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_comment()))
}

/// C ABI: `XMLWriter::writeComment`.
///
/// # Safety
/// `content` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_comment(handle: i64, content: *const c_char) -> i64 {
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_comment(content)))
}

/// C ABI: `XMLWriter::startElement`.
///
/// # Safety
/// `name` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_element(handle: i64, name: *const c_char) -> i64 {
    let name = c_bytes(name);
    with_writer(handle, 0, |writer| flag(writer.start_element(name)))
}

/// C ABI: `XMLWriter::startElementNs`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_element_ns(
    handle: i64,
    has_prefix: i64,
    prefix: *const c_char,
    name: *const c_char,
    has_uri: i64,
    uri: *const c_char,
) -> i64 {
    let prefix = c_optional(has_prefix, prefix);
    let name = c_bytes(name);
    let uri = c_optional(has_uri, uri);
    with_writer(handle, 0, |writer| flag(writer.start_element_ns(prefix, name, uri)))
}

/// C ABI: `XMLWriter::endElement`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_element(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_element()))
}

/// C ABI: `XMLWriter::fullEndElement`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_full_end_element(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.full_end_element()))
}

/// C ABI: `XMLWriter::writeElement`; `has_content == 0` writes an empty-element tag.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_element(
    handle: i64,
    name: *const c_char,
    has_content: i64,
    content: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let content = c_optional(has_content, content);
    with_writer(handle, 0, |writer| flag(writer.write_element(name, content)))
}

/// C ABI: `XMLWriter::writeElementNs`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_element_ns(
    handle: i64,
    has_prefix: i64,
    prefix: *const c_char,
    name: *const c_char,
    has_uri: i64,
    uri: *const c_char,
    has_content: i64,
    content: *const c_char,
) -> i64 {
    let prefix = c_optional(has_prefix, prefix);
    let name = c_bytes(name);
    let uri = c_optional(has_uri, uri);
    let content = c_optional(has_content, content);
    with_writer(handle, 0, |writer| {
        flag(writer.write_element_ns(prefix, name, uri, content))
    })
}

/// C ABI: `XMLWriter::startAttribute`.
///
/// # Safety
/// `name` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_attribute(handle: i64, name: *const c_char) -> i64 {
    let name = c_bytes(name);
    with_writer(handle, 0, |writer| flag(writer.start_attribute(name)))
}

/// C ABI: `XMLWriter::startAttributeNs`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_attribute_ns(
    handle: i64,
    has_prefix: i64,
    prefix: *const c_char,
    name: *const c_char,
    has_uri: i64,
    uri: *const c_char,
) -> i64 {
    let prefix = c_optional(has_prefix, prefix);
    let name = c_bytes(name);
    let uri = c_optional(has_uri, uri);
    with_writer(handle, 0, |writer| {
        flag(writer.start_attribute_ns(prefix, name, uri))
    })
}

/// C ABI: `XMLWriter::endAttribute`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_attribute(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_attribute()))
}

/// C ABI: `XMLWriter::writeAttribute`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_attribute(
    handle: i64,
    name: *const c_char,
    value: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let value = c_bytes(value);
    with_writer(handle, 0, |writer| flag(writer.write_attribute(name, value)))
}

/// C ABI: `XMLWriter::writeAttributeNs`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_attribute_ns(
    handle: i64,
    has_prefix: i64,
    prefix: *const c_char,
    name: *const c_char,
    has_uri: i64,
    uri: *const c_char,
    value: *const c_char,
) -> i64 {
    let prefix = c_optional(has_prefix, prefix);
    let name = c_bytes(name);
    let uri = c_optional(has_uri, uri);
    let value = c_bytes(value);
    with_writer(handle, 0, |writer| {
        flag(writer.write_attribute_ns(prefix, name, uri, value))
    })
}

/// C ABI: `XMLWriter::startPi`.
///
/// # Safety
/// `target` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_pi(handle: i64, target: *const c_char) -> i64 {
    let target = c_bytes(target);
    with_writer(handle, 0, |writer| flag(writer.start_pi(target)))
}

/// C ABI: `XMLWriter::endPi`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_pi(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_pi()))
}

/// C ABI: `XMLWriter::writePi`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_pi(
    handle: i64,
    target: *const c_char,
    content: *const c_char,
) -> i64 {
    let target = c_bytes(target);
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_pi(target, content)))
}

/// C ABI: `XMLWriter::startCdata`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_start_cdata(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.start_cdata()))
}

/// C ABI: `XMLWriter::endCdata`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_cdata(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_cdata()))
}

/// C ABI: `XMLWriter::writeCdata`.
///
/// # Safety
/// `content` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_cdata(handle: i64, content: *const c_char) -> i64 {
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_cdata(content)))
}

/// C ABI: `XMLWriter::text`.
///
/// # Safety
/// `content` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_text(handle: i64, content: *const c_char) -> i64 {
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_string(content)))
}

/// C ABI: `XMLWriter::writeRaw`.
///
/// # Safety
/// `content` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_raw(handle: i64, content: *const c_char) -> i64 {
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_raw(content)))
}

/// C ABI: `XMLWriter::startDtd`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_dtd(
    handle: i64,
    name: *const c_char,
    has_public_id: i64,
    public_id: *const c_char,
    has_system_id: i64,
    system_id: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let public_id = c_optional(has_public_id, public_id);
    let system_id = c_optional(has_system_id, system_id);
    with_writer(handle, 0, |writer| {
        flag(writer.start_dtd(name, public_id, system_id))
    })
}

/// C ABI: `XMLWriter::endDtd`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_dtd(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_dtd()))
}

/// C ABI: `XMLWriter::writeDtd`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_dtd(
    handle: i64,
    name: *const c_char,
    has_public_id: i64,
    public_id: *const c_char,
    has_system_id: i64,
    system_id: *const c_char,
    has_content: i64,
    content: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let public_id = c_optional(has_public_id, public_id);
    let system_id = c_optional(has_system_id, system_id);
    let content = c_optional(has_content, content);
    with_writer(handle, 0, |writer| {
        flag(writer.write_dtd(name, public_id, system_id, content))
    })
}

/// C ABI: `XMLWriter::startDtdElement`.
///
/// # Safety
/// `name` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_dtd_element(handle: i64, name: *const c_char) -> i64 {
    let name = c_bytes(name);
    with_writer(handle, 0, |writer| flag(writer.start_dtd_element(name)))
}

/// C ABI: `XMLWriter::endDtdElement`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_dtd_element(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_dtd_element()))
}

/// C ABI: `XMLWriter::writeDtdElement`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_dtd_element(
    handle: i64,
    name: *const c_char,
    content: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_dtd_element(name, content)))
}

/// C ABI: `XMLWriter::startDtdAttlist`.
///
/// # Safety
/// `name` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_dtd_attlist(handle: i64, name: *const c_char) -> i64 {
    let name = c_bytes(name);
    with_writer(handle, 0, |writer| flag(writer.start_dtd_attlist(name)))
}

/// C ABI: `XMLWriter::endDtdAttlist`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_dtd_attlist(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_dtd_attlist()))
}

/// C ABI: `XMLWriter::writeDtdAttlist`.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_dtd_attlist(
    handle: i64,
    name: *const c_char,
    content: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let content = c_bytes(content);
    with_writer(handle, 0, |writer| flag(writer.write_dtd_attlist(name, content)))
}

/// C ABI: `XMLWriter::startDtdEntity`.
///
/// # Safety
/// `name` must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_start_dtd_entity(
    handle: i64,
    name: *const c_char,
    is_param: i64,
) -> i64 {
    let name = c_bytes(name);
    with_writer(handle, 0, |writer| {
        flag(writer.start_dtd_entity(name, is_param != 0))
    })
}

/// C ABI: `XMLWriter::endDtdEntity`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_end_dtd_entity(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| flag(writer.end_dtd_entity()))
}

/// Presence bit of `elephc_xml_writer_write_dtd_entity`'s public id.
pub const DTD_ENTITY_HAS_PUBLIC_ID: i64 = 1;
/// Presence bit of `elephc_xml_writer_write_dtd_entity`'s system id.
pub const DTD_ENTITY_HAS_SYSTEM_ID: i64 = 2;
/// Presence bit of `elephc_xml_writer_write_dtd_entity`'s notation.
pub const DTD_ENTITY_HAS_NOTATION: i64 = 4;

/// C ABI: `XMLWriter::writeDtdEntity` (`xmlTextWriterWriteDTDEntity`, which picks the
/// external form when a public or system id is given). The three optional ids share one
/// `flags` presence bitmask so the call fits the eight-integer extern ABI.
///
/// # Safety
/// Every string pointer must be null or NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_writer_write_dtd_entity(
    handle: i64,
    name: *const c_char,
    content: *const c_char,
    is_param: i64,
    flags: i64,
    public_id: *const c_char,
    system_id: *const c_char,
    notation: *const c_char,
) -> i64 {
    let name = c_bytes(name);
    let content = c_bytes(content);
    let public_id = c_optional(flags & DTD_ENTITY_HAS_PUBLIC_ID, public_id);
    let system_id = c_optional(flags & DTD_ENTITY_HAS_SYSTEM_ID, system_id);
    let notation = c_optional(flags & DTD_ENTITY_HAS_NOTATION, notation);
    with_writer(handle, 0, |writer| {
        flag(writer.write_dtd_entity(
            name,
            Some(content),
            is_param != 0,
            public_id,
            system_id,
            notation,
        ))
    })
}

/// C ABI: the buffered output without consuming it (`outputMemory(false)`); flushes the
/// writer's conversion buffer first, as `xmlTextWriterFlush` does before php-src reads
/// the memory buffer.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_output(handle: i64) -> *const c_char {
    let bytes = with_writer(handle, Vec::new(), |writer| {
        writer.flush();
        writer.output().to_vec()
    });
    stash(&WRITER_OUTPUT, bytes)
}

/// C ABI: takes the buffered output, leaving the writer empty (`outputMemory(true)`).
#[no_mangle]
pub extern "C" fn elephc_xml_writer_take_output(handle: i64) -> *const c_char {
    let bytes = with_writer(handle, Vec::new(), |writer| writer.take_output());
    stash(&WRITER_TAKEN, bytes)
}

/// C ABI: the number of buffered output bytes after flushing the conversion buffer.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_output_len(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| {
        writer.flush();
        writer.output().len() as i64
    })
}

/// C ABI: answers 1 when the buffered output (after flushing) contains a NUL byte, which
/// a C string cannot carry: a document written in UTF-16 / UCS-2 / UCS-4. The prelude then
/// reads it through `elephc_xml_writer_output_hex`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_output_has_nul(handle: i64) -> i64 {
    with_writer(handle, 0, |writer| {
        writer.flush();
        flag(writer.output().contains(&0))
    })
}

/// C ABI: the buffered output as lowercase hexadecimal (binary-safe); `take != 0` empties
/// the writer like `elephc_xml_writer_take_output`, `0` peeks like `elephc_xml_writer_output`.
#[no_mangle]
pub extern "C" fn elephc_xml_writer_output_hex(handle: i64, take: i64) -> *const c_char {
    let bytes = with_writer(handle, Vec::new(), |writer| {
        if take != 0 {
            writer.take_output()
        } else {
            writer.flush();
            writer.output().to_vec()
        }
    });
    let mut hex = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(HEX_DIGITS[(byte >> 4) as usize]);
        hex.push(HEX_DIGITS[(byte & 0x0f) as usize]);
    }
    stash(&WRITER_HEX, hex)
}

/// Lowercase hexadecimal digits for `elephc_xml_writer_output_hex`.
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the ABI plumbing that sits above the engines: handle lifecycle, option storage,
    //! the target-encoding conversion, and the fail-closed answers for unknown handles.
    //!
    //! Called from:
    //! - `cargo test -p elephc-xml` through Rust's test harness.
    //!
    //! Key details:
    //! - Engine behavior is covered by the engine modules; these tests avoid depending on
    //!   parser/writer internals beyond creating and freeing handles.
    //! - Anything that reaches a registry entry (and so libxml2) is gated on
    //!   `elephc_xml_native`; the pure string-policy tests always run.

    use super::*;

    /// Unknown handles answer the failure value without touching any registry entry.
    #[cfg(elephc_xml_native)]
    #[test]
    fn unknown_handles_fail_closed() {
        assert_eq!(elephc_xml_parser_free(0), 0);
        assert_eq!(elephc_xml_parser_get_option(0, OPTION_CASE_FOLDING), 0);
        assert_eq!(elephc_xml_parser_next(0), EVENT_FAILED);
        assert_eq!(elephc_xml_writer_free(0), 0);
        assert_eq!(elephc_xml_writer_end_element(0), 0);
    }

    /// The target-encoding conversion replaces unrepresentable code points with `?`.
    #[test]
    fn target_encoding_replaces_unrepresentable_code_points() {
        assert_eq!(TargetEncoding::Iso88591.decode("caf\u{e9} \u{20ac}".as_bytes()), b"caf\xe9 ?");
        assert_eq!(TargetEncoding::UsAscii.decode("caf\u{e9}".as_bytes()), b"caf?");
        assert_eq!(TargetEncoding::Utf8.decode("caf\u{e9}".as_bytes()), "caf\u{e9}".as_bytes());
        assert_eq!(TargetEncoding::parse(b"utf-8"), Some(TargetEncoding::Utf8));
        assert_eq!(TargetEncoding::parse(b"iso-8859-1"), Some(TargetEncoding::Iso88591));
        assert_eq!(TargetEncoding::parse(b"bogus"), None);
    }

    /// Option storage round-trips through the getters with PHP's defaults.
    /// A UTF-16 document carries NUL bytes: the C-string entry points cannot deliver it,
    /// `elephc_xml_writer_output_has_nul` says so, and the hex entry point hands over the
    /// exact bytes (peek keeps them, take empties the writer).
    #[cfg(elephc_xml_native)]
    #[test]
    fn writer_output_with_nul_bytes_crosses_hex_encoded() {
        let handle = elephc_xml_writer_create();
        let version = CString::new("1.0").unwrap();
        let encoding = CString::new("UTF-16").unwrap();
        let name = CString::new("r").unwrap();
        let content = CString::new("\u{e9}").unwrap();
        unsafe {
            assert_eq!(
                elephc_xml_writer_start_document(handle, 1, version.as_ptr(), 1, encoding.as_ptr(), 0, std::ptr::null()),
                1
            );
            assert_eq!(elephc_xml_writer_write_element(handle, name.as_ptr(), 1, content.as_ptr()), 1);
        }
        assert_eq!(elephc_xml_writer_output_has_nul(handle), 1);
        assert_eq!(elephc_xml_writer_output_len(handle), 98);
        let peeked = unsafe { CStr::from_ptr(elephc_xml_writer_output_hex(handle, 0)) }.to_bytes().to_vec();
        assert_eq!(peeked.len(), 196);
        assert!(peeked.starts_with(b"fffe3c003f00"));
        let taken = unsafe { CStr::from_ptr(elephc_xml_writer_output_hex(handle, 1)) }.to_bytes().to_vec();
        assert_eq!(taken, peeked);
        assert_eq!(elephc_xml_writer_output_len(handle), 0);
        assert_eq!(elephc_xml_writer_output_has_nul(handle), 0);
        assert_eq!(elephc_xml_writer_free(handle), 1);
    }

    #[cfg(elephc_xml_native)]
    #[test]
    fn parser_options_round_trip() {
        let handle = unsafe { elephc_xml_parser_create(0, std::ptr::null()) };
        assert!(handle > 0);
        assert_eq!(elephc_xml_parser_get_option(handle, OPTION_CASE_FOLDING), 1);
        assert_eq!(elephc_xml_parser_get_option(handle, OPTION_SKIP_TAGSTART), 0);
        assert_eq!(elephc_xml_parser_get_option(handle, OPTION_SKIP_WHITE), 0);
        assert_eq!(elephc_xml_parser_get_option(handle, OPTION_PARSE_HUGE), 0);
        assert_eq!(elephc_xml_parser_set_option(handle, OPTION_SKIP_TAGSTART, 3), 1);
        assert_eq!(elephc_xml_parser_get_option(handle, OPTION_SKIP_TAGSTART), 3);
        assert_eq!(elephc_xml_parser_set_option(handle, 99, 1), 0);
        let name = CString::new("iso-8859-1").expect("literal");
        assert_eq!(
            unsafe { elephc_xml_parser_set_target_encoding(handle, name.as_ptr()) },
            1
        );
        let reported = unsafe { CStr::from_ptr(elephc_xml_parser_target_encoding(handle)) };
        assert_eq!(reported.to_bytes(), b"ISO-8859-1");
        assert_eq!(elephc_xml_parser_free(handle), 1);
        assert_eq!(elephc_xml_parser_free(handle), 0);
    }

    /// The error-string entry point defers to the parser table, with "Unknown" outside it.
    #[test]
    fn error_strings_come_from_the_parser_table() {
        let unknown = unsafe { CStr::from_ptr(elephc_xml_error_string(-7)) };
        assert_eq!(unknown.to_bytes(), b"Unknown");
    }
}
