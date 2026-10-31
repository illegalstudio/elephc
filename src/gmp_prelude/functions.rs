//! Purpose:
//! Builds the 51 PHP-visible GMP helper functions that the generic AOT builtin
//! lowering calls after boxing their operands.
//!
//! Called from:
//! - `crate::gmp_prelude::declarations`.
//!
//! Key details:
//! - Every helper's first parameter is the compiler-owned `bool $strict`, followed by
//!   `mixed` operands in source order. The checker sees the real `GMP|string|int`
//!   contract; this function owns php-src's coercions and diagnostics.
//! - Common shapes share private builders (`binary_object`, `unary_object`,
//!   `binary_int`, ...) so a family fix cannot drift between members.
//! - No PHP source is parsed.

use super::dsl::*;
use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// A fresh GMP result type.
fn gmp() -> TypeExpr {
    t_class("GMP")
}

/// One argument read through `__elephc_gmp_operand`.
fn operand(param: &str, function: &str, position: i64, name: &str) -> Expr {
    call(
        "__elephc_gmp_operand",
        vec![v(param), v("strict"), s(function), i(position), s(name)],
    )
}

/// One argument read through `__elephc_gmp_init_operand`.
fn init_operand(param: &str, function: &str, position: i64, name: &str, base: &str) -> Expr {
    call(
        "__elephc_gmp_init_operand",
        vec![v(param), v("strict"), s(function), i(position), s(name), v(base)],
    )
}

/// Starts a public helper with its compiler-owned strict flag.
fn helper(name: &str) -> FunctionBuilder {
    let helper = format!("__elephc_gmp_{}", name.trim_start_matches("gmp_"));
    function(&helper).param("strict", TypeExpr::Bool)
}

/// A GMP result built from one bridge string packet.
fn gmp_from(expression: Expr) -> Expr {
    new("GMP", vec![call("__elephc_gmp_value", vec![expression])])
}

/// Builds every public helper declaration.
pub(super) fn declarations() -> Vec<Stmt> {
    let mut functions = Vec::new();
    functions.extend(binary_objects());
    functions.extend(unary_objects());
    functions.extend(int_family());
    functions.extend(bool_family());
    functions.extend(string_family());
    functions.extend(array_family());
    functions.extend(special_family());
    functions.push(unserialize_helper());
    functions
}

/// `__elephc_gmp_unserialize()` keeps native unserialize throws inside an eval invoker boundary.
///
/// Magician reaches this compiled helper when the GMP prelude is present, so malformed GMP
/// payloads surface through the ordinary catchable Throwable path instead of the runtime's
/// top-level longjmp. The helper is compiler-internal and never exposed to PHP.
fn unserialize_helper() -> Stmt {
    function("__elephc_gmp_unserialize")
        .param("data", TypeExpr::Str)
        .returns(t_mixed())
        .returning(call("unserialize", vec![v("data")]))
        .build()
}

/// Binary arithmetic/bitwise/GCD functions over two GMP operands.
fn binary_objects() -> Vec<Stmt> {
    [
        ("gmp_add", BIN_ADD, "Division by zero"),
        ("gmp_sub", BIN_SUB, "Division by zero"),
        ("gmp_mul", BIN_MUL, "Division by zero"),
        ("gmp_and", BIN_AND, "Division by zero"),
        ("gmp_or", BIN_OR, "Division by zero"),
        ("gmp_xor", BIN_XOR, "Division by zero"),
        ("gmp_gcd", BIN_GCD, "Division by zero"),
        ("gmp_lcm", BIN_LCM, "Division by zero"),
    ]
    .into_iter()
    .map(|(name, operation, zero)| {
        helper(name)
            .param("num1", t_mixed())
            .param("num2", t_mixed())
            .returns(gmp())
            .returning(call(
                "__elephc_gmp_arithmetic",
                vec![
                    i(operation),
                    operand("num1", name, 1, "num1"),
                    operand("num2", name, 2, "num2"),
                    s(zero),
                ],
            ))
            .build()
    })
    .collect()
}

/// Unary functions returning GMP, including the non-negative domain checks.
fn unary_objects() -> Vec<Stmt> {
    let mut functions = Vec::new();
    for (name, operation) in [
        ("gmp_neg", UN_NEG),
        ("gmp_abs", UN_ABS),
        ("gmp_com", UN_COM),
        ("gmp_nextprime", UN_NEXTPRIME),
    ] {
        functions.push(
            helper(name)
                .param("num", t_mixed())
                .returns(gmp())
                .returning(gmp_from(call(
                    "elephc_gmp_unary",
                    vec![i(operation), operand("num", name, 1, "num"), i(0)],
                )))
                .build(),
        );
    }
    for (name, operation) in [("gmp_fact", UN_FACT), ("gmp_sqrt", UN_SQRT)] {
        functions.push(
            helper(name)
                .param("num", t_mixed())
                .returns(gmp())
                .body(vec![
                    assign("value", operand("num", name, 1, "num")),
                    expr(call(
                        "__elephc_gmp_require_nonnegative",
                        vec![
                            v("value"),
                            s(name),
                            i(1),
                            s("num"),
                            s("must be greater than or equal to 0"),
                        ],
                    )),
                    ret(gmp_from(call(
                        "elephc_gmp_unary",
                        vec![i(operation), v("value"), i(0)],
                    ))),
                ])
                .build(),
        );
    }
    functions
}

