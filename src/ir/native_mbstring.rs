//! Purpose:
//! Decides which typed mbstring calls are answered entirely by native runtime helpers, so
//! runtime-feature discovery and codegen agree on whether the shared mbstring bridge is used.
//!
//! Called from:
//! - `crate::ir_lower::program::runtime_features` while deriving optional runtime families.
//! - `crate::codegen::lower_inst::builtins::strings::mbstring` before selecting a fast path.
//!
//! Key details:
//! - `mb_check_encoding($string, 'UTF-8')` validates through `__rt_mb_strlen_utf8`, a leaf
//!   helper with no mbstring state, so such a call must not enable the bridge, its request
//!   lifecycle, or the per-literal identity hooks.
//! - Only canonical operands qualify: a subject already typed `string` and an encoding operand
//!   that is a constant `UTF-8`/`UTF8` spelling, possibly behind `Acquire` copies. Every other
//!   shape (arrays, nullable or coerced subjects, dynamic encodings, spread arguments) keeps the
//!   shared bridge and its PHP diagnostics.

use crate::types::PhpType;

use super::{Function, Immediate, Instruction, Module, Op, RuntimeArgumentLayout, RuntimeCallTarget, RuntimeFnId, ValueDef, ValueId};

/// Maximum `Acquire` copies followed back to the constant encoding operand.
const ACQUIRE_DEPTH: usize = 4;

/// Returns whether this instruction is an `mb_check_encoding()` call answered natively.
pub fn check_encoding_is_native_utf8(module: &Module, function: &Function, inst: &Instruction) -> bool {
    let positional = match inst.immediate {
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::MbCheckEncoding))) => true,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
            target: RuntimeFnId::MbCheckEncoding,
            arguments,
            ..
        })) => arguments != RuntimeArgumentLayout::IndexedArray,
        _ => false,
    };
    positional
        && inst.op == Op::RuntimeCall
        && inst.operands.len() == 2
        && value_is_plain_string(function, inst.operands[0])
        && value_is_plain_string(function, inst.operands[1])
        && const_string_is_utf8(module, function, inst.operands[1])
}

/// Returns whether a value is already a PHP string, so no coercion or diagnostic can run.
fn value_is_plain_string(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|metadata| is_plain_string_type(&metadata.php_type))
}

/// Accepts `string` and unions whose every member is `string`.
fn is_plain_string_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Str => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(is_plain_string_type),
        _ => false,
    }
}

/// Returns whether a value is a constant naming PHP's UTF-8 encoding.
pub fn const_string_is_utf8(module: &Module, function: &Function, value: ValueId) -> bool {
    const_string(module, function, value)
        .is_some_and(|encoding| encoding.eq_ignore_ascii_case("UTF-8") || encoding.eq_ignore_ascii_case("UTF8"))
}

/// Resolves a constant string operand through a bounded chain of `Acquire` copies.
fn const_string(module: &Module, function: &Function, value: ValueId) -> Option<String> {
    let mut value = value;
    for _ in 0..=ACQUIRE_DEPTH {
        let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
            return None;
        };
        let instruction = function.instruction(inst)?;
        match instruction.op {
            Op::Acquire if instruction.operands.len() == 1 => value = instruction.operands[0],
            Op::ConstStr => {
                let Some(Immediate::Data(data)) = instruction.immediate else {
                    return None;
                };
                let literal = module.data.strings.get(data.as_raw() as usize)?;
                return Some(crate::names::literal_symbol_name(literal).unwrap_or_else(|| literal.clone()));
            }
            _ => return None,
        }
    }
    None
}
