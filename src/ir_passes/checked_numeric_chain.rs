//! Purpose:
//! Fuses boxed left-associated checked integer arithmetic chains at an integer cast sink.
//! Keeps every in-range intermediate in scalar registers instead of allocating Mixed cells.
//!
//! Called from:
//! - The fixed-point EIR pass driver after single-operation checked-int sinking.
//!
//! Key details:
//! - Only add/sub/mul chains in one basic block are accepted, and every boxed intermediate
//!   may be used only by the next chain operation plus removable release bookkeeping.
//! - The fused opcode retains the ordered operation list so codegen can switch to PHP float
//!   semantics at the first overflowing operation and evaluate the remaining suffix exactly.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, CheckedNumericChainImmediate, DataPool, Function, Immediate, InstId, IrType,
    MixedNumericOp, Op, Ownership, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Fuses a checked numeric region whose sole observable result is an integer cast.
pub struct CheckedNumericChain;

impl IrPass for CheckedNumericChain {
    /// Returns the stable pass name used in fixed-point diagnostics.
    fn name(&self) -> &'static str {
        "checked_numeric_chain"
    }

    /// Skips functions that contain no integer cast fed by dynamic numeric arithmetic.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            inst.op == Op::Cast
                && matches!(inst.immediate, Some(Immediate::CastTarget(IrType::I64)))
                && inst.operands.first().is_some_and(|value| {
                    defining_instruction(function, *value)
                        .is_some_and(|(_, definition)| definition.op == Op::MixedNumericBinop)
                })
        })
    }

    /// Finds and commits every independent checked numeric chain in the function.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let uses = instruction_uses(function);
        let terminator_uses = terminator_used_values(function);
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(raw, inst)| {
                (inst.op == Op::Cast
                    && matches!(inst.immediate, Some(Immediate::CastTarget(IrType::I64))))
                .then_some(InstId::from_raw(raw as u32))
            })
            .collect();
        let mut changed = false;

        for sink in candidates {
            let Some(rewrite) = analyze_candidate(function, sink, &uses, &terminator_uses) else {
                continue;
            };
            apply_rewrite(function, sink, rewrite);
            changed = true;
        }
        changed
    }
}

/// Deferred data needed to replace one boxed region atomically.
struct ChainRewrite {
    operands: Vec<ValueId>,
    operations: Vec<MixedNumericOp>,
    neutralize: HashSet<InstId>,
}

/// Proves one cast sink and returns its ordered scalar expression when safe.
fn analyze_candidate(
    function: &Function,
    sink: InstId,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
) -> Option<ChainRewrite> {
    let sink_inst = function.instruction(sink)?;
    let root = *sink_inst.operands.first()?;
    let sink_block = instruction_block(function, sink)?;
    let rewrite = collect_chain(
        function,
        root,
        sink,
        sink_block,
        uses,
        terminator_uses,
    )?;
    (rewrite.operations.len() >= 2).then_some(rewrite)
}

/// Recursively collects a left-associated chain ending at `expected_user`.
fn collect_chain(
    function: &Function,
    value: ValueId,
    expected_user: InstId,
    sink_block: BlockId,
    uses: &HashMap<ValueId, Vec<InstId>>,
    terminator_uses: &HashSet<ValueId>,
) -> Option<ChainRewrite> {
    let (definition_id, definition) = defining_instruction(function, value)?;
    if instruction_block(function, definition_id)? != sink_block || terminator_uses.contains(&value)
    {
        return None;
    }
    let releases = removable_uses(function, value, expected_user, uses)?;

    let mut rewrite = match definition.op {
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            let lhs = *definition.operands.first()?;
            let rhs = *definition.operands.get(1)?;
            if !value_is_i64(function, lhs) || !value_is_i64(function, rhs) {
                return None;
            }
            ChainRewrite {
                operands: vec![lhs, rhs],
                operations: vec![checked_operation(definition.op)?],
                neutralize: HashSet::new(),
            }
        }
        Op::MixedNumericBinop => {
            let lhs = *definition.operands.first()?;
            let Some(Immediate::MixedNumericOp(operation)) = definition.immediate else {
                return None;
            };
            if matches!(operation, MixedNumericOp::Pow | MixedNumericOp::UnaryPlus) {
                return None;
            }
            let rhs = *definition.operands.get(1)?;
            if !value_is_i64(function, rhs) {
                return None;
            }
            let mut inner = collect_chain(
                function,
                lhs,
                definition_id,
                sink_block,
                uses,
                terminator_uses,
            )?;
            inner.operands.push(rhs);
            inner.operations.push(operation);
            inner
        }
        _ => return None,
    };

    rewrite.neutralize.insert(definition_id);
    rewrite.neutralize.extend(releases);
    Some(rewrite)
}

