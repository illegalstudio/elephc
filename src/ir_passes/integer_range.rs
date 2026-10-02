//! Purpose:
//! Proves inclusive integer ranges for EIR SSA values and rewrites checked integer
//! arithmetic to unchecked scalar operations only when signed overflow is impossible.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `mem2reg` and checked integer sink specialization.
//!
//! Key details:
//! - Forward dataflow is edge-sensitive for signed integer comparisons.
//! - Natural-loop block parameters with constant-step recurrences receive bounded induction
//!   ranges, preventing abstract interpretation from unrolling loops one iteration at a time.
//! - Every arithmetic proof uses `i128`; unknown or unsupported shapes keep PHP's checked
//!   overflow-to-float path unchanged.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ir::{
    validate_function, BlockId, CmpPredicate, DataPool, Function, Immediate, InstId, IrType,
    Op, Ownership, PassOrigin, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::cfg::has_exception_handlers;
use super::dominance::{compute_dominance, DominanceInfo};
use super::driver::IrPass;
use super::loops::compute_loops;

/// Maximum number of block transfers before a malformed or unexpectedly complex CFG fails closed.
const MAX_DATAFLOW_STEPS: usize = 100_000;

#[cfg(test)]
#[path = "tests/integer_range_domain_test.rs"]
mod domain_tests;

/// Inclusive signed integer interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IntRange {
    /// Smallest possible value.
    pub(super) lo: i64,
    /// Largest possible value.
    pub(super) hi: i64,
}

impl IntRange {
    /// Returns the full signed 64-bit domain.
    const fn full() -> Self {
        Self {
            lo: i64::MIN,
            hi: i64::MAX,
        }
    }

    /// Returns the point interval containing only `value`.
    const fn point(value: i64) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }

    /// Returns the least interval containing both inputs.
    const fn hull(self, other: Self) -> Self {
        Self {
            lo: if self.lo < other.lo { self.lo } else { other.lo },
            hi: if self.hi > other.hi { self.hi } else { other.hi },
        }
    }

    /// Intersects two intervals, returning `None` for an unreachable empty range.
    fn intersect(self, other: Self) -> Option<Self> {
        let range = Self {
            lo: self.lo.max(other.lo),
            hi: self.hi.min(other.hi),
        };
        (range.lo <= range.hi).then_some(range)
    }

    /// Returns the single value carried by a point interval.
    fn exact(self) -> Option<i64> {
        (self.lo == self.hi).then_some(self.lo)
    }
}

/// Path-local integer facts at one block boundary.
type RangeState = HashMap<ValueId, IntRange>;

/// Rewrites overflow-checked integer operations proven safe by range analysis.
pub struct IntegerRange;

impl IrPass for IntegerRange {
    /// Returns the pass name used by validation diagnostics.
    fn name(&self) -> &'static str {
        "integer_range"
    }

    /// Skips functions without checked add, subtract, or multiply operations.
    fn is_applicable(&self, function: &Function) -> bool {
        function
            .instructions
            .iter()
            .any(|inst| unchecked_op(inst.op).is_some())
    }

    /// Analyzes one explicit CFG and rewrites each independently proven operation.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function) {
            return false;
        }
        let induction = discover_induction_ranges(function);
        let Some(states) = analyze_block_entries(function, &induction) else {
            return false;
        };
        let candidates = collect_safe_candidates(function, &states);
        rewrite_candidates(function, &candidates)
    }
}

