//! Purpose:
//! Defines the core `PhpType` model used throughout checking and code generation.
//! Captures PHP scalar, compound, object, callable, FFI, pointer, and internal runtime shapes.
//!
//! Called from:
//! - `crate::types::checker`
//! - `crate::codegen`
//!
//! Key details:
//! - Internal types such as Mixed and runtime resources encode codegen/runtime contracts, not just PHP syntax.

use std::collections::HashMap;
use std::fmt;
use super::ScalarWidth;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Callable used in match arms, constructed when closures are added
/// PHP runtime type.
pub enum PhpType {
    Int,
    Float,
    Str,
    Bool,
    /// The PHP literal `false` subtype. Runtime representation is identical to `Bool`.
    False,
    Void,
    Never,
    Iterable,
    Mixed,
    Array(Box<PhpType>),
    AssocArray {
        key: Box<PhpType>,
        value: Box<PhpType>,
    },
    Buffer(Box<PhpType>),
    /// Storage format of a `buffer<T>` element declared with a C width name (`buffer<i32>`,
    /// `buffer<u8>`, `buffer<f32>`). It only ever appears as the element type inside `Buffer`:
    /// an element read yields `storage_value_type()` (`int` or `float`), and a write takes
    /// that same type, so no value is ever typed `Narrow`. Packed class fields keep the same
    /// widths in their field metadata instead (`PackedFieldInfo::width`).
    Narrow(ScalarWidth),
    Callable,
    Object(String),
    Packed(String),
    Pointer(Option<String>), // None = opaque ptr, Some("Class") = typed ptr<Class>
    Resource(Option<String>), // None = generic resource, Some("stream") = file/stdio stream
    Union(Vec<PhpType>),
    /// Codegen-internal inline nullable scalar: two words `{payload, tag}` with no heap
    /// allocation. The tag reuses the runtime value tag scheme (0 = int, 8 = null), so the
    /// pair is word-compatible with a boxed Mixed cell. The checker never produces this
    /// type; codegen funnels construct it from `int|null` unions only under
    /// `NullRepr::Tagged`. Under the legacy sentinel opt-out it never exists.
    TaggedScalar,
}

impl PhpType {
    /// Represents PHP's unrestricted array contract with a boxed packed-or-hash payload.
    /// Unlike a concrete storage type, this declaration cannot narrow to one call site's layout.
    pub(crate) fn php_array() -> Self {
        Self::Union(vec![
            Self::Array(Box::new(Self::Mixed)),
            Self::AssocArray { key: Box::new(Self::Mixed), value: Box::new(Self::Mixed) },
        ])
    }

    /// Identifies the exact non-null PHP array contract, not a union that also permits scalars.
    pub(crate) fn is_php_array(&self) -> bool {
        matches!(self, Self::Union(members) if members.len() == 2
            && members.iter().any(|member| matches!(member, Self::Array(value) if **value == Self::Mixed))
            && members.iter().any(|member| matches!(member, Self::AssocArray { key, value }
                if **key == Self::Mixed && **value == Self::Mixed)))
    }

    /// Returns the type a packed field or buffer element of this storage type reads as.
    ///
    /// A width (`PhpType::Narrow`) reads as its value type, `int` or `float`; every other
    /// element type is its own value type.
    pub fn storage_value_type(&self) -> PhpType {
        match self {
            PhpType::Narrow(width) => width.value_type(),
            other => other.clone(),
        }
    }

    /// Returns a `PhpType::Resource(Some("stream"))` representing a stream resource.
    pub fn stream_resource() -> PhpType {
        PhpType::Resource(Some("stream".to_string()))
    }

    /// Returns true if `expected` is compatible with `actual` for resource type matching.
    /// A typed resource (Some) is compatible with a generic resource (None), and two typed
    /// resources are compatible when their kind strings match.
    pub fn resource_types_compatible(expected: &PhpType, actual: &PhpType) -> bool {
        match (expected, actual) {
            (PhpType::Resource(None), PhpType::Resource(_))
            | (PhpType::Resource(_), PhpType::Resource(None)) => true,
            (PhpType::Resource(Some(expected)), PhpType::Resource(Some(actual))) => {
                expected == actual
            }
            _ => false,
        }
    }

