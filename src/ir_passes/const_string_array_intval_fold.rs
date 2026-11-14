//! Purpose:
//! Folds `(int) $texts[$i & M]` for an immutable literal string array built once in the
//! entry block whose only observer is the integer cast.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the string and integer sink passes.
//!
//! Key details:
//! - The receiver must be a `PhpLocal` built in the entry block from `array_new` plus
//!   `array_push` calls over single-use `ConstStr` literals, stored once and loaded only by
//!   the indexed read. Any other store, load, unset or element mutation keeps the generic
//!   array path.
//! - The index must be `value & (len - 1)` with a power-of-two length, so every read is
//!   in bounds and non-negative; the checked `array_get` becomes `array_get_unchecked`.
//! - Each pushed string is replaced in place by the compile-time PHP integer cast of that
//!   literal, the array slot and every wrapper are retyped to `array<int>`, and the
//!   acquire/cast/release scaffolding disappears. PHP's leading-numeric-run grammar,
//!   float-form truncation and out-of-range saturation come from the shared AST cast fold.
//! - A dynamic index, mismatched mask, shared literal, non-literal element, observed array,
//!   non-entry construction or non-cast observer falls back to the runtime conversion.

use std::collections::HashMap;

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, LocalKind,
    LocalSlotId, Op, Ownership, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Folds integer casts of masked reads over immutable literal string arrays.
pub struct ConstStringArrayIntvalFold;

