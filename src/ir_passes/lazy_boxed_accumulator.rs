//! Purpose:
//! Keeps integer accumulators unboxed across loops with arbitrary bodies.
//! A mixed local gets a raw `I64` companion and a tag that says which one is current.
//!
//! Called from:
//! - `optimize_module`, once, after the function passes converged. The scalar passes run
//!   again afterwards.
//!
//! Key details:
//! - The tag is RAW (the companion holds the value, the mixed slot holds a stale but valid
//!   int box) or BOXED (the mixed slot holds the value). A function starts BOXED.
//! - A modeled update (`$s += x`, `-=`, `*=`, `++`, `--`) takes the fast path when the
//!   local and `x` are ints and the wrapping operation does not overflow. It stores the
//!   companion and sets RAW. Anything else syncs and runs the original instructions, so
//!   float promotion, warnings, and errors are the unchanged mixed code.
//! - A modeled compare (`$s > x`, `==`, `!=`) of a RAW local with an int is an integer
//!   compare. Otherwise it syncs and runs the original compare.
//! - A value read from another rewritten local is always loaded after that local's sync.
//!   A compare of two candidate locals is therefore modeled by neither.
//! - A sync boxes the companion into the mixed slot when the tag is RAW. One is placed
//!   right before every other instruction that names the slot: a read, another store, an
//!   unset. A by-reference argument is such a read, so the slot is current while the
//!   callee runs.
//! - A function with literal `eval` code is left alone: the fragment reads locals by name,
//!   through no instruction that names a slot.
//! - Nothing happens at function exit. In RAW state the slot still owns a valid box.
//! - There is no loop copy and no entry guard, so any body shape and any number of
//!   accumulators are handled with code linear in the number of update and read sites.
//! - It runs after convergence so `RawPromotableLoop` gets every straight-line loop first:
//!   that copy has no tag tests and continues a promoted local as a raw float.
//! - A local is rewritten only when one of its updates sits in a loop that has no sync
//!   for it. Otherwise the box allocation would only move from the update to the read.
//! - Each plan is tried on a copy of the function. It is dropped when it would cost
//!   another local the frame cell codegen gives it, because that proof needs a read and
//!   its user in one block and this rewrite splits blocks.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, Builder, CmpPredicate, DataPool, Function, Immediate, InstId, IrHeapKind, IrType,
    LocalKind, LocalSlotId, MixedNumericOp, Op, PhpTypePredicate, Terminator, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::loops::compute_loops;
use super::raw_promotable_loop::{
    eligible, emit, frame_cells, integer_op, load_mixed, mixed_local, release_slot,
};

/// Gives overflow-promotable accumulators a raw integer companion in general loops.
pub struct LazyBoxedAccumulator;

impl IrPass for LazyBoxedAccumulator {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "lazy_boxed_accumulator"
    }

    /// Skips functions that have no mixed arithmetic to specialize.
    fn is_applicable(&self, function: &Function) -> bool {
        function
            .instructions
            .iter()
            .any(|inst| matches!(inst.op, Op::MixedNumericBinop | Op::StrIncDec))
    }

    /// Rewrites every profitable accumulator of the function.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if !eligible(function, data) || reads_locals_by_name(function) {
            return false;
        }
        let plans = analyze(function);
        if plans.is_empty() {
            return false;
        }
        // Each plan is tried on a copy first: it must not cost another local its frame cell.
        let cells = frame_cells(function, data);
        let mut slots = HashSet::new();
        let mut values = HashSet::new();
        let mut changed = false;
        for plan in &plans {
            let mut trial = function.clone();
            apply(&mut trial, plan);
            slots.insert(plan.slot);
            let results: Vec<ValueId> = plan
                .sites
                .iter()
                .filter_map(|site| function.instruction(site.binop).and_then(|inst| inst.result))
                .collect();
            values.extend(results.iter().copied());
            if cells.kept_by(&frame_cells(&trial, data), &slots, &values) {
                *function = trial;
                changed = true;
            } else {
                slots.remove(&plan.slot);
                for value in results {
                    values.remove(&value);
                }
            }
        }
        changed
    }
}

