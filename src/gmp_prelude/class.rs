//! Purpose:
//! Builds the synthetic `GMP` value class: storage, casts, serialization and PHP 8.5's
//! operator adapters over the prelude validators.
//!
//! Called from:
//! - `crate::gmp_prelude::declarations`.
//!
//! Key details:
//! - The value lives in `$num`'s public storage slot `__elephc_num`; `__debugInfo()`
//!   projects it as PHP's virtual `num` property.
//! - The class is final. It exposes only `__construct`, `__serialize` and `__unserialize`
//!   to PHP; a hidden `__elephc_legacy_unserialize` hook lets the runtime keep accepting
//!   php-src's legacy `C:3:"GMP":...` payload without implementing `Serializable`.
//! - Operator adapters take `mixed` operands so the validator, not the EEL checker,
//!   owns php-src's weak coercion and diagnostics.

use super::dsl::*;
use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds the final `GMP` class declaration.
pub(super) fn gmp_class() -> Stmt {
    let class = class("GMP")
        .final_()
        .prop("__elephc_num", TypeExpr::Str, None)
        .method(constructor())
        .method(to_string())
        .method(debug_info())
        .method(serialize_magic())
        .method(unserialize_magic())
        .method(legacy_unserialize())
        .method(forbidden_legacy_method("serialize", false))
        .method(forbidden_legacy_method("unserialize", true))
        .method(cast_int())
        .method(cast_float())
        .method(cast_number())
        .method(cast_bool())
        .method(arithmetic("__elephc_add", BIN_ADD, "Division by zero"))
        .method(arithmetic("__elephc_sub", BIN_SUB, "Division by zero"))
        .method(arithmetic("__elephc_mul", BIN_MUL, "Division by zero"))
        .method(arithmetic("__elephc_div", BIN_TDIV_Q, "Division by zero"))
        .method(arithmetic("__elephc_mod", BIN_MOD, "Modulo by zero"))
        .method(power())
        .method(shift("__elephc_shl", UN_SHL, "<<", "Shift"))
        .method(shift("__elephc_shr", UN_SHR, ">>", "Shift"))
        .method(arithmetic("__elephc_bitand", BIN_AND, "Division by zero"))
        .method(arithmetic("__elephc_bitor", BIN_OR, "Division by zero"))
        .method(arithmetic("__elephc_bitxor", BIN_XOR, "Division by zero"))
        .method(compare())
        .method(boolean())
        .build();
    let mut class = class;
    class.attributes.push(attr("AllowDynamicProperties", Vec::new()));
    class
}

/// `GMP::__construct(string|int $num = 0, int $base = 0)`.
fn constructor() -> MethodBuilder {
    method("__construct")
        .param_default("num", TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::Int]), e_int(0))
        .param_default("base", TypeExpr::Int, e_int(0))
        .body(vec![
            if_(
                and(
                    bin(v("base"), BinOp::StrictNotEq, i(0)),
                    or(bin(v("base"), BinOp::Lt, i(2)), bin(v("base"), BinOp::Gt, i(62))),
                ),
                vec![throw_new(
                    "ValueError",
                    s("GMP::__construct(): Argument #2 ($base) must be 0 or between 2 and 62"),
                )],
                Vec::new(),
                None,
            ),
            // `new GMP(null)` deprecates and constructs zero; floats deprecate or report
            // php-src's ValueError, and numeric strings honor `$base`. The shared init
            // operand helper owns those weak-coercion rules for the constructor too.
            set_prop(
                this(),
                "__elephc_num",
                call(
                    "__elephc_gmp_init_operand",
                    vec![
                        v("num"),
                        b(false),
                        s("GMP::__construct"),
                        i(1),
                        s("num"),
                        v("base"),
                    ],
                ),
            ),
        ])
}

/// `__toString(): string`.
fn to_string() -> MethodBuilder {
    method("__toString").returns(TypeExpr::Str).returning(prop("__elephc_num"))
}

/// Foldable `__debugInfo(): array` projection: only the virtual `num` property.
fn debug_info() -> MethodBuilder {
    method("__debugInfo")
        .returns(t_array())
        .returning(assoc(vec![(s("num"), prop("__elephc_num"))]))
}

