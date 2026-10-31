//! Purpose:
//! Injects PHP's six fileinfo functions and finfo class over the optional native bridge.
//!
//! Called from:
//! - The compilation pipeline and EIR/codegen test harness before name resolution.
//!
//! Key details:
//! - Synthetic declarations use the ordinary EIR, object and extern ABI paths on every target.
//! - Opaque dynamic eval can opt in with --with-fileinfo.

pub(crate) mod abi;
mod build;
mod detect;
mod streams;
mod wrappers;
mod pending;
mod stream_magic;
use crate::parser::ast::{Program, Stmt};
use crate::php_version::PhpVersion;

/// Builds the class, PHP functions and bridge extern declarations.
#[cfg(test)]
pub(crate) fn fileinfo_declarations() -> Program { build::declarations(PhpVersion::default()) }

/// Detects fileinfo references in entry programs and Composer autoload closures.
pub fn program_uses_surface(program: &[Stmt]) -> bool {
    detect::program_uses_fileinfo(program)
}

/// Prepends the fileinfo surface only when referenced or explicitly forced.
pub fn inject_if_used(program: Program, force: bool, inventory: &mut crate::optimize::reachability::PreludeInventory) -> Program {
    inject_if_used_for_version(program, force, PhpVersion::default(), inventory)
}

/// Selects reflection deprecation metadata for the requested PHP compatibility profile.
pub fn inject_if_used_for_version(program: Program, force: bool, php_version: PhpVersion, inventory: &mut crate::optimize::reachability::PreludeInventory) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !program_uses_surface(&program) { return program; }
        let mut declarations = build::declarations(php_version);
        inventory.record_program("fileinfo", &declarations);
        declarations.extend(program);
        declarations
    })
}
