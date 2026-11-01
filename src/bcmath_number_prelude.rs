//! Purpose:
//! Declares php 8.4's `BcMath\Number` class over the `bc*` functions, and the two static helpers
//! the binary-operator lowering calls for its operator overloads.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, when the program names `BcMath\Number` and
//!   declares no class of that name itself.
//! - `crate::types::checker::inference::ops` and `crate::ir_lower::expr::bcmath_number_ops`, which
//!   route `+ - * / % **` and the comparisons on a `Number` operand to `__elephc_arith()` and
//!   `__elephc_compare()`.
//!
//! Key details:
//! - A `Number` is its decimal string and that string's fractional digit count, both `readonly`.
//!   Every operation computes through the matching `bc*` function at an explicit scale, then builds
//!   a new `Number` from the result, so the scale of the result is the digit count of what bcmath
//!   returned.
//! - The default result scales are php's: the larger operand scale for `add`/`sub`/`mod`/`compare`,
//!   their sum for `mul`, `scale * exponent` for a non-negative `pow`, and for `div`, `sqrt` and a
//!   negative `pow` ten extra digits with the trailing zeros dropped again, never below this
//!   number's own scale (`10/4` is `2.5`, `10.0/2` is `5.0`, `3/0.3` is `10`).
//! - Operator operands follow php's conversions: an int or a well-formed numeric string becomes a
//!   `Number`, a float is truncated to an int, anything else is `TypeError: Unsupported operand
//!   types`. A comparison whose operand cannot be converted is uncomparable, which php answers
//!   with 1, so `<` and `==` are false against it.
//! - Not modelled: `RoundingMode` (no such enum exists here, `round()` takes php's integer
//!   `PHP_ROUND_*` modes), the float-to-int precision-loss deprecation of an operator operand, and
//!   the `false` a zero `Number` casts to.
//! - The declarations are BUILT, never parsed.

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{
    BinOp, CastType, Expr, ExprKind, InstanceOfTarget, Program, StaticReceiver, Stmt, StmtKind,
    TypeExpr,
};
use crate::synthetic_class::{
    class, e_array, e_array_assoc, e_binop, e_call, e_cast, e_concat_all, e_index, e_instance_of,
    e_int, e_method_call, e_new, e_not, e_null, e_null_coalesce, e_self_call, e_str, e_this,
    e_this_prop, e_ternary, e_var, method, php_semantics_declarations, s_assign, s_if,
    s_prop_assign, s_return, s_throw, t_array, t_class, t_mixed, t_nullable, t_union,
};

/// The class this prelude declares, as php spells it.
pub(crate) const NUMBER_CLASS: &str = "BcMath\\Number";
/// The static helper the lowering calls for `+ - * / % **` with a `Number` operand.
pub(crate) const ARITH_HELPER: &str = "__elephc_arith";
/// The static helper the lowering calls for a comparison with a `Number` operand.
pub(crate) const COMPARE_HELPER: &str = "__elephc_compare";

/// Root-anchored spelling of the class, for the names the declarations spell.
const NUMBER_FQ: &str = "\\BcMath\\Number";

/// Whether `name` is `BcMath\Number` (any case, with or without the leading `\`).
pub(crate) fn is_number_class(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case(NUMBER_CLASS)
}

/// Whether the program names `BcMath\Number` and does not declare it itself.
pub(crate) fn needed_by(program: &Program) -> bool {
    let mut named = false;
    let mut declared = false;
    walk_nodes(program, &mut |node| match node {
        Node::Expr(expr) => match &expr.kind {
            ExprKind::NewObject { class_name, .. } => named |= is_number_class(class_name.as_str()),
            ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(name), .. }
            | ExprKind::ClassConstant { receiver: StaticReceiver::Named(name) } => {
                named |= is_number_class(name.as_str());
            }
            ExprKind::InstanceOf { target: InstanceOfTarget::Name(name), .. } => {
                named |= is_number_class(name.as_str());
            }
            _ => {}
        },
        Node::Stmt(stmt) => {
            if let StmtKind::ClassDecl { name, .. } = &stmt.kind {
                declared |= is_number_class(name);
            }
        }
    });
    named && !declared
}

