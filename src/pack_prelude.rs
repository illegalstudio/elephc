//! Purpose:
//! Injects PHP's `pack()` implemented with synthetic AST builders when a program references it.
//!
//! Called from:
//! - `crate::pipeline::compile()` before name resolution.
//!
//! Key details:
//! - A user declaration of `pack` always wins; the prelude is never injected then.
//! - The function is built from typed AST nodes, never parsed from PHP source, so every
//!   operand follows the ordinary checker, EIR lowering, and runtime without a backend path.
//! - Hex, string-padding, integer, float, pad/back-up, and repeat formats are implemented.
//!   Floats expose the interpreter's native IEEE-754 representation through `ptr()` so the
//!   emitted bytes match PHP on every supported target.

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// The checked-in implementation is deliberately limited to byte-exact formats.
fn concat(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Concat, right)
}

/// Returns `$values[$index]` with PHP's missing-element default.
fn value_at(values: &str, index: &str, default: Expr) -> Expr {
    e_null_coalesce(e_index(e_var(values), e_var(index)), default)
}

/// Advances `$index` by one.
fn next_value() -> Stmt {
    s_expr(e_post_inc("index"))
}

/// Returns `(int) $values[$index]` without consuming the value.
fn int_value() -> Expr {
    e_cast(CastType::Int, value_at("values", "index", e_int(0)))
}

/// Builds `$out .= ...` for one packed fragment.
fn append(value: Expr) -> Stmt {
    s_assign("out", concat(e_var("out"), value))
}

/// Emits PHP's missing-argument ValueError before a format consumes its value.
fn require_value(suffix: &str) -> Stmt {
    s_if(
        e_not(e_call("array_key_exists", vec![e_var("index"), e_var("values")])),
        vec![s_throw(e_new_fq("ValueError", vec![concat(
            e_str("Type "),
            concat(e_var("code"), e_str(suffix)),
        )]))],
        vec![],
        None,
    )
}

/// Emits a `pack()` warning through the shared diagnostic sink.
fn pack_warning(message: &str) -> Stmt {
    s_expr(e_call("__elephc_builtin_warning", vec![e_str(message)]))
}

/// Builds one `H`/`h` case.
fn hex_case(high_nibble: bool) -> Vec<Stmt> {
    let first = e_ternary(
        e_binop(e_var("j"), BinOp::Lt, e_call("strlen", vec![e_var("text")])),
        e_call("strtolower", vec![e_index(e_var("text"), e_var("j"))]),
        e_str("0"),
    );
    let second = e_ternary(
        e_binop(e_binop(e_var("j"), BinOp::Add, e_int(1)), BinOp::Lt,
            e_call("strlen", vec![e_var("text")])),
        e_call("strtolower", vec![e_index(e_var("text"), e_binop(e_var("j"), BinOp::Add, e_int(1)))]),
        e_str("0"),
    );
    let pair = if high_nibble {
        concat(first, second)
    } else {
        concat(second, first)
    };
    vec![
        require_value(": not enough arguments"),
        s_assign("text", e_cast(CastType::String, value_at("values", "index", e_str("")))),
        next_value(),
        s_assign("count", e_ternary(e_var("star"), e_call("strlen", vec![e_var("text")]), e_var("repeat"))),
        s_assign("hex", e_str("")),
        s_assign("j", e_int(0)),
        s_while(e_binop(e_var("j"), BinOp::Lt, e_var("count")), vec![
            s_assign("hex", concat(e_var("hex"), pair)),
            s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(2))),
        ]),
        append(e_cast(CastType::String, e_call("hex2bin", vec![e_var("hex")]))),
        s_break(1),
    ]
}

