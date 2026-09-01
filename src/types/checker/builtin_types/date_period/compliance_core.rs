//! Purpose:
//! Core DatePeriod initialization, overload, iteration, factory, and debug metadata.
//!
//! Called from:
//! - The DatePeriod checker metadata facade and sibling compliance module.
//!
//! Key details:
//! - Preserves the audited php-src DatePeriod semantics in the split checker layout.

#[allow(unused_imports)]
use super::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, FlattenedClass,
    HashMap, Name, PropertyHooks, StaticReceiver, Stmt, StmtKind, TypeExpr, Visibility,
};

pub(super) fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

/// Builds an integer-literal expression.
pub(super) fn int_lit(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), dummy())
}

/// Builds a `$name` variable expression.
pub(super) fn var(name: &str) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), dummy())
}

/// Builds a `$this->property` access expression.
pub(super) fn this_prop(property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `$var->property` access expression.
pub(super) fn var_prop(var_name: &str, property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(var(var_name)),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `left <op> right` binary expression.
pub(super) fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    Expr::new(
        ExprKind::BinaryOp { left: Box::new(left), op, right: Box::new(right) },
        dummy(),
    )
}

/// Builds an `$object-><method>(args)` method-call expression.
pub(super) fn mcall(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::MethodCall { object: Box::new(object), method: method.to_string(), args },
        dummy(),
    )
}

/// Builds an `(int) expr` cast expression. Used to unbox a `mixed` value into an
/// integer slot without relying on flow-sensitive narrowing in the type checker.
pub(super) fn cast_int(value: Expr) -> Expr {
    Expr::new(ExprKind::Cast { target: CastType::Int, expr: Box::new(value) }, dummy())
}

/// Builds a `null` literal expression.
pub(super) fn null_lit() -> Expr {
    Expr::new(ExprKind::Null, dummy())
}

/// Builds a `$this->property = value;` statement.
pub(super) fn assign_this(property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
            value,
        },
        dummy(),
    )
}

/// Builds an expression statement (a bare expression used for its side effects).
pub(super) fn expr_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(value), dummy())
}

/// Builds a `return <expr>;` statement.
pub(super) fn ret(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), dummy())
}

/// Builds an `if (cond) { then } else { else_body }` statement (no elseif clauses).
pub(super) fn if_else(condition: Expr, then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>>) -> Stmt {
    Stmt::new(
        StmtKind::If { condition, then_body, elseif_clauses: Vec::new(), else_body },
        dummy(),
    )
}

/// Builds a public method parameter `(name, type, default, by_ref)`.
pub(super) fn param(
    name: &str,
    ty: Option<TypeExpr>,
    default: Option<Expr>,
) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), ty, default, false)
}

/// Builds a method with the given visibility, params, return type, and body.
pub(super) fn method_vis(
    name: &str,
    visibility: Visibility,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public method.
pub(super) fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    method_vis(name, Visibility::Public, params, return_type, body)
}

/// Builds a private integer storage property defaulting to `0`.
pub(super) fn int_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Private,
        set_visibility: None,
        type_expr: Some(TypeExpr::Int),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(int_lit(0)),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a private boolean storage property defaulting to `false`.
pub(super) fn bool_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Private,
        set_visibility: None,
        type_expr: Some(TypeExpr::Bool),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(Expr::new(ExprKind::BoolLiteral(false), dummy())),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the hidden initialization marker used by php-src's `DatePeriod` object handler.
pub(super) fn date_period_initialized_property() -> ClassProperty {
    bool_property("__elephc_initialized")
}

/// Builds the shared `DatePeriod` payload guard with php-src's subclass-aware error text.
pub(super) fn date_period_assert_initialized() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if (!$this->__elephc_initialized) {
    $objectClass = get_class($this);
    $inheritance = $objectClass === "DatePeriod" ? "" : " (inheriting DatePeriod)";
    throw new DateObjectError(
        "Object of type " . $objectClass . $inheritance
        . " has not been correctly initialized by calling parent::__construct() in its constructor"
    );
}
"#,
    )
    .expect("DatePeriod initialization guard must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod initialization guard must parse");
    let mut result = method(
        "__elephc_assert_initialized",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    );
    result.is_final = true;
    result
}

/// Builds the iterator-entry guard whose php-src diagnostic intentionally names `DatePeriod`.
pub(super) fn date_period_assert_iterable_initialized() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if (!$this->__elephc_initialized) {
    throw new DateObjectError(
        "Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor"
    );
}
"#,
    )
    .expect("DatePeriod iterator initialization guard must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod iterator initialization guard must parse");
    let mut result = method(
        "__elephc_assert_iterable_initialized",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    );
    result.is_final = true;
    result
}

/// Builds php-src's runtime rejection for by-reference iteration over a DatePeriod iterator.
pub(super) fn date_period_assert_foreach_by_reference() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
throw new Error("An iterator cannot be used with foreach by reference");
"#,
    )
    .expect("DatePeriod by-reference foreach guard must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod by-reference foreach guard must parse");
    let mut result = method(
        "__elephc_assert_foreach_by_reference",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    );
    result.is_final = true;
    result
}

/// Guards public `DatePeriod` surfaces that require materialized constructor state.
///
/// `getEndDate()` and `getRecurrences()` intentionally remain callable on uninitialized objects,
/// while iteration keeps php-src's distinct hard-coded `DatePeriod` error path.
pub(super) fn guard_date_period_payload_methods(methods: &mut [ClassMethod]) {
    let tokens = crate::lexer::tokenize("<?php $this->__elephc_assert_initialized();")
        .expect("DatePeriod guard call must tokenize");
    let guard = crate::parser::parse(&tokens)
        .expect("DatePeriod guard call must parse")
        .into_iter()
        .next()
        .expect("DatePeriod guard call must contain one statement");
    let start_hook = crate::names::property_hook_get_method("start");
    let current_hook = crate::names::property_hook_get_method("current");
    let interval_hook = crate::names::property_hook_get_method("interval");
    for method in methods {
        if matches!(
            method.name.as_str(),
            "getStartDate" | "getDateInterval" | "__serialize" | "__elephc_debug_dump"
        ) || method.name == start_hook
            || method.name == current_hook
            || method.name == interval_hook
        {
            method.body.insert(0, guard.clone());
        }
    }
}