/// The `Number|string|int` operand type every method takes.
fn operand_type() -> TypeExpr {
    t_union(vec![t_class(NUMBER_FQ), TypeExpr::Str, TypeExpr::Int])
}

/// `?int`.
fn nullable_int() -> TypeExpr {
    t_nullable(TypeExpr::Int)
}

/// `left === right`.
fn same(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::StrictEq, right)
}

/// `left !== right`.
fn not_same(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::StrictNotEq, right)
}

/// `left || right`.
fn or(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Or, right)
}

/// `left && right`.
fn and(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::And, right)
}

/// `new \BcMath\Number($value)`.
fn new_number(value: Expr) -> Expr {
    e_new(NUMBER_FQ, vec![value])
}

/// `throw new \<class>(<message>);`
fn throw(class_name: &str, message: Expr) -> Stmt {
    s_throw(e_new(&format!("\\{class_name}"), vec![message]))
}

/// `max($this->scale, $os)`: the larger of this number's scale and the operand's.
fn max_scale() -> Expr {
    e_call("max", vec![e_this_prop("scale"), e_var("os")])
}

/// `$scale ?? <default>`.
fn scale_or(default: Expr) -> Expr {
    e_null_coalesce(e_var("scale"), default)
}

/// `[$ov, $os] = self::operand($num, '<method>', <position>, '<param>');`, as two assignments.
fn bind_operand(source: &str, method_name: &str, position: i64, param: &str) -> Vec<Stmt> {
    vec![
        s_assign(
            "o",
            e_self_call(
                "operand",
                vec![e_var(source), e_str(method_name), e_int(position), e_str(param)],
            ),
        ),
        s_assign("ov", e_index(e_var("o"), e_int(0))),
        s_assign("os", e_index(e_var("o"), e_int(1))),
    ]
}

/// `self::checkScale($scale, '<method>', <position>);`
fn check_scale(method_name: &str, position: i64) -> Stmt {
    crate::synthetic_class::s_expr(e_self_call(
        "checkScale",
        vec![e_var("scale"), e_str(method_name), e_int(position)],
    ))
}

/// `if (bccomp($ov, '0', $os) === 0) throw new \DivisionByZeroError('<message>');`
fn refuse_zero_divisor(message: &str) -> Stmt {
    s_if(
        same(e_call("bccomp", vec![e_var("ov"), e_str("0"), e_var("os")]), e_int(0)),
        vec![throw("DivisionByZeroError", e_str(message))],
        vec![],
        None,
    )
}

/// `private static function parse(string|int $num): ?array` -- `[value, scale]`, or null when the
/// string is not a decimal number (`[+-]digits[.digits]`, at least one digit, or the empty string).
fn parse_method() -> crate::synthetic_class::MethodBuilder {
    let digits_only = |name: &str| {
        same(
            e_call("strspn", vec![e_var(name), e_str("0123456789")]),
            e_call("strlen", vec![e_var(name)]),
        )
    };
    method("parse")
        .private()
        .static_()
        .param("num", t_union(vec![TypeExpr::Str, TypeExpr::Int]))
        .returns(t_nullable(t_array()))
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("num")]),
                vec![s_return(e_array(vec![e_cast(CastType::String, e_var("num")), e_int(0)]))],
                vec![],
                None,
            ),
            s_if(
                same(e_var("num"), e_str("")),
                vec![s_return(e_array(vec![e_str("0"), e_int(0)]))],
                vec![],
                None,
            ),
            s_assign("negative", e_bool_false()),
            s_assign("first", e_index(e_var("num"), e_int(0))),
            s_if(
                or(same(e_var("first"), e_str("+")), same(e_var("first"), e_str("-"))),
                vec![
                    s_assign("negative", same(e_var("first"), e_str("-"))),
                    s_assign("num", e_call("substr", vec![e_var("num"), e_int(1)])),
                ],
                vec![],
                None,
            ),
            s_assign("dot", e_call("strpos", vec![e_var("num"), e_str(".")])),
            s_assign(
                "int",
                e_ternary(
                    same(e_var("dot"), e_bool_false()),
                    e_var("num"),
                    e_call("substr", vec![e_var("num"), e_int(0), e_var("dot")]),
                ),
            ),
            s_assign(
                "frac",
                e_ternary(
                    same(e_var("dot"), e_bool_false()),
                    e_str(""),
                    e_call(
                        "substr",
                        vec![e_var("num"), e_binop(e_var("dot"), BinOp::Add, e_int(1))],
                    ),
                ),
            ),
            s_if(
                or(
                    and(same(e_var("int"), e_str("")), same(e_var("frac"), e_str(""))),
                    or(e_not(digits_only("int")), e_not(digits_only("frac"))),
                ),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_assign("int", e_call("ltrim", vec![e_var("int"), e_str("0")])),
            s_if(
                same(e_var("int"), e_str("")),
                vec![s_assign("int", e_str("0"))],
                vec![],
                None,
            ),
            s_assign(
                "value",
                e_ternary(
                    same(e_var("frac"), e_str("")),
                    e_var("int"),
                    e_concat_all(vec![e_var("int"), e_str("."), e_var("frac")]),
                ),
            ),
            // A zero is never negative: `-0.0` is `0.0`.
            s_if(
                and(
                    e_var("negative"),
                    not_same(
                        e_call("strspn", vec![e_var("value"), e_str("0.")]),
                        e_call("strlen", vec![e_var("value")]),
                    ),
                ),
                vec![s_assign("value", e_concat_all(vec![e_str("-"), e_var("value")]))],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_var("value"), e_call("strlen", vec![e_var("frac")])])),
        ])
}