/// Integer-returning comparison, symbol and bit-count functions.
fn int_family() -> Vec<Stmt> {
    let mut functions = Vec::new();
    for (name, operation) in [
        ("gmp_cmp", BIN_CMP),
        ("gmp_jacobi", BIN_JACOBI),
        ("gmp_legendre", BIN_LEGENDRE),
        ("gmp_kronecker", BIN_KRONECKER),
        ("gmp_hamdist", BIN_HAMDIST),
    ] {
        functions.push(
            helper(name)
                .param("num1", t_mixed())
                .param("num2", t_mixed())
                .returns(TypeExpr::Int)
                .returning(as_int(call(
                    "__elephc_gmp_value",
                    vec![call(
                        "elephc_gmp_binary",
                        vec![
                            i(operation),
                            operand("num1", name, 1, "num1"),
                            operand("num2", name, 2, "num2"),
                        ],
                    )],
                )))
                .build(),
        );
    }
    for (name, operation) in [
        ("gmp_sign", UN_SIGN),
        ("gmp_popcount", UN_POPCOUNT),
    ] {
        functions.push(
            helper(name)
                .param("num", t_mixed())
                .returns(TypeExpr::Int)
                .returning(as_int(call(
                    "__elephc_gmp_value",
                    vec![call(
                        "elephc_gmp_unary",
                        vec![i(operation), operand("num", name, 1, "num"), i(0)],
                    )],
                )))
                .build(),
        );
    }
    functions.push(
        helper("gmp_intval")
            .param("num", t_mixed())
            .returns(TypeExpr::Int)
            .returning(call("elephc_gmp_intval", vec![operand("num", "gmp_intval", 1, "num")]))
            .build(),
    );
    functions.push(
        helper("gmp_prob_prime")
            .param("num", t_mixed())
            .param_default("repetitions", t_mixed(), i(10))
            .returns(TypeExpr::Int)
            .body(vec![
                assign("value", operand("num", "gmp_prob_prime", 1, "num")),
                assign(
                    "repetitions",
                    call(
                        "__elephc_gmp_int_parameter",
                        vec![
                            v("repetitions"),
                            v("strict"),
                            s("gmp_prob_prime"),
                            i(2),
                            s("repetitions"),
                        ],
                    ),
                ),
                ret(as_int(call(
                    "__elephc_gmp_value",
                    vec![call(
                        "elephc_gmp_unary",
                        vec![i(UN_PROB_PRIME), v("value"), v("repetitions")],
                    )],
                ))),
            ])
            .build(),
    );
    functions
}

/// Boolean-returning predicates.
fn bool_family() -> Vec<Stmt> {
    let mut functions = Vec::new();
    for (name, operation) in [
        ("gmp_perfect_square", UN_PERFECT_SQUARE),
        ("gmp_perfect_power", UN_PERFECT_POWER),
    ] {
        functions.push(
            helper(name)
                .param("num", t_mixed())
                .returns(TypeExpr::Bool)
                .returning(bin(
                    call(
                        "__elephc_gmp_value",
                        vec![call(
                            "elephc_gmp_unary",
                            vec![i(operation), operand("num", name, 1, "num"), i(0)],
                        )],
                    ),
                    BinOp::StrictEq,
                    s("1"),
                ))
                .build(),
        );
    }
    functions.push(
        helper("gmp_testbit")
            .param("num", t_mixed())
            .param("index", t_mixed())
            .returns(TypeExpr::Bool)
            .body(vec![
                assign("value", operand("num", "gmp_testbit", 1, "num")),
                assign(
                    "index",
                    call(
                        "__elephc_gmp_int_parameter",
                        vec![v("index"), v("strict"), s("gmp_testbit"), i(2), s("index")],
                    ),
                ),
                if_(
                    or(
                        bin(v("index"), BinOp::Lt, i(0)),
                        bin(call("intdiv", vec![v("index"), i(64)]), BinOp::GtEq, i(2147483647)),
                    ),
                    vec![throw_new(
                        "ValueError",
                        s("gmp_testbit(): Argument #2 ($index) must be between 0 and 2147483647 * 64"),
                    )],
                    Vec::new(),
                    None,
                ),
                ret(bin(
                    call(
                        "__elephc_gmp_value",
                        vec![call(
                            "elephc_gmp_unary",
                            vec![i(UN_TESTBIT), v("value"), v("index")],
                        )],
                    ),
                    BinOp::StrictEq,
                    s("1"),
                )),
            ])
            .build(),
    );
    functions
}