/// Computes fixed-point entry states for every reachable block.
fn analyze_block_entries(
    function: &Function,
    induction: &HashMap<ValueId, IntRange>,
) -> Option<Vec<Option<RangeState>>> {
    let dominance = compute_dominance(function);
    let loops = compute_loops(function, &dominance);
    let loop_headers: HashSet<BlockId> = loops.loops().iter().map(|lp| lp.header).collect();
    let mut states = vec![None; function.blocks.len()];
    let mut entry = RangeState::new();
    for &param in &function.block(function.entry)?.params {
        if value_is_i64(function, param) {
            entry.insert(param, induction.get(&param).copied().unwrap_or(IntRange::full()));
        }
    }
    states[function.entry.as_raw() as usize] = Some(entry);

    let mut work = VecDeque::from([function.entry]);
    let mut queued: HashSet<BlockId> = HashSet::from([function.entry]);
    let mut expansions: HashMap<(BlockId, ValueId), u8> = HashMap::new();
    let mut steps = 0usize;

    while let Some(block_id) = work.pop_front() {
        queued.remove(&block_id);
        steps += 1;
        if steps > MAX_DATAFLOW_STEPS {
            return None;
        }
        let Some(mut state) = states[block_id.as_raw() as usize].clone() else {
            continue;
        };
        let block = function.block(block_id)?;
        for &inst_id in &block.instructions {
            transfer_instruction(function, inst_id, &mut state);
        }
        let Some(term) = &block.terminator else {
            continue;
        };
        for edge in outgoing_edges(term) {
            let mut edge_state = state.clone();
            if let Some((condition, taken)) = edge.condition {
                if refine_comparison_edge(function, condition, taken, &mut edge_state).is_none() {
                    continue;
                }
            }
            install_target_arguments(
                function,
                edge.target,
                &edge.args,
                induction,
                &mut edge_state,
            );
            retain_values_available_at(function, &dominance, edge.target, &mut edge_state);
            if merge_entry_state(
                function,
                edge.target,
                edge_state,
                &loop_headers,
                induction,
                &mut expansions,
                &mut states,
            ) && queued.insert(edge.target)
            {
                work.push_back(edge.target);
            }
        }
    }
    Some(states)
}

/// One explicit CFG edge plus the branch polarity that selected it.
struct Edge {
    target: BlockId,
    args: Vec<ValueId>,
    condition: Option<(ValueId, bool)>,
}