/// `false`.
fn e_bool_false() -> Expr {
    crate::synthetic_class::e_bool(false)
}

/// `private static function operand(Number|string|int $num, string $method, int $position,
/// string $param): array` -- a method argument as `[value, scale]`, or php's ValueError.
fn operand_method() -> crate::synthetic_class::MethodBuilder {
    method("operand")
        .private()
        .static_()
        .param("num", operand_type())
        .param("method", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("param", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_if(
                e_instance_of(e_var("num"), NUMBER_FQ),
                vec![s_return(e_array(vec![
                    crate::synthetic_class::e_prop(e_var("num"), "value"),
                    crate::synthetic_class::e_prop(e_var("num"), "scale"),
                ]))],
                vec![],
                None,
            ),
            s_assign("parsed", e_self_call("parse", vec![e_var("num")])),
            s_if(
                same(e_var("parsed"), e_null()),
                vec![throw(
                    "ValueError",
                    e_concat_all(vec![
                        e_str("BcMath\\Number::"),
                        e_var("method"),
                        e_str("(): Argument #"),
                        e_var("position"),
                        e_str(" ($"),
                        e_var("param"),
                        e_str(") is not well-formed"),
                    ]),
                )],
                vec![],
                None,
            ),
            s_return(e_var("parsed")),
        ])
}

/// `private static function checkScale(?int $scale, string $method, int $position): void`.
fn check_scale_method() -> crate::synthetic_class::MethodBuilder {
    method("checkScale")
        .private()
        .static_()
        .param("scale", nullable_int())
        .param("method", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![s_if(
            and(
                not_same(e_var("scale"), e_null()),
                or(
                    e_binop(e_var("scale"), BinOp::Lt, e_int(0)),
                    e_binop(e_var("scale"), BinOp::Gt, e_int(2147483647)),
                ),
            ),
            vec![throw(
                "ValueError",
                e_concat_all(vec![
                    e_str("BcMath\\Number::"),
                    e_var("method"),
                    e_str("(): Argument #"),
                    e_var("position"),
                    e_str(" ($scale) must be between 0 and 2147483647"),
                ]),
            )],
            vec![],
            None,
        )])
}

/// `private static function trim(string $value, int $min): string` -- the trailing fractional
/// zeros dropped, keeping at least `$min` digits.
fn trim_method() -> crate::synthetic_class::MethodBuilder {
    method("trim")
        .private()
        .static_()
        .param("value", TypeExpr::Str)
        .param("min", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("dot", e_call("strpos", vec![e_var("value"), e_str(".")])),
            s_if(
                same(e_var("dot"), e_bool_false()),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_assign(
                "keep",
                e_call(
                    "strlen",
                    vec![e_call(
                        "rtrim",
                        vec![
                            e_call(
                                "substr",
                                vec![e_var("value"), e_binop(e_var("dot"), BinOp::Add, e_int(1))],
                            ),
                            e_str("0"),
                        ],
                    )],
                ),
            ),
            s_if(
                e_binop(e_var("keep"), BinOp::Lt, e_var("min")),
                vec![s_assign("keep", e_var("min"))],
                vec![],
                None,
            ),
            s_if(
                same(e_var("keep"), e_int(0)),
                vec![s_return(e_call("substr", vec![e_var("value"), e_int(0), e_var("dot")]))],
                vec![],
                None,
            ),
            s_return(e_call(
                "substr",
                vec![
                    e_var("value"),
                    e_int(0),
                    e_binop(
                        e_binop(e_var("dot"), BinOp::Add, e_int(1)),
                        BinOp::Add,
                        e_var("keep"),
                    ),
                ],
            )),
        ])
}

