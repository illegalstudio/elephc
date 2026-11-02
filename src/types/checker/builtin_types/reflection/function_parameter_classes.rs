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
        vec![(
            "function",
            Some(TypeExpr::Union(vec![
                TypeExpr::Named(Name::unqualified("Closure")),
                TypeExpr::Str,
            ])),
            None,
            false,
        )],
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
        implements: vec!["Reflector".to_string()],
        is_abstract: false,
        // Not final in php: psysh's `ReflectionLanguageConstructParameter` extends it.
        is_final: false,
        is_readonly_class: false,
        properties: vec![
            builtin_property("name", Visibility::Public, Some(TypeExpr::Str), empty_string()),
            builtin_property("__name", Visibility::Private, Some(TypeExpr::Str), empty_string()),
            builtin_property("__attrs", Visibility::Private, Some(array_type()), empty_array()),
            // PHP renders a parameter as `Parameter #0 [ <required> int $a ]`. This slot used
            // to be absent and `__toString` read `__name`, so `(string) $parameter` answered
            // the bare name (#1080).
            builtin_property("__string", Visibility::Private, Some(TypeExpr::Str), empty_string()),
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
            builtin_property("__default_error", Visibility::Private, Some(TypeExpr::Str), empty_string()),
            builtin_property("__default_factory", Visibility::Private, Some(mixed_type()), null_lit()),
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
            builtin_reflection_class_string_method("__toString", "__string"),
            builtin_reflection_owner_get_attributes_method("ReflectionParameter"),
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
        type_params: Vec::new(),
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
            reflection_throw_if_deferred_default_error(dummy_span),
            reflection_parameter_lazy_factory_return(dummy_span),
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
    // Zero counts too: the lazily built member table retains an argument-less default
    // (`new Foo`, or a constant of an undefined class) as an empty array, not null.
    for arg_count in 0..=8 {
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
/// A call on a receiver typed `ReflectionClass` never reaches this body: the call site lowers
/// straight to a constructorless allocation. A receiver the compiler only knows as `mixed` (a
/// reflector kept in an array, as DeepClone does) dispatches here instead, so the body is that
/// same call on `$this` -- which the call-site lowering turns into the allocation. It used to
/// return null, and every such call answered null.
pub(super) fn builtin_reflection_class_new_instance_without_constructor_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
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
            StmtKind::Return(Some(method_call_expr(
                Expr::new(ExprKind::This, dummy_span),
                "newInstanceWithoutConstructor",
                Vec::new(),
                dummy_span,
            ))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass::newLazyGhost()` method backed by eager initialization.
///
/// The AOT object model does not defer property access, so it allocates without invoking the
/// target constructor, calls the initializer once with that fresh object, and returns the
/// initialized instance. This preserves initializer and constructor-bypass semantics while
/// making the object immediately usable through the ordinary runtime object representation.
///
/// A Throwable the initializer raises is NOT propagated from here. php runs the initializer only
/// on the object's first use, so a ghost nothing ever touches never raises at all: Symfony's
/// container hands out ghosts whose initializer resolves configuration the running command never
/// needs, and an eager failure (`uri_signer` with an empty `kernel.secret`) aborted
/// `debug:event-dispatcher`, which php completes. The known difference: a ghost whose initializer
/// failed and that IS used later behaves as the half-initialized object, where php would raise
/// the initializer's Throwable at that first use.
pub(super) fn builtin_reflection_class_new_lazy_ghost_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    let instance = variable_expr("instance", dummy_span);
    ClassMethod {
        type_params: Vec::new(),
        name: "newLazyGhost".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "initializer".to_string(),
                Some(TypeExpr::Named(Name::unqualified("Closure"))),
                None,
                false,
            ),
            (
                "options".to_string(),
                Some(TypeExpr::Int),
                int_lit(0),
                false,
            ),
        ],
        param_attributes: vec![Vec::new(), Vec::new()],
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(object_type()),
        by_ref_return: false,
        body: vec![
            Stmt::new(
                StmtKind::Assign {
                    name: "instance".to_string(),
                    value: method_call_expr(
                        Expr::new(ExprKind::This, dummy_span),
                        "newInstanceWithoutConstructor",
                        Vec::new(),
                        dummy_span,
                    ),
                },
                dummy_span,
            ),
            Stmt::new(
                StmtKind::Try {
                    try_body: vec![Stmt::new(
                        StmtKind::ExprStmt(Expr::new(
                            ExprKind::ExprCall {
                                callee: Box::new(variable_expr("initializer", dummy_span)),
                                args: vec![instance.clone()],
                            },
                            dummy_span,
                        )),
                        dummy_span,
                    )],
                    catches: vec![crate::parser::ast::CatchClause {
                        exception_type_args: Vec::new(),
                        exception_types: vec![Name::from_parts(
                            crate::names::NameKind::FullyQualified,
                            vec!["Throwable".to_string()],
                        )],
                        variable: None,
                        body: Vec::new(),
                    }],
                    finally_body: None,
                },
                dummy_span,
            ),
            Stmt::new(StmtKind::Return(Some(instance)), dummy_span),
        ],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::resetAsLazyGhost()` / `resetAsLazyProxy()` in the eager model the
/// `newLazy*()` methods above use.
///
/// php resets the object to a lazy state and re-runs the initializer (or factory) on its first
/// use; Symfony's Doctrine `ManagerRegistry::resetService()` re-creates a closed entity manager in
/// place this way. Eagerly: a ghost has every non-static property with a default reset to it and
/// the initializer run on the object; a proxy copies the properties the factory's instance has
/// initialized onto the object. A Throwable the callback raises is swallowed, as for
/// `newLazyGhost()`. The known differences: a readonly property already set cannot be reset, and
/// a proxy is the object itself rather than a forwarder to the factory's instance.
pub(super) fn builtin_reflection_class_reset_as_lazy_method(proxy: bool) -> ClassMethod {
    use crate::synthetic_class::{
        e_call, e_method_call, e_not, e_var, method, s_assign, s_expr, s_foreach, s_if, s_try,
    };
    let callback = if proxy { "factory" } else { "initializer" };
    let source = if proxy { "_real" } else { "object" };
    let copy_value = if proxy {
        e_method_call(e_var("_p"), "getValue", vec![e_var("_real")])
    } else {
        e_method_call(e_var("_p"), "getDefaultValue", vec![])
    };
    let applies = if proxy {
        e_method_call(e_var("_p"), "isInitialized", vec![e_var("_real")])
    } else {
        e_method_call(e_var("_p"), "hasDefaultValue", vec![])
    };
    let copy = s_foreach(
        e_method_call(Expr::new(ExprKind::This, crate::span::Span::dummy()), "getProperties", vec![]),
        None,
        "_p",
        vec![s_if(
            crate::synthetic_class::e_binop(
                e_not(e_method_call(e_var("_p"), "isStatic", vec![])),
                crate::parser::ast::BinOp::And,
                applies,
            ),
            vec![s_try(
                vec![s_expr(e_method_call(e_var("_p"), "setValue", vec![e_var("object"), copy_value]))],
                vec![(vec!["Error"], None, vec![])],
                None,
            )],
            vec![],
            None,
        )],
    );
    let run = Expr::new(
        ExprKind::ExprCall {
            callee: Box::new(e_var(callback)),
            args: vec![e_var("object")],
        },
        crate::span::Span::dummy(),
    );
    let body = if proxy {
        vec![
            s_try(vec![s_assign("_real", run)], vec![(vec!["Throwable"], None, vec![crate::synthetic_class::s_return_void()])], None),
            s_if(e_not(e_call("is_object", vec![e_var(source)])), vec![crate::synthetic_class::s_return_void()], vec![], None),
            copy,
        ]
    } else {
        vec![copy, s_try(vec![s_expr(run)], vec![(vec!["Throwable"], None, vec![])], None)]
    };
    method(if proxy { "resetAsLazyProxy" } else { "resetAsLazyGhost" })
        .param("object", object_type())
        .param(callback, TypeExpr::Named(Name::unqualified("Closure")))
        .param_default("options", TypeExpr::Int, int_lit(0).expect("int literal"))
        .returns(TypeExpr::Void)
        .body(body)
        .build()
}

/// Returns a public `ReflectionClass::isUninitializedLazyObject()` method that answers false.
///
/// `newLazyGhost()` and `newLazyProxy()` above are backed by EAGER initialization: the object is
/// allocated and its initializer run before either returns, so the AOT object model never holds
/// an object in the uninitialized lazy state this predicate asks about. False is therefore the
/// exact answer here, not an approximation of one.
///
/// `Symfony\Component\DependencyInjection\ServicesResetter::isInitialized()` calls it on every
/// service it considers resetting.
pub(super) fn builtin_reflection_class_is_uninitialized_lazy_object_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "isUninitializedLazyObject".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(object_type()), None, false)],
        param_attributes: vec![Vec::new()],
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Bool),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(ExprKind::BoolLiteral(false), dummy_span))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns `ReflectionClass::initializeLazyObject()` or `markLazyObjectAsInitialized()`, which
/// both hand back the object they were given.
///
/// Exact, not approximate, for the same reason `isUninitializedLazyObject()` answers false: the
/// compiled lazy objects are initialized before `newLazyGhost()`/`newLazyProxy()` return, so
/// there is nothing left to initialize or to mark. Ghosts of interpreter-declared classes are
/// genuinely lazy, and the interpreter answers these methods for them before this body is reached.
pub(super) fn builtin_reflection_class_lazy_object_identity_method(name: &str) -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("object".to_string(), Some(object_type()), None, false)],
        param_attributes: vec![Vec::new()],
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(object_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(variable_expr("object", dummy_span))),
            dummy_span,
        )],
        span: dummy_span,
        attributes: Vec::new(),
    }
}