/// `__serialize(): array` — hex value first, dynamic properties second when present.
fn serialize_magic() -> MethodBuilder {
    method("__serialize")
        .returns(t_array())
        .body(vec![
            assign(
                "result",
                array(vec![call(
                    "__elephc_gmp_value",
                    vec![call("elephc_gmp_strval", vec![prop("__elephc_num"), i(16)])],
                )]),
            ),
            assign("properties", call("get_object_vars", vec![this()])),
            expr(call(
                "unset",
                vec![index(v("properties"), s("__elephc_num"))],
            )),
            if_(
                bin(v("properties"), BinOp::StrictNotEq, array(Vec::new())),
                vec![push("result", v("properties"))],
                Vec::new(),
                None,
            ),
            ret(v("result")),
        ])
}

/// `__unserialize(array $data): void`.
fn unserialize_magic() -> MethodBuilder {
    method("__unserialize")
        .param("data", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            if_(
                or(
                    not(call("array_key_exists", vec![i(0), v("data")])),
                    not(call("is_string", vec![index(v("data"), i(0))])),
                ),
                vec![throw_new("Exception", s("Could not unserialize number"))],
                Vec::new(),
                None,
            ),
            assign(
                "packet",
                call("elephc_gmp_parse", vec![index(v("data"), i(0)), i(16)]),
            ),
            if_(
                bin(index(v("packet"), i(0)), BinOp::StrictNotEq, s("O")),
                vec![throw_new("Exception", s("Could not unserialize number"))],
                Vec::new(),
                None,
            ),
            set_prop(
                this(),
                "__elephc_num",
                call("substr", vec![v("packet"), i(1)]),
            ),
            if_(
                call("array_key_exists", vec![i(1), v("data")]),
                vec![
                    if_(
                        not(call("is_array", vec![index(v("data"), i(1))])),
                        vec![throw_new("Exception", s("Could not unserialize properties"))],
                        Vec::new(),
                        None,
                    ),
                    foreach_(
                        index(v("data"), i(1)),
                        Some("name"),
                        "value",
                        vec![expr(e_assign(
                            e_dyn_prop(this(), v("name")),
                            v("value"),
                        ))],
                    ),
                ],
                Vec::new(),
                None,
            ),
        ])
}

/// Hidden `serialize()`/`unserialize()` methods that PHP reports as undefined.
///
/// `GMP` no longer implements `Serializable`, but the compiler still needs a callable
/// symbol so a source-written `$g->serialize()` reaches php-src's runtime `Error` instead
/// of a compile-time "Undefined method" diagnostic. The class contract keeps both names
/// out of `method_exists()`, reflection and `get_class_methods()`.
fn forbidden_legacy_method(name: &'static str, takes_data: bool) -> MethodBuilder {
    let mut method = method(name).returns(t_mixed());
    if takes_data {
        method = method.param("data", TypeExpr::Str);
    }
    method.body(vec![throw_new(
        "Error",
        s(&format!("Call to undefined method GMP::{name}()")),
    )])
}