/// Expands a terminator into its explicit successor edges.
fn outgoing_edges(term: &Terminator) -> Vec<Edge> {
    match term {
        Terminator::Br { target, args } => vec![Edge {
            target: *target,
            args: args.clone(),
            condition: None,
        }],
        Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        } => vec![
            Edge {
                target: *then_target,
                args: then_args.clone(),
                condition: Some((*cond, true)),
            },
            Edge {
                target: *else_target,
                args: else_args.clone(),
                condition: Some((*cond, false)),
            },
        ],
        Terminator::Switch {
            cases,
            default,
            default_args,
            ..
        } => {
            let mut edges: Vec<Edge> = cases
                .iter()
                .map(|case| Edge {
                    target: case.target,
                    args: case.args.clone(),
                    condition: None,
                })
                .collect();
            edges.push(Edge {
                target: *default,
                args: default_args.clone(),
                condition: None,
            });
            edges
        }
        Terminator::GeneratorSuspend {
            resume,
            resume_args,
            ..
        } => vec![Edge {
            target: *resume,
            args: resume_args.clone(),
            condition: None,
        }],
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Drops SSA facts whose definitions are not available at the destination entry.
fn retain_values_available_at(
    function: &Function,
    dominance: &DominanceInfo,
    target: BlockId,
    state: &mut RangeState,
) {
    state.retain(|value, _| match function.value(*value).map(|value| value.def) {
        Some(ValueDef::BlockParam { block, .. }) => dominance.dominates(block, target),
        Some(ValueDef::Instruction { block, .. }) => {
            block != target && dominance.dominates(block, target)
        }
        None => false,
    });
}

/// Rebinds destination block parameters to the ranges of their edge arguments.
fn install_target_arguments(
    function: &Function,
    target: BlockId,
    args: &[ValueId],
    induction: &HashMap<ValueId, IntRange>,
    state: &mut RangeState,
) {
    let Some(block) = function.block(target) else {
        return;
    };
    let argument_ranges: Vec<Option<IntRange>> = args
        .iter()
        .map(|value| state.get(value).copied())
        .collect();
    for (index, &param) in block.params.iter().enumerate() {
        state.remove(&param);
        if !value_is_i64(function, param) {
            continue;
        }
        let range = induction.get(&param).copied().or_else(|| {
            argument_ranges
                .get(index)
                .copied()
                .flatten()
                .or(Some(IntRange::full()))
        });
        if let Some(range) = range {
            state.insert(param, range);
        }
    }
}

/// Joins an incoming edge state into a block entry, widening unstable loop parameters.
fn merge_entry_state(
    function: &Function,
    target: BlockId,
    incoming: RangeState,
    loop_headers: &HashSet<BlockId>,
    induction: &HashMap<ValueId, IntRange>,
    expansions: &mut HashMap<(BlockId, ValueId), u8>,
    states: &mut [Option<RangeState>],
) -> bool {
    let slot = &mut states[target.as_raw() as usize];
    let Some(current) = slot.as_mut() else {
        *slot = Some(incoming);
        return true;
    };
    let params: HashSet<ValueId> = function
        .block(target)
        .map(|block| block.params.iter().copied().collect())
        .unwrap_or_default();
    let mut changed = false;
    // Absence means unknown, not an unseen contribution to the join.
    current.retain(|&value, previous| {
        let Some(&next) = incoming.get(&value) else {
            changed = true;
            return false;
        };
        let mut joined = previous.hull(next);
        if joined != *previous
            && loop_headers.contains(&target)
            && params.contains(&value)
            && !induction.contains_key(&value)
        {
            let count = expansions.entry((target, value)).or_insert(0);
            *count = count.saturating_add(1);
            if *count >= 3 {
                if joined.lo < previous.lo {
                    joined.lo = i64::MIN;
                }
                if joined.hi > previous.hi {
                    joined.hi = i64::MAX;
                }
            }
        }
        if joined != *previous {
            *previous = joined;
            changed = true;
        }
        true
    });
    changed
}

/// Transfers one instruction result into the current path state.
fn transfer_instruction(function: &Function, inst_id: InstId, state: &mut RangeState) {
    let Some(inst) = function.instruction(inst_id) else {
        return;
    };
    let Some(result) = inst.result else {
        return;
    };
    if let Some(range) = instruction_range(function, inst, state) {
        state.insert(result, range);
    } else {
        state.remove(&result);
    }
}

/// Computes one instruction's integer interval from the current operand facts.
fn instruction_range(
    function: &Function,
    inst: &crate::ir::Instruction,
    state: &RangeState,
) -> Option<IntRange> {
    let operand = |index: usize| {
        inst.operands
            .get(index)
            .and_then(|value| range_for_value(function, state, *value))
    };
    match (inst.op, inst.immediate.as_ref()) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(IntRange::point(*value)),
        (Op::ConstBool, Some(Immediate::Bool(value))) => {
            Some(IntRange::point(i64::from(*value)))
        }
        (Op::IAdd | Op::ICheckedAdd | Op::ICheckedAddToInt, _) => {
            checked_add(operand(0)?, operand(1)?).or_else(|| scalar_result_top(function, inst))
        }
        (Op::ISub | Op::ICheckedSub | Op::ICheckedSubToInt, _) => {
            checked_sub(operand(0)?, operand(1)?).or_else(|| scalar_result_top(function, inst))
        }
        (Op::IMul | Op::ICheckedMul | Op::ICheckedMulToInt, _) => {
            checked_mul(operand(0)?, operand(1)?).or_else(|| scalar_result_top(function, inst))
        }
        (Op::IBitAnd, _) => bitand_range(operand(0)?, operand(1)?),
        (Op::IBitOr, _) => bitor_range(operand(0)?, operand(1)?),
        (Op::IBitXor, _) => bitxor_range(operand(0)?, operand(1)?),
        (Op::IShl, _) => shift_left_range(operand(0)?, operand(1)?),
        (Op::IShrA, _) => shift_right_range(operand(0)?, operand(1)?),
        (Op::INeg, _) => negate_range(operand(0)?),
        (Op::IBitNot, _) => Some(IntRange {
            lo: !operand(0)?.hi,
            hi: !operand(0)?.lo,
        }),
        (Op::ICmp, _) => Some(IntRange { lo: 0, hi: 1 }),
        (Op::Cast, Some(Immediate::CastTarget(IrType::I64))) => {
            if inst.result_php_type == PhpType::Bool {
                Some(IntRange { lo: 0, hi: 1 })
            } else if inst.result_php_type == PhpType::Int
                && inst.operands.first().and_then(|value| function.value(*value))
                .is_some_and(|value| matches!(
                    value.php_type, PhpType::Int | PhpType::Bool
                ) || (value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Mixed)
                    && matches!(value.php_type, PhpType::Mixed | PhpType::Union(_))))
            {
                operand(0)
            } else {
                // Resource IDs, nullable scalar tags, strings, and floats are conversions,
                // not identities even when their input happens to use I64 storage.
                scalar_result_top(function, inst)
            }
        }
        _ => scalar_result_top(function, inst),
    }
}

/// Returns a known state range, or the full domain for any scalar I64 value.
fn range_for_value(
    function: &Function,
    state: &RangeState,
    value: ValueId,
) -> Option<IntRange> {
    state
        .get(&value)
        .copied()
        .or_else(|| value_is_i64(function, value).then_some(IntRange::full()))
}