/// Returns true when the function runs literal `eval` code.
///
/// Such a fragment reads and writes the frame's locals by name. The call names no slot in
/// its immediate, so no sync would run before it and it would see a stale box.
fn reads_locals_by_name(function: &Function) -> bool {
    function
        .instructions
        .iter()
        .any(|inst| matches!(inst.op, Op::EvalLiteralCall | Op::EvalScopeGet | Op::EvalScopeSet))
}

/// Name prefix of the raw integer companion of a rewritten local.
const INT_PREFIX: &str = "lazyint";

/// Block name marker for code this pass wrote. Updates inside are never modeled again.
const MARKER: &str = "lazy.";

/// Returns true when `slot` already has a raw companion from this pass.
///
/// Its mixed slot may be stale, so no other pass may read it as the local's value without
/// the sync this pass places before each read.
pub(super) fn is_lazy(function: &Function, slot: LocalSlotId) -> bool {
    let name = format!("{INT_PREFIX}{}", slot.as_raw());
    function.locals.iter().any(|local| local.name.as_deref() == Some(name.as_str()))
}

/// One local to rewrite: its modeled updates and compares, and every other instruction
/// naming its slot.
struct Plan {
    slot: LocalSlotId,
    sites: Vec<Site>,
    compares: Vec<Compare>,
    syncs: Vec<InstId>,
}

/// One modeled compare of a local with an int, load and compare in one block.
struct Compare {
    /// The `PhpRelCmp`, `LooseEq`, or `LooseNotEq`.
    compare: InstId,
    /// The `LoadLocal` that feeds the local into `compare`. Nothing else uses it.
    load: InstId,
    predicate: CmpPredicate,
    other: Other,
    /// True when the local is the left operand.
    local_on_left: bool,
}

/// One modeled update of a local, all in one block.
struct Site {
    /// The `MixedNumericBinop` or `StrIncDec`.
    binop: InstId,
    /// The `LoadLocal` that feeds the local into `binop`. Nothing else uses it.
    load: InstId,
    /// `binop` and the instructions that only exist to store its result.
    members: HashSet<InstId>,
    op: MixedNumericOp,
    other: Other,
    /// True when the local is the left operand.
    local_on_left: bool,
}

/// The operand of an update that is not the local itself.
#[derive(Clone, Copy)]
enum Other {
    /// The unit step of `++` and `--`.
    One,
    /// A raw PHP int.
    Int(ValueId),
    /// A mixed value, tested for an int payload at run time.
    Mixed(ValueId),
}

/// Where every instruction sits and who uses every value.
struct Index {
    position: HashMap<InstId, (BlockId, usize)>,
    users: HashMap<ValueId, Vec<InstId>>,
    terminator_uses: HashSet<ValueId>,
}

/// Indexes the instructions that are still listed in a block.
fn index(function: &Function) -> Index {
    let mut position = HashMap::new();
    let mut users: HashMap<ValueId, Vec<InstId>> = HashMap::new();
    let mut terminator_uses = HashSet::new();
    for block in &function.blocks {
        for (at, id) in block.instructions.iter().enumerate() {
            position.insert(*id, (block.id, at));
            for operand in &function.instructions[id.as_raw() as usize].operands {
                users.entry(*operand).or_default().push(*id);
            }
        }
        if let Some(terminator) = &block.terminator {
            terminator_uses.extend(terminator.used_values());
        }
    }
    Index { position, users, terminator_uses }
}

/// Returns the local slots an instruction names in its immediate.
pub(super) fn slots_of(function: &Function, id: InstId) -> Vec<LocalSlotId> {
    match function.instructions[id.as_raw() as usize].immediate.as_ref() {
        Some(Immediate::LocalSlot(slot)) => vec![*slot],
        Some(Immediate::LocalSlotPair { first, second }) => vec![*first, *second],
        Some(Immediate::IterStart(metadata)) => metadata.local_slots().collect(),
        _ => Vec::new(),
    }
}