/// Returns a public `ReflectionClass::newLazyProxy()` method backed by eager initialization.
///
/// The AOT object model has no deferred proxy boundary. Calling the supplied factory immediately,
/// with an uninitialized instance standing in for the proxy PHP would pass, preserves its
/// object-producing contract and returns the ordinary object representation that
/// subsequent property and method operations already understand.
pub(super) fn builtin_reflection_class_new_lazy_proxy_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
        name: "newLazyProxy".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "factory".to_string(),
                Some(TypeExpr::Named(Name::unqualified("Closure"))),
                None,
                false,
            ),
            (
                "options".to_string(),
                Some(TypeExpr::Int),
                int_lit(0),
                false,
            ),
        ],
        param_attributes: vec![Vec::new(), Vec::new()],
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(object_type()),
        by_ref_return: false,
        body: vec![Stmt::new(
            StmtKind::Return(Some(Expr::new(
                ExprKind::ExprCall {
                    callee: Box::new(variable_expr("factory", dummy_span)),
                    // PHP hands the factory the (still uninitialized) proxy; factories declare
                    // that parameter, so an uninitialized instance stands in for it here.
                    args: vec![method_call_expr(
                        Expr::new(ExprKind::This, dummy_span),
                        "newInstanceWithoutConstructor",
                        Vec::new(),
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

/// Builds `ReflectionParameter::isDefaultValueConstant()` over retained default metadata.
pub(super) fn builtin_reflection_parameter_is_default_value_constant_method() -> ClassMethod {
    let dummy_span = crate::span::Span::dummy();
    ClassMethod {
        type_params: Vec::new(),
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
        type_params: Vec::new(),
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
        type_params: Vec::new(),
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
