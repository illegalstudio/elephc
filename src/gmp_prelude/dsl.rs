//! Purpose:
//! Small expression/statement vocabulary shared by the synthetic GMP prelude builders.
//!
//! Called from:
//! - `crate::gmp_prelude::{class, validators, functions}`.
//!
//! Key details:
//! - Every helper returns ordinary parser AST nodes; nothing in this module parses PHP text.
//! - The names are short because the prelude is a direct transcription of php-src's GMP
//!   adapter, and long constructors would bury the control flow being reviewed.

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind};
use crate::synthetic_class::*;

/// GMP bridge binary operation codes, mirrored from `crates/elephc-gmp/src/ops.rs`.
pub(super) const BIN_ADD: i64 = 0;
pub(super) const BIN_SUB: i64 = 1;
pub(super) const BIN_MUL: i64 = 2;
pub(super) const BIN_TDIV_Q: i64 = 3;
pub(super) const BIN_MOD: i64 = 4;
pub(super) const BIN_DIVEXACT: i64 = 5;
pub(super) const BIN_AND: i64 = 6;
pub(super) const BIN_OR: i64 = 7;
pub(super) const BIN_XOR: i64 = 8;
pub(super) const BIN_GCD: i64 = 9;
pub(super) const BIN_LCM: i64 = 10;
pub(super) const BIN_CMP: i64 = 11;
pub(super) const BIN_JACOBI: i64 = 12;
pub(super) const BIN_LEGENDRE: i64 = 13;
pub(super) const BIN_KRONECKER: i64 = 14;
pub(super) const BIN_HAMDIST: i64 = 15;
pub(super) const BIN_INVERT: i64 = 16;
pub(super) const BIN_DIV_QR_TDIV: i64 = 17;
pub(super) const BIN_DIV_QR_CDIV: i64 = 18;
pub(super) const BIN_DIV_QR_FDIV: i64 = 19;
pub(super) const BIN_GCDEXT: i64 = 20;

/// GMP bridge unary operation codes.
pub(super) const UN_NEG: i64 = 0;
pub(super) const UN_ABS: i64 = 1;
pub(super) const UN_COM: i64 = 2;
pub(super) const UN_NEXTPRIME: i64 = 3;
pub(super) const UN_SQRT: i64 = 4;
pub(super) const UN_FACT: i64 = 5;
pub(super) const UN_SIGN: i64 = 6;
pub(super) const UN_POPCOUNT: i64 = 7;
pub(super) const UN_PERFECT_SQUARE: i64 = 8;
pub(super) const UN_PERFECT_POWER: i64 = 9;
pub(super) const UN_PROB_PRIME: i64 = 10;
pub(super) const UN_ROOT: i64 = 11;
pub(super) const UN_ROOTREM: i64 = 12;
pub(super) const UN_SQRTREM: i64 = 13;
pub(super) const UN_POW: i64 = 14;
pub(super) const UN_SHL: i64 = 15;
pub(super) const UN_SHR: i64 = 16;
pub(super) const UN_TESTBIT: i64 = 17;
pub(super) const UN_SCAN0: i64 = 20;
pub(super) const UN_SCAN1: i64 = 21;
pub(super) const UN_BINOMIAL: i64 = 22;

/// A variable read.
pub(super) fn v(name: &str) -> Expr {
    e_var(name)
}

/// A string literal.
pub(super) fn s(value: &str) -> Expr {
    e_str(value)
}

/// An integer literal.
pub(super) fn i(value: i64) -> Expr {
    e_int(value)
}

/// A boolean literal.
pub(super) fn b(value: bool) -> Expr {
    e_bool(value)
}

/// `$this`.
pub(super) fn this() -> Expr {
    e_this()
}

/// `$this->name`.
pub(super) fn prop(name: &str) -> Expr {
    e_this_prop(name)
}

/// A global function call.
pub(super) fn call(name: &str, args: Vec<Expr>) -> Expr {
    e_call(name, args)
}

/// An instance method call.
pub(super) fn instance_method(object: Expr, name: &str, args: Vec<Expr>) -> Expr {
    e_method_call(object, name, args)
}

/// `new Class(args...)`.
pub(super) fn new(class: &str, args: Vec<Expr>) -> Expr {
    e_new_fq(class, args)
}

/// `$array[$index]`.
pub(super) fn index(array: Expr, index: Expr) -> Expr {
    e_index(array, index)
}

/// `(int) $value`.
pub(super) fn as_int(value: Expr) -> Expr {
    e_cast(CastType::Int, value)
}

/// `(string) $value`.
pub(super) fn as_str(value: Expr) -> Expr {
    e_cast(CastType::String, value)
}

/// `(bool) $value`.
pub(super) fn as_bool(value: Expr) -> Expr {
    e_cast(CastType::Bool, value)
}

/// `$left op $right`.
pub(super) fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    e_binop(left, op, right)
}

/// Logical AND.
pub(super) fn and(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::And, right)
}

/// Logical OR.
pub(super) fn or(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::Or, right)
}

/// Logical negation.
pub(super) fn not(value: Expr) -> Expr {
    e_not(value)
}

/// String concatenation.
pub(super) fn concat(left: Expr, right: Expr) -> Expr {
    e_concat(left, right)
}

/// A PHP indexed array literal.
pub(super) fn array(items: Vec<Expr>) -> Expr {
    e_array(items)
}

/// A PHP associative array literal.
pub(super) fn assoc(entries: Vec<(Expr, Expr)>) -> Expr {
    e_array_assoc(entries)
}

/// `$name = value;`
pub(super) fn assign(name: &str, value: Expr) -> Stmt {
    s_assign(name, value)
}

/// `$object->property = value;`
pub(super) fn set_prop(object: Expr, property: &str, value: Expr) -> Stmt {
    s_prop_assign(object, property, value)
}

/// `return value;`
pub(super) fn ret(value: Expr) -> Stmt {
    s_return(value)
}

/// `expression;`
pub(super) fn expr(value: Expr) -> Stmt {
    s_expr(value)
}

/// `$array[] = value;`
pub(super) fn push(array: &str, value: Expr) -> Stmt {
    s_array_push(array, value)
}

/// `if (...) { ... } elseif (...) { ... } else { ... }`
pub(super) fn if_(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Stmt {
    s_if(condition, then_body, elseif_clauses, else_body)
}

/// `for (...; ...; ...) { ... }`
pub(super) fn for_(
    init: Option<Stmt>,
    condition: Option<Expr>,
    update: Option<Stmt>,
    body: Vec<Stmt>,
) -> Stmt {
    s_for(init, condition, update, body)
}

/// `foreach (... as $key => $value) { ... }`
pub(super) fn foreach_(
    array: Expr,
    key: Option<&str>,
    value: &str,
    body: Vec<Stmt>,
) -> Stmt {
    s_foreach(array, key, value, body)
}

/// `break;`
pub(super) fn break_() -> Stmt {
    s_break(1)
}

/// `throw new Class(message);` with a source-bearing synthetic span.
///
/// php-src's GMP handlers raise these while executing the user's `unserialize()` call, and
/// PHP reports that call site on the uncaught fatal. A dummy span (line 0) made the engine
/// omit the location entirely, so the throw carries a synthetic span instead: it names the
/// script and a generated line, which is the closest attribution available without a call
/// stack in EIR.
pub(super) fn throw_new(class: &str, message: Expr) -> Stmt {
    let span = crate::span::Span::synthetic();
    let object = Expr::new(
        ExprKind::NewObject {
            class_name: class_name(class),
            args: vec![message],
        },
        span,
    );
    Stmt::new(StmtKind::Throw(object), span)
}