/// String-returning conversions and export.
fn string_family() -> Vec<Stmt> {
    let mut functions = Vec::new();
    functions.push(
        helper("gmp_strval")
            .param("num", t_mixed())
            .param_default("base", t_mixed(), i(10))
            .returns(TypeExpr::Str)
            .body(vec![
                assign("value", operand("num", "gmp_strval", 1, "num")),
                assign(
                    "base",
                    call(
                        "__elephc_gmp_int_parameter",
                        vec![v("base"), v("strict"), s("gmp_strval"), i(2), s("base")],
                    ),
                ),
                if_(
                    or(
                        and(bin(v("base"), BinOp::Lt, i(2)), bin(v("base"), BinOp::Gt, i(-2))),
                        or(bin(v("base"), BinOp::Gt, i(62)), bin(v("base"), BinOp::Lt, i(-36))),
                    ),
                    vec![throw_new(
                        "ValueError",
                        s("gmp_strval(): Argument #2 ($base) must be between 2 and 62, or -2 and -36"),
                    )],
                    Vec::new(),
                    None,
                ),
                ret(call(
                    "__elephc_gmp_value",
                    vec![call("elephc_gmp_strval", vec![v("value"), v("base")])],
                )),
            ])
            .build(),
    );
    functions.push(
        helper("gmp_export")
            .param("num", t_mixed())
            .param_default("word_size", t_mixed(), i(1))
            .param_default("flags", t_mixed(), i(17))
            .returns(TypeExpr::Str)
            .body(vec![
                assign("value", operand("num", "gmp_export", 1, "num")),
                assign(
                    "word_size",
                    call(
                        "__elephc_gmp_int_parameter",
                        vec![
                            v("word_size"),
                            v("strict"),
                            s("gmp_export"),
                            i(2),
                            s("word_size"),
                        ],
                    ),
                ),
                assign(
                    "flags",
                    call(
                        "__elephc_gmp_int_parameter",
                        vec![v("flags"), v("strict"), s("gmp_export"), i(3), s("flags")],
                    ),
                ),
                assign(
                    "params",
                    call(
                        "__elephc_gmp_base_parameters",
                        vec![v("word_size"), v("flags"), s("gmp_export")],
                    ),
                ),
                if_(
                    bin(v("word_size"), BinOp::Gt, call("intdiv", vec![e_const("PHP_INT_MAX"), i(4)])),
                    vec![throw_new(
                        "ValueError",
                        s("gmp_export(): Argument #2 ($word_size) is too large for argument #1 ($num)"),
                    )],
                    Vec::new(),
                    None,
                ),
                assign(
                    "packet",
                    call(
                        "elephc_gmp_export",
                        vec![
                            v("value"),
                            v("word_size"),
                            index(v("params"), i(0)),
                            index(v("params"), i(1)),
                        ],
                    ),
                ),
                if_(
                    bin(index(v("packet"), i(0)), BinOp::StrictEq, s("V")),
                    vec![throw_new(
                        "ValueError",
                        s("gmp_export(): Argument #2 ($word_size) is too large for argument #1 ($num)"),
                    )],
                    Vec::new(),
                    None,
                ),
                assign("hex", call("__elephc_gmp_value", vec![v("packet")])),
                ret(e_ternary(
                    bin(v("hex"), BinOp::StrictEq, s("")),
                    s(""),
                    as_str(call("hex2bin", vec![v("hex")])),
                )),
            ])
            .build(),
    );
    functions
}

