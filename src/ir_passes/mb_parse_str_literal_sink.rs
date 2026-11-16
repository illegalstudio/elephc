//! Purpose:
//! Folds `mb_parse_str($simple_literal_query, $out)` and the literal-key reads of `$out`
//! when the parsed keys are all known and the output cell has no other observer.
//!
//! Called from:
//! - The EIR fixed-point pass driver after by-reference locals are promoted.
//!
//! Key details:
//! - Only `key=value` pairs of ASCII letters, digits, `_`, `.` and `-` are accepted. Percent
//!   escapes, `+`, brackets, duplicate keys, and empty keys keep the runtime parser.
//! - Every use of the output cell after the call must be a `$out[key]` read for a key present
//!   in the parsed map. A read is replaced by a boxed copy of the parsed string, so skipping
//!   the array write is unobservable.
//! - The parse call, its query guard/acquire scaffolding, and the dead cell loads are retired
//!   together. The parse result itself becomes `true`, matching PHP 8.

use std::collections::HashMap;

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, Op, Ownership,
    MixedNumericOp, PassOrigin, RuntimeCallTarget, RuntimeFnId, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{count_value_uses, defining_instruction, neutralize_to_nop};
use super::string_len_sink::const_string_value;

/// Folds simple literal `mb_parse_str()` calls and their key reads.
pub struct MbParseStrLiteralSink;

impl IrPass for MbParseStrLiteralSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_parse_str_literal_sink"
    }

    /// Skips functions without a parse-str call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(direct_target(inst), Some(RuntimeFnId::MbParseStr))
        })
    }

    /// Folds every independently proven literal query.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| matches!(direct_target(inst), Some(RuntimeFnId::MbParseStr)))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if fold(function, data, candidate) {
                changed = true;
            }
        }
        changed
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

/// Parses the conservative literal query subset accepted by this fold.
fn parse_simple_query(query: &[u8]) -> Option<HashMap<Vec<u8>, Vec<u8>>> {
    let mut entries = HashMap::new();
    for pair in query.split(|&byte| byte == b'&') {
        if pair.is_empty() {
            return None;
        }
        let mut fields = pair.splitn(2, |&byte| byte == b'=');
        let key = fields.next()?;
        let value = fields.next()?;
        if key.is_empty()
            || !key
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'.' | b'-'))
            || !value.iter().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'.' | b'-' | b' ')
            })
        {
            return None;
        }
        if entries.insert(key.to_vec(), value.to_vec()).is_some() {
            return None;
        }
    }
    Some(entries)
}

