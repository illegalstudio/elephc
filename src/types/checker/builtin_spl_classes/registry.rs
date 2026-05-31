//! Purpose:
//! Owns the public SPL class-name registry used while injecting checker metadata.
//! Performs redeclaration checks before synthetic classes are added.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - The list mirrors public SPL classes, not private compiler helper classes.
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

/// Ensures no redeclarations is available before the caller continues.
pub(super) fn ensure_no_redeclarations(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for class_name in SPL_CLASS_NAMES {
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
                &format!("Cannot redeclare built-in SPL class: {}", class_name),
            ));
        }
    }

    Ok(())
}