/// A binary method (`add`, `sub`, `mul`, `mod`) over `bc<function>` at `default_scale`.
fn binary_method(
    name: &str,
    bc_function: &str,
    default_scale: Expr,
    zero_message: Option<&str>,
) -> crate::synthetic_class::MethodBuilder {
    let mut body = bind_operand("num", name, 1, "num");
    body.push(check_scale(name, 2));
    if let Some(message) = zero_message {
        body.push(refuse_zero_divisor(message));
    }
    body.push(s_return(new_number(e_call(
        bc_function,
        vec![e_this_prop("value"), e_var("ov"), scale_or(default_scale)],
    ))));
    method(name)
        .param("num", operand_type())
        .param_default("scale", nullable_int(), e_null())
        .returns(t_class(NUMBER_FQ))
        .body(body)
}

/// `div()`: ten digits past the larger operand scale, trimmed back to the dividend's scale, when no
/// scale is given.
fn div_method() -> crate::synthetic_class::MethodBuilder {
    let mut body = bind_operand("num", "div", 1, "num");
    body.push(check_scale("div", 2));
    body.push(refuse_zero_divisor("Division by zero"));
    body.push(s_if(
        not_same(e_var("scale"), e_null()),
        vec![s_return(new_number(e_call(
            "bcdiv",
            vec![e_this_prop("value"), e_var("ov"), e_var("scale")],
        )))],
        vec![],
        None,
    ));
    body.push(s_return(new_number(e_self_call(
        "trim",
        vec![
            e_call(
                "bcdiv",
                vec![e_this_prop("value"), e_var("ov"), e_binop(max_scale(), BinOp::Add, e_int(10))],
            ),
            e_this_prop("scale"),
        ],
    ))));
    method("div")
        .param("num", operand_type())
        .param_default("scale", nullable_int(), e_null())
        .returns(t_class(NUMBER_FQ))
        .body(body)
}

/// `divmod()`: the integer quotient and the remainder at the larger operand scale.
fn divmod_method() -> crate::synthetic_class::MethodBuilder {
    let mut body = bind_operand("num", "divmod", 1, "num");
    body.push(check_scale("divmod", 2));
    body.push(refuse_zero_divisor("Division by zero"));
    body.push(s_assign(
        "pair",
        e_call(
            "bcdivmod",
            vec![e_this_prop("value"), e_var("ov"), scale_or(max_scale())],
        ),
    ));
    body.push(s_return(e_array(vec![
        new_number(e_index(e_var("pair"), e_int(0))),
        new_number(e_index(e_var("pair"), e_int(1))),
    ])));
    method("divmod")
        .param("num", operand_type())
        .param_default("scale", nullable_int(), e_null())
        .returns(t_array())
        .body(body)
}

/// `if ($es > 0 && strspn(substr($ev, strpos($ev, '.') + 1), '0') !== $es) throw ...;` -- an
/// exponent with a non-zero fractional digit.
fn refuse_fractional_exponent(message: &str) -> Stmt {
    s_if(
        and(
            e_binop(e_var("es"), BinOp::Gt, e_int(0)),
            not_same(
                e_call(
                    "strspn",
                    vec![
                        e_call(
                            "substr",
                            vec![
                                e_var("ev"),
                                e_binop(
                                    e_call("strpos", vec![e_var("ev"), e_str(".")]),
                                    BinOp::Add,
                                    e_int(1),
                                ),
                            ],
                        ),
                        e_str("0"),
                    ],
                ),
                e_var("es"),
            ),
        ),
        vec![throw("ValueError", e_str(message))],
        vec![],
        None,
    )
}