/// Builds one private mixed storage property defaulting to `null`.
pub(super) fn mixed_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Private,
        set_visibility: None,
        type_expr: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(null_lit()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public integer class constant.
pub(super) fn class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: Some(TypeExpr::Int),
        value: int_lit(value),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// The seven `DateInterval` components stored on `DatePeriod`, paired with the
/// `DateInterval` property they mirror: (storage property, interval property).
pub(super) const INTERVAL_PARTS: [(&str, &str); 7] = [
    ("iv_y", "y"),
    ("iv_m", "m"),
    ("iv_d", "d"),
    ("iv_h", "h"),
    ("iv_i", "i"),
    ("iv_s", "s"),
    ("iv_invert", "invert"),
];

/// `DatePeriod::__construct(DateTimeInterface $start, DateInterval $interval, DateTimeInterface|int $end, int $options = 0)`.
///
/// Records the start timestamp, decomposes the interval into its seven integer
/// components, and reads the `EXCLUDE_START_DATE` / `INCLUDE_END_DATE` option bits.
/// The third argument selects the period form: a `DateTimeInterface` sets an end
/// bound (`useCount = 0`, `endTs` recorded), while an `int` sets a recurrence count
/// (`useCount = 1`, `recurrences` recorded) so iteration stops by count instead of date.
pub(super) fn date_period_constructor() -> ClassMethod {
    let dti = Some(TypeExpr::Named(Name::unqualified("DateTimeInterface")));
    let interval_ty = Some(TypeExpr::Named(Name::unqualified("DateInterval")));
    // `mixed` so an int recurrence count or a DateTimeInterface end both pass the checker.
    let end_ty = Some(TypeExpr::Named(Name::unqualified("mixed")));
    let validation_tokens = crate::lexer::tokenize(
        r#"<?php
$__elephc_uses_recurrence_end = false;
$__elephc_recurrence_end = 0;
if (is_int($end)) {
    $__elephc_uses_recurrence_end = true;
    $__elephc_recurrence_end = (int) $end;
} elseif ($end instanceof DateTimeInterface) {
    $__elephc_uses_recurrence_end = false;
} elseif (is_float($end) || (is_string($end) && is_numeric($end)) || is_bool($end) || is_null($end)) {
    $__elephc_uses_recurrence_end = true;
    $__elephc_recurrence_end = (int) $end;
} else {
    throw new TypeError(
        "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
    );
}
if ($__elephc_uses_recurrence_end) {
    if ($__elephc_recurrence_end < 1 || $__elephc_recurrence_end > 2147483639) {
        throw new DateMalformedPeriodStringException(
            "DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640"
        );
    }
    $totalRecurrences = $__elephc_recurrence_end
        + (($options & DatePeriod::EXCLUDE_START_DATE) ? 0 : 1)
        + (($options & DatePeriod::INCLUDE_END_DATE) ? 1 : 0);
    if ($totalRecurrences > 2147483639) {
        throw new DateMalformedStringException(
            "DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640 (including options)"
        );
    }
}
"#,
    )
    .expect("DatePeriod constructor validation must tokenize");
    let mut body = crate::parser::parse(&validation_tokens)
        .expect("DatePeriod constructor validation must parse");
    body.insert(
        0,
        assign_this(
            "__elephc_initialized",
            Expr::new(ExprKind::BoolLiteral(true), dummy()),
        ),
    );
    body.extend(vec![
        assign_this(
            "startTs",
            mcall(
                Expr::new(ExprKind::This, dummy()),
                "__elephc_datetime_interface_timestamp",
                vec![var("start")],
            ),
        ),
        assign_this(
            "startIsImmutable",
            Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(var("start")),
                    target: crate::parser::ast::InstanceOfTarget::Name(Name::unqualified("DateTimeImmutable")),
                },
                dummy(),
            ),
        ),
    ]);
    for (store, part) in INTERVAL_PARTS {
        body.push(assign_this(store, var_prop("interval", part)));
    }
    // The normalized recurrence flag separates weak scalar-to-int coercion from the
    // DateTimeInterface end-bound form without changing the mixed parameter's flow type.
    body.push(if_else(
        var("__elephc_uses_recurrence_end"),
        vec![
            assign_this("useCount", int_lit(1)),
            assign_this(
                "_recurrence_count",
                cast_int(var("__elephc_recurrence_end")),
            ),
            assign_this("endTs", int_lit(0)),
        ],
        Some(vec![
            assign_this("useCount", int_lit(0)),
            assign_this("_recurrence_count", int_lit(0)),
            assign_this(
                "endTs",
                mcall(
                    Expr::new(ExprKind::This, dummy()),
                    "__elephc_datetime_interface_timestamp",
                    vec![var("end")],
                ),
            ),
        ]),
    ));
    // EXCLUDE_START_DATE = 1, INCLUDE_END_DATE = 2 → keep only the relevant bit.
    body.push(assign_this("excludeStart", bin(var("options"), BinOp::BitAnd, int_lit(1))));
    body.push(assign_this("includeEnd", bin(var("options"), BinOp::BitAnd, int_lit(2))));
    body.push(assign_this("curTs", this_prop("startTs")));
    body.push(assign_this("idx", int_lit(0)));
    // Populate private storage backing PHP 8.2+'s virtual public properties.
    body.push(assign_this(
        "_start",
        mcall(
            Expr::new(ExprKind::This, dummy()),
            "__elephc_clone_datetime_interface_storage",
            vec![var("start")],
        ),
    ));
    // include_start_date = !excludeStart; include_end_date = (includeEnd != 0).
    body.push(assign_this(
        "_include_start_date",
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(this_prop("excludeStart")),
                op: BinOp::Eq,
                right: Box::new(int_lit(0)),
            },
            dummy(),
        ),
    ));
    body.push(assign_this(
        "_include_end_date",
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(this_prop("includeEnd")),
                op: BinOp::NotEq,
                right: Box::new(int_lit(0)),
            },
            dummy(),
        ),
    ));
    // `end` depends on the overload. The public `recurrences` property is the minimum
    // number of yielded instances: explicit count + included start + included end.
    body.push(if_else(
        var("__elephc_uses_recurrence_end"),
        vec![assign_this("_end", null_lit())],
        Some(vec![assign_this(
            "_end",
            mcall(
                Expr::new(ExprKind::This, dummy()),
                "__elephc_clone_datetime_interface_storage",
                vec![var("end")],
            ),
        )]),
    ));
    // php-src snapshots the interval after the optional end boundary, which is
    // observable through object handles in recursive debug output.
    body.push(assign_this(
        "_interval",
        mcall(
            var("interval"),
            "__elephc_clone_interval_for_period_storage",
            Vec::new(),
        ),
    ));
    body.push(assign_this(
        "_recurrences",
        bin(
            bin(
                this_prop("_recurrence_count"),
                BinOp::Add,
                cast_int(this_prop("_include_start_date")),
            ),
            BinOp::Add,
            cast_int(this_prop("_include_end_date")),
        ),
    ));
    method(
        "__construct",
        vec![
            param("start", dti, None),
            param("interval", interval_ty, None),
            param("end", end_ty, None),
            param("options", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        None,
        body,
    )
}

/// Builds the final end-bound initializer used by constructorless DatePeriod factories.
pub(super) fn date_period_initialize_end_components() -> ClassMethod {
    date_period_factory_initializer(
        "__elephc_initialize_end_components",
        TypeExpr::Int,
        false,
    )
}

/// Builds the final recurrence-count initializer used by constructorless DatePeriod factories.
pub(super) fn date_period_initialize_recurrence_components() -> ClassMethod {
    date_period_factory_initializer(
        "__elephc_initialize_recurrence_components",
        TypeExpr::Int,
        true,
    )
}

/// Derives one factory-only initializer from the constructor storage body without its overload checks.
fn date_period_factory_initializer(
    name: &str,
    end_type: TypeExpr,
    uses_recurrence_count: bool,
) -> ClassMethod {
    let mut result = date_period_constructor();
    let storage_start = result
        .body
        .iter()
        .position(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::PropertyAssign { property, .. } if property == "startTs"
            )
        })
        .expect("DatePeriod constructor storage body must assign startTs");
    let recurrence_value = if uses_recurrence_count {
        var("end")
    } else {
        int_lit(0)
    };
    let mut setup = vec![
        Stmt::new(
            StmtKind::Assign {
                name: "__elephc_uses_recurrence_end".to_string(),
                value: Expr::new(ExprKind::BoolLiteral(uses_recurrence_count), dummy()),
            },
            dummy(),
        ),
        Stmt::new(
            StmtKind::Assign {
                name: "__elephc_recurrence_end".to_string(),
                value: recurrence_value,
            },
            dummy(),
        ),
    ];
    if !uses_recurrence_count {
        setup.push(Stmt::new(
            StmtKind::Assign {
                name: "end".to_string(),
                value: Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver: StaticReceiver::Named(Name::unqualified(
                            "DateTimeImmutable",
                        )),
                        method: "createFromTimestamp".to_string(),
                        args: vec![var("endTimestamp")],
                    },
                    dummy(),
                ),
            },
            dummy(),
        ));
    }
    result.body.splice(1..storage_start, setup);
    result.name = name.to_string();
    result.params[2].1 = Some(end_type);
    if !uses_recurrence_count {
        result.params[2].0 = "endTimestamp".to_string();
    }
    result.is_final = true;
    result
}

