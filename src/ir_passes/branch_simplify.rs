//! Purpose:
//! Branch simplification over EIR functions: fold constant-condition `CondBr`
//! and `Switch` terminators to unconditional `Br`, thread empty forwarding
//! blocks, and neutralize blocks that become unreachable.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Unreachable blocks are neutralized in place (terminator set to
//!   `Unreachable`, instructions rewritten to `nop`) rather than physically
//!   removed. The validator treats an unreachable block's value *uses* as
//!   `UseNotDominated`, so clearing all uses keeps the function valid while
//!   preserving `block.id == index` and every `ValueDef`/`ValueId`/`InstId`
//!   slot — no renumbering, and `try` handler tokens (block ids encoded in
//!   `try_push_handler` immediates) stay correct.
//! - Functions containing exception-handling ops are skipped entirely: their
//!   handler blocks are reachable through implicit edges not present in the
//!   terminator graph, so terminator-only reachability could wrongly neutralize
//!   a live handler.
//! - Removing edges only enlarges dominator sets, and threaded forwarding blocks
//!   carry no definitions, so simplification never invalidates a use that was
//!   valid before.

use std::collections::{HashMap, HashSet};

use crate::ir::{BlockId, DataPool, Function, Immediate, Op, Terminator, ValueId};

use super::cfg::successors;
use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop};

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
        changed
    }
}

/// Returns true when the function uses any exception-handling opcode.
///
/// Such functions have handler blocks reachable only through implicit edges
/// (a `try_push_handler` token names the handler block id), so terminator-graph
/// reachability is incomplete and the pass conservatively skips them.
fn has_exception_handlers(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        matches!(
            inst.op,
            Op::TryPushHandler
                | Op::TryPopHandler
                | Op::CatchCurrent
                | Op::CatchBind
                | Op::FinallyEnter
                | Op::FinallyExit
        )
    })
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

/// Threads predecessors through empty forwarding blocks.
///
/// A forwarding block is a non-entry block with no parameters, no real
/// instructions (only `nop`s), and an unconditional `Br` to a different block.
/// Edges targeting such a block are redirected to the end of the forwarding
/// chain. Because forwarding blocks have no parameters, every edge into them
/// carries empty arguments, so retargeting needs no argument rewriting. Returns
/// whether any edge changed.
fn thread_empty_forwarding_blocks(function: &mut Function) -> bool {
    let forwards = forwarding_targets(function);
    if forwards.is_empty() {
        return false;
    }

    let mut changed = false;
    for index in 0..function.blocks.len() {
        let Some(mut term) = function.blocks[index].terminator.clone() else {
            continue;
        };
        let mut edge_changed = false;
        for target in terminator_targets_mut(&mut term) {
            let resolved = resolve_forwarding(*target, &forwards);
            if resolved != *target {
                *target = resolved;
                edge_changed = true;
            }
        }
        if edge_changed {
            function.blocks[index].terminator = Some(term);
            changed = true;
        }
    }
    changed
}

/// Collects each empty forwarding block and the block its `Br` targets.
fn forwarding_targets(function: &Function) -> HashMap<BlockId, BlockId> {
    let mut forwards = HashMap::new();
    for block in &function.blocks {
        if block.id == function.entry || !block.params.is_empty() {
            continue;
        }
        let all_nop = block
            .instructions
            .iter()
            .all(|inst_id| function.instruction(*inst_id).map(|inst| inst.op == Op::Nop).unwrap_or(true));
        if !all_nop {
            continue;
        }
        if let Some(Terminator::Br { target, args }) = &block.terminator {
            if args.is_empty() && *target != block.id {
                forwards.insert(block.id, *target);
            }
        }
    }
    forwards
}

/// Follows a forwarding chain to its final target, stopping on a cycle.
fn resolve_forwarding(start: BlockId, forwards: &HashMap<BlockId, BlockId>) -> BlockId {
    let mut current = start;
    let mut seen: HashSet<BlockId> = HashSet::new();
    while let Some(&next) = forwards.get(&current) {
        if !seen.insert(current) {
            // Cycle of empty blocks: stop where we entered it.
            return start;
        }
        current = next;
    }
    current
}

/// Returns mutable references to every successor block id carried by a
/// terminator, so callers can retarget edges without rebuilding the terminator.
fn terminator_targets_mut(term: &mut Terminator) -> Vec<&mut BlockId> {
    match term {
        Terminator::Br { target, .. } => vec![target],
        Terminator::CondBr {
            then_target,
            else_target,
            ..
        } => vec![then_target, else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut targets: Vec<&mut BlockId> = cases.iter_mut().map(|case| &mut case.target).collect();
            targets.push(default);
            targets
        }
        Terminator::GeneratorSuspend { resume, .. } => vec![resume],
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
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