/// Returns the full integer domain for an I64 instruction result.
fn scalar_result_top(
    function: &Function,
    inst: &crate::ir::Instruction,
) -> Option<IntRange> {
    inst.result
        .filter(|result| value_is_i64(function, *result))
        .map(|_| IntRange::full())
}

/// Returns whether one SSA value has the scalar I64 representation.
fn value_is_i64(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::I64)
}

/// Computes an exact add interval when every mathematical result fits in `i64`.
fn checked_add(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    bounded_range(lhs.lo as i128 + rhs.lo as i128, lhs.hi as i128 + rhs.hi as i128)
}

/// Computes an exact subtract interval when every mathematical result fits in `i64`.
fn checked_sub(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    bounded_range(lhs.lo as i128 - rhs.hi as i128, lhs.hi as i128 - rhs.lo as i128)
}

/// Computes an exact multiply interval when every mathematical result fits in `i64`.
fn checked_mul(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    let products = [
        lhs.lo as i128 * rhs.lo as i128,
        lhs.lo as i128 * rhs.hi as i128,
        lhs.hi as i128 * rhs.lo as i128,
        lhs.hi as i128 * rhs.hi as i128,
    ];
    bounded_range(
        *products.iter().min().expect("four products"),
        *products.iter().max().expect("four products"),
    )
}

/// Converts widened endpoints back to a signed 64-bit interval when both fit.
fn bounded_range(lo: i128, hi: i128) -> Option<IntRange> {
    if lo < i64::MIN as i128 || hi > i64::MAX as i128 || lo > hi {
        return None;
    }
    Some(IntRange {
        lo: lo as i64,
        hi: hi as i64,
    })
}

/// Bounds a bitwise AND, with a precise nonnegative mask rule for common index loops.
fn bitand_range(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    if let (Some(lhs), Some(rhs)) = (lhs.exact(), rhs.exact()) {
        return Some(IntRange::point(lhs & rhs));
    }
    if let Some(mask) = rhs.exact().filter(|mask| *mask >= 0) {
        return Some(IntRange { lo: 0, hi: mask });
    }
    if let Some(mask) = lhs.exact().filter(|mask| *mask >= 0) {
        return Some(IntRange { lo: 0, hi: mask });
    }
    if lhs.lo >= 0 && rhs.lo >= 0 {
        return Some(IntRange {
            lo: 0,
            hi: lhs.hi.min(rhs.hi),
        });
    }
    Some(IntRange::full())
}

/// Bounds bitwise OR for exact inputs or nonnegative bounded operands.
fn bitor_range(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    if let (Some(lhs), Some(rhs)) = (lhs.exact(), rhs.exact()) {
        return Some(IntRange::point(lhs | rhs));
    }
    nonnegative_bitwise_upper(lhs, rhs)
}

/// Bounds bitwise XOR for exact inputs or nonnegative bounded operands.
fn bitxor_range(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    if let (Some(lhs), Some(rhs)) = (lhs.exact(), rhs.exact()) {
        return Some(IntRange::point(lhs ^ rhs));
    }
    nonnegative_bitwise_upper(lhs, rhs)
}

/// Returns a conservative all-bits-below-the-top-bit range for nonnegative bitwise operands.
fn nonnegative_bitwise_upper(lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    if lhs.lo < 0 || rhs.lo < 0 {
        return Some(IntRange::full());
    }
    let high = lhs.hi.max(rhs.hi) as u64;
    let upper = if high == 0 {
        0
    } else {
        u64::MAX >> high.leading_zeros()
    };
    Some(IntRange {
        lo: 0,
        hi: upper.min(i64::MAX as u64) as i64,
    })
}

/// Computes a left-shift interval when the count is exact and no shifted value wraps.
fn shift_left_range(value: IntRange, count: IntRange) -> Option<IntRange> {
    let count = count.exact()?;
    if !(0..=63).contains(&count) {
        return None;
    }
    let factor = 1i128 << count;
    bounded_range(value.lo as i128 * factor, value.hi as i128 * factor)
        .or(Some(IntRange::full()))
}

/// Computes an arithmetic right-shift interval for an exact valid count.
fn shift_right_range(value: IntRange, count: IntRange) -> Option<IntRange> {
    let count = count.exact()?;
    if !(0..=63).contains(&count) {
        return None;
    }
    Some(IntRange {
        lo: value.lo >> count,
        hi: value.hi >> count,
    })
}

/// Computes a negation interval unless it may contain `PHP_INT_MIN`.
fn negate_range(value: IntRange) -> Option<IntRange> {
    if value.lo == i64::MIN {
        return Some(IntRange::full());
    }
    Some(IntRange {
        lo: -value.hi,
        hi: -value.lo,
    })
}