/// Finds the locals worth rewriting.
fn analyze(function: &Function) -> Vec<Plan> {
    let index = index(function);
    let dominance = super::dominance::compute_dominance(function);
    let loops = compute_loops(function, &dominance);
    if loops.is_empty() {
        return Vec::new();
    }
    let mut references: HashMap<LocalSlotId, Vec<InstId>> = HashMap::new();
    for block in &function.blocks {
        for id in &block.instructions {
            for slot in slots_of(function, *id) {
                references.entry(slot).or_default().push(*id);
            }
        }
    }
    let mut slots: Vec<_> = references.keys().copied().collect();
    slots.sort_unstable();
    let mut drafts = Vec::new();
    for slot in slots {
        if !mixed_local(function, slot) {
            continue;
        }
        let mut sites = Vec::new();
        let mut modeled: HashSet<InstId> = HashSet::new();
        for id in &references[&slot] {
            let Some(site) = match_site(function, &index, slot, *id) else {
                continue;
            };
            modeled.insert(site.load);
            modeled.extend(site.members.iter().copied());
            sites.push(site);
        }
        let mut compares = Vec::new();
        for id in &references[&slot] {
            if modeled.contains(id) {
                continue;
            }
            if let Some(compare) = match_compare(function, &index, slot, *id) {
                compares.push(compare);
            }
        }
        drafts.push((slot, sites, modeled, compares));
    }
    // A compare of two candidate locals is claimed by both. Each side would then unbox the
    // other's stale slot, so neither models it: both loads become reads that sync.
    let mut claims: HashMap<InstId, usize> = HashMap::new();
    for (_, _, _, compares) in &drafts {
        for compare in compares {
            *claims.entry(compare.compare).or_default() += 1;
        }
    }
    let mut plans = Vec::new();
    for (slot, sites, mut modeled, mut compares) in drafts {
        compares.retain(|compare| claims[&compare.compare] == 1);
        modeled.extend(compares.iter().map(|compare| compare.load));
        let syncs: Vec<InstId> =
            references[&slot].iter().copied().filter(|id| !modeled.contains(id)).collect();
        let profitable = sites.iter().any(|site| {
            let block = index.position[&site.binop].0;
            loops.innermost_loop(block).is_some_and(|lp| {
                !syncs.iter().any(|sync| lp.contains(index.position[sync].0))
            })
        });
        if profitable && address_reads_are_safe(function, &index, slot, &sites) {
            plans.push(Plan { slot, sites, compares, syncs });
        }
    }
    plans
}

