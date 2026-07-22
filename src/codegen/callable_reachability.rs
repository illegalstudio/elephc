//! Purpose:
//! Computes finite runtime string-callable target sets from EIR value and local provenance.
//! Keeps descriptor emission demand-driven without narrowing genuinely open runtime strings.
//!
//! Called from:
//! - `crate::codegen::context::FunctionContext::new()` before assembly lowering.
//!
//! Key details:
//! - Local string values are propagated through the EIR CFG to a fixed point.
//! - Control-flow joins union finite name sets; unknown inputs keep the result open.
//! - Ref-cell and eval writes invalidate affected local facts conservatively.

use std::collections::{BTreeSet, VecDeque};

use crate::ir::{
    BlockId, Function, Immediate, LocalSlotId, Module, Op, Terminator, ValueId,
};

const MAX_FINITE_CALLABLE_NAMES: usize = 32;

/// Finite callable-name provenance for one EIR value or local slot.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CallableNameSet {
    Unknown,
    Known(BTreeSet<String>),
}

impl CallableNameSet {
    /// Creates a canonical singleton target set for one PHP callable name.
    fn singleton(name: &str) -> Self {
        Self::Known(BTreeSet::from([canonical_callable_name(name)]))
    }

    /// Merges an incoming CFG fact and reports whether the receiver changed.
    fn merge_from(&mut self, incoming: &Self) -> bool {
        let merged = match (&*self, incoming) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Known(left), Self::Known(right)) => {
                let mut names = left.clone();
                names.extend(right.iter().cloned());
                if names.len() > MAX_FINITE_CALLABLE_NAMES {
                    Self::Unknown
                } else {
                    Self::Known(names)
                }
            }
        };
        if *self == merged {
            return false;
        }
        *self = merged;
        true
    }

    /// Returns the sorted finite names, or `None` when runtime input stays open.
    fn finite_names(&self) -> Option<Vec<String>> {
        match self {
            Self::Unknown => None,
            Self::Known(names) => Some(names.iter().cloned().collect()),
        }
    }
}

/// Per-function finite string-callable facts indexed by EIR value id.
pub(super) struct CallableReachabilityAnalysis {
    value_names: Vec<CallableNameSet>,
}

impl CallableReachabilityAnalysis {
    /// Runs the forward local-value analysis for one lowered EIR function.
    pub(super) fn new(module: &Module, function: &Function) -> Self {
        let value_names = analyze_value_names(module, function);
        Self { value_names }
    }

    /// Returns a finite candidate set for `value`, or `None` for an open runtime string.
    pub(super) fn candidates(&self, value: ValueId) -> Option<Vec<String>> {
        self.value_names
            .get(value.as_raw() as usize)
            .and_then(CallableNameSet::finite_names)
    }
}

/// Propagates local callable-name facts across the EIR CFG and records SSA results.
fn analyze_value_names(module: &Module, function: &Function) -> Vec<CallableNameSet> {
    let mut value_names = vec![CallableNameSet::Unknown; function.values.len()];
    if function.blocks.is_empty() {
        return value_names;
    }

    let unknown_locals = vec![CallableNameSet::Unknown; function.locals.len()];
    let mut block_inputs = vec![None::<Vec<CallableNameSet>>; function.blocks.len()];
    let entry_index = function.entry.as_raw() as usize;
    if entry_index >= block_inputs.len() {
        return value_names;
    }
    block_inputs[entry_index] = Some(unknown_locals.clone());
    let mut worklist = VecDeque::from([function.entry]);

    for handler in exception_handler_blocks(function) {
        let index = handler.as_raw() as usize;
        if index >= block_inputs.len() {
            continue;
        }
        block_inputs[index] = Some(unknown_locals.clone());
        if handler != function.entry {
            worklist.push_back(handler);
        }
    }

    let mut block_outputs = vec![None::<Vec<CallableNameSet>>; function.blocks.len()];
    while let Some(block_id) = worklist.pop_front() {
        let block_index = block_id.as_raw() as usize;
        let Some(mut state) = block_inputs[block_index].clone() else {
            continue;
        };
        let block = &function.blocks[block_index];
        scan_block(module, function, block, &mut state, &mut value_names);
        if block_outputs[block_index].as_ref() == Some(&state) {
            continue;
        }
        block_outputs[block_index] = Some(state.clone());
        let Some(terminator) = block.terminator.as_ref() else {
            continue;
        };
        for successor in terminator_successors(terminator) {
            let successor_index = successor.as_raw() as usize;
            if successor_index >= block_inputs.len() {
                continue;
            }
            let changed = match &mut block_inputs[successor_index] {
                Some(input) => merge_local_states(input, &state),
                None => {
                    block_inputs[successor_index] = Some(state.clone());
                    true
                }
            };
            if changed {
                worklist.push_back(successor);
            }
        }
    }

    // Re-scan stable block inputs so every recorded SSA fact reflects the final join state.
    for block in &function.blocks {
        let Some(mut state) = block_inputs[block.id.as_raw() as usize].clone() else {
            continue;
        };
        scan_block(module, function, block, &mut state, &mut value_names);
    }
    value_names
}

