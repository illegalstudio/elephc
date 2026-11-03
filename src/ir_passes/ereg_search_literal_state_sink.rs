//! Purpose:
//! Folds a literal `mb_ereg_search_init("[0-9]+")` request and its progressive
//! search/getpos/setpos observations to constants.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the constant string locals are available.
//!
//! Key details:
//! - The whole function must contain no user call, extern call, eval entry, or any other
//!   regex-session operation: otherwise the removed initialization could be observed by a
//!   call this pass does not rewrite.
//! - Only literal ASCII/UTF-8 subjects and the exact `[0-9]+` pattern are simulated. Digit
//!   runs can never start inside a UTF-8 continuation byte, so byte positions match PHP's.
//! - A foldable call whose operator shape is unsupported aborts every rewrite in the
//!   function, leaving the original progressive session intact.

use crate::ir::{DataPool, Function, Immediate, Op, Ownership, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::defining_instruction;
use super::string_len_sink::const_string_value;

/// Folds the literal digit-run progressive search state machine.
pub struct EregSearchLiteralStateSink;

/// One constant replacement for a folded runtime call.
enum Replacement {
    Bool(bool),
    Int(i64),
}

/// The known progressive session state inside one basic block.
struct State {
    subject: Vec<u8>,
    position: usize,
}

impl IrPass for EregSearchLiteralStateSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "ereg_search_literal_state_sink"
    }

    /// Skips functions without a progressive search initializer.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(direct_target(inst), Some(RuntimeFnId::MbEregSearchInit))
        })
    }

    /// Folds the complete literal session, or leaves the function untouched.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        if !function_is_isolated(function) {
            return false;
        }
        let mut replacements = Vec::new();
        for block in &function.blocks {
            let mut state = None;
            for id in &block.instructions {
                let Some(inst) = function.instruction(*id) else {
                    return false;
                };
                let Some(target) = direct_target(inst) else {
                    continue;
                };
                if !is_foldable(target) {
                    continue;
                }
                let Some(replacement) = simulate(function, data, inst, target, &mut state) else {
                    return false;
                };
                replacements.push((*id, replacement));
            }
        }
        if replacements.is_empty() {
            return false;
        }
        for (id, replacement) in replacements {
            let Some(inst) = function.instruction_mut(id) else {
                return false;
            };
            let Some(result) = inst.result else {
                return false;
            };
            match replacement {
                Replacement::Bool(value) => {
                    inst.op = Op::ConstBool;
                    inst.operands.clear();
                    inst.immediate = Some(Immediate::Bool(value));
                    inst.result_type = crate::ir::IrType::I64;
                    inst.result_php_type = PhpType::Bool;
                    inst.result_ownership = Ownership::NonHeap;
                    inst.effects = Op::ConstBool.default_effects();
                    if let Some(value) = function.value_mut(result) {
                        value.ir_type = crate::ir::IrType::I64;
                        value.php_type = PhpType::Bool;
                        value.ownership = Ownership::NonHeap;
                    }
                }
                Replacement::Int(value) => {
                    inst.op = Op::ConstI64;
                    inst.operands.clear();
                    inst.immediate = Some(Immediate::I64(value));
                    inst.result_type = crate::ir::IrType::I64;
                    inst.result_php_type = PhpType::Int;
                    inst.result_ownership = Ownership::NonHeap;
                    inst.effects = Op::ConstI64.default_effects();
                    if let Some(value) = function.value_mut(result) {
                        value.ir_type = crate::ir::IrType::I64;
                        value.php_type = PhpType::Int;
                        value.ownership = Ownership::NonHeap;
                    }
                }
            }
        }
        true
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

/// Returns whether the target is one of the progressive calls this pass can simulate.
fn is_foldable(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::MbEregSearchInit
            | RuntimeFnId::MbEregSearch
            | RuntimeFnId::MbEregSearchGetpos
            | RuntimeFnId::MbEregSearchSetpos
    )
}

/// Returns whether the target reads or mutates the shared regex session.
fn is_regex_session_target(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::MbEreg
            | RuntimeFnId::MbEregi
            | RuntimeFnId::MbEregMatch
            | RuntimeFnId::MbEregSearchPos
            | RuntimeFnId::MbEregSearchRegs
            | RuntimeFnId::MbEregSearchGetregs
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

/// Rejects functions in which an indirect call or another regex operation could observe the
/// progressive session after this pass removes its initialization.
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
            Some(target) if is_foldable(target) => true,
            Some(target) if is_regex_session_target(target) => false,
            _ => true,
        },
        _ => true,
    })
}

/// Applies one progressive call to the simulated state, returning its constant result.
fn simulate(
    function: &Function,
    data: &DataPool,
    inst: &crate::ir::Instruction,
    target: RuntimeFnId,
    state: &mut Option<State>,
) -> Option<Replacement> {
    match target {
        RuntimeFnId::MbEregSearchInit => {
            if inst.operands.len() != 2 {
                return None;
            }
            let subject = const_string_value(function, data, inst.operands[0])?;
            let pattern = const_string_value(function, data, inst.operands[1])?;
            if pattern != "[0-9]+" {
                return None;
            }
            let subject = crate::string_bytes::literal_bytes(&subject);
            std::str::from_utf8(&subject).ok()?;
            *state = Some(State { subject, position: 0 });
            Some(Replacement::Bool(true))
        }
        RuntimeFnId::MbEregSearch => {
            if !inst.operands.is_empty() {
                return None;
            }
            let current = state.as_mut()?;
            let mut start = current.position;
            while start < current.subject.len() && !current.subject[start].is_ascii_digit() {
                start += 1;
            }
            if start == current.subject.len() {
                current.position = current.subject.len();
                Some(Replacement::Bool(false))
            } else {
                let mut end = start;
                while end < current.subject.len() && current.subject[end].is_ascii_digit() {
                    end += 1;
                }
                current.position = end;
                Some(Replacement::Bool(true))
            }
        }
        RuntimeFnId::MbEregSearchGetpos => {
            if !inst.operands.is_empty() {
                return None;
            }
            let current = state.as_ref()?;
            Some(Replacement::Int(current.position as i64))
        }
        RuntimeFnId::MbEregSearchSetpos => {
            if inst.operands.len() != 1 {
                return None;
            }
            let current = state.as_mut()?;
            let position = const_i64(function, inst.operands[0])?;
            if position < 0 || position as u64 > current.subject.len() as u64 {
                return None;
            }
            current.position = position as usize;
            Some(Replacement::Bool(true))
        }
        _ => None,
    }
}

/// Returns a literal `i64` value produced by `ConstI64`.
fn const_i64(function: &Function, value: crate::ir::ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}
