//! Purpose:
//! Checks original GMP builtin operands before narrowing them to a machine integer.
//!
//! Called from:
//! - GMP prelude injection and the checked gmp_intval AOT/eval homes.
//!
//! Key details:
//! - Existing GMP parsing and PHP strict_types diagnostics remain authoritative.
//! - A weak floating source is range-checked before the ordinary adapter can truncate it.

use super::dsl::*;
use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds one checked projection over the same original operand domain as gmp_intval.
pub(super) fn integer() -> Stmt {
    let range = elephc_semantics::numeric_cast::NumericCastFailure::Range.message();
    function("__elephc_gmp_checked_intval")
        .param("strict", TypeExpr::Bool)
        .param("num", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            if_(and(not(v("strict")), and(call("is_float", vec![v("num")]),
                or(not(call("is_finite", vec![v("num")])),
                    or(bin(v("num"), BinOp::Lt, e_neg(e_float(9223372036854775808.0))),
                        bin(v("num"), BinOp::GtEq, e_float(9223372036854775808.0)))))),
                vec![throw_new("ValueError", s(range))], Vec::new(), None),
            assign("canonical", call("__elephc_gmp_operand",
                vec![v("num"), v("strict"), s("gmp_intval"), i(1), s("num")])),
            if_(bin(call("elephc_gmp_fits_slong", vec![v("canonical")]), BinOp::StrictEq, i(0)),
                vec![throw_new("ValueError", s(range))], Vec::new(), None),
            ret(call("elephc_gmp_intval", vec![v("canonical")])),
        ])
        .build()
}