/// Builds the typed clone boundary used for DatePeriod's mixed third argument.
pub(super) fn date_period_clone_datetime_interface() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($value instanceof DateTimeImmutable) {
    return $value->__elephc_clone_for_period();
}
if ($value instanceof DateTime) {
    return $value->__elephc_clone_for_period();
}
throw new DateMalformedPeriodStringException("Invalid DatePeriod boundary");
"#,
    )
    .expect("DatePeriod DateTimeInterface clone helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod DateTimeInterface clone helper must parse");
    method_vis(
        "__elephc_clone_datetime_interface",
        Visibility::Private,
        vec![param(
            "value",
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
        )],
        Some(date_period_datetime_implementation_type()),
        body,
    )
}

/// Builds the typed handleless clone boundary for DatePeriod's private date storage.
pub(super) fn date_period_clone_datetime_interface_storage() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($value instanceof DateTimeImmutable) {
    return $value->__elephc_clone_for_period_storage();
}
if ($value instanceof DateTime) {
    return $value->__elephc_clone_for_period_storage();
}
throw new DateMalformedPeriodStringException("Invalid DatePeriod boundary");
"#,
    )
    .expect("DatePeriod handleless DateTimeInterface clone helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod handleless DateTimeInterface clone helper must parse");
    method_vis(
        "__elephc_clone_datetime_interface_storage",
        Visibility::Private,
        vec![param(
            "value",
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
        )],
        Some(date_period_datetime_implementation_type()),
        body,
    )
}

/// Returns the two concrete implementation families allowed by DateTimeInterface.
pub(super) fn date_period_datetime_implementation_type() -> TypeExpr {
    TypeExpr::Union(vec![
        TypeExpr::Named(Name::unqualified("DateTime")),
        TypeExpr::Named(Name::unqualified("DateTimeImmutable")),
    ])
}