/// `[$ev, $es] = self::operand($exponent, ...)`.
fn bind_exponent(method_name: &str) -> Vec<Stmt> {
    vec![
        s_assign(
            "e",
            e_self_call(
                "operand",
                vec![e_var("exponent"), e_str(method_name), e_int(1), e_str("exponent")],
            ),
        ),
        s_assign("ev", e_index(e_var("e"), e_int(0))),
        s_assign("es", e_index(e_var("e"), e_int(1))),
    ]
}

/// `pow()`: `scale * exponent` digits for a non-negative exponent, ten extra (trimmed) otherwise.
fn pow_method() -> crate::synthetic_class::MethodBuilder {
    let mut body = bind_exponent("pow");
    body.push(check_scale("pow", 2));
    body.push(refuse_fractional_exponent(
        "BcMath\\Number::pow(): Argument #1 ($exponent) exponent cannot have a fractional part",
    ));
    body.push(s_assign("n", e_cast(CastType::Int, e_var("ev"))));
    let power = |scale: Expr| {
        e_call(
            "bcpow",
            vec![e_this_prop("value"), e_cast(CastType::String, e_var("n")), scale],
        )
    };
    body.push(s_if(
        not_same(e_var("scale"), e_null()),
        vec![s_return(new_number(power(e_var("scale"))))],
        vec![],
        None,
    ));
    body.push(s_if(
        e_binop(e_var("n"), BinOp::GtEq, e_int(0)),
        vec![s_return(new_number(power(e_binop(
            e_this_prop("scale"),
            BinOp::Mul,
            e_var("n"),
        ))))],
        vec![],
        None,
    ));
    body.push(s_if(
        same(
            e_call("bccomp", vec![e_this_prop("value"), e_str("0"), e_this_prop("scale")]),
            e_int(0),
        ),
        vec![throw("DivisionByZeroError", e_str("Negative power of zero"))],
        vec![],
        None,
    ));
    body.push(s_return(new_number(e_self_call(
        "trim",
        vec![
            power(e_binop(e_this_prop("scale"), BinOp::Add, e_int(10))),
            e_this_prop("scale"),
        ],
    ))));
    method("pow")
        .param("exponent", operand_type())
        .param_default("scale", nullable_int(), e_null())
        .returns(t_class(NUMBER_FQ))
        .body(body)
}

/// `powmod()`: integral exponent and modulus, scale 0 by default.
fn powmod_method() -> crate::synthetic_class::MethodBuilder {
    let mut body = bind_exponent("powmod");
    body.push(check_scale("powmod", 3));
    body.push(refuse_fractional_exponent(
        "BcMath\\Number::powmod(): Argument #1 ($exponent) cannot have a fractional part",
    ));
    body.push(s_assign(
        "m",
        e_self_call(
            "operand",
            vec![e_var("modulus"), e_str("powmod"), e_int(2), e_str("modulus")],
        ),
    ));
    body.push(s_return(new_number(e_call(
        "bcpowmod",
        vec![
            e_this_prop("value"),
            e_var("ev"),
            e_index(e_var("m"), e_int(0)),
            scale_or(e_int(0)),
        ],
    ))));
    method("powmod")
        .param("exponent", operand_type())
        .param("modulus", operand_type())
        .param_default("scale", nullable_int(), e_null())
        .returns(t_class(NUMBER_FQ))
        .body(body)
}

/// `sqrt()`: ten extra digits trimmed back to this number's scale when no scale is given.
fn sqrt_method() -> crate::synthetic_class::MethodBuilder {
    method("sqrt")
        .param_default("scale", nullable_int(), e_null())
        .returns(t_class(NUMBER_FQ))
        .body(vec![
            check_scale("sqrt", 1),
            s_if(
                e_binop(
                    e_call("bccomp", vec![e_this_prop("value"), e_str("0"), e_this_prop("scale")]),
                    BinOp::Lt,
                    e_int(0),
                ),
                vec![throw("ValueError", e_str("Base number must be greater than or equal to 0"))],
                vec![],
                None,
            ),
            s_if(
                not_same(e_var("scale"), e_null()),
                vec![s_return(new_number(e_call(
                    "bcsqrt",
                    vec![e_this_prop("value"), e_var("scale")],
                )))],
                vec![],
                None,
            ),
            s_return(new_number(e_self_call(
                "trim",
                vec![
                    e_call(
                        "bcsqrt",
                        vec![
                            e_this_prop("value"),
                            e_binop(e_this_prop("scale"), BinOp::Add, e_int(10)),
                        ],
                    ),
                    e_this_prop("scale"),
                ],
            ))),
        ])
}

