//! Purpose:
//! Builds and patches checker metadata for PHP builtin declarations types.
//! Supplies synthetic declarations or contract validation for classes and interfaces that user code may reference.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Dummy AST members carry type contracts only; runtime behavior is implemented elsewhere.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;

use super::exception::{
    builtin_exception_code_property, builtin_exception_constructor_method,
    builtin_exception_get_code_method, builtin_exception_get_message_method,
    builtin_exception_message_property, builtin_throwable_get_message_method,
};
use super::fiber::builtin_fiber_methods;

pub(crate) struct InterfaceDeclInfo {
    pub name: String,
    pub extends: Vec<String>,
    pub methods: Vec<crate::parser::ast::ClassMethod>,
    pub span: crate::span::Span,
    pub constants: Vec<crate::parser::ast::ClassConst>,
}

impl Clone for InterfaceDeclInfo {
    fn clone(&self) -> Self {
        InterfaceDeclInfo {
            name: self.name.clone(),
            extends: self.extends.clone(),
            methods: self.methods.clone(),
            span: self.span,
            constants: self.constants.clone(),
        }
    }
}

pub(crate) fn inject_builtin_throwables(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for builtin_name in [
        "Throwable",
        "Exception",
        "RuntimeException",
        "JsonException",
        "Fiber",
        "FiberError",
    ] {
        let builtin_key = php_symbol_key(builtin_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == builtin_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == builtin_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in type: {}", builtin_name),
            ));
        }
    }

    interface_map.insert(
        "Throwable".to_string(),
        InterfaceDeclInfo {
            name: "Throwable".to_string(),
            extends: Vec::new(),
            methods: vec![builtin_throwable_get_message_method()],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );
    class_map.insert(
        "Exception".to_string(),
        FlattenedClass {
            name: "Exception".to_string(),
            extends: None,
            implements: vec!["Throwable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: vec![
                builtin_exception_message_property(),
                builtin_exception_code_property(),
            ],
            methods: vec![
                builtin_exception_constructor_method(),
                builtin_exception_get_message_method(),
                builtin_exception_get_code_method(),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );
    // RuntimeException and JsonException inherit message + code + constructor
    // + getMessage + getCode from Exception via the standard inheritance
    // machinery; they don't need to redeclare anything locally.
    class_map.insert(
        "RuntimeException".to_string(),
        FlattenedClass {
            name: "RuntimeException".to_string(),
            extends: Some("Exception".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );
    class_map.insert(
        "JsonException".to_string(),
        FlattenedClass {
            name: "JsonException".to_string(),
            extends: Some("RuntimeException".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );

    // Fiber: cooperative coroutine class. Methods are placeholders here — the
    // codegen intercepts every Fiber operation (`new Fiber(...)`, instance
    // methods, `Fiber::suspend`, `Fiber::getCurrent`) and emits direct calls
    // into the `__rt_fiber_*` runtime helpers. Bodies are nominal returns so
    // the type checker sees a well-formed declaration.
    class_map.insert(
        "Fiber".to_string(),
        FlattenedClass {
            name: "Fiber".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: true,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: builtin_fiber_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );

    // FiberError: extends the standard Exception so catch(Exception) and
    // catch(FiberError) both behave per PHP semantics.
    class_map.insert(
        "FiberError".to_string(),
        FlattenedClass {
            name: "FiberError".to_string(),
            extends: Some("Exception".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );

    Ok(())
}
