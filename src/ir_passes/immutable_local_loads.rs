//! Purpose:
//! Classifies integer and float `load_local` instructions whose slot value cannot change.
//! Makes those reads pure so CSE and LICM can reason about local-backed arithmetic.
//!
//! Called from:
//! - The fixed-point pass driver before checked-int specialization, CSE, and LICM.
//!
//! Key details:
//! - Only concrete integer and float PHP locals are considered; aliases, ref cells, statics,
//!   hidden temporaries, mixed storage, and any slot named by an unknown operation fail closed.
//! - A slot is immutable when it is a read-only incoming parameter/main `$argc`, or has
//!   exactly one entry-block store that dominates every load. The entry must have no
//!   predecessor, proving that the store executes once rather than once per loop iteration.
//! - A SLOT PASSED BY REFERENCE IS EXCLUDED (`super::by_ref_alias`), because the callee
//!   writes it through the alias with no `store_local` in this function to count. Skipping
//!   that check made every `do { f($x); … } while ($n > 0);` whose `$n` is a by-reference
//!   out-parameter run its body exactly once: the load was called pure, so LICM hoisted the
//!   loop condition into the preheader.
//! - Dynamic eval can read or rewrite later-declared locals through the frame, so
//!   those functions cannot mark scalar loads immutable.

use std::collections::HashSet;

use crate::ir::{
    DataPool, Effects, Function, Immediate, InstId, Instruction, IrType, LocalKind, LocalSlotId, Op,
};
use crate::types::PhpType;

use super::cfg::predecessors;
use super::dominance::compute_dominance;
use super::driver::IrPass;

/// Marks loads from proven-immutable scalar integer and float locals as pure.
pub struct ImmutableLocalLoads;

impl IrPass for ImmutableLocalLoads {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "immutable_local_loads"
    }

    /// Skips functions without an effectful numeric local load whose arithmetic can use it.
    fn is_applicable(&self, function: &Function) -> bool {
        let consumers = NumericConsumers::of(function);
        function.instructions.iter().any(|inst| {
            actionable_numeric_load_slot(function, inst)
                .is_some_and(|slot| consumers.admits(function.locals[slot.as_raw() as usize].ir_type))
        })
    }

    /// Refines effects for every load whose local slot satisfies the immutable contract.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates = actionable_numeric_load_slots(function);
        if candidates.is_empty() {
            return false;
        }

        let eligible = immutable_numeric_slots(function, &candidates);
        if eligible.is_empty() {
            return false;
        }

        let mut changed = false;
        for inst in &mut function.instructions {
            if inst.op != Op::LoadLocal || inst.effects.is_pure() {
                continue;
            }
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                continue;
            };
            if eligible.contains(&slot) {
                inst.effects = Effects::PURE;
                changed = true;
            }
        }
        changed
    }
}

/// Which numeric kinds have arithmetic in a function that a pure local load can feed.
///
/// Integer loads are refined only next to checked integer arithmetic and float loads only next to
/// float arithmetic, so a function without such arithmetic (for example a generic identity body
/// that shape merging folds with its other instantiations) keeps its loads unchanged.
#[derive(Clone, Copy)]
struct NumericConsumers {
    /// The function contains checked integer add, subtract or multiply.
    checked_int: bool,
    /// The function contains float add, subtract, multiply or divide.
    float: bool,
}

impl NumericConsumers {
    /// Scans one function's instructions for the arithmetic that justifies each kind.
    fn of(function: &Function) -> Self {
        let has = |ops: &[Op]| function.instructions.iter().any(|inst| ops.contains(&inst.op));
        Self {
            checked_int: has(&[Op::ICheckedAdd, Op::ICheckedSub, Op::ICheckedMul]),
            float: has(&[Op::FAdd, Op::FSub, Op::FMul, Op::FDiv]),
        }
    }

    /// Returns whether loads of this storage kind are worth refining in the function.
    fn admits(self, ir_type: IrType) -> bool {
        match ir_type {
            IrType::I64 => self.checked_int,
            IrType::F64 => self.float,
            _ => false,
        }
    }
}

/// Returns concrete integer or float slots with at least one effectful load to refine.
fn actionable_numeric_load_slots(function: &Function) -> HashSet<LocalSlotId> {
    function
        .instructions
        .iter()
        .filter_map(|inst| actionable_numeric_load_slot(function, inst))
        .collect()
}

/// Returns the concrete numeric slot for an effectful load that this pass may refine.
fn actionable_numeric_load_slot(
    function: &Function,
    inst: &Instruction,
) -> Option<LocalSlotId> {
    if inst.op != Op::LoadLocal || inst.effects.is_pure() {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return None;
    };
    function
        .locals
        .get(slot.as_raw() as usize)
        .filter(|local| {
            local.kind == LocalKind::PhpLocal
                && is_concrete_numeric_storage(local.ir_type, &local.php_type)
        })
        .map(|_| slot)
}