/// `floor()` / `ceil()`: an integral `Number` through `bc<name>`.
fn integral_method(name: &str) -> crate::synthetic_class::MethodBuilder {
    method(name)
        .returns(t_class(NUMBER_FQ))
        .body(vec![s_return(new_number(e_call(
            &format!("bc{name}"),
            vec![e_this_prop("value")],
        )))])
}

/// `round()`: `bcround()` with php's integer `PHP_ROUND_*` mode (see the module notes).
fn round_method() -> crate::synthetic_class::MethodBuilder {
    method("round")
        .param_default("precision", TypeExpr::Int, e_int(0))
        .param_default("mode", TypeExpr::Int, e_int(1))
        .returns(t_class(NUMBER_FQ))
        .body(vec![s_return(new_number(e_call(
            "bcround",
            vec![e_this_prop("value"), e_var("precision"), e_var("mode")],
        )))])
}

/// `compare()`: -1, 0 or 1 at the larger operand scale.
fn compare_method() -> crate::synthetic_class::MethodBuilder {
    let mut body = bind_operand("num", "compare", 1, "num");
    body.push(check_scale("compare", 2));
    body.push(s_return(e_call(
        "bccomp",
        vec![e_this_prop("value"), e_var("ov"), scale_or(max_scale())],
    )));
    method("compare")
        .param("num", operand_type())
        .param_default("scale", nullable_int(), e_null())
        .returns(TypeExpr::Int)
        .body(body)
}

/// `private static function operatorOperand(mixed $value): ?Number` -- an operator operand as a
/// `Number`: a float is truncated, an int or well-formed string converted; null for anything else
/// (the caller tells a malformed string apart, which php reports with its own ValueError).
fn operator_operand_method() -> crate::synthetic_class::MethodBuilder {
    method("operatorOperand")
        .private()
        .static_()
        .param("value", t_mixed())
        .returns(t_nullable(t_class(NUMBER_FQ)))
        .body(vec![
            s_if(
                e_instance_of(e_var("value"), NUMBER_FQ),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![s_assign("value", e_cast(CastType::Int, e_var("value")))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![s_return(new_number(e_var("value")))],
                vec![],
                None,
            ),
            s_if(
                and(
                    e_call("is_string", vec![e_var("value")]),
                    not_same(e_self_call("parse", vec![e_var("value")]), e_null()),
                ),
                vec![s_return(new_number(e_var("value")))],
                vec![],
                None,
            ),
            s_return(e_null()),
        ])
}

/// `public static function __elephc_arith(string $op, mixed $left, mixed $right): Number`.
fn arith_helper() -> crate::synthetic_class::MethodBuilder {
    let unsupported = || {
        throw(
            "TypeError",
            e_concat_all(vec![
                e_str("Unsupported operand types: "),
                e_call("get_debug_type", vec![e_var("left")]),
                e_str(" "),
                e_var("op"),
                e_str(" "),
                e_call("get_debug_type", vec![e_var("right")]),
            ]),
        )
    };
    let refuse_side = |side: &str, value: &str| {
        s_if(
            same(e_var(&format!("{value}n")), e_null()),
            vec![
                s_if(
                    e_call("is_string", vec![e_var(value)]),
                    vec![throw(
                        "ValueError",
                        e_str(&format!("{side} string operand cannot be converted to BcMath\\Number")),
                    )],
                    vec![],
                    None,
                ),
                unsupported(),
            ],
            vec![],
            None,
        )
    };
    let arm = |op: &str, call: Expr| (same(e_var("op"), e_str(op)), vec![s_return(call)]);
    let on = |name: &str| e_method_call(e_var("leftn"), name, vec![e_var("rightn")]);
    method(ARITH_HELPER)
        .static_()
        .param("op", TypeExpr::Str)
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(t_class(NUMBER_FQ))
        .body(vec![
            s_assign("leftn", e_self_call("operatorOperand", vec![e_var("left")])),
            s_assign("rightn", e_self_call("operatorOperand", vec![e_var("right")])),
            refuse_side("Left", "left"),
            refuse_side("Right", "right"),
            s_if(
                same(e_var("op"), e_str("+")),
                vec![s_return(on("add"))],
                vec![
                    arm("-", on("sub")),
                    arm("*", on("mul")),
                    arm("/", on("div")),
                    arm("%", on("mod")),
                ],
                Some(vec![s_return(on("pow"))]),
            ),
        ])
}

/// `public static function __elephc_compare(mixed $left, mixed $right): int` -- an uncomparable
/// operand answers 1, php's `ZEND_UNCOMPARABLE`.
fn compare_helper() -> crate::synthetic_class::MethodBuilder {
    method(COMPARE_HELPER)
        .static_()
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("leftn", e_self_call("operatorOperand", vec![e_var("left")])),
            s_assign("rightn", e_self_call("operatorOperand", vec![e_var("right")])),
            s_if(
                or(same(e_var("leftn"), e_null()), same(e_var("rightn"), e_null())),
                vec![s_return(e_int(1))],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("leftn"), "compare", vec![e_var("rightn")])),
        ])
}

