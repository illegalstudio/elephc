//! Purpose:
//! Per-block constant propagation over EIR: folds pure operations whose operands
//! are all compile-time constants into a single `Const*` instruction, in place.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Constants in SSA are program-wide: a value defined by a `Const*` op is that
//!   constant at every use, so a single forward scan over the instruction table
//!   (definitions precede uses) is enough to discover and fold constant operands.
//!   The scan accumulates folded results into the same constant map, so chained
//!   folds collapse within one sweep.
//! - Propagation *through local slots* is realized by composition: the peephole
//!   scalar load/store value-numbering (`peephole/load_store.rs`) forwards a
//!   `load_local` of a slot that holds a constant to that constant value id, and
//!   this pass then folds the resulting constant-operand operation. Together,
//!   under the fixed-point driver, they constitute per-block constant propagation
//!   over EIR value ids and local slots.
//! - Each fold replaces the instruction in place with the matching `Const*` op,
//!   keeping the same result `ValueId` and result type (no RAUW needed — every
//!   later use already reads that value). This mirrors `identity_arith`'s
//!   convert-to-constant rewrite and stays validator-clean (`Const*` ops take no
//!   operands and carry a matching immediate).
//! - Boxed checked arithmetic may narrow to a scalar constant only when the shared
//!   `boxed_narrowing` consumer policy preserves every conversion and storage contract.
//! - Only folds that exactly reproduce the runtime lowering are performed, so the
//!   compiled result is unchanged: integer ops use 64-bit wrapping (matching the
//!   native `add`/`sub`/`mul`/`neg` lowering), shifts fold only for in-range
//!   counts, and the trapping/division and `NaN`-sensitive float-division paths
//!   are left untouched (consistent with `identity_arith`).

use std::collections::HashMap;

use crate::ir::{
    CmpPredicate, DataPool, Function, Immediate, InstId, Instruction, IrType, Op, Ownership,
    ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;

/// Per-block constant folding pass. See the module docs for the rewrite rules.
pub struct ConstFold;

/// A compile-time constant value carried by an EIR `Const*` instruction.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Const {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

/// The type narrowing applied when a fold changes the result type. `None`
/// means the folded constant has the same type as the original instruction
/// (the common case for `IAdd` → `ConstI64`). `ToInt`/`ToFloat` are used when
/// a checked op (`ICheckedAdd`, result type `Heap(Mixed)`) folds to a scalar
/// constant, narrowing the result from `Mixed` to `Int` or `Float`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeNarrowing {
    None,
    ToInt,
    ToFloat,
}

impl Const {
    /// Interprets the constant as the 64-bit integer the runtime would load for an
    /// integer-typed operand: bools widen to `0`/`1` and null coerces to `0`,
    /// matching the integer-operand load path. Floats are never reinterpreted.
    fn as_i64(self) -> Option<i64> {
        match self {
            Const::Int(n) => Some(n),
            Const::Bool(b) => Some(i64::from(b)),
            Const::Null => Some(0),
            Const::Float(_) => None,
        }
    }

    /// Interprets the constant as a 64-bit float. Only genuine float constants
    /// qualify; integers would require an explicit `i_to_f` conversion at runtime.
    fn as_f64(self) -> Option<f64> {
        match self {
            Const::Float(f) => Some(f),
            _ => None,
        }
    }

    /// Returns PHP truthiness for this constant, matching the `is_truthy` lowering:
    /// nonzero integers, `true`, and nonzero floats are truthy; null is falsy.
    fn truthiness(self) -> bool {
        match self {
            Const::Int(n) => n != 0,
            Const::Bool(b) => b,
            Const::Float(f) => f != 0.0,
            Const::Null => false,
        }
    }
}