/// Refines both operands of a signed integer comparison on one outgoing edge.
fn refine_comparison_edge(
    function: &Function,
    condition: ValueId,
    taken: bool,
    state: &mut RangeState,
) -> Option<()> {
    let ValueDef::Instruction { inst, .. } = function.value(condition)?.def else {
        return Some(());
    };
    let compare = function.instruction(inst)?;
    if compare.op != Op::ICmp || compare.operands.len() != 2 {
        return Some(());
    }
    let Some(Immediate::CmpPredicate(mut predicate)) = compare.immediate else {
        return Some(());
    };
    // Unsupported comparisons cannot prove that either successor is unreachable.
    if matches!(predicate, CmpPredicate::Olt | CmpPredicate::Ole | CmpPredicate::Ogt | CmpPredicate::Oge) {
        return Some(());
    }
    if !taken {
        predicate = invert_predicate(predicate)?;
    }
    let lhs = compare.operands[0];
    let rhs = compare.operands[1];
    let Some((lhs_range, rhs_range)) = range_for_value(function, state, lhs)
        .zip(range_for_value(function, state, rhs))
    else {
        return Some(());
    };
    let (next_lhs, next_rhs) = refine_ranges(lhs_range, predicate, rhs_range)?;
    state.insert(lhs, next_lhs);
    state.insert(rhs, next_rhs);
    Some(())
}

/// Returns interval refinements implied by one true signed comparison.
fn refine_ranges(
    lhs: IntRange,
    predicate: CmpPredicate,
    rhs: IntRange,
) -> Option<(IntRange, IntRange)> {
    match predicate {
        CmpPredicate::Eq => {
            let common = lhs.intersect(rhs)?;
            Some((common, common))
        }
        CmpPredicate::Ne => Some((exclude_point(lhs, rhs), exclude_point(rhs, lhs))),
        CmpPredicate::Slt => refine_less(lhs, rhs, true),
        CmpPredicate::Sle => refine_less(lhs, rhs, false),
        CmpPredicate::Sgt => {
            let (rhs, lhs) = refine_less(rhs, lhs, true)?;
            Some((lhs, rhs))
        }
        CmpPredicate::Sge => {
            let (rhs, lhs) = refine_less(rhs, lhs, false)?;
            Some((lhs, rhs))
        }
        CmpPredicate::Olt
        | CmpPredicate::Ole
        | CmpPredicate::Ogt
        | CmpPredicate::Oge => None,
    }
}

/// Refines `lhs < rhs` or `lhs <= rhs` using the opposite interval's endpoints.
fn refine_less(lhs: IntRange, rhs: IntRange, strict: bool) -> Option<(IntRange, IntRange)> {
    let lhs_hi = if strict {
        rhs.hi.checked_sub(1)?
    } else {
        rhs.hi
    };
    let rhs_lo = if strict {
        lhs.lo.checked_add(1)?
    } else {
        lhs.lo
    };
    Some((
        lhs.intersect(IntRange {
            lo: i64::MIN,
            hi: lhs_hi,
        })?,
        rhs.intersect(IntRange {
            lo: rhs_lo,
            hi: i64::MAX,
        })?,
    ))
}

/// Removes one excluded endpoint when the other side is a point.
fn exclude_point(range: IntRange, excluded: IntRange) -> IntRange {
    let Some(excluded) = excluded.exact() else {
        return range;
    };
    if range.lo == excluded && range.lo < range.hi {
        return IntRange {
            lo: range.lo + 1,
            hi: range.hi,
        };
    }
    if range.hi == excluded && range.lo < range.hi {
        return IntRange {
            lo: range.lo,
            hi: range.hi - 1,
        };
    }
    range
}

/// Returns the logical negation of a signed comparison predicate.
fn invert_predicate(predicate: CmpPredicate) -> Option<CmpPredicate> {
    match predicate {
        CmpPredicate::Eq => Some(CmpPredicate::Ne),
        CmpPredicate::Ne => Some(CmpPredicate::Eq),
        CmpPredicate::Slt => Some(CmpPredicate::Sge),
        CmpPredicate::Sle => Some(CmpPredicate::Sgt),
        CmpPredicate::Sgt => Some(CmpPredicate::Sle),
        CmpPredicate::Sge => Some(CmpPredicate::Slt),
        CmpPredicate::Olt
        | CmpPredicate::Ole
        | CmpPredicate::Ogt
        | CmpPredicate::Oge => None,
    }
}

