use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;
use crate::types::PhpType;

use super::Checker;

pub(crate) struct InterfaceDeclInfo {
    pub name: String,
    pub extends: Vec<String>,
    pub methods: Vec<crate::parser::ast::ClassMethod>,
    pub span: crate::span::Span,
}

impl Clone for InterfaceDeclInfo {
    fn clone(&self) -> Self {
        InterfaceDeclInfo {
            name: self.name.clone(),
            extends: self.extends.clone(),
            methods: self.methods.clone(),
            span: self.span,
        }
    }
}

pub(crate) fn inject_builtin_throwables(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for builtin_name in ["Throwable", "Exception"] {
        if interface_map.contains_key(builtin_name) || class_map.contains_key(builtin_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in exception type: {}", builtin_name),
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
            properties: vec![builtin_exception_message_property()],
            methods: vec![
                builtin_exception_constructor_method(),
                builtin_exception_get_message_method(),
            ],
        },
    );

    Ok(())
}

fn builtin_exception_message_property() -> ClassProperty {
    ClassProperty {
        name: "message".to_string(),
        visibility: Visibility::Public,
        type_expr: Some(TypeExpr::Str),
        readonly: false,
        is_final: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        )),
        span: crate::span::Span::dummy(),
    }
}

fn builtin_exception_constructor_method() -> ClassMethod {
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "message".to_string(),
            None,
            Some(Expr::new(
                ExprKind::StringLiteral(String::new()),
                crate::span::Span::dummy(),
            )),
            false,
        )],
        variadic: None,
        return_type: None,
        body: vec![Stmt::new(
            StmtKind::PropertyAssign {
                object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                property: "message".to_string(),
                value: Expr::new(
                    ExprKind::Variable("message".to_string()),
                    crate::span::Span::dummy(),
                ),
            },
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
    }
}

fn builtin_exception_get_message_method() -> ClassMethod {
    ClassMethod {
        name: "getMessage".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: None,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "message".to_string(),
                },
                crate::span::Span::dummy(),
            ))),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
    }
}

fn builtin_throwable_get_message_method() -> ClassMethod {
    ClassMethod {
        name: "getMessage".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params: Vec::new(),
        variadic: None,
        return_type: None,
        body: Vec::new(),
        span: crate::span::Span::dummy(),
    }
}

pub(crate) fn patch_builtin_exception_signatures(checker: &mut Checker) {
    if let Some(interface_info) = checker.interfaces.get_mut("Throwable") {
        if let Some(sig) = interface_info.methods.get_mut("getMessage") {
            sig.return_type = PhpType::Str;
        }
    }
    if let Some(class_info) = checker.classes.get_mut("Exception") {
        if let Some(sig) = class_info.methods.get_mut("__construct") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
            sig.return_type = PhpType::Void;
        }
        if let Some(sig) = class_info.methods.get_mut("getMessage") {
            sig.return_type = PhpType::Str;
        }
    }
}

pub(crate) fn patch_magic_method_signatures(checker: &mut Checker) {
    for class_info in checker.classes.values_mut() {
        if let Some(sig) = class_info.methods.get_mut("__get") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
        }
        if let Some(sig) = class_info.methods.get_mut("__set") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
            if let Some(param) = sig.params.get_mut(1) {
                param.1 = PhpType::Mixed;
            }
        }
    }
}

pub(crate) fn validate_magic_method_contracts(checker: &Checker) -> Result<(), CompileError> {
    let mut errors = Vec::new();
    for (class_name, class_info) in &checker.classes {
        for method in &class_info.method_decls {
            match method.name.as_str() {
                "__toString" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must be non-static: {}::__toString",
                                class_name
                            ),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__toString", class_name),
                        ));
                        continue;
                    }
                    if !method.params.is_empty() || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must take 0 arguments: {}::__toString",
                                class_name
                            ),
                        ));
                        continue;
                    }
                    if class_info
                        .methods
                        .get("__toString")
                        .map(|sig| sig.return_type.clone())
                        != Some(PhpType::Str)
                    {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must return string: {}::__toString",
                                class_name
                            ),
                        ));
                    }
                }
                "__get" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__get", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__get", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 1 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 1 argument: {}::__get", class_name),
                        ));
                    }
                }
                "__set" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__set", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__set", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 2 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 2 arguments: {}::__set", class_name),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::from_many(errors))
    }
}
