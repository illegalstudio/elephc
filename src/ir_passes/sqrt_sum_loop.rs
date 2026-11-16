//! Purpose:
//! Replaces a scalar `sqrt($i + c)` accumulation loop with one range-reducer call.
//!
//! Called from:
//! - The EIR fixed-point driver after integer-range specialization and loop normalization.
//!
//! Key details:
//! - The matched loop is `for ($i = start; $i < bound; $i++) { $sum += sqrt($i + c); }`
//!   with a float accumulator, a unit induction, and a constant integer addend.
//! - The body must be exactly the boxed checked-add, sqrt call, release, float add and
//!   unit latch scaffold plus nops/concat resets; any other instruction or observation
//!   rejects the rewrite.
//! - `__rt_sqrt_sum_range` adds every square root in source order, so PHP's exact float
//!   result and its checked-add overflow promotion are preserved. The induction value must
//!   not be observed after the loop; accumulator uses after the loop are replaced by the
//!   helper result.
//! - The preheader branches straight to the loop exit; the now-unreachable loop blocks are
//!   neutralized by the later branch simplification pass.

use std::collections::HashMap;

use crate::ir::{
    BlockId, CmpPredicate, DataPool, Function, Immediate, InstId, Instruction, IrType, Op,
    Ownership, PassOrigin, RuntimeCallTarget, RuntimeFnId, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::cfg::has_exception_handlers;
use super::dominance::{compute_dominance, DominanceInfo};
use super::driver::IrPass;
use super::licm::build_def_block;
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::{defining_instruction, replace_all_uses};

/// Rewrites a proven scalar square-root accumulation loop into one runtime call.
pub struct SqrtSumLoop;

impl IrPass for SqrtSumLoop {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "sqrt_sum_loop"
    }

    /// Rewrites every loop whose body is exactly the scalar sqrt reduction scaffold.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function)
            || function.flags.is_generator
            || function.flags.is_fiber_wrapper
            || super::local_scope::has_dynamic_eval(function)
            || function.instructions.iter().any(|inst| {
                inst.op == Op::RuntimeCall && matches!(inst.immediate, Some(Immediate::Data(_)))
            })
        {
            return false;
        }
        let dominance = compute_dominance(function);
        let loops = compute_loops(function, &dominance);
        if loops.is_empty() {
            return false;
        }
        let def_block = build_def_block(function);
        let mut changed = false;
        for loop_ref in loops.loops() {
            if loop_ref.blocks.len() != 2 {
                continue;
            }
            let Some(plan) = reduction_plan(function, loop_ref, &def_block, &dominance) else {
                continue;
            };
            apply(function, plan);
            changed = true;
        }
        changed
    }
}

/// The proven sqrt range reduction scaffold.
struct ReductionPlan {
    preheader: BlockId,
    exit: BlockId,
    accumulator: ValueId,
    initial_accumulator: ValueId,
    initial_counter: ValueId,
    bound: ValueId,
    addend: ValueId,
}

/// Recovers the exact sqrt reduction scaffold from a two-block natural loop.
fn reduction_plan(
    function: &Function,
    loop_ref: &NaturalLoop,
    def_block: &HashMap<ValueId, BlockId>,
    dominance: &DominanceInfo,
) -> Option<ReductionPlan> {
    let preheader = loop_ref.preheader?;
    let header = function.block(loop_ref.header)?;
    if header.params.len() != 2 || loop_ref.latches.len() != 1 {
        return None;
    }
    let body = loop_ref
        .blocks
        .iter()
        .copied()
        .find(|&block| block != loop_ref.header)?;
    let Some(Terminator::CondBr {
        cond,
        then_target,
        then_args,
        else_target,
        else_args,
    }) = header.terminator.as_ref()
    else {
        return None;
    };
    if *then_target != body
        || !then_args.is_empty()
        || !else_args.is_empty()
        || !function.block(*else_target)?.params.is_empty()
    {
        return None;
    }
    let condition = defining_instruction(function, *cond)?;
    if condition.op != Op::ICmp
        || condition.immediate != Some(Immediate::CmpPredicate(CmpPredicate::Slt))
        || condition.operands.len() != 2
    {
        return None;
    }
    let counter = condition.operands[0];
    let bound = condition.operands[1];
    if !header.params.contains(&counter) {
        return None;
    }
    let accumulator = *header
        .params
        .iter()
        .find(|&&param| param != counter)?;
    if function.value(accumulator)?.ir_type != IrType::F64
        || function.value(counter)?.ir_type != IrType::I64
    {
        return None;
    }
    if def_block
        .get(&bound)
        .is_some_and(|block| loop_ref.contains(*block))
    {
        return None;
    }
    let Some(Terminator::Br {
        target,
        args: preheader_args,
    }) = function.block(preheader)?.terminator.as_ref()
    else {
        return None;
    };
    if *target != loop_ref.header || preheader_args.len() != header.params.len() {
        return None;
    }
    let accumulator_position = header
        .params
        .iter()
        .position(|&param| param == accumulator)?;
    let counter_position = header
        .params
        .iter()
        .position(|&param| param == counter)?;
    let initial_accumulator = *preheader_args.get(accumulator_position)?;
    let initial_counter = *preheader_args.get(counter_position)?;
    if !dominance.dominates(preheader, *else_target) {
        return None;
    }
    let body_plan = body_plan(function, body, loop_ref, accumulator, counter)?;
    if !induction_has_no_external_uses(function, loop_ref, counter) {
        return None;
    }
    if !accumulator_uses_stay_in_exit(function, loop_ref, accumulator, *else_target) {
        return None;
    }
    let addend = body_plan.addend?;
    if def_block
        .get(&addend)
        .is_some_and(|block| loop_ref.contains(*block))
    {
        return None;
    }
    Some(ReductionPlan {
        preheader,
        exit: *else_target,
        accumulator,
        initial_accumulator,
        initial_counter,
        bound,
        addend,
    })
}

