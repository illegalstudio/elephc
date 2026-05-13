//! Purpose:
//! Injects PHP iterator interfaces and helper methods into checker metadata.
//! Provides builtin iterable contracts for classes before trait and interface validation run.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Injected names must obey PHP case-insensitive collision rules and share flattened class metadata.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::builtin_types::InterfaceDeclInfo;
use super::Checker;

pub(crate) fn inject_builtin_iterators(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for builtin_name in ["Iterator", "IteratorAggregate", "Generator"] {
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
                &format!("Cannot redeclare built-in interface: {}", builtin_name),
            ));
        }
    }

    interface_map.insert(
        "Iterator".to_string(),
        InterfaceDeclInfo {
            name: "Iterator".to_string(),
            extends: Vec::new(),
            methods: vec![
                builtin_iterator_method("current", TypeExpr::Named(Name::unqualified("mixed"))),
                builtin_iterator_method("key", TypeExpr::Named(Name::unqualified("mixed"))),
                builtin_iterator_method("next", TypeExpr::Void),
                builtin_iterator_method("valid", TypeExpr::Bool),
                builtin_iterator_method("rewind", TypeExpr::Void),
            ],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "IteratorAggregate".to_string(),
        InterfaceDeclInfo {
            name: "IteratorAggregate".to_string(),
            extends: Vec::new(),
            methods: vec![builtin_iterator_method(
                "getIterator",
                TypeExpr::Named(Name::unqualified("Iterator")),
            )],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    class_map.insert(
        "Generator".to_string(),
        FlattenedClass {
            name: "Generator".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string()],
            is_abstract: false,
            is_final: true,
            is_readonly_class: false,
            properties: Vec::<ClassProperty>::new(),
            methods: vec![
                stub_method_returning_null("current"),
                stub_method_returning_null("key"),
                stub_void_method("next"),
                stub_method_returning_false("valid"),
                stub_void_method("rewind"),
                stub_method_returning_null_with_param("send", "value"),
                stub_method_returning_null_with_param("throw", "exception"),
                stub_method_returning_null("getReturn"),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
        },
    );

    Ok(())
}

fn builtin_iterator_method(name: &str, return_type: TypeExpr) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params: Vec::new(),
        variadic: None,
        return_type: Some(return_type),
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// A stub method whose body is `return null;`. Used for the `Generator`
/// built-in class — codegen special-cases each Generator method to dispatch
/// directly to `__rt_gen_*` runtime helpers; this body is only here so that
/// type-check sees a concrete return value compatible with `mixed`.
fn stub_method_returning_null(name: &str) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::Null, crate::span::Span::dummy()))),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn stub_method_returning_null_with_param(name: &str, param: &str) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(param.to_string(), None, None, false)],
        variadic: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::Null, crate::span::Span::dummy()))),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn stub_method_returning_false(name: &str) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: Some(TypeExpr::Bool),
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::BoolLiteral(false),
                crate::span::Span::dummy(),
            ))),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

fn stub_void_method(name: &str) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: Some(TypeExpr::Void),
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

pub(crate) fn patch_builtin_generator_signatures(checker: &mut Checker) {
    if let Some(class_info) = checker.classes.get_mut("Generator") {
        for (name, ty) in &[
            ("current", PhpType::Mixed),
            ("key", PhpType::Mixed),
            ("next", PhpType::Void),
            ("valid", PhpType::Bool),
            ("rewind", PhpType::Void),
            ("send", PhpType::Mixed),
            ("throw", PhpType::Mixed),
            ("getReturn", PhpType::Mixed),
        ] {
            if let Some(sig) = class_info.methods.get_mut(*name) {
                sig.return_type = ty.clone();
            }
        }
        if let Some(sig) = class_info.methods.get_mut("send") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Mixed;
            }
        }
        if let Some(sig) = class_info.methods.get_mut("throw") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Object("Throwable".to_string());
            }
        }
    }
}
