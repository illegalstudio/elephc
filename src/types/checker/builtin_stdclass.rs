//! Purpose:
//! Injects the PHP `stdClass` builtin into the checker schema.
//! Gives `new stdClass()` and default `json_decode()` object results a nominal class entry with dynamic-property behavior.
//!
//! Called from:
//! - `crate::types::checker::driver` during builtin type/schema initialization.
//!
//! Key details:
//! - `stdClass` has no declared properties; property reads and writes are typed as `mixed` and handled by runtime hash helpers.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;

/// Inject the PHP `stdClass` builtin so that `new stdClass()`, `instanceof
/// stdClass`, and the default object form returned by `json_decode($json)` all
/// type-check.
///
/// `stdClass` is a special builtin: it has no statically declared properties,
/// yet user code can read or write any property name on instances. The
/// type-checker treats `stdClass` property access as `mixed`, and codegen
/// routes property reads/writes through `__rt_stdclass_get` /
/// `__rt_stdclass_set` so the underlying hash table stores arbitrary names at
/// runtime.
pub(crate) fn inject_builtin_stdclass(
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    let builtin_key = php_symbol_key("stdClass");
    if class_map
        .keys()
        .any(|name| php_symbol_key(name) == builtin_key)
    {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "Cannot redeclare built-in class: stdClass",
        ));
    }

    class_map.insert(
        "stdClass".to_string(),
        FlattenedClass {
            name: "stdClass".to_string(),
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
        },
    );

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