    /// Returns true when a null property default must be materialized into a slot of
    /// this type (and the literal-default emitters support doing so).
    ///
    /// `Void`, nullable unions, `Mixed`, and object slots encode null distinctly, so the
    /// default write is required and supported. Every other slot either has no null
    /// encoding (plain scalars, strings, arrays — those slots are always overwritten
    /// before an observable read when refinement rebound them) or reads zero-initialized
    /// storage as null already (callable/pointer-shaped slots), so the null default is
    /// skipped for them.
    pub fn null_property_default_required(&self) -> bool {
        match self {
            PhpType::Void | PhpType::Mixed | PhpType::TaggedScalar | PhpType::Object(_) => true,
            PhpType::Union(members) => members.iter().any(|member| matches!(member, PhpType::Void)),
            PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::False
            | PhpType::Never
            | PhpType::Iterable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Narrow(_)
            | PhpType::Callable
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Resource(_) => false,
        }
    }

    /// Size in bytes on the stack.
    pub fn stack_size(&self) -> usize {
        match self {
            PhpType::Bool | PhpType::False => 8,
            PhpType::Int => 8,
            PhpType::Float => 8,
            PhpType::Str => 16,
            PhpType::Void => 8,              // null sentinel stored as 8 bytes
            PhpType::Never => 0,             // never materialized; functions with :never do not return
            PhpType::Iterable => 8,          // type-erased pointer (array|Traversable)
            PhpType::Mixed => 8,             // pointer to heap-tagged mixed cell
            PhpType::Array(_) => 8,          // pointer to heap
            PhpType::AssocArray { .. } => 8, // pointer to heap
            PhpType::Buffer(_) => 8,         // pointer to buffer header
            PhpType::Narrow(_) => 8,         // never a value; its element reads as a word
            PhpType::Callable => 8,          // callable descriptor address
            PhpType::Object(_) => 8,         // pointer to heap
            PhpType::Packed(_) => 8,         // metadata-only nominal type, usually accessed by pointer
            PhpType::Pointer(_) => 8,        // 64-bit address
            PhpType::Resource(_) => 8,       // runtime resource id / native handle
            PhpType::Union(_) => 8,          // boxed runtime-tagged payload (same storage as Mixed)
            PhpType::TaggedScalar => 16,     // inline nullable scalar: payload word + tag word
        }
    }

    /// Number of registers used to pass this type as an argument.
    pub fn register_count(&self) -> usize {
        match self {
            PhpType::Bool | PhpType::False => 1,
            PhpType::Int => 1,
            PhpType::Float => 1,
            PhpType::Str => 2,
            PhpType::Void => 0,
            PhpType::Never => 0,
            PhpType::Iterable => 1,
            PhpType::Mixed => 1,
            PhpType::Array(_) => 1,
            PhpType::AssocArray { .. } => 1,
            PhpType::Buffer(_) => 1,
            PhpType::Narrow(_) => 1,
            PhpType::Callable => 1,
            PhpType::Object(_) => 1,
            PhpType::Packed(_) => 1,
            PhpType::Pointer(_) => 1,
            PhpType::Resource(_) => 1,
            PhpType::Union(_) => 1,
            PhpType::TaggedScalar => 2,
        }
    }

    /// Returns true if this type uses a floating-point register (d0-d7).
    pub fn is_float_reg(&self) -> bool {
        matches!(self, PhpType::Float)
    }

    /// Reports whether an integer must widen to satisfy this declared numeric contract.
    pub(crate) fn coerces_integer_to_float(&self) -> bool {
        match self {
            Self::Float => true,
            Self::Union(members) => members.contains(&Self::Float)
                && !members.iter().any(|member| matches!(member, Self::Int | Self::Mixed)),
            _ => false,
        }
    }

    /// Returns true for heap values whose lifetime is tracked with runtime refcounts.
    pub fn is_refcounted(&self) -> bool {
        matches!(
            self,
            PhpType::Iterable
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Union(_)
        )
    }

    /// Lower high-level checker-only types to the runtime representation used by codegen.
    /// `Iterable` keeps its own runtime shape (raw heap pointer dispatched via the heap-kind tag),
    /// so it is no longer collapsed to `Mixed` here.
    pub fn codegen_repr(&self) -> PhpType {
        match self {
            PhpType::Union(members)
                if crate::codegen::sentinels::null_repr_is_tagged()
                    && nullable_int_union_members(members) =>
            {
                PhpType::TaggedScalar
            }
            // `?ptr<T>` is one raw pointer word where 0 is null, like a C pointer.
            PhpType::Union(members) if nullable_pointer_union_member(members).is_some() => {
                nullable_pointer_union_member(members).expect("checked above")
            }
            PhpType::Union(_) => PhpType::Mixed,
            PhpType::False => PhpType::Bool,
            PhpType::Resource(_) => PhpType::Int,
            PhpType::Never => PhpType::Void, // never should not be materialized; fallback to void sentinel
            _ => self.clone(),
        }
    }

