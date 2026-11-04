//! Purpose:
//! Builds the PHP 8.6 `ReflectionProperty::isReadable()` and `isWritable()` method bodies.
//!
//! Called from:
//! - The Reflection checker metadata facade through `owner_methods`.
//!
//! Key details:
//! - The bodies are ordinary PHP over the retained reflection metadata plus the public
//!   `ReflectionClass` snapshot, so they need no new runtime ABI.
//! - `(array)$object` distinguishes an initialized backing slot from an unset or never
//!   initialized one without invoking the property's own get hook.
//! - The writable body recognizes a readonly reinitialization window by looking for the
//!   matching `__clone` frame, mirroring php-src's reinitializable-property flag.

use super::*;
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, InstanceOfTarget, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_class_const, e_concat, e_concat_all, e_const,
    e_index, e_int, e_method_call, e_new, e_null, e_null_coalesce, e_str, e_this, e_this_prop,
    e_var, method, s_assign, s_break, s_foreach, s_if, s_return, s_throw, t_mixed, t_nullable,
};

/// Builds `ReflectionProperty::isReadable(?string $scope, ?object $object = null): bool`.
pub(super) fn builtin_reflection_property_is_readable_method() -> ClassMethod {
    let scope = e_var("scope");
    let object = e_var("object");
    let declaring = declaring_class_name();
    let modifiers = e_method_call(e_this(), "getModifiers", Vec::new());
    let name = e_this_prop("__name");
    let probe = initialized_probe(&declaring, &name, &modifiers, &object);

    let virtual_without_get = s_if(
        e_not(hook_present("Get")),
        vec![
            s_assign("__elephc_readable", e_bool(false)),
            s_assign("__elephc_allow_magic", e_bool(false)),
        ],
        Vec::new(),
        None,
    );

    let backed_branch = {
        let instance_branch = s_if(
            and(e_not_null(&object), e_not(hook_present("Get"))),
            probe,
            Vec::new(),
            None,
        );
        s_if(
            e_method_call(e_this(), "isStatic", Vec::new()),
            vec![s_assign(
                "__elephc_readable",
                e_method_call(e_this(), "isInitialized", Vec::new()),
            )],
            Vec::new(),
            Some(vec![instance_branch]),
        )
    };

    let body = vec![
        s_assign("__elephc_declaring", declaring),
        scope_error_guard(&scope),
        static_object_guard(&object),
        instance_object_guard(&object, &e_var("__elephc_declaring")),
        s_assign(
            "__elephc_visible",
            visibility_check(&modifiers, &scope, &e_var("__elephc_declaring"), &object),
        ),
        s_assign("__elephc_readable", e_var("__elephc_visible")),
        s_assign("__elephc_allow_magic", e_bool(true)),
        s_if(
            e_not(e_var("__elephc_visible")),
            Vec::new(),
            vec![(
                e_method_call(e_this(), "isVirtual", Vec::new()),
                vec![virtual_without_get],
            )],
            Some(vec![backed_branch]),
        ),
        magic_read_fallback(&object),
        s_return(e_var("__elephc_readable")),
    ];

    method("isReadable")
        .param("scope", t_nullable(TypeExpr::Str))
        .param_default("object", t_mixed(), e_null())
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}