/// Array-returning division, root and extended-GCD functions.
fn array_family() -> Vec<Stmt> {
    let mut functions = Vec::new();
    for (name, function_name, kind) in [
        ("gmp_div_qr", "gmp_div_qr", "pair"),
        ("gmp_div_q", "gmp_div_q", "quotient"),
        ("gmp_div_r", "gmp_div_r", "remainder"),
        ("gmp_div", "gmp_div", "quotient"),
    ] {
        let mut body = vec![assign(
            "parts",
            call(
                "__elephc_gmp_div_parts",
                vec![
                    v("strict"),
                    v("num1"),
                    v("num2"),
                    v("rounding_mode"),
                    s(function_name),
                ],
            ),
        )];
        match kind {
            "pair" => body.push(ret(array(vec![
                new("GMP", vec![index(v("parts"), i(0))]),
                new("GMP", vec![index(v("parts"), i(1))]),
            ]))),
            "remainder" => body.push(ret(new("GMP", vec![index(v("parts"), i(1))]))),
            _ => body.push(ret(new("GMP", vec![index(v("parts"), i(0))]))),
        }
        let function = helper(name)
            .param("num1", t_mixed())
            .param("num2", t_mixed())
            .param_default("rounding_mode", t_mixed(), i(0));
        let function = if kind == "pair" {
            function.returns(t_array())
        } else {
            function.returns(gmp())
        };
        functions.push(function.body(body).build());
    }
    functions.push(
        helper("gmp_sqrtrem")
            .param("num", t_mixed())
            .returns(t_array())
            .body(vec![
                assign("value", operand("num", "gmp_sqrtrem", 1, "num")),
                expr(call(
                    "__elephc_gmp_require_nonnegative",
                    vec![
                        v("value"),
                        s("gmp_sqrtrem"),
                        i(1),
                        s("num"),
                        s("must be greater than or equal to 0"),
                    ],
                )),
                assign(
                    "parts",
                    call(
                        "explode",
                        vec![
                            s("|"),
                            call(
                                "__elephc_gmp_value",
                                vec![call("elephc_gmp_unary", vec![i(UN_SQRTREM), v("value"), i(0)])],
                            ),
                        ],
                    ),
                ),
                ret(array(vec![
                    new("GMP", vec![index(v("parts"), i(0))]),
                    new("GMP", vec![index(v("parts"), i(1))]),
                ])),
            ])
            .build(),
    );
    functions.push(
        helper("gmp_rootrem")
            .param("num", t_mixed())
            .param("nth", t_mixed())
            .returns(t_array())
            .body(rootrem_body())
            .build(),
    );
    functions.push(
        helper("gmp_gcdext")
            .param("num1", t_mixed())
            .param("num2", t_mixed())
            .returns(t_array())
            .body(vec![
                assign("first", operand("num1", "gmp_gcdext", 1, "num1")),
                assign("second", operand("num2", "gmp_gcdext", 2, "num2")),
                assign(
                    "parts",
                    call(
                        "explode",
                        vec![
                            s("|"),
                            call(
                                "__elephc_gmp_value",
                                vec![call(
                                    "elephc_gmp_binary",
                                    vec![i(BIN_GCDEXT), v("first"), v("second")],
                                )],
                            ),
                        ],
                    ),
                ),
                ret(assoc(vec![
                    (s("g"), new("GMP", vec![index(v("parts"), i(0))])),
                    (s("s"), new("GMP", vec![index(v("parts"), i(1))])),
                    (s("t"), new("GMP", vec![index(v("parts"), i(2))])),
                ])),
            ])
            .build(),
    );
    functions
}

/// Special constructors, mutation, random and power functions.
fn special_family() -> Vec<Stmt> {
    let mut functions = vec![
        init(),
        import(),
        mod_function(),
        divexact(),
        invert(),
        root(),
        pow(),
        powm(),
        random_seed(),
        random_bits(),
        random_range(),
        setbit(),
        clrbit(),
        scan("gmp_scan0", UN_SCAN0),
        scan("gmp_scan1", UN_SCAN1),
        binomial(),
        pow_operator(),
        minmax_array(),
        legacy_property_writer(),
    ];
    functions.extend(intval_helpers());
    functions
}

/// Error handler used while replaying a legacy `C:` payload.
///
/// php-src's internal GMP legacy unserializer applies the stored property table directly,
/// so it predates (and bypasses) PHP 8.2's dynamic-property deprecation. The handler lets
/// `GMP::unserialize()` perform the same writes through ordinary PHP without reporting them.
fn legacy_property_writer() -> Stmt {
    function("__elephc_gmp_swallow_error")
        .param("errno", TypeExpr::Int)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .returning(b(true))
        .build()
}

/// `intval()`/`floatval()` adapters that dispatch on a runtime GMP value.
///
/// A `mixed` subject can hold GMP, in which case PHP's integer/float conversion calls the
/// class cast handler. The string path keeps the base-aware builtin on a statically string
/// subject so it cannot route back into this helper.
fn intval_helpers() -> Vec<Stmt> {
    vec![
        function("__elephc_gmp_intval_cast")
            .param("value", t_mixed())
            .param("base", TypeExpr::Int)
            .returns(TypeExpr::Int)
            .body(vec![
                if_(
                    e_instance_of(v("value"), "GMP"),
                    vec![ret(e_method_call(v("value"), "__elephc_to_int", vec![]))],
                    Vec::new(),
                    None,
                ),
                if_(
                    call("is_string", vec![v("value")]),
                    vec![ret(call(
                        "__elephc_gmp_intval_cast_string",
                        vec![v("value"), v("base")],
                    ))],
                    Vec::new(),
                    None,
                ),
                ret(e_cast(crate::parser::ast::CastType::Int, v("value"))),
            ])
            .build(),
        function("__elephc_gmp_intval_cast_string")
            .param("value", TypeExpr::Str)
            .param("base", TypeExpr::Int)
            .returns(TypeExpr::Int)
            .returning(call("intval", vec![v("value"), v("base")]))
            .build(),
        function("__elephc_gmp_floatval_cast")
            .param("value", t_mixed())
            .returns(TypeExpr::Float)
            .body(vec![
                if_(
                    e_instance_of(v("value"), "GMP"),
                    vec![ret(e_method_call(v("value"), "__elephc_to_float", vec![]))],
                    Vec::new(),
                    None,
                ),
                ret(e_cast(crate::parser::ast::CastType::Float, v("value"))),
            ])
            .build(),
    ]
}