    /// Returns whether a reference cell can be read with either payload representation.
    ///
    /// Object class names do not change pointer storage; semantic return compatibility is
    /// checked separately. Container element layouts still matter even when both outer
    /// values are pointers, so comparing only their EIR heap kind would be too permissive.
    pub(crate) fn reference_payload_compatible(&self, expected: &PhpType) -> bool {
        match (self.codegen_repr(), expected.codegen_repr()) {
            (PhpType::Object(_), PhpType::Object(_)) => true,
            (PhpType::Array(actual), PhpType::Array(expected)) => {
                actual.reference_payload_compatible(&expected)
            }
            (
                PhpType::AssocArray { key: actual_key, value: actual_value },
                PhpType::AssocArray { key: expected_key, value: expected_value },
            ) => actual_key.reference_payload_compatible(&expected_key)
                && actual_value.reference_payload_compatible(&expected_value),
            (actual, expected) => actual == expected,
        }
    }

    /// Returns true for the `int|float` union PHP's arithmetic produces at the overflow boundary.
    ///
    /// This is what `$i++` leaves in an integer local: `PHP_INT_MAX++` is a float in php-src and
    /// in elephc, so the local afterwards is one or the other. It is a genuine type rather than
    /// a fallback, which is why it gets a name: `mixed` would say the same thing about a counter
    /// as about a `json_decode` result, and every value derived from the counter would inherit
    /// that.
    ///
    /// Exact and order-independent: a WIDER union (`int|float|string`, `int|false`) is not this
    /// type and must keep reaching the ordinary rules — in particular the `X|false` diagnostics,
    /// which exist to catch an unchecked sentinel.
    pub fn is_int_float_union(&self) -> bool {
        let PhpType::Union(members) = self else {
            return false;
        };
        members.len() == 2 && members.contains(&PhpType::Int) && members.contains(&PhpType::Float)

    }

    /// Returns true if this is an indexed array of a scalar (int/float/bool) element type.
    ///
    /// The hash-based builtins accept such indexed inputs by converting them to integer-keyed
    /// hashes; scalar elements are copied by value, so the converted temporaries are safe to
    /// free. String/heap element indexed inputs are a follow-up (they hit x86-specific converter
    /// and clone-shallow issues), so the checker restricts indexed inputs to scalar elements.
    pub fn is_scalar_indexed_array(&self) -> bool {
        matches!(
            self,
            PhpType::Array(elem)
                if matches!(**elem, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False)
        )
    }

    /// Returns the hash key type this array type contributes: `Int` for an indexed array,
    /// the declared key for an associative array, `Int` otherwise.
    pub fn hash_key_type(&self) -> PhpType {
        match self {
            PhpType::Array(_) => PhpType::Int,
            PhpType::AssocArray { key, .. } => (**key).clone(),
            _ => PhpType::Int,
        }
    }

    /// Returns the hash value type this array type contributes: the element type for an indexed
    /// array, the declared value for an associative array, `Mixed` otherwise.
    pub fn hash_value_type(&self) -> PhpType {
        match self {
            PhpType::Array(elem) => (**elem).clone(),
            PhpType::AssocArray { value, .. } => (**value).clone(),
            _ => PhpType::Mixed,
        }
    }

    /// Widens two types to a common type: the type itself when both agree, else `Mixed`.
    pub fn widen(a: PhpType, b: PhpType) -> PhpType {
        if a == b {
            a
        } else {
            PhpType::Mixed
        }
    }

    /// Widens element types contributed by alternative array-producing branches.
    ///
    /// `Never` is the placeholder element type of an empty array literal, so it
    /// contributes no runtime value and defers to the populated branch. The
    /// `false` subtype and `bool` share the boolean contract, and pointers and
    /// `null` share the pointer word (see `merge_pointer_elements`); every other
    /// disagreement requires boxed `Mixed` element storage.
    pub fn widen_array_branch_element(a: PhpType, b: PhpType) -> PhpType {
        if a == b {
            return a;
        }
        if a == PhpType::Never {
            return b;
        }
        if b == PhpType::Never {
            return a;
        }
        if matches!(
            (&a, &b),
            (PhpType::Bool, PhpType::False) | (PhpType::False, PhpType::Bool)
        ) {
            return PhpType::Bool;
        }
        if let Some(pointer) = merge_pointer_elements(&a, &b) {
            return pointer;
        }
        PhpType::Mixed
    }

