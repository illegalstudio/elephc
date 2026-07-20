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
    /// `NullRepr::Tagged`. Under the default sentinel representation it never exists.
    TaggedScalar,
}

impl PhpType {
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
            PhpType::Union(_) => PhpType::Mixed,
            PhpType::False => PhpType::Bool,
            PhpType::Resource(_) => PhpType::Int,
            PhpType::Never => PhpType::Void, // never should not be materialized; fallback to void sentinel
            _ => self.clone(),
        }
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
            PhpType::Callable => write!(f, "callable"),
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
