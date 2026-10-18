//! Purpose:
//! Defines the native return ABI shared by legacy hook metadata and runtime consumers.
//!
//! Called from:
//! - User metadata emission and Serializable serialization/hydration adapters.
//!
//! Key details:
//! - Tentative PHP return hints permit every storage shape, including omitted hints.
//! - Strings, boxed values, and other heap payloads require different cleanup paths.

use crate::types::PhpType;

/// Dense table encoding for one legacy method's native return storage.
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum SerializableReturnAbi {
    /// A native string result pair, owning or persistent according to the ordinary string ABI.
    String = 0,
    /// A boxed Mixed owner carrying its own PHP tag.
    Mixed = 1,
    /// A void result observed by PHP as null.
    Null = 2,
    /// A scalar that has no heap owner.
    Scalar = 3,
    /// An owned native container, object, or callable pointer.
    Heap = 4,
    /// An inline nullable integer pair whose second native return word is its PHP tag.
    TaggedScalar = 5,
}

impl SerializableReturnAbi {
    /// Selects cleanup and runtime validation from the checked native storage representation.
    pub(crate) fn of(ty: &PhpType) -> Self {
        match ty.codegen_repr() {
            PhpType::Str => Self::String,
            PhpType::Mixed | PhpType::Union(_) => Self::Mixed,
            PhpType::Void | PhpType::Never => Self::Null,
            PhpType::TaggedScalar => Self::TaggedScalar,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) | PhpType::Callable | PhpType::Iterable => Self::Heap,
            _ => Self::Scalar,
        }
    }
}
