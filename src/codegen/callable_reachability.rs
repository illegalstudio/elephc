//! Purpose:
//! Computes finite runtime string-callable target sets from EIR value and local provenance.
//! Keeps descriptor emission demand-driven without narrowing genuinely open runtime strings.
//!
//! Called from:
//! - `crate::codegen::context::FunctionContext::runtime_callable_candidates()` during lowering.
//!
//! Key details:
//! - Only finite local name sets are stored during CFG propagation.
//! - Control-flow joins union finite name sets; unknown inputs keep the result open.
//! - Ref-cell and eval writes invalidate affected local facts conservatively.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ir::control_flow::explicit_successors;
use crate::ir::{BlockId, Function, Immediate, LocalSlotId, Module, Op, ValueId};

const MAX_FINITE_CALLABLE_NAMES: usize = 32;

/// Absent slots have an open name set, while a missing whole state is unreachable.
type LocalNameState = BTreeMap<LocalSlotId, BTreeSet<String>>;

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

    /// Returns the sorted finite names, or `None` when runtime input stays open.
    fn finite_names(&self) -> Option<Vec<String>> {
        match self {
            Self::Unknown => None,
            Self::Known(names) => Some(names.iter().cloned().collect()),
        }
    }
}

/// Per-function finite string-callable facts indexed by EIR value id.
pub(crate) struct CallableReachabilityAnalysis {
    value_names: Vec<CallableNameSet>,
}

impl CallableReachabilityAnalysis {
    /// Runs the forward local-value analysis for one lowered EIR function.
    pub(crate) fn new(module: &Module, function: &Function) -> Self {
        let value_names = analyze_value_names(module, function);
        Self { value_names }
    }

    /// Returns a finite candidate set for `value`, or `None` for an open runtime string.
    pub(crate) fn candidates(&self, value: ValueId) -> Option<Vec<String>> {
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

    let mut block_inputs = vec![None::<LocalNameState>; function.blocks.len()];
    let entry_index = function.entry.as_raw() as usize;
    if entry_index >= block_inputs.len() {
        return value_names;
    }
    block_inputs[entry_index] = Some(LocalNameState::new());
    let mut worklist = VecDeque::from([function.entry]);

    for handler in exception_handler_blocks(function) {
        let index = handler.as_raw() as usize;
        if index >= block_inputs.len() {
            continue;
        }
        block_inputs[index] = Some(LocalNameState::new());
        if handler != function.entry {
            worklist.push_back(handler);
        }
    }

    let mut block_outputs = vec![None::<LocalNameState>; function.blocks.len()];
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
        for successor in explicit_successors(terminator) {
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
    state: &mut LocalNameState,
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
        apply_local_transfer(inst, state, value_names, function.locals.len());
    }
}

/// Computes callable-name provenance for one instruction result.
fn instruction_result_names(
    module: &Module,
    inst: &crate::ir::Instruction,
    state: &LocalNameState,
    value_names: &[CallableNameSet],
) -> CallableNameSet {
    match inst.op {
        Op::ConstStr | Op::ClosureNew => inst
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
                Immediate::LocalSlot(slot) => state.get(slot),
                _ => None,
            })
            .map_or(CallableNameSet::Unknown, |names| {
                CallableNameSet::Known(names.clone())
            }),
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
    state: &mut LocalNameState,
    value_names: &[CallableNameSet],
    local_count: usize,
) {
    match (inst.op, inst.immediate.as_ref()) {
        (Op::StoreLocal | Op::StoreLocalRetiringBox, Some(Immediate::LocalSlot(slot)))
            if (slot.as_raw() as usize) < local_count =>
        {
            let names = inst
                .operands
                .first()
                .and_then(|value| value_names.get(value.as_raw() as usize));
            if let Some(CallableNameSet::Known(names)) = names {
                state.insert(*slot, names.clone());
            } else {
                state.remove(slot);
            }
        }
        (
            Op::UnsetLocal | Op::BindRefCellPtr | Op::IterCurrentValueRef,
            Some(Immediate::LocalSlot(slot)),
        ) => {
            state.remove(slot);
        }
        (
            Op::PromoteLocalRefCell,
            Some(Immediate::LocalSlotPair { first, second })
            | Some(Immediate::PropertyRefCell { first, second, .. }),
        ) => {
            state.remove(first);
            state.remove(second);
        }
        (
            Op::AliasLocalRefCell | Op::RetainLocalRefCell | Op::BindRefCellPtr | Op::AdoptRefCellPtr,
            Some(Immediate::LocalSlotPair { first, second }),
        ) => {
            state.remove(first);
            state.remove(second);
        }
        (Op::EvalLiteralCall | Op::EvalScopeSet | Op::ListUnpack, _) => {
            state.clear();
        }
        _ => {}
    }
}

/// Joins finite names only when every incoming path has a finite fact for the slot.
fn merge_local_states(target: &mut LocalNameState, incoming: &LocalNameState) -> bool {
    let mut changed = false;
    target.retain(|slot, names| {
        let Some(other) = incoming.get(slot) else {
            changed = true;
            return false;
        };
        if !other.is_subset(names) {
            names.extend(other.iter().cloned());
            changed = true;
        }
        names.len() <= MAX_FINITE_CALLABLE_NAMES
    });
    changed
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

/// Canonicalizes a PHP callable name for case-insensitive candidate matching.
fn canonical_callable_name(name: &str) -> String {
    crate::names::php_symbol_key(name.trim_start_matches('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::ir::{Builder, IrType, LocalKind, Ownership, Terminator};
    use crate::types::PhpType;

    /// Unknown inputs erase finite proofs and overflowing unions become open.
    #[test]
    fn sparse_join_keeps_only_bounded_names_known_on_every_path() {
        let missing = LocalSlotId::from_raw(7);
        let overflow = LocalSlotId::from_raw(65);
        let bounded = LocalSlotId::from_raw(70_000);
        let names = (0..MAX_FINITE_CALLABLE_NAMES)
            .map(|index| format!("callback{index}"))
            .collect();
        let mut target = LocalNameState::from([
            (missing, BTreeSet::from(["missing".into()])),
            (overflow, names),
            (bounded, BTreeSet::from(["toupper".into()])),
        ]);
        let incoming = LocalNameState::from([
            (overflow, BTreeSet::from(["another".into()])),
            (bounded, BTreeSet::from(["tolower".into()])),
        ]);
        assert!(merge_local_states(&mut target, &incoming));
        assert_eq!(
            target,
            LocalNameState::from([(bounded, BTreeSet::from(["tolower".into(), "toupper".into()]))]),
        );
        assert!(!merge_local_states(&mut target, &incoming));
    }

    /// Publishing a mutated hash cannot leave the prior local's callable name attached.
    #[test]
    fn retiring_box_store_invalidates_an_older_callable_name() {
        let slot = LocalSlotId::from_raw(0);
        let mut state = LocalNameState::from([(slot, BTreeSet::from(["callback".into()]))]);
        let store = crate::ir::Instruction::new(
            Op::StoreLocalRetiringBox,
            vec![ValueId::from_raw(0)],
            Some(Immediate::LocalSlot(slot)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::StoreLocalRetiringBox.default_effects(),
            None,
        );
        apply_local_transfer(&store, &mut state, &[CallableNameSet::Unknown], 1);
        assert!(state.is_empty());
    }

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
