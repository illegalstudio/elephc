//! Purpose:
//! Read-only PHP value formatting for the native debugger. Locals, boxed Mixed
//! cells, indexed arrays, associative hashes, strings and reference cells are
//! decoded from the runtime layouts documented in `docs/internals/memory-model.md`.
//!
//! Called from:
//! - the human stdin session and the DAP adapter in `crates/elephc-debug`.
//!
//! Key details:
//! - Inspection is borrowed: this module never increfs, decrefs, triggers COW or
//!   invokes user code.
//! - Every pointer is treated as untrusted. Null words, implausible lengths and
//!   cycles are rejected or capped instead of dereferenced.
//! - The returned `Description` is display-only; `Value` can be used later to
//!   enumerate array/hash children without materializing a PHP copy.

use serde::Deserialize;

use super::Local;

/// Runtime value tag for PHP `int`.
pub(crate) const TAG_INT: i64 = 0;
/// Runtime value tag for PHP `string`.
pub(crate) const TAG_STRING: i64 = 1;
/// Runtime value tag for PHP `float`.
pub(crate) const TAG_FLOAT: i64 = 2;
/// Runtime value tag for PHP `bool`.
pub(crate) const TAG_BOOL: i64 = 3;
/// Runtime value tag for an indexed array.
pub(crate) const TAG_ARRAY: i64 = 4;
/// Runtime value tag for an associative array.
pub(crate) const TAG_HASH: i64 = 5;
/// Runtime value tag for an object.
pub(crate) const TAG_OBJECT: i64 = 6;
/// Runtime value tag for a nested boxed Mixed cell.
pub(crate) const TAG_MIXED: i64 = 7;
/// Runtime value tag for PHP `null`.
pub(crate) const TAG_NULL: i64 = 8;
/// Runtime value tag for a PHP resource.
pub(crate) const TAG_RESOURCE: i64 = 9;
/// Runtime value tag for a callable descriptor.
pub(crate) const TAG_CALLABLE: i64 = 10;
/// Indexed-array storage tag for inline tagged scalars.
const TAGGED_SCALAR_ARRAY: i64 = 11;
/// Hash/property storage tag for an inline tagged scalar.
const TAGGED_SCALAR_PROPERTY: i64 = 12;
/// Internal display tag for an uninitialized typed property.
const TAG_UNINITIALIZED: i64 = -2;
/// Marker stored in the high word of an uninitialized typed property.
const UNINITIALIZED_TYPED_PROPERTY_SENTINEL: u64 = 0x7fff_ffff_ffff_fffd;
/// Runtime value tag for a raw pointer word stored in a container.
const TAG_POINTER: i64 = 13;
/// Uniform heap kind for an indexed array.
const HEAP_KIND_ARRAY: u64 = 2;
/// Uniform heap kind for an associative hash.
const HEAP_KIND_HASH: u64 = 3;
/// Uniform heap kind for a managed object.
const HEAP_KIND_OBJECT: u64 = 4;
/// Uniform heap kind for a managed reference cell.
const HEAP_KIND_REF_CELL: u64 = 8;
/// Minimum address accepted before a word read; rejects the null page.
const MIN_VALID_ADDRESS: usize = 0x1000;
/// Maximum bytes read from one PHP string for display.
const MAX_STRING_BYTES: usize = 1 << 20;
/// Maximum indexed-array elements read for one display.
const MAX_ARRAY_ELEMENTS: usize = 1 << 10;
/// Maximum hash entries read for one display.
const MAX_HASH_ENTRIES: usize = 1 << 10;
/// Maximum recursion depth while unboxing nested Mixed cells.
const MAX_DEPTH: usize = 8;
/// Maximum bytes of one string shown by the call-capture renderer.
pub(crate) const CAPTURE_MAX_STRING_BYTES: usize = 256;
/// Maximum container children shown by the call-capture renderer.
pub(crate) const CAPTURE_MAX_CHILDREN: usize = 16;
/// Maximum nested container depth shown by the call-capture renderer.
pub(crate) const CAPTURE_MAX_DEPTH: usize = 3;

/// One bounded call-capture rendering plus its overflow accounting.
#[derive(Clone, Debug, Default)]
pub(crate) struct CaptureRender {
    /// PHP-style text with explicit truncation/redaction markers where needed.
    pub(crate) text: String,
    /// Whether any string or container was truncated.
    pub(crate) truncated: bool,
    /// String bytes omitted by the per-string cap.
    pub(crate) string_bytes_omitted: usize,
    /// Container children omitted by the per-container cap.
    pub(crate) children_omitted: usize,
    /// Whether a container's children were hidden by the depth cap.
    pub(crate) depth_truncated: bool,
}

impl CaptureRender {
    /// Returns a plain, untruncated rendering.
    fn plain(text: String) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }

    /// Folds one child rendering's overflow counts into this container.
    fn add_child_overflow(&mut self, child: &Self) {
        self.truncated |= child.truncated;
        self.string_bytes_omitted += child.string_bytes_omitted;
        self.children_omitted += child.children_omitted;
        self.depth_truncated |= child.depth_truncated;
    }
}

/// One borrowed runtime value: a tag and its two payload words.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Value {
    /// Runtime value tag (`TAG_*`).
    pub(crate) tag: i64,
    /// Low payload word (integer value, string pointer, container pointer, …).
    pub(crate) lo: u64,
    /// High payload word (string length, tagged-scalar tag, …).
    pub(crate) hi: u64,
    /// Whether the value was reached through a reference cell.
    pub(crate) by_ref: bool,
}

/// A display-only description of one runtime value.
#[derive(Clone, Debug)]
pub(crate) struct Description {
    /// PHP-style value text without the reference marker.
    pub(crate) display: String,
    /// PHP type spelling used by the debugger.
    pub(crate) type_name: String,
    /// Whether the value was reached through a reference cell.
    pub(crate) by_ref: bool,
    /// Whether `children` can return nested entries.
    pub(crate) has_children: bool,
}

/// Scalar view of a debugger value used by conditional-breakpoint expressions.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Scalar {
    /// PHP integer.
    Int(i64),
    /// PHP float.
    Float(f64),
    /// PHP boolean.
    Bool(bool),
    /// PHP null.
    Null,
    /// PHP string.
    String(String),
    /// Arrays, objects, resources, callables and other non-scalar values.
    Other,
}

/// Converts one borrowed value to the scalar view used by conditions.
pub(crate) fn scalar_value(value: Value) -> Scalar {
    match value.tag {
        TAG_INT => Scalar::Int(value.lo as i64),
        TAG_FLOAT => Scalar::Float(f64::from_bits(value.lo)),
        TAG_BOOL => Scalar::Bool(value.lo != 0),
        TAG_NULL => Scalar::Null,
        TAG_STRING => unsafe { string_bytes(value.lo, value.hi) }
            .map(Scalar::String)
            .unwrap_or(Scalar::Other),
        TAG_MIXED => unsafe { mixed_cell(value.lo) }
            .map(scalar_value)
            .unwrap_or(Scalar::Null),
        TAGGED_SCALAR_ARRAY | TAGGED_SCALAR_PROPERTY => scalar_value(Value {
            tag: value.hi as i64,
            lo: value.lo,
            hi: 0,
            by_ref: value.by_ref,
        }),
        TAG_POINTER => {
            if value.lo == 0 {
                Scalar::Null
            } else {
                Scalar::Int(value.lo as i64)
            }
        }
        _ => Scalar::Other,
    }
}

/// One class from the embedded debug metadata.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct Class {
    /// Compiler-assigned class id stored in every instance header.
    pub(crate) id: u64,
    /// PHP-visible class name.
    pub(crate) name: String,
    /// Parent class name, empty for a root declaration. Reserved for class
    /// scopes and `parent::` display.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) parent: String,
    /// Whether the declaration is abstract. Reserved for class scopes.
    #[serde(default, rename = "abstract")]
    #[allow(dead_code)]
    pub(crate) is_abstract: bool,
    /// Instance properties in layout order.
    #[serde(default)]
    pub(crate) properties: Vec<Property>,
    /// Byte offset of the per-object dynamic-property hash, when reserved.
    #[serde(default)]
    pub(crate) dynamic_offset: Option<usize>,
    /// Static `__debugInfo()` projection or runtime-only marker.
    #[serde(default)]
    pub(crate) debug_info: DebugInfo,
    /// Hooked properties keyed by PHP-visible name. A missing entry means the
    /// property declares no `get`/`set` hook.
    #[serde(default)]
    pub(crate) property_hooks: std::collections::HashMap<String, PropertyHooks>,
}

/// Debugger view of a class's `__debugInfo()` support.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct DebugInfo {
    /// `none`, `projection` or `runtime`.
    #[serde(default)]
    pub(crate) kind: String,
    /// Key-to-property mapping for a statically folded projection.
    #[serde(default)]
    pub(crate) entries: Vec<DebugInfoEntry>,
}

/// One projected `__debugInfo()` entry: a live property read or a static literal.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct DebugInfoEntry {
    /// Array key `__debugInfo()` returns.
    pub(crate) key: String,
    /// Declared property whose slot supplies the value.
    #[serde(default)]
    pub(crate) property: Option<String>,
    /// Display-only compile-time literal text.
    #[serde(default)]
    pub(crate) value: Option<String>,
    /// PHP type of the literal value.
    #[serde(default)]
    pub(crate) php_type: Option<String>,
}

