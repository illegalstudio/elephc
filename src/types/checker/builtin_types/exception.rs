//! Purpose:
//! Builds and patches checker metadata for PHP builtin exception types.
//! Supplies synthetic declarations or contract validation for classes and interfaces that user code may reference.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Dummy AST members carry type contracts only; runtime behavior is implemented elsewhere.

use crate::names::{Name, php_symbol_key};
use crate::parser::ast::{
    ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind, TypeExpr,
    Visibility,
};
use crate::types::PhpType;

use super::super::Checker;

/// Returns a synthetic `ClassProperty` AST node for the `message` property of builtin Exception classes.
/// The property is public, typed `string`, with an empty string default value.
pub(super) fn builtin_exception_message_property() -> ClassProperty {
    ClassProperty {
        name: "message".to_string(),
        visibility: Visibility::Public,
        type_expr: Some(TypeExpr::Str),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        )),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a synthetic `ClassMethod` AST node for the `__construct` method of builtin Exception classes.
/// Takes `message` (string, default `""`) and `code` (int, default `0`) parameters and assigns them to the corresponding properties.
pub(super) fn builtin_exception_constructor_method() -> ClassMethod {
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "message".to_string(),
                None,
                Some(Expr::new(
                    ExprKind::StringLiteral(String::new()),
                    crate::span::Span::dummy(),
                )),
                false,
            ),
            (
                "code".to_string(),
                None,
                Some(Expr::new(
                    ExprKind::IntLiteral(0),
                    crate::span::Span::dummy(),
                )),
                false,
            ),
        ],
        variadic: None,
        return_type: None,
        body: vec![
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "message".to_string(),
                    value: Expr::new(
                        ExprKind::Variable("message".to_string()),
                        crate::span::Span::dummy(),
                    ),
                },
                crate::span::Span::dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                    property: "code".to_string(),
                    value: Expr::new(
                        ExprKind::Variable("code".to_string()),
                        crate::span::Span::dummy(),
                    ),
                },
                crate::span::Span::dummy(),
            ),
        ],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a synthetic `ClassProperty` AST node for the `code` property of builtin Exception classes.
/// The property is protected, typed `int`, with a `0` default value.
pub(super) fn builtin_exception_code_property() -> ClassProperty {
    ClassProperty {
        name: "code".to_string(),
        visibility: Visibility::Protected,
        type_expr: Some(TypeExpr::Int),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(Expr::new(
            ExprKind::IntLiteral(0),
            crate::span::Span::dummy(),
        )),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns a synthetic `ClassMethod` for `Exception::getCode()`.
/// Body returns `$this->code` cast to `int`.
pub(super) fn builtin_exception_get_code_method() -> ClassMethod {
    concrete_throwable_method(
        "getCode",
        TypeExpr::Int,
        Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                property: "code".to_string(),
            },
            crate::span::Span::dummy(),
        ),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::getMessage()`.
/// Body returns `$this->message` typed as `string`.
pub(super) fn builtin_exception_get_message_method() -> ClassMethod {
    concrete_throwable_method(
        "getMessage",
        TypeExpr::Str,
        Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                property: "message".to_string(),
            },
            crate::span::Span::dummy(),
        ),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::getFile()`.
/// Body returns a dummy empty string; the real file path is injected at runtime by the compiler.
pub(super) fn builtin_exception_get_file_method() -> ClassMethod {
    concrete_throwable_method(
        "getFile",
        TypeExpr::Str,
        Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        ),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::getLine()`.
/// Body returns a dummy `0`; the real line number is injected at runtime by the compiler.
pub(super) fn builtin_exception_get_line_method() -> ClassMethod {
    concrete_throwable_method(
        "getLine",
        TypeExpr::Int,
        Expr::new(ExprKind::IntLiteral(0), crate::span::Span::dummy()),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::getTrace()`.
/// Body returns an empty array; the real backtrace is built at runtime by the compiler.
pub(super) fn builtin_exception_get_trace_method() -> ClassMethod {
    concrete_throwable_method(
        "getTrace",
        array_type(),
        Expr::new(ExprKind::ArrayLiteral(Vec::new()), crate::span::Span::dummy()),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::getTraceAsString()`.
/// Body returns a dummy empty string; the real trace string is built at runtime by the compiler.
pub(super) fn builtin_exception_get_trace_as_string_method() -> ClassMethod {
    concrete_throwable_method(
        "getTraceAsString",
        TypeExpr::Str,
        Expr::new(
            ExprKind::StringLiteral(String::new()),
            crate::span::Span::dummy(),
        ),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::getPrevious()`.
/// Return type is `?Throwable`; body returns `null` in the dummy AST.
pub(super) fn builtin_exception_get_previous_method() -> ClassMethod {
    concrete_throwable_method(
        "getPrevious",
        nullable_throwable_type(),
        Expr::new(ExprKind::Null, crate::span::Span::dummy()),
    )
}

/// Returns a synthetic `ClassMethod` for `Exception::__toString()`.
/// Body returns `$this->message` cast to `string`.
pub(super) fn builtin_exception_to_string_method() -> ClassMethod {
    concrete_throwable_method(
        "__toString",
        TypeExpr::Str,
        Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(Expr::new(ExprKind::This, crate::span::Span::dummy())),
                property: "message".to_string(),
            },
            crate::span::Span::dummy(),
        ),
    )
}

/// Returns the list of abstract `ClassMethod` nodes for the Throwable interface methods:
/// getMessage, getCode, getFile, getLine, getTrace, getTraceAsString, getPrevious, __toString.
pub(super) fn builtin_throwable_methods() -> Vec<ClassMethod> {
    vec![
        abstract_throwable_method("getMessage", TypeExpr::Str),
        abstract_throwable_method("getCode", TypeExpr::Int),
        abstract_throwable_method("getFile", TypeExpr::Str),
        abstract_throwable_method("getLine", TypeExpr::Int),
        abstract_throwable_method("getTrace", array_type()),
        abstract_throwable_method("getTraceAsString", TypeExpr::Str),
        abstract_throwable_method("getPrevious", nullable_throwable_type()),
        abstract_throwable_method("__toString", TypeExpr::Str),
    ]
}

/// Builds a concrete (body-bearing) throwable method with the given name, return type, and return value expression.
fn concrete_throwable_method(name: &str, return_type: TypeExpr, value: Expr) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: Some(return_type),
        body: vec![Stmt::new(
            StmtKind::Return(Some(value)),
            crate::span::Span::dummy(),
        )],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Builds an abstract method declaration for the Throwable interface with the given name and return type.
fn abstract_throwable_method(name: &str, return_type: TypeExpr) -> ClassMethod {
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

/// Returns a `TypeExpr::Named("array")` used as the return type for `getTrace()`.
fn array_type() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("array"))
}

/// Returns a nullable `TypeExpr` wrapping `Throwable`; used as the return type for `getPrevious()`.
fn nullable_throwable_type() -> TypeExpr {
    TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified("Throwable"))))
}