    /// Computes the result hash type for a two-input hash builtin (the `array_replace` /
    /// `array_diff_assoc` family). The key and value each widen to `Mixed` when the two inputs
    /// disagree, so a `foreach` over the result performs the correct runtime key/value dispatch
    /// when an indexed input is mixed with a string-keyed associative input.
    pub fn two_input_hash_result(t1: &PhpType, t2: &PhpType) -> PhpType {
        PhpType::AssocArray {
            key: Box::new(PhpType::widen(t1.hash_key_type(), t2.hash_key_type())),
            value: Box::new(PhpType::widen(t1.hash_value_type(), t2.hash_value_type())),
        }
    }
}

/// Merges the pointer types one local holds at different points into the type of its word.
///
/// Unlike an array element (`merge_pointer_elements`), a local keeps a typed target over an
/// untyped `ptr`, as the checker's assignment merge does, so `$p = ptr_null();` followed by
/// `$p = $buffer[0];` still reaches the record's fields. Two different targets share an untyped
/// `ptr`. Both sides are expected to be `PhpType::Pointer`.
pub(crate) fn merge_local_pointer_targets(a: &PhpType, b: &PhpType) -> PhpType {
    match (a, b) {
        _ if a == b => a.clone(),
        (PhpType::Pointer(None), PhpType::Pointer(Some(_))) => b.clone(),
        (PhpType::Pointer(Some(_)), PhpType::Pointer(None)) => a.clone(),
        _ => PhpType::Pointer(None),
    }
}

/// Merges two array element types that are pointers, `?ptr<T>` or `null` into one pointer element.
///
/// Every pointer is one raw word, so pointers with different target tags share an untyped `ptr`
/// slot, and a `null` element is the null pointer word, which keeps the element `?ptr<T>`.
/// Returns `None` when either side is something else (or both are `null`), leaving the caller's
/// ordinary widening to `mixed`.
pub(crate) fn merge_pointer_elements(a: &PhpType, b: &PhpType) -> Option<PhpType> {
    /// Splits a pointer-like element into its pointer (none for `null`) and nullability.
    fn split(ty: &PhpType) -> Option<(Option<PhpType>, bool)> {
        match ty {
            PhpType::Pointer(_) => Some((Some(ty.clone()), false)),
            PhpType::Void => Some((None, true)),
            PhpType::Union(members) => nullable_pointer_union_member(members).map(|p| (Some(p), true)),
            _ => None,
        }
    }
    let (left, left_null) = split(a)?;
    let (right, right_null) = split(b)?;
    let pointer = match (left, right) {
        (Some(left), Some(right)) if left == right => left,
        (Some(_), Some(_)) => PhpType::Pointer(None),
        (Some(pointer), None) | (None, Some(pointer)) => pointer,
        (None, None) => return None,
    };
    Some(if left_null || right_null {
        PhpType::Union(vec![pointer, PhpType::Void])
    } else {
        pointer
    })
}

/// Returns the type of an element read that can miss: a pointer element becomes `?ptr<T>`.
///
/// A missing pointer element is the null pointer word, which is PHP null, so the read keeps
/// `null` in its type (a plain pointer would make `=== null` fold to false). Other element
/// types are returned unchanged.
pub(crate) fn pointer_element_read_type(element: PhpType) -> PhpType {
    if matches!(element, PhpType::Pointer(_)) {
        PhpType::Union(vec![element, PhpType::Void])
    } else {
        element
    }
}

/// Returns the pointer of a `?ptr<T>` union: exactly one pointer member plus `null`.
///
/// Such a union is stored as that pointer word with 0 meaning null (decision recorded for task
/// 6da89f60), so a null pointer and PHP `null` are the same value.
pub(crate) fn nullable_pointer_union_member(members: &[PhpType]) -> Option<PhpType> {
    let mut pointer = None;
    let mut has_null = false;
    for member in members {
        match member {
            PhpType::Pointer(_) if pointer.is_none() => pointer = Some(member.clone()),
            PhpType::Void | PhpType::Never => has_null = true,
            _ => return None,
        }
    }
    pointer.filter(|_| has_null)
}

