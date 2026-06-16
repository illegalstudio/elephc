//! Purpose:
//! Injects the PHP `php_user_filter` builtin base class into the checker schema.
//! Provides the inherited `$params` property used by userspace stream filters.
//!
//! Called from:
//! - `crate::types::checker::driver` during builtin type/schema initialization.
//!
//! Key details:
//! - The runtime seeds `php_user_filter::$params` before calling `onCreate()`,
//!   and object GC owns the boxed Mixed value after it is stored in the slot.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{ClassProperty, Expr, ExprKind, PropertyHooks, TypeExpr, Visibility};
use crate::span::Span;
use crate::types::traits::FlattenedClass;

/// Injects the PHP `php_user_filter` builtin so user filter classes can extend
/// it and read the runtime-seeded public `$params` property.
pub(crate) fn inject_builtin_user_filter(
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    let builtin_key = php_symbol_key("php_user_filter");
    if class_map
        .keys()
        .any(|name| php_symbol_key(name) == builtin_key)
    {
        return Err(CompileError::new(
            Span::dummy(),
            "Cannot redeclare built-in class: php_user_filter",
        ));
    }

    class_map.insert(
        "php_user_filter".to_string(),
        FlattenedClass {
            name: "php_user_filter".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: vec![params_property()],
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
        },
    );

    Ok(())
}

/// Builds the synthetic public `mixed $params = null` property inherited by
/// userspace stream-filter classes.
fn params_property() -> ClassProperty {
    ClassProperty {
        name: "params".to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(Expr::new(ExprKind::Null, Span::dummy())),
        span: Span::dummy(),
        attributes: Vec::new(),
    }
}
