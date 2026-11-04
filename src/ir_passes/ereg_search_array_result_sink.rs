//! Purpose:
//! Folds the array result of a literal `[0-9]+` progressive search (`pos`/`regs`/`getregs`)
//! into the known start/length or matched bytes when every read of the result is known.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the scalar progressive state fold.
//!
//! Key details:
//! - The function must not contain user calls, extern calls, eval entries, or any other
//!   regex-session operation. This pass simulates the literal request locally.
//! - Only the standard ownership chain (call -> clone -> acquire -> store -> load) and
//!   generic mixed array reads are accepted. Any whole-array observer keeps the runtime call.
//! - A folded `pos`/`regs` call advances the session, so no later regex call may remain in the
//!   block after it. `getregs` reads the retained match without advancing.

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, Op, Ownership,
    PassOrigin, RuntimeCallTarget, RuntimeFnId, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;
use super::string_len_sink::const_string_value;

/// Folds literal progressive-search array results.
pub struct EregSearchArrayResultSink;

/// The simulated progressive session inside one basic block.
struct State {
    subject: Vec<u8>,
    position: usize,
    matched: Option<(usize, usize)>,
}

/// One known array element and the read that consumes it.
enum Element {
    Int(i64),
    Str(Vec<u8>),
}

/// One planned call/read fold.
struct Fold {
    call: InstId,
    chain: Vec<InstId>,
    reads: Vec<(InstId, Element)>,
}

impl IrPass for EregSearchArrayResultSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "ereg_search_array_result_sink"
    }

    /// Skips functions without an array-returning progressive call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(
                    RuntimeFnId::MbEregSearchPos
                        | RuntimeFnId::MbEregSearchRegs
                        | RuntimeFnId::MbEregSearchGetregs
                )
            )
        })
    }

    /// Folds every independently proven array result.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if !function_is_isolated(function) {
            return false;
        }
        let mut folds = Vec::new();
        for block in &function.blocks {
            let mut state = None;
            let mut pending = Vec::new();
            let mut session_candidates = Vec::new();
            for (position, id) in block.instructions.iter().enumerate() {
                let Some(inst) = function.instruction(*id) else {
                    return false;
                };
                let Some(target) = direct_target(inst) else {
                    continue;
                };
                match target {
                    RuntimeFnId::MbEregSearchInit => {
                        if !initialize(function, data, inst, &mut state) {
                            return false;
                        }
                    }
                    RuntimeFnId::MbEregSearch => {
                        if !search_step(function, data, target, &mut state, None, &mut pending) {
                            return false;
                        }
                    }
                    RuntimeFnId::MbEregSearchPos
                    | RuntimeFnId::MbEregSearchRegs
                    | RuntimeFnId::MbEregSearchGetregs => {
                        let before = pending.len();
                        if !search_step(function, data, target, &mut state, Some(*id), &mut pending) {
                            return false;
                        }
                        if pending.len() == before
                            && matches!(
                                target,
                                RuntimeFnId::MbEregSearchPos | RuntimeFnId::MbEregSearchRegs
                            )
                        {
                            session_candidates.push((*id, position, target));
                        }
                    }
                    _ => {}
                }
            }
            let folded_calls: std::collections::HashSet<InstId> =
                pending.iter().map(|fold| fold.call).collect();
            for (call, position, _target) in session_candidates {
                let later_unfolded = block.instructions[position + 1..].iter().any(|later| {
                    let Some(later) = function.instruction(*later) else {
                        return true;
                    };
                    match direct_target(later) {
                        Some(RuntimeFnId::MbEregSearchInit) => false,
                        Some(
                            RuntimeFnId::MbEregSearchPos
                            | RuntimeFnId::MbEregSearchRegs
                            | RuntimeFnId::MbEregSearchGetregs,
                        ) => !folded_calls.contains(&later_id(function, later)),
                        _ => false,
                    }
                });
                if later_unfolded {
                    continue;
                }
                let Some(result) = function.instruction(call).and_then(|inst| inst.result) else {
                    return false;
                };
                let mut chain = vec![call];
                chain.extend(releases_of(function, result));
                pending.push(Fold { call, chain, reads: Vec::new() });
            }
            folds.extend(pending);
        }
        if folds.is_empty() {
            return false;
        }
        for fold in folds {
            commit(function, data, fold);
        }
        true
    }
}