/// Recognizes the update that ends in `store`, a `StoreLocal` of `slot`.
///
/// The shape is a load of the local, one arithmetic instruction that reads it, owner copies
/// and clones of the result, a release of the slot, the store, and releases of the result. Everything
/// from the arithmetic to the last of them sits in one block. Other instructions in that
/// span may only be releases of unrelated values, which run after the update either way.
fn match_site(function: &Function, index: &Index, slot: LocalSlotId, store: InstId) -> Option<Site> {
    let store_inst = function.instruction(store)?;
    if store_inst.op != Op::StoreLocal || store_inst.immediate != Some(Immediate::LocalSlot(slot)) {
        return None;
    }
    let (block, store_at) = *index.position.get(&store)?;
    let name = &function.block(block)?.name;
    // The raw loop copies keep their own mixed fallback, and this pass keeps its own.
    if name.contains(MARKER) || name.contains("rawint.") {
        return None;
    }
    let mut members = HashSet::from([store]);
    let mut chain = Vec::new();
    let mut value = *store_inst.operands.first()?;
    let binop = loop {
        chain.push(value);
        let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
            return None;
        };
        let producer = function.instruction(inst)?;
        let (producer_block, producer_at) = *index.position.get(&inst)?;
        if producer_block != block || producer_at >= store_at || chain.len() > 8 {
            return None;
        }
        members.insert(inst);
        match producer.op {
            Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned | Op::MixedClone
                if producer.immediate.is_none() && producer.operands.len() == 1 =>
            {
                value = producer.operands[0];
            }
            Op::MixedNumericBinop | Op::StrIncDec => break inst,
            _ => return None,
        }
    };
    let binop_inst = function.instruction(binop)?;
    let binop_at = index.position[&binop].1;
    let is_local_load = |value: ValueId| local_load(function, slot, value);
    let (op, load, other, local_on_left) = match (binop_inst.op, &binop_inst.immediate) {
        (Op::StrIncDec, Some(Immediate::I64(step @ (1 | -1)))) => {
            let load = is_local_load(*binop_inst.operands.first()?)?;
            let op = if *step == 1 { MixedNumericOp::Add } else { MixedNumericOp::Sub };
            (op, load, Other::One, true)
        }
        (
            Op::MixedNumericBinop,
            Some(Immediate::MixedNumericOp(
                op @ (MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul),
            )),
        ) => {
            let [lhs, rhs] = binop_inst.operands.as_slice() else {
                return None;
            };
            let (load, other, local_on_left) = match (is_local_load(*lhs), is_local_load(*rhs)) {
                (Some(load), None) => (load, *rhs, true),
                (None, Some(load)) => (load, *lhs, false),
                _ => return None,
            };
            (*op, load, other_operand(function, other)?, local_on_left)
        }
        _ => return None,
    };
    // The load feeds only this update, from the same block, with the slot untouched between.
    let load_result = function.instruction(load)?.result?;
    let (load_block, load_at) = *index.position.get(&load)?;
    if load_block != block
        || load_at >= binop_at
        || index.users.get(&load_result).map(Vec::as_slice) != Some(&[binop])
        || index.terminator_uses.contains(&load_result)
    {
        return None;
    }
    let instructions = &function.block(block)?.instructions;
    if instructions[load_at + 1..binop_at].iter().any(|id| slots_of(function, *id).contains(&slot)) {
        return None;
    }
    // After the arithmetic: releases of its result, and the one release of the slot.
    for (at, id) in instructions.iter().enumerate().skip(binop_at + 1) {
        let inst = function.instruction(*id)?;
        let releases_result =
            inst.op == Op::Release && inst.operands.first().is_some_and(|value| chain.contains(value));
        let releases_slot = at < store_at
            && inst.op == Op::ReleaseLocalSlot
            && inst.immediate == Some(Immediate::LocalSlot(slot));
        if releases_result || releases_slot {
            members.insert(*id);
        }
    }
    let last = members.iter().map(|id| index.position[id].1).max()?;
    for id in &instructions[binop_at..=last] {
        if members.contains(id) {
            continue;
        }
        let inst = function.instruction(*id)?;
        if !matches!(inst.op, Op::Release | Op::Nop) || slots_of(function, *id).contains(&slot) {
            return None;
        }
    }
    // The result and its copies live only inside the update.
    for value in &chain {
        if index.terminator_uses.contains(value)
            || index.users.get(value).is_some_and(|users| users.iter().any(|id| !members.contains(id)))
        {
            return None;
        }
    }
    Some(Site { binop, load, members, op, other, local_on_left })
}

/// Returns the instruction that loads `slot` and defines `value`, when there is one.
fn local_load(function: &Function, slot: LocalSlotId, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    let producer = function.instruction(inst)?;
    (producer.op == Op::LoadLocal && producer.immediate == Some(Immediate::LocalSlot(slot)))
        .then_some(inst)
}

/// Classifies the operand of an update or compare that is not the local.
fn other_operand(function: &Function, value: ValueId) -> Option<Other> {
    let metadata = function.value(value)?;
    match (&metadata.ir_type, &metadata.php_type) {
        (IrType::I64, PhpType::Int) => Some(Other::Int(value)),
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed) => Some(Other::Mixed(value)),
        _ => None,
    }
}

/// Recognizes a compare whose one operand is `load`, a `LoadLocal` of `slot` used by
/// nothing else.
///
/// Two PHP ints compare as integers under `<`, `<=`, `>`, `>=`, `==`, and `!=`, with no
/// warning. Load and compare sit in one block with the slot untouched between them.
fn match_compare(function: &Function, index: &Index, slot: LocalSlotId, load: InstId) -> Option<Compare> {
    let load_inst = function.instruction(load)?;
    if load_inst.op != Op::LoadLocal || load_inst.immediate != Some(Immediate::LocalSlot(slot)) {
        return None;
    }
    let load_result = load_inst.result?;
    let [compare] = index.users.get(&load_result)?.as_slice() else {
        return None;
    };
    if index.terminator_uses.contains(&load_result) {
        return None;
    }
    let compare_inst = function.instruction(*compare)?;
    let predicate = match (compare_inst.op, &compare_inst.immediate) {
        (Op::LooseEq, None) => CmpPredicate::Eq,
        (Op::LooseNotEq, None) => CmpPredicate::Ne,
        (
            Op::PhpRelCmp,
            Some(Immediate::CmpPredicate(
                predicate @ (CmpPredicate::Slt | CmpPredicate::Sle | CmpPredicate::Sgt | CmpPredicate::Sge),
            )),
        ) => *predicate,
        _ => return None,
    };
    let [lhs, rhs] = compare_inst.operands.as_slice() else {
        return None;
    };
    let (other, local_on_left) = match (*lhs == load_result, *rhs == load_result) {
        (true, false) => (*rhs, true),
        (false, true) => (*lhs, false),
        _ => return None,
    };
    if local_load(function, slot, other).is_some() || compare_inst.result.is_none() {
        return None;
    }
    let other = other_operand(function, other)?;
    let (block, load_at) = *index.position.get(&load)?;
    let (compare_block, compare_at) = *index.position.get(compare)?;
    let name = &function.block(block)?.name;
    if compare_block != block || load_at >= compare_at || name.contains(MARKER) || name.contains("rawint.") {
        return None;
    }
    let instructions = &function.block(block)?.instructions;
    if instructions[load_at + 1..compare_at].iter().any(|id| slots_of(function, *id).contains(&slot)) {
        return None;
    }
    Some(Compare { compare: *compare, load, predicate, other, local_on_left })
}

