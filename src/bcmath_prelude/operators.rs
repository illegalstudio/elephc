//! Purpose:
//! Builds Number's internal arithmetic, comparison, and boolean adapters.
//!
//! Called from:
//! - The Number synthetic class declaration builder.
//!
//! Key details:
//! - Operators accept PHP's numeric scalar coercions independently of method signatures.
//! - Public method errors are translated to the corresponding operator diagnostics.

use super::*;
use crate::parser::ast::Expr;

/// Builds the runtime predicate for the scalar/object domain accepted by Number arithmetic.
fn accepted(value: Expr) -> Expr {
    let tests = ["is_int", "is_string", "is_bool"].into_iter()
        .map(|name| global_call(name, vec![value.clone()]));
    let float = e_binop(global_call("is_float", vec![value.clone()]), BinOp::And,
        global_call("__elephc_bcnumber_float_fits", vec![value.clone()]));
    tests.chain([float, e_instance_of(value.clone(), NUMBER_CLASS)])
        .reduce(|left, right| e_binop(left, BinOp::Or, right)).expect("Number operand domain")
}

/// Converts an operator operand through the shared numeric-scalar adapter.
fn operand(name: &str) -> Expr {
    global_call("__elephc_bcnumber_operand", vec![e_var(name)])
}

/// Builds the unsupported-operands TypeError using the actual PHP-visible operand types.
fn unsupported(symbol: &str) -> Stmt {
    let message = e_binop(e_binop(e_binop(e_str("Unsupported operand types: "), BinOp::Concat,
        global_call("__elephc_bcnumber_type", vec![e_var("_bcleft")])), BinOp::Concat, e_str(&format!(" {symbol} "))),
        BinOp::Concat, global_call("__elephc_bcnumber_type", vec![e_var("_bcright")]));
    s_throw(e_new_fq("TypeError", vec![message]))
}

/// Adds the methods named by the shared internal-class operator contract.
pub(super) fn add_adapters(mut number: ClassBuilder) -> ClassBuilder {
    for (name, symbol) in [("add", "+"), ("sub", "-"), ("mul", "*"), ("div", "/"), ("mod", "%"), ("pow", "**")] {
        let calculate = e_new_fq(NUMBER_CLASS, vec![operation(name, vec![operand("_bcleft"), operand("_bcright"), e_null()])]);
        let mut caught = vec![s_assign("_bcmessage", e_method_call(e_var("_bcerror"), "getMessage", vec![]))];
        for (needle, message) in [
            ("exponent cannot have a fractional part", "exponent cannot have a fractional part"),
            ("exponent is too large", "exponent is too large"),
        ] {
            caught.push(s_if(global_call("str_contains", vec![e_var("_bcmessage"), e_str(needle)]),
                vec![s_throw(e_new_fq("ValueError", vec![e_str(message)]))], vec![], None));
        }
        caught.push(s_if(global_call("str_contains", vec![e_var("_bcmessage"), e_str("is not well-formed")]), vec![
            s_if(global_call("is_string", vec![e_var("_bcleft")]),
                vec![s_throw(e_new_fq("ValueError", vec![e_str("Left string operand cannot be converted to BcMath\\Number")]))], vec![],
                Some(vec![s_throw(e_new_fq("ValueError", vec![e_str("Right string operand cannot be converted to BcMath\\Number")]))])),
        ], vec![], None));
        caught.push(s_throw(e_var("_bcerror")));
        number = number.method(method(&format!("__elephc_{name}")).static_()
            .param("_bcleft", t_mixed()).param("_bcright", t_mixed()).returns(t_class(NUMBER_CLASS))
            .body(vec![
                s_if(e_not(e_binop(accepted(e_var("_bcleft")), BinOp::And, accepted(e_var("_bcright")))),
                    vec![unsupported(symbol)], vec![], None),
                s_try(vec![s_return(calculate)], vec![
                    (vec!["\\ValueError"], Some("_bcerror"), caught),
                    (vec!["\\TypeError"], None, vec![unsupported(symbol)]),
                ], None),
            ]));
    }
    let mut compare = vec![];
    for (boolean, other, reverse) in [("_bcleft", "_bcright", false), ("_bcright", "_bcleft", true)] {
        let boolean = e_cast(CastType::Int, e_var(boolean));
        let other = e_cast(CastType::Int, e_binop(operation("comp", vec![operand(other), e_str("0"), e_null()]), BinOp::NotEq, e_int(0)));
        let result = if reverse { e_binop(other, BinOp::Sub, boolean) } else { e_binop(boolean, BinOp::Sub, other) };
        compare.push(s_if(global_call("is_bool", vec![e_var(if reverse { "_bcright" } else { "_bcleft" })]),
            vec![s_return(e_cast(CastType::Int, result))], vec![], None));
    }
    for (value, result) in [("_bcleft", -1), ("_bcright", 1)] {
        let result = if result < 0 { e_neg(e_int(-result)) } else { e_int(result) };
        compare.push(s_if(global_call("is_null", vec![e_var(value)]), vec![s_return(result)], vec![], None));
    }
    let unordered = elephc_builtin_contract::ObjectOrdering::Unordered as i64;
    compare.push(s_if(e_not(e_binop(accepted(e_var("_bcleft")), BinOp::And, accepted(e_var("_bcright")))),
        vec![s_return(e_int(unordered))], vec![], None));
    compare.push(s_return(operation("comp", vec![operand("_bcleft"), operand("_bcright"), e_null()])));
    number.method(method("__elephc_compare").static_()
        .param("_bcleft", t_mixed()).param("_bcright", t_mixed()).returns(TypeExpr::Int)
        .body(vec![s_try(compare, vec![(vec!["\\ValueError", "\\TypeError"], None, vec![s_return(e_int(unordered))])], None)]))
        .method(method("__elephc_bool").returns(TypeExpr::Bool)
            .returning(e_binop(operation("comp", vec![e_this_prop("value"), e_str("0"), e_null()]), BinOp::NotEq, e_int(0))))
}