/// Returns the `BcMath\Number` declaration.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        let construct_body = vec![
            s_assign("parsed", e_self_call("parse", vec![e_var("num")])),
            s_if(
                same(e_var("parsed"), e_null()),
                vec![throw(
                    "ValueError",
                    e_str("BcMath\\Number::__construct(): Argument #1 ($num) is not well-formed"),
                )],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "value", e_index(e_var("parsed"), e_int(0))),
            s_prop_assign(e_this(), "scale", e_index(e_var("parsed"), e_int(1))),
        ];
        let unserialize_body = vec![
            s_assign("value", e_null_coalesce(e_index(e_var("data"), e_str("value")), e_null())),
            s_assign(
                "parsed",
                e_ternary(
                    e_call("is_string", vec![e_var("value")]),
                    e_self_call("parse", vec![e_var("value")]),
                    e_null(),
                ),
            ),
            s_if(
                same(e_var("parsed"), e_null()),
                vec![throw("ValueError", e_str("Invalid serialization data for BcMath\\Number object"))],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "value", e_index(e_var("parsed"), e_int(0))),
            s_prop_assign(e_this(), "scale", e_index(e_var("parsed"), e_int(1))),
        ];
        // The backend-gap preludes join the program after name resolution, so the class is
        // declared by its resolved name rather than inside a `namespace BcMath` block.
        let number = class(NUMBER_CLASS)
            .final_()
            .readonly_prop("value", TypeExpr::Str)
            .readonly_prop("scale", TypeExpr::Int)
            .method(
                method("__construct")
                    .param("num", t_union(vec![TypeExpr::Str, TypeExpr::Int]))
                    .body(construct_body),
            )
            .method(binary_method("add", "bcadd", max_scale(), None))
            .method(binary_method("sub", "bcsub", max_scale(), None))
            .method(binary_method(
                "mul",
                "bcmul",
                e_binop(e_this_prop("scale"), BinOp::Add, e_var("os")),
                None,
            ))
            .method(div_method())
            .method(binary_method("mod", "bcmod", max_scale(), Some("Modulo by zero")))
            .method(divmod_method())
            .method(pow_method())
            .method(powmod_method())
            .method(sqrt_method())
            .method(integral_method("floor"))
            .method(integral_method("ceil"))
            .method(round_method())
            .method(compare_method())
            .method(
                method("__toString")
                    .returns(TypeExpr::Str)
                    .body(vec![s_return(e_this_prop("value"))]),
            )
            .method(
                method("__serialize")
                    .returns(t_array())
                    .body(vec![s_return(e_array_assoc(vec![(
                        e_str("value"),
                        e_this_prop("value"),
                    )]))]),
            )
            .method(
                method("__unserialize")
                    .param("data", t_array())
                    .returns(TypeExpr::Void)
                    .body(unserialize_body),
            )
            .method(arith_helper())
            .method(compare_helper())
            .method(operator_operand_method())
            .method(operand_method())
            .method(check_scale_method())
            .method(trim_method())
            .method(parse_method())
            .build();
        vec![number]
    })
}