/// Builds the canonical base-class snapshot used for values yielded by DatePeriod.
///
/// php-src retains the concrete start class in DatePeriod's stored state, but its
/// iterator deliberately materializes subclass instances as their DateTime or
/// DateTimeImmutable base class.
pub(super) fn date_period_clone_iterator_value() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($value instanceof DateTimeImmutable) {
    return DateTimeImmutable::createFromInterface($value);
}
if ($value instanceof DateTime) {
    return DateTime::createFromInterface($value);
}
throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
"#,
    )
    .expect("DatePeriod iterator value clone helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod iterator value clone helper must parse");
    method_vis(
        "__elephc_clone_iterator_value",
        Visibility::Private,
        vec![param(
            "value",
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
        )],
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// Builds the typed timestamp boundary used for DatePeriod's mixed third argument.
pub(super) fn date_period_datetime_interface_timestamp() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($value instanceof DateTimeImmutable) {
    if (!$value->__elephc_is_initialized()) {
        throw new DateObjectError(
            "Object of type DateTimeInterface has not been correctly initialized by calling parent::__construct() in its constructor"
        );
    }
    return $value->getTimestamp();
}
if ($value instanceof DateTime) {
    if (!$value->__elephc_is_initialized()) {
        throw new DateObjectError(
            "Object of type DateTimeInterface has not been correctly initialized by calling parent::__construct() in its constructor"
        );
    }
    return $value->getTimestamp();
}
throw new TypeError(
    "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
);
"#,
    )
    .expect("DatePeriod DateTimeInterface timestamp helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod DateTimeInterface timestamp helper must parse");
    method_vis(
        "__elephc_datetime_interface_timestamp",
        Visibility::Private,
        vec![param(
            "value",
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
        )],
        Some(TypeExpr::Int),
        body,
    )
}

/// Builds the typed dispatch boundary used to advance mutable and immutable cursors.
pub(super) fn date_period_add_interval() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($value instanceof DateTimeImmutable) {
    return $value->add($interval);
}
if ($value instanceof DateTime) {
    $value->add($interval);
    return $value;
}
throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
"#,
    )
    .expect("DatePeriod interval addition helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod interval addition helper must parse");
    method_vis(
        "__elephc_add_interval",
        Visibility::Private,
        vec![
            param(
                "value",
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
            ),
            param(
                "interval",
                Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
                None,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// `DatePeriod::_advance(): void` — advances the typed cursor with PHP calendar semantics.
pub(super) fn date_period_advance() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$cursor = $this->_cursor;
$interval = $this->getDateInterval();
$this->_cursor = $this->__elephc_add_interval($cursor, $interval);
"#,
    )
    .expect("DatePeriod advance body must tokenize");
    let body = crate::parser::parse(&tokens).expect("DatePeriod advance body must parse");
    method_vis("_advance", Visibility::Private, Vec::new(), Some(TypeExpr::Void), body)
}

/// `DatePeriod::rewind(): void` — clones the full start snapshot and applies start exclusion.
pub(super) fn date_period_rewind() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$this->_cursor = $this->__elephc_clone_datetime_interface_storage($this->_start);
$this->idx = 0;
if ($this->excludeStart) {
    $this->_advance();
}
"#,
    )
    .expect("DatePeriod rewind body must tokenize");
    let body = crate::parser::parse(&tokens).expect("DatePeriod rewind body must parse");
    method_vis("rewind", Visibility::Private, Vec::new(), Some(TypeExpr::Void), body)
}

/// `DatePeriod::valid(): bool`.
///
/// Count form (`useCount`): valid while the index fits the explicit recurrence count plus the
/// independently requested included end occurrence, minus an excluded start. End-date form:
/// valid while the cursor is before the end (`<=` with `INCLUDE_END_DATE`, `<` otherwise).
pub(super) fn date_period_valid() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($this->useCount) {
    $includedEnd = $this->includeEnd !== 0 ? 1 : 0;
    return $this->idx <= $this->_recurrence_count - $this->excludeStart + $includedEnd;
}
$cursor = $this->_cursor;
$end = $this->_end;
if (!($cursor instanceof DateTimeInterface) || !($end instanceof DateTimeInterface)) {
    throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
}
$cursorTimestamp = $this->__elephc_datetime_interface_timestamp($cursor);
$endTimestamp = $this->__elephc_datetime_interface_timestamp($end);
if ($cursorTimestamp < $endTimestamp) { return true; }
if ($cursorTimestamp > $endTimestamp) { return false; }
$cursorMicrosecond = $cursor->getMicrosecond();
$endMicrosecond = $end->getMicrosecond();
if ($cursorMicrosecond < $endMicrosecond) { return true; }
if ($cursorMicrosecond > $endMicrosecond) { return false; }
return $this->includeEnd !== 0;
"#,
    )
    .expect("DatePeriod valid body must tokenize");
    let body = crate::parser::parse(&tokens).expect("DatePeriod valid body must parse");
    method_vis("valid", Visibility::Private, Vec::new(), Some(TypeExpr::Bool), body)
}

pub(super) const CURRENT_SRC: &str = r#"<?php
$cursor = $this->_cursor;
if (!($cursor instanceof DateTimeInterface)) {
    throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
}
return $this->__elephc_clone_iterator_value($cursor);
"#;

/// `DatePeriod::current(): DateTimeInterface` — returns a fresh snapshot at the cursor,
/// canonicalized to the php-src iterator base class (`DateTime` or `DateTimeImmutable`).
pub(super) fn date_period_current() -> ClassMethod {
    let tokens = crate::lexer::tokenize(CURRENT_SRC).expect("current body must tokenize");
    let body = crate::parser::parse(&tokens).expect("current body must parse");
    method_vis(
        "current",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// `DatePeriod::key(): int` — returns the zero-based iteration index.
pub(super) fn date_period_key() -> ClassMethod {
    method_vis(
        "key",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Int),
        vec![ret(this_prop("idx"))],
    )
}

/// `DatePeriod::next(): void` — advances the cursor by one interval and bumps the index.
pub(super) fn date_period_next() -> ClassMethod {
    method_vis(
        "next",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Void),
        vec![
            expr_stmt(mcall(Expr::new(ExprKind::This, dummy()), "_advance", Vec::new())),
            assign_this("idx", bin(this_prop("idx"), BinOp::Add, int_lit(1))),
        ],
    )
}

pub(super) const GET_START_DATE_SRC: &str = r#"<?php
$start = $this->_start;
if (!($start instanceof DateTimeInterface)) {
    throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
}
return $this->__elephc_clone_datetime_interface($start);
"#;