/// Returns the predicate for the same comparison after swapping its operands.
fn swap_predicate(predicate: CmpPredicate) -> Option<CmpPredicate> {
    match predicate {
        CmpPredicate::Eq | CmpPredicate::Ne => Some(predicate),
        CmpPredicate::Slt => Some(CmpPredicate::Sgt),
        CmpPredicate::Sle => Some(CmpPredicate::Sge),
        CmpPredicate::Sgt => Some(CmpPredicate::Slt),
        CmpPredicate::Sge => Some(CmpPredicate::Sle),
        CmpPredicate::Olt
        | CmpPredicate::Ole
        | CmpPredicate::Ogt
        | CmpPredicate::Oge => None,
    }
}

/// Finds bounded constant-step induction parameters in natural loops.
fn discover_induction_ranges(function: &Function) -> HashMap<ValueId, IntRange> {
    let dominance = compute_dominance(function);
    let loops = compute_loops(function, &dominance);
    let mut summaries = HashMap::new();
    for lp in loops.loops() {
        let Some(preheader) = lp.preheader else {
            continue;
        };
        let Some(header) = function.block(lp.header) else {
            continue;
        };
        let Some(init_args) = branch_args_to(function, preheader, lp.header) else {
            continue;
        };
        let Some((condition, continue_when_true)) = loop_condition(function, lp) else {
            continue;
        };
        for (index, &param) in header.params.iter().enumerate() {
            if !value_is_i64(function, param) {
                continue;
            }
            let Some(&init) = init_args.get(index) else {
                continue;
            };
            let Some(step) = common_recurrence_step(function, lp, index, param, &summaries)
            else {
                continue;
            };
            let Some((predicate, bound)) = induction_condition(
                function,
                condition,
                continue_when_true,
                param,
            ) else {
                continue;
            };
            if value_def_block(function, bound).is_some_and(|block| lp.contains(block)) {
                continue;
            }
            let mut memo = HashMap::new();
            let Some(init_range) = static_value_range(
                function,
                init,
                &summaries,
                &mut memo,
            ) else {
                continue;
            };
            let Some(bound_range) = static_value_range(
                function,
                bound,
                &summaries,
                &mut memo,
            ) else {
                continue;
            };
            if let Some(summary) = induction_summary(init_range, step, predicate, bound_range) {
                summaries.insert(param, summary);
            }
        }
    }
    summaries
}

/// Returns the block defining a value.
fn value_def_block(function: &Function, value: ValueId) -> Option<BlockId> {
    match function.value(value)?.def {
        ValueDef::BlockParam { block, .. } | ValueDef::Instruction { block, .. } => Some(block),
    }
}

/// Returns arguments only when every parallel edge to the target agrees.
fn branch_args_to(
    function: &Function,
    from: BlockId,
    target: BlockId,
) -> Option<Vec<ValueId>> {
    let term = function.block(from)?.terminator.as_ref()?;
    let mut edges = outgoing_edges(term)
        .into_iter()
        .filter(|edge| edge.target == target);
    let args = edges.next()?.args;
    edges.all(|edge| edge.args == args).then_some(args)
}

/// Finds a loop header comparison and whether its true edge continues the loop.
fn loop_condition(
    function: &Function,
    lp: &super::loops::NaturalLoop,
) -> Option<(ValueId, bool)> {
    let Terminator::CondBr {
        cond,
        then_target,
        else_target,
        ..
    } = function.block(lp.header)?.terminator.as_ref()?
    else {
        return None;
    };
    match (lp.contains(*then_target), lp.contains(*else_target)) {
        (true, false) => Some((*cond, true)),
        (false, true) => Some((*cond, false)),
        _ => None,
    }
}

/// Requires every loop latch to pass the same constant-step recurrence for a parameter.
fn common_recurrence_step(
    function: &Function,
    lp: &super::loops::NaturalLoop,
    index: usize,
    param: ValueId,
    summaries: &HashMap<ValueId, IntRange>,
) -> Option<i64> {
    let mut common = None;
    for &latch in &lp.latches {
        let args = branch_args_to(function, latch, lp.header)?;
        let recurrence = *args.get(index)?;
        let step = recurrence_step(function, recurrence, param, summaries)?;
        match common {
            Some(previous) if previous != step => return None,
            Some(_) => {}
            None => common = Some(step),
        }
    }
    common.filter(|step| *step != 0)
}