/// One instance property from a class's debug metadata.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct Property {
    /// PHP-visible property name.
    pub(crate) name: String,
    /// Byte offset from the object pointer to the property's value word.
    pub(crate) offset: usize,
    /// Declared PHP type spelling.
    pub(crate) php_type: String,
    /// `public`, `protected` or `private`.
    #[serde(default)]
    pub(crate) visibility: String,
    /// Whether the slot holds a reference-cell pointer.
    #[serde(default)]
    pub(crate) by_ref: bool,
    /// Whether the slot is a typed property that can be uninitialized.
    #[serde(default)]
    pub(crate) declared: bool,
}

/// Hooks declared on one property, decoded from the class debug metadata.
///
/// The debugger never runs these accessors. A backed hooked property is shown
/// from its raw backing slot; a virtual property has no backing slot and is
/// shown as a marker instead.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub(crate) struct PropertyHooks {
    /// A by-value `get` hook is declared.
    #[serde(default)]
    pub(crate) get: bool,
    /// A `set` hook is declared.
    #[serde(default)]
    pub(crate) set: bool,
    /// The `get` hook returns by reference.
    #[serde(default)]
    pub(crate) get_by_ref: bool,
    /// The property has hooks but no backing slot anywhere in its ancestry.
    #[serde(default, rename = "virtual")]
    pub(crate) is_virtual: bool,
}

impl PropertyHooks {
    /// Returns whether any property hook is declared.
    pub(crate) fn any(&self) -> bool {
        self.get || self.set || self.get_by_ref
    }

    /// Returns whether reading the property would invoke a getter.
    pub(crate) fn requires_get(&self) -> bool {
        self.get || self.get_by_ref
    }
}

/// Read-only caveat attached to one object property child.
#[derive(Clone, Debug)]
pub(crate) struct PropertyHookNote {
    /// Accessors declared on the property.
    pub(crate) hooks: PropertyHooks,
    /// Declared PHP type, used when a virtual property has no value to inspect.
    pub(crate) php_type: String,
}

impl PropertyHookNote {
    /// Returns whether the property has no backing slot.
    pub(crate) fn is_virtual(&self) -> bool {
        self.hooks.is_virtual
    }

    /// Returns the value text for a virtual property, which cannot be read
    /// without running user code.
    pub(crate) fn virtual_display(&self) -> String {
        match (self.hooks.requires_get(), self.hooks.set) {
            (true, true) => "virtual [get/set hooks not executed]".to_string(),
            (true, false) => "virtual [get hook not executed]".to_string(),
            (false, true) => "virtual [write-only; set hook]".to_string(),
            (false, false) => "virtual".to_string(),
        }
    }

    /// Returns the suffix for a backed getter: the displayed value is raw
    /// storage, not the value PHP code would observe through the hook.
    pub(crate) fn backing_suffix(&self) -> &'static str {
        if self.hooks.get_by_ref {
            " [backing value; get-by-ref hook not executed]"
        } else {
            " [backing value; get hook not executed]"
        }
    }
}

/// One object/array child with an optional property-hook caveat.
#[derive(Clone, Debug)]
pub(crate) struct ObjectChild {
    /// Child name as displayed by the object/array view.
    pub(crate) name: String,
    /// Borrowed child value; a placeholder when `static_text` is present.
    pub(crate) value: Value,
    /// Compile-time literal text from a safe `__debugInfo()` projection.
    pub(crate) static_text: Option<String>,
    /// PHP type of the compile-time literal.
    pub(crate) static_type: Option<String>,
    /// Hook caveat when this child is a hooked object property.
    pub(crate) hook_note: Option<PropertyHookNote>,
}

impl ObjectChild {
    /// Wraps a plain `(name, value)` pair with no hook caveat.
    fn plain(child: (String, Value)) -> Self {
        Self {
            name: child.0,
            value: child.1,
            static_text: None,
            static_type: None,
            hook_note: None,
        }
    }
}

/// Reads one aligned 64-bit word from a candidate address.
///
/// Returns `None` for the null page. The caller owns the higher-level validity
/// checks; the debugger reads a live program, so arbitrary foreign addresses are
/// not recoverable from a plain load.
unsafe fn word(address: usize) -> Option<u64> {
    if address < MIN_VALID_ADDRESS {
        return None;
    }
    Some(std::ptr::read_unaligned(address as *const u64))
}