impl IrPass for ConstStringArrayIntvalFold {
    /// Returns the stable pass name used in fixed-point diagnostics.
    fn name(&self) -> &'static str {
        "const_string_array_intval_fold"
    }

    /// Skips functions with no integer cast.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_int_cast)
    }

    /// Folds every independently proven masked literal-array read.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if super::local_scope::has_dynamic_eval(function) {
            return false;
        }
        let dominance = super::dominance::compute_dominance(function);
        let loops = super::loops::compute_loops(function, &dominance);
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_int_cast(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if fold(function, data, candidate, &dominance, &loops) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a one-operand string-to-int cast.
fn is_int_cast(inst: &Instruction) -> bool {
    inst.op == Op::Cast
        && inst.immediate == Some(Immediate::CastTarget(IrType::I64))
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Int
}

/// Returns the defining instruction and its id for one SSA value.
fn defining_instruction_with_id(
    function: &Function,
    value: ValueId,
) -> Option<(InstId, &Instruction)> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Follows transparent ownership wrappers back to the producer value.
fn underlying(function: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..8 {
        let Some((_, inst)) = defining_instruction_with_id(function, value) else {
            break;
        };
        if matches!(
            inst.op,
            Op::Acquire | Op::Borrow | Op::EnsureOwned | Op::Move | Op::MixedClone
        ) && inst.operands.len() == 1
        {
            value = inst.operands[0];
        } else {
            break;
        }
    }
    value
}

/// Folds one masked literal-array read after proving every consumer and construction step.
fn fold(
    function: &mut Function,
    data: &DataPool,
    candidate: InstId,
    dominance: &super::dominance::DominanceInfo,
    loops: &super::loops::LoopInfo,
) -> bool {
    let uses = function.value_use_counts();
    let Some(cast) = function.instruction(candidate) else {
        return false;
    };
    let Some(cast_result) = cast.result else {
        return false;
    };
    let cast_operand = cast.operands[0];
    let (cast_wrapper, get_value) = match defining_instruction_with_id(function, cast_operand) {
        Some((id, inst))
            if matches!(inst.op, Op::Acquire | Op::Borrow | Op::EnsureOwned)
                && inst.operands.len() == 1 =>
        {
            (Some(id), inst.operands[0])
        }
        _ => (None, cast_operand),
    };
    let Some((get_id, get)) = defining_instruction_with_id(function, get_value) else {
        return false;
    };
    if get.op != Op::ArrayGet
        || get.operands.len() != 2
        || get.result_php_type.codegen_repr() != PhpType::Str
    {
        return false;
    }
    if !cast_chain_is_scaffold(function, cast_result, get_value, cast_wrapper, candidate, &uses) {
        return false;
    }
    let array_operand = get.operands[0];
    let array_value = underlying(function, array_operand);
    if array_operand != array_value {
        return false;
    }
    let index = get.operands[1];
    let Some((load_id, load)) = defining_instruction_with_id(function, array_value) else {
        return false;
    };
    if load.op != Op::LoadLocal {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return false;
    };
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    if local.kind != LocalKind::PhpLocal
        || !matches!(
            local.php_type.codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Str
        )
    {
        return false;
    }
    let Some(store_id) = unique_slot_store(function, slot, load_id) else {
        return false;
    };
    let Some(array_new_id) = literal_array_construction(function, store_id, &uses) else {
        return false;
    };
    let Some(array_new_value) = function
        .instruction(array_new_id)
        .and_then(|inst| inst.result)
    else {
        return false;
    };
    let pushes = array_pushes(function, array_new_value);
    let length = pushes.len();
    if length == 0 || !index_is_proven_in_bounds(function, index, length) {
        return false;
    }
    let Some(get_block) = instruction_block(function, get_id) else {
        return false;
    };
    if !slot_releases_preserve_array(function, slot, get_block, dominance, loops) {
        return false;
    }
    let Some(get_position) = instruction_position(function, get_id) else {
        return false;
    };
    let construction_blocks = std::iter::once(array_new_id)
        .chain(std::iter::once(store_id))
        .chain(pushes.iter().map(|(push_id, _)| *push_id));
    for instruction in construction_blocks {
        let Some(block) = instruction_block(function, instruction) else {
            return false;
        };
        if !dominance.dominates(block, get_block) {
            return false;
        }
        if block == get_block {
            let Some(position) = instruction_position(function, instruction) else {
                return false;
            };
            if position >= get_position {
                return false;
            }
        }
    }
    let mut replacements = Vec::with_capacity(pushes.len());
    for (push_id, push_value) in &pushes {
        let Some((literal_id, literal)) = defining_instruction_with_id(function, *push_value) else {
            return false;
        };
        if literal.op != Op::ConstStr || uses.get(push_value).copied().unwrap_or(0) != 1 {
            return false;
        }
        let Some(Immediate::Data(data_id)) = literal.immediate else {
            return false;
        };
        let Some(text) = data.strings.get(data_id.as_raw() as usize) else {
            return false;
        };
        let Some(integer) = crate::optimize::fold::casts::parse_string_cast_int(text) else {
            return false;
        };
        replacements.push((*push_id, literal_id, integer));
    }

    // -- retype the literal container and element slots --
    let array_type = PhpType::Array(Box::new(PhpType::Int));
    retype_value(
        function,
        array_new_value,
        IrType::Heap(IrHeapKind::Array),
        array_type.clone(),
    );
    if let Some(store_value) = function
        .instruction(store_id)
        .and_then(|inst| inst.operands.first().copied())
    {
        retype_value(
            function,
            store_value,
            IrType::Heap(IrHeapKind::Array),
            array_type.clone(),
        );
    }
    if let Some(load_value) = function.instruction(load_id).and_then(|inst| inst.result) {
        retype_value(
            function,
            load_value,
            IrType::Heap(IrHeapKind::Array),
            array_type.clone(),
        );
    }
    function.locals[slot.as_raw() as usize].php_type = array_type;
    for (_, literal_id, integer) in &replacements {
        let Some(literal) = function.instruction_mut(*literal_id) else {
            return false;
        };
        literal.op = Op::ConstI64;
        literal.immediate = Some(Immediate::I64(*integer));
        literal.result_type = IrType::I64;
        literal.result_php_type = PhpType::Int;
        literal.result_ownership = Ownership::NonHeap;
        literal.effects = Op::ConstI64.default_effects();
        if let Some(result) = literal.result {
            if let Some(value) = function.value_mut(result) {
                value.ir_type = IrType::I64;
                value.php_type = PhpType::Int;
                value.ownership = Ownership::NonHeap;
            }
        }
    }

    // -- rewrite the masked read itself --
    let Some(get_result) = function.instruction(get_id).and_then(|inst| inst.result) else {
        return false;
    };
    {
        let Some(get) = function.instruction_mut(get_id) else {
            return false;
        };
        get.op = Op::ArrayGetUnchecked;
        get.result_type = IrType::I64;
        get.result_php_type = PhpType::Int;
        get.result_ownership = Ownership::NonHeap;
        get.effects = Op::ArrayGetUnchecked.default_effects();
    }
    if let Some(value) = function.value_mut(get_result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    replace_all_uses(function, &HashMap::from([(cast_result, get_result)]));
    let mut removable = vec![candidate];
    removable.extend(cast_wrapper);
    removable.extend(cast_releases(function, cast_result, get_value, cast_operand));
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Returns the single store into `slot`, rejecting every mention but the expected load.
fn unique_slot_store(function: &Function, slot: LocalSlotId, load_id: InstId) -> Option<InstId> {
    let mut store = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !super::checked_int_sink::instruction_mentions_slot(inst, slot) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StoreLocal => {
                if store.replace(id).is_some() {
                    return None;
                }
            }
            Op::LoadLocal => {
                if id != load_id {
                    return None;
                }
            }
            Op::ReleaseLocalSlot => {}
            _ => return None,
        }
    }
    store
}

/// Rejects a slot release that can free the literal array before the masked read.
///
/// A release inside the read's natural loop would leave the next iteration reading freed
/// storage, and a release in a block that dominates the read runs on the only path to it.
/// Function-exit cleanup lives in blocks that do neither and stays allowed.
fn slot_releases_preserve_array(
    function: &Function,
    slot: LocalSlotId,
    get_block: BlockId,
    dominance: &super::dominance::DominanceInfo,
    loops: &super::loops::LoopInfo,
) -> bool {
    let reading_loop = loops.innermost_loop(get_block);
    for (raw, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::ReleaseLocalSlot
            || !super::checked_int_sink::instruction_mentions_slot(inst, slot)
        {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        let Some(block) = instruction_block(function, id) else {
            return false;
        };
        if dominance.dominates(block, get_block)
            || reading_loop.is_some_and(|loop_ref| loop_ref.contains(block))
        {
            return false;
        }
    }
    true
}

/// Returns the `array_new` producer of the stored literal array.
fn literal_array_construction(
    function: &Function,
    store_id: InstId,
    uses: &HashMap<ValueId, usize>,
) -> Option<InstId> {
    let stored = function.instruction(store_id)?.operands.first().copied()?;
    let array_new = underlying(function, stored);
    let (array_new_id, inst) = defining_instruction_with_id(function, array_new)?;
    if inst.op != Op::ArrayNew {
        return None;
    }
    let value = inst.result?;
    let push_count = function
        .instructions
        .iter()
        .filter(|user| user.op == Op::ArrayPush && user.operands.first() == Some(&value))
        .count();
    let acquire_count = function
        .instructions
        .iter()
        .filter(|user| user.op == Op::Acquire && user.operands.first() == Some(&value))
        .count();
    let release_count = function
        .instructions
        .iter()
        .filter(|user| user.op == Op::Release && user.operands.first() == Some(&value))
        .count();
    if push_count == 0
        || acquire_count != 1
        || uses.get(&value).copied().unwrap_or(0) != push_count + acquire_count + release_count
    {
        return None;
    }
    if stored == array_new || uses.get(&stored).copied().unwrap_or(0) != 1 {
        return None;
    }
    Some(array_new_id)
}

/// Returns the ordered `(push instruction, pushed value)` pairs of one literal array.
fn array_pushes(function: &Function, array: ValueId) -> Vec<(InstId, ValueId)> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == Op::ArrayPush && inst.operands.first() == Some(&array))
        .filter_map(|(raw, inst)| {
            (inst.operands.len() == 2).then_some((InstId::from_raw(raw as u32), inst.operands[1]))
        })
        .collect()
}