/// The body instructions recovered from the loop.
struct BodyPlan {
    addend: Option<ValueId>,
}

/// Recovers and validates the single body block of the loop.
fn body_plan(
    function: &Function,
    body: BlockId,
    loop_ref: &NaturalLoop,
    accumulator: ValueId,
    counter: ValueId,
) -> Option<BodyPlan> {
    let block = function.block(body)?;
    let mut checked_add = None;
    let mut sqrt_call = None;
    let mut release = None;
    let mut float_add = None;
    let mut counter_add = None;
    for &id in &block.instructions {
        let inst = function.instruction(id)?;
        match inst.op {
            Op::Nop | Op::ConcatReset => {}
            Op::ICheckedAdd => {
                if checked_add.replace(id).is_some() || inst.result.is_none() {
                    return None;
                }
            }
            Op::RuntimeCall => {
                if !matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                        RuntimeFnId::Sqrt
                    )))
                ) || inst.result.is_none()
                {
                    return None;
                }
                if sqrt_call.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release => {
                if release.replace(id).is_some() {
                    return None;
                }
            }
            Op::FAdd => {
                if float_add.replace(id).is_some() || inst.result.is_none() {
                    return None;
                }
            }
            Op::IAdd => {
                if counter_add.replace(id).is_some() || inst.result.is_none() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    let checked_add = checked_add?;
    let sqrt_call = sqrt_call?;
    let float_add = float_add?;
    let counter_add = counter_add?;
    let add = function.instruction(checked_add)?;
    if add.operands.len() != 2 || add.result.is_none() {
        return None;
    }
    let addend = if add.operands[0] == counter {
        add.operands[1]
    } else if add.operands[1] == counter {
        add.operands[0]
    } else {
        return None;
    };
    if constant_i64(function, addend).is_none() {
        return None;
    }
    let checked_result = add.result?;
    let sqrt = function.instruction(sqrt_call)?;
    if sqrt.operands.as_slice() != [checked_result] {
        return None;
    }
    let sqrt_result = sqrt.result?;
    if let Some(release) = release {
        if function.instruction(release)?.operands.as_slice() != [checked_result] {
            return None;
        }
    }
    let fadd = function.instruction(float_add)?;
    if fadd.operands.len() != 2 || fadd.result.is_none() {
        return None;
    }
    let accumulator_latch = if fadd.operands[0] == accumulator {
        fadd.operands[1]
    } else if fadd.operands[1] == accumulator {
        fadd.operands[0]
    } else {
        return None;
    };
    if accumulator_latch != sqrt_result {
        return None;
    }
    let float_add_result = fadd.result?;
    let counter_inst = function.instruction(counter_add)?;
    if !matches!(counter_inst.origin, Some(PassOrigin::IntegerRange)) {
        return None;
    }
    if counter_inst.operands.len() != 2
        || !matches!(
            counter_inst.immediate,
            None | Some(Immediate::Bool(false))
        )
    {
        return None;
    }
    let one = if counter_inst.operands[0] == counter {
        counter_inst.operands[1]
    } else if counter_inst.operands[1] == counter {
        counter_inst.operands[0]
    } else {
        return None;
    };
    if constant_i64(function, one) != Some(1) {
        return None;
    }
    let counter_add_result = counter_inst.result?;
    let Some(Terminator::Br { target, args }) = block.terminator.as_ref() else {
        return None;
    };
    if *target != loop_ref.header {
        return None;
    }
    let accumulator_position = function
        .block(loop_ref.header)?
        .params
        .iter()
        .position(|&param| param == accumulator)?;
    let counter_position = function
        .block(loop_ref.header)?
        .params
        .iter()
        .position(|&param| param == counter)?;
    let accumulator_latch = *args.get(accumulator_position)?;
    let counter_latch = *args.get(counter_position)?;
    if accumulator_latch != float_add_result || counter_latch != counter_add_result {
        return None;
    }
    if !checked_result_uses_are_scaffold(function, checked_result, sqrt_call, release) {
        return None;
    }
    if !single_use(function, sqrt_result, float_add)
        || !single_terminator_use(function, float_add_result, body)
        || !single_terminator_use(function, counter_add_result, body)
    {
        return None;
    }
    Some(BodyPlan {
        addend: Some(addend),
    })
}

