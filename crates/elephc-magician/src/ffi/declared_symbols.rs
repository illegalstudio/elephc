//! Purpose:
//! Exports registration of generated declaration-name metadata into eval.
//! The interpreter uses these lists to answer `get_declared_*()` calls without
//! treating AOT class-like symbols as eval-owned declarations.
//!
//! Called from:
//! - Generated EIR backend assembly when a persistent eval context is created.
//!
//! Key details:
//! - Invalid context handles, ABI versions, or names fail closed as `false`.
//! - Duplicate names are ignored case-insensitively while preserving first spelling.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};

/// Class-like declaration list selected by one registration entry point.
#[derive(Clone, Copy)]
enum DeclaredSymbolKind {
    Class,
    Interface,
    Trait,
}

/// Registers one generated class or enum name for eval `get_declared_classes()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_declared_class_name(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_declared_symbol_inner(ctx, name_ptr, name_len, DeclaredSymbolKind::Class)
    })
    .unwrap_or(0)
}

/// Registers one generated interface name for eval `get_declared_interfaces()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_declared_interface_name(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_declared_symbol_inner(ctx, name_ptr, name_len, DeclaredSymbolKind::Interface)
    })
    .unwrap_or(0)
}

/// Registers one generated trait name for eval `get_declared_traits()`.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_declared_trait_name(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        register_declared_symbol_inner(ctx, name_ptr, name_len, DeclaredSymbolKind::Trait)
    })
    .unwrap_or(0)
}

/// Runs declared-name registration after installing a panic boundary.
///
/// # Safety
/// Mirrors the exported registration functions; invalid handles or unreadable
/// name storage fail closed as `false`.
unsafe fn register_declared_symbol_inner(
    ctx: *mut ElephcEvalContext,
    name_ptr: *const u8,
    name_len: u64,
    kind: DeclaredSymbolKind,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        return 0;
    }
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return 0;
    };
    let registered = match kind {
        DeclaredSymbolKind::Class => context.define_external_declared_class_name(&name),
        DeclaredSymbolKind::Interface => context.define_external_declared_interface_name(&name),
        DeclaredSymbolKind::Trait => context.define_external_declared_trait_name(&name),
    };
    i32::from(registered)
}