/// Hidden `__elephc_legacy_unserialize(string $data): void` hook for the legacy
/// `C:3:"GMP":...` payload. PHP does not expose `Serializable` on `GMP`; the runtime
/// calls this symbol directly while decoding the backward-compatible wire format.
fn legacy_unserialize() -> MethodBuilder {
    method("__elephc_legacy_unserialize")
        .param("data", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            if_(
                or(
                    bin(call("strlen", vec![v("data")]), BinOp::Lt, i(4)),
                    or(
                        bin(index(v("data"), i(0)), BinOp::StrictNotEq, s("s")),
                        bin(index(v("data"), i(1)), BinOp::StrictNotEq, s(":")),
                    ),
                ),
                vec![throw_new("Exception", s("Could not unserialize number"))],
                Vec::new(),
                None,
            ),
            assign("colon", call("strpos", vec![v("data"), s(":"), i(2)])),
            if_(
                bin(v("colon"), BinOp::StrictEq, b(false)),
                vec![throw_new("Exception", s("Could not unserialize number"))],
                Vec::new(),
                None,
            ),
            assign(
                "length",
                as_int(call("substr", vec![v("data"), i(2), bin(v("colon"), BinOp::Sub, i(2))])),
            ),
            assign("start", bin(v("colon"), BinOp::Add, i(2))),
            if_(
                or(
                    bin(
                        bin(v("start"), BinOp::Add, v("length")),
                        BinOp::Gt,
                        call("strlen", vec![v("data")]),
                    ),
                    or(
                        bin(index(v("data"), bin(v("start"), BinOp::Sub, i(1))), BinOp::StrictNotEq, s("\"")),
                        or(
                            bin(
                                index(v("data"), bin(v("start"), BinOp::Add, v("length"))),
                                BinOp::StrictNotEq,
                                s("\""),
                            ),
                            bin(
                                index(
                                    v("data"),
                                    bin(bin(v("start"), BinOp::Add, v("length")), BinOp::Add, i(1)),
                                ),
                                BinOp::StrictNotEq,
                                s(";"),
                            ),
                        ),
                    ),
                ),
                vec![throw_new("Exception", s("Could not unserialize number"))],
                Vec::new(),
                None,
            ),
            assign(
                "number",
                call("substr", vec![v("data"), v("start"), v("length")]),
            ),
            assign("packet", call("elephc_gmp_parse", vec![v("number"), i(10)])),
            if_(
                bin(index(v("packet"), i(0)), BinOp::StrictNotEq, s("O")),
                vec![throw_new("Exception", s("Could not unserialize number"))],
                Vec::new(),
                None,
            ),
            assign(
                "rest",
                call(
                    "substr",
                    vec![v("data"), bin(bin(v("start"), BinOp::Add, v("length")), BinOp::Add, i(2))],
                ),
            ),
            assign(
                "properties",
                e_ternary(
                    bin(v("rest"), BinOp::StrictEq, s("")),
                    b(false),
                    call("unserialize", vec![v("rest")]),
                ),
            ),
            if_(
                not(call("is_array", vec![v("properties")])),
                vec![throw_new("Exception", s("Could not unserialize properties"))],
                Vec::new(),
                None,
            ),
            set_prop(
                this(),
                "__elephc_num",
                call("substr", vec![v("packet"), i(1)]),
            ),
            // php-src applies a legacy C: property table internally, without the PHP 8.2
            // dynamic-property deprecation. A temporary handler reproduces that silence
            // while the replay still uses ordinary property storage.
            expr(call(
                "set_error_handler",
                vec![e_str("__elephc_gmp_swallow_error")],
            )),
            foreach_(
                v("properties"),
                Some("name"),
                "value",
                vec![expr(e_assign(e_dyn_prop(this(), v("name")), v("value")))],
            ),
            expr(call("restore_error_handler", Vec::new())),
        ])
}

/// `(int)` cast adapter.
fn cast_int() -> MethodBuilder {
    method("__elephc_to_int")
        .returns(TypeExpr::Int)
        .returning(call("elephc_gmp_intval", vec![prop("__elephc_num")]))
}

/// `(float)` cast adapter.
fn cast_float() -> MethodBuilder {
    method("__elephc_to_float")
        .returns(TypeExpr::Float)
        .returning(call("elephc_gmp_double", vec![prop("__elephc_num")]))
}

/// PHP's `_IS_NUMBER` adapter: int when the value fits, float otherwise.
fn cast_number() -> MethodBuilder {
    method("__elephc_to_number")
        .returns(t_mixed())
        .body(vec![
            if_(
                bin(
                    call("elephc_gmp_fits_slong", vec![prop("__elephc_num")]),
                    BinOp::StrictNotEq,
                    i(0),
                ),
                vec![ret(call("elephc_gmp_intval", vec![prop("__elephc_num")]))],
                Vec::new(),
                None,
            ),
            ret(call("elephc_gmp_double", vec![prop("__elephc_num")])),
        ])
}

/// `(bool)` cast adapter.
fn cast_bool() -> MethodBuilder {
    method("__elephc_to_bool")
        .returns(TypeExpr::Bool)
        .returning(bin(prop("__elephc_num"), BinOp::StrictNotEq, s("0")))
}

/// One binary arithmetic/bitwise adapter.
fn arithmetic(name: &str, operation: i64, zero_message: &str) -> MethodBuilder {
    method(name)
        .static_()
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(t_class("GMP"))
        .returning(call(
            "__elephc_gmp_arithmetic",
            vec![i(operation), v("left"), v("right"), s(zero_message)],
        ))
}

