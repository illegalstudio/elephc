//! Purpose:
//! Provides external global symbol loads and stores for statement assignments.
//! Offers a typed storage interface to assignment and expression statement paths.
//!
//! Called from:
//! - `crate::codegen_support::stmt::storage`
//!
//! Key details:
//! - Loads and stores must use ABI value sizes and preserve refcounted ownership conventions.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::types::PhpType;

/// Emits a store to an external global symbol for the given PHP variable.
///
/// Dispatches by `ty` to select the correct ABI register and conversion:
/// - Scalar types (bool, int, resource, pointer, buffer, packed, callable) use `int_result_reg`
/// - Float uses `float_result_reg`
/// - String allocates a null-terminated copy via `__rt_str_to_cstr` before storing the `char*`
/// - Void, never, iterable, mixed, union, array, assoc_array, and object emit a warning and
///   perform no store — these types cannot be stored to extern globals without runtime support.
pub(super) fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    emitter.comment(&format!("store to extern global ${}", name));
    let sym = emitter.target.extern_symbol(name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_store_reg_to_extern_symbol(emitter, abi::int_result_reg(emitter), &sym, 0); // store integer or pointer payload into extern global storage
        }
        PhpType::Float => {
            abi::emit_store_reg_to_extern_symbol(emitter, abi::float_result_reg(emitter), &sym, 0); // store floating-point payload into extern global storage
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_str_to_cstr");                 // allocate a null-terminated copy before publishing it to C-owned global storage
            abi::emit_store_reg_to_extern_symbol(emitter, abi::int_result_reg(emitter), &sym, 0); // store the returned char* into the extern global slot
        }
        PhpType::Void
        | PhpType::Never
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::TaggedScalar
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_) => {
            emitter.comment(&format!(
                "WARNING: unsupported extern global store for ${}",
                name
            ));
        }
    }
}

/// Emits a load from an external global symbol for the given PHP variable.
///
/// Dispatches by `ty` to select the correct ABI register and conversion:
/// - Scalar types (bool, int, resource, pointer, buffer, packed, callable) load into `int_result_reg`
/// - Float loads into `float_result_reg`
/// - String loads the `char*` first, then calls `__rt_cstr_to_str` to convert it to the PHP string
///   result convention (borrowed C string → owned PhpString)
/// - Void, never, iterable, mixed, union, array, assoc_array, and object emit a warning and
///   perform no load — these types cannot be loaded from extern globals without runtime support.
pub(super) fn emit_extern_global_load(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    emitter.comment(&format!("load from extern global ${}", name));
    let sym = emitter.target.extern_symbol(name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_load_extern_symbol_to_reg(emitter, abi::int_result_reg(emitter), &sym, 0); // load integer or pointer payload from extern global storage
        }
        PhpType::Float => {
            abi::emit_load_extern_symbol_to_reg(emitter, abi::float_result_reg(emitter), &sym, 0); // load floating-point payload from extern global storage
        }
        PhpType::Str => {
            abi::emit_load_extern_symbol_to_reg(emitter, abi::int_result_reg(emitter), &sym, 0); // load the borrowed char* from extern global storage
            abi::emit_call_label(emitter, "__rt_cstr_to_str");                 // convert the borrowed C string into the elephc string result convention
        }
        PhpType::Void
        | PhpType::Never
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::TaggedScalar
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_) => {
            emitter.comment(&format!(
                "WARNING: unsupported extern global load for ${}",
                name
            ));
        }
    }
}
