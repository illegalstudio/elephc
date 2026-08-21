//! Purpose:
//! Orchestrates injection of SPL-style builtin class metadata into the checker.
//! Delegates each builtin family to focused submodules to keep declarations small and cohesive.
//!
//! Called from:
//! - `crate::types::checker::driver`
//!
//! Key details:
//! - Public builtin names are checked for redeclaration before synthetic classes are inserted.
//! - Signature/storage refinements run after class flattening through `patch_builtin_spl_storage_signatures`.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::types::traits::FlattenedClass;

use super::{builtin_types::InterfaceDeclInfo, Checker};

mod append;
mod append_array_iterator;
mod append_storage;
mod caching;
mod common;
mod containers;
mod filesystem;
mod filters;
mod forwarding;
mod heaps;
mod multiple;
mod object_storage;
mod patch;
mod phar;
mod recursive;
mod recursive_array;
mod recursive_iterator_iterator;
mod recursive_iterator_iterator_traversal;
mod regex;
mod registry;

pub(crate) use registry::program_may_reference_spl;
mod storage;
mod zip;

/// Injects builtin SPL classes into the compiler metadata registry.
///
/// `register` is the pay-for-use decision (see `program_may_reference_spl`). The redeclaration
/// CHECK runs either way and is deliberately outside it: it is a statement about the USER's
/// declarations, not about ours. A program declaring `class SplFileInfo {}` must be told it
/// cannot, whether or not it goes on to reference the builtin — gating the check behind the
/// reference scan let that program compile silently, shadowing a builtin, which is exactly the
/// quiet failure this gate is supposed to be free of. `error_tests::spl_builtins` caught it.
pub(crate) fn inject_builtin_spl_classes(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    register: bool,
) -> Result<(), CompileError> {
    registry::ensure_no_redeclarations(interface_map, class_map)?;
    if !register {
        return Ok(());
    }

    containers::insert_classes(class_map);
    storage::insert_classes(class_map);
    recursive_array::insert_class(class_map);
    forwarding::insert_classes(class_map);
    filters::insert_classes(class_map);
    caching::insert_class(class_map);
    recursive::insert_classes(class_map);
    recursive_iterator_iterator::insert_class(class_map);
    regex::insert_classes(class_map);
    filesystem::insert_classes(class_map);
    append::insert_classes(class_map);
    multiple::insert_class(class_map);
    heaps::insert_classes(class_map);
    object_storage::insert_class(class_map);
    phar::insert_classes(class_map);
    zip::insert_classes(class_map);

    Ok(())
}

/// Patches builtin SPL storage signatures in the compiler metadata registry.
pub(crate) fn patch_builtin_spl_storage_signatures(checker: &mut Checker) {
    patch::patch_builtin_spl_storage_signatures(checker);
}
