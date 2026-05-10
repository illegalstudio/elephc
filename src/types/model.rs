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
pub enum PhpType {
    Int,
    Float,
    Str,
    Bool,
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
}

impl PhpType {
    pub fn stream_resource() -> PhpType {
        PhpType::Resource(Some("stream".to_string()))
    }

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

    /// Size in bytes on the stack.
    pub fn stack_size(&self) -> usize {
        match self {
            PhpType::Bool => 8,
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
            PhpType::Callable => 8,          // function address
            PhpType::Object(_) => 8,         // pointer to heap
            PhpType::Packed(_) => 8,         // metadata-only nominal type, usually accessed by pointer
            PhpType::Pointer(_) => 8,        // 64-bit address
            PhpType::Resource(_) => 8,       // runtime resource id / native handle
            PhpType::Union(_) => 8,          // boxed runtime-tagged payload (same storage as Mixed)
        }
    }

    /// Number of registers used to pass this type as an argument.
    pub fn register_count(&self) -> usize {
        match self {
            PhpType::Bool => 1,
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
            PhpType::Union(_) => PhpType::Mixed,
            PhpType::Resource(_) => PhpType::Int,
            PhpType::Never => PhpType::Void, // never should not be materialized; fallback to void sentinel
            _ => self.clone(),
        }
    }
}

impl fmt::Display for PhpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhpType::Int => write!(f, "int"),
            PhpType::Float => write!(f, "float"),
            PhpType::Str => write!(f, "string"),
            PhpType::Bool => write!(f, "bool"),
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
