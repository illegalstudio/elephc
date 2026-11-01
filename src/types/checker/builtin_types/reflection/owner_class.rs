//! Purpose:
//! Builds the shared synthetic classes for Reflection member owners.
//!
//! Called from:
//! - The Reflection checker metadata facade and sibling builders.
//!
//! Key details:
//! - Per-owner properties, methods, and constants remain selected by class name.

use super::*;

/// Builds the abstract callable-reflection base from the methods shared by its two concrete
/// children. The abstract contracts contain no storage-dependent bodies; concrete owners retain
/// their populated slot-backed implementations and override every inherited contract.
pub(super) fn builtin_reflection_function_abstract_class(
    function: &FlattenedClass,
    method: &FlattenedClass,
) -> FlattenedClass {
    let method_names = method
        .methods
        .iter()
        .map(|member| php_symbol_key(&member.name))
        .collect::<HashSet<_>>();
    let mut methods = function
        .methods
        .iter()
        .filter(|member| {
            let key = php_symbol_key(&member.name);
            method_names.contains(&key)
                && !key.starts_with("__fill")
                && key != "__type_from"
                && !matches!(
                    key.as_str(),
                    "__construct" | "invoke" | "invokeargs" | "getclosure"
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    // php's methods are concrete: on an object whose engine-side function was never set (a user
    // subclass such as psysh's `ReflectionLanguageConstruct`, which overrides only what it needs)
    // they raise this `Error`. Abstract contracts made that subclass fail to compile.
    for member in &mut methods {
        member.is_abstract = false;
        member.has_body = true;
        member.body = vec![crate::synthetic_class::s_throw(crate::synthetic_class::e_new(
            "\\Error",
            vec![crate::synthetic_class::e_str(
                "Internal error: Failed to retrieve the reflection object",
            )],
        ))];
    }
    FlattenedClass {
        name: "ReflectionFunctionAbstract".to_string(),
        span: dummy(),
        extends: None,
        implements: vec!["Reflector".to_string(), "Stringable".to_string()],
        is_abstract: true,
        is_final: false,
        is_readonly_class: false,
        properties: Vec::new(),
        methods,
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Builds a `FlattenedClass` for simple reflection owner classes
/// with a private `__attrs` array property and two methods: `__construct`
/// (public, accepting the supplied params) and `getAttributes` (public,
/// returning the `__attrs` array).
pub(super) fn builtin_reflection_owner_class(
    name: &str,
    has_name: bool,
    constructor_params: Vec<(&str, Option<TypeExpr>, Option<Expr>, bool)>,
) -> FlattenedClass {
    let mut properties = Vec::new();
    let mut methods = vec![builtin_reflection_owner_constructor_method(
        constructor_params,
    )];
    if has_name {
        properties.push(builtin_property(
            "name",
            Visibility::Public,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        properties.push(builtin_property(
            "__name",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        methods.push(builtin_reflection_class_string_method("getName", "__name"));
    }
    if matches!(
        name,
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "class",
            Visibility::Public,
            Some(TypeExpr::Str),
            empty_string(),
        ));
    }
    if reflection_owner_has_doc_comment_method(name) {
        methods.push(builtin_reflection_constant_false_union_method(
            "getDocComment",
        ));
    }
    if reflection_owner_has_extension_methods(name) {
        methods.push(builtin_reflection_constant_false_union_method(
            "getExtensionName",
        ));
        methods.push(builtin_reflection_constant_null_mixed_method(
            "getExtension",
        ));
    }
    add_reflection_function_method_origin_methods(name, &mut properties, &mut methods);
    add_reflection_member_flag_methods(name, &mut properties, &mut methods);
    if matches!(
        name,
        "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__declaring_class",
            Visibility::Private,
            Some(mixed_type()),
            false_bool(),
        ));
        if reflection_declaring_class_is_lazy(name) {
            properties.push(builtin_property(
                "__declaring_class_thunk",
                Visibility::Private,
                Some(TypeExpr::Int),
                int_lit(0),
            ));
            methods.push(builtin_reflection_lazy_declaring_class_method());
        } else {
            methods.push(builtin_reflection_class_object_method(
                "getDeclaringClass",
                "__declaring_class",
                "ReflectionClass",
            ));
        }
    }
    if matches!(
        name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
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
        methods.push(builtin_reflection_constant_false_bool_method(
            "isDeprecated",
        ));
        if name == "ReflectionClassConstant" {
            properties.push(builtin_property(
                "__has_type",
                Visibility::Private,
                Some(TypeExpr::Bool),
                bool_lit(false),
            ));
            properties.push(builtin_property(
                "__type",
                Visibility::Private,
                Some(mixed_type()),
                null_expr(),
            ));
            methods.push(builtin_reflection_slot_getter(
                "hasType",
                "__has_type",
                TypeExpr::Bool,
            ));
            methods.push(builtin_reflection_slot_getter(
                "getType",
                "__type",
                mixed_type(),
            ));
        } else {
            methods.push(builtin_reflection_constant_false_bool_method("hasType"));
            methods.push(builtin_reflection_constant_null_mixed_method("getType"));
        }
    }
    if matches!(name, "ReflectionFunction" | "ReflectionMethod") {
        properties.push(builtin_property(
            "__file_name",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        properties.push(builtin_property(
            "__start_line",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        properties.push(builtin_property(
            "__end_line",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_slot_getter(
            "getFileName",
            "__file_name",
            string_or_bool_type(),
        ));
        methods.push(builtin_reflection_slot_getter(
            "getStartLine",
            "__start_line",
            int_or_bool_type(),
        ));
        methods.push(builtin_reflection_slot_getter(
            "getEndLine",
            "__end_line",
            int_or_bool_type(),
        ));
        properties.push(builtin_property(
            "__string",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        properties.push(builtin_property(
            "__parameters",
            Visibility::Private,
            Some(object_array_type("ReflectionParameter")),
            empty_array(),
        ));
        properties.push(builtin_property(
            "__is_deprecated",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__is_generator",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__returns_reference",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__type",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        properties.push(builtin_property(
            "__has_return_type",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__required_parameter_count",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        methods.push(builtin_reflection_class_array_method(
            "getParameters",
            "__parameters",
            object_array_type("ReflectionParameter"),
        ));
        methods.push(builtin_reflection_parameter_count_method());
        methods.push(builtin_reflection_class_int_method(
            "getNumberOfRequiredParameters",
            "__required_parameter_count",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasReturnType",
            "__has_return_type",
        ));
        methods.push(builtin_reflection_class_mixed_method("getReturnType", "__type"));
        methods.push(builtin_reflection_constant_false_bool_method("isClosure"));
        methods.push(builtin_reflection_class_bool_method(
            "isDeprecated",
            "__is_deprecated",
        ));
        // Was a baked `false`, which is wrong for every `function &f()` and
        // `public function &m()`: PHP answers `true` for those (#1231). Every sibling predicate
        // that can vary is property-backed for exactly this reason.
        methods.push(builtin_reflection_class_bool_method(
            "returnsReference",
            "__returns_reference",
        ));
        methods.push(builtin_reflection_class_bool_method(
            "isGenerator",
            "__is_generator",
        ));
        methods.push(builtin_reflection_constant_false_bool_method(
            "hasTentativeReturnType",
        ));
        methods.push(builtin_reflection_constant_null_mixed_method(
            "getTentativeReturnType",
        ));
        methods.push(builtin_reflection_function_method_is_variadic_method());
        methods.push(builtin_reflection_class_string_method(
            "__toString",
            "__string",
        ));
        // A compiled reflector builds its parameter reflectors on first use: `__parameters_filler`
        // holds the compiled filler's address until then, and `__parameter_count` answers
        // `getNumberOfParameters()` without building them.
        properties.push(member_filler_property("__parameters"));
        properties.push(builtin_property(
            "__parameter_count",
            Visibility::Private,
            Some(TypeExpr::Int),
            int_lit(0),
        ));
        for method in &mut methods {
            match method.name.as_str() {
                "getParameters" | "isVariadic" => {
                    method.body.insert(
                        0,
                        if matches!(name, "ReflectionMethod" | "ReflectionFunction") {
                            reflection_member_fill_preamble_table("__parameters")
                        } else {
                            reflection_member_fill_preamble("__parameters")
                        },
                    );
                }
                "getNumberOfParameters" => {
                    method.body.insert(0, reflection_unfilled_parameter_count_return());
                }
                _ => {}
            }
        }
    }
    if name == "ReflectionFunction" {
        properties.push(builtin_property(
            "__callable",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
    }
    if name == "ReflectionMethod" {
        properties.push(builtin_property(
            "__class",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        properties.push(builtin_property(
            "__has_prototype",
            Visibility::Private,
            Some(bool_type()),
            false_bool(),
        ));
        properties.push(builtin_property(
            "__prototype",
            Visibility::Private,
            Some(mixed_type()),
            null_expr(),
        ));
        methods.push(builtin_reflection_class_bool_method(
            "hasPrototype",
            "__has_prototype",
        ));
        methods.push(builtin_reflection_method_get_prototype_method());
        methods.push(builtin_reflection_method_invoke_method());
        methods.push(builtin_reflection_method_invoke_args_method());
        methods.push(builtin_reflection_get_closure_method(true));
        methods.push(builtin_reflection_method_create_from_method_name_method());
        methods.push(builtin_reflection_set_accessible_method());
    }
    if name == "ReflectionFunction" {
        methods.push(builtin_reflection_function_invoke_method());
        methods.push(builtin_reflection_function_invoke_args_method());
        methods.push(builtin_reflection_get_closure_method(false));
        methods.push(builtin_reflection_constant_false_bool_method(
            "isAnonymous",
        ));
        methods.push(builtin_reflection_constant_false_bool_method("isStatic"));
        methods.push(builtin_reflection_constant_null_mixed_method(
            "getClosureThis",
        ));
        methods.push(builtin_reflection_constant_null_mixed_method(
            "getClosureScopeClass",
        ));
        methods.push(builtin_reflection_constant_null_mixed_method(
            "getClosureCalledClass",
        ));
        methods.push(builtin_reflection_constant_empty_array_method(
            "getClosureUsedVariables",
        ));
        // Same closure-introspection simplification as `getClosureUsedVariables` above: elephc
        // resolves a closure's bindings at compile time into a descriptor, so there is no runtime
        // name-to-value table to hand back. Declaring it keeps programs that merely MENTION the
        // method compilable — Symfony's `LazyString::resolve()` calls it only while building the
        // message for a `TypeError` it has already caught.
        methods.push(builtin_reflection_constant_empty_array_method(
            "getStaticVariables",
        ));
        methods.push(builtin_reflection_constant_false_bool_method(
            "isDisabled",
        ));
    }
    if name == "ReflectionProperty" {
        properties.push(builtin_property(
            "__class",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
        methods.push(builtin_reflection_set_accessible_method());
    }
    // php's `$constant->class`: the declaring class, which every dump of the reflector shows.
    if matches!(
        name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        properties.push(builtin_property(
            "__class",
            Visibility::Private,
            Some(TypeExpr::Str),
            empty_string(),
        ));
    }
    properties.push(builtin_property(
        "__attrs",
        Visibility::Private,
        Some(array_type()),
        empty_array(),
    ));
    // PHP names the class that declares `getAttributes()` in its `$flags` ValueError.
    let attributes_owner = match name {
        "ReflectionFunction" | "ReflectionMethod" => "ReflectionFunctionAbstract",
        "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase" => "ReflectionClassConstant",
        other => other,
    };
    methods.push(builtin_reflection_owner_get_attributes_method(attributes_owner));
    FlattenedClass {
        name: name.to_string(),
        span: dummy(),
        extends: None,
        implements: vec!["Reflector".to_string()],
        is_abstract: false,
        // None of the reflection owners php declares is final (`ReflectionProperty`,
        // `ReflectionClassConstant`, `ReflectionEnum*Case`, ... all report `isFinal() === false`),
        // and libraries extend them: Doctrine's `RuntimeReflectionProperty extends
        // \ReflectionProperty` backs every mapped entity field. Declared final, that class could
        // not be declared at run time and every page reading an entity failed.
        is_final: false,
        is_readonly_class: false,
        properties,
        methods,
        attributes: Vec::new(),
        constants: reflection_owner_constants(name),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Returns true when PHP exposes `getDocComment()` on this synthetic reflection owner.
pub(super) fn reflection_owner_has_doc_comment_method(class_name: &str) -> bool {
    matches!(
        class_name,
        "ReflectionFunction"
            | "ReflectionMethod"
            | "ReflectionProperty"
            | "ReflectionClassConstant"
            | "ReflectionEnumUnitCase"
            | "ReflectionEnumBackedCase"
    )
}

/// Returns true when this member reflector builds its declaring `ReflectionClass` on demand.
///
/// A compiled reflector of these classes records the address of the declaring class's shared
/// materializer in `__declaring_class_thunk` (see `emit_reflection_declaring_class_property`);
/// one built by the eval bridge still carries the object in `__declaring_class` and a zero thunk.
pub(super) fn reflection_declaring_class_is_lazy(class_name: &str) -> bool {
    matches!(class_name, "ReflectionMethod" | "ReflectionProperty")
}

/// Builds `getDeclaringClass()` for a reflector whose declaring class is built on demand:
/// `__elephc_reflection_materialize($this->__declaring_class_thunk)` when the reflector recorded
/// a materializer, else the object the eval bridge stored. Each call returns a fresh object, as
/// php's does.
fn builtin_reflection_lazy_declaring_class_method() -> ClassMethod {
    let span = crate::span::Span::dummy();
    let materialized = Stmt::new(
        StmtKind::If {
            condition: binary_expr(
                reflection_this_property("__declaring_class_thunk", span),
                BinOp::StrictNotEq,
                Expr::new(ExprKind::IntLiteral(0), span),
                span,
            ),
            then_body: vec![Stmt::new(
                StmtKind::Return(Some(reflection_materialized_declaring_class(span))),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    );
    ClassMethod {
        type_params: Vec::new(),
        name: "getDeclaringClass".to_string(),
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
        return_type: Some(TypeExpr::Named(Name::unqualified("ReflectionClass"))),
        by_ref_return: false,
        body: vec![
            materialized,
            Stmt::new(
                StmtKind::Return(Some(reflection_this_property("__declaring_class", span))),
                span,
            ),
        ],
        span,
        attributes: Vec::new(),
    }
}

/// `__elephc_reflection_materialize($this->__declaring_class_thunk)`: a fresh declaring
/// `ReflectionClass` built by the materializer the reflector recorded.
fn reflection_materialized_declaring_class(span: crate::span::Span) -> Expr {
    function_call(
        "__elephc_reflection_materialize",
        vec![reflection_this_property("__declaring_class_thunk", span)],
        span,
    )
}

/// `if ($this->__parameters_filler !== 0) { return $this->__parameter_count; }`: the count of
/// parameters a compiled reflector has not built yet.
fn reflection_unfilled_parameter_count_return() -> Stmt {
    let span = crate::span::Span::dummy();
    Stmt::new(
        StmtKind::If {
            condition: binary_expr(
                reflection_this_property("__parameters_filler", span),
                BinOp::StrictNotEq,
                Expr::new(ExprKind::IntLiteral(0), span),
                span,
            ),
            then_body: vec![Stmt::new(
                StmtKind::Return(Some(reflection_this_property("__parameter_count", span))),
                span,
            )],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        span,
    )
}

/// Returns true when PHP exposes extension-origin APIs on this reflection owner.
pub(super) fn reflection_owner_has_extension_methods(class_name: &str) -> bool {
    matches!(class_name, "ReflectionFunction" | "ReflectionMethod")
}

/// Returns public class constants exposed by a synthetic reflection owner.
pub(super) fn reflection_owner_constants(class_name: &str) -> Vec<ClassConst> {
    if class_name == "ReflectionMethod" {
        return vec![
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_STATIC", 16),
            builtin_class_const("IS_FINAL", 32),
            builtin_class_const("IS_ABSTRACT", 64),
        ];
    }
    if class_name == "ReflectionProperty" {
        return vec![
            builtin_class_const("IS_STATIC", 16),
            builtin_class_const("IS_READONLY", 128),
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_ABSTRACT", 64),
            builtin_class_const("IS_PROTECTED_SET", 2048),
            builtin_class_const("IS_PRIVATE_SET", 4096),
            builtin_class_const("IS_VIRTUAL", 512),
            builtin_class_const("IS_FINAL", 32),
        ];
    }
    if matches!(
        class_name,
        "ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase"
    ) {
        return vec![
            builtin_class_const("IS_PUBLIC", 1),
            builtin_class_const("IS_PROTECTED", 2),
            builtin_class_const("IS_PRIVATE", 4),
            builtin_class_const("IS_FINAL", 32),
        ];
    }
    Vec::new()
}
