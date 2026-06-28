//! Purpose:
//! Common-subexpression elimination over EIR: removes a pure computation whose
//! identical predecessor is already available, redirecting its uses to that
//! earlier value. Covers both per-block redundancy and dominance-aware
//! cross-block redundancy in one dominator-tree value-numbering traversal.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Dominator-tree scoped value numbering: a scoped hash table maps each pure
//!   instruction's key `(op, result type, immediate, canonicalized operands)` to
//!   the value that first computed it. Visiting blocks in dominator-tree preorder
//!   means the table holds exactly the definitions that dominate the current
//!   block (its own earlier instructions plus those of dominating blocks). A
//!   match is therefore an earlier value that dominates the current instruction,
//!   so redirecting the redundant result to it (RAUW) is dominance-safe; the
//!   redundant instruction is neutralized to `nop`. Entries inserted by a block
//!   are removed when its whole subtree is done.
//! - Only **pure** (`Effects::PURE`) instructions that have at least one operand
//!   and whose result is `NonHeap` or `Persistent` are eligible. Purity
//!   guarantees the value is a function of its operands alone (no memory/state
//!   dependence, no fault), and the ownership restriction keeps the rewrite
//!   refcount-neutral — those results carry no owned-heap cleanup, exactly like
//!   the values dead-instruction elimination is allowed to drop. SSA operands are
//!   equal-by-value, so identical pure ops on identical operand values compute
//!   identical results. Nullary pure ops — constant and data-address
//!   materializations (`const_*`, `data_addr`) — are deliberately *not* CSE'd:
//!   they are cheaper to rematerialize at each use than to keep live, and sharing
//!   one across calls would force it into a callee-saved register or spill slot
//!   for the whole span with no work removed. CSE therefore only deduplicates
//!   computations.
//! - Functions with exception handlers are skipped: their handler blocks are
//!   reachable through implicit edges absent from the terminator graph, so a
//!   terminator-graph dominator can be bypassed at runtime by a throw, making
//!   cross-block redirection unsound (the same restriction `branch_simplify`
//!   uses).

use std::collections::HashMap;

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, IrType, Op, Ownership, ValueId,
};

use super::cfg::has_exception_handlers;
use super::dominance::compute_dominance;
use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop, replace_all_uses, resolve_chains};

/// Common-subexpression elimination pass. See the module docs for the model.
pub struct Cse;

/// Hashable, equality-correct encoding of an instruction immediate for the value
/// key. CSE-eligible ops (pure, with operands) only ever carry a comparison
/// predicate or no immediate, so the `Debug` form is a sound, unique encoding;
/// it also distinguishes float spellings such as `0.0` and `-0.0` should a
/// float-immediate op ever become eligible.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ImmKey {
    None,
    Repr(String),
}

/// Canonical encoding of one operand in the value key.
///
/// A non-constant operand is keyed by its SSA representative `ValueId` (equal
/// values share one id after redirection). A *constant* operand — one defined by a
/// nullary pure materialization (`const_i64 1`, `data_addr X`, …) — is keyed by its
/// value instead, so two distinct `const_i64 1` instructions compare equal. Without
/// this, `($n + 1) * ($n + 1)` keeps two separate `const_i64 1` operands, the two
/// `iadd`s get different keys, and CSE cannot collapse the repeated `$n + 1`.
/// Nullary constants are still never CSE'd as instructions (each use rematerializes
/// its own); this only unifies them when they appear as *operands* of a real
/// computation.
#[derive(Clone, PartialEq, Eq, Hash)]
enum OperandKey {
    Value(ValueId),
    Const {
        op: Op,
        ir_type: IrType,
        immediate: ImmKey,
    },
}

/// Value-numbering key: two pure instructions with equal keys compute the same
/// value when one dominates the other.
///
/// The PHP type is part of the key, not just the IR storage type: the same
/// `(op, immediate, operands)` can carry different PHP type metadata over the
/// same `I64` storage (for instance a `const_i64 0` used as a plain integer
/// versus a null-resource sentinel), and downstream lowering dispatches on that
/// PHP type. `PhpType` is not `Hash`, so its `Debug` form (which uniquely encodes
/// each type) is used.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    op: Op,
    result_type: IrType,
    php_type: String,
    ownership: Ownership,
    immediate: ImmKey,
    operands: Vec<OperandKey>,
}