/// Builds one string-padding `a`/`A`/`Z` case.
fn string_case(pad: &str) -> Vec<Stmt> {
    let padded = concat(
        e_var("text"),
        e_call("str_repeat", vec![
            e_str(pad),
            e_binop(e_var("count"), BinOp::Sub, e_call("strlen", vec![e_var("text")])),
        ]),
    );
    vec![
        require_value(": not enough arguments"),
        s_assign("text", e_cast(CastType::String, value_at("values", "index", e_str("")))),
        next_value(),
        s_assign("count", e_ternary(e_var("star"), e_call("strlen", vec![e_var("text")]), e_var("repeat"))),
        s_if(
            e_binop(e_call("strlen", vec![e_var("text")]), BinOp::GtEq, e_var("count")),
            vec![append(e_call("substr", vec![e_var("text"), e_int(0), e_var("count")]))],
            vec![],
            Some(vec![append(padded)]),
        ),
        s_break(1),
    ]
}

/// Builds PHP's `Z` case: truncate to `count - 1` bytes and always NUL-terminate.
fn z_case() -> Vec<Stmt> {
    vec![
        require_value(": not enough arguments"),
        s_assign("text", e_cast(CastType::String, value_at("values", "index", e_str("")))),
        next_value(),
        s_assign("count", e_ternary(e_var("star"), e_binop(e_call("strlen", vec![e_var("text")]), BinOp::Add, e_int(1)), e_var("repeat"))),
        s_if(e_binop(e_var("count"), BinOp::LtEq, e_int(0)),
            vec![s_break(1)], vec![], None),
        s_assign("text", e_call("substr", vec![e_var("text"), e_int(0), e_binop(e_var("count"), BinOp::Sub, e_int(1))])),
        append(concat(e_var("text"), e_call("str_repeat", vec![e_str("\0"),
            e_binop(e_var("count"), BinOp::Sub, e_call("strlen", vec![e_var("text")]))]))),
        s_break(1),
    ]
}

/// Builds one repeated integer case with an explicit byte width and byte order.
fn integer_case(width: usize, little_endian: bool) -> Vec<Stmt> {
    let mut append_bytes = Vec::new();
    for position in 0..width {
        let shift = if little_endian { position } else { width - 1 - position } ;
        append_bytes.push(append(e_call("chr", vec![e_binop(
            e_binop(e_var("bits"), BinOp::ShiftRight, e_int((shift * 8) as i64)),
            BinOp::BitAnd,
            e_int(0xff),
        )])));
    }
    let mask = if width >= 8 { e_int(-1) } else { e_int((1i64 << (width * 8)) - 1) };
    vec![
        s_assign("count", e_ternary(e_var("star"), e_binop(e_call("count", vec![e_var("values")]), BinOp::Sub, e_var("index")), e_var("repeat"))),
        s_assign("j", e_int(0)),
        s_while(e_binop(e_var("j"), BinOp::Lt, e_var("count")), {
            let mut body = vec![
                require_value(": too few arguments"),
                s_assign("bits", e_binop(int_value(), BinOp::BitAnd, mask)),
                next_value(),
            ];
            body.extend(append_bytes);
            body.push(s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(1))));
            body
        }),
        s_break(1),
    ]
}

/// Builds one IEEE-754 float case (`f`/`d`/`e`/`g`/`G`).
///
/// The value is stored in a float local; taking its address exposes the exact native
/// representation, and `ptr_write_f32` rounds to single precision for the 4-byte forms.
fn float_case(width: usize, little_endian: bool) -> Vec<Stmt> {
    let mut append_bytes = Vec::new();
    for position in 0..width {
        // `ptr_read8` reads increasing addresses; big-endian output reverses them.
        let offset = if little_endian { position } else { width - 1 - position };
        append_bytes.push(append(e_call("chr", vec![e_call("ptr_read8", vec![
            e_call("ptr_offset", vec![e_var("p"), e_int(offset as i64)]),
        ])])));
    }
    let mut body = vec![
        require_value(": too few arguments"),
        s_assign("value", e_cast(CastType::Float, value_at("values", "index", e_float(0.0)))),
        next_value(),
        s_assign("p", e_call("ptr", vec![e_var("value")])),
    ];
    if width == 4 {
        body.push(s_expr(e_call("ptr_write_f32", vec![e_var("p"), e_var("value")])));
    }
    body.extend(append_bytes);
    body.push(s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(1))));
    vec![
        s_assign("count", e_ternary(e_var("star"), e_binop(e_call("count", vec![e_var("values")]), BinOp::Sub, e_var("index")), e_var("repeat"))),
        s_assign("j", e_int(0)),
        s_while(e_binop(e_var("j"), BinOp::Lt, e_var("count")), body),
        s_break(1),
    ]
}

