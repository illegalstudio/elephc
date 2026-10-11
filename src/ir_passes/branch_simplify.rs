//! Purpose:
//! Branch simplification over EIR functions: fold constant-condition `CondBr`
//! and `Switch` terminators to unconditional `Br`, thread empty forwarding
//! blocks with SSA argument composition, neutralize unreachable blocks, and merge
//! single-predecessor update blocks within natural loops.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Unreachable blocks are neutralized in place (terminator set to
//!   `Unreachable`, instructions rewritten to `nop`) rather than physically
//!   removed. Clearing its uses avoids retaining dead SSA dependencies and
//!   keeps same-block ordering plus sibling dead-block provenance trivially valid,
//!   while preserving `block.id == index` and every `ValueDef`/`ValueId`/`InstId`
//!   slot. No renumbering occurs, and `try` handler tokens (block ids encoded in
//!   `try_push_handler` immediates) stay correct.
//! - Functions containing exception-handling ops are skipped entirely: their
//!   handler blocks are reachable through implicit edges not present in the
//!   terminator graph, so terminator-only reachability could wrongly neutralize
//!   a live handler.
//! - Forwarding parameters may be used only by their outgoing branch. Composing
//!   each incoming edge preserves parallel argument assignment; escaping definitions
//!   keep their block. Loop merging rewrites parameters before relocating definitions.

use std::collections::{HashMap, HashSet};

use crate::ir::{BlockId, DataPool, Function, Immediate, Op, Terminator, ValueId};

use super::cfg::{has_exception_handlers, successor_edges, successors};
use super::driver::IrPass;
use super::rewrite::{count_value_uses, defining_instruction, neutralize_to_nop};

/// CFG branch simplification pass.
pub struct BranchSimplify;

impl IrPass for BranchSimplify {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "branch-simplify"
    }

    /// Folds constant branches, threads empty blocks, and neutralizes unreachable
    /// blocks, reporting whether the function changed. The literal pool is unused
    /// because the pass never materializes new constants.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function) {
            return false;
        }
        let mut changed = false;
        changed |= fold_constant_terminators(function);
        changed |= thread_empty_forwarding_blocks(function);
        changed |= neutralize_unreachable_blocks(function);
        changed |= super::loop_cfg::merge_loop_blocks(function);
        changed
    }
}

/// Resolves a branch condition value to a compile-time truthiness, if known.
///
/// Recognizes the constant-producing opcodes a folded condition can reduce to:
/// `const_bool`, `const_i64` (PHP truthiness: non-zero is true), and
/// `const_null` (always false). Returns `None` for any runtime-dependent value.
fn const_truthiness(function: &Function, value: ValueId) -> Option<bool> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, inst.immediate.as_ref()) {
        (Op::ConstBool, Some(Immediate::Bool(b))) => Some(*b),
        (Op::ConstI64, Some(Immediate::I64(n))) => Some(*n != 0),
        (Op::ConstNull, _) => Some(false),
        _ => None,
    }
}

/// Resolves a switch scrutinee to a compile-time integer, if known.
fn const_int(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, inst.immediate.as_ref()) {
        (Op::ConstI64, Some(Immediate::I64(n))) => Some(*n),
        (Op::ConstBool, Some(Immediate::Bool(b))) => Some(*b as i64),
        _ => None,
    }
}

/// Folds `CondBr`/`Switch` terminators whose selector is a compile-time constant
/// into an unconditional `Br` to the taken edge. Returns whether any terminator
/// changed.
fn fold_constant_terminators(function: &mut Function) -> bool {
    let mut changed = false;
    for index in 0..function.blocks.len() {
        let Some(term) = function.blocks[index].terminator.clone() else {
            continue;
        };
        let folded = match term {
            Terminator::CondBr {
                cond,
                then_target,
                then_args,
                else_target,
                else_args,
            } => const_truthiness(function, cond).map(|taken| {
                if taken {
                    Terminator::Br {
                        target: then_target,
                        args: then_args,
                    }
                } else {
                    Terminator::Br {
                        target: else_target,
                        args: else_args,
                    }
                }
            }),
            Terminator::Switch {
                scrutinee,
                cases,
                default,
                default_args,
            } => const_int(function, scrutinee).map(|value| {
                match cases.into_iter().find(|case| case.value == value) {
                    Some(case) => Terminator::Br {
                        target: case.target,
                        args: case.args,
                    },
                    None => Terminator::Br {
                        target: default,
                        args: default_args,
                    },
                }
            }),
            _ => None,
        };
        if let Some(new_term) = folded {
            function.blocks[index].terminator = Some(new_term);
            changed = true;
        }
    }
    changed
}

