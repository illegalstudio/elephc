//! Purpose:
//! Builds ReflectionFunction and ReflectionParameter synthetic classes.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Default-value metadata retains object, constant, and by-reference semantics.

use super::*;

/// Builds the `ReflectionFunction` shell with private name/short-name,
/// attribute, and parameter slots. Codegen populates these from the reflected
/// function's signature and attribute metadata.
pub(super) fn builtin_reflection_function() -> FlattenedClass {
    let mut class = builtin_reflection_owner_class(
        "ReflectionFunction",
        true,
        vec![("function", Some(TypeExpr::Str), None, false)],
    );
    if let Some(constructor) = class
        .methods
        .iter_mut()
        .find(|method| method.name == "__construct")
    {
        *constructor = builtin_reflection_function_constructor_method();
    }
    class
}

/// Builds the `ReflectionParameter` shell with private name/position/optional/
/// variadic slots and public accessors, populated at codegen from the reflected
/// function's signature.
pub(super) fn builtin_reflection_parameter() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionParameter".to_string(),
        span: dummy(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties: vec![
            builtin_property("__name", Visibility::Private, Some(TypeExpr::Str), empty_string()),
            builtin_property(
                "__attrs",
                Visibility::Private,
                Some(object_array_type("ReflectionAttribute")),
                empty_array(),
            ),
            builtin_property("__position", Visibility::Private, Some(TypeExpr::Int), int_lit(0)),
            builtin_property(
                "__optional",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__variadic",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_passed_by_reference",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_promoted",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property("__has_type", Visibility::Private, Some(TypeExpr::Bool), bool_lit(false)),
            builtin_property(
                "__allows_null",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(true),
            ),
            builtin_property(
                "__is_array_type",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_callable_type",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property("__type", Visibility::Private, Some(mixed_type()), null_lit()),
            builtin_property("__class", Visibility::Private, Some(mixed_type()), null_lit()),
            builtin_property(
                "__has_default_value",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_default_value_constant",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__default_value_constant_name",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__default_value",
                Visibility::Private,
                Some(mixed_type()),
                null_lit(),
            ),
            builtin_property(
                "__default_value_object_class",
                Visibility::Private,
                Some(TypeExpr::Str),
                empty_string(),
            ),
            builtin_property(
                "__declaring_class",
                Visibility::Private,
                Some(mixed_type()),
                null_lit(),
            ),
            builtin_property(
                "__declaring_function",
                Visibility::Private,
                Some(mixed_type()),
                null_lit(),
            ),
        ],
        methods: vec![
            builtin_reflection_owner_constructor_method(vec![
                ("function", Some(mixed_type()), None, false),
                ("param", Some(mixed_type()), None, false),
            ]),
            builtin_reflection_slot_getter("getName", "__name", TypeExpr::Str),
            builtin_reflection_slot_getter("getPosition", "__position", TypeExpr::Int),
            builtin_reflection_slot_getter("isOptional", "__optional", TypeExpr::Bool),
            builtin_reflection_slot_getter("isVariadic", "__variadic", TypeExpr::Bool),
            builtin_reflection_slot_getter(
                "isPassedByReference",
                "__is_passed_by_reference",
                TypeExpr::Bool,
            ),
            builtin_reflection_parameter_can_be_passed_by_value_method(),
            builtin_reflection_slot_getter("isPromoted", "__is_promoted", TypeExpr::Bool),
            builtin_reflection_slot_getter("hasType", "__has_type", TypeExpr::Bool),
            builtin_reflection_slot_getter("allowsNull", "__allows_null", TypeExpr::Bool),
            builtin_reflection_slot_getter("isArray", "__is_array_type", TypeExpr::Bool),
            builtin_reflection_slot_getter("isCallable", "__is_callable_type", TypeExpr::Bool),
            builtin_reflection_slot_getter("getType", "__type", mixed_type()),
            builtin_reflection_slot_getter("getClass", "__class", mixed_type()),
            builtin_reflection_slot_getter("__toString", "__name", TypeExpr::Str),
            builtin_reflection_owner_get_attributes_method(),
            builtin_reflection_slot_getter(
                "isDefaultValueAvailable",
                "__has_default_value",
                TypeExpr::Bool,
            ),
            builtin_reflection_parameter_is_default_value_constant_method(),
            builtin_reflection_parameter_get_default_value_constant_name_method(),
            builtin_reflection_parameter_get_default_value_method(),
            builtin_reflection_slot_getter("getDeclaringClass", "__declaring_class", mixed_type()),
            builtin_reflection_slot_getter(
                "getDeclaringFunction",
                "__declaring_function",
                mixed_type(),
            ),
        ],
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds `ReflectionParameter::getDefaultValue()` over the retained default slot.
pub(super) fn builtin_reflection_parameter_get_default_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let object_default_body = reflection_parameter_get_default_object_body(dummy_span);
    ClassMethod {
        name: "getDefaultValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![
            reflection_parameter_throw_if_default_missing(dummy_span),
            Stmt::new(
                StmtKind::If {
                    condition: binary_expr(
                        reflection_this_property("__default_value_object_class", dummy_span),
                        BinOp::NotEq,
                        string_lit("", dummy_span),
                        dummy_span,
                    ),
                    then_body: object_default_body,
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(reflection_this_property(
                    "__default_value",
                    dummy_span,
                ))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds the object-default branch for `ReflectionParameter::getDefaultValue()`.
pub(super) fn reflection_parameter_get_default_object_body(span: crate::span::Span) -> Vec<Stmt> {
    let arg_count_var = "__default_value_arg_count";
    let mut body = vec![
        Stmt::new(
            StmtKind::If {
                condition: binary_expr(
                    reflection_this_property("__default_value", span),
                    BinOp::StrictEq,
                    null_value(span),
                    span,
                ),
                then_body: vec![reflection_parameter_return_dynamic_default_object(
                    Vec::new(),
                    span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ),
        Stmt::new(
            StmtKind::Assign {
                name: arg_count_var.to_string(),
                value: function_call(
                    "count",
                    vec![reflection_this_property("__default_value", span)],
                    span,
                ),
            },
            span,
        ),
    ];
    for arg_count in 1..=8 {
        body.push(reflection_parameter_default_object_arg_count_branch(
            arg_count,
            arg_count_var,
            span,
        ));
    }
    body.push(throw_new_reflection_exception(
        string_lit("Internal error: Failed to retrieve the default value", span),
        span,
    ));
    body
}

/// Builds one constructor-argument-count branch for object defaults.
pub(super) fn reflection_parameter_default_object_arg_count_branch(
    arg_count: usize,
    arg_count_var: &str,
    span: crate::span::Span,
) -> Stmt {
    let args = (0..arg_count)
        .map(|index| reflection_parameter_default_object_arg(index, span))
        .collect();
    Stmt::new(
        StmtKind::If {
            condition: binary_expr(
                variable_expr(arg_count_var, span),
                BinOp::StrictEq,
                Expr::new(ExprKind::IntLiteral(arg_count as i64), span),
                span,
            ),
            then_body: vec![reflection_parameter_return_dynamic_default_object(args, span)],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds a return statement that constructs the retained object default class.
pub(super) fn reflection_parameter_return_dynamic_default_object(
    args: Vec<Expr>,
    span: crate::span::Span,
) -> Stmt {
    Stmt::new(
        StmtKind::Return(Some(Expr::new(
            ExprKind::NewDynamic {
                name_expr: Box::new(reflection_this_property(
                    "__default_value_object_class",
                    span,
                )),
                args,
            },
            span,
        ))),
        span,
    )
}

/// Builds `$this->__default_value[$index]` for retained object-default args.
pub(super) fn reflection_parameter_default_object_arg(index: usize, span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(reflection_this_property("__default_value", span)),
            index: Box::new(Expr::new(ExprKind::IntLiteral(index as i64), span)),
        },
        span,
    )
}

/// Returns a public `ReflectionClass::newInstanceWithoutConstructor()` method.
///
/// Eval dispatch supplies the real constructorless allocation. The body remains
/// a conservative placeholder so the built-in class metadata exposes the PHP
/// method without forcing ordinary method lowering to construct a class.
pub(super) fn builtin_reflection_class_new_instance_without_constructor_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "newInstanceWithoutConstructor".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::Null, dummy_span))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds `ReflectionParameter::isDefaultValueConstant()` over retained default metadata.
pub(super) fn builtin_reflection_parameter_is_default_value_constant_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "isDefaultValueConstant".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![
            reflection_parameter_throw_if_default_missing(dummy_span),
            Stmt::new(
                StmtKind::Return(Some(reflection_this_property(
                    "__is_default_value_constant",
                    dummy_span,
                ))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds `ReflectionParameter::getDefaultValueConstantName()` over retained default metadata.
pub(super) fn builtin_reflection_parameter_get_default_value_constant_name_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "getDefaultValueConstantName".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(mixed_type()),
        by_ref_return: false,
        body: vec![
            reflection_parameter_throw_if_default_missing(dummy_span),
            Stmt::new(
                StmtKind::If {
                    condition: Expr::new(
                        ExprKind::Not(Box::new(reflection_this_property(
                            "__is_default_value_constant",
                            dummy_span,
                        ))),
                        dummy_span,
                    ),
                    then_body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(ExprKind::Null, dummy_span))),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(reflection_this_property(
                    "__default_value_constant_name",
                    dummy_span,
                ))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds the PHP-compatible default-metadata guard shared by ReflectionParameter methods.
pub(super) fn reflection_parameter_throw_if_default_missing(span: crate::span::Span) -> Stmt {
    Stmt::new(
        StmtKind::If {
            condition: Expr::new(
                ExprKind::Not(Box::new(reflection_this_property(
                    "__has_default_value",
                    span,
                ))),
                span,
            ),
            then_body: vec![throw_new_reflection_exception(
                string_lit("Internal error: Failed to retrieve the default value", span),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Builds `ReflectionParameter::canBePassedByValue()` from the retained by-ref flag.
pub(super) fn builtin_reflection_parameter_can_be_passed_by_value_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        name: "canBePassedByValue".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(bool_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::Not(Box::new(reflection_this_property(
                    "__is_passed_by_reference",
                    dummy_span,
                ))),
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}