/// Returns true when no update can run between a load whose address a call may take and
/// that call.
///
/// A by-reference argument is lowered as a plain load handed to the call, and the callee
/// then writes the slot itself. The sync before the load makes the slot current. It stays
/// current only if no update of the local runs before the consumer, which this checks
/// within one block and refuses across blocks.
fn address_reads_are_safe(function: &Function, index: &Index, slot: LocalSlotId, sites: &[Site]) -> bool {
    let site_loads: HashSet<InstId> = sites.iter().map(|site| site.load).collect();
    // A compare load is used by its compare alone, which reads the value.
    for (id, (block, at)) in &index.position {
        let inst = &function.instructions[id.as_raw() as usize];
        if inst.op != Op::LoadLocal
            || inst.immediate != Some(Immediate::LocalSlot(slot))
            || site_loads.contains(id)
        {
            continue;
        }
        let Some(result) = inst.result else {
            continue;
        };
        for user in index.users.get(&result).map(Vec::as_slice).unwrap_or_default() {
            if super::by_ref_alias::op_is_value_only_consumer(function.instructions[user.as_raw() as usize].op) {
                continue;
            }
            let (user_block, user_at) = index.position[user];
            let update_between = sites.iter().any(|site| {
                let (site_block, site_at) = index.position[&site.binop];
                site_block == *block && site_at > *at && site_at < user_at
            });
            if user_block != *block || update_between {
                return false;
            }
        }
    }
    true
}

/// The raw companions of one rewritten local.
#[derive(Clone, Copy)]
struct Companions {
    slot: LocalSlotId,
    int: LocalSlotId,
    tag: LocalSlotId,
}