/// Builds `ReflectionProperty::isWritable(?string $scope, ?object $object = null): bool`.
pub(super) fn builtin_reflection_property_is_writable_method() -> ClassMethod {
    let scope = e_var("scope");
    let object = e_var("object");
    let declaring = declaring_class_name();
    let modifiers = e_method_call(e_this(), "getModifiers", Vec::new());
    let name = e_this_prop("__name");
    let probe = initialized_probe(&declaring, &name, &modifiers, &object);

    let readonly_branch = {
        let mut body = probe;
        let mut then_body = vec![s_assign("__elephc_in_clone", e_bool(false))];
        then_body.extend(clone_frame_probe(&object));
        then_body.push(s_if(
            e_not(e_var("__elephc_in_clone")),
            vec![s_assign("__elephc_writable", e_bool(false))],
            Vec::new(),
            None,
        ));
        body.push(s_if(
            e_var("__elephc_initialized"),
            then_body,
            Vec::new(),
            None,
        ));
        body
    };

    let virtual_branch = s_if(
        e_not(hook_present("Set")),
        vec![s_assign("__elephc_writable", e_bool(false))],
        Vec::new(),
        None,
    );

    let body = vec![
        s_assign("__elephc_declaring", declaring),
        scope_error_guard(&scope),
        static_object_guard(&object),
        instance_object_guard(&object, &e_var("__elephc_declaring")),
        s_assign(
            "__elephc_visible",
            visibility_check(&modifiers, &scope, &e_var("__elephc_declaring"), &object),
        ),
        s_if(
            and(
                and(
                    e_not(e_var("__elephc_visible")),
                    e_not(e_method_call(e_this(), "isStatic", Vec::new())),
                ),
                and(
                    e_not_null(&object),
                    has_method(&object, "__set"),
                ),
            ),
            vec![s_return(e_bool(true))],
            Vec::new(),
            None,
        ),
        s_assign(
            "__elephc_writable",
            and(
                e_var("__elephc_visible"),
                write_visibility_check(&modifiers, &scope, &e_var("__elephc_declaring"), &object),
            ),
        ),
        s_if(
            e_method_call(e_this(), "isVirtual", Vec::new()),
            vec![virtual_branch],
            Vec::new(),
            Some(vec![s_if(
                and(
                    and(
                        e_method_call(e_this(), "isReadOnly", Vec::new()),
                        e_not_null(&object),
                    ),
                    e_var("__elephc_writable"),
                ),
                readonly_branch,
                Vec::new(),
                None,
            )]),
        ),
        s_return(e_var("__elephc_writable")),
    ];

    method("isWritable")
        .param("scope", t_nullable(TypeExpr::Str))
        .param_default("object", t_mixed(), e_null())
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}

/// Returns `$this->getDeclaringClass()->getName()`.
fn declaring_class_name() -> Expr {
    e_method_call(
        e_method_call(e_this(), "getDeclaringClass", Vec::new()),
        "getName",
        Vec::new(),
    )
}

/// Returns a read-visibility test for a reflected property.
fn visibility_check(modifiers: &Expr, scope: &Expr, declaring: &Expr, object: &Expr) -> Expr {
    or(
        bit_set(modifiers, "IS_PUBLIC"),
        or(
            and(
                bit_set(modifiers, "IS_PROTECTED"),
                scope_related(scope, declaring, object),
            ),
            and(
                bit_set(modifiers, "IS_PRIVATE"),
                exact_scope(scope, declaring),
            ),
        ),
    )
}

/// Returns a set-visibility test, falling back to the read visibility when unnamed.
fn write_visibility_check(modifiers: &Expr, scope: &Expr, declaring: &Expr, object: &Expr) -> Expr {
    let explicit = or(bit_set(modifiers, "IS_PRIVATE_SET"), bit_set(modifiers, "IS_PROTECTED_SET"));
    or(
        and(
            bit_set(modifiers, "IS_PRIVATE_SET"),
            exact_scope(scope, declaring),
        ),
        or(
            and(
                bit_set(modifiers, "IS_PROTECTED_SET"),
                scope_related(scope, declaring, object),
            ),
            and(e_not(explicit), visibility_check(modifiers, scope, declaring, object)),
        ),
    )
}

/// Returns `(modifiers & ReflectionProperty::MASK) !== 0`.
fn bit_set(modifiers: &Expr, mask: &str) -> Expr {
    e_binop(
        e_binop(
            modifiers.clone(),
            BinOp::BitAnd,
            e_class_const("ReflectionProperty", mask),
        ),
        BinOp::StrictNotEq,
        e_int(0),
    )
}