/// Returns numeric PHP-local slots whose contents are immutable for the whole function.
fn immutable_numeric_slots(
    function: &Function,
    candidates: &HashSet<LocalSlotId>,
) -> HashSet<LocalSlotId> {
    if super::local_scope::has_dynamic_eval(function) {
        return HashSet::new();
    }
    let mut eligible = candidates.clone();

    // A SLOT PASSED BY REFERENCE IS NOT IMMUTABLE, and nothing else here can tell:
    // a by-reference argument is lowered as a plain `load_local` whose result the
    // call consumes, and codegen turns that into the slot's ADDRESS, so the callee
    // writes the caller's slot with no `store_local` anywhere in this function. The
    // store-counting below therefore sees "one entry store, never written again" and
    // marks the load pure — after which LICM legitimately hoists it out of the loop
    // that reads it. Measured: `$n = 0; do { f($m, $n); $c++; } while ($n > 0);` ran
    // its body exactly once because the whole `$n > 0` compare was evaluated in the
    // preheader. `super::by_ref_alias` is the shared judgement, also used by dead
    // store elimination for the same aliasing reason.
    for slot in super::by_ref_alias::address_escaping_slots(function) {
        eligible.remove(&slot);
    }

    let mut loads: Vec<Vec<InstId>> = vec![Vec::new(); function.locals.len()];
    let mut stores: Vec<Vec<InstId>> = vec![Vec::new(); function.locals.len()];
    for (raw, inst) in function.instructions.iter().enumerate() {
        let inst_id = InstId::from_raw(raw as u32);
        match inst.immediate.as_ref() {
            Some(Immediate::LocalSlot(slot)) if eligible.contains(slot) => match inst.op {
                Op::LoadLocal => loads[slot.as_raw() as usize].push(inst_id),
                Op::StoreLocal => stores[slot.as_raw() as usize].push(inst_id),
                _ => {
                    eligible.remove(slot);
                }
            },
            Some(Immediate::LocalSlotPair { first, second }) => {
                eligible.remove(first);
                eligible.remove(second);
            }
            _ => {}
        }
    }

    let mut immutable = HashSet::new();
    let mut single_store = Vec::new();
    for slot in eligible {
        let slot_loads = &loads[slot.as_raw() as usize];
        if slot_loads.is_empty() {
            continue;
        }
        match stores[slot.as_raw() as usize].as_slice() {
            [] if slot_is_read_only_input(function, slot) => {
                immutable.insert(slot);
            }
            [store] => single_store.push((slot, *store)),
            _ => {}
        }
    }
    if single_store.is_empty() {
        return immutable;
    }

    let entry_has_no_predecessor = predecessors(function)
        .get(function.entry.as_raw() as usize)
        .is_some_and(Vec::is_empty);
    if !entry_has_no_predecessor {
        return immutable;
    }

    let dominance = compute_dominance(function);
    let locations = instruction_locations(function);
    for (slot, store) in single_store {
        let slot_loads = &loads[slot.as_raw() as usize];
        if store_dominates_loads(function, store, slot_loads, &locations, &dominance) {
            immutable.insert(slot);
        }
    }
    immutable
}

/// Maps every instruction table id to its current block and in-block position.
fn instruction_locations(function: &Function) -> Vec<Option<(crate::ir::BlockId, usize)>> {
    let mut locations = vec![None; function.instructions.len()];
    for block in &function.blocks {
        for (position, inst) in block.instructions.iter().copied().enumerate() {
            locations[inst.as_raw() as usize] = Some((block.id, position));
        }
    }
    locations
}

/// Returns true for an unwritten numeric parameter or the top-level integer `$argc` slot.
fn slot_is_read_only_input(function: &Function, slot: LocalSlotId) -> bool {
    let local = &function.locals[slot.as_raw() as usize];
    let Some(name) = local.name.as_deref() else {
        return false;
    };
    (function.flags.is_main
        && name == "argc"
        && local.ir_type == IrType::I64
        && matches!(local.php_type.codegen_repr(), PhpType::Int))
        || function.params.iter().any(|param| {
            param.name == name
                && !param.by_ref
                && !param.variadic
                && param.ir_type == local.ir_type
                && param.php_type.codegen_repr() == local.php_type.codegen_repr()
                && is_concrete_numeric_storage(param.ir_type, &param.php_type)
        })
}

/// Returns true only for concrete PHP integers in `I64` storage or floats in `F64` storage.
fn is_concrete_numeric_storage(ir_type: IrType, php_type: &PhpType) -> bool {
    matches!(
        (ir_type, php_type.codegen_repr()),
        (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float)
    )
}

/// Proves that the sole store is in the entry and occurs before every local load.
fn store_dominates_loads(
    function: &Function,
    store: InstId,
    loads: &[InstId],
    locations: &[Option<(crate::ir::BlockId, usize)>],
    dominance: &super::dominance::DominanceInfo,
) -> bool {
    let Some((store_block, store_position)) = locations[store.as_raw() as usize] else {
        return false;
    };
    if store_block != function.entry {
        return false;
    }
    loads.iter().all(|load| {
        let Some((load_block, load_position)) = locations[load.as_raw() as usize] else {
            return false;
        };
        if load_block == store_block {
            store_position < load_position
        } else {
            dominance.dominates(store_block, load_block)
        }
    })
}