/// Shared `min()`/`max()` reducer for one array of operands.
///
/// The reducer applies PHP's last-operand-wins tie rule and only uses GMP's comparison
/// adapter when either value is a GMP object; every other pair goes through the ordinary
/// PHP comparison so mixed scalar arrays keep their engine semantics.
fn minmax_array() -> Stmt {
    function("__elephc_gmp_minmax_array")
        .param("strict", TypeExpr::Bool)
        .param("max", TypeExpr::Bool)
        .param("last_min_tie", TypeExpr::Bool)
        .param("values", t_array())
        .returns(t_mixed())
        .body(vec![
            if_(
                bin(call("count", vec![v("values")]), BinOp::StrictEq, i(0)),
                vec![throw_new(
                    "ValueError",
                    concat(
                        e_ternary(v("max"), s("max"), s("min")),
                        s("(): Argument #1 ($value) must contain at least one element"),
                    ),
                )],
                Vec::new(),
                None,
            ),
            assign("best", e_null()),
            assign("first", b(true)),
            foreach_(v("values"), None, "value", vec![
                if_(
                    v("first"),
                    vec![assign("best", v("value"))],
                    Vec::new(),
                    Some(vec![
                        assign(
                            "cmp",
                            e_ternary(
                                or(
                                    e_instance_of(v("value"), "GMP"),
                                    e_instance_of(v("best"), "GMP"),
                                ),
                                e_static_call(
                                    "GMP",
                                    "__elephc_compare",
                                    vec![v("value"), v("best")],
                                ),
                                bin(v("value"), BinOp::Spaceship, v("best")),
                            ),
                        ),
                        // php-src replaces strictly, so equal values keep the first
                        // candidate. The two-argument frameless min() is the one exception:
                        // it returns its right operand on equality.
                        if_(
                            e_ternary(
                                v("max"),
                                bin(v("cmp"), BinOp::Gt, i(0)),
                                e_ternary(
                                    v("last_min_tie"),
                                    bin(v("cmp"), BinOp::LtEq, i(0)),
                                    bin(v("cmp"), BinOp::Lt, i(0)),
                                ),
                            ),
                            vec![assign("best", v("value"))],
                            Vec::new(),
                            None,
                        ),
                    ]),
                ),
                assign("first", b(false)),
            ]),
            ret(v("best")),
        ])
        .build()
}

/// Shared `pow()` adapter used by the language builtin for GMP and mixed operands.
fn pow_operator() -> Stmt {
    function("__elephc_gmp_pow_operator")
        .param("strict", TypeExpr::Bool)
        .param("base", t_mixed())
        .param("exponent", t_mixed())
        .returns(t_mixed())
        .body(vec![
            if_(
                or(
                    e_instance_of(v("base"), "GMP"),
                    e_instance_of(v("exponent"), "GMP"),
                ),
                vec![ret(e_static_call(
                    "GMP",
                    "__elephc_pow",
                    vec![v("base"), v("exponent")],
                ))],
                Vec::new(),
                None,
            ),
            ret(bin(v("base"), BinOp::Pow, v("exponent"))),
        ])
        .build()
}

/// `gmp_init(int|string $num, int $base = 0): GMP`.
fn init() -> Stmt {
    helper("gmp_init")
        .param("num", t_mixed())
        .param_default("base", t_mixed(), i(0))
        .returns(gmp())
        .body(vec![
            assign(
                "base",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("base"), v("strict"), s("gmp_init"), i(2), s("base")],
                ),
            ),
            if_(
                and(
                    bin(v("base"), BinOp::StrictNotEq, i(0)),
                    or(bin(v("base"), BinOp::Lt, i(2)), bin(v("base"), BinOp::Gt, i(62))),
                ),
                vec![throw_new(
                    "ValueError",
                    s("gmp_init(): Argument #2 ($base) must be 0 or between 2 and 62"),
                )],
                Vec::new(),
                None,
            ),
            ret(new(
                "GMP",
                vec![init_operand("num", "gmp_init", 1, "num", "base")],
            )),
        ])
        .build()
}