/// Patches the checker metadata for the Throwable interface and all builtin exception classes.
/// Updates return types for getter methods and the `__construct` parameter types for Error, TypeError,
/// ValueError, Exception, RuntimeException, JsonException, and FiberError.
pub(crate) fn patch_builtin_exception_signatures(checker: &mut Checker) {
    let nullable_throwable =
        checker.normalize_union_type(vec![PhpType::Object("Throwable".to_string()), PhpType::Void]);
    if let Some(interface_info) = checker.interfaces.get_mut("Throwable") {
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getMessage")) {
            sig.return_type = PhpType::Str;
        }
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getCode")) {
            sig.return_type = PhpType::Int;
        }
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getFile")) {
            sig.return_type = PhpType::Str;
        }
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getLine")) {
            sig.return_type = PhpType::Int;
        }
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getTrace")) {
            sig.return_type = PhpType::Array(Box::new(PhpType::Mixed));
        }
        if let Some(sig) = interface_info
            .methods
            .get_mut(&php_symbol_key("getTraceAsString"))
        {
            sig.return_type = PhpType::Str;
        }
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("getPrevious")) {
            sig.return_type = nullable_throwable.clone();
        }
        if let Some(sig) = interface_info.methods.get_mut(&php_symbol_key("__toString")) {
            sig.return_type = PhpType::Str;
        }
    }
    for class_name in [
        "Error",
        "TypeError",
        "ValueError",
        "Exception",
        "RuntimeException",
        "JsonException",
        "FiberError",
    ] {
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            if let Some(sig) = class_info.methods.get_mut("__construct") {
                if let Some(param) = sig.params.get_mut(0) {
                    param.1 = PhpType::Str;
                }
                if let Some(param) = sig.params.get_mut(1) {
                    param.1 = PhpType::Int;
                }
                sig.return_type = PhpType::Void;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getMessage")) {
                sig.return_type = PhpType::Str;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getCode")) {
                sig.return_type = PhpType::Int;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getFile")) {
                sig.return_type = PhpType::Str;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getLine")) {
                sig.return_type = PhpType::Int;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getTrace")) {
                sig.return_type = PhpType::Array(Box::new(PhpType::Mixed));
            }
            if let Some(sig) = class_info
                .methods
                .get_mut(&php_symbol_key("getTraceAsString"))
            {
                sig.return_type = PhpType::Str;
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getPrevious")) {
                sig.return_type = nullable_throwable.clone();
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("__toString")) {
                sig.return_type = PhpType::Str;
            }
        }
    }
}