pub(super) const GET_END_DATE_SRC: &str = r#"<?php
$end = $this->_end;
if ($end === null) { return null; }
return $this->__elephc_clone_datetime_interface($end);
"#;

/// `DatePeriod::getStartDate(): DateTimeInterface` — returns the start instant as the same
/// concrete class that was passed to the constructor.
pub(super) fn date_period_get_start_date() -> ClassMethod {
    let tokens = crate::lexer::tokenize(GET_START_DATE_SRC).expect("getStartDate body must tokenize");
    let body = crate::parser::parse(&tokens).expect("getStartDate body must parse");
    method(
        "getStartDate",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// `DatePeriod::getEndDate(): ?DateTimeInterface` — returns the end bound for the end-date form, or
/// `null` when the period was constructed with a recurrence count (matching PHP's nullable interface
/// return type).
pub(super) fn date_period_get_end_date() -> ClassMethod {
    let tokens = crate::lexer::tokenize(GET_END_DATE_SRC).expect("getEndDate body must tokenize");
    let body = crate::parser::parse(&tokens).expect("getEndDate body must parse");
    method(
        "getEndDate",
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateTimeInterface",
        ))))),
        body,
    )
}

/// `DatePeriod::getDateInterval(): DateInterval` — rebuilds the interval from its components.
pub(super) fn date_period_get_interval() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$interval = $this->_interval;
if (!($interval instanceof DateInterval)) {
    throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
}
return $interval->__elephc_clone();
"#,
    )
    .expect("DatePeriod getDateInterval body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod getDateInterval body must parse");
    method(
        "getDateInterval",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        body,
    )
}

/// `DatePeriod::getRecurrences(): ?int` — derives the public count from the raw serialized
/// recurrence total and inclusion flags, including deliberately unusual restored states.
pub(super) fn date_period_get_recurrences() -> ClassMethod {
    method(
        "getRecurrences",
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Int))),
        vec![if_else(
            bin(this_prop("_recurrence_count"), BinOp::Eq, int_lit(0)),
            vec![ret(null_lit())],
            Some(vec![ret(this_prop("_recurrence_count"))]),
        )],
    )
}

/// PHP source backing `DatePeriod::getIterator()`.
///
/// It snapshots fresh date values into an `InternalIterator`. The callback mirrors
/// the iterator's live cursor onto `DatePeriod::$current`, matching php-src while
/// keeping separate `getIterator()` calls independent.
pub(super) const DATEPERIOD_GET_ITERATOR_SRC: &str = r#"<?php
if (!$this->__elephc_initialized) {
    throw new DateObjectError(
        "Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor"
    );
}
$items = [];
$this->rewind();
while ($this->valid()) {
    $items[] = $this->current();
    $this->next();
}
$onCurrent = function($value): mixed {
    if ($value === null) {
        $current = $this->current();
        $result = $current;
    } else {
        $result = $this->__elephc_clone_datetime_interface($value);
        $current = $this->__elephc_clone_datetime_interface_storage($value);
    }
    $this->_current = $current;
    return $result;
};
return new InternalIterator($items, $onCurrent);
"#;

/// `DatePeriod::getIterator(): Iterator` — returns an independent internal iterator
/// over the period's dates.
pub(super) fn date_period_get_iterator() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEPERIOD_GET_ITERATOR_SRC)
        .expect("DatePeriod::getIterator body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod::getIterator body must parse");
    method(
        "getIterator",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("Iterator"))),
        body,
    )
}

/// PHP source backing `DatePeriod::createFromISO8601String()` (PHP 8.3+).
///
/// Delegates the complete grammar to the vendored php-src timelib parser, then
/// performs php-src's start/interval/end-or-recurrence validation in the same order.
#[cfg(test)]
pub(super) const CREATE_FROM_ISO8601_SRC: &str = r#"<?php
$result = null;
if (static::class === DatePeriod::class) {
    $result = __elephc_new_instance_without_constructor("DatePeriod");
} else {
    $result = __elephc_new_instance_without_constructor(static::class);
}
$typedResult = $result->__elephc_factory_result();
unset($result);
$parsed = __elephc_timelib_period_parse($specification);
if ($parsed["status"] !== "P") {
    throw new DateMalformedPeriodStringException(
        "Unknown or bad format (" . $specification . ")"
    );
}
if (!$parsed["has_start"]) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::createFromISO8601String(): ISO interval must contain a start date, \""
        . $specification . "\" given"
    );
}
if (!$parsed["has_interval"]) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::createFromISO8601String(): ISO interval must contain an interval, \""
        . $specification . "\" given"
    );
}
if (!$parsed["has_end"] && $parsed["recurrences"] === 0) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::createFromISO8601String(): ISO interval must contain an end date or a recurrence count, \""
        . $specification . "\" given"
    );
}
$start = DateTimeImmutable::createFromTimestamp($parsed["start"]);
$endTimestamp = 0;
if ($parsed["has_end"]) {
    $endTimestamp = $parsed["end"];
}
$interval = new DateInterval("PT0S");
$interval->y = $parsed["y"];
$interval->m = $parsed["m"];
$interval->d = $parsed["d"];
$interval->h = $parsed["h"];
$interval->i = $parsed["i"];
$interval->s = $parsed["s"];
$interval->f = $parsed["us"] / 1000000.0;
if ($parsed["has_end"]) {
    $typedResult->__elephc_initialize_end_components(
        $start,
        $interval,
        $endTimestamp,
        $options
    );
} else {
    $typedResult->__elephc_initialize_recurrence_components(
        $start,
        $interval,
        $parsed["recurrences"],
        $options
    );
}
unset($interval);
unset($start);
unset($parsed);
return $typedResult;
"#;

