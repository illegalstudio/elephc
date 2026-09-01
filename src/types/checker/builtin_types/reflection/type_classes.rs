//! Purpose:
//! Builds named, union, and intersection Reflection type classes.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - String rendering and nullability mirror the retained type metadata.

use super::*;

/// Builds the `ReflectionNamedType` shell: a parameter/return type rendered as a
/// runtime object with a name, nullability flag, and builtin flag. Populated at
/// codegen from the declared type.
pub(super) fn builtin_reflection_named_type() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionNamedType".to_string(),
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
            builtin_property(
                "__allows_null",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_builtin",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
        ],
        methods: vec![
            builtin_reflection_private_constructor_method(),
            builtin_reflection_slot_getter("getName", "__name", TypeExpr::Str),
            builtin_reflection_named_type_to_string_method(),
            builtin_reflection_slot_getter("allowsNull", "__allows_null", TypeExpr::Bool),
            builtin_reflection_slot_getter("isBuiltin", "__is_builtin", TypeExpr::Bool),
        ],
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds the `ReflectionUnionType` shell returned for supported union hints.
pub(super) fn builtin_reflection_union_type() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionUnionType".to_string(),
        span: dummy(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties: vec![
            builtin_property(
                "__types",
                Visibility::Private,
                Some(object_array_type("ReflectionNamedType")),
                empty_array(),
            ),
            builtin_property(
                "__attrs",
                Visibility::Private,
                Some(object_array_type("ReflectionAttribute")),
                empty_array(),
            ),
            builtin_property(
                "__allows_null",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_builtin",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
        ],
        methods: vec![
            builtin_reflection_private_constructor_method(),
            builtin_reflection_class_array_method(
                "getTypes",
                "__types",
                object_array_type("ReflectionNamedType"),
            ),
            builtin_reflection_composite_type_string_method("__toString", "|", true),
            builtin_reflection_composite_type_string_method("getName", "|", true),
            builtin_reflection_class_bool_method("allowsNull", "__allows_null"),
            builtin_reflection_class_bool_method("isBuiltin", "__is_builtin"),
        ],
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds the `ReflectionIntersectionType` shell returned for supported intersection hints.
pub(super) fn builtin_reflection_intersection_type() -> FlattenedClass {
    FlattenedClass {
        name: "ReflectionIntersectionType".to_string(),
        span: dummy(),
        extends: None,
        implements: Vec::new(),
        is_abstract: false,
        is_final: true,
        is_readonly_class: false,
        properties: vec![
            builtin_property(
                "__types",
                Visibility::Private,
                Some(object_array_type("ReflectionNamedType")),
                empty_array(),
            ),
            builtin_property(
                "__attrs",
                Visibility::Private,
                Some(object_array_type("ReflectionAttribute")),
                empty_array(),
            ),
            builtin_property(
                "__allows_null",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
            builtin_property(
                "__is_builtin",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ),
        ],
        methods: vec![
            builtin_reflection_private_constructor_method(),
            builtin_reflection_class_array_method(
                "getTypes",
                "__types",
                object_array_type("ReflectionNamedType"),
            ),
            builtin_reflection_composite_type_string_method("__toString", "&", false),
            builtin_reflection_composite_type_string_method("getName", "&", false),
            builtin_reflection_class_bool_method("allowsNull", "__allows_null"),
            builtin_reflection_class_bool_method("isBuiltin", "__is_builtin"),
        ],
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds `ReflectionNamedType::__toString()` from retained name/nullability slots.
pub(super) fn builtin_reflection_named_type_to_string_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let name = reflection_this_property("__name", dummy_span);
    let nullable_named_type = binary_expr(
        reflection_this_property("__allows_null", dummy_span),
        BinOp::And,
        binary_expr(
            name.clone(),
            BinOp::StrictNotEq,
            string_lit("mixed", dummy_span),
            dummy_span,
        ),
        dummy_span,
    );
    ClassMethod {
        name: "__toString".to_string(),
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
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::If {
                    condition: nullable_named_type,
                    then_body: vec![Stmt::new(
                        StmtKind::Return(Some(concat_expr(
                            string_lit("?", dummy_span),
                            name.clone(),
                            dummy_span,
                        ))),
                        dummy_span,
                    )],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(Some(name)), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds a string-rendering method for `ReflectionUnionType` and `ReflectionIntersectionType`.
pub(super) fn builtin_reflection_composite_type_string_method(
    method_name: &str,
    separator: &'static str,
    append_null: bool,
) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let mut body = vec![
        Stmt::new(
            StmtKind::TypedAssign {
                type_expr: TypeExpr::Str,
                name: "result".to_string(),
                value: string_lit("", dummy_span),
            },
            dummy_span,
        ),
        Stmt::new(
            StmtKind::Foreach {
                array: reflection_this_property("__types", dummy_span),
                key_var: None,
                value_var: "type".to_string(),
                value_by_ref: false,
                body: reflection_composite_type_append_body(
                    method_call_expr(
                        variable_expr("type", dummy_span),
                        "getName",
                        Vec::new(),
                        dummy_span,
                    ),
                    separator,
                    dummy_span,
                ),
            },
            dummy_span,
        ),
    ];
    if append_null {
        body.push(Stmt::new(
            StmtKind::If {
                condition: reflection_this_property("__allows_null", dummy_span),
                then_body: reflection_composite_type_append_body(
                    string_lit("null", dummy_span),
                    separator,
                    dummy_span,
                ),
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            dummy_span,
        ));
    }
    body.push(Stmt::new(
        StmtKind::Return(Some(variable_expr("result", dummy_span))),
        dummy_span,
    ));
    ClassMethod {
        name: method_name.to_string(),
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
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds the statements that append one rendered type segment to `$result`.
pub(super) fn reflection_composite_type_append_body(
    value: Expr,
    separator: &'static str,
    span: crate::span::Span,
) -> Vec<Stmt> {
    vec![
        Stmt::new(
            StmtKind::If {
                condition: binary_expr(
                    variable_expr("result", span),
                    BinOp::StrictNotEq,
                    string_lit("", span),
                    span,
                ),
                then_body: vec![Stmt::new(
                    StmtKind::Assign {
                        name: "result".to_string(),
                        value: concat_expr(
                            variable_expr("result", span),
                            string_lit(separator, span),
                            span,
                        ),
                    },
                    span,
                )],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ),
        Stmt::new(
            StmtKind::Assign {
                name: "result".to_string(),
                value: concat_expr(variable_expr("result", span), value, span),
            },
            span,
        ),
    ]
}