/// Folds one parse call and all literal-key reads of its output cell.
fn fold(function: &mut Function, data: &mut DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    if call.operands.len() != 2 {
        return false;
    }
    let query_value = call.operands[0];
    let output_value = call.operands[1];
    let Some(query) = const_string_value(function, data, query_value) else {
        return false;
    };
    let Some(output_load) = defining_instruction(function, output_value) else {
        return false;
    };
    if output_load.op != Op::LoadRefCell {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = output_load.immediate else {
        return false;
    };
    let Some(entries) = parse_simple_query(&crate::string_bytes::literal_bytes(&query)) else {
        return false;
    };
    if entries.is_empty() {
        return false;
    }
    let Some(ValueDef::Instruction { block, .. }) = function.value(output_value).map(|value| value.def)
    else {
        return false;
    };
    // Every post-call load of the output cell must feed a recognized literal-key read.
    let mut reads = Vec::new();
    let mut loads_to_retire = Vec::new();
    let mut slot_instructions = 0usize;
    for (raw, inst) in function.instructions.iter().enumerate() {
        let id = InstId::from_raw(raw as u32);
        let touches_slot = matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot)
            || matches!(inst.immediate, Some(Immediate::LocalSlotPair { first, second }) if first == slot || second == slot);
        if !touches_slot {
            continue;
        }
        slot_instructions += 1;
        match inst.op {
            Op::LoadRefCell => {
                if id == defining_id(function, output_value).unwrap_or(id) {
                    continue;
                }
                let Some(result) = inst.result else {
                    return false;
                };
                if !recognized_read(function, data, result, block, &entries, &mut reads) {
                    return false;
                }
                loads_to_retire.push(id);
            }
            Op::PromoteLocalRefCell | Op::ReleaseLocalSlot => {}
            _ => return false,
        }
    }
    if reads.is_empty() || slot_instructions == 0 {
        return false;
    }

    // Replace each read with a boxed copy of its parsed value. A canonical integer whose
    // only consumers are mixed numeric operations is boxed as an int, avoiding the string
    // payload copy while preserving PHP's numeric coercion.
    for (read_id, value) in reads {
        let Some(read) = function.instruction(read_id) else {
            return false;
        };
        let Some(result) = read.result else {
            return false;
        };
        let Some(ValueDef::Instruction { block, index, .. }) = function.value(result).map(|value| value.def)
        else {
            return false;
        };
        let integer = numeric_only_consumer(function, result)
            .then(|| parse_canonical_i64(&value))
            .flatten();
        let literal = match integer {
            Some(value) => insert_const_i64(function, block, index as usize, value),
            None => insert_const_str(function, data, block, index as usize, &value),
        };
        let Some(read) = function.instruction_mut(read_id) else {
            return false;
        };
        read.op = Op::MixedBox;
        read.operands = vec![literal];
        read.immediate = None;
        read.result_type = IrType::Heap(IrHeapKind::Mixed);
        read.result_php_type = PhpType::Mixed;
        read.result_ownership = Ownership::Owned;
        read.effects = Op::MixedBox.default_effects();
        if let Some(value) = function.value_mut(result) {
            value.ir_type = IrType::Heap(IrHeapKind::Mixed);
            value.php_type = PhpType::Mixed;
            value.ownership = Ownership::Owned;
        }
    }

    // The parse call itself always returns true for the accepted subset.
    let Some(result) = function.instruction(call_id).and_then(|inst| inst.result) else {
        return false;
    };
    if let Some(call) = function.instruction_mut(call_id) {
        call.op = Op::ConstBool;
        call.operands.clear();
        call.immediate = Some(Immediate::Bool(true));
        call.result_type = IrType::I64;
        call.result_php_type = PhpType::Bool;
        call.result_ownership = Ownership::NonHeap;
        call.effects = Op::ConstBool.default_effects();
    }
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Bool;
        value.ownership = Ownership::NonHeap;
    }

    neutralize_query_staging(function, query_value);
    // The output local is no longer read after the folded call and reads. Retire its
    // ref-cell promotions and every now-dead cell load; the frame slot stays zero.
    let mut dead = loads_to_retire;
    for (raw, inst) in function.instructions.iter().enumerate() {
        let id = InstId::from_raw(raw as u32);
        let matches_slot = matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot)
            || matches!(inst.immediate, Some(Immediate::LocalSlotPair { first, .. }) if first == slot);
        if !matches_slot {
            continue;
        }
        if inst.op == Op::PromoteLocalRefCell || inst.op == Op::LoadRefCell {
            dead.push(id);
        }
    }
    dead.sort_unstable_by_key(|id| id.as_raw());
    dead.dedup();
    let uses = count_value_uses(function);
    for id in dead {
        let result = function.instruction(id).and_then(|inst| inst.result);
        let removable = match result {
            Some(result) => uses.get(&result).copied().unwrap_or(0) == 0,
            None => function.instruction(id).is_some_and(|inst| inst.op == Op::PromoteLocalRefCell),
        };
        if removable {
            if let Some(inst) = function.instruction_mut(id) {
                neutralize_to_nop(inst);
            }
        }
    }
    true
}