/// Accepts exactly one semantic consumer plus any number of release instructions.
fn removable_uses(
    function: &Function,
    value: ValueId,
    expected_user: InstId,
    uses: &HashMap<ValueId, Vec<InstId>>,
) -> Option<Vec<InstId>> {
    let mut saw_expected = false;
    let mut releases = Vec::new();
    for user_id in uses.get(&value)? {
        if *user_id == expected_user && !saw_expected {
            saw_expected = true;
            continue;
        }
        let user = function.instruction(*user_id)?;
        if user.op != Op::Release || user.operands.as_slice() != [value] {
            return None;
        }
        releases.push(*user_id);
    }
    saw_expected.then_some(releases)
}

/// Maps a boxed checked integer opcode to the chain's backend-neutral operation.
fn checked_operation(op: Op) -> Option<MixedNumericOp> {
    match op {
        Op::ICheckedAdd => Some(MixedNumericOp::Add),
        Op::ICheckedSub => Some(MixedNumericOp::Sub),
        Op::ICheckedMul => Some(MixedNumericOp::Mul),
        _ => None,
    }
}

/// Returns the defining instruction for one SSA value.
fn defining_instruction(function: &Function, value: ValueId) -> Option<(InstId, &crate::ir::Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Returns the basic block containing one instruction result or instruction list entry.
fn instruction_block(function: &Function, inst: InstId) -> Option<BlockId> {
    let instruction = function.instruction(inst)?;
    if let Some(result) = instruction.result {
        let ValueDef::Instruction { block, .. } = function.value(result)?.def else {
            return None;
        };
        return Some(block);
    }
    function
        .blocks
        .iter()
        .find(|block| block.instructions.contains(&inst))
        .map(|block| block.id)
}

/// Returns whether an SSA value has concrete integer storage.
fn value_is_i64(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| value.ir_type == IrType::I64)
}

/// Builds an instruction-use adjacency list for all SSA operands.
fn instruction_uses(function: &Function) -> HashMap<ValueId, Vec<InstId>> {
    let mut uses: HashMap<ValueId, Vec<InstId>> = HashMap::new();
    for (raw, instruction) in function.instructions.iter().enumerate() {
        let inst = InstId::from_raw(raw as u32);
        for operand in &instruction.operands {
            uses.entry(*operand).or_default().push(inst);
        }
    }
    uses
}

/// Collects SSA values read by terminators, which are observable chain uses.
fn terminator_used_values(function: &Function) -> HashSet<ValueId> {
    function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .flat_map(super::liveness::terminator_uses)
        .collect()
}

/// Replaces the cast with the fused scalar opcode and erases proven boxed scaffolding.
fn apply_rewrite(function: &mut Function, sink: InstId, rewrite: ChainRewrite) {
    for instruction in rewrite.neutralize {
        if let Some(inst) = function.instruction_mut(instruction) {
            neutralize_to_nop(inst);
        }
    }
    let result = function.instruction(sink).and_then(|inst| inst.result);
    if let Some(inst) = function.instruction_mut(sink) {
        inst.op = Op::ICheckedNumericChainToInt;
        inst.operands = rewrite.operands;
        inst.immediate = Some(Immediate::CheckedNumericChain(Box::new(
            CheckedNumericChainImmediate::new(rewrite.operations),
        )));
        inst.result_type = IrType::I64;
        inst.result_php_type = PhpType::Int;
        inst.result_ownership = Ownership::NonHeap;
        inst.effects = Op::ICheckedNumericChainToInt.default_effects();
    }
    if let Some(result) = result.and_then(|value| function.value_mut(value)) {
        result.ir_type = IrType::I64;
        result.php_type = PhpType::Int;
        result.ownership = Ownership::NonHeap;
    }
}