/// `gmp_import(string $data, int $word_size = 1, int $flags = ...): GMP`.
fn import() -> Stmt {
    helper("gmp_import")
        .param("data", t_mixed())
        .param_default("word_size", t_mixed(), i(1))
        .param_default("flags", t_mixed(), i(17))
        .returns(gmp())
        .body(vec![
            assign(
                "data",
                call(
                    "__elephc_gmp_string_parameter",
                    vec![v("data"), s("gmp_import"), i(1), s("data")],
                ),
            ),
            assign(
                "word_size",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("word_size"), v("strict"), s("gmp_import"), i(2), s("word_size")],
                ),
            ),
            assign(
                "flags",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("flags"), v("strict"), s("gmp_import"), i(3), s("flags")],
                ),
            ),
            assign(
                "params",
                call(
                    "__elephc_gmp_base_parameters",
                    vec![v("word_size"), v("flags"), s("gmp_import")],
                ),
            ),
            if_(
                bin(
                    bin(call("strlen", vec![v("data")]), BinOp::Mod, v("word_size")),
                    BinOp::StrictNotEq,
                    i(0),
                ),
                vec![throw_new(
                    "ValueError",
                    s("gmp_import(): Argument #1 ($data) must be a multiple of argument #2 ($word_size)"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_import",
                vec![
                    call("bin2hex", vec![v("data")]),
                    v("word_size"),
                    index(v("params"), i(0)),
                    index(v("params"), i(1)),
                ],
            ))),
        ])
        .build()
}

/// `gmp_mod()` with its function-specific zero diagnostic.
fn mod_function() -> Stmt {
    helper("gmp_mod")
        .param("num1", t_mixed())
        .param("num2", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("first", operand("num1", "gmp_mod", 1, "num1")),
            assign("second", operand("num2", "gmp_mod", 2, "num2")),
            if_(
                bin(v("second"), BinOp::StrictEq, s("0")),
                vec![throw_new(
                    "DivisionByZeroError",
                    s("gmp_mod(): Argument #2 ($num2) Modulo by zero"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_binary",
                vec![i(BIN_MOD), v("first"), v("second")],
            ))),
        ])
        .build()
}

/// `gmp_divexact()` with its function-specific zero diagnostic.
fn divexact() -> Stmt {
    helper("gmp_divexact")
        .param("num1", t_mixed())
        .param("num2", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("first", operand("num1", "gmp_divexact", 1, "num1")),
            assign("second", operand("num2", "gmp_divexact", 2, "num2")),
            if_(
                bin(v("second"), BinOp::StrictEq, s("0")),
                vec![throw_new(
                    "DivisionByZeroError",
                    s("gmp_divexact(): Argument #2 ($num2) Division by zero"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_binary",
                vec![i(BIN_DIVEXACT), v("first"), v("second")],
            ))),
        ])
        .build()
}

/// `gmp_invert(): GMP|false`.
fn invert() -> Stmt {
    helper("gmp_invert")
        .param("num1", t_mixed())
        .param("num2", t_mixed())
        .returns(TypeExpr::Union(vec![gmp(), TypeExpr::False]))
        .body(vec![
            assign("first", operand("num1", "gmp_invert", 1, "num1")),
            assign("second", operand("num2", "gmp_invert", 2, "num2")),
            if_(
                bin(v("second"), BinOp::StrictEq, s("0")),
                vec![throw_new("DivisionByZeroError", s("Division by zero"))],
                Vec::new(),
                None,
            ),
            assign(
                "packet",
                call(
                    "elephc_gmp_binary",
                    vec![i(BIN_INVERT), v("first"), v("second")],
                ),
            ),
            if_(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("F")),
                vec![ret(b(false))],
                Vec::new(),
                None,
            ),
            ret(gmp_from(v("packet"))),
        ])
        .build()
}

/// `gmp_root()`.
fn root() -> Stmt {
    helper("gmp_root")
        .param("num", t_mixed())
        .param("nth", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("value", operand("num", "gmp_root", 1, "num")),
            assign(
                "nth",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("nth"), v("strict"), s("gmp_root"), i(2), s("nth")],
                ),
            ),
            if_(
                bin(v("nth"), BinOp::LtEq, i(0)),
                vec![throw_new(
                    "ValueError",
                    s("gmp_root(): Argument #2 ($nth) must be greater than 0"),
                )],
                Vec::new(),
                None,
            ),
            if_(
                and(
                    bin(bin(v("nth"), BinOp::Mod, i(2)), BinOp::StrictEq, i(0)),
                    bin(index(v("value"), i(0)), BinOp::StrictEq, s("-")),
                ),
                vec![throw_new(
                    "ValueError",
                    s("gmp_root(): Argument #2 ($nth) must be odd if argument #1 ($a) is negative"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_unary",
                vec![i(UN_ROOT), v("value"), v("nth")],
            ))),
        ])
        .build()
}