/// Proves the index is either a literal in `0..length` or `value & (length - 1)`.
fn index_is_proven_in_bounds(function: &Function, index: ValueId, length: usize) -> bool {
    let Some((_, inst)) = defining_instruction_with_id(function, index) else {
        return false;
    };
    if inst.op == Op::ConstI64 {
        let Some(Immediate::I64(value)) = inst.immediate else {
            return false;
        };
        return value >= 0 && (value as u64) < length as u64;
    }
    if inst.op != Op::IBitAnd || inst.operands.len() != 2 || !length.is_power_of_two() {
        return false;
    }
    let mask = length as i64 - 1;
    inst.operands.iter().any(|&candidate| {
        defining_instruction_with_id(function, candidate).is_some_and(|(_, constant)| {
            constant.immediate == Some(Immediate::I64(mask))
        })
    })
}

/// Confirms the cast and its transparent wrapper observe nothing but each other.
fn cast_chain_is_scaffold(
    function: &Function,
    cast_result: ValueId,
    get_value: ValueId,
    wrapper: Option<InstId>,
    cast: InstId,
    uses: &HashMap<ValueId, usize>,
) -> bool {
    let mut observed = false;
    if let Some(wrapper_id) = wrapper {
        let Some(wrapper_result) = function.instruction(wrapper_id).and_then(|inst| inst.result)
        else {
            return false;
        };
        if function
            .instruction(cast)
            .and_then(|inst| inst.operands.first())
            != Some(&wrapper_result)
        {
            return false;
        }
        for (raw, inst) in function.instructions.iter().enumerate() {
            if !inst.operands.contains(&wrapper_result) {
                continue;
            }
            let id = InstId::from_raw(raw as u32);
            if id != cast && inst.op != Op::Release {
                return false;
            }
            observed = true;
        }
    }
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&get_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if Some(id) == wrapper || id == cast || inst.op == Op::Release {
            observed = true;
            continue;
        }
        return false;
    }
    if !observed || uses.get(&cast_result).copied().unwrap_or(0) == 0 {
        return false;
    }
    let wrapper_result = wrapper
        .and_then(|id| function.instruction(id))
        .and_then(|inst| inst.result);
    function.blocks.iter().all(|block| {
        block.terminator.as_ref().is_none_or(|term| {
            let used = term.used_values();
            !used.contains(&get_value)
                && wrapper_result.is_none_or(|value| !used.contains(&value))
        })
    })
}