/// Builds one `c`/`C` single-byte case.
fn byte_case() -> Vec<Stmt> {
    vec![
        s_assign("count", e_ternary(e_var("star"), e_binop(e_call("count", vec![e_var("values")]), BinOp::Sub, e_var("index")), e_var("repeat"))),
        s_assign("j", e_int(0)),
        s_while(e_binop(e_var("j"), BinOp::Lt, e_var("count")), vec![
            require_value(": too few arguments"),
            append(e_call("chr", vec![e_binop(int_value(), BinOp::BitAnd, e_int(0xff))])),
            next_value(),
            s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(1))),
        ]),
        s_break(1),
    ]
}

/// Builds one 64-bit big-endian case (`q`/`Q`) via an explicit little-endian buffer.
fn quad_big_endian_case() -> Vec<Stmt> {
    vec![
        s_assign("count", e_ternary(e_var("star"), e_binop(e_call("count", vec![e_var("values")]), BinOp::Sub, e_var("index")), e_var("repeat"))),
        s_assign("j", e_int(0)),
        s_while(e_binop(e_var("j"), BinOp::Lt, e_var("count")), vec![
            require_value(": too few arguments"),
            s_assign("bits", int_value()),
            next_value(),
            s_assign("quad", e_str("")),
            s_assign("k", e_int(0)),
            s_while(e_binop(e_var("k"), BinOp::Lt, e_int(8)), vec![
                s_assign("quad", concat(e_var("quad"), e_call("chr", vec![e_binop(
                    e_binop(e_var("bits"), BinOp::ShiftRight, e_binop(e_var("k"), BinOp::Mul, e_int(8))),
                    BinOp::BitAnd,
                    e_int(0xff),
                )]))),
                s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
            ]),
            append(e_call("strrev", vec![e_var("quad")])),
            s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(1))),
        ]),
        s_break(1),
    ]
}