/// Adds the companions, starts the function BOXED, and rewrites every site of the plan.
fn apply(function: &mut Function, plan: &Plan) {
    let raw = plan.slot.as_raw();
    // Mem2reg promotes only named scalar PHP locals. The names are not PHP variables.
    let int = function.add_local(
        Some(format!("{INT_PREFIX}{raw}")),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    let tag = function.add_local(
        Some(format!("lazytag{raw}")),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    let local = Companions { slot: plan.slot, int, tag };
    start_boxed(function, local);
    for site in &plan.sites {
        rewrite_update(function, local, site);
    }
    for compare in &plan.compares {
        rewrite_compare(function, local, compare);
    }
    for sync in &plan.syncs {
        sync_before(function, local, *sync);
    }
}

/// Stores the BOXED tag and a zero companion at the top of the entry block.
fn start_boxed(function: &mut Function, local: Companions) {
    let entry = function.entry;
    let scratch = {
        let mut builder = Builder::new(function);
        let scratch = builder.create_named_block("lazy.init", vec![]);
        builder.position_at_end(scratch);
        let zero = builder.emit_const_i64(0);
        builder.emit_store_local(local.int, zero);
        builder.emit_store_local(local.tag, zero);
        builder.terminate(Terminator::Unreachable);
        scratch
    };
    let mut instructions = std::mem::take(&mut function.blocks[scratch.as_raw() as usize].instructions);
    instructions.append(&mut function.blocks[entry.as_raw() as usize].instructions);
    function.blocks[entry.as_raw() as usize].instructions = instructions;
    refresh_definitions(function, entry);
}

/// Points every value a block defines back at its current block and position.
fn refresh_definitions(function: &mut Function, block: BlockId) {
    let instructions = function.blocks[block.as_raw() as usize].instructions.clone();
    for (index, inst) in instructions.into_iter().enumerate() {
        if let Some(result) = function.instructions[inst.as_raw() as usize].result {
            function.values[result.as_raw() as usize].def =
                ValueDef::Instruction { block, index: index as u32, inst };
        }
    }
}

/// Returns the block and position of an instruction that is listed in a block.
fn locate(function: &Function, id: InstId) -> Option<(BlockId, usize)> {
    function.blocks.iter().find_map(|block| {
        block.instructions.iter().position(|inst| *inst == id).map(|at| (block.id, at))
    })
}

/// Moves the instructions from `at` on, and the terminator, into a new block.
///
/// The original block keeps its identity, parameters, and name, so a loop header stays a
/// loop header. The caller terminates it.
fn split_at(function: &mut Function, block: BlockId, at: usize) -> BlockId {
    let tail = Builder::new(function).create_named_block("lazy.cont", vec![]);
    let source = &mut function.blocks[block.as_raw() as usize];
    let moved = source.instructions.split_off(at);
    let terminator = source.terminator.take();
    let target = &mut function.blocks[tail.as_raw() as usize];
    target.instructions = moved;
    target.terminator = terminator;
    refresh_definitions(function, tail);
    tail
}

/// Emits the test for the RAW tag.
fn emit_is_raw(builder: &mut Builder<'_>, local: Companions) -> ValueId {
    let tag = builder.emit_load_local(local.tag, IrType::I64, PhpType::Int);
    let zero = builder.emit_const_i64(0);
    emit(
        builder,
        Op::ICmp,
        vec![tag, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
        IrType::I64,
        PhpType::Bool,
    )
}

/// Boxes the companion into the mixed slot and sets BOXED.
fn emit_publish(builder: &mut Builder<'_>, local: Companions) {
    let value = builder.emit_load_local(local.int, IrType::I64, PhpType::Int);
    let boxed = emit(
        builder,
        Op::MixedBox,
        vec![value],
        None,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    release_slot(builder, local.slot);
    builder.emit_store_local(local.slot, boxed);
    let zero = builder.emit_const_i64(0);
    builder.emit_store_local(local.tag, zero);
}

/// Emits a sync at the end of `block` and continues in `next`.
fn emit_sync(builder: &mut Builder<'_>, local: Companions, block: BlockId, next: BlockId) {
    let publish = builder.create_named_block("lazy.sync", vec![]);
    builder.position_at_end(block);
    let raw = emit_is_raw(builder, local);
    builder.terminate(Terminator::CondBr {
        cond: raw,
        then_target: publish,
        then_args: vec![],
        else_target: next,
        else_args: vec![],
    });
    builder.position_at_end(publish);
    emit_publish(builder, local);
    builder.terminate(Terminator::Br { target: next, args: vec![] });
}

/// Makes the mixed slot current right before `inst`.
fn sync_before(function: &mut Function, local: Companions, inst: InstId) {
    let Some((block, at)) = locate(function, inst) else {
        return;
    };
    let tail = split_at(function, block, at);
    emit_sync(&mut Builder::new(function), local, block, tail);
}

/// Splits one update into the raw fast path and the original instructions.
fn rewrite_update(function: &mut Function, local: Companions, site: &Site) {
    let Some((block, binop_at)) = locate(function, site.binop) else {
        return;
    };
    let load_inst = function.instructions[site.load.as_raw() as usize].clone();
    let Some(stale_load) = load_inst.result else {
        return;
    };
    // The update's own instructions go to the generic block. What sat between them, and
    // everything after, continues in the join block.
    let tail = function.blocks[block.as_raw() as usize].instructions.split_off(binop_at);
    let terminator = function.blocks[block.as_raw() as usize].terminator.take();
    let last = tail.iter().rposition(|id| site.members.contains(id)).unwrap_or(0);
    let mut update = Vec::new();
    let mut after = Vec::new();
    for (at, id) in tail.into_iter().enumerate() {
        if at <= last && site.members.contains(&id) {
            update.push(id);
        } else {
            after.push(id);
        }
    }
    let mut builder = Builder::new(function);
    let boxed = builder.create_named_block("lazy.boxed", vec![]);
    let unbox = builder.create_named_block("lazy.unbox", vec![]);
    let compute = builder.create_named_block("lazy.compute", vec![]);
    let fits = builder.create_named_block("lazy.fits", vec![]);
    let generic = builder.create_named_block("lazy.generic", vec![]);
    let original = builder.create_named_block("lazy.generic.update", vec![]);
    let join = builder.create_named_block("lazy.join", vec![]);

    builder.position_at_end(block);
    let raw = emit_is_raw(&mut builder, local);
    builder.terminate(Terminator::CondBr {
        cond: raw,
        then_target: compute,
        then_args: vec![],
        else_target: boxed,
        else_args: vec![],
    });

    // BOXED: the slot must hold an int for the fast path.
    builder.position_at_end(boxed);
    let current = load_mixed(&mut builder, local.slot);
    let is_int = emit_int_test(&mut builder, current);
    builder.terminate(Terminator::CondBr {
        cond: is_int,
        then_target: unbox,
        then_args: vec![],
        else_target: generic,
        else_args: vec![],
    });
    // The companion is written while the tag is still BOXED. A later failure of the fast
    // path leaves the tag BOXED, so this value is never read.
    builder.position_at_end(unbox);
    let payload = emit(&mut builder, Op::MixedUnbox, vec![current], None, IrType::I64, PhpType::Int);
    builder.emit_store_local(local.int, payload);
    builder.terminate(Terminator::Br { target: compute, args: vec![] });

    builder.position_at_end(compute);
    let own = builder.emit_load_local(local.int, IrType::I64, PhpType::Int);
    let other = match site.other {
        Other::One => builder.emit_const_i64(1),
        Other::Int(value) => value,
        Other::Mixed(value) => {
            let is_int = emit_int_test(&mut builder, value);
            let unbox_other = builder.create_named_block("lazy.operand", vec![]);
            builder.terminate(Terminator::CondBr {
                cond: is_int,
                then_target: unbox_other,
                then_args: vec![],
                else_target: generic,
                else_args: vec![],
            });
            builder.position_at_end(unbox_other);
            emit(&mut builder, Op::MixedUnbox, vec![value], None, IrType::I64, PhpType::Int)
        }
    };
    let pair = if site.local_on_left { vec![own, other] } else { vec![other, own] };
    let result = emit(&mut builder, integer_op(site.op), pair.clone(), None, IrType::I64, PhpType::Int);
    let overflow = emit(
        &mut builder,
        Op::IOverflows,
        pair,
        Some(Immediate::MixedNumericOp(site.op)),
        IrType::I64,
        PhpType::Bool,
    );
    builder.terminate(Terminator::CondBr {
        cond: overflow,
        then_target: generic,
        then_args: vec![],
        else_target: fits,
        else_args: vec![],
    });
    builder.position_at_end(fits);
    builder.emit_store_local(local.int, result);
    let one = builder.emit_const_i64(1);
    builder.emit_store_local(local.tag, one);
    builder.terminate(Terminator::Br { target: join, args: vec![] });

    // Generic: make the slot current, then run the original update on a fresh load. The
    // load before the split may point at the box a sync has just released.
    emit_sync(&mut builder, local, generic, original);
    builder.position_at_end(original);
    let fresh = builder
        .emit(
            Op::LoadLocal,
            vec![],
            Some(Immediate::LocalSlot(local.slot)),
            load_inst.result_type,
            load_inst.result_php_type.clone(),
            load_inst.result_ownership,
        )
        .expect("local load produces a value");
    drop(builder);

    for operand in &mut function.instructions[site.binop.as_raw() as usize].operands {
        if *operand == stale_load {
            *operand = fresh;
        }
    }
    let original_block = &mut function.blocks[original.as_raw() as usize];
    original_block.instructions.extend(update);
    original_block.terminator = Some(Terminator::Br { target: join, args: vec![] });
    refresh_definitions(function, original);
    let join_block = &mut function.blocks[join.as_raw() as usize];
    join_block.instructions = after;
    join_block.terminator = terminator;
    refresh_definitions(function, join);
}

/// Splits one compare into an integer compare of the companion and the original compare.
///
/// The join block receives the boolean as a parameter, which replaces the original result
/// in every use.
fn rewrite_compare(function: &mut Function, local: Companions, site: &Compare) {
    let Some((block, compare_at)) = locate(function, site.compare) else {
        return;
    };
    let load_inst = function.instructions[site.load.as_raw() as usize].clone();
    let compare_inst = function.instructions[site.compare.as_raw() as usize].clone();
    let (Some(stale_load), Some(original_result)) = (load_inst.result, compare_inst.result) else {
        return;
    };
    let mut after = function.blocks[block.as_raw() as usize].instructions.split_off(compare_at);
    after.remove(0);
    let mut terminator = function.blocks[block.as_raw() as usize].terminator.take();

    let mut builder = Builder::new(function);
    let raw_block = builder.create_named_block("lazy.compare", vec![]);
    let generic = builder.create_named_block("lazy.generic", vec![]);
    let original = builder.create_named_block("lazy.generic.compare", vec![]);
    let join = builder.create_named_block(
        "lazy.join",
        vec![(compare_inst.result_type, compare_inst.result_php_type.clone())],
    );
    let joined = builder.block_param(join, 0);

    builder.position_at_end(block);
    let raw = emit_is_raw(&mut builder, local);
    builder.terminate(Terminator::CondBr {
        cond: raw,
        then_target: raw_block,
        then_args: vec![],
        else_target: original,
        else_args: vec![],
    });

    builder.position_at_end(raw_block);
    let own = builder.emit_load_local(local.int, IrType::I64, PhpType::Int);
    let other = match site.other {
        Other::One => builder.emit_const_i64(1),
        Other::Int(value) => value,
        Other::Mixed(value) => {
            let is_int = emit_int_test(&mut builder, value);
            let unbox_other = builder.create_named_block("lazy.operand", vec![]);
            builder.terminate(Terminator::CondBr {
                cond: is_int,
                then_target: unbox_other,
                then_args: vec![],
                else_target: generic,
                else_args: vec![],
            });
            builder.position_at_end(unbox_other);
            emit(&mut builder, Op::MixedUnbox, vec![value], None, IrType::I64, PhpType::Int)
        }
    };
    let pair = if site.local_on_left { vec![own, other] } else { vec![other, own] };
    let fast = emit(
        &mut builder,
        Op::ICmp,
        pair,
        Some(Immediate::CmpPredicate(site.predicate)),
        compare_inst.result_type,
        compare_inst.result_php_type.clone(),
    );
    builder.terminate(Terminator::Br { target: join, args: vec![fast] });

    // The other operand is not an int: make the slot current and compare the mixed values.
    emit_sync(&mut builder, local, generic, original);
    builder.position_at_end(original);
    let fresh = builder
        .emit(
            Op::LoadLocal,
            vec![],
            Some(Immediate::LocalSlot(local.slot)),
            load_inst.result_type,
            load_inst.result_php_type.clone(),
            load_inst.result_ownership,
        )
        .expect("local load produces a value");
    drop(builder);

    // Rename the uses first: the edge below must still pass the original result.
    let renamed = HashMap::from([(original_result, joined)]);
    super::rewrite::replace_all_uses(function, &renamed);
    if let Some(terminator) = terminator.as_mut() {
        super::rewrite::replace_in_terminator(terminator, &renamed);
    }
    for operand in &mut function.instructions[site.compare.as_raw() as usize].operands {
        if *operand == stale_load {
            *operand = fresh;
        }
    }
    let original_block = &mut function.blocks[original.as_raw() as usize];
    original_block.instructions.push(site.compare);
    original_block.terminator = Some(Terminator::Br { target: join, args: vec![original_result] });
    refresh_definitions(function, original);
    let join_block = &mut function.blocks[join.as_raw() as usize];
    join_block.instructions = after;
    join_block.terminator = terminator;
    refresh_definitions(function, join);
}

/// Emits the run-time test for a mixed value that holds a PHP int.
fn emit_int_test(builder: &mut Builder<'_>, value: ValueId) -> ValueId {
    emit(
        builder,
        Op::TypePredicate,
        vec![value],
        Some(Immediate::TypePredicate(PhpTypePredicate::Int)),
        IrType::I64,
        PhpType::Bool,
    )
}