/// Returns the direct or profiled runtime target for one call.
fn direct_target(inst: &Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Rejects functions where an indirect call or another regex operation could observe the
/// session after this pass removes an advancing array call.
fn function_is_isolated(function: &Function) -> bool {
    function.instructions.iter().all(|inst| match inst.op {
        Op::Call
        | Op::FunctionVariantCall
        | Op::MethodCall
        | Op::StaticMethodCall
        | Op::NullsafeMethodCall
        | Op::LanguageConstructCall
        | Op::EvalLiteralCall
        | Op::EvalFunctionCall
        | Op::EvalFunctionCallArray
        | Op::ExternCall => false,
        Op::RuntimeCall => match direct_target(inst) {
            Some(
                RuntimeFnId::MbEregSearchInit
                | RuntimeFnId::MbEregSearch
                | RuntimeFnId::MbEregSearchPos
                | RuntimeFnId::MbEregSearchRegs
                | RuntimeFnId::MbEregSearchGetregs,
            ) => true,
            Some(target) if is_other_regex_target(target) => false,
            _ => true,
        },
        _ => true,
    })
}

/// Returns whether the target is a regex operation outside the supported progressive set.
fn is_other_regex_target(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::MbEreg
            | RuntimeFnId::MbEregi
            | RuntimeFnId::MbEregMatch
            | RuntimeFnId::MbEregSearchGetpos
            | RuntimeFnId::MbEregSearchSetpos
            | RuntimeFnId::MbEregReplace
            | RuntimeFnId::MbEregReplaceLen
            | RuntimeFnId::MbEregReplaceCallback
            | RuntimeFnId::MbEregiReplace
            | RuntimeFnId::MbRegexEncoding
            | RuntimeFnId::MbRegexEncodingInt
            | RuntimeFnId::MbRegexSetOptions
            | RuntimeFnId::MbRegexSetOptionsLen
    )
}

/// Applies the literal `[0-9]+` initializer to the simulated state.
fn initialize(
    function: &Function,
    data: &DataPool,
    inst: &Instruction,
    state: &mut Option<State>,
) -> bool {
    if inst.operands.len() != 2 {
        return false;
    }
    let Some(subject) = const_string_value(function, data, inst.operands[0]) else {
        return false;
    };
    let Some(pattern) = const_string_value(function, data, inst.operands[1]) else {
        return false;
    };
    if pattern != "[0-9]+" {
        return false;
    }
    let subject = crate::string_bytes::literal_bytes(&subject);
    if std::str::from_utf8(&subject).is_err() {
        return false;
    }
    *state = Some(State { subject, position: 0, matched: None });
    true
}

/// Simulates one search step and optionally plans the array-result fold for `call_id`.
fn search_step(
    function: &Function,
    data: &DataPool,
    target: RuntimeFnId,
    state: &mut Option<State>,
    call_id: Option<InstId>,
    pending: &mut Vec<Fold>,
) -> bool {
    let Some(current) = state.as_mut() else {
        return false;
    };
    let matched = match target {
        RuntimeFnId::MbEregSearch | RuntimeFnId::MbEregSearchPos | RuntimeFnId::MbEregSearchRegs => {
            let mut start = current.position;
            while start < current.subject.len() && !current.subject[start].is_ascii_digit() {
                start += 1;
            }
            if start == current.subject.len() {
                current.matched = None;
                current.position = current.subject.len();
                return call_id.is_none();
            }
            let mut end = start;
            while end < current.subject.len() && current.subject[end].is_ascii_digit() {
                end += 1;
            }
            current.matched = Some((start, end));
            current.position = end;
            (start, end)
        }
        RuntimeFnId::MbEregSearchGetregs => {
            let Some(matched) = current.matched else {
                return false;
            };
            matched
        }
        _ => return false,
    };

    let Some(call_id) = call_id else {
        return true;
    };
    let Some(result) = function.instruction(call_id).and_then(|inst| inst.result) else {
        return false;
    };
    // An unobserved array result (only a release) does not need folding.
    let observed = function.instructions.iter().any(|candidate| {
        candidate.operands.contains(&result) && candidate.op != Op::Release
    });
    if !observed {
        return true;
    }
    let subject = current.subject.clone();
    let Some((chain, reads)) = array_chain(function, data, call_id, result, target, matched, &subject) else {
        return false;
    };
    pending.push(Fold { call: call_id, chain, reads });
    true
}