/// Returns whether a type is a `?ptr<T>` union, stored as a pointer word where 0 is null.
pub(crate) fn is_nullable_pointer_type(php_type: &PhpType) -> bool {
    matches!(php_type, PhpType::Union(members) if nullable_pointer_union_member(members).is_some())
}

/// Returns whether `ty` is an indexed array whose elements are stored as raw pointer words
/// (`array<ptr<T>>` or `array<ptr<T>|null>`), as opposed to boxed Mixed cells.
pub(crate) fn is_pointer_element_indexed_array(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Pointer(_))
    )
}

/// Returns the type a value of a declared type is boxed into `mixed` as: its codegen type, except
/// that a `?ptr<T>` keeps its union, so address 0 boxes as PHP null instead of the integer 0.
pub(crate) fn mixed_box_php_type(php_type: &PhpType) -> PhpType {
    if is_nullable_pointer_type(php_type) {
        php_type.clone()
    } else {
        php_type.codegen_repr()
    }
}

/// Spells a pointer parameter type for a diagnostic: `?ptr<T>` for the nullable union, which
/// otherwise prints as `ptr<T>|null`.
pub(crate) fn pointer_param_display(php_type: &PhpType) -> String {
    if is_nullable_pointer_type(php_type) {
        format!("?{}", php_type.codegen_repr())
    } else {
        php_type.to_string()
    }
}

/// Returns the PHP type an EIR value carries for a declared type.
///
/// That is the codegen representation, except for a `?ptr<T>`: its representation is the bare
/// pointer word, and a value typed without the null member would make `=== null`, `is_null()`,
/// `isset()`, `??` and `echo` treat a null result as a set pointer.
pub(crate) fn eir_value_php_type(php_type: &PhpType) -> PhpType {
    if is_nullable_pointer_type(php_type) {
        php_type.clone()
    } else {
        php_type.codegen_repr()
    }
}

/// Returns true for an `int|null` union that can use the inline tagged-scalar representation.
fn nullable_int_union_members(members: &[PhpType]) -> bool {
    let mut has_int = false;
    let mut has_null = false;
    for member in members {
        match member {
            PhpType::Int => has_int = true,
            PhpType::Void | PhpType::Never => has_null = true,
            _ => return false,
        }
    }
    has_int && has_null
}

impl fmt::Display for PhpType {
    /// Formats the type as a human-readable string using PHP-style syntax (e.g., `int`, `array<int>`,
    /// `resource<stream>`, `ptr<MyClass>`). Used for error messages and debug output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_php_array() {
            return write!(f, "array");
        }
        match self {
            PhpType::Int => write!(f, "int"),
            PhpType::Float => write!(f, "float"),
            PhpType::Str => write!(f, "string"),
            PhpType::Bool => write!(f, "bool"),
            PhpType::False => write!(f, "false"),
            PhpType::Void => write!(f, "null"),
            PhpType::Never => write!(f, "never"),
            PhpType::Iterable => write!(f, "iterable"),
            PhpType::Mixed => write!(f, "mixed"),
            PhpType::Array(inner) => write!(f, "array<{}>", inner),
            PhpType::AssocArray { key, value } => write!(f, "array<{}, {}>", key, value),
            PhpType::Buffer(inner) => write!(f, "buffer<{}>", inner),
            PhpType::Narrow(width) => write!(f, "{}", width.name()),
            PhpType::Callable => write!(f, "callable"),
            // An EMPTY class name is the `object` type hint: any object, no particular class.
            // Rendering it as the empty string produced "expects , got int".
            PhpType::Object(name) if name.is_empty() => write!(f, "object"),
            PhpType::Object(name) => write!(f, "{}", name),
            PhpType::Packed(name) => write!(f, "packed {}", name),
            PhpType::Pointer(Some(name)) => write!(f, "ptr<{}>", name),
            PhpType::Pointer(None) => write!(f, "ptr"),
            PhpType::Resource(Some(kind)) => write!(f, "resource<{}>", kind),
            PhpType::Resource(None) => write!(f, "resource"),
            PhpType::TaggedScalar => write!(f, "int|null"),
            PhpType::Union(members) => {
                for (i, member) in members.iter().enumerate() {
                    if i > 0 {
                        write!(f, "|")?;
                    }
                    write!(f, "{}", member)?;
                }
                Ok(())
            }
        }
    }
}

/// Maps variable names to their resolved types.
pub type TypeEnv = HashMap<String, PhpType>;
