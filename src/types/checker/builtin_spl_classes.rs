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

    Ok(())
}

/// Injects the narrow SPL schema needed before any synthetic iterator body can construct it.
///
/// DOM bridge activation is discovered while type-checking expressions, after ordinary builtin
/// class registration has already run. `InternalIterator` declares `SplFixedArray` owner slots
/// and synthetic helpers, so both classes must be present before class-info construction. This
/// avoids a second, incomplete source scan while leaving unrelated SPL classes gated. Runtime
/// metadata remains reachability-filtered, so a program that never constructs the iterator pays
/// no emitted class-table cost.
pub(crate) fn inject_builtin_internal_iterator(
    class_map: &mut HashMap<String, FlattenedClass>,
) {
    containers::insert_spl_fixed_array(class_map);
    containers::insert_internal_iterator(class_map);
}

/// Patches builtin SPL storage signatures in the compiler metadata registry.
pub(crate) fn patch_builtin_spl_storage_signatures(checker: &mut Checker) {
    patch::patch_builtin_spl_storage_signatures(checker);
}