/// Builds the static `createFromISO8601String(string $specification, int $options = 0): DatePeriod`
/// method.
///
/// The body is the parsed `CREATE_FROM_ISO8601_SRC` PHP source. It forwards to the regular
/// `(start, interval, end|recurrences, options)` constructor on success and throws
/// `DateMalformedPeriodStringException` on malformed input (PHP 8.3+ never returns `false`).
pub(super) fn date_period_create_from_iso8601_string(uses_timelib: bool) -> ClassMethod {
    let body = if uses_timelib {
        super::bodies::create_from_iso8601()
    } else {
        vec![Stmt::new(
            StmtKind::Throw(Expr::new(
                ExprKind::NewObject {
                    class_name: Name::unqualified("DateMalformedPeriodStringException"),
                    args: vec![bin(
                        bin(
                            Expr::new(
                                ExprKind::StringLiteral("Unknown or bad format (".to_string()),
                                dummy(),
                            ),
                            BinOp::Concat,
                            var("specification"),
                        ),
                        BinOp::Concat,
                        Expr::new(ExprKind::StringLiteral(")".to_string()), dummy()),
                    )],
                },
                dummy(),
            )),
            dummy(),
        )]
    };
    ClassMethod {
        name: "createFromISO8601String".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param("specification", Some(TypeExpr::Str), None),
            param("options", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        // PHP 8.3+: returns a `DatePeriod` or throws (never `false`).
        return_type: Some(TypeExpr::Named(Name::unqualified("static"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the private ownership-transfer boundary for constructorless factory results.
pub(super) fn date_period_factory_result() -> ClassMethod {
    let mut result = method_vis(
        "__elephc_factory_result",
        Visibility::Public,
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DatePeriod"))),
        vec![ret(Expr::new(ExprKind::This, dummy()))],
    );
    result.is_final = true;
    result
}

/// PHP source backing the deprecated string constructor overload.
pub(super) const DEPRECATED_STRING_CONSTRUCTOR_SRC: &str = r#"<?php
__elephc_diag_warning("\nDeprecated: Calling DatePeriod::__construct(string \$isostr, int \$options = 0) is deprecated, use DatePeriod::createFromISO8601String() instead", $line, E_DEPRECATED);
return DatePeriod::createFromISO8601String($specification, $options);
"#;

/// Builds DatePeriod's weak string coercion with php-src's ordered null deprecation.
pub(super) fn date_period_weak_string_argument() -> ClassMethod {
    let source = r#"<?php
if ($value === null) {
    __elephc_diag_warning(
        "\nDeprecated: DatePeriod::__construct(): Passing null to parameter #1 (\$start) of type string is deprecated",
        $line,
        E_DEPRECATED
    );
    return "";
}
if (is_array($value) || (is_object($value) && !($value instanceof Stringable))) {
    throw new TypeError("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments");
}
return (string) $value;
"#;
    let tokens = crate::lexer::tokenize(source)
        .expect("DatePeriod weak string argument helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod weak string argument helper must parse");
    ClassMethod {
        name: "__elephc_weak_string_argument".to_string(),
        visibility: Visibility::Private,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param("value", Some(TypeExpr::Named(Name::unqualified("mixed"))), None),
            param("line", Some(TypeExpr::Int), None),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal wrapper used for `new DatePeriod(string[, int])`.
pub(super) fn date_period_deprecated_string_constructor() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DEPRECATED_STRING_CONSTRUCTOR_SRC)
        .expect("DatePeriod deprecated string constructor body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod deprecated string constructor body must parse");
    ClassMethod {
        name: "__elephc_deprecated_string_constructor".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param("specification", Some(TypeExpr::Str), None),
            param("options", Some(TypeExpr::Int), Some(int_lit(0))),
            param("line", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("DatePeriod"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing in-place initialization for the deprecated string constructor.
pub(super) const INITIALIZE_FROM_ISO8601_STRING_SRC: &str = r#"<?php
$__elephc_options = $interval;
__elephc_diag_warning("\nDeprecated: Calling DatePeriod::__construct(string \$isostr, int \$options = 0) is deprecated, use DatePeriod::createFromISO8601String() instead", $line, E_DEPRECATED);
$parsed = __elephc_timelib_period_parse($start);
if ($parsed["status"] !== "P") {
    throw new DateMalformedPeriodStringException(
        "Unknown or bad format (" . $start . ")"
    );
}
if (!$parsed["has_start"]) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::__construct(): ISO interval must contain a start date, \""
        . $start . "\" given"
    );
}
if (!$parsed["has_interval"]) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::__construct(): ISO interval must contain an interval, \""
        . $start . "\" given"
    );
}
if (!$parsed["has_end"] && $parsed["recurrences"] === 0) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::__construct(): ISO interval must contain an end date or a recurrence count, \""
        . $start . "\" given"
    );
}
$periodStart = DateTimeImmutable::createFromTimestamp($parsed["start"]);
$periodInterval = new DateInterval("PT0S");
$periodInterval->y = $parsed["y"];
$periodInterval->m = $parsed["m"];
$periodInterval->d = $parsed["d"];
$periodInterval->h = $parsed["h"];
$periodInterval->i = $parsed["i"];
$periodInterval->s = $parsed["s"];
$periodInterval->f = $parsed["us"] / 1000000.0;
if ($parsed["has_end"]) {
    $periodEnd = DateTimeImmutable::createFromTimestamp($parsed["end"]);
    $this->__construct($periodStart, $periodInterval, $periodEnd, $__elephc_options);
} else {
    $this->__construct(
        $periodStart,
        $periodInterval,
        $parsed["recurrences"],
        $__elephc_options
    );
}
"#;

/// Builds the hidden in-place initializer used after php-src-order object allocation.
pub(super) fn date_period_initialize_from_iso8601_string(uses_timelib: bool) -> ClassMethod {
    let source = if uses_timelib {
        INITIALIZE_FROM_ISO8601_STRING_SRC
    } else {
        r#"<?php
__elephc_diag_warning("\nDeprecated: Calling DatePeriod::__construct(string \$isostr, int \$options = 0) is deprecated, use DatePeriod::createFromISO8601String() instead", $line, E_DEPRECATED);
throw new DateMalformedPeriodStringException(
    "Unknown or bad format (" . $start . ")"
);
"#
    };
    let tokens = crate::lexer::tokenize(source)
        .expect("DatePeriod in-place string initializer body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod in-place string initializer body must parse");
    ClassMethod {
        name: "__elephc_initialize_from_iso8601_string".to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param("start", Some(TypeExpr::Str), None),
            param("interval", Some(TypeExpr::Int), Some(int_lit(0))),
            param("line", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the hidden runtime overload dispatcher for a preallocated DatePeriod shell.
pub(super) fn date_period_initialize_from_argument_array() -> ClassMethod {
    let src = r#"<?php
$count = count($arguments);
if ($count === 0) {
    throw new TypeError(
        "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
    );
}
$hasStart = false;
$hasInterval = false;
$hasEnd = false;
$hasOptions = false;
$nextPosition = 0;
$seenNamed = false;
foreach ($arguments as $key => $value) {
    if (is_int($key)) {
        if ($seenNamed) {
            throw new Error(
                "Cannot use positional argument after named argument during unpacking"
            );
        }
        if ($nextPosition === 0) {
            $start = $value;
            $hasStart = true;
        } elseif ($nextPosition === 1) {
            $interval = $value;
            $hasInterval = true;
        } elseif ($nextPosition === 2) {
            $end = $value;
            $hasEnd = true;
        } elseif ($nextPosition === 3) {
            $options = $value;
            $hasOptions = true;
        } else {
            throw new TypeError(
                "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
            );
        }
        $nextPosition++;
    } else {
        $seenNamed = true;
        if ($key === "start") {
            if ($hasStart) {
                throw new Error(
                    "Named parameter \$start overwrites previous argument"
                );
            }
            $start = $value;
            $hasStart = true;
        } elseif ($key === "interval") {
            if ($hasInterval) {
                throw new Error(
                    "Named parameter \$interval overwrites previous argument"
                );
            }
            $interval = $value;
            $hasInterval = true;
        } elseif ($key === "end") {
            if ($hasEnd) {
                throw new Error(
                    "Named parameter \$end overwrites previous argument"
                );
            }
            $end = $value;
            $hasEnd = true;
        } elseif ($key === "options") {
            if ($hasOptions) {
                throw new Error(
                    "Named parameter \$options overwrites previous argument"
                );
            }
            $options = $value;
            $hasOptions = true;
        } else {
            throw new Error("Unknown named parameter \$" . $key);
        }
    }
}
if (!$hasStart) {
    throw new ArgumentCountError(
        "DatePeriod::__construct(): Argument #1 (\$start) not passed"
    );
}
if (!$hasInterval && ($hasEnd || $hasOptions)) {
    throw new ArgumentCountError(
        "DatePeriod::__construct(): Argument #2 (\$interval) must be passed explicitly, because the default value is not known"
    );
}
if (!$hasEnd && $hasOptions) {
    throw new ArgumentCountError(
        "DatePeriod::__construct(): Argument #3 (\$end) must be passed explicitly, because the default value is not known"
    );
}
if (!$hasEnd) {
    $specification = $start;
    if (is_array($specification)
        || (is_object($specification) && !($specification instanceof Stringable))
    ) {
        throw new TypeError(
            "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
        );
    }
    if (is_null($specification)) {
        __elephc_diag_warning(
            "\nDeprecated: DatePeriod::__construct(): Passing null to parameter #1 (\$start) of type string is deprecated",
            $line,
            E_DEPRECATED
        );
    }
    $specification = (string) $specification;
    $stringOptions = 0;
    if ($hasInterval) {
        $stringOptions = $interval;
        if (!(is_int($stringOptions)
            || is_float($stringOptions)
            || is_bool($stringOptions)
            || is_null($stringOptions)
            || (is_string($stringOptions) && is_numeric($stringOptions)))
        ) {
            throw new TypeError(
                "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
            );
        }
        $stringOptions = (int) $stringOptions;
    }
    $this->__elephc_initialize_from_iso8601_string($specification, $stringOptions, $line);
    return;
}
if (!($start instanceof DateTimeInterface) || !($interval instanceof DateInterval)) {
    throw new TypeError(
        "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
    );
}
$objectOptions = 0;
if ($hasOptions) {
    $objectOptions = $options;
    if (!(is_int($objectOptions)
        || is_float($objectOptions)
        || is_bool($objectOptions)
        || is_null($objectOptions)
        || (is_string($objectOptions) && is_numeric($objectOptions)))
    ) {
        throw new TypeError(
            "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
        );
    }
    $objectOptions = (int) $objectOptions;
}
$this->__construct($start, $interval, $end, $objectOptions);
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod runtime overload dispatcher body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod runtime overload dispatcher body must parse");
    ClassMethod {
        name: "__elephc_initialize_from_argument_array".to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param(
                "arguments",
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
            ),
            param("line", Some(TypeExpr::Int), None),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the hidden initializer for incremental PHP argument-unpack normalization.
pub(super) fn date_period_begin_argument_array() -> ClassMethod {
    let src = r#"<?php
$this->__elephc_arguments = [];
$this->__elephc_seen_named_argument = false;
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod argument-array initializer body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod argument-array initializer body must parse");
    method_vis(
        "__elephc_begin_argument_array",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    )
}

/// Builds the hidden primitive that appends one normalized positional or named argument.
pub(super) fn date_period_append_one_argument() -> ClassMethod {
    let src = r#"<?php
$arguments = $this->__elephc_arguments;
if (is_int($key)) {
    if ($this->__elephc_seen_named_argument) {
        throw new Error(
            "Cannot use positional argument after named argument during unpacking"
        );
    }
    $arguments[] = $value;
    $this->__elephc_arguments = $arguments;
    return;
}
if (!is_string($key)) {
    throw new Error(
        "Keys must be of type int|string during argument unpacking"
    );
}
$this->__elephc_seen_named_argument = true;
if (!($key === "start"
    || $key === "interval"
    || $key === "end"
    || $key === "options")
) {
    throw new Error("Unknown named parameter \$" . $key);
}
$parameterIndex = -1;
if ($key === "start") {
    $parameterIndex = 0;
} elseif ($key === "interval") {
    $parameterIndex = 1;
} elseif ($key === "end") {
    $parameterIndex = 2;
} else {
    $parameterIndex = 3;
}
$positionalCount = 0;
foreach ($arguments as $existingKey => $existingValue) {
    if (is_int($existingKey)) {
        $positionalCount++;
    }
}
if ($parameterIndex < $positionalCount) {
    throw new Error("Named parameter \$" . $key . " overwrites previous argument");
}
if (array_key_exists($key, $arguments)) {
    throw new Error("Named parameter \$" . $key . " overwrites previous argument");
}
$arguments[$key] = $value;
$this->__elephc_arguments = $arguments;
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod single-argument append body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod single-argument append body must parse");
    method_vis(
        "__elephc_append_one_argument",
        Visibility::Private,
        vec![
            param(
                "key",
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
            ),
            param(
                "value",
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
            ),
        ],
        Some(TypeExpr::Void),
        body,
    )
}

/// Builds the hidden source-order append operation for one ordinary, named, or spread argument.
pub(super) fn date_period_append_argument_chunk() -> ClassMethod {
    let src = r#"<?php
if ($kind === 1) {
    if (!(is_array($value) || $value instanceof Traversable)) {
        DateTime::__elephc_argument_type_error(
            $value,
            "Only arrays and Traversables can be unpacked, "
        );
    }
    foreach ($value as $key => $unpackedValue) {
        $this->__elephc_append_one_argument($key, $unpackedValue);
    }
    return;
}
if ($kind === 2) {
    $this->__elephc_append_one_argument($name, $value);
    return;
}
$this->__elephc_append_one_argument(0, $value);
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod argument-chunk append body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod argument-chunk append body must parse");
    method_vis(
        "__elephc_append_argument_chunk",
        Visibility::Private,
        vec![
            param("kind", Some(TypeExpr::Int), None),
            param("name", Some(TypeExpr::Str), None),
            param(
                "value",
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
            ),
        ],
        Some(TypeExpr::Void),
        body,
    )
}

/// Builds the hidden finalizer that dispatches the normalized argument array.
pub(super) fn date_period_finish_argument_array() -> ClassMethod {
    let src = r#"<?php
$this->__elephc_initialize_from_argument_array($this->__elephc_arguments, $line);
$this->__elephc_arguments = null;
$this->__elephc_seen_named_argument = false;
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod argument-array finalizer body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod argument-array finalizer body must parse");
    method_vis(
        "__elephc_finish_argument_array",
        Visibility::Private,
        vec![param("line", Some(TypeExpr::Int), None)],
        Some(TypeExpr::Void),
        body,
    )
}

/// Builds the hidden weak-int validator for DatePeriod's `$options` overload slot.
pub(super) fn date_period_weak_options() -> ClassMethod {
    let src = r#"<?php
if (is_int($value)
    || is_float($value)
    || is_bool($value)
    || is_null($value)
    || (is_string($value) && is_numeric($value))
) {
    return (int) $value;
}
throw new TypeError(
    "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments"
);
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("DatePeriod options validator body must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("DatePeriod options validator body must parse");
    ClassMethod {
        name: "__elephc_weak_options".to_string(),
        visibility: Visibility::Private,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![param(
            "value",
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal renderer for php-src's seven-field `DatePeriod` debug shape.
pub(super) fn date_period_debug_dump() -> ClassMethod {
    let src = r#"<?php
$pad = str_repeat(" ", __elephc_var_dump_indent(0));
$field_pad = $pad . "  ";
$start = $this->start;
$current = $this->current;
$end = $this->end;
$interval = $this->interval;
$property_count = __elephc_var_dump_object_property_count($this);
echo $pad . "object(" . get_class($this) . ")#" . spl_object_id($this) . " (" . ($property_count + 7) . ") {\n";
__elephc_var_dump_indent(2);
__elephc_var_dump_object_properties($this);
__elephc_var_dump_indent(-2);
echo $field_pad . "[\"start\"]=>\n";
__elephc_var_dump_indent(2); $start->__elephc_debug_dump(); __elephc_var_dump_indent(-2);
echo $field_pad . "[\"current\"]=>\n";
if ($current === null) {
    echo $field_pad; var_dump(null);
} else {
    __elephc_var_dump_indent(2); $current->__elephc_debug_dump(); __elephc_var_dump_indent(-2);
}

echo $field_pad . "[\"end\"]=>\n";
if ($end === null) {
    echo $field_pad; var_dump(null);
} else {
    __elephc_var_dump_indent(2); $end->__elephc_debug_dump(); __elephc_var_dump_indent(-2);
}
echo $field_pad . "[\"interval\"]=>\n";
__elephc_var_dump_indent(2); $interval->__elephc_debug_dump(); __elephc_var_dump_indent(-2);
echo $field_pad . "[\"recurrences\"]=>\n"; echo $field_pad; var_dump($this->recurrences);
echo $field_pad . "[\"include_start_date\"]=>\n"; echo $field_pad; var_dump($this->include_start_date);
echo $field_pad . "[\"include_end_date\"]=>\n"; echo $field_pad; var_dump($this->include_end_date);
echo $pad . "}\n";
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod debug dump source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod debug dump source must parse");
    method(
        "__elephc_debug_dump",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    )
}

#[cfg(test)]
mod ast_migration_tests {
    use super::*;
    use crate::synthetic_class::transcribe::transcribe;

    /// Prints the direct AST-builder transcription used while removing production PHP parsing.
    #[test]
    fn transcribes_iso_factory_body() {
        if std::env::var_os("ELEPHC_DUMP_DATETIME_AST").is_none() {
            return;
        }
        let tokens = crate::lexer::tokenize(CREATE_FROM_ISO8601_SRC)
            .expect("DatePeriod ISO factory source must tokenize in the migration test");
        let body = crate::parser::parse(&tokens)
            .expect("DatePeriod ISO factory source must parse in the migration test");
        eprintln!("{}", transcribe(&body));
    }
}
