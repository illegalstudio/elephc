//! Purpose:
//! Owns the public builtin class-name registry used while injecting checker metadata.
//! Performs redeclaration checks before synthetic classes are added.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - The lists mirror public builtin classes, not private compiler helper classes.
//! - Name comparison uses PHP's case-insensitive symbol keying.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;

use super::super::builtin_types::InterfaceDeclInfo;

pub(super) const SPL_CLASS_NAMES: &[&str] = &[
    "SplDoublyLinkedList",
    "SplStack",
    "SplQueue",
    "SplFixedArray",
    "EmptyIterator",
    "InternalIterator",
    "ArrayIterator",
    "RecursiveArrayIterator",
    "ArrayObject",
    "IteratorIterator",
    "LimitIterator",
    "NoRewindIterator",
    "InfiniteIterator",
    "FilterIterator",
    "CallbackFilterIterator",
    "CachingIterator",
    "RecursiveFilterIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveIteratorIterator",
    "ParentIterator",
    "RegexIterator",
    "RecursiveRegexIterator",
    "SplFileInfo",
    "SplFileObject",
    "SplTempFileObject",
    "DirectoryIterator",
    "FilesystemIterator",
    "GlobIterator",
    "RecursiveDirectoryIterator",
    "RecursiveCachingIterator",
    "AppendIterator",
    "MultipleIterator",
    "SplHeap",
    "SplMaxHeap",
    "SplMinHeap",
    "SplPriorityQueue",
    "SplObjectStorage",
];

const PHAR_CLASS_NAMES: &[&str] = &["Phar", "PharData", "PharFileInfo"];

/// Ensures no redeclarations is available before the caller continues.
pub(super) fn ensure_no_redeclarations(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    ensure_no_class_redeclarations(
        interface_map,
        class_map,
        SPL_CLASS_NAMES,
        "Cannot redeclare built-in SPL class",
    )?;
    ensure_no_class_redeclarations(
        interface_map,
        class_map,
        PHAR_CLASS_NAMES,
        "Cannot redeclare built-in class",
    )
}

/// Checks one public builtin-class family for user/interface redeclarations.
fn ensure_no_class_redeclarations(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
    class_names: &[&str],
    message_prefix: &str,
) -> Result<(), CompileError> {
    for class_name in class_names {
        let class_key = php_symbol_key(class_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == class_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == class_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("{}: {}", message_prefix, class_name),
            ));
        }
    }

    Ok(())
}
