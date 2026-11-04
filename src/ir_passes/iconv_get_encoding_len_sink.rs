//! Purpose:
//! Folds `strlen((string)iconv_get_encoding("internal_encoding"))` to the default UTF-8
//! name length when the function cannot change the setting.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant locals are available.
//!
//! Key details:
//! - The whole function must be free of user calls, extern calls, eval, and any runtime
//!   operation other than `hrtime()` and the getter itself. This excludes
//!   `iconv_set_encoding()` and INI mutation through helpers.
//! - Only the `internal_encoding` selector is folded. A different selector, a changed
//!   setting, or an observed whole result keeps the iconv bridge.

use crate::ir::{DataPool, Function, Immediate, InstId, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop};
use super::string_len_sink::const_string_value;

/// Folds the default internal-encoding length.
pub struct IconvGetEncodingLenSink;

impl IrPass for IconvGetEncodingLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "iconv_get_encoding_len_sink"
    }

    /// Skips functions without a getter call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(direct_target(inst), Some(RuntimeFnId::IconvGetEncoding))
        })
    }

    /// Folds every independently proven internal-encoding length.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if !function_is_isolated(function) {
            return false;
        }
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::StrLen && inst.operands.len() == 1)
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
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Rejects functions that could mutate the iconv setting through an indirect call.
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
        Op::RuntimeCall => match inst.immediate {
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::ExceptionGuardOwned | RuntimeCallTarget::ExceptionUnguardOwned,
            )) => true,
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::IconvGetEncoding | RuntimeFnId::Hrtime)
                | RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::IconvGetEncoding | RuntimeFnId::Hrtime,
                    ..
                },
            )) => true,
            _ => false,
        },
        _ => true,
    })
}

/// Folds one `strlen((string)iconv_get_encoding("internal_encoding"))` chain.
fn fold(function: &mut Function, data: &DataPool, length_id: InstId) -> bool {
    let Some(length) = function.instruction(length_id) else {
        return false;
    };
    let Some(cast_value) = length.operands.first().copied() else {
        return false;
    };
    let Some(cast) = defining_instruction(function, cast_value) else {
        return false;
    };
    if cast.op != Op::Cast || cast.operands.len() != 1 {
        return false;
    }
    let call_value = cast.operands[0];
    let Some(call) = defining_instruction(function, call_value) else {
        return false;
    };
    if direct_target(call) != Some(RuntimeFnId::IconvGetEncoding) || call.operands.len() != 1 {
        return false;
    }
    let Some(selector) = const_string_value(function, data, call.operands[0]) else {
        return false;
    };
    if !selector.eq_ignore_ascii_case("internal_encoding") {
        return false;
    }
    if count_uses_outside(function, call_value, &[Op::Cast, Op::Release]) > 0
        || count_uses_outside(function, cast_value, &[Op::StrLen, Op::Release]) > 0
    {
        return false;
    }
    let Some(result) = length.result else {
        return false;
    };
    let Some(length) = function.instruction_mut(length_id) else {
        return false;
    };
    length.op = Op::ConstI64;
    length.operands.clear();
    length.immediate = Some(Immediate::I64(5));
    length.result_type = crate::ir::IrType::I64;
    length.result_php_type = PhpType::Int;
    length.result_ownership = Ownership::NonHeap;
    length.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    let Some(call_id) = defining_id(function, call_value) else {
        return false;
    };
    let Some(cast_id) = defining_id(function, cast_value) else {
        return false;
    };
    let releases: Vec<InstId> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            (inst.op == Op::Release
                && inst.operands.len() == 1
                && (inst.operands[0] == call_value || inst.operands[0] == cast_value))
                .then_some(InstId::from_raw(raw as u32))
        })
        .collect();
    for id in [call_id, cast_id].into_iter().chain(releases) {
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

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some(inst)
}