/// Returns `true` when `$scope` names the declaring class exactly.
fn exact_scope(scope: &Expr, declaring: &Expr) -> Expr {
    and(
        e_not_null(scope),
        e_binop(
            e_call("strcasecmp", vec![scope.clone(), declaring.clone()]),
            BinOp::StrictEq,
            e_int(0),
        ),
    )
}

/// Returns `true` when `$scope` is the declaring class, an ancestor, or a descendant.
fn scope_related(scope: &Expr, declaring: &Expr, object: &Expr) -> Expr {
    and(
        e_not_null(scope),
        or(
            e_binop(
                e_call("strcasecmp", vec![scope.clone(), declaring.clone()]),
                BinOp::StrictEq,
                e_int(0),
            ),
            or(
                e_method_call(
                    e_method_call(e_this(), "getDeclaringClass", Vec::new()),
                    "isSubclassOf",
                    vec![scope.clone()],
                ),
                and(
                    e_not_null(object),
                    e_binop(
                        e_call(
                            "strcasecmp",
                            vec![e_call("get_class", vec![object.clone()]), scope.clone()],
                        ),
                        BinOp::StrictEq,
                        e_int(0),
                    ),
                ),
            ),
        ),
    )
}

/// Emits the unknown-scope guard php-src reports through `Error`.
fn scope_error_guard(scope: &Expr) -> Stmt {
    s_if(
        and(
            e_not_null(scope),
            e_not(e_call("class_exists", vec![scope.clone()])),
        ),
        vec![s_throw(e_new(
            "Error",
            vec![e_concat_all(vec![
                e_str("Class \""),
                scope.clone(),
                e_str("\" not found"),
            ])],
        ))],
        Vec::new(),
        None,
    )
}

/// Emits the static-property object guard shared by both predicates.
fn static_object_guard(object: &Expr) -> Stmt {
    s_if(
        and(
            e_method_call(e_this(), "isStatic", Vec::new()),
            e_not_null(object),
        ),
        vec![s_throw(e_new(
            "ReflectionException",
            vec![e_str(
                "null is expected as object argument for static properties",
            )],
        ))],
        Vec::new(),
        None,
    )
}

/// Emits the declared-class instance guard shared by both predicates.
fn instance_object_guard(object: &Expr, declaring: &Expr) -> Stmt {
    s_if(
        and(
            e_not_null(object),
            e_not(instance_of_declaring(object, declaring)),
        ),
        vec![s_throw(e_new(
            "ReflectionException",
            vec![e_str(
                "Given object is not an instance of the class this property was declared in",
            )],
        ))],
        Vec::new(),
        None,
    )
}

/// Returns `$object instanceof $declaring` for a runtime class name.
fn instance_of_declaring(object: &Expr, declaring: &Expr) -> Expr {
    Expr::new(
        ExprKind::InstanceOf {
            value: Box::new(object.clone()),
            target: InstanceOfTarget::Expr(Box::new(declaring.clone())),
        },
        crate::span::Span::dummy(),
    )
}

/// Emits the backing-slot probe and records `__elephc_initialized`.
fn initialized_probe(declaring: &Expr, name: &Expr, modifiers: &Expr, object: &Expr) -> Vec<Stmt> {
    vec![
        s_if(
            bit_set(modifiers, "IS_PRIVATE"),
            vec![s_assign(
                "__elephc_key",
                e_concat_all(vec![
                    e_str("\0"),
                    declaring.clone(),
                    e_str("\0"),
                    name.clone(),
                ]),
            )],
            Vec::new(),
            Some(vec![s_if(
                bit_set(modifiers, "IS_PROTECTED"),
                vec![s_assign(
                    "__elephc_key",
                    e_concat(e_str("\0*\0"), name.clone()),
                )],
                Vec::new(),
                Some(vec![s_assign("__elephc_key", name.clone())]),
            )]),
        ),
        s_assign(
            "__elephc_initialized",
            e_call(
                "array_key_exists",
                vec![e_var("__elephc_key"), e_cast(CastType::Array, object.clone())],
            ),
        ),
        s_if(
            e_not(e_var("__elephc_initialized")),
            vec![s_if(
                and(
                    e_method_call(e_this(), "hasType", Vec::new()),
                    e_not(e_method_call(e_this(), "hasDefaultValue", Vec::new())),
                ),
                vec![
                    s_assign("__elephc_readable", e_bool(false)),
                    s_assign("__elephc_allow_magic", e_bool(false)),
                ],
                Vec::new(),
                Some(vec![s_assign("__elephc_readable", e_bool(false))]),
            )],
            Vec::new(),
            None,
        ),
    ]
}