impl IrPass for ConstFold {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "const-fold"
    }

    /// Folds constant-operand operations in one function, returning true on change.
    /// The literal pool is unused: every fold materializes a scalar `Const*` in
    /// place, so no new data-pool literal is interned.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        // Phase 1 (read-only): scan instructions in definition order, tracking the
        // constant carried by each value and recording instructions to fold. The
        // accumulating map lets chained folds collapse in a single sweep.
        let mut consts: HashMap<ValueId, Const> = HashMap::new();
        let mut folds: Vec<(InstId, Const, TypeNarrowing)> = Vec::new();
        for (index, inst) in function.instructions.iter().enumerate() {
            let Some(result) = inst.result else {
                continue;
            };
            if let Some(value) = const_of_const_op(inst) {
                consts.insert(result, value);
                continue;
            }
            if let Some((folded, narrowing)) = try_fold(function, inst, &consts) {
                consts.insert(result, folded);
                folds.push((InstId::from_raw(index as u32), folded, narrowing));
            }
        }
        if folds.is_empty() {
            return false;
        }

        // A known numeric payload does not make its consumers representation-polymorphic.
        // Share the range pass's safety boundary so a later fold cannot undo its refusal.
        if folds.iter().any(|(_, _, narrowing)| *narrowing != TypeNarrowing::None) {
            let int_blocked = super::boxed_narrowing::blocked_results(function, IrType::I64);
            let float_blocked = super::boxed_narrowing::blocked_results(function, IrType::F64);
            folds.retain(|(inst_id, value, narrowing)| {
                if *narrowing == TypeNarrowing::ToInt {
                    let Const::Int(payload) = value else { return false; };
                    if !super::boxed_narrowing::integer_range_can_narrow(*payload, *payload) {
                        return false;
                    }
                }
                let blocked = match narrowing {
                    TypeNarrowing::None => return true,
                    TypeNarrowing::ToInt => &int_blocked,
                    TypeNarrowing::ToFloat => &float_blocked,
                };
                function.instruction(*inst_id).and_then(|inst| inst.result)
                    .is_some_and(|result| !blocked.contains(&result))
            });
            if folds.is_empty() {
                return false;
            }
        }

        // Phase 2 (mutate): rewrite each folded instruction in place to a constant.
        // When the fold narrows the result type (e.g. ICheckedAdd → ConstI64),
        // also update the instruction's result_type/result_php_type/ownership and
        // the corresponding Value metadata so the validator and codegen see the
        // narrowed type.
        for (inst_id, value, narrowing) in folds {
            if let Some(inst) = function.instruction_mut(inst_id) {
                convert_to_const(inst, value);
                if narrowing != TypeNarrowing::None {
                    apply_type_narrowing(inst, narrowing);
                }
            }
            if narrowing != TypeNarrowing::None {
                if let Some(result) = function.instruction(inst_id).and_then(|i| i.result) {
                    if let Some(val) = function.value_mut(result) {
                        apply_value_type_narrowing(val, narrowing);
                    }
                }
            }
        }
        true
    }
}

/// Returns the constant carried by a `Const*` instruction, or `None` otherwise.
fn const_of_const_op(inst: &Instruction) -> Option<Const> {
    match (inst.op, inst.immediate.as_ref()) {
        (Op::ConstI64, Some(Immediate::I64(n))) => Some(Const::Int(*n)),
        (Op::ConstF64, Some(Immediate::F64(f))) => Some(Const::Float(*f)),
        (Op::ConstBool, Some(Immediate::Bool(b))) => Some(Const::Bool(*b)),
        (Op::ConstNull, _) => Some(Const::Null),
        _ => None,
    }
}