/// Extracts the standard ownership chain and the known array reads for one call.
fn array_chain(
    function: &Function,
    data: &DataPool,
    call_id: InstId,
    result: ValueId,
    target: RuntimeFnId,
    matched: (usize, usize),
    subject: &[u8],
) -> Option<(Vec<InstId>, Vec<(InstId, Element)>)> {
    let (start, end) = matched;
    let (clone, clone_result) = single_use(function, result, Op::MixedClone)?;
    let (acquire, acquire_result) = single_use(function, clone_result, Op::Acquire)?;
    let (store, store_slot) = single_store(function, acquire_result)?;
    let mut chain = vec![call_id, clone, acquire, store];
    for value in [result, clone_result, acquire_result] {
        chain.extend(releases_of(function, value));
    }
    let mut loads = Vec::new();
    let mut slot_stores = 0;
    let mut slot_cleanups = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if slot == store_slot) {
            continue;
        }
        match inst.op {
            Op::StoreLocal => {
                slot_stores += 1;
                if InstId::from_raw(raw as u32) != store {
                    return None;
                }
            }
            Op::LoadLocal => loads.push(inst.result?),
            Op::ReleaseLocalSlot => slot_cleanups.push(InstId::from_raw(raw as u32)),
            _ => return None,
        }
    }
    if slot_stores != 1 {
        return None;
    }
    chain.extend(slot_cleanups);
    let mut reads = Vec::new();
    for load in loads {
        for (raw, inst) in function.instructions.iter().enumerate() {
            if !inst.operands.contains(&load) {
                continue;
            }
            if inst.op == Op::Release {
                continue;
            }
            if inst.op != Op::RuntimeCall
                || inst.immediate.is_some()
                || inst.operands.len() != 3
                || inst.operands[0] != load
                || inst.result_php_type.codegen_repr() != PhpType::Mixed
            {
                return None;
            }
            let key = match const_string_value(function, data, inst.operands[1]) {
                Some(key) => crate::string_bytes::literal_bytes(&key),
                None => const_i64(function, inst.operands[1])?.to_string().into_bytes(),
            };
            let element = match (target, key.as_slice()) {
                (RuntimeFnId::MbEregSearchPos, b"0") => Element::Int(start as i64),
                (RuntimeFnId::MbEregSearchPos, b"1") => Element::Int((end - start) as i64),
                (RuntimeFnId::MbEregSearchRegs | RuntimeFnId::MbEregSearchGetregs, b"0") => {
                    Element::Str(subject[start..end].to_vec())
                }
                _ => return None,
            };
            reads.push((InstId::from_raw(raw as u32), element));
        }
    }
    Some((chain, reads))
}