/// Returns true when the checked-add result is used only by the sqrt call and its release.
fn checked_result_uses_are_scaffold(
    function: &Function,
    checked_result: ValueId,
    sqrt_call: InstId,
    release: Option<InstId>,
) -> bool {
    let mut uses = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&checked_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == sqrt_call || Some(id) == release {
            uses += 1;
        } else {
            return false;
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&checked_result))
        {
            return false;
        }
    }
    uses == if release.is_some() { 2 } else { 1 }
}

/// Returns true when `value` has exactly one use, and that use is `instruction`.
fn single_use(function: &Function, value: ValueId, instruction: InstId) -> bool {
    let mut uses = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if inst.operands.contains(&value) {
            uses += 1;
            if InstId::from_raw(raw as u32) != instruction {
                return false;
            }
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&value))
        {
            return false;
        }
    }
    uses == 1
}

/// Returns true when `value` is used only by the given block's terminator.
fn single_terminator_use(function: &Function, value: ValueId, block: BlockId) -> bool {
    for inst in &function.instructions {
        if inst.operands.contains(&value) {
            return false;
        }
    }
    let mut uses = 0;
    for candidate in &function.blocks {
        if candidate
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&value))
        {
            if candidate.id != block {
                return false;
            }
            uses += 1;
        }
    }
    uses == 1
}

/// Rejects a loop whose counter value is observed outside the loop.
fn induction_has_no_external_uses(
    function: &Function,
    loop_ref: &NaturalLoop,
    param: ValueId,
) -> bool {
    for block in &function.blocks {
        let inside = loop_ref.contains(block.id);
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if inst.operands.contains(&param) && !inside {
                return false;
            }
        }
        if let Some(term) = block.terminator.as_ref() {
            if term.used_values().contains(&param) && !inside {
                return false;
            }
        }
    }
    true
}

/// Rejects an accumulator observed in a block other than the loop exit.
fn accumulator_uses_stay_in_exit(
    function: &Function,
    loop_ref: &NaturalLoop,
    accumulator: ValueId,
    exit: BlockId,
) -> bool {
    for block in &function.blocks {
        if loop_ref.contains(block.id) {
            continue;
        }
        for &id in &block.instructions {
            let Some(inst) = function.instruction(id) else {
                return false;
            };
            if inst.operands.contains(&accumulator) && block.id != exit {
                return false;
            }
        }
        if let Some(term) = block.terminator.as_ref() {
            if term.used_values().contains(&accumulator) && block.id != exit {
                return false;
            }
        }
    }
    true
}

/// Reads one integer constant value, if the value is defined by `const_i64`.
fn constant_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    if inst.op != Op::ConstI64 {
        return None;
    }
    match inst.immediate {
        Some(Immediate::I64(value)) => Some(value),
        _ => None,
    }
}

/// Emits the helper call in the preheader and redirects the loop to its exit.
fn apply(function: &mut Function, plan: ReductionPlan) {
    let Some(result) = push_runtime_call(
        function,
        plan.preheader,
        vec![
            plan.initial_accumulator,
            plan.initial_counter,
            plan.bound,
            plan.addend,
        ],
    ) else {
        return;
    };
    replace_all_uses(
        function,
        &HashMap::from([(plan.accumulator, result)]),
    );
    if let Some(block) = function.block_mut(plan.preheader) {
        block.terminator = Some(Terminator::Br {
            target: plan.exit,
            args: vec![],
        });
    }
}

/// Appends the `SqrtSumRange` runtime call to the preheader.
fn push_runtime_call(function: &mut Function, block: BlockId, operands: Vec<ValueId>) -> Option<ValueId> {
    let block_index = block.as_raw() as usize;
    let position = function.blocks.get(block_index)?.instructions.len();
    let result = ValueId::from_raw(function.values.len() as u32);
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    function.values.push(crate::ir::Value {
        ir_type: IrType::F64,
        php_type: PhpType::Float,
        def: ValueDef::Instruction {
            block,
            index: position as u32,
            inst: inst_id,
        },
        ownership: Ownership::NonHeap,
    });
    let mut inst = Instruction::new(
        Op::RuntimeCall,
        operands,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
            RuntimeFnId::SqrtSumRange,
        ))),
        Some(result),
        IrType::F64,
        PhpType::Float,
        Ownership::NonHeap,
        RuntimeFnId::SqrtSumRange.intrinsic_effects(),
        None,
    );
    inst.origin = Some(PassOrigin::SqrtSumLoop);
    function.instructions.push(inst);
    function.blocks[block_index].instructions.push(inst_id);
    Some(result)
}