/// Applies every instruction in one block to the local and SSA callable-name states.
fn scan_block(
    module: &Module,
    function: &Function,
    block: &crate::ir::BasicBlock,
    state: &mut [CallableNameSet],
    value_names: &mut [CallableNameSet],
) {
    for inst_id in &block.instructions {
        let Some(inst) = function.instruction(*inst_id) else {
            continue;
        };
        let result_names = instruction_result_names(module, inst, state, value_names);
        if let Some(result) = inst.result {
            if let Some(slot) = value_names.get_mut(result.as_raw() as usize) {
                *slot = result_names;
            }
        }
        apply_local_transfer(inst, state, value_names);
    }
}

/// Computes callable-name provenance for one instruction result.
fn instruction_result_names(
    module: &Module,
    inst: &crate::ir::Instruction,
    state: &[CallableNameSet],
    value_names: &[CallableNameSet],
) -> CallableNameSet {
    match inst.op {
        Op::ConstStr => inst
            .immediate
            .as_ref()
            .and_then(|immediate| match immediate {
                Immediate::Data(data) => module.data.strings.get(data.as_raw() as usize),
                _ => None,
            })
            .map_or(CallableNameSet::Unknown, |name| {
                CallableNameSet::singleton(name)
            }),
        Op::LoadLocal => inst
            .immediate
            .as_ref()
            .and_then(|immediate| match immediate {
                Immediate::LocalSlot(slot) => state.get(slot.as_raw() as usize),
                _ => None,
            })
            .cloned()
            .unwrap_or(CallableNameSet::Unknown),
        Op::Acquire
        | Op::Borrow
        | Op::Move
        | Op::EnsureOwned
        | Op::StrPersist
        | Op::MixedBox
        | Op::MixedUnbox => inst
            .operands
            .first()
            .and_then(|value| value_names.get(value.as_raw() as usize))
            .cloned()
            .unwrap_or(CallableNameSet::Unknown),
        _ => CallableNameSet::Unknown,
    }
}

/// Updates local facts after an instruction that writes or invalidates local storage.
fn apply_local_transfer(
    inst: &crate::ir::Instruction,
    state: &mut [CallableNameSet],
    value_names: &[CallableNameSet],
) {
    match (inst.op, inst.immediate.as_ref()) {
        (Op::StoreLocal, Some(Immediate::LocalSlot(slot))) => {
            let names = inst
                .operands
                .first()
                .and_then(|value| value_names.get(value.as_raw() as usize))
                .cloned()
                .unwrap_or(CallableNameSet::Unknown);
            set_local_state(state, *slot, names);
        }
        (
            Op::UnsetLocal | Op::BindRefCellPtr | Op::IterCurrentValueRef,
            Some(Immediate::LocalSlot(slot)),
        ) => {
            set_local_state(state, *slot, CallableNameSet::Unknown);
        }
        (
            Op::PromoteLocalRefCell | Op::AliasLocalRefCell,
            Some(Immediate::LocalSlotPair { first, second }),
        ) => {
            set_local_state(state, *first, CallableNameSet::Unknown);
            set_local_state(state, *second, CallableNameSet::Unknown);
        }
        (Op::EvalLiteralCall | Op::EvalScopeSet | Op::ListUnpack, _) => {
            state.fill(CallableNameSet::Unknown);
        }
        _ => {}
    }
}