/// `gmp_pow()`.
fn pow() -> Stmt {
    helper("gmp_pow")
        .param("num", t_mixed())
        .param("exponent", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("value", operand("num", "gmp_pow", 1, "num")),
            assign(
                "exponent",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("exponent"), v("strict"), s("gmp_pow"), i(2), s("exponent")],
                ),
            ),
            if_(
                bin(v("exponent"), BinOp::Lt, i(0)),
                vec![throw_new(
                    "ValueError",
                    s("gmp_pow(): Argument #2 ($exponent) must be greater than or equal to 0"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_unary",
                vec![i(UN_POW), v("value"), v("exponent")],
            ))),
        ])
        .build()
}

/// `gmp_powm()`.
fn powm() -> Stmt {
    helper("gmp_powm")
        .param("num", t_mixed())
        .param("exponent", t_mixed())
        .param("modulus", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("base", operand("num", "gmp_powm", 1, "num")),
            assign("power", operand("exponent", "gmp_powm", 2, "exponent")),
            assign("mod", operand("modulus", "gmp_powm", 3, "modulus")),
            if_(
                bin(index(v("power"), i(0)), BinOp::StrictEq, s("-")),
                vec![throw_new(
                    "ValueError",
                    s("gmp_powm(): Argument #2 ($exponent) must be greater than or equal to 0"),
                )],
                Vec::new(),
                None,
            ),
            assign(
                "packet",
                call(
                    "elephc_gmp_ternary",
                    vec![i(0), v("base"), v("power"), v("mod")],
                ),
            ),
            if_(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("Z")),
                vec![throw_new("DivisionByZeroError", s("Modulo by zero"))],
                Vec::new(),
                None,
            ),
            ret(gmp_from(v("packet"))),
        ])
        .build()
}

/// `gmp_random_seed()`.
fn random_seed() -> Stmt {
    helper("gmp_random_seed")
        .param("seed", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            assign("value", operand("seed", "gmp_random_seed", 1, "seed")),
            expr(call("elephc_gmp_random_seed", vec![v("value")])),
        ])
        .build()
}

/// `gmp_random_bits()`.
fn random_bits() -> Stmt {
    helper("gmp_random_bits")
        .param("bits", t_mixed())
        .returns(gmp())
        .body(vec![
            assign(
                "bits",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("bits"), v("strict"), s("gmp_random_bits"), i(1), s("bits")],
                ),
            ),
            if_(
                or(
                    bin(v("bits"), BinOp::LtEq, i(0)),
                    bin(v("bits"), BinOp::Gt, i(2147483647)),
                ),
                vec![throw_new(
                    "ValueError",
                    s("gmp_random_bits(): Argument #1 ($bits) must be between 1 and 2147483647"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call("elephc_gmp_random_bits", vec![v("bits")]))),
        ])
        .build()
}

/// `gmp_random_range()`.
fn random_range() -> Stmt {
    helper("gmp_random_range")
        .param("min", t_mixed())
        .param("max", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("minimum", operand("min", "gmp_random_range", 1, "min")),
            assign("maximum", operand("max", "gmp_random_range", 2, "max")),
            if_(
                bin(
                    as_int(call(
                        "__elephc_gmp_value",
                        vec![call(
                            "elephc_gmp_binary",
                            vec![i(BIN_CMP), v("minimum"), v("maximum")],
                        )],
                    )),
                    BinOp::GtEq,
                    i(0),
                ),
                vec![throw_new(
                    "ValueError",
                    s("gmp_random_range(): Argument #1 ($min) must be less than argument #2 ($maximum)"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_random_range",
                vec![v("minimum"), v("maximum")],
            ))),
        ])
        .build()
}

/// `gmp_setbit()`.
fn setbit() -> Stmt {
    helper("gmp_setbit")
        .param("num", t_mixed())
        .param("index", t_mixed())
        .param_default("value", t_mixed(), b(true))
        .returns(TypeExpr::Void)
        .body(vec![
            if_(
                not(e_instance_of(v("num"), "GMP")),
                vec![throw_new(
                    "TypeError",
                    concat(
                        s("gmp_setbit(): Argument #1 ($num) must be of type GMP, "),
                        concat(call("__elephc_gmp_type_name", vec![v("num")]), s(" given")),
                    ),
                )],
                Vec::new(),
                None,
            ),
            assign(
                "index",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("index"), v("strict"), s("gmp_setbit"), i(2), s("index")],
                ),
            ),
            expr(call(
                "__elephc_gmp_index_parameter",
                vec![v("index"), s("gmp_setbit")],
            )),
            assign(
                "value",
                call(
                    "__elephc_gmp_bool_parameter",
                    vec![v("value"), v("strict"), s("gmp_setbit"), i(3), s("value")],
                ),
            ),
            set_prop(
                v("num"),
                "__elephc_num",
                call(
                    "__elephc_gmp_value",
                    vec![call(
                        "elephc_gmp_bit",
                        vec![e_prop(v("num"), "__elephc_num"), v("index"), v("value")],
                    )],
                ),
            ),
        ])
        .build()
}