/// Declares scalar coercion through an ordinary coercive int parameter, including PHP diagnostics.
pub(super) fn declarations() -> Program {
    vec![
        function("__elephc_bcnumber_float_fits").param("_bcfloat", TypeExpr::Float)
            .returns(TypeExpr::Bool).returning(e_binop(
                e_binop(e_var("_bcfloat"), BinOp::GtEq, e_neg(e_float(9223372036854775808.0))), BinOp::And,
                e_binop(e_var("_bcfloat"), BinOp::Lt, e_float(9223372036854775808.0)),
            )).build(),
        function("__elephc_bcnumber_parameter").param("_bcparameter", t_mixed()).returns(TypeExpr::Str)
            .body(vec![
                s_if(e_binop(global_call("is_bool", vec![e_var("_bcparameter")]), BinOp::Or,
                    global_call("is_null", vec![e_var("_bcparameter")])),
                    vec![s_return(decimal(e_cast(CastType::Int, e_var("_bcparameter"))))], vec![], None),
                s_if(e_binop(global_call("is_float", vec![e_var("_bcparameter")]), BinOp::And,
                    global_call("__elephc_bcnumber_float_fits", vec![e_var("_bcparameter")])),
                    vec![s_return(decimal(e_cast(CastType::Int, e_var("_bcparameter"))))], vec![], None),
                s_return(decimal(e_var("_bcparameter"))),
            ]).build(),
        function("__elephc_bcnumber_type").param("_bctype", t_mixed()).returns(TypeExpr::Str)
            .body(vec![
                s_if(global_call("is_object", vec![e_var("_bctype")]),
                    vec![s_return(global_call("get_class", vec![e_var("_bctype")]))], vec![], None),
                s_return(e_match(global_call("gettype", vec![e_var("_bctype")]), vec![
                    (vec![e_str("integer")], e_str("int")),
                    (vec![e_str("double")], e_str("float")),
                    (vec![e_str("boolean")], e_str("bool")),
                    (vec![e_str("NULL")], e_str("null")),
                ], Some(global_call("gettype", vec![e_var("_bctype")])))),
            ]).build(),
        function("__elephc_bcnumber_integer").param("_bcinteger", TypeExpr::Int)
            .returns(TypeExpr::Str).returning(decimal(e_var("_bcinteger"))).build(),
        function("__elephc_bcnumber_operand").param("_bcoperand", t_mixed()).returns(TypeExpr::Str)
            .body(vec![
                s_if(e_binop(global_call("is_float", vec![e_var("_bcoperand")]), BinOp::Or,
                    global_call("is_bool", vec![e_var("_bcoperand")])),
                    vec![s_return(global_call("__elephc_bcnumber_integer", vec![e_var("_bcoperand")]))], vec![], None),
                s_return(decimal(e_var("_bcoperand"))),
            ]).build(),
    ]
}
