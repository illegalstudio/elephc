//! Purpose:
//! Adds parameter counts, origin metadata, and member flags to Reflection owners.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Function and member predicates remain grouped around retained metadata slots.

use super::*;

/// Builds `getNumberOfParameters()` over the retained parameter array.
pub(super) fn builtin_reflection_parameter_count_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "getNumberOfParameters".to_string(),
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
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("count"),
                    args: vec![Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(Expr::new(ExprKind::This, dummy_span)),
                            property: "__parameters".to_string(),
                        },
                        dummy_span,
                    )],
                },
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Builds `ReflectionFunctionAbstract::isVariadic()` from the retained parameter list.
pub(super) fn builtin_reflection_function_method_is_variadic_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let parameters = variable_expr("parameters", dummy_span);
    let count = variable_expr("count", dummy_span);
    let last_index = binary_expr(
        count.clone(),
        BinOp::Sub,
        Expr::new(ExprKind::IntLiteral(1), dummy_span),
        dummy_span,
    );
    let last_parameter = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(parameters.clone()),
            index: Box::new(last_index),
        },
        dummy_span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "isVariadic".to_string(),
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
            Stmt::new(
                StmtKind::Assign {
                    name: "parameters".to_string(),
                    value: reflection_this_property("__parameters", dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Assign {
                    name: "count".to_string(),
                    value: function_call("count", vec![parameters], dummy_span),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::If {
                    condition: binary_expr(
                        count.clone(),
                        BinOp::StrictEq,
                        Expr::new(ExprKind::IntLiteral(0), dummy_span),
                        dummy_span,
                    ),
                    then_body: vec![Stmt::new(StmtKind::Return(false_bool()), dummy_span)],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Return(Some(method_call_expr(
                    last_parameter,
                    "isVariadic",
                    Vec::new(),
                    dummy_span,
                ))),
                dummy_span,
            ),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Adds namespace/name-origin accessors shared by ReflectionFunction and ReflectionMethod.
pub(super) fn add_reflection_function_method_origin_methods(
    class_name: &str,
    properties: &mut Vec<ClassProperty>,
    methods: &mut Vec<ClassMethod>,
) {
    if !matches!(class_name, "ReflectionFunction" | "ReflectionMethod") {
        return;
    }
    properties.push(builtin_property(
        "__short_name",
        Visibility::Private,
        Some(TypeExpr::Str),
        empty_string(),
    ));
    properties.push(builtin_property(
        "__namespace_name",
        Visibility::Private,
        Some(TypeExpr::Str),
        empty_string(),
    ));
    properties.push(builtin_property(
        "__in_namespace",
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    properties.push(builtin_property(
        "__is_internal",
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    properties.push(builtin_property(
        "__is_user_defined",
        Visibility::Private,
        Some(bool_type()),
        false_bool(),
    ));
    methods.push(builtin_reflection_class_string_method(
        "getShortName",
        "__short_name",
    ));
    methods.push(builtin_reflection_class_string_method(
        "getNamespaceName",
        "__namespace_name",
    ));
    methods.push(builtin_reflection_class_bool_method(
        "inNamespace",
        "__in_namespace",
    ));
    methods.push(builtin_reflection_class_bool_method(
        "isInternal",
        "__is_internal",
    ));
    methods.push(builtin_reflection_class_bool_method(
        "isUserDefined",
        "__is_user_defined",
    ));
}

/// Adds member visibility/staticity predicates for method and property reflection owners.
pub(super) fn add_reflection_member_flag_methods(
    class_name: &str,
    properties: &mut Vec<ClassProperty>,
    methods: &mut Vec<ClassMethod>,
) {
    let visibility_flags = [
        ("__is_public", "isPublic"),
        ("__is_protected", "isProtected"),
        ("__is_private", "isPrivate"),
    ];
    if matches!(
        class_name,
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        for (property, method) in visibility_flags {
            properties.push(builtin_property(
                property,
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(method, property));
        }
    }
    if matches!(class_name, "ReflectionMethod" | "ReflectionProperty") {
        properties.push(builtin_property(
            "__is_static",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isStatic",
            "__is_static",
        ));
    }
    if class_name == "ReflectionMethod" {
        properties.push(builtin_property(
            "__modifiers",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_int_method(
            "getModifiers",
            "__modifiers",
        ));
        methods.push(builtin_reflection_method_name_predicate_method(
            "isConstructor",
            "__construct",
        ));
        methods.push(builtin_reflection_method_name_predicate_method(
            "isDestructor",
            "__destruct",
        ));
    }
    if class_name == "ReflectionProperty" {
        properties.push(builtin_property(
            "__type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__settable_type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__has_default_value",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__is_promoted",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__is_virtual",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__is_dynamic",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__has_hooks",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__hooks",
            Visibility::Private,
            Some(array_type()),
            empty_array(),
        ));
        properties.push(builtin_property(
            "__default_value",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__modifiers",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_int_method(
            "getModifiers",
            "__modifiers",
        ));
        methods.push(builtin_reflection_property_has_type_method());
        methods.push(builtin_reflection_class_mixed_method("getType", "__type"));
        methods.push(builtin_reflection_class_mixed_method(
            "getSettableType",
            "__settable_type",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasDefaultValue",
            "__has_default_value",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isPromoted",
            "__is_promoted",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isVirtual",
            "__is_virtual",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isDynamic",
            "__is_dynamic",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasHooks",
            "__has_hooks",
        ));
        methods.push(builtin_reflection_class_array_method(
            "getHooks",
            "__hooks",
            array_type(),
        ));
        methods.push(builtin_reflection_property_has_hook_method());
        methods.push(builtin_reflection_property_get_hook_method());
        properties.push(builtin_property(
            "__string",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        methods.push(builtin_reflection_class_string_method(
            "__toString",
            "__string",
        ));
        methods.push(builtin_reflection_property_is_lazy_method());
        methods.push(builtin_reflection_property_skip_lazy_initialization_method());
        methods.push(builtin_reflection_property_get_value_method());
        methods.push(builtin_reflection_property_set_value_method());
        methods.push(builtin_reflection_property_is_initialized_method());
        methods.push(builtin_reflection_property_modifier_mask_method(
            "isProtectedSet",
            2048,
        ));
        methods.push(builtin_reflection_property_modifier_mask_method(
            "isPrivateSet",
            4096,
        ));
        methods.push(builtin_reflection_property_is_default_method());
        methods.push(builtin_reflection_class_mixed_method(
            "getDefaultValue",
            "__default_value",
        ));
        for (property, method) in [("__is_final", "isFinal"), ("__is_abstract", "isAbstract")] {
            properties.push(builtin_property(
                property,
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(method, property));
        }
        properties.push(builtin_property(
            "__is_readonly",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isReadOnly",
            "__is_readonly",
        ));
    }
    if class_name == "ReflectionClassConstant" {
        properties.push(builtin_property(
            "__value",
            Visibility::Private,
            Some(mixed_type()),
            Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        ));
        methods.push(builtin_reflection_class_mixed_method("getValue", "__value"));
    }
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__is_enum_case",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isEnumCase",
            "__is_enum_case",
        ));
        properties.push(builtin_property(
            "__is_final",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isFinal",
            "__is_final",
        ));
        properties.push(builtin_property(
            "__modifiers",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_int_method(
            "getModifiers",
            "__modifiers",
        ));
    }
    if matches!(
        class_name,
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__enum",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        methods.push(builtin_reflection_class_mixed_method("getEnum", "__enum"));
        properties.push(builtin_property(
            "__value",
            Visibility::Private,
            Some(mixed_type()),
            Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        ));
        methods.push(builtin_reflection_class_mixed_method("getValue", "__value"));
    }
    if class_name == "ReflectionEnumBackedCase" {
        properties.push(builtin_property(
            "__backing_value",
            Visibility::Private,
            Some(mixed_type()),
            Some(Expr::new(ExprKind::Null, crate::span::Span::dummy())),
        ));
        methods.push(builtin_reflection_class_mixed_method(
            "getBackingValue",
            "__backing_value",
        ));
    }
    if class_name == "ReflectionMethod" {
        for (property, method) in [("__is_final", "isFinal"), ("__is_abstract", "isAbstract")] {
            properties.push(builtin_property(
                property,
                Visibility::Private,
                Some(bool_type()),
                false_bool(),
            ));
            methods.push(builtin_reflection_class_bool_method(method, property));
        }
    }
}