/// `gmp_clrbit()`.
fn clrbit() -> Stmt {
    helper("gmp_clrbit")
        .param("num", t_mixed())
        .param("index", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            if_(
                not(e_instance_of(v("num"), "GMP")),
                vec![throw_new(
                    "TypeError",
                    concat(
                        s("gmp_clrbit(): Argument #1 ($num) must be of type GMP, "),
                        concat(call("__elephc_gmp_type_name", vec![v("num")]), s(" given")),
                    ),
                )],
                Vec::new(),
                None,
            ),
            assign(
                "index",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("index"), v("strict"), s("gmp_clrbit"), i(2), s("index")],
                ),
            ),
            expr(call(
                "__elephc_gmp_index_parameter",
                vec![v("index"), s("gmp_clrbit")],
            )),
            set_prop(
                v("num"),
                "__elephc_num",
                call(
                    "__elephc_gmp_value",
                    vec![call("elephc_gmp_bit", vec![e_prop(v("num"), "__elephc_num"), v("index"), b(false)])],
                ),
            ),
        ])
        .build()
}

/// `gmp_scan0()` / `gmp_scan1()`.
fn scan(name: &str, operation: i64) -> Stmt {
    helper(name)
        .param("num", t_mixed())
        .param("start", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            assign("value", operand("num", name, 1, "num1")),
            assign(
                "start",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("start"), v("strict"), s(name), i(2), s("start")],
                ),
            ),
            if_(
                or(
                    bin(v("start"), BinOp::Lt, i(0)),
                    bin(call("intdiv", vec![v("start"), i(64)]), BinOp::GtEq, i(2147483647)),
                ),
                vec![throw_new(
                    "ValueError",
                    concat(
                        s(name),
                        s("(): Argument #2 ($start) must be between 0 and 2147483647 * 64"),
                    ),
                )],
                Vec::new(),
                None,
            ),
            ret(as_int(call(
                "__elephc_gmp_value",
                vec![call(
                    "elephc_gmp_unary",
                    vec![i(operation), v("value"), v("start")],
                )],
            ))),
        ])
        .build()
}

/// `gmp_binomial()`.
fn binomial() -> Stmt {
    helper("gmp_binomial")
        .param("num", t_mixed())
        .param("k", t_mixed())
        .returns(gmp())
        .body(vec![
            assign("value", operand("num", "gmp_binomial", 1, "n")),
            assign(
                "k",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![v("k"), v("strict"), s("gmp_binomial"), i(2), s("k")],
                ),
            ),
            if_(
                bin(v("k"), BinOp::Lt, i(0)),
                vec![throw_new(
                    "ValueError",
                    s("gmp_binomial(): Argument #2 ($k) must be greater than or equal to 0"),
                )],
                Vec::new(),
                None,
            ),
            ret(gmp_from(call(
                "elephc_gmp_unary",
                vec![i(UN_BINOMIAL), v("value"), v("k")],
            ))),
        ])
        .build()
}

/// `gmp_rootrem()` body.
fn rootrem_body() -> Vec<Stmt> {
    vec![
        assign("value", operand("num", "gmp_rootrem", 1, "num")),
        assign(
            "nth",
            call(
                "__elephc_gmp_int_parameter",
                vec![v("nth"), v("strict"), s("gmp_rootrem"), i(2), s("nth")],
            ),
        ),
        if_(
            bin(v("nth"), BinOp::LtEq, i(0)),
            vec![throw_new(
                "ValueError",
                s("gmp_rootrem(): Argument #2 ($nth) must be greater than or equal to 1"),
            )],
            Vec::new(),
            None,
        ),
        if_(
            and(
                bin(bin(v("nth"), BinOp::Mod, i(2)), BinOp::StrictEq, i(0)),
                bin(index(v("value"), i(0)), BinOp::StrictEq, s("-")),
            ),
            vec![throw_new(
                "ValueError",
                s("gmp_rootrem(): Argument #2 ($nth) must be odd if argument #1 ($a) is negative"),
            )],
            Vec::new(),
            None,
        ),
        assign(
            "parts",
            call(
                "explode",
                vec![
                    s("|"),
                    call(
                        "__elephc_gmp_value",
                        vec![call(
                            "elephc_gmp_unary",
                            vec![i(UN_ROOTREM), v("value"), v("nth")],
                        )],
                    ),
                ],
            ),
        ),
        ret(array(vec![
            new("GMP", vec![index(v("parts"), i(0))]),
            new("GMP", vec![index(v("parts"), i(1))]),
        ])),
    ]
}
