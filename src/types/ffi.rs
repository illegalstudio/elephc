//! Purpose:
//! Defines FFI type and declaration metadata used by checking and native call lowering.
//! Maps compiler extension types into ABI-visible shapes for extern declarations.
//!
//! Called from:
//! - `crate::types::checker::extern_decl`
//! - `crate::codegen`
//!
//! Key details:
//! - FFI metadata must preserve target-independent type contracts while leaving register/stack layout to ABI helpers.

use std::collections::HashMap;

use crate::parser::ast::CType;

use super::{PackedClassInfo, PhpType};

/// Maps a parser CType to its corresponding PhpType for FFI and extern declaration lowering.
///
/// Used when lowering `extern` declarations to connect the parser's C types (Int, Float,
/// Str, Bool, Void, Ptr, TypedPtr, Callable) to the compiler's internal PhpType representation.
pub fn ctype_to_php_type(ct: &CType) -> PhpType {
    match ct {
        CType::Int => PhpType::Int,
        CType::Float => PhpType::Float,
        CType::Str => PhpType::Str,
        CType::Bool => PhpType::Bool,
        CType::Void => PhpType::Void,
        CType::Ptr => PhpType::Pointer(None),
        CType::TypedPtr(name) => PhpType::Pointer(Some(name.clone())),
        CType::Callable => PhpType::Callable,
    }
}

/// Returns the stack frame size in bytes for a C-facing FFI type.
///
/// All integer, float, bool, pointer, typed pointer, callable, and string (char*) types
/// occupy 8 bytes on the stack. Void occupies 0 bytes. Used during codegen frame layout
/// to determine how callee-saved registers and local variables are sized.
pub fn ctype_stack_size(ct: &CType) -> usize {
    match ct {
        CType::Int
        | CType::Float
        | CType::Bool
        | CType::Ptr
        | CType::TypedPtr(_)
        | CType::Callable => 8,
        CType::Str => 8, // char*
        CType::Void => 0,
    }
}

/// Returns the ABI size for a PhpType, or None if the type has no fixed size.
pub fn packed_type_size(
    ty: &PhpType,
    packed_classes: &HashMap<String, PackedClassInfo>,
) -> Option<usize> {
    match ty {
        PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => Some(8),
        PhpType::Packed(name) => packed_classes.get(name).map(|info| info.total_size),
        _ => None,
    }
}