/// Emits the magic `__get`/`__isset` fallback for an unreadable declared property.
fn magic_read_fallback(object: &Expr) -> Stmt {
    s_if(
        and(
            and(
                e_not(e_var("__elephc_readable")),
                e_var("__elephc_allow_magic"),
            ),
            e_not_null(object),
        ),
        vec![s_if(
            has_method(object, "__isset"),
            vec![s_assign(
                "__elephc_readable",
                e_cast(
                    CastType::Bool,
                    e_call(
                        "call_user_func",
                        vec![
                            e_array(vec![object.clone(), e_str("__isset")]),
                            e_this_prop("__name"),
                        ],
                    ),
                ),
            )],
            Vec::new(),
            Some(vec![s_if(
                has_method(object, "__get"),
                vec![s_assign("__elephc_readable", e_bool(true))],
                Vec::new(),
                None,
            )]),
        )],
        Vec::new(),
        None,
    )
}

/// Emits the `__clone` frame probe into `__elephc_in_clone`.
fn clone_frame_probe(object: &Expr) -> Vec<Stmt> {
    vec![s_if(
        has_method(object, "__clone"),
        vec![s_foreach(
            e_call(
                "debug_backtrace",
                vec![e_const("DEBUG_BACKTRACE_PROVIDE_OBJECT")],
            ),
            None,
            "__elephc_frame",
            vec![s_if(
                and(
                    e_binop(
                        e_null_coalesce(
                            e_index(e_var("__elephc_frame"), e_str("function")),
                            e_str(""),
                        ),
                        BinOp::StrictEq,
                        e_str("__clone"),
                    ),
                    e_binop(
                        e_null_coalesce(
                            e_index(e_var("__elephc_frame"), e_str("object")),
                            e_null(),
                        ),
                        BinOp::StrictEq,
                        object.clone(),
                    ),
                ),
                vec![
                    s_assign("__elephc_in_clone", e_bool(true)),
                    s_break(1),
                ],
                Vec::new(),
                None,
            )],
        )],
        Vec::new(),
        None,
    )]
}

/// Returns `!is_null(value)`.
fn e_not_null(value: &Expr) -> Expr {
    e_binop(value.clone(), BinOp::StrictNotEq, e_null())
}

/// Returns `!value`.
fn e_not(value: Expr) -> Expr {
    Expr::new(ExprKind::Not(Box::new(value)), crate::span::Span::dummy())
}

/// Returns `left && right`.
fn and(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::And, right)
}

/// Returns `left || right`.
fn or(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Or, right)
}

/// Returns whether the runtime class of `$object` declares `$method` publicly.
fn has_method(object: &Expr, method: &str) -> Expr {
    e_call(
        "in_array",
        vec![
            e_str(method),
            e_call("get_class_methods", vec![object.clone()]),
            e_bool(true),
        ],
    )
}

/// Returns `$this->hasHook(PropertyHookType::{kind})`.
fn hook_present(kind: &str) -> Expr {
    e_method_call(
        e_this(),
        "hasHook",
        vec![e_class_const("PropertyHookType", kind)],
    )
}
