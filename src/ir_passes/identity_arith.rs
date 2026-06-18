//! Purpose:
//! Identity arithmetic folding pass over EIR. Rewrites algebraic identities
//! such as `x + 0`, `x * 1`, `x ^ x`, and `x * 0` to their trivial results.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Two dominance-safe, validator-clean rewrites are used:
//!   - Fold-to-operand: the result equals an existing operand `x`. The folded
//!     instruction is neutralized to `Nop` and its result uses are redirected to
//!     `x` via RAUW. `x` was an operand, so it already dominates every use.
//!   - Fold-to-zero: the result is the integer `0`. The instruction is converted
//!     in place to `ConstI64 0` (same result value id, so no RAUW is needed).
//! - Only PHP-equivalent identities are folded. Integer `x / 0` / `x % 0` are
//!   left to trap, and float additive-zero / `* 0.0` are excluded because of
//!   signed-zero and NaN observability.
//! - Fold-to-operand chains within one sweep are resolved transitively so a
//!   neutralized (dead) result is never used as a replacement target.

use std::collections::HashMap;

use crate::ir::{Function, Immediate, InstId, Instruction, IrType, Op, ValueId};

use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop, replace_all_uses, resolve_chains};

/// Identity arithmetic folding pass. See the module docs for the rewrite rules.
pub struct IdentityArith;

impl IrPass for IdentityArith {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "identity-arith"
    }

    /// Folds algebraic identities in one function, returning true on any change.
    /// The literal pool is unused: every fold reuses an existing operand or the
    /// in-place `const_i64 0` rewrite, so no new literal is interned.
    fn run(&self, function: &mut Function, _data: &mut crate::ir::DataPool) -> bool {
        // Phase 1: detect folds without mutating (immutable borrow of operands).
        let mut to_operand: Vec<(InstId, ValueId, ValueId)> = Vec::new();
        let mut to_zero: Vec<InstId> = Vec::new();
        for (index, inst) in function.instructions.iter().enumerate() {
            let inst_id = InstId::from_raw(index as u32);
            let Some(result) = inst.result else {
                continue;
            };
            match classify(function, inst) {
                Some(Fold::ToOperand(replacement)) => {
                    to_operand.push((inst_id, result, replacement));
                }
                Some(Fold::ToZeroI64) => to_zero.push(inst_id),
                None => {}
            }
        }
        if to_operand.is_empty() && to_zero.is_empty() {
            return false;
        }

        // Phase 2: resolve fold-to-operand chains so map targets are terminal.
        let raw: HashMap<ValueId, ValueId> = to_operand
            .iter()
            .map(|&(_, result, replacement)| (result, replacement))
            .collect();
        let resolved = resolve_chains(&raw);

        // Phase 3: neutralize folded instructions and materialize zero constants.
        for &(inst_id, _, _) in &to_operand {
            if let Some(inst) = function.instruction_mut(inst_id) {
                neutralize_to_nop(inst);
            }
        }
        for &inst_id in &to_zero {
            if let Some(inst) = function.instruction_mut(inst_id) {
                convert_to_const_zero(inst);
            }
        }

        // Phase 4: redirect every remaining use of a folded result to its target.
        replace_all_uses(function, &resolved);
        true
    }
}

/// The kind of rewrite an identity match implies.
enum Fold {
    /// The instruction's result equals this already-defined operand value.
    ToOperand(ValueId),
    /// The instruction's result is the integer constant zero.
    ToZeroI64,
}

/// Classifies a binary arithmetic/bitwise instruction as a foldable identity, or
/// `None` if no PHP-equivalent identity applies. Only two-operand ops are folded.
fn classify(function: &Function, inst: &Instruction) -> Option<Fold> {
    if inst.operands.len() != 2 {
        return None;
    }
    let lhs = inst.operands[0];
    let rhs = inst.operands[1];
    let lhs_is_zero = is_const_i64(function, lhs, 0);
    let rhs_is_zero = is_const_i64(function, rhs, 0);
    let lhs_is_one = is_const_i64(function, lhs, 1);
    let rhs_is_one = is_const_i64(function, rhs, 1);
    let same_operand = lhs == rhs;

    match inst.op {
        // x + 0 = 0 + x = x
        Op::IAdd => {
            if lhs_is_zero {
                Some(Fold::ToOperand(rhs))
            } else if rhs_is_zero {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x - x = 0; x - 0 = x (0 - x is -x, not foldable here)
        Op::ISub => {
            if same_operand {
                Some(Fold::ToZeroI64)
            } else if rhs_is_zero {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x * 0 = 0 * x = 0; x * 1 = 1 * x = x
        Op::IMul => {
            if lhs_is_zero || rhs_is_zero {
                Some(Fold::ToZeroI64)
            } else if lhs_is_one {
                Some(Fold::ToOperand(rhs))
            } else if rhs_is_one {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x / 1 = x (x / 0 must trap and is excluded)
        Op::IDiv | Op::ISDiv => {
            if rhs_is_one {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x % 1 = 0 (x % 0 must trap and is excluded)
        Op::ISMod => {
            if rhs_is_one {
                Some(Fold::ToZeroI64)
            } else {
                None
            }
        }
        // x & 0 = 0 & x = 0; x & x = x
        Op::IBitAnd => {
            if lhs_is_zero || rhs_is_zero {
                Some(Fold::ToZeroI64)
            } else if same_operand {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x | 0 = 0 | x = x; x | x = x
        Op::IBitOr => {
            if lhs_is_zero {
                Some(Fold::ToOperand(rhs))
            } else if rhs_is_zero || same_operand {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x ^ x = 0; x ^ 0 = 0 ^ x = x
        Op::IBitXor => {
            if same_operand {
                Some(Fold::ToZeroI64)
            } else if lhs_is_zero {
                Some(Fold::ToOperand(rhs))
            } else if rhs_is_zero {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x << 0 = x; x >> 0 = x
        Op::IShl | Op::IShrA => {
            if rhs_is_zero {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x * 1.0 = 1.0 * x = x (exact identity for NaN/-0.0/INF)
        Op::FMul => {
            if is_const_f64(function, lhs, 1.0) {
                Some(Fold::ToOperand(rhs))
            } else if is_const_f64(function, rhs, 1.0) {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        // x / 1.0 = x (exact identity for NaN/-0.0/INF)
        Op::FDiv => {
            if is_const_f64(function, rhs, 1.0) {
                Some(Fold::ToOperand(lhs))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Returns true when `value` is an `i64` constant equal to `expected`.
fn is_const_i64(function: &Function, value: ValueId, expected: i64) -> bool {
    matches!(
        defining_instruction(function, value),
        Some(inst) if inst.op == Op::ConstI64 && inst.immediate == Some(Immediate::I64(expected))
    )
}

/// Returns true when `value` is an `f64` constant exactly equal to `expected`.
fn is_const_f64(function: &Function, value: ValueId, expected: f64) -> bool {
    matches!(
        defining_instruction(function, value),
        Some(inst)
            if inst.op == Op::ConstF64
                && matches!(inst.immediate, Some(Immediate::F64(n)) if n == expected)
    )
}

/// Converts a folded instruction in place into `ConstI64 0`, keeping its result
/// value id and type. Only used for instructions whose result type is `I64`.
fn convert_to_const_zero(inst: &mut Instruction) {
    debug_assert_eq!(
        inst.result_type,
        IrType::I64,
        "fold-to-zero only applies to I64 results"
    );
    inst.op = Op::ConstI64;
    inst.operands.clear();
    inst.immediate = Some(Immediate::I64(0));
    inst.effects = Op::ConstI64.default_effects();
}
