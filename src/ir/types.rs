//! Purpose:
//! Defines the EIR storage type lattice and conversion helpers from PHP types.
//!
//! Called from:
//! - `crate::ir::value`, `crate::ir::builder`, and future AST-to-EIR lowering.
//!
//! Key details:
//! - `IrType` is an ABI/storage contract; precise PHP semantics remain in
//!   `PhpType` metadata carried beside each IR value.

use crate::types::PhpType;

/// Storage-level type of an EIR value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrType {
    I64,
    F64,
    Str,
    TaggedScalar,
    Heap(IrHeapKind),
    Void,
}

/// Heap category metadata for values stored as runtime heap pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrHeapKind {
    Array,
    Hash,
    Object,
    Mixed,
    Iterable,
    Union,
    Buffer,
}

impl IrType {
    /// Converts a checked PHP type to its EIR storage representation.
    pub fn from_php(php: &PhpType) -> Self {
        match php {
            PhpType::Int
            | PhpType::Bool
            | PhpType::False
            | PhpType::Callable
            | PhpType::Pointer(_)
            | PhpType::Resource(_) => IrType::I64,
            PhpType::Float => IrType::F64,
            PhpType::Str => IrType::Str,
            PhpType::TaggedScalar => IrType::TaggedScalar,
            PhpType::Void | PhpType::Never => IrType::Void,
            PhpType::Iterable => IrType::Heap(IrHeapKind::Iterable),
            PhpType::Mixed => IrType::Heap(IrHeapKind::Mixed),
            PhpType::Array(_) => IrType::Heap(IrHeapKind::Array),
            PhpType::AssocArray { .. } => IrType::Heap(IrHeapKind::Hash),
            PhpType::Buffer(_) => IrType::Heap(IrHeapKind::Buffer),
            PhpType::Object(_) | PhpType::Packed(_) => IrType::Heap(IrHeapKind::Object),
            PhpType::Union(_) => IrType::Heap(IrHeapKind::Union),
        }
    }

    /// Returns the number of ABI registers needed to carry this value.
    pub fn register_count(self) -> usize {
        match self {
            IrType::I64 | IrType::F64 | IrType::Heap(_) => 1,
            IrType::Str | IrType::TaggedScalar => 2,
            IrType::Void => 0,
        }
    }

    /// Returns true when this storage type needs floating-point register pools.
    pub fn is_float(self) -> bool {
        matches!(self, IrType::F64)
    }

    /// Returns true when this storage type can participate in runtime lifetime tracking.
    pub fn is_refcounted_storage(self) -> bool {
        matches!(self, IrType::Str | IrType::Heap(_))
    }

    /// Returns true when this storage type can be used as a normal operand.
    pub fn is_void(self) -> bool {
        matches!(self, IrType::Void)
    }

    /// Formats the storage type using the EIR textual format spelling.
    pub fn as_eir(self) -> String {
        match self {
            IrType::I64 => "I64".to_string(),
            IrType::F64 => "F64".to_string(),
            IrType::Str => "Str".to_string(),
            IrType::TaggedScalar => "TaggedScalar".to_string(),
            IrType::Heap(kind) => format!("Heap({})", kind.as_eir()),
            IrType::Void => "Void".to_string(),
        }
    }
}

impl IrHeapKind {
    /// Formats the heap subkind using the EIR textual format spelling.
    pub fn as_eir(self) -> &'static str {
        match self {
            IrHeapKind::Array => "Array",
            IrHeapKind::Hash => "Hash",
            IrHeapKind::Object => "Object",
            IrHeapKind::Mixed => "Mixed",
            IrHeapKind::Iterable => "Iterable",
            IrHeapKind::Union => "Union",
            IrHeapKind::Buffer => "Buffer",
        }
    }
}