impl IrPass for Cse {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "cse"
    }

    /// Eliminates redundant pure computations in one function, returning true on
    /// change. The literal pool is unused: the pass only redirects uses and
    /// neutralizes instructions, never materializing a new constant.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        if has_exception_handlers(function) {
            return false;
        }
        let dominance = compute_dominance(function);

        // Scoped value table over the dominator tree, plus the rewrites to apply.
        let mut table: HashMap<Key, ValueId> = HashMap::new();
        let mut rauw: HashMap<ValueId, ValueId> = HashMap::new();
        let mut redundant: Vec<InstId> = Vec::new();

        // Iterative dominator-tree DFS. `Leave` events run after a node's whole
        // subtree, removing the keys that node introduced so they stay scoped to
        // the region they dominate.
        let mut work: Vec<Event> = vec![Event::Enter(function.entry)];
        while let Some(event) = work.pop() {
            match event {
                Event::Leave(keys) => {
                    for key in keys {
                        table.remove(&key);
                    }
                }
                Event::Enter(block) => {
                    let inserted =
                        visit_block(function, block, &mut table, &mut rauw, &mut redundant);
                    work.push(Event::Leave(inserted));
                    for &child in dominance.children(block) {
                        work.push(Event::Enter(child));
                    }
                }
            }
        }

        if redundant.is_empty() {
            return false;
        }

        // Commit: neutralize the redundant instructions, then redirect every use
        // of their results to the surviving (dominating) values.
        let resolved = resolve_chains(&rauw);
        for inst_id in &redundant {
            if let Some(inst) = function.instruction_mut(*inst_id) {
                neutralize_to_nop(inst);
            }
        }
        replace_all_uses(function, &resolved);
        true
    }
}

/// A node of the dominator-tree traversal: enter a block, or leave it (after its
/// subtree) and drop the value-table keys it introduced.
enum Event {
    Enter(BlockId),
    Leave(Vec<Key>),
}

/// Processes one block's instructions in program order against the scoped value
/// table. Records a RAUW + neutralization for each redundant pure instruction and
/// returns the keys this block newly inserted (to be removed when its subtree is
/// done).
fn visit_block(
    function: &Function,
    block: BlockId,
    table: &mut HashMap<Key, ValueId>,
    rauw: &mut HashMap<ValueId, ValueId>,
    redundant: &mut Vec<InstId>,
) -> Vec<Key> {
    let mut inserted: Vec<Key> = Vec::new();
    let instructions = match function.block(block) {
        Some(block) => block.instructions.clone(),
        None => return inserted,
    };
    for inst_id in instructions {
        let Some(inst) = function.instruction(inst_id) else {
            continue;
        };
        let Some(result) = inst.result else {
            continue;
        };
        if inst.op == Op::Nop || !inst.effects.is_pure() {
            continue;
        }
        // Nullary pure ops are constant/address materializations (`const_*`,
        // `data_addr`). They are cheaper to rematerialize at each use than to keep
        // live, so CSE-ing them only lengthens live ranges (a single constant
        // shared across calls would have to occupy a callee-saved register or a
        // spill slot for the whole span) without removing real work. They are left
        // for the backend to rematerialize; CSE only deduplicates computations,
        // which always have operands.
        if inst.operands.is_empty() {
            continue;
        }
        if !matches!(inst.result_ownership, Ownership::NonHeap | Ownership::Persistent) {
            continue;
        }
        let key = make_key(function, inst, rauw);
        match table.get(&key) {
            Some(&existing) => {
                // An identical value already computed in a dominating position.
                rauw.insert(result, existing);
                redundant.push(inst_id);
            }
            None => {
                table.insert(key.clone(), result);
                inserted.push(key);
            }
        }
    }
    inserted
}

/// Builds the value-numbering key for a pure instruction, canonicalizing each
/// operand through the redirection map (and unifying constant operands by value) so
/// equal operand values share one representative.
fn make_key(
    function: &Function,
    inst: &crate::ir::Instruction,
    rauw: &HashMap<ValueId, ValueId>,
) -> Key {
    let operands = inst
        .operands
        .iter()
        .map(|&value| canon_operand(function, rauw, value))
        .collect();
    Key {
        op: inst.op,
        result_type: inst.result_type,
        php_type: format!("{:?}", inst.result_php_type),
        ownership: inst.result_ownership,
        immediate: immediate_key(inst.immediate.as_ref()),
        operands,
    }
}

/// Canonicalizes an operand for the value key: a constant operand (defined by a
/// nullary pure materialization) is keyed by its `(op, type, immediate)` value, so
/// two distinct constants of the same value unify; any other operand is keyed by its
/// SSA representative. Purity is required so impure nullary defs (e.g. `load_local`,
/// whose value depends on slot state) are never treated as value-equal.
fn canon_operand(
    function: &Function,
    rauw: &HashMap<ValueId, ValueId>,
    value: ValueId,
) -> OperandKey {
    let repr = canon(rauw, value);
    if let Some(def) = defining_instruction(function, repr) {
        if def.operands.is_empty() && def.effects.is_pure() {
            return OperandKey::Const {
                op: def.op,
                ir_type: def.result_type,
                immediate: immediate_key(def.immediate.as_ref()),
            };
        }
    }
    OperandKey::Value(repr)
}

/// Follows the redirection chain to a value's current representative. Chains are
/// short (representatives are never redirected), and the length guard is a
/// defensive backstop.
fn canon(rauw: &HashMap<ValueId, ValueId>, mut value: ValueId) -> ValueId {
    let mut steps = 0;
    while let Some(&next) = rauw.get(&value) {
        value = next;
        steps += 1;
        if steps > rauw.len() {
            break;
        }
    }
    value
}

/// Encodes an instruction immediate into the hashable, equality-correct key form.
fn immediate_key(immediate: Option<&Immediate>) -> ImmKey {
    match immediate {
        None => ImmKey::None,
        Some(other) => ImmKey::Repr(format!("{:?}", other)),
    }
}