/// Replaces one local fact when the referenced slot exists.
fn set_local_state(state: &mut [CallableNameSet], slot: LocalSlotId, names: CallableNameSet) {
    if let Some(local) = state.get_mut(slot.as_raw() as usize) {
        *local = names;
    }
}

/// Joins a predecessor's local facts into a successor input state.
fn merge_local_states(target: &mut [CallableNameSet], incoming: &[CallableNameSet]) -> bool {
    target
        .iter_mut()
        .zip(incoming)
        .fold(false, |changed, (target, incoming)| {
            target.merge_from(incoming) || changed
        })
}

/// Returns blocks entered through implicit exception-handler control-flow edges.
fn exception_handler_blocks(function: &Function) -> BTreeSet<BlockId> {
    function
        .instructions
        .iter()
        .filter_map(|inst| {
            if inst.op != Op::TryPushHandler {
                return None;
            }
            let Some(Immediate::I64(raw)) = inst.immediate else {
                return None;
            };
            u32::try_from(raw).ok().map(BlockId::from_raw)
        })
        .collect()
}

/// Returns the explicit CFG successors for one EIR terminator.
fn terminator_successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Br { target, .. } => vec![*target],
        Terminator::CondBr {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut successors = cases.iter().map(|case| case.target).collect::<Vec<_>>();
            successors.push(*default);
            successors
        }
        Terminator::GeneratorSuspend { resume, .. } => vec![*resume],
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Canonicalizes a PHP callable name for case-insensitive candidate matching.
fn canonical_callable_name(name: &str) -> String {
    crate::names::php_symbol_key(name.trim_start_matches('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::ir::{Builder, IrType, LocalKind, Ownership};
    use crate::types::PhpType;

    /// Verifies branch-local string stores form a finite candidate union at the CFG join.
    #[test]
    fn branch_join_collects_finite_callable_names() {
        let mut module = Module::new(Target::detect_host());
        let upper_name = module.data.intern_string("STRTOUPPER");
        let lower_name = module.data.intern_string("strtolower");
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        let callback = function.add_local(
            Some("callback".to_string()),
            IrType::Str,
            PhpType::Str,
            LocalKind::PhpLocal,
        );
        let loaded;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let then_block = builder.create_named_block("then", Vec::new());
            let else_block = builder.create_named_block("else", Vec::new());
            let merge = builder.create_named_block("merge", Vec::new());
            builder.set_entry(entry);

            builder.position_at_end(entry);
            let cond = builder.emit_const_bool(true);
            builder.terminate(Terminator::CondBr {
                cond,
                then_target: then_block,
                then_args: Vec::new(),
                else_target: else_block,
                else_args: Vec::new(),
            });

            builder.position_at_end(then_block);
            let upper = builder
                .emit(
                    Op::ConstStr,
                    Vec::new(),
                    Some(Immediate::Data(upper_name)),
                    IrType::Str,
                    PhpType::Str,
                    Ownership::MaybeOwned,
                )
                .expect("const string produces a value");
            builder.emit(
                Op::StoreLocal,
                vec![upper],
                Some(Immediate::LocalSlot(callback)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Br {
                target: merge,
                args: Vec::new(),
            });

            builder.position_at_end(else_block);
            let lower = builder
                .emit(
                    Op::ConstStr,
                    Vec::new(),
                    Some(Immediate::Data(lower_name)),
                    IrType::Str,
                    PhpType::Str,
                    Ownership::MaybeOwned,
                )
                .expect("const string produces a value");
            builder.emit(
                Op::StoreLocal,
                vec![lower],
                Some(Immediate::LocalSlot(callback)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Br {
                target: merge,
                args: Vec::new(),
            });

            builder.position_at_end(merge);
            loaded = builder
                .emit(
                    Op::LoadLocal,
                    Vec::new(),
                    Some(Immediate::LocalSlot(callback)),
                    IrType::Str,
                    PhpType::Str,
                    Ownership::Borrowed,
                )
                .expect("load local produces a value");
            builder.terminate(Terminator::Return { value: None });
        }

        let analysis = CallableReachabilityAnalysis::new(&module, &function);
        assert_eq!(
            analysis.candidates(loaded),
            Some(vec!["strtolower".to_string(), "strtoupper".to_string()])
        );
    }
}