/// Returns whether `load` feeds only a literal-key mixed array read and records it.
fn recognized_read(
    function: &Function,
    data: &DataPool,
    load: ValueId,
    call_block: BlockId,
    entries: &HashMap<Vec<u8>, Vec<u8>>,
    reads: &mut Vec<(InstId, Vec<u8>)>,
) -> bool {
    let mut found = false;
    let Some(load_block) = function.value(load).and_then(|value| match value.def {
        ValueDef::Instruction { block, .. } => Some(block),
        _ => None,
    }) else {
        return false;
    };
    if load_block != call_block {
        return false;
    }
    for inst in &function.instructions {
        if !inst.operands.contains(&load) {
            continue;
        }
        // Only the generic three-operand mixed array read is accepted here.
        if inst.op != Op::RuntimeCall
            || inst.immediate.is_some()
            || inst.operands.len() != 3
            || inst.operands[0] != load
            || inst.result_php_type.codegen_repr() != PhpType::Mixed
        {
            return false;
        }
        let Some(key) = const_string_value(function, data, inst.operands[1]) else {
            return false;
        };
        let key = crate::string_bytes::literal_bytes(&key);
        let Some(value) = entries.get(&key) else {
            return false;
        };
        let Some(result) = inst.result else {
            return false;
        };
        let Some(block) = function.value(result).and_then(|value| match value.def {
            ValueDef::Instruction { block, .. } => Some(block),
            _ => None,
        }) else {
            return false;
        };
        if block != call_block {
            return false;
        }
        reads.push((InstId::from_raw(function.instructions.iter().position(|candidate| std::ptr::eq(candidate, inst)).unwrap() as u32), value.clone()));
        found = true;
    }
    found
}

/// Retires the query's exception guard/acquire scaffolding.
fn neutralize_query_staging(function: &mut Function, query_value: ValueId) {
    let acquire_id = defining_id(function, query_value);
    let source = acquire_id
        .and_then(|id| function.instruction(id))
        .filter(|inst| inst.op == Op::Acquire && inst.operands.len() == 1)
        .map(|inst| inst.operands[0])
        .unwrap_or(query_value);
    let guards: Vec<(InstId, Option<ValueId>)> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            if !matches!(
                inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
            ) {
                return None;
            }
            (inst.operands.first() == Some(&query_value)
                || inst.operands.first() == Some(&source))
            .then_some((InstId::from_raw(raw as u32), inst.result))
        })
        .collect();
    for (guard_id, token) in guards {
        if let Some(token) = token {
            for inst in &mut function.instructions {
                if matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionUnguardOwned))
                ) && inst.operands.first() == Some(&token)
                {
                    neutralize_to_nop(inst);
                }
            }
        }
        if let Some(inst) = function.instruction_mut(guard_id) {
            neutralize_to_nop(inst);
        }
    }
    if let Some(id) = acquire_id {
        if function.instruction(id).is_some_and(|inst| inst.op == Op::Acquire) {
            if let Some(inst) = function.instruction_mut(id) {
                neutralize_to_nop(inst);
            }
        }
    }
    for inst in &mut function.instructions {
        if inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == query_value {
            neutralize_to_nop(inst);
        }
    }
}

/// Returns whether every consumer of `value` is a mixed numeric add/sub/mul or release.
fn numeric_only_consumer(function: &Function, value: ValueId) -> bool {
    function
        .instructions
        .iter()
        .filter(|inst| inst.operands.contains(&value))
        .all(|inst| match inst.op {
            Op::Release => true,
            Op::MixedNumericBinop => matches!(
                inst.immediate,
                Some(Immediate::MixedNumericOp(
                    MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul
                ))
            ),
            _ => false,
        })
}

/// Parses a canonical decimal integer that PHP's numeric-string coercion keeps as an int.
fn parse_canonical_i64(value: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(value).ok()?;
    if text.is_empty() || text.starts_with('+') {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    if text.starts_with("-0") && text.len() > 2 {
        return None;
    }
    text.parse::<i64>().ok()
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
    result
}

/// Inserts a persistent `ConstStr` immediately before `position` in `block`.
fn insert_const_str(
    function: &mut Function,
    data: &mut DataPool,
    block: BlockId,
    position: usize,
    value: &[u8],
) -> ValueId {
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
    result
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some(inst)
}
