//! Purpose:
//! Injects PHP core object classes into the checker schema.
//! Gives `stdClass` and `__PHP_Incomplete_Class` nominal entries for object operations and type checks.
//!
//! Called from:
//! - `crate::types::checker::driver` during builtin type/schema initialization.
//!
//! Key details:
//! - `stdClass` has no declared properties; property reads and writes are typed as `mixed` and handled by runtime hash helpers.
//! - `__PHP_Incomplete_Class` is an internal nominal type produced by deserialization and is not constructible by user code.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;

/// Injects PHP's core object classes so their type hints and `instanceof` checks resolve.
///
/// `stdClass` is a special builtin: it has no statically declared properties,
/// yet user code can read or write any property name on instances. The
/// type-checker treats `stdClass` property access as `mixed`, and codegen
/// routes property reads/writes through `__rt_stdclass_get` /
/// `__rt_stdclass_set` so the underlying hash table stores arbitrary names at
/// runtime.
///
/// `__PHP_Incomplete_Class` is represented as an empty nominal class because PHP creates it only
/// while deserializing unavailable classes; static construction remains unavailable to user code.
pub(crate) fn inject_builtin_core_object_classes(
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for class_name in ["stdClass", "__PHP_Incomplete_Class"] {
        let builtin_key = php_symbol_key(class_name);
        if class_map
            .keys()
            .any(|name| php_symbol_key(name) == builtin_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in class: {class_name}"),
            ));
        }

        class_map.insert(
            class_name.to_string(),
            FlattenedClass {
                name: class_name.to_string(),
                span: crate::span::Span::dummy(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: Vec::new(),
                methods: Vec::new(),
                attributes: Vec::new(),
                constants: Vec::new(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            },
        );
    }

    Ok(())
}

/// Returns true if `class_name` refers to the built-in stdClass.
///
/// PHP class names are case-insensitive for the purposes of this kind of
/// dispatch, but elephc canonicalizes builtin class names to their declared
/// spelling before hitting any code that calls into here. Compare on the
/// canonical form.
pub fn is_stdclass(class_name: &str) -> bool {
    class_name == "stdClass"
}