/// Attempts to fold an instruction whose operands are all known constants into a
/// single constant, reproducing exactly what the op's lowering computes at
/// runtime. Returns `None` when an operand is non-constant or the op/edge case is
/// intentionally not folded (division, modulo, float division, out-of-range
/// shifts, non-signed compare predicates).
fn try_fold(
    function: &Function,
    inst: &Instruction,
    consts: &HashMap<ValueId, Const>,
) -> Option<(Const, TypeNarrowing)> {
    let operand = |index: usize| -> Option<Const> {
        inst.operands.get(index).and_then(|value| consts.get(value).copied())
    };

    match inst.op {
        // -- integer binary arithmetic and bitwise (64-bit wrapping) --
        Op::IAdd | Op::ISub | Op::IMul | Op::IBitAnd | Op::IBitOr | Op::IBitXor => {
            let lhs = operand(0)?.as_i64()?;
            let rhs = operand(1)?.as_i64()?;
            Some((Const::Int(fold_int_binop(inst.op, lhs, rhs)), TypeNarrowing::None))
        }
        // -- integer shifts: only well-defined PHP shift counts (0..=63) --
        Op::IShl | Op::IShrA => {
            let lhs = operand(0)?.as_i64()?;
            let rhs = operand(1)?.as_i64()?;
            if !(0..=63).contains(&rhs) {
                return None;
            }
            let result = match inst.op {
                Op::IShl => lhs.wrapping_shl(rhs as u32),
                _ => lhs >> rhs,
            };
            Some((Const::Int(result), TypeNarrowing::None))
        }
        // -- integer unary --
        Op::INeg => Some((Const::Int(operand(0)?.as_i64()?.wrapping_neg()), TypeNarrowing::None)),
        Op::IBitNot => Some((Const::Int(!operand(0)?.as_i64()?), TypeNarrowing::None)),
        // -- checked integer arithmetic: PHP promotes to float on overflow --
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            let lhs = operand(0)?.as_i64()?;
            let rhs = operand(1)?.as_i64()?;
            let (result, narrowing) = fold_checked_int_binop(inst.op, lhs, rhs);
            Some((result, narrowing))
        }
        // -- float arithmetic (IEEE-754, exact) --
        Op::FAdd | Op::FSub | Op::FMul => {
            let lhs = operand(0)?.as_f64()?;
            let rhs = operand(1)?.as_f64()?;
            Some((Const::Float(fold_float_binop(inst.op, lhs, rhs)), TypeNarrowing::None))
        }
        Op::FNeg => Some((Const::Float(-operand(0)?.as_f64()?), TypeNarrowing::None)),
        // -- signed integer comparison --
        Op::ICmp => {
            let lhs = operand(0)?.as_i64()?;
            let rhs = operand(1)?.as_i64()?;
            let predicate = match inst.immediate.as_ref() {
                Some(Immediate::CmpPredicate(predicate)) => *predicate,
                _ => return None,
            };
            Some((Const::Bool(fold_icmp(predicate, lhs, rhs)?), TypeNarrowing::None))
        }
        // -- scalar predicates over a constant --
        Op::IsNull if null_predicate_can_fold(function, inst, operand(0)?) => {
            Some((Const::Bool(matches!(operand(0)?, Const::Null)), TypeNarrowing::None))
        }
        // A constant NAN is deliberately NOT folded: it is truthy, but PHP 8.5 also reports
        // the coercion, and the warning lives on the runtime truthiness path this fold would
        // delete. See `crate::optimize::fold::casts::try_fold_cast`.
        Op::IsTruthy if matches!(operand(0)?, Const::Float(f) if f.is_nan()) => None,
        Op::IsTruthy => Some((Const::Bool(operand(0)?.truthiness()), TypeNarrowing::None)),
        _ => None,
    }
}

/// Retains null checks when scalar storage interprets the constant's bits as an in-band null.
fn null_predicate_can_fold(function: &Function, inst: &Instruction, constant: Const) -> bool {
    let Some(source) = inst.operands.first().and_then(|value| function.value(*value)) else {
        return false;
    };
    let sentinel = crate::codegen_support::sentinels::NULL_SENTINEL;
    match (source.php_type.codegen_repr(), constant) {
        (PhpType::Int | PhpType::Callable, Const::Int(value)) => {
            super::boxed_narrowing::integer_range_can_narrow(value, value)
        }
        (PhpType::Float, Const::Float(value)) => value.to_bits() != sentinel as u64,
        _ => true,
    }
}

/// Computes a wrapping 64-bit integer binary/bitwise op, matching the native
/// `add`/`sub`/`mul`/`and`/`orr`/`eor` lowering.
fn fold_int_binop(op: Op, lhs: i64, rhs: i64) -> i64 {
    match op {
        Op::IAdd => lhs.wrapping_add(rhs),
        Op::ISub => lhs.wrapping_sub(rhs),
        Op::IMul => lhs.wrapping_mul(rhs),
        Op::IBitAnd => lhs & rhs,
        Op::IBitOr => lhs | rhs,
        Op::IBitXor => lhs ^ rhs,
        _ => unreachable!("fold_int_binop called with non-integer-binop {:?}", op),
    }
}

/// Computes a checked 64-bit integer binary op, matching the runtime helper
/// `__rt_int_{add,sub,mul}_checked` lowering. Returns `(Const::Int(result),
/// ToInt)` when the result fits in `i64`, or `(Const::Float(result), ToFloat)`
/// when it overflows — exactly reproducing PHP's integer-to-float promotion.
fn fold_checked_int_binop(op: Op, lhs: i64, rhs: i64) -> (Const, TypeNarrowing) {
    let checked = match op {
        Op::ICheckedAdd => lhs.checked_add(rhs),
        Op::ICheckedSub => lhs.checked_sub(rhs),
        Op::ICheckedMul => lhs.checked_mul(rhs),
        _ => unreachable!("fold_checked_int_binop called with non-checked {:?}", op),
    };
    match checked {
        Some(result) => (Const::Int(result), TypeNarrowing::ToInt),
        None => {
            // Overflow: PHP promotes both original integer operands to double
            // and then performs the arithmetic in floating point.
            let lhs = lhs as f64;
            let rhs = rhs as f64;
            let promoted = match op {
                Op::ICheckedAdd => lhs + rhs,
                Op::ICheckedSub => lhs - rhs,
                Op::ICheckedMul => lhs * rhs,
                _ => unreachable!(),
            };
            (Const::Float(promoted), TypeNarrowing::ToFloat)
        }
    }
}