/// Returns a literal `i64` value produced by `ConstI64`.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let id = match function.value(value)?.def {
        ValueDef::Instruction { inst, .. } => inst,
        _ => return None,
    };
    match (function.instruction(id)?.op, &function.instruction(id)?.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Returns the instruction identifier for one instruction reference during a block scan.
fn later_id(function: &Function, target: &Instruction) -> InstId {
    function
        .instructions
        .iter()
        .position(|candidate| std::ptr::eq(candidate, target))
        .map(|index| InstId::from_raw(index as u32))
        .unwrap_or(InstId::from_raw(0))
}

/// Returns the single instruction of `op` using `value`, plus its result.
fn single_use(function: &Function, value: ValueId, op: Op) -> Option<(InstId, ValueId)> {
    let mut matches = function.instructions.iter().enumerate().filter_map(|(raw, inst)| {
        (inst.op == op && inst.operands.contains(&value)).then_some((InstId::from_raw(raw as u32), inst.result?))
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// Returns the single `StoreLocal` of `value` and its slot.
fn single_store(function: &Function, value: ValueId) -> Option<(InstId, crate::ir::LocalSlotId)> {
    let mut matches = function.instructions.iter().enumerate().filter_map(|(raw, inst)| {
        (inst.op == Op::StoreLocal && inst.operands.len() == 1 && inst.operands[0] == value)
            .then_some((InstId::from_raw(raw as u32), inst.immediate.clone()))
    });
    let (id, immediate) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let Some(Immediate::LocalSlot(slot)) = immediate else {
        return None;
    };
    Some((id, slot))
}

/// Returns every release of `value`.
fn releases_of(function: &Function, value: ValueId) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            (inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == value)
                .then_some(InstId::from_raw(raw as u32))
        })
        .collect()
}

/// Applies one planned fold: box the known reads and retire the array ownership chain.
fn commit(function: &mut Function, data: &mut DataPool, fold: Fold) {
    let mut reads = fold.reads;
    reads.sort_by_key(|(id, _)| std::cmp::Reverse(id.as_raw()));
    for (read_id, element) in reads {
        let Some(read) = function.instruction(read_id) else {
            continue;
        };
        let Some(result) = read.result else {
            continue;
        };
        let Some(ValueDef::Instruction { block, index, .. }) = function.value(result).map(|value| value.def) else {
            continue;
        };
        let literal = match element {
            Element::Int(value) => insert_const_i64(function, block, index as usize, value),
            Element::Str(value) => {
                if fold_str_len(function, read_id, result, value.len()) {
                    continue;
                }
                insert_const_str(function, data, block, index as usize, &value)
            }
        };
        if let Some(read) = function.instruction_mut(read_id) {
            read.op = Op::MixedBox;
            read.operands = vec![literal];
            read.immediate = None;
            read.result_type = IrType::Heap(IrHeapKind::Mixed);
            read.result_php_type = PhpType::Mixed;
            read.result_ownership = Ownership::Owned;
            read.effects = Op::MixedBox.default_effects();
        }
        if let Some(value) = function.value_mut(result) {
            value.ir_type = IrType::Heap(IrHeapKind::Mixed);
            value.php_type = PhpType::Mixed;
            value.ownership = Ownership::Owned;
        }
    }
    for id in fold.chain {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    let uses = function.value_use_counts();
    let dead_loads: Vec<InstId> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            (inst.op == Op::LoadLocal
                && inst.result.is_some_and(|result| uses.get(&result).copied().unwrap_or(0) == 0))
            .then_some(InstId::from_raw(raw as u32))
        })
        .collect();
    for id in dead_loads {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
}

/// Replaces a `strlen((string)$read)` chain with the known match length.
///
/// This avoids boxing and persisting the matched bytes on every loop iteration. Any other
/// observer of the read or its string cast keeps the ordinary boxed-value path.
fn fold_str_len(function: &mut Function, read_id: InstId, read_result: ValueId, length: usize) -> bool {
    let Some((cast_id, cast_result)) = single_use(function, read_result, Op::Cast) else {
        return false;
    };
    if count_uses_outside(function, read_result, &[Op::Cast, Op::Release]) > 0 {
        return false;
    }
    let Some(cast) = function.instruction(cast_id) else {
        return false;
    };
    if cast.result_php_type.codegen_repr() != PhpType::Str {
        return false;
    }
    let Some((len_id, len_result)) = single_use(function, cast_result, Op::StrLen) else {
        return false;
    };
    if count_uses_outside(function, cast_result, &[Op::StrLen, Op::Release]) > 0 {
        return false;
    }
    let Some(len) = function.instruction_mut(len_id) else {
        return false;
    };
    len.op = Op::ConstI64;
    len.operands.clear();
    len.immediate = Some(Immediate::I64(length as i64));
    len.result_type = IrType::I64;
    len.result_php_type = PhpType::Int;
    len.result_ownership = Ownership::NonHeap;
    len.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(len_result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    for id in [read_id, cast_id]
        .into_iter()
        .chain(releases_of(function, read_result))
        .chain(releases_of(function, cast_result))
    {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Counts uses of `value` whose opcode is not in `allowed`.
fn count_uses_outside(function: &Function, value: ValueId, allowed: &[Op]) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.operands.contains(&value) && !allowed.contains(&inst.op))
        .count()
}

/// Inserts a `ConstI64` immediately before `position` in `block`.
fn insert_const_i64(function: &mut Function, block: BlockId, position: usize, value: i64) -> ValueId {
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let result = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        def: ValueDef::Instruction { block, index: position as u32, inst: inst_id },
        ownership: Ownership::NonHeap,
    });
    let mut inst = Instruction::new(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        Some(result),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        Op::ConstI64.default_effects(),
        None,
    );
    inst.origin = Some(PassOrigin::ConstFold);
    function.instructions.push(inst);
    function.blocks[block.as_raw() as usize].instructions.insert(position, inst_id);
    shift_defs(function, block, position);
    result
}

/// Inserts a persistent `ConstStr` immediately before `position` in `block`.
fn insert_const_str(function: &mut Function, data: &mut DataPool, block: BlockId, position: usize, value: &[u8]) -> ValueId {
    let text = String::from_utf8_lossy(value).into_owned();
    let data_id = data.intern_string(&text);
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let result = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type: IrType::Str,
        php_type: PhpType::Str,
        def: ValueDef::Instruction { block, index: position as u32, inst: inst_id },
        ownership: Ownership::Persistent,
    });
    let mut inst = Instruction::new(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(data_id)),
        Some(result),
        IrType::Str,
        PhpType::Str,
        Ownership::Persistent,
        Op::ConstStr.default_effects(),
        None,
    );
    inst.origin = Some(PassOrigin::ConstFold);
    function.instructions.push(inst);
    function.blocks[block.as_raw() as usize].instructions.insert(position, inst_id);
    shift_defs(function, block, position);
    result
}

/// Refreshes definition indices after inserting one instruction at `position`.
fn shift_defs(function: &mut Function, block: BlockId, position: usize) {
    let shifted: Vec<InstId> =
        function.blocks[block.as_raw() as usize].instructions[position + 1..].to_vec();
    for (offset, shifted_id) in shifted.into_iter().enumerate() {
        if let Some(value) = function.instructions[shifted_id.as_raw() as usize].result {
            function.values[value.as_raw() as usize].def = ValueDef::Instruction {
                block,
                index: (position + 1 + offset) as u32,
                inst: shifted_id,
            };
        }
    }
}