/// Collects the release instructions of the cast chain.
fn cast_releases(
    function: &Function,
    cast_result: ValueId,
    get_value: ValueId,
    cast_operand: ValueId,
) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| {
            inst.op == Op::Release
                && (inst.operands.as_slice() == [get_value]
                    || inst.operands.as_slice() == [cast_operand]
                    || inst.operands.as_slice() == [cast_result])
        })
        .map(|(raw, _)| InstId::from_raw(raw as u32))
        .collect()
}

/// Retags one value and its defining instruction's result metadata.
fn retype_value(function: &mut Function, value: ValueId, ir_type: IrType, php_type: PhpType) {
    if let Some(stored) = function.value_mut(value) {
        stored.ir_type = ir_type;
        stored.php_type = php_type.clone();
    }
    if let Some(inst_id) = function.value(value).and_then(|stored| match stored.def {
        ValueDef::Instruction { inst, .. } => Some(inst),
        _ => None,
    }) {
        if let Some(inst) = function.instruction_mut(inst_id) {
            inst.result_type = ir_type;
            inst.result_php_type = php_type;
        }
    }
}

/// Returns the block containing an instruction, if it is placed in one.
fn instruction_block(function: &Function, target: InstId) -> Option<BlockId> {
    function
        .blocks
        .iter()
        .find(|block| block.instructions.contains(&target))
        .map(|block| block.id)
}

/// Returns the position of an instruction within its block.
fn instruction_position(function: &Function, target: InstId) -> Option<usize> {
    function
        .blocks
        .iter()
        .find_map(|block| block.instructions.iter().position(|&id| id == target))
}