thread_local! {
    /// The operations the checker typed as a `Number` overload (`CheckResult::bcmath_operator_sites`),
    /// published for one program-lowering invocation. The lowering asks this instead of re-deriving
    /// the operand types, so the two cannot disagree about which `+` is a call.
    static OPERATOR_SITES: std::cell::RefCell<std::collections::HashSet<crate::span::Span>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Restores the enclosing lowering invocation's operator sites on drop.
pub(crate) struct OperatorSitesGuard {
    previous: std::collections::HashSet<crate::span::Span>,
}

impl Drop for OperatorSitesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        OPERATOR_SITES.with(|sites| *sites.borrow_mut() = previous);
    }
}

/// Publishes the checker's `Number` operator sites for one program-lowering invocation.
pub(crate) fn set_operator_sites(
    sites: std::collections::HashSet<crate::span::Span>,
) -> OperatorSitesGuard {
    let previous = OPERATOR_SITES.with(|set| std::mem::replace(&mut *set.borrow_mut(), sites));
    OperatorSitesGuard { previous }
}

/// Whether the checker typed the operation at `span` as a `Number` overload.
pub(crate) fn is_operator_site(span: crate::span::Span) -> bool {
    OPERATOR_SITES.with(|sites| sites.borrow().contains(&span))
}

/// Whether a checker type is a `Number` object.
pub(crate) fn is_number_type(ty: &crate::types::PhpType) -> bool {
    matches!(ty, crate::types::PhpType::Object(name) if is_number_class(name))
}

/// The operator a binary arithmetic `BinOp` names in php's messages, when `Number` overloads it.
pub(crate) fn arith_operator_symbol(op: &BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Pow => "**",
        _ => return None,
    })
}

/// The expression php evaluates for `-value` on a `Number`: a multiplication by `-1`, whose scale is
/// the operand's (`-$n` of `1.52` is `-1.52`).
pub(crate) fn negated_number_expr(value: &Expr, span: crate::span::Span) -> Expr {
    overloaded_binary_expr(
        value,
        &BinOp::Mul,
        &Expr::new(ExprKind::IntLiteral(-1), crate::span::Span::dummy()),
        span,
    )
    .expect("multiplication is overloaded")
}

/// The expression php evaluates for `left <op> right` with a `Number` operand: a call to one of the
/// static helpers, compared against zero for an ordering. php orders `a > b` as `b < a`, which is
/// what keeps an uncomparable operand false both ways.
pub(crate) fn overloaded_binary_expr(left: &Expr, op: &BinOp, right: &Expr, span: crate::span::Span) -> Option<Expr> {
    let call = |method_name: &str, args: Vec<Expr>| {
        Expr::new(
            ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(crate::synthetic_class::class_name(NUMBER_FQ)),
                method: method_name.to_string(),
                args,
            },
            span,
        )
    };
    if let Some(symbol) = arith_operator_symbol(op) {
        return Some(call(
            ARITH_HELPER,
            vec![Expr::new(ExprKind::StringLiteral(symbol.to_string()), span), left.clone(), right.clone()],
        ));
    }
    let compare = |a: &Expr, b: &Expr| call(COMPARE_HELPER, vec![a.clone(), b.clone()]);
    // The comparison against zero is a plain int comparison, and it must not carry the overloaded
    // operation's span: the lowering would take it for that operation again.
    let zero = Expr::new(ExprKind::IntLiteral(0), crate::span::Span::dummy());
    let against_zero = |value: Expr, op: BinOp| {
        Expr::new(
            ExprKind::BinaryOp { left: Box::new(value), op, right: Box::new(zero.clone()) },
            crate::span::Span::dummy(),
        )
    };
    Some(match op {
        BinOp::Spaceship => compare(left, right),
        BinOp::Lt => against_zero(compare(left, right), BinOp::Lt),
        BinOp::LtEq => against_zero(compare(left, right), BinOp::LtEq),
        BinOp::Gt => against_zero(compare(right, left), BinOp::Lt),
        BinOp::GtEq => against_zero(compare(right, left), BinOp::LtEq),
        BinOp::Eq => against_zero(compare(left, right), BinOp::StrictEq),
        BinOp::NotEq => against_zero(compare(left, right), BinOp::StrictNotEq),
        _ => return None,
    })
}