/// Builds the `pack()` function declaration.
fn pack_function() -> Stmt {
    let digit_condition = e_binop(
        e_binop(e_var("i"), BinOp::Lt, e_var("length")),
        BinOp::And,
        e_binop(
            e_binop(e_index(e_var("format"), e_var("i")), BinOp::GtEq, e_str("0")),
            BinOp::And,
            e_binop(e_index(e_var("format"), e_var("i")), BinOp::LtEq, e_str("9")),
        ),
    );
    let body = vec![
        s_assign("out", e_str("")),
        s_assign("index", e_int(0)),
        s_assign("length", e_call("strlen", vec![e_var("format")])),
        s_assign("i", e_int(0)),
        s_while(e_binop(e_var("i"), BinOp::Lt, e_var("length")), vec![
            s_assign("code", e_index(e_var("format"), e_var("i"))),
            s_expr(e_post_inc("i")),
            s_assign("repeat", e_int(0)),
            s_assign("star", e_bool(false)),
            s_assign("has_count", e_bool(false)),
            s_while(digit_condition, vec![
                s_assign("has_count", e_bool(true)),
                s_assign("repeat", e_binop(e_binop(e_var("repeat"), BinOp::Mul, e_int(10)),
                    BinOp::Add, e_binop(e_call("ord", vec![e_index(e_var("format"), e_var("i"))]), BinOp::Sub, e_int(48)))),
                s_expr(e_post_inc("i")),
            ]),
            s_if(e_binop(
                e_binop(e_var("i"), BinOp::Lt, e_var("length")),
                BinOp::And,
                e_binop(e_index(e_var("format"), e_var("i")), BinOp::StrictEq, e_str("*")),
            ), vec![s_assign("star", e_bool(true)), s_expr(e_post_inc("i"))], vec![], None),
            // An omitted count means one repetition; an explicit `0` means none.
            s_if(e_binop(e_not(e_var("star")), BinOp::And, e_not(e_var("has_count"))),
                vec![s_assign("repeat", e_int(1))], vec![], None),
            s_switch(e_var("code"), vec![
                (vec![e_str("H")], hex_case(true)),
                (vec![e_str("h")], hex_case(false)),
                (vec![e_str("a")], string_case("\0")),
                (vec![e_str("A")], string_case(" ")),
                (vec![e_str("Z")], z_case()),
                // `f`/`d` are machine order (little-endian on every supported target);
                // `e`/`g`/`G` are the explicit little/big-endian double/float forms.
                (vec![e_str("f"), e_str("g")], float_case(4, true)),
                (vec![e_str("G")], float_case(4, false)),
                (vec![e_str("d"), e_str("e")], float_case(8, true)),
                (vec![e_str("c"), e_str("C")], byte_case()),
                // PHP's `s`/`S`/`l`/`L`/`q`/`Q` use machine byte order. Every supported
                // target is little-endian; `n`/`N`/`J` are the explicit big-endian forms.
                (vec![e_str("s"), e_str("S")], integer_case(2, true)),
                (vec![e_str("n")], integer_case(2, false)),
                (vec![e_str("v")], integer_case(2, true)),
                (vec![e_str("l"), e_str("L"), e_str("i"), e_str("I")], integer_case(4, true)),
                (vec![e_str("N")], integer_case(4, false)),
                (vec![e_str("V")], integer_case(4, true)),
                (vec![e_str("q"), e_str("Q")], integer_case(8, true)),
                (vec![e_str("J")], quad_big_endian_case()),
                (vec![e_str("P")], integer_case(8, true)),
                (vec![e_str("X")], vec![
                    s_if(e_var("star"), vec![
                        pack_warning("pack(): Type X: '*' ignored"),
                        s_assign("repeat", e_int(1)),
                    ], vec![], None),
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("out")]), BinOp::Lt, e_var("repeat")),
                        vec![
                            pack_warning("pack(): Type X: outside of string"),
                            s_assign("out", e_str("")),
                        ],
                        vec![],
                        Some(vec![s_assign("out", e_call("substr", vec![
                            e_var("out"),
                            e_int(0),
                            e_binop(e_call("strlen", vec![e_var("out")]), BinOp::Sub, e_var("repeat")),
                        ]))]),
                    ),
                    s_break(1),
                ]),
                (vec![e_str("x")], vec![
                    s_if(e_var("star"), vec![pack_warning("pack(): Type x: '*' ignored")], vec![], None),
                    append(e_call("str_repeat", vec![e_str("\0"), e_ternary(e_var("star"), e_int(1), e_var("repeat"))])),
                    s_break(1),
                ]),
                (vec![e_str("@")], vec![
                    s_if(e_var("star"), vec![pack_warning("pack(): Type @: '*' ignored"), s_assign("repeat", e_int(1))], vec![], None),
                    s_if(e_binop(e_call("strlen", vec![e_var("out")]), BinOp::Lt, e_var("repeat")),
                        vec![append(e_call("str_repeat", vec![e_str("\0"), e_binop(e_var("repeat"),
                            BinOp::Sub, e_call("strlen", vec![e_var("out")]))]))], vec![], None),
                    s_break(1),
                ]),
            ], Some(vec![
                s_throw(e_new_fq("ValueError", vec![concat(e_str("Type "),
                    concat(e_var("code"), e_str(": unknown format code")))])),
            ])),
        ]),
        s_return(e_var("out")),
    ];
    function("pack")
        .param("format", TypeExpr::Str)
        .variadic("values", Some(t_mixed()))
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// Reports whether the program calls `pack()` without declaring its own implementation.
pub fn program_uses_pack(program: &[Stmt]) -> bool {
    !crate::opcache_prelude::detect::program_declares(program, "pack")
        && crate::opcache_prelude::detect::program_references(program, "pack")
}

/// Prepends the `pack()` implementation when the program references it.
pub fn inject_if_used(program: Program, used: bool) -> Program {
    if !used {
        return program;
    }
    let mut declarations = vec![pack_function()];
    declarations.extend(program);
    declarations
}