/// Threads empty blocks by composing each incoming edge's SSA arguments independently.
/// Parameters may only be used by the forwarding terminator: escaping definitions
/// must remain available to their dominated users. Cycles are left unchanged.
fn thread_empty_forwarding_blocks(function: &mut Function) -> bool {
    let uses = count_value_uses(function);
    let mut forwards = HashMap::new();
    for block in &function.blocks {
        if block.id == function.entry { continue; }
        let Some(Terminator::Br { target, args }) = &block.terminator else { continue; };
        if *target == block.id { continue; }
        if !block.instructions.iter().all(|id| function.instruction(*id).is_some_and(|inst| {
            inst.op == Op::Nop && inst.result.is_none_or(|value| uses.get(&value).copied().unwrap_or(0) == 0)
        })) { continue; }
        if block.params.iter().any(|param| {
            uses.get(param).copied().unwrap_or(0) != args.iter().filter(|arg| *arg == param).count()
        }) { continue; }
        forwards.insert(block.id, (block.params.clone(), *target, args.clone()));
    }
    let mut changed = false;
    for block in &mut function.blocks {
        let Some(term) = &mut block.terminator else { continue; };
        for (target, args) in successor_edges(term) {
            let (mut next, mut values) = (*target, args.clone());
            let mut seen = HashSet::new();
            while let Some((params, destination, outgoing)) = forwards.get(&next) {
                if !seen.insert(next) {
                    next = *target;
                    values = args.clone();
                    break;
                }
                let replacements: HashMap<_, _> = params.iter().copied().zip(values).collect();
                values = outgoing.iter().map(|value| replacements.get(value).copied().unwrap_or(*value)).collect();
                next = *destination;
            }
            if next != *target || values != *args {
                *target = next;
                *args = values;
                changed = true;
            }
        }
    }
    changed
}

/// Neutralizes every block unreachable from the entry: its terminator becomes
/// `Unreachable` and its instructions become `nop`, clearing all value uses so
/// the function stays valid without renumbering. Returns whether any block was
/// neutralized this run (blocks already in neutral form are left untouched so
/// the pass converges).
fn neutralize_unreachable_blocks(function: &mut Function) -> bool {
    let reachable = reachable_blocks(function);
    let mut changed = false;
    for index in 0..function.blocks.len() {
        let block_id = function.blocks[index].id;
        if reachable.contains(&block_id) {
            continue;
        }
        if block_is_neutralized(function, index) {
            continue;
        }
        let inst_ids = function.blocks[index].instructions.clone();
        for inst_id in inst_ids {
            if let Some(inst) = function.instruction_mut(inst_id) {
                neutralize_to_nop(inst);
            }
        }
        function.blocks[index].terminator = Some(Terminator::Unreachable);
        changed = true;
    }
    changed
}

/// Returns true when a block is already in neutralized form (an `Unreachable`
/// terminator with every instruction a `nop`).
fn block_is_neutralized(function: &Function, index: usize) -> bool {
    let block = &function.blocks[index];
    if !matches!(block.terminator, Some(Terminator::Unreachable)) {
        return false;
    }
    block
        .instructions
        .iter()
        .all(|inst_id| function.instruction(*inst_id).map(|inst| inst.op == Op::Nop).unwrap_or(true))
}

/// Computes the set of blocks reachable from the entry via terminator edges.
///
/// Functions with exception handlers (which add implicit handler edges) are
/// filtered out before this runs, so terminator successors are the complete
/// edge set here.
fn reachable_blocks(function: &Function) -> HashSet<BlockId> {
    let mut reachable: HashSet<BlockId> = HashSet::new();
    let mut stack = vec![function.entry];
    while let Some(block_id) = stack.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        if let Some(block) = function.block(block_id) {
            if let Some(term) = block.terminator.as_ref() {
                for succ in successors(term) {
                    if !reachable.contains(&succ) {
                        stack.push(succ);
                    }
                }
            }
        }
    }
    reachable
}