/// Computes an IEEE-754 double binary op, matching the `fadd`/`fsub`/`fmul`
/// lowering. `NaN`/`inf` propagate exactly as the hardware would.
fn fold_float_binop(op: Op, lhs: f64, rhs: f64) -> f64 {
    match op {
        Op::FAdd => lhs + rhs,
        Op::FSub => lhs - rhs,
        Op::FMul => lhs * rhs,
        _ => unreachable!("fold_float_binop called with non-float-binop {:?}", op),
    }
}

/// Evaluates a signed integer comparison predicate, or `None` for the ordered
/// float-only predicates that never appear on an `icmp`.
fn fold_icmp(predicate: CmpPredicate, lhs: i64, rhs: i64) -> Option<bool> {
    match predicate {
        CmpPredicate::Eq => Some(lhs == rhs),
        CmpPredicate::Ne => Some(lhs != rhs),
        CmpPredicate::Slt => Some(lhs < rhs),
        CmpPredicate::Sle => Some(lhs <= rhs),
        CmpPredicate::Sgt => Some(lhs > rhs),
        CmpPredicate::Sge => Some(lhs >= rhs),
        CmpPredicate::Olt
        | CmpPredicate::Ole
        | CmpPredicate::Ogt
        | CmpPredicate::Oge => None,
    }
}

/// Rewrites an instruction in place into the `Const*` op for `value`, keeping its
/// result `ValueId` and result type. Operands are cleared and the immediate and
/// effects are reset so the rewrite is validator-clean.
fn convert_to_const(inst: &mut Instruction, value: Const) {
    inst.operands.clear();
    match value {
        Const::Int(n) => {
            inst.op = Op::ConstI64;
            inst.immediate = Some(Immediate::I64(n));
        }
        Const::Float(f) => {
            inst.op = Op::ConstF64;
            inst.immediate = Some(Immediate::F64(f));
        }
        Const::Bool(b) => {
            inst.op = Op::ConstBool;
            inst.immediate = Some(Immediate::Bool(b));
        }
        Const::Null => {
            inst.op = Op::ConstNull;
            inst.immediate = None;
        }
    }
    inst.effects = inst.op.default_effects();
    inst.origin = Some(crate::ir::PassOrigin::ConstFold);
}

/// Narrows the instruction's `result_type`, `result_php_type`, and
/// `result_ownership` to match the folded constant's scalar type. Used when a
/// checked op (result type `Heap(Mixed)`) folds to an `Int` or `Float`
/// constant, so the validator and codegen see the narrowed scalar type.
fn apply_type_narrowing(inst: &mut Instruction, narrowing: TypeNarrowing) {
    match narrowing {
        TypeNarrowing::ToInt => {
            inst.result_type = IrType::I64;
            inst.result_php_type = crate::types::PhpType::Int;
            inst.result_ownership = Ownership::NonHeap;
        }
        TypeNarrowing::ToFloat => {
            inst.result_type = IrType::F64;
            inst.result_php_type = crate::types::PhpType::Float;
            inst.result_ownership = Ownership::NonHeap;
        }
        TypeNarrowing::None => {}
    }
}

/// Narrows the SSA value's `ir_type`, `php_type`, and `ownership` to match the
/// folded constant's scalar type. Mirrors `apply_type_narrowing` but updates
/// the `Value` metadata that the validator checks against the instruction.
fn apply_value_type_narrowing(val: &mut crate::ir::Value, narrowing: TypeNarrowing) {
    match narrowing {
        TypeNarrowing::ToInt => {
            val.ir_type = IrType::I64;
            val.php_type = crate::types::PhpType::Int;
            val.ownership = Ownership::NonHeap;
        }
        TypeNarrowing::ToFloat => {
            val.ir_type = IrType::F64;
            val.php_type = crate::types::PhpType::Float;
            val.ownership = Ownership::NonHeap;
        }
        TypeNarrowing::None => {}
    }
}