/// Matches `param + constant` or `param - constant` recurrences.
fn recurrence_step(
    function: &Function,
    recurrence: ValueId,
    param: ValueId,
    summaries: &HashMap<ValueId, IntRange>,
) -> Option<i64> {
    let ValueDef::Instruction { inst, .. } = function.value(recurrence)?.def else {
        return None;
    };
    let instruction = function.instruction(inst)?;
    if !matches!(
        instruction.op,
        Op::IAdd | Op::ICheckedAddToInt | Op::ISub | Op::ICheckedSubToInt
    ) || instruction.operands.len() != 2
    {
        return None;
    }
    let mut memo = HashMap::new();
    match instruction.op {
        Op::IAdd | Op::ICheckedAddToInt if instruction.operands[0] == param => {
            static_value_range(
                function,
                instruction.operands[1],
                summaries,
                &mut memo,
            )?
            .exact()
        }
        Op::IAdd | Op::ICheckedAddToInt if instruction.operands[1] == param => {
            static_value_range(
                function,
                instruction.operands[0],
                summaries,
                &mut memo,
            )?
            .exact()
        }
        Op::ISub | Op::ICheckedSubToInt if instruction.operands[0] == param => {
            static_value_range(
                function,
                instruction.operands[1],
                summaries,
                &mut memo,
            )?
            .exact()?
            .checked_neg()
        }
        _ => None,
    }
}

/// Normalizes a loop continuation comparison to `param predicate bound`.
fn induction_condition(
    function: &Function,
    condition: ValueId,
    continue_when_true: bool,
    param: ValueId,
) -> Option<(CmpPredicate, ValueId)> {
    let ValueDef::Instruction { inst, .. } = function.value(condition)?.def else {
        return None;
    };
    let compare = function.instruction(inst)?;
    if compare.op != Op::ICmp || compare.operands.len() != 2 {
        return None;
    }
    let Some(Immediate::CmpPredicate(mut predicate)) = compare.immediate else {
        return None;
    };
    if !continue_when_true {
        predicate = invert_predicate(predicate)?;
    }
    if compare.operands[0] == param {
        return Some((predicate, compare.operands[1]));
    }
    if compare.operands[1] == param {
        return Some((swap_predicate(predicate)?, compare.operands[0]));
    }
    None
}

/// Builds a sound header range after first proving the checked recurrence cannot overflow.
fn induction_summary(
    init: IntRange,
    step: i64,
    predicate: CmpPredicate,
    bound: IntRange,
) -> Option<IntRange> {
    let preliminary = match (step.signum(), predicate) {
        (1, CmpPredicate::Slt) => IntRange {
            lo: i64::MIN,
            hi: bound.hi.checked_sub(1)?,
        },
        (1, CmpPredicate::Sle) => IntRange {
            lo: i64::MIN,
            hi: bound.hi,
        },
        (-1, CmpPredicate::Sgt) => IntRange {
            lo: bound.lo.checked_add(1)?,
            hi: i64::MAX,
        },
        (-1, CmpPredicate::Sge) => IntRange {
            lo: bound.lo,
            hi: i64::MAX,
        },
        _ => return None,
    };
    checked_add(preliminary, IntRange::point(step))?;
    let body = if step > 0 {
        preliminary.intersect(IntRange {
            lo: init.lo,
            hi: i64::MAX,
        })?
    } else {
        preliminary.intersect(IntRange {
            lo: i64::MIN,
            hi: init.hi,
        })?
    };
    let next = checked_add(body, IntRange::point(step))?;
    Some(init.hull(next))
}

/// Evaluates an expression DAG in postorder, caching unknown results without native recursion.
fn static_value_range(
    function: &Function,
    value: ValueId,
    summaries: &HashMap<ValueId, IntRange>,
    memo: &mut HashMap<ValueId, Option<IntRange>>,
) -> Option<IntRange> {
    let mut work = vec![(value, false)];
    let mut visiting = HashSet::new();
    while let Some((current, ready)) = work.pop() {
        if let Some(range) = summaries.get(&current) {
            memo.insert(current, Some(*range));
            continue;
        }
        if memo.contains_key(&current) {
            continue;
        }
        let range = match function.value(current)?.def {
            ValueDef::BlockParam { .. } => {
                value_is_i64(function, current).then_some(IntRange::full())
            }
            ValueDef::Instruction { inst, .. } => {
                let instruction = function.instruction(inst)?;
                if !ready {
                    if !visiting.insert(current) {
                        return None;
                    }
                    work.push((current, true));
                    work.extend(instruction.operands.iter().map(|operand| (*operand, false)));
                    continue;
                }
                visiting.remove(&current);
                let state = instruction.operands.iter().filter_map(|operand| {
                    memo.get(operand).copied().flatten().map(|range| (*operand, range))
                }).collect();
                instruction_range(function, instruction, &state)
            }
        };
        memo.insert(current, range);
    }
    memo.get(&value).copied().flatten()
}