/// `**` with PHP's shift/exponent operand rules.
fn power() -> MethodBuilder {
    method("__elephc_pow")
        .static_()
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(t_class("GMP"))
        .body(vec![
            assign("base", operator_value("left")),
            assign("exponent", shift_value("right", "**")),
            if_(
                bin(v("exponent"), BinOp::Lt, i(0)),
                vec![throw_new(
                    "ValueError",
                    s("Exponent must be greater than or equal to 0"),
                )],
                Vec::new(),
                None,
            ),
            ret(new(
                "GMP",
                vec![call(
                    "__elephc_gmp_value",
                    vec![call("elephc_gmp_unary", vec![i(UN_POW), v("base"), v("exponent")])],
                )],
            )),
        ])
}

/// `<<`/`>>` with PHP's shift operand rules.
fn shift(name: &str, operation: i64, symbol: &str, label: &str) -> MethodBuilder {
    method(name)
        .static_()
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(t_class("GMP"))
        .body(vec![
            assign("base", operator_value("left")),
            assign("shift", shift_value("right", symbol)),
            if_(
                bin(v("shift"), BinOp::Lt, i(0)),
                vec![throw_new(
                    "ValueError",
                    s(&format!("{label} must be greater than or equal to 0")),
                )],
                Vec::new(),
                None,
            ),
            ret(new(
                "GMP",
                vec![call(
                    "__elephc_gmp_value",
                    vec![call("elephc_gmp_unary", vec![i(operation), v("base"), v("shift")])],
                )],
            )),
        ])
}

/// `__elephc_compare` — PHP's standard null/boolean rules, then GMP's numeric ordering.
fn compare() -> MethodBuilder {
    method("__elephc_compare")
        .static_()
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            // PHP's standard object-versus-null ordering. This adapter also feeds the
            // min()/max() reducer, so it must return a real ordering rather than a
            // sentinel: null sorts below every object.
            if_(
                bin(v("left"), BinOp::StrictEq, e_null()),
                vec![
                    ret(e_ternary(
                        bin(v("right"), BinOp::StrictEq, e_null()),
                        i(0),
                        i(-1),
                    )),
                ],
                Vec::new(),
                None,
            ),
            if_(
                bin(v("right"), BinOp::StrictEq, e_null()),
                vec![ret(i(1))],
                Vec::new(),
                None,
            ),
            if_(
                or(
                    call("is_bool", vec![v("left")]),
                    call("is_bool", vec![v("right")]),
                ),
                vec![
                    assign(
                        "left_bool",
                        e_ternary(
                            e_instance_of(v("left"), "GMP"),
                            instance_method(v("left"), "__elephc_bool", Vec::new()),
                            as_bool(v("left")),
                        ),
                    ),
                    assign(
                        "right_bool",
                        e_ternary(
                            e_instance_of(v("right"), "GMP"),
                            instance_method(v("right"), "__elephc_bool", Vec::new()),
                            as_bool(v("right")),
                        ),
                    ),
                    ret(e_ternary(
                        bin(v("left_bool"), BinOp::StrictEq, v("right_bool")),
                        i(0),
                        e_ternary(v("left_bool"), i(1), i(-1)),
                    )),
                ],
                Vec::new(),
                None,
            ),
            assign("first", operator_value("left")),
            assign("second", operator_value("right")),
            ret(as_int(call(
                "__elephc_gmp_value",
                vec![call(
                    "elephc_gmp_binary",
                    vec![i(BIN_CMP), v("first"), v("second")],
                )],
            ))),
        ])
}

/// Instance truthiness adapter.
fn boolean() -> MethodBuilder {
    method("__elephc_bool")
        .returns(TypeExpr::Bool)
        .returning(bin(prop("__elephc_num"), BinOp::StrictNotEq, s("0")))
}

/// `__elephc_gmp_operator_value($value)`.
fn operator_value(name: &str) -> Expr {
    call("__elephc_gmp_operator_value", vec![v(name)])
}

/// `__elephc_gmp_shift_value($value, $symbol)`.
fn shift_value(name: &str, symbol: &str) -> Expr {
    call("__elephc_gmp_shift_value", vec![v(name), s(symbol)])
}
