//! Purpose:
//! Refines an integer compared with PHP semantics against a checked `+`/`-` whose overflow can
//! go only one way, so a loop counter bounded by `$n - 1` keeps the headroom its update needs.
//!
//! Called from:
//! - `super::refine_comparison_edge`, for every `php_rel_cmp` branch condition.
//!
//! Key details:
//! - `php_rel_cmp` converts an int to double against a float, and a checked `X - c` with
//!   `c >= 1` can only overflow to a float at or below -2^63, which no converted int is below.
//! - Only `L <= PHP_INT_MAX - 1` (or the mirrored `L >= PHP_INT_MIN + 1`) is claimed. An
//!   outcome the bound cannot decide leaves the edge state untouched and never prunes the edge.

use crate::ir::{CmpPredicate, Function, Immediate, Instruction, IrType, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::{range_for_value, set_range, swap_predicate, IntRange, RangeState};

/// The only direction in which a checked bound can leave the integer range.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overflow {
    /// The bound is an integer at most `PHP_INT_MAX - 1`, or a float at or below -2^63.
    Downward,
    /// The bound is an integer at least `PHP_INT_MIN + 1`, or a float at or above 2^63.
    Upward,
}

/// Narrows the integer operand of `php_rel_cmp(L, R)` on the edge selected by `taken`.
///
/// With a downward bound `R`, an outcome that puts `L` below `R` (`<` or `<=` taken, `>` or `>=`
/// not taken) needs either an integer `R <= PHP_INT_MAX - 1` that `L` does not exceed, or the
/// overflowed float, which only `(double)L == -2^63` can reach. Both leave
/// `L <= PHP_INT_MAX - 1`. The upward mirror leaves `L >= PHP_INT_MIN + 1`. Every other
/// combination, and any bound this module cannot classify, keeps the state as it is.
pub(super) fn refine_php_comparison_edge(
    function: &Function,
    compare: &Instruction,
    taken: bool,
    state: &mut RangeState,
) {
    let Some(Immediate::CmpPredicate(predicate)) = compare.immediate else {
        return;
    };
    let &[first, second] = compare.operands.as_slice() else {
        return;
    };
    // Normalized to `integer predicate bound`.
    let (integer, predicate, bound) = if php_int(function, first) && !php_int(function, second) {
        (first, predicate, second)
    } else if php_int(function, second) && !php_int(function, first) {
        let Some(swapped) = swap_predicate(predicate) else {
            return;
        };
        (second, swapped, first)
    } else {
        return;
    };
    let below = match predicate {
        CmpPredicate::Slt | CmpPredicate::Sle => taken,
        CmpPredicate::Sgt | CmpPredicate::Sge => !taken,
        _ => return,
    };
    let limit = match (one_sided_overflow(function, state, bound), below) {
        (Some(Overflow::Downward), true) => IntRange {
            lo: i64::MIN,
            hi: i64::MAX - 1,
        },
        (Some(Overflow::Upward), false) => IntRange {
            lo: i64::MIN + 1,
            hi: i64::MAX,
        },
        _ => return,
    };
    let Some(current) = range_for_value(function, state, integer) else {
        return;
    };
    // An empty intersection would mean the edge is unreachable. That is true, but proving
    // reachability is not this refinement's job, so the edge keeps its state instead.
    if let Some(next) = current.intersect(limit) {
        set_range(function, state, integer, Some(next));
    }
}

/// Classifies a checked `+`/`-` of two raw integers by the one direction it can overflow in.
///
/// `X - Y` with `Y >= 1`, and `X + Y` with either operand `<= -1`, can only fall below the
/// integer range, so as an integer they are at most `PHP_INT_MAX - 1`. `X + Y` with either
/// operand `>= 1`, and `X - Y` with `Y <= -1`, can only climb above it, so as an integer they
/// are at least `PHP_INT_MIN + 1`. Multiplication and operands of unknown sign return `None`.
fn one_sided_overflow(function: &Function, state: &RangeState, bound: ValueId) -> Option<Overflow> {
    let ValueDef::Instruction { inst, .. } = function.value(bound)?.def else {
        return None;
    };
    let producer = function.instruction(inst)?;
    let &[lhs, rhs] = producer.operands.as_slice() else {
        return None;
    };
    if !php_int(function, lhs) || !php_int(function, rhs) {
        return None;
    }
    let sign = |value: ValueId| {
        let range = range_for_value(function, state, value)?;
        if range.lo >= 1 {
            Some(1)
        } else if range.hi <= -1 {
            Some(-1)
        } else {
            None
        }
    };
    match producer.op {
        Op::ICheckedSub => match sign(rhs)? {
            1 => Some(Overflow::Downward),
            _ => Some(Overflow::Upward),
        },
        Op::ICheckedAdd => match sign(rhs).or_else(|| sign(lhs))? {
            1 => Some(Overflow::Upward),
            _ => Some(Overflow::Downward),
        },
        _ => None,
    }
}

/// Returns whether a value is a raw PHP integer, the only operand this refinement narrows.
fn php_int(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::I64 && value.php_type == PhpType::Int)
}