/// Replays final entry states to collect checked operations whose complete interval fits.
fn collect_safe_candidates(
    function: &Function,
    states: &[Option<RangeState>],
) -> Vec<InstId> {
    let mut candidates = Vec::new();
    for block in &function.blocks {
        let Some(mut state) = states[block.id.as_raw() as usize].clone() else {
            continue;
        };
        for &inst_id in &block.instructions {
            let Some(inst) = function.instruction(inst_id) else {
                continue;
            };
            if unchecked_op(inst.op).is_some()
                && inst.operands.len() == 2
                && range_for_value(function, &state, inst.operands[0])
                    .zip(range_for_value(function, &state, inst.operands[1]))
                    .and_then(|(lhs, rhs)| checked_binop_range(inst.op, lhs, rhs))
                    .filter(|range| inst.result_type == IrType::I64
                        || super::boxed_narrowing::integer_range_can_narrow(range.lo, range.hi))
                    .is_some()
            {
                candidates.push(inst_id);
            }
            transfer_instruction(function, inst_id, &mut state);
        }
    }
    candidates
}

/// Computes the mathematical interval for one checked arithmetic opcode.
fn checked_binop_range(op: Op, lhs: IntRange, rhs: IntRange) -> Option<IntRange> {
    match op {
        Op::ICheckedAdd | Op::ICheckedAddToInt => checked_add(lhs, rhs),
        Op::ICheckedSub | Op::ICheckedSubToInt => checked_sub(lhs, rhs),
        Op::ICheckedMul | Op::ICheckedMulToInt => checked_mul(lhs, rhs),
        _ => None,
    }
}

/// Maps a checked operation to its unchecked scalar counterpart.
fn unchecked_op(op: Op) -> Option<Op> {
    match op {
        Op::ICheckedAdd | Op::ICheckedAddToInt => Some(Op::IAdd),
        Op::ICheckedSub | Op::ICheckedSubToInt => Some(Op::ISub),
        Op::ICheckedMul | Op::ICheckedMulToInt => Some(Op::IMul),
        _ => None,
    }
}

/// Applies scalar proofs directly and validates boxed narrowing as one fail-closed batch.
fn rewrite_candidates(function: &mut Function, candidates: &[InstId]) -> bool {
    if candidates.is_empty() {
        return false;
    }
    let blocked = super::boxed_narrowing::blocked_results(function, IrType::I64);
    let mut boxed = Vec::new();
    let mut changed = false;
    for &candidate in candidates {
        let Some(inst) = function.instruction(candidate) else {
            continue;
        };
        if inst.result_type == IrType::I64 {
            rewrite_candidate(function, candidate);
            changed = true;
        } else if inst.result.is_some_and(|result| !blocked.contains(&result)) {
            boxed.push(candidate);
        }
    }
    if boxed.is_empty() {
        return changed;
    }
    // Avoid cloning and validating the entire function for every arithmetic operation.
    // If any representation contract rejects the batch, retain all its checked forms.
    let mut trial = function.clone();
    for candidate in boxed {
        rewrite_candidate(&mut trial, candidate);
    }
    if validate_function(&trial).is_ok() {
        *function = trial;
        return true;
    }
    changed
}

/// Rewrites one proven checked operation and narrows boxed result metadata when required.
fn rewrite_candidate(function: &mut Function, candidate: InstId) {
    let Some(replacement) = function
        .instruction(candidate)
        .and_then(|inst| unchecked_op(inst.op))
    else {
        return;
    };
    let result = function.instruction(candidate).and_then(|inst| inst.result);
    if let Some(inst) = function.instruction_mut(candidate) {
        inst.op = replacement;
        inst.result_type = IrType::I64;
        inst.result_php_type = PhpType::Int;
        inst.result_ownership = Ownership::NonHeap;
        inst.effects = replacement.default_effects();
        inst.origin = Some(PassOrigin::IntegerRange);
    }
    if let Some(value) = result.and_then(|result| function.value_mut(result)) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
}
