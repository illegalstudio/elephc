//! Purpose:
//! Turns `L < (X - c)`, with an integer `L` and a small literal `c`, into an integer comparison
//! against the checked difference, so a loop bound such as `$end - 1` is never boxed.
//!
//! Called from:
//! - `super::CheckedIntSink::run`, after the integer-sink phases.
//!
//! Key details:
//! - PHP computes an overflowing `X - c` as `(double)X - (double)c`, and so does the cold path
//!   of `ichecked_sub_to_int`. For `1 <= c <= 1024` that double is exactly -2^63: `(double)X`
//!   is within 512 of -2^63, and the difference rounds to -2^63 rather than to either
//!   neighbour (-2^63 + 1024 or -2^63 - 2048). PHP's float-to-int conversion of -2^63 is
//!   PHP_INT_MIN, so the cast-mode producer yields PHP_INT_MIN exactly where PHP has the float.
//! - `L < PHP_INT_MIN` is false for every integer, and so is PHP's `(double)L < -2^63`. Only the
//!   strict `<` (or `>` with the bound on the left) is rewritten: `<=` would disagree for an `L`
//!   within 512 of PHP_INT_MIN, where `(double)L` is also -2^63.

use std::collections::HashMap;

use crate::ir::{
    CmpPredicate, Function, Immediate, InstId, Instruction, IrType, Op, Ownership, ValueDef,
    ValueId,
};
use crate::types::PhpType;

use super::super::rewrite::{defining_instruction, neutralize_to_nop};
use super::{instruction_uses, terminator_used_values};

/// Largest literal subtrahend whose overflowing difference still rounds to exactly -2^63.
const MAX_EXACT_SUBTRAHEND: i64 = 1024;

/// The comparisons and releases of one bound, collected before anything is rewritten.
struct Plan {
    comparisons: Vec<InstId>,
    releases: Vec<InstId>,
}

/// Returns whether a PHP comparison in the function reads a bound this rewrite could unbox.
pub(super) fn has_candidate(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        inst.op == Op::PhpRelCmp
            && inst.operands.iter().any(|&operand| {
                defining_instruction(function, operand)
                    .is_some_and(|definition| candidate(function, definition))
            })
    })
}

/// Rewrites every bound whose observers are all strict comparisons; returns whether any changed.
pub(super) fn sink(function: &mut Function) -> bool {
    let uses = instruction_uses(function);
    let terminator_uses = terminator_used_values(function);
    let mut rewrites = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !candidate(function, inst) {
            continue;
        }
        let Some(bound) = inst.result else {
            continue;
        };
        if terminator_uses.contains(&bound) {
            continue;
        }
        if let Some(plan) = plan(function, bound, &uses) {
            rewrites.push((InstId::from_raw(raw as u32), bound, plan));
        }
    }
    let changed = !rewrites.is_empty();
    for (producer, bound, plan) in rewrites {
        apply(function, producer, bound, plan);
    }
    changed
}

/// Accepts a boxed `ichecked_sub` of a raw integer and a literal in `1..=1024`.
fn candidate(function: &Function, inst: &Instruction) -> bool {
    if inst.op != Op::ICheckedSub {
        return false;
    }
    let &[minuend, subtrahend] = inst.operands.as_slice() else {
        return false;
    };
    php_int(function, minuend)
        && literal(function, subtrahend)
            .is_some_and(|value| (1..=MAX_EXACT_SUBTRAHEND).contains(&value))
}

/// Requires every observer of `bound` to be a strict comparison or a release of the box.
fn plan(function: &Function, bound: ValueId, uses: &HashMap<ValueId, Vec<InstId>>) -> Option<Plan> {
    let mut plan = Plan {
        comparisons: Vec::new(),
        releases: Vec::new(),
    };
    for &id in uses.get(&bound)? {
        let user = function.instruction(id)?;
        match user.op {
            Op::Release => plan.releases.push(id),
            Op::PhpRelCmp if strict_comparison(function, user, bound) => plan.comparisons.push(id),
            _ => return None,
        }
    }
    (!plan.comparisons.is_empty()).then_some(plan)
}

/// Accepts `L < bound` or `bound > L` with a raw PHP integer `L`.
fn strict_comparison(function: &Function, compare: &Instruction, bound: ValueId) -> bool {
    let &[lhs, rhs] = compare.operands.as_slice() else {
        return false;
    };
    match compare.immediate {
        Some(Immediate::CmpPredicate(CmpPredicate::Slt)) => {
            rhs == bound && lhs != bound && php_int(function, lhs)
        }
        Some(Immediate::CmpPredicate(CmpPredicate::Sgt)) => {
            lhs == bound && rhs != bound && php_int(function, rhs)
        }
        _ => false,
    }
}

/// Makes the bound an unboxed cast-mode difference and its comparisons integer comparisons.
fn apply(function: &mut Function, producer: InstId, bound: ValueId, plan: Plan) {
    if let Some(inst) = function.instruction_mut(producer) {
        inst.op = Op::ICheckedSubToInt;
        // Cast mode: an overflow yields PHP's `(int)` of the promoted double, PHP_INT_MIN here.
        inst.immediate = Some(Immediate::Bool(true));
        inst.result_type = IrType::I64;
        inst.result_php_type = PhpType::Int;
        inst.result_ownership = Ownership::NonHeap;
        inst.effects = Op::ICheckedSubToInt.default_effects();
    }
    if let Some(value) = function.value_mut(bound) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    for id in plan.comparisons {
        if let Some(inst) = function.instruction_mut(id) {
            inst.op = Op::ICmp;
            inst.effects = Op::ICmp.default_effects();
        }
    }
    for id in plan.releases {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
}

/// Returns the value of an `i64` literal operand.
fn literal(function: &Function, value: ValueId) -> Option<i64> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let definition = function.instruction(inst)?;
    match (definition.op, &definition.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Returns whether a value is a raw PHP integer.
fn php_int(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::I64 && value.php_type == PhpType::Int)
}