/// Reads a PHP string payload and returns it as lossy UTF-8.
///
/// DAP is JSON, so invalid UTF-8 must be replaced rather than emitted raw.
unsafe fn string_bytes(pointer: u64, length: u64) -> Option<String> {
    if pointer == 0 {
        return None;
    }
    let length = usize::try_from(length).ok()?;
    if length > MAX_STRING_BYTES {
        return None;
    }
    let bytes = std::slice::from_raw_parts(pointer as usize as *const u8, length);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Returns the uniform heap-kind byte stored before a managed allocation.
unsafe fn heap_kind(pointer: u64) -> Option<u64> {
    let header = word(usize::try_from(pointer).ok()?.checked_sub(8)?)?;
    Some(header & 0xff)
}

/// Returns an indexed array's element value-type tag.
unsafe fn array_value_type(pointer: u64) -> Option<i64> {
    let header = word(usize::try_from(pointer).ok()?.checked_sub(8)?)?;
    Some(((header >> 8) & 0x0f) as i64)
}

/// The PHP-visible fields stored in every Throwable's fixed prefix.
pub(crate) struct ThrowableDetails {
    /// Class name resolved from the object header, or `class_id=N`.
    pub(crate) class: String,
    /// Exception message; empty when absent or unreadable.
    pub(crate) message: String,
    /// Exception code from the fixed layout.
    pub(crate) code: i64,
}

/// Reads a Throwable's class, message and code from the runtime object layout.
///
/// The fixed prefix is `class_id @ 0`, `message_ptr @ 8`, `message_len @ 16`,
/// `code @ 24`; user subclasses append their own slots after it. Every field is
/// treated as untrusted, and an unreadable message becomes empty rather than
/// failing the whole postmortem.
pub(crate) fn throwable_details(
    pointer: u64,
    classes: &std::collections::HashMap<u64, Class>,
) -> Option<ThrowableDetails> {
    let address = usize::try_from(pointer).ok()?;
    if address < MIN_VALID_ADDRESS {
        return None;
    }
    let class_id = unsafe { word(address)? };
    let class = classes
        .get(&class_id)
        .map(|class| class.name.clone())
        .unwrap_or_else(|| format!("class_id={class_id}"));
    let message_pointer = unsafe { word(address + 8)? };
    let message_length = unsafe { word(address + 16)? };
    let message = unsafe { string_bytes(message_pointer, message_length) }
        .unwrap_or_default();
    let code = unsafe { word(address + 24)? } as i64;
    Some(ThrowableDetails {
        class,
        message,
        code,
    })
}

/// Reads a boxed Mixed pointer into a borrowed value for a capture event.
pub(crate) fn value_from_mixed(pointer: u64) -> Option<Value> {
    unsafe { mixed_cell(pointer) }
}

/// Reads a boxed Mixed cell at `pointer` into a borrowed value.
unsafe fn mixed_cell(pointer: u64) -> Option<Value> {
    let address = usize::try_from(pointer).ok()?;
    if address < MIN_VALID_ADDRESS {
        return None;
    }
    Some(Value {
        tag: word(address)? as i64,
        lo: word(address + 8)?,
        hi: word(address + 16)?,
        by_ref: false,
    })
}

/// Returns the runtime tag implied by a declared PHP type when a reference
/// address is borrowed rather than a managed cell.
fn declared_tag(ty: &str) -> i64 {
    if ty == "mixed" || ty == "iterable" {
        TAG_MIXED
    } else if ty.starts_with("array") {
        TAG_ARRAY
    } else if ty.starts_with("string") {
        TAG_STRING
    } else if ty.starts_with("float") {
        TAG_FLOAT
    } else if ty.starts_with("bool") || ty == "false" {
        TAG_BOOL
    } else if ty.starts_with("int") {
        TAG_INT
    } else if ty.starts_with("resource") {
        TAG_RESOURCE
    } else if ty.starts_with("callable") {
        TAG_CALLABLE
    } else if ty.starts_with("ptr") || ty.starts_with("buffer") || ty.starts_with("packed") {
        TAG_POINTER
    } else if ty == "null" || ty == "void" || ty == "never" {
        TAG_NULL
    } else {
        TAG_OBJECT
    }
}

/// Reads a reference address and exposes its payload. An owned managed cell
/// carries its payload tag in the heap header; a borrowed address falls back to
/// the declared type so an array interior is still inspectable.
unsafe fn ref_cell(pointer: u64, fallback_tag: i64) -> Value {
    let address = usize::try_from(pointer).ok();
    let payload_tag = address
        .and_then(|address| {
            let header = word(address.checked_sub(8)?)?;
            (header & 0xff == HEAP_KIND_REF_CELL).then_some(((header >> 8) & 0x7f) as i64)
        })
        .unwrap_or(fallback_tag);
    let lo = address.and_then(|address| word(address)).unwrap_or(0);
    let hi = address.and_then(|address| word(address + 8)).unwrap_or(0);
    Value {
        tag: payload_tag,
        lo,
        hi,
        by_ref: true,
    }
}

/// Builds the debugger value for one named local slot.
///
/// `frame` is the selected activation's frame pointer. The metadata offset is
/// relative to that pointer (`address = frame - frame_offset`).
pub(crate) fn local_value(local: &Local, frame: usize) -> Value {
    let address = frame.wrapping_sub(local.frame_offset);
    let slot = unsafe { word(address) }.unwrap_or(0);
    match local.storage.as_str() {
        "ref_cell" => {
            if slot == 0 {
                Value::null_ref()
            } else {
                unsafe { ref_cell(slot, declared_tag(&local.php_type)) }
            }
        }
        "dynamic" => {
            let state = local
                .state_offset
                .and_then(|offset| unsafe { word(frame.wrapping_sub(offset)) })
                .unwrap_or(0);
            if state != 0 {
                if slot == 0 {
                    Value::null_ref()
                } else {
                    unsafe { ref_cell(slot, declared_tag(&local.php_type)) }
                }
            } else {
                raw_local_value(local, address)
            }
        }
        _ => raw_local_value(local, address),
    }
}

/// Decodes a non-ref-cell local from its declared PHP type and raw slot words.
fn raw_local_value(local: &Local, address: usize) -> Value {
    let low = unsafe { word(address) }.unwrap_or(0);
    let high = unsafe { word(address + 8) }.unwrap_or(0);
    typed_value(&local.php_type, low, high)
}

/// Builds a borrowed value from a declared PHP type and its two slot words.
fn typed_value(ty: &str, low: u64, high: u64) -> Value {
    if ty == "mixed" || ty == "iterable" {
        if low == 0 {
            return Value::null();
        }
        if ty == "iterable" {
            if let Some(kind) = unsafe { heap_kind(low) } {
                return match kind {
                    HEAP_KIND_ARRAY => Value::array(low),
                    HEAP_KIND_HASH => Value::hash(low),
                    4 | 6 => Value::object(low),
                    5 => unsafe { mixed_cell(low) }.unwrap_or_else(Value::null),
                    _ => Value::mixed(low),
                };
            }
        }
        return unsafe { mixed_cell(low) }.unwrap_or_else(Value::null);
    }
    if ty.starts_with("array") {
        if low == 0 {
            return Value::null();
        }
        return match unsafe { heap_kind(low) } {
            Some(HEAP_KIND_HASH) => Value::hash(low),
            Some(HEAP_KIND_ARRAY) => Value::array(low),
            Some(5) => unsafe { mixed_cell(low) }.unwrap_or_else(Value::null),
            _ => Value::array(low),
        };
    }
    if ty.starts_with("string") {
        return Value::string(low, high);
    }
    if ty == "int|null" || ty == "?int" {
        return Value {
            tag: high as i64,
            lo: low,
            hi: 0,
            by_ref: false,
        };
    }
    if ty.starts_with("int") {
        return Value::int(low as i64);
    }
    if ty.starts_with("float") {
        return Value {
            tag: TAG_FLOAT,
            lo: low,
            hi: 0,
            by_ref: false,
        };
    }
    if ty.starts_with("bool") || ty == "false" {
        return Value::bool(low != 0);
    }
    if ty.starts_with("resource") {
        return Value {
            tag: TAG_RESOURCE,
            lo: low,
            hi: 0,
            by_ref: false,
        };
    }
    if ty.starts_with("callable") {
        return Value {
            tag: TAG_CALLABLE,
            lo: low,
            hi: 0,
            by_ref: false,
        };
    }
    if ty.starts_with("ptr") || ty.starts_with("buffer") || ty.starts_with("packed") {
        return Value {
            tag: TAG_POINTER,
            lo: low,
            hi: 0,
            by_ref: false,
        };
    }
    if ty == "null" || ty == "void" || ty == "never" {
        return Value::null();
    }
    if low == 0 {
        Value::null()
    } else if unsafe { heap_kind(low) } == Some(5) {
        unsafe { mixed_cell(low) }.unwrap_or_else(Value::null)
    } else {
        Value::object(low)
    }
}

impl Value {
    /// Constructs a PHP null value.
    fn null() -> Self {
        Self {
            tag: TAG_NULL,
            lo: 0,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs a null value reached through an empty reference cell.
    fn null_ref() -> Self {
        Self {
            tag: TAG_NULL,
            lo: 0,
            hi: 0,
            by_ref: true,
        }
    }

    /// Constructs an uninitialized typed-property marker.
    fn uninitialized() -> Self {
        Self {
            tag: TAG_UNINITIALIZED,
            lo: 0,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs a PHP integer value.
    fn int(value: i64) -> Self {
        Self {
            tag: TAG_INT,
            lo: value as u64,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs a PHP boolean value.
    fn bool(value: bool) -> Self {
        Self {
            tag: TAG_BOOL,
            lo: value as u64,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs a PHP string value.
    fn string(pointer: u64, length: u64) -> Self {
        Self {
            tag: TAG_STRING,
            lo: pointer,
            hi: length,
            by_ref: false,
        }
    }

    /// Constructs an indexed-array value.
    fn array(pointer: u64) -> Self {
        Self {
            tag: TAG_ARRAY,
            lo: pointer,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs an associative-hash value.
    fn hash(pointer: u64) -> Self {
        Self {
            tag: TAG_HASH,
            lo: pointer,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs an object value.
    fn object(pointer: u64) -> Self {
        Self {
            tag: TAG_OBJECT,
            lo: pointer,
            hi: 0,
            by_ref: false,
        }
    }

    /// Constructs a nested Mixed-cell value.
    fn mixed(pointer: u64) -> Self {
        Self {
            tag: TAG_MIXED,
            lo: pointer,
            hi: 0,
            by_ref: false,
        }
    }
}

/// Escapes one PHP string for a single-line debugger display.
fn escape_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            ch if ch.is_control() => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

/// Describes one value without class metadata (objects show their class id).
#[cfg(test)]
pub(crate) fn describe(value: Value) -> Description {
    describe_with(value, &std::collections::HashMap::new())
}

/// Describes one value with the embedded class table for object names/properties.
pub(crate) fn describe_with(
    value: Value,
    classes: &std::collections::HashMap<u64, Class>,
) -> Description {
    describe_depth(value, 0, classes)
}

/// Describes one value at `depth`, unboxing nested Mixed cells.
fn describe_depth(
    value: Value,
    depth: usize,
    classes: &std::collections::HashMap<u64, Class>,
) -> Description {
    if depth > MAX_DEPTH {
        return Description {
            display: "<depth limit>".to_string(),
            type_name: "mixed".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        };
    }
    match value.tag {
        TAG_INT => Description {
            display: (value.lo as i64).to_string(),
            type_name: "int".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAG_FLOAT => Description {
            display: format!("{}", f64::from_bits(value.lo)),
            type_name: "float".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAG_BOOL => Description {
            display: if value.lo != 0 { "true" } else { "false" }.to_string(),
            type_name: "bool".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAG_STRING => {
            let text = unsafe { string_bytes(value.lo, value.hi) };
            match text {
                Some(text) => Description {
                    display: format!("string({}) \"{}\"", value.hi, escape_string(&text)),
                    type_name: "string".to_string(),
                    by_ref: value.by_ref,
                    has_children: false,
                },
                None => Description {
                    display: "string(<invalid>)".to_string(),
                    type_name: "string".to_string(),
                    by_ref: value.by_ref,
                    has_children: false,
                },
            }
        }
        TAG_ARRAY => describe_array(value, depth),
        TAG_HASH => describe_hash(value, depth),
        TAG_OBJECT => {
            let class_id = unsafe { word(value.lo as usize) }.unwrap_or(0);
            let class = classes.get(&class_id);
            let (name, property_count) = match class {
                Some(class) => (
                    class.name.as_str(),
                    object_child_count(value.lo, class),
                ),
                None => ("<unknown>", 0),
            };
            let display = if property_count == 0 {
                format!("object({name})")
            } else {
                format!("object({name}) ({property_count})")
            };
            // A runtime-only `__debugInfo()` is deliberately not invoked: doing so
            // would run user code and could terminate the stopped program. Surface
            // the caveat instead of silently presenting declared properties as the
            // method's result.
            let display = if class.is_some_and(|class| class.debug_info.kind == "runtime") {
                format!("{display} [__debugInfo() runtime]")
            } else {
                display
            };
            Description {
                display,
                type_name: if class.is_some() {
                    name.to_string()
                } else {
                    format!("class_id={class_id}")
                },
                by_ref: value.by_ref,
                has_children: property_count > 0,
            }
        }
        TAG_UNINITIALIZED => Description {
            display: "<uninitialized>".to_string(),
            type_name: "uninitialized".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAG_MIXED => {
            let inner = unsafe { mixed_cell(value.lo) };
            match inner {
                Some(inner) => {
                    let mut description = describe_depth(inner, depth + 1, classes);
                    description.by_ref |= value.by_ref;
                    description
                }
                None => Description {
                    display: "null".to_string(),
                    type_name: "null".to_string(),
                    by_ref: value.by_ref,
                    has_children: false,
                },
            }
        }
        TAG_NULL => Description {
            display: "null".to_string(),
            type_name: "null".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAG_RESOURCE => Description {
            display: format!("resource({})", value.lo),
            type_name: "resource".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAG_CALLABLE => Description {
            display: "callable".to_string(),
            type_name: "callable".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        TAGGED_SCALAR_ARRAY | TAGGED_SCALAR_PROPERTY => describe_depth(
            Value {
                tag: value.hi as i64,
                lo: value.lo,
                hi: 0,
                by_ref: value.by_ref,
            },
            depth + 1,
            classes,
        ),
        TAG_POINTER => Description {
            display: if value.lo == 0 {
                "null".to_string()
            } else {
                format!("0x{:x}", value.lo)
            },
            type_name: "ptr".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
        other => Description {
            display: format!("<unknown tag {other}>"),
            type_name: "unknown".to_string(),
            by_ref: value.by_ref,
            has_children: false,
        },
    }
}

/// Describes an indexed array.
fn describe_array(value: Value, depth: usize) -> Description {
    if let Some(HEAP_KIND_HASH) = unsafe { heap_kind(value.lo) } {
        return describe_hash(
            Value {
                tag: TAG_HASH,
                lo: value.lo,
                hi: 0,
                by_ref: value.by_ref,
            },
            depth,
        );
    }
    let length = unsafe { word(value.lo as usize) }.unwrap_or(0);
    let length = length.min(MAX_ARRAY_ELEMENTS as u64);
    Description {
        display: format!("array({length})"),
        type_name: "array".to_string(),
        by_ref: value.by_ref,
        has_children: length > 0,
    }
}

/// Describes an associative hash.
fn describe_hash(value: Value, _depth: usize) -> Description {
    let count = unsafe { word(value.lo as usize) }.unwrap_or(0);
    let count = count.min(MAX_HASH_ENTRIES as u64);
    Description {
        display: format!("array({count})"),
        type_name: "array".to_string(),
        by_ref: value.by_ref,
        has_children: count > 0,
    }
}

/// Returns the children of one value without class metadata.
#[cfg(test)]
pub(crate) fn children(value: Value) -> Vec<(String, Value)> {
    children_with_annotated(value, &std::collections::HashMap::new())
        .into_iter()
        .map(|child| (child.name, child.value))
        .collect()
}

/// Enumerates an indexed array's live elements.
unsafe fn indexed_children(
    pointer: u64,
    depth: usize,
    classes: &std::collections::HashMap<u64, Class>,
) -> Vec<(String, Value)> {
    let address = match usize::try_from(pointer) {
        Ok(address) if address >= MIN_VALID_ADDRESS => address,
        _ => return Vec::new(),
    };
    // A statically indexed array can hold hash storage after a runtime promotion.
    if heap_kind(pointer) == Some(HEAP_KIND_HASH) {
        return hash_children(pointer, depth, classes, HashKeyStyle::Container);
    }
    let length = word(address).unwrap_or(0).min(MAX_ARRAY_ELEMENTS as u64) as usize;
    let value_type = array_value_type(pointer).unwrap_or(0);
    let stride = if value_type == TAG_STRING || value_type == TAGGED_SCALAR_ARRAY {
        16
    } else {
        8
    };
    let mut children = Vec::with_capacity(length.min(32));
    for index in 0..length {
        let slot = address + 24 + index * stride;
        let value = match value_type {
            TAG_STRING => Value::string(word(slot).unwrap_or(0), word(slot + 8).unwrap_or(0)),
            TAGGED_SCALAR_ARRAY => Value {
                tag: word(slot + 8).unwrap_or(TAG_NULL as u64) as i64,
                lo: word(slot).unwrap_or(0),
                hi: 0,
                by_ref: false,
            },
            TAG_MIXED => match word(slot) {
                Some(cell) if cell != 0 => mixed_cell(cell).unwrap_or_else(Value::null),
                _ => Value::null(),
            },
            TAG_POINTER => Value {
                tag: TAG_POINTER,
                lo: word(slot).unwrap_or(0),
                hi: 0,
                by_ref: false,
            },
            tag => Value {
                tag: tag as i64,
                lo: word(slot).unwrap_or(0),
                hi: 0,
                by_ref: false,
            },
        };
        children.push((format!("[{index}]"), value));
        if children.len() >= MAX_ARRAY_ELEMENTS {
            break;
        }
    }
    children
}

/// How a hash key is named in a debugger child list.
#[derive(Clone, Copy, PartialEq)]
enum HashKeyStyle {
    /// Array key: `[0]` or `["name"]`.
    Container,
    /// Object property: plain `name`.
    Object,
}

/// Enumerates an associative hash in insertion order.
unsafe fn hash_children(
    pointer: u64,
    _depth: usize,
    _classes: &std::collections::HashMap<u64, Class>,
    style: HashKeyStyle,
) -> Vec<(String, Value)> {
    let address = match usize::try_from(pointer) {
        Ok(address) if address >= MIN_VALID_ADDRESS => address,
        _ => return Vec::new(),
    };
    let count = word(address).unwrap_or(0).min(MAX_HASH_ENTRIES as u64) as usize;
    let capacity = match word(address + 8) {
        Some(capacity) if capacity > 0 && capacity <= MAX_HASH_ENTRIES as u64 => capacity as usize,
        _ => return Vec::new(),
    };
    let head = match word(address + 24) {
        Some(head) if (head as i64) >= 0 && (head as usize) < capacity => head as usize,
        _ => return Vec::new(),
    };
    let entries = match word(address + 40) {
        Some(entries) if entries != 0 => entries as usize,
        _ => return Vec::new(),
    };
    let mut children = Vec::with_capacity(count.min(32));
    let mut slot = head;
    let mut visited = std::collections::HashSet::new();
    while children.len() < count && visited.insert(slot) {
        if slot >= capacity {
            break;
        }
        let entry = entries + slot * 64;
        if word(entry).unwrap_or(0) != 1 {
            break;
        }
        let key_pointer = word(entry + 8).unwrap_or(0);
        let key_length = word(entry + 16).unwrap_or(0) as i64;
        let key = if key_length == -1 {
            match style {
                HashKeyStyle::Container => format!("[{}]", key_pointer as i64),
                HashKeyStyle::Object => (key_pointer as i64).to_string(),
            }
        } else {
            let text = string_bytes(key_pointer, key_length.max(0) as u64)
                .unwrap_or_else(|| "<invalid key>".to_string());
            match style {
                HashKeyStyle::Container => format!("[\"{}\"]", escape_string(&text)),
                HashKeyStyle::Object => escape_string(&text),
            }
        };
        let value_tag = word(entry + 40).unwrap_or(TAG_NULL as u64) as i64;
        let value_lo = word(entry + 24).unwrap_or(0);
        let value_hi = word(entry + 32).unwrap_or(0);
        let value = decode_hash_value(value_tag, value_lo, value_hi);
        children.push((key, value));
        let next = word(entry + 56).unwrap_or(u64::MAX) as i64;
        if next < 0 {
            break;
        }
        slot = next as usize;
    }
    children
}

/// Decodes one hash entry's tag/payload words into a borrowed value.
fn decode_hash_value(tag: i64, lo: u64, hi: u64) -> Value {
    match tag {
        TAG_MIXED => unsafe { mixed_cell(lo) }.unwrap_or_else(Value::null),
        TAGGED_SCALAR_PROPERTY => Value {
            tag: hi as i64,
            lo,
            hi: 0,
            by_ref: false,
        },
        TAG_NULL => Value::null(),
        tag => Value {
            tag,
            lo,
            hi,
            by_ref: false,
        },
    }
}

/// Reads one string-keyed dynamic property from an object's per-object hash.
///
/// `dynamic_offset` comes from the class metadata. The object's class id is
/// revalidated before the hash walk; a missing key (unset property), a freed or
/// reallocated object, or a stale layout offset returns `None`. Keys are compared
/// as raw bytes so non-UTF-8 property names still match exactly.
pub(crate) fn object_dynamic_property_value(
    pointer: u64,
    class_id: u64,
    dynamic_offset: usize,
    key: &[u8],
) -> Option<Value> {
    if object_class_id(pointer)? != class_id {
        return None;
    }
    let address = usize::try_from(pointer).ok()?;
    let hash = unsafe { word(address.checked_add(dynamic_offset)?) }?;
    if hash < MIN_VALID_ADDRESS as u64 {
        return None;
    }
    let hash = usize::try_from(hash).ok()?;
    let count = unsafe { word(hash) }?.min(MAX_HASH_ENTRIES as u64) as usize;
    let capacity = match unsafe { word(hash + 8) } {
        Some(capacity) if capacity > 0 && capacity <= MAX_HASH_ENTRIES as u64 => capacity as usize,
        _ => return None,
    };
    let head = match unsafe { word(hash + 24) } {
        Some(head) if (head as i64) >= 0 && (head as usize) < capacity => head as usize,
        _ => return None,
    };
    let entries = match unsafe { word(hash + 40) } {
        Some(entries) if entries >= MIN_VALID_ADDRESS as u64 => entries as usize,
        _ => return None,
    };
    let mut slot = head;
    let mut visited = std::collections::HashSet::new();
    let mut seen = 0usize;
    while seen < count && visited.insert(slot) {
        if slot >= capacity {
            break;
        }
        let entry = entries.checked_add(slot.checked_mul(64)?)?;
        if unsafe { word(entry) }? != 1 {
            break;
        }
        let key_pointer = unsafe { word(entry + 8) }?;
        let key_length = unsafe { word(entry + 16) }? as i64;
        if key_length >= 0 {
            let key_length = usize::try_from(key_length).ok()?;
            if key_length <= MAX_STRING_BYTES && key_pointer >= MIN_VALID_ADDRESS as u64 {
                let bytes = unsafe {
                    std::slice::from_raw_parts(key_pointer as usize as *const u8, key_length)
                };
                if bytes == key {
                    let tag = unsafe { word(entry + 40) }? as i64;
                    let lo = unsafe { word(entry + 24) }?;
                    let hi = unsafe { word(entry + 32) }?;
                    return Some(decode_hash_value(tag, lo, hi));
                }
            }
        }
        seen += 1;
        let next = unsafe { word(entry + 56) }? as i64;
        if next < 0 {
            break;
        }
        slot = next as usize;
    }
    None
}

/// Returns an object's class id when `pointer` is a live managed object.
///
/// The probe checks the uniform heap kind before reading the class word, so a
/// freed/reused integer or array payload is rejected instead of being decoded as
/// a class id. It is still a heuristic: allocation reuse that happens to stamp
/// object kind and the same class id cannot be distinguished without a runtime
/// lifetime registry.
pub(crate) fn object_class_id(pointer: u64) -> Option<u64> {
    let address = usize::try_from(pointer).ok()?;
    if address < MIN_VALID_ADDRESS {
        return None;
    }
    if unsafe { heap_kind(pointer) } != Some(HEAP_KIND_OBJECT) {
        return None;
    }
    unsafe { word(address) }
}

/// Reads one declared property of a live object, validating the class id.
///
/// Returns `None` when the object no longer exists, was reallocated as another
/// kind/class, or the property offset is outside the two readable words. The
/// property slot is decoded with the same rules as an object child.
pub(crate) fn object_property_value(
    pointer: u64,
    class_id: u64,
    property: &Property,
) -> Option<Value> {
    if object_class_id(pointer)? != class_id {
        return None;
    }
    let address = usize::try_from(pointer).ok()?;
    let slot = address.checked_add(property.offset)?;
    let low = unsafe { word(slot) }?;
    let high = unsafe { word(slot + 8) }?;
    Some(property_value(property, low, high))
}

/// Decodes one property slot from its declared type and its two stored words.
fn property_value(property: &Property, low: u64, high: u64) -> Value {
    if property.declared && high == UNINITIALIZED_TYPED_PROPERTY_SENTINEL {
        Value::uninitialized()
    } else if property.by_ref {
        if low == 0 {
            Value::null_ref()
        } else {
            unsafe { ref_cell(low, declared_tag(&property.php_type)) }
        }
    } else {
        typed_value(&property.php_type, low, high)
    }
}

/// Counts the children an object exposes without materializing their values.
fn object_child_count(pointer: u64, class: &Class) -> usize {
    if class.debug_info.kind == "projection" {
        return class
            .debug_info
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .property
                    .as_ref()
                    .is_some_and(|name| {
                        class
                            .properties
                            .iter()
                            .any(|property| &property.name == name)
                    })
                    || entry.value.is_some()
            })
            .count()
            .min(MAX_ARRAY_ELEMENTS);
    }
    let mut count = class.properties.len();
    if let Some(offset) = class.dynamic_offset {
        if let Ok(address) = usize::try_from(pointer) {
            if let Some(hash) = address
                .checked_add(offset)
                .and_then(|slot| unsafe { word(slot) })
                .filter(|hash| *hash >= MIN_VALID_ADDRESS as u64)
            {
                if let Some(length) = unsafe { word(hash as usize) } {
                    count += length.min(MAX_HASH_ENTRIES as u64) as usize;
                }
            }
        }
    }
    count.min(MAX_ARRAY_ELEMENTS)
}

/// Enumerates every child of a value with object-property hook caveats.
///
/// Arrays, hashes and dynamic properties carry no hook note. A nested object
/// reached through a Mixed box is expanded with the same annotations.
pub(crate) fn children_with_annotated(
    value: Value,
    classes: &std::collections::HashMap<u64, Class>,
) -> Vec<ObjectChild> {
    children_with_annotated_depth(value, 0, classes)
}

/// Unboxes and dispatches one value's children, preserving object hook notes.
fn children_with_annotated_depth(
    value: Value,
    depth: usize,
    classes: &std::collections::HashMap<u64, Class>,
) -> Vec<ObjectChild> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    match value.tag {
        TAG_MIXED => unsafe { mixed_cell(value.lo) }
            .map(|inner| children_with_annotated_depth(inner, depth + 1, classes))
            .unwrap_or_default(),
        TAG_ARRAY => unsafe { indexed_children(value.lo, depth, classes) }
            .into_iter()
            .map(ObjectChild::plain)
            .collect(),
        TAG_HASH => unsafe { hash_children(value.lo, depth, classes, HashKeyStyle::Container) }
            .into_iter()
            .map(ObjectChild::plain)
            .collect(),
        TAG_OBJECT => unsafe { object_children_annotated(value.lo, classes) },
        TAGGED_SCALAR_ARRAY | TAGGED_SCALAR_PROPERTY => children_with_annotated_depth(
            Value {
                tag: value.hi as i64,
                lo: value.lo,
                hi: 0,
                by_ref: value.by_ref,
            },
            depth + 1,
            classes,
        ),
        _ => Vec::new(),
    }
}

/// Builds the hook caveat for one object property, when it reads differently
/// from PHP code or cannot be read at all.
fn property_hook_note(class: &Class, property: &Property) -> Option<PropertyHookNote> {
    let hooks = class.property_hooks.get(&property.name).copied()?;
    if !hooks.any() || (!hooks.is_virtual && !hooks.requires_get()) {
        return None;
    }
    Some(PropertyHookNote {
        hooks,
        php_type: property.php_type.clone(),
    })
}

/// Enumerates one object's properties: a static `__debugInfo()` projection when
/// present, otherwise declared properties followed by dynamic properties.
///
/// A virtual property has no backing slot, so its child value is a placeholder
/// and the hook note carries the marker. A backed getter's child value is the
/// raw backing slot; the note says the getter was not executed.
unsafe fn object_children_annotated(
    pointer: u64,
    classes: &std::collections::HashMap<u64, Class>,
) -> Vec<ObjectChild> {
    let address = match usize::try_from(pointer) {
        Ok(address) if address >= MIN_VALID_ADDRESS => address,
        _ => return Vec::new(),
    };
    let class_id = word(address).unwrap_or(0);
    let Some(class) = classes.get(&class_id) else {
        return Vec::new();
    };
    if class.debug_info.kind == "projection" {
        return class
            .debug_info
            .entries
            .iter()
            .filter_map(|entry| {
                // A property entry reads the live slot; a literal entry was
                // folded at compile time and is display-only. The method itself
                // is never called here.
                let Some(property_name) = &entry.property else {
                    return Some(ObjectChild {
                        name: entry.key.clone(),
                        value: Value::null(),
                        static_text: Some(entry.value.clone()?),
                        static_type: entry.php_type.clone(),
                        hook_note: None,
                    });
                };
                let property = class
                    .properties
                    .iter()
                    .find(|property| &property.name == property_name)?;
                let hook_note = property_hook_note(class, property);
                let value = if hook_note
                    .as_ref()
                    .is_some_and(PropertyHookNote::is_virtual)
                {
                    Value::null()
                } else {
                    let slot = address.checked_add(property.offset)?;
                    let low = word(slot)?;
                    let high = word(slot + 8)?;
                    property_value(property, low, high)
                };
                Some(ObjectChild {
                    name: entry.key.clone(),
                    value,
                    static_text: None,
                    static_type: None,
                    hook_note,
                })
            })
            .collect();
    }
    let mut children = Vec::with_capacity(class.properties.len().min(32));
    for property in &class.properties {
        let hook_note = property_hook_note(class, property);
        let value = if hook_note
            .as_ref()
            .is_some_and(PropertyHookNote::is_virtual)
        {
            Value::null()
        } else {
            let slot = address + property.offset;
            let low = word(slot).unwrap_or(0);
            let high = word(slot + 8).unwrap_or(0);
            property_value(property, low, high)
        };
        let name = if property.visibility == "private" || property.visibility == "protected" {
            format!("{} ({})", property.name, property.visibility)
        } else {
            property.name.clone()
        };
        children.push(ObjectChild {
            name,
            value,
            static_text: None,
            static_type: None,
            hook_note,
        });
    }
    // Dynamic properties live in a per-object hash at the layout's reserved slot.
    // The hash is written through `__rt_hash_set` with boxed Mixed values, so the
    // standard hash walker decodes it; object keys are plain names, not `["key"]`.
    if let Some(offset) = class.dynamic_offset {
        if let Some(hash) = address
            .checked_add(offset)
            .and_then(|slot| word(slot))
            .filter(|hash| *hash >= MIN_VALID_ADDRESS as u64)
        {
            children.extend(
                hash_children(hash, 0, classes, HashKeyStyle::Object)
                    .into_iter()
                    .map(ObjectChild::plain),
            );
        }
    }
    children
}

/// Renders a value including bounded nested array/object children.
///
/// Retained as the test-side unbounded marker helper for the debugger value
/// tests; production call capture uses `render_capture` for redaction and
/// overflow accounting.
#[cfg(test)]
pub(crate) fn render_nested(
    value: Value,
    classes: &std::collections::HashMap<u64, Class>,
    depth: usize,
) -> String {
    let description = describe_with(value, classes);
    let base = render(&description);
    if !description.has_children || depth >= 3 {
        return base;
    }
    let children = children_with_annotated(value, classes);
    if children.is_empty() {
        return base;
    }
    let mut out = format!("{base} {{");
    for (index, child) in children.iter().take(16).enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&child.name);
        out.push_str(" => ");
        if let Some(text) = &child.static_text {
            out.push_str(text);
            continue;
        }
        if let Some(note) = &child.hook_note {
            if note.is_virtual() {
                out.push_str(&note.virtual_display());
                continue;
            }
        }
        out.push_str(&render_nested(child.value, classes, depth + 1));
        if let Some(note) = &child.hook_note {
            out.push_str(note.backing_suffix());
        }
    }
    out.push('}');
    out
}

/// Renders one value for call capture with bounded strings and containers.
///
/// `nested` selects the `full` mode's child expansion; `scalars` keeps container
/// summaries while still bounding string length. Explicit markers are embedded
/// in the text so the text report, JSONL value and HTML panel agree.
pub(crate) fn render_capture(
    value: Value,
    classes: &std::collections::HashMap<u64, Class>,
    nested: bool,
) -> CaptureRender {
    render_capture_depth(value, classes, 0, nested)
}

/// Renders one value at `depth` under the capture limits.
fn render_capture_depth(
    value: Value,
    classes: &std::collections::HashMap<u64, Class>,
    depth: usize,
    nested: bool,
) -> CaptureRender {
    match value.tag {
        TAG_MIXED => value_from_mixed(value.lo)
            .map(|inner| render_capture_depth(inner, classes, depth, nested))
            .unwrap_or_else(|| CaptureRender::plain("null".to_string())),
        TAGGED_SCALAR_ARRAY | TAGGED_SCALAR_PROPERTY => render_capture_depth(
            Value {
                tag: value.hi as i64,
                lo: value.lo,
                hi: 0,
                by_ref: value.by_ref,
            },
            classes,
            depth,
            nested,
        ),
        TAG_STRING => {
            let full_length = value.hi as usize;
            if full_length <= CAPTURE_MAX_STRING_BYTES {
                return CaptureRender::plain(render(&describe_with(value, classes)));
            }
            let omitted = full_length - CAPTURE_MAX_STRING_BYTES;
            let prefix = unsafe {
                string_prefix(value.lo, value.hi, CAPTURE_MAX_STRING_BYTES)
            };
            let text = match prefix {
                Some(prefix) => {
                    let reference = if value.by_ref { "&" } else { "" };
                    format!(
                        "{reference}string({full_length}) \"{}…<{omitted} bytes omitted>\"",
                        escape_string(&prefix)
                    )
                }
                None => format!("string({full_length}) \"<invalid>…<{omitted} bytes omitted>\""),
            };
            CaptureRender {
                text,
                truncated: true,
                string_bytes_omitted: omitted,
                ..CaptureRender::default()
            }
        }
        _ => {
            let description = describe_with(value, classes);
            let base = render(&description);
            if !nested || !description.has_children {
                return CaptureRender::plain(base);
            }
            if depth >= CAPTURE_MAX_DEPTH {
                return CaptureRender {
                    text: format!("{base} …<depth limit>"),
                    truncated: true,
                    depth_truncated: true,
                    ..CaptureRender::default()
                };
            }
            let children = children_with_annotated(value, classes);
            if children.is_empty() {
                return CaptureRender::plain(base);
            }
            let shown = children.len().min(CAPTURE_MAX_CHILDREN);
            let omitted = children.len().saturating_sub(shown);
            let mut result = CaptureRender {
                text: format!("{base} {{"),
                truncated: omitted > 0,
                children_omitted: omitted,
                ..CaptureRender::default()
            };
            for (index, child) in children.iter().take(shown).enumerate() {
                if index > 0 {
                    result.text.push_str(", ");
                }
                result.text.push_str(&child.name);
                result.text.push_str(" => ");
                if let Some(text) = &child.static_text {
                    result.text.push_str(text);
                    continue;
                }
                if let Some(note) = &child.hook_note {
                    if note.is_virtual() {
                        result.text.push_str(&note.virtual_display());
                        continue;
                    }
                }
                let child_render = render_capture_depth(child.value, classes, depth + 1, nested);
                result.text.push_str(&child_render.text);
                if let Some(note) = &child.hook_note {
                    result.text.push_str(note.backing_suffix());
                }
                result.add_child_overflow(&child_render);
            }
            if omitted > 0 {
                result
                    .text
                    .push_str(&format!(", … +{omitted} more"));
            }
            result.text.push('}');
            result
        }
    }
}

/// Reads at most `limit` bytes of a PHP string for a bounded display.
unsafe fn string_prefix(pointer: u64, length: u64, limit: usize) -> Option<String> {
    if pointer == 0 {
        return None;
    }
    let full = usize::try_from(length).ok()?;
    let take = full.min(limit);
    if take == 0 {
        return Some(String::new());
    }
    let bytes = std::slice::from_raw_parts(pointer as usize as *const u8, take);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Renders a description the way the terminal session and DAP clients show it.
pub(crate) fn render(description: &Description) -> String {
    let body = match description.type_name.as_str() {
        "int" | "float" | "bool" => {
            format!("{}({})", description.type_name, description.display)
        }
        _ => description.display.clone(),
    };
    if description.by_ref {
        format!("&{body}")
    } else {
        body
    }
}


/// Unboxes nested Mixed cells and inline tagged containers for read-only access.
pub(crate) fn unbox_value(value: Value) -> Option<Value> {
    match value.tag {
        TAG_MIXED => unsafe { mixed_cell(value.lo) },
        TAGGED_SCALAR_ARRAY | TAGGED_SCALAR_PROPERTY => unbox_value(Value {
            tag: value.hi as i64,
            lo: value.lo,
            hi: 0,
            by_ref: value.by_ref,
        }),
        _ => Some(value),
    }
}

/// Reads one array element or hash entry by an integer/string key.
///
/// The lookup is borrowed: it never copies the container and never invokes PHP
/// code. Object keys are rejected because `ArrayAccess` would require executing
/// user code.
pub(crate) fn element_at(value: Value, key: &Scalar) -> Option<Value> {
    let value = unbox_value(value)?;
    match value.tag {
        TAG_ARRAY => {
            if unsafe { heap_kind(value.lo) } == Some(HEAP_KIND_HASH) {
                return hash_element_at(value.lo, key);
            }
            let Scalar::Int(index) = key else {
                return None;
            };
            if *index < 0 || *index as usize >= MAX_ARRAY_ELEMENTS {
                return None;
            }
            let address = usize::try_from(value.lo).ok().filter(|addr| *addr >= MIN_VALID_ADDRESS)?;
            let length = unsafe { word(address) }?.min(MAX_ARRAY_ELEMENTS as u64) as usize;
            let index = *index as usize;
            if index >= length {
                return None;
            }
            let value_type = unsafe { array_value_type(value.lo) }.unwrap_or(0);
            let stride = if value_type == TAG_STRING || value_type == TAGGED_SCALAR_ARRAY {
                16
            } else {
                8
            };
            let slot = address
                .checked_add(24)?
                .checked_add(index.checked_mul(stride)?)?;
            Some(match value_type {
                TAG_STRING => Value::string(
                    unsafe { word(slot) }.unwrap_or(0),
                    unsafe { word(slot + 8) }.unwrap_or(0),
                ),
                TAGGED_SCALAR_ARRAY => Value {
                    tag: unsafe { word(slot + 8) }.unwrap_or(TAG_NULL as u64) as i64,
                    lo: unsafe { word(slot) }.unwrap_or(0),
                    hi: 0,
                    by_ref: false,
                },
                TAG_MIXED => match unsafe { word(slot) } {
                    Some(cell) if cell != 0 => unsafe { mixed_cell(cell) }.unwrap_or_else(Value::null),
                    _ => Value::null(),
                },
                TAG_POINTER => Value {
                    tag: TAG_POINTER,
                    lo: unsafe { word(slot) }.unwrap_or(0),
                    hi: 0,
                    by_ref: false,
                },
                tag => Value {
                    tag: tag as i64,
                    lo: unsafe { word(slot) }.unwrap_or(0),
                    hi: 0,
                    by_ref: false,
                },
            })
        }
        TAG_HASH => hash_element_at(value.lo, key),
        TAG_STRING => {
            let Scalar::Int(index) = key else {
                return None;
            };
            let length = value.hi as i64;
            let index = if *index < 0 { length + *index } else { *index };
            if index < 0 || index >= length {
                return None;
            }
            Some(Value::string(value.lo + index as u64, 1))
        }
        _ => None,
    }
}

/// Looks up one hash entry by key without materializing its children.
fn hash_element_at(pointer: u64, key: &Scalar) -> Option<Value> {
    let address = usize::try_from(pointer)
        .ok()
        .filter(|addr| *addr >= MIN_VALID_ADDRESS)?;
    let count = unsafe { word(address) }? as usize;
    let capacity = unsafe { word(address + 8) }?;
    if capacity == 0 || capacity > MAX_HASH_ENTRIES as u64 {
        return None;
    }
    let capacity = capacity as usize;
    let head = unsafe { word(address + 24) }? as usize;
    let entries = unsafe { word(address + 40) }?;
    if entries == 0 {
        return None;
    }
    let mut slot = head;
    let mut visited = std::collections::HashSet::new();
    let mut seen = 0usize;
    while seen < count && slot < capacity && visited.insert(slot) {
        let entry = usize::try_from(entries)
            .ok()?
            .checked_add(slot.checked_mul(64)?)?;
        if unsafe { word(entry) }? != 1 {
            break;
        }
        let key_pointer = unsafe { word(entry + 8) }?;
        let key_length = unsafe { word(entry + 16) }? as i64;
        let matches = match key {
            Scalar::Int(value) => key_length == -1 && key_pointer as i64 == *value,
            Scalar::String(value) => {
                key_length >= 0
                    && unsafe { string_bytes(key_pointer, key_length as u64) }.as_deref()
                        == Some(value.as_str())
            }
            _ => false,
        };
        if matches {
            let tag = unsafe { word(entry + 40) }? as i64;
            let lo = unsafe { word(entry + 24) }?;
            let hi = unsafe { word(entry + 32) }?;
            return Some(decode_hash_value(tag, lo, hi));
        }
        let next = unsafe { word(entry + 56) }? as i64;
        if next < 0 {
            break;
        }
        slot = next as usize;
        seen += 1;
    }
    None
}

/// Returns the exact array/hash element count for `count()`.
pub(crate) fn count(value: Value) -> Result<usize, String> {
    let value = unbox_value(value).ok_or_else(|| "count() expects an array".to_string())?;
    match value.tag {
        TAG_ARRAY => {
            if unsafe { heap_kind(value.lo) } == Some(HEAP_KIND_HASH) {
                return hash_count(value.lo);
            }
            let address = usize::try_from(value.lo)
                .ok()
                .filter(|addr| *addr >= MIN_VALID_ADDRESS)
                .ok_or_else(|| "count() received an invalid array".to_string())?;
            let length = unsafe { word(address) }
                .ok_or_else(|| "count() received an invalid array".to_string())?;
            usize::try_from(length).map_err(|_| "array length is out of range".to_string())
        }
        TAG_HASH => hash_count(value.lo),
        _ => Err("count() expects an array".to_string()),
    }
}

/// Returns one hash's element count after validating its pointer.
fn hash_count(pointer: u64) -> Result<usize, String> {
    let address = usize::try_from(pointer)
        .ok()
        .filter(|addr| *addr >= MIN_VALID_ADDRESS)
        .ok_or_else(|| "count() received an invalid array".to_string())?;
    let count = unsafe { word(address) }
        .ok_or_else(|| "count() received an invalid array".to_string())?;
    usize::try_from(count).map_err(|_| "array length is out of range".to_string())
}

/// Returns a string's byte length for `strlen()`.
pub(crate) fn strlen(value: Value) -> Result<usize, String> {
    let value = unbox_value(value).ok_or_else(|| "strlen() expects a string".to_string())?;
    if value.tag != TAG_STRING {
        return Err("strlen() expects a string".to_string());
    }
    usize::try_from(value.hi).map_err(|_| "string length is out of range".to_string())
}

/// Returns PHP's `gettype()` spelling for one borrowed value.
pub(crate) fn gettype(value: Value) -> &'static str {
    let Some(value) = unbox_value(value) else {
        return "unknown type";
    };
    match value.tag {
        TAG_INT => "integer",
        TAG_FLOAT => "double",
        TAG_BOOL => "boolean",
        TAG_NULL => "NULL",
        TAG_STRING => "string",
        TAG_ARRAY | TAG_HASH => "array",
        TAG_OBJECT => "object",
        TAG_RESOURCE => "resource",
        TAG_CALLABLE => "callable",
        TAG_POINTER => "integer",
        TAG_UNINITIALIZED => "uninitialized",
        _ => "unknown type",
    }
}

/// Builds the display description for one computed scalar evaluation result.
pub(crate) fn describe_scalar(scalar: &Scalar) -> Description {
    match scalar {
        Scalar::Int(value) => Description {
            display: value.to_string(),
            type_name: "int".to_string(),
            by_ref: false,
            has_children: false,
        },
        Scalar::Float(value) => Description {
            display: format!("{value}"),
            type_name: "float".to_string(),
            by_ref: false,
            has_children: false,
        },
        Scalar::Bool(value) => Description {
            display: if *value { "true" } else { "false" }.to_string(),
            type_name: "bool".to_string(),
            by_ref: false,
            has_children: false,
        },
        Scalar::Null => Description {
            display: "null".to_string(),
            type_name: "null".to_string(),
            by_ref: false,
            has_children: false,
        },
        Scalar::String(value) => Description {
            display: format!("string({}) \"{}\"", value.len(), escape_string(value)),
            type_name: "string".to_string(),
            by_ref: false,
            has_children: false,
        },
        Scalar::Other => Description {
            display: "<non-scalar>".to_string(),
            type_name: "mixed".to_string(),
            by_ref: false,
            has_children: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies scalar tags render with PHP spellings and no pointer leaks.
    #[test]
    fn describes_scalars() {
        assert_eq!(render(&describe(Value::int(42))), "int(42)");
        assert_eq!(render(&describe(Value::bool(true))), "bool(true)");
        assert_eq!(render(&describe(Value::null())), "null");
        let float = Value {
            tag: TAG_FLOAT,
            lo: 1.5f64.to_bits(),
            hi: 0,
            by_ref: false,
        };
        assert_eq!(render(&describe(float)), "float(1.5)");
    }

    /// Verifies a borrowed string renders with its length and escapes control bytes.
    #[test]
    fn describes_strings() {
        let bytes = b"hello\nworld";
        let value = Value::string(bytes.as_ptr() as u64, bytes.len() as u64);
        assert_eq!(
            render(&describe(value)),
            "string(11) \"hello\\nworld\""
        );
    }

    /// Verifies an indexed array header and its int slots are read without
    /// touching PHP-owned memory beyond the borrowed snapshot.
    #[test]
    fn describes_indexed_array_children() {
        let mut storage = vec![0u64; 1 + 3 + 2];
        storage[0] = HEAP_KIND_ARRAY; // heap kind before the array header
        storage[1] = 2; // length
        storage[2] = 2; // capacity
        storage[3] = 8; // elem_size
        storage[4] = 10;
        storage[5] = 20;
        let header = unsafe { storage.as_mut_ptr().add(1) };
        let value = Value::array(header as u64);
        assert_eq!(render(&describe(value)), "array(2)");
        let children = children(value);
        assert_eq!(children.len(), 2);
        assert_eq!(render(&describe(children[0].1)), "int(10)");
        assert_eq!(render(&describe(children[1].1)), "int(20)");
    }

    /// Verifies an object is named from the class table and its declared
    /// properties are read from the 16-byte slots, including a string and an
    /// uninitialized typed property.
    #[test]
    fn describes_object_properties() {
        let text = b"origin";
        let mut object = vec![0u64; 9];
        object[0] = 7; // class id
        object[1] = 2; // x payload
        object[3] = 3; // y payload
        object[5] = text.as_ptr() as u64; // label pointer
        object[6] = text.len() as u64; // label length
        object[8] = UNINITIALIZED_TYPED_PROPERTY_SENTINEL; // uninitialized typed property high word
        let classes = std::collections::HashMap::from([(
            7,
            Class {
                id: 7,
                name: "Point".to_string(),
                parent: String::new(),
                is_abstract: false,
                properties: vec![
                    Property {
                        name: "x".to_string(),
                        offset: 8,
                        php_type: "int".to_string(),
                        visibility: "public".to_string(),
                        by_ref: false,
                        declared: true,
                    },
                    Property {
                        name: "y".to_string(),
                        offset: 24,
                        php_type: "int".to_string(),
                        visibility: "public".to_string(),
                        by_ref: false,
                        declared: true,
                    },
                    Property {
                        name: "label".to_string(),
                        offset: 40,
                        php_type: "string".to_string(),
                        visibility: "public".to_string(),
                        by_ref: false,
                        declared: true,
                    },
                    Property {
                        name: "uninit".to_string(),
                        offset: 56,
                        php_type: "int".to_string(),
                        visibility: "private".to_string(),
                        by_ref: false,
                        declared: true,
                    },
                ],
                dynamic_offset: None,
                debug_info: DebugInfo::default(),
                property_hooks: std::collections::HashMap::new(),
            },
        )]);
        let value = Value::object(object.as_ptr() as u64);
        let description = describe_with(value, &classes);
        assert_eq!(description.type_name, "Point");
        assert!(description.has_children);
        assert!(
            description.display.contains("object(Point)"),
            "{description:?}"
        );
        let children = children_with_annotated(value, &classes);
        let names: Vec<&str> = children.iter().map(|child| child.name.as_str()).collect();
        assert_eq!(names, ["x", "y", "label", "uninit (private)"]);
        assert_eq!(render(&describe_with(children[0].value, &classes)), "int(2)");
        assert_eq!(render(&describe_with(children[1].value, &classes)), "int(3)");
        assert_eq!(
            render(&describe_with(children[2].value, &classes)),
            "string(6) \"origin\""
        );
        assert_eq!(
            render(&describe_with(children[3].value, &classes)),
            "<uninitialized>"
        );
    }

    /// Verifies dynamic properties are read from the per-object hash after the
    /// declared slots, and that the object description counts both.
    #[test]
    fn describes_dynamic_object_properties() {
        let key = b"extra";
        let mut entries = vec![0u64; 8];
        entries[0] = 1; // occupied
        entries[1] = key.as_ptr() as u64;
        entries[2] = key.len() as u64;
        entries[3] = 42; // value low word
        entries[4] = 0; // value high word
        entries[5] = TAG_INT as u64;
        entries[7] = u64::MAX; // end of chain
        let mut hash = vec![0u64; 6];
        hash[0] = 1; // length
        hash[1] = 8; // capacity
        hash[3] = 0; // head slot
        hash[5] = entries.as_ptr() as u64;
        let mut object = vec![0u64; 5];
        object[0] = HEAP_KIND_OBJECT; // heap kind before the object pointer
        object[1] = 7; // class id
        object[2] = 3; // declared id
        object[3] = 0; // declared id high word
        object[4] = hash.as_ptr() as u64; // dynamic hash at offset 24
        let classes = std::collections::HashMap::from([(
            7,
            Class {
                id: 7,
                name: "Bag".to_string(),
                parent: String::new(),
                is_abstract: false,
                properties: vec![Property {
                    name: "id".to_string(),
                    offset: 8,
                    php_type: "int".to_string(),
                    visibility: "public".to_string(),
                    by_ref: false,
                    declared: true,
                }],
                dynamic_offset: Some(24),
                debug_info: DebugInfo::default(),
                property_hooks: std::collections::HashMap::new(),
            },
        )]);
        let pointer = unsafe { object.as_ptr().add(1) as u64 };
        let value = Value::object(pointer);
        let description = describe_with(value, &classes);
        assert_eq!(description.display, "object(Bag) (2)");
        assert!(description.has_children);
        let children = children_with_annotated(value, &classes);
        let names: Vec<&str> = children.iter().map(|child| child.name.as_str()).collect();
        assert_eq!(names, ["id", "extra"]);
        assert_eq!(render(&describe_with(children[0].value, &classes)), "int(3)");
        assert_eq!(render(&describe_with(children[1].value, &classes)), "int(42)");
        let extra = object_dynamic_property_value(pointer, 7, 24, b"extra")
            .expect("watchable dynamic property");
        assert_eq!(render(&describe_with(extra, &classes)), "int(42)");
        assert!(
            object_dynamic_property_value(pointer, 7, 24, b"missing").is_none(),
            "missing keys must not resolve"
        );
        assert!(
            object_dynamic_property_value(pointer, 8, 24, b"extra").is_none(),
            "a mismatched class id must not resolve"
        );
    }

    /// Verifies a folded `__debugInfo()` projection replaces the declared names
    /// with the array keys while reading the underlying property slots.
    #[test]
    fn describes_debug_info_projection() {
        let mut object = vec![0u64; 3];
        object[0] = HEAP_KIND_OBJECT; // heap kind before the object pointer
        object[1] = 7; // class id
        object[2] = 9; // x payload
        let classes = std::collections::HashMap::from([(
            7,
            Class {
                id: 7,
                name: "Projected".to_string(),
                parent: String::new(),
                is_abstract: false,
                properties: vec![Property {
                    name: "x".to_string(),
                    offset: 8,
                    php_type: "int".to_string(),
                    visibility: "public".to_string(),
                    by_ref: false,
                    declared: true,
                }],
                dynamic_offset: None,
                debug_info: DebugInfo {
                    kind: "projection".to_string(),
                    entries: vec![DebugInfoEntry {
                        key: "label".to_string(),
                        property: Some("x".to_string()),
                        value: None,
                        php_type: None,
                    }],
                },
                property_hooks: std::collections::HashMap::new(),
            },
        )]);
        let pointer = unsafe { object.as_ptr().add(1) as u64 };
        let value = Value::object(pointer);
        let description = describe_with(value, &classes);
        assert_eq!(description.display, "object(Projected) (1)");
        let children = children_with_annotated(value, &classes);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "label");
        assert_eq!(render(&describe_with(children[0].value, &classes)), "int(9)");
    }

    /// Verifies a mixed projection keeps live property reads and renders safe
    /// compile-time literals without executing `__debugInfo()`.
    #[test]
    fn describes_static_debug_info_literals() {
        let mut object = vec![0u64; 3];
        object[0] = HEAP_KIND_OBJECT; // heap kind before the object pointer
        object[1] = 7; // class id
        object[2] = 9; // id payload
        let classes = std::collections::HashMap::from([(
            7,
            Class {
                id: 7,
                name: "Projected".to_string(),
                parent: String::new(),
                is_abstract: false,
                properties: vec![Property {
                    name: "id".to_string(),
                    offset: 8,
                    php_type: "int".to_string(),
                    visibility: "public".to_string(),
                    by_ref: false,
                    declared: true,
                }],
                dynamic_offset: None,
                debug_info: DebugInfo {
                    kind: "projection".to_string(),
                    entries: vec![
                        DebugInfoEntry {
                            key: "id".to_string(),
                            property: Some("id".to_string()),
                            value: None,
                            php_type: None,
                        },
                        DebugInfoEntry {
                            key: "version".to_string(),
                            property: None,
                            value: Some("string(3) \"1.0\"".to_string()),
                            php_type: Some("string".to_string()),
                        },
                        DebugInfoEntry {
                            key: "flags".to_string(),
                            property: None,
                            value: Some(
                                "array(2) {[0] => int(1), [1] => int(2)}".to_string()
                            ),
                            php_type: Some("array".to_string()),
                        },
                    ],
                },
                property_hooks: std::collections::HashMap::new(),
            },
        )]);
        let pointer = unsafe { object.as_ptr().add(1) as u64 };
        let value = Value::object(pointer);
        let description = describe_with(value, &classes);
        assert_eq!(description.display, "object(Projected) (3)");
        let children = children_with_annotated(value, &classes);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].name, "id");
        assert_eq!(render(&describe_with(children[0].value, &classes)), "int(9)");
        assert_eq!(
            children[1].static_text.as_deref(),
            Some("string(3) \"1.0\"")
        );
        assert_eq!(children[2].static_type.as_deref(), Some("array"));
        let nested = render_nested(value, &classes, 0);
        assert!(nested.contains("id => int(9)"), "{nested}");
        assert!(
            nested.contains("version => string(3) \"1.0\""),
            "{nested}"
        );
        assert!(
            nested.contains("flags => array(2) {[0] => int(1), [1] => int(2)}"),
            "{nested}"
        );
    }

    /// Verifies a virtual property is shown as a read-only marker and a backed
    /// getter is shown from its raw slot with the "getter not executed" caveat.
    #[test]
    fn describes_property_hooks_without_executing_them() {
        let mut object = vec![0u64; 8];
        object[0] = HEAP_KIND_OBJECT; // heap kind before the object pointer
        object[1] = 7; // class id
        object[2] = 10; // celsius
        object[4] = 0; // virtual fahrenheit has no meaningful slot
        object[6] = 4; // doubled backing value; the getter would return 8
        let property = |name: &str, offset: usize| Property {
            name: name.to_string(),
            offset,
            php_type: "int".to_string(),
            visibility: "public".to_string(),
            by_ref: false,
            declared: true,
        };
        let classes = std::collections::HashMap::from([(
            7,
            Class {
                id: 7,
                name: "Temperature".to_string(),
                parent: String::new(),
                is_abstract: false,
                properties: vec![
                    property("celsius", 8),
                    property("fahrenheit", 24),
                    property("doubled", 40),
                ],
                dynamic_offset: None,
                debug_info: DebugInfo::default(),
                property_hooks: std::collections::HashMap::from([
                    (
                        "fahrenheit".to_string(),
                        PropertyHooks {
                            get: true,
                            set: true,
                            get_by_ref: false,
                            is_virtual: true,
                        },
                    ),
                    (
                        "doubled".to_string(),
                        PropertyHooks {
                            get: true,
                            set: true,
                            get_by_ref: false,
                            is_virtual: false,
                        },
                    ),
                ]),
            },
        )]);
        let pointer = unsafe { object.as_ptr().add(1) as u64 };
        let value = Value::object(pointer);
        let children = children_with_annotated(value, &classes);
        let names: Vec<&str> = children.iter().map(|child| child.name.as_str()).collect();
        assert_eq!(names, ["celsius", "fahrenheit", "doubled"]);
        assert!(children[0].hook_note.is_none());
        let virtual_note = children[1]
            .hook_note
            .as_ref()
            .expect("virtual property note");
        assert!(virtual_note.is_virtual());
        assert_eq!(
            virtual_note.virtual_display(),
            "virtual [get/set hooks not executed]"
        );
        let backed_note = children[2]
            .hook_note
            .as_ref()
            .expect("backed getter note");
        assert!(!backed_note.is_virtual());
        assert_eq!(
            render(&describe_with(children[2].value, &classes)),
            "int(4)"
        );
        assert_eq!(
            backed_note.backing_suffix(),
            " [backing value; get hook not executed]"
        );
        let nested = render_nested(value, &classes, 0);
        assert!(
            nested.contains("fahrenheit => virtual [get/set hooks not executed]"),
            "{nested}"
        );
        assert!(
            nested.contains("doubled => int(4) [backing value; get hook not executed]"),
            "{nested}"
        );

        let write_only = PropertyHookNote {
            hooks: PropertyHooks {
                set: true,
                is_virtual: true,
                ..PropertyHooks::default()
            },
            php_type: "int".to_string(),
        };
        assert_eq!(write_only.virtual_display(), "virtual [write-only; set hook]");
    }

    /// Verifies a property watch resolves the slot only while the object still
    /// carries the expected class id, and observes an in-place write.
    #[test]
    fn reads_watched_object_property() {
        let text = b"origin";
        let mut storage = vec![0u64; 10];
        storage[0] = HEAP_KIND_OBJECT; // heap-kind word before the object pointer
        storage[1] = 7; // class id
        storage[2] = 2; // x payload
        storage[4] = 3; // y payload
        storage[6] = text.as_ptr() as u64; // label pointer
        storage[7] = text.len() as u64; // label length
        let object = unsafe { storage.as_ptr().add(1) as u64 };
        let property = Property {
            name: "x".to_string(),
            offset: 8,
            php_type: "int".to_string(),
            visibility: "public".to_string(),
            by_ref: false,
            declared: true,
        };
        assert_eq!(object_class_id(object), Some(7));
        let value = object_property_value(object, 7, &property).expect("read x");
        assert_eq!(render(&describe(value)), "int(2)");
        storage[2] = 9;
        let value = object_property_value(object, 7, &property).expect("read written x");
        assert_eq!(render(&describe(value)), "int(9)");
        assert!(
            object_property_value(object, 8, &property).is_none(),
            "a mismatched class id must not decode the slot"
        );
        assert!(
            object_property_value(0, 7, &property).is_none(),
            "the null page must not be decoded"
        );
    }

    /// Verifies a managed reference cell exposes its payload tag and renders
    /// with the `&` marker used by the terminal and DAP clients.
    #[test]
    fn describes_managed_reference_cell() {
        // The heap kind word sits immediately before the cell payload.
        let mut cell = vec![0u64; 3];
        cell[0] = HEAP_KIND_REF_CELL; // payload tag 0 (int) in bits 8..14
        cell[1] = 42;
        cell[2] = 0;
        let cell_pointer = unsafe { cell.as_ptr().add(1) as u64 };
        let mut frame_words = vec![cell_pointer, 0];
        let frame = unsafe { frame_words.as_mut_ptr().add(1) as usize };
        let local = Local {
            function_id: 0,
            name: "ref".to_string(),
            frame_offset: 8,
            php_type: "int".to_string(),
            storage: "ref_cell".to_string(),
            state_offset: None,
            param: Some(0),
            sensitive: false,
        };
        let value = local_value(&local, frame);
        assert_eq!(render(&describe(value)), "&int(42)");
    }
}
