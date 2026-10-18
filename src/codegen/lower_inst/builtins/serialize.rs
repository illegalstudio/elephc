//! Purpose:
//! Lowers the `serialize()` and `unserialize()` builtins on the EIR backend.
//! Bridges already-evaluated EIR operands to the shared serialize runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - `serialize()` adapts static or boxed operands to `(tag, lo, hi)` and enters
//!   `__rt_serialize_call_value`, which protects reference state across callbacks.
//!   Its borrowed concat-buffer result is persisted by `store_if_result`.
//! - `unserialize()` brackets parsing with begin/end helpers, preserving policy and
//!   reference owners across nested hooks. Serializable callbacks share references;
//!   modern hooks isolate them. A parser failure is boxed as PHP `false`.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::load_value_to_first_int_arg;
use super::{expect_operand, store_if_result};

/// Lowers `serialize($value)` into the shared serialize runtime helper.
///
/// Scalar, array, object, and Closure static types enter the guarded serializer as a
/// tagged value; a Mixed/Union argument is first unboxed by `__rt_mixed_unbox`.
/// A Closure makes the runtime throw, as PHP does. Other static types are rejected.
pub(crate) fn lower_serialize(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "serialize", 1)?;
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    let is_x86 = ctx.emitter.target.arch == Arch::X86_64;

    match value_ty.codegen_repr() {
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = integer payload
                ctx.emitter.instruction("mov rdi, 0");                          // value_tag = int
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // value_lo = integer payload
                ctx.emitter.instruction("mov x0, #0");                          // value_tag = int
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = bool payload
                ctx.emitter.instruction("mov rdi, 3");                          // value_tag = bool
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // value_lo = bool payload
                ctx.emitter.instruction("mov x0, #3");                          // value_tag = bool
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("movq rsi, xmm0");                      // value_lo = float bit pattern
                ctx.emitter.instruction("mov rdi, 2");                          // value_tag = float
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("fmov x1, d0");                         // value_lo = float bit pattern
                ctx.emitter.instruction("mov x0, #2");                          // value_tag = float
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = string pointer (len stays in rdx)
                ctx.emitter.instruction("mov rdi, 1");                          // value_tag = string
            } else {
                ctx.emitter.instruction("mov x0, #1");                          // value_tag = string (lo/hi already in x1/x2)
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Void | PhpType::Never => {
            if is_x86 {
                ctx.emitter.instruction("mov rdi, 8");                          // value_tag = null
                ctx.emitter.instruction("mov rsi, 0");                          // value_lo unused
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x0, #8");                          // value_tag = null
                ctx.emitter.instruction("mov x1, #0");                          // value_lo unused
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rdi");                        // stage the unboxed payload before overwriting the native tag argument
                ctx.emitter.instruction("mov rdi, rax");                        // adapt the unboxed runtime tag to the serializer C argument
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = indexed array pointer
                ctx.emitter.instruction("mov rdi, 4");                          // value_tag = indexed array
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // value_lo = indexed array pointer
                ctx.emitter.instruction("mov x0, #4");                          // value_tag = indexed array
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = hash pointer
                ctx.emitter.instruction("mov rdi, 5");                          // value_tag = associative array
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // value_lo = hash pointer
                ctx.emitter.instruction("mov x0, #5");                          // value_tag = associative array
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = object pointer
                ctx.emitter.instruction("mov rdi, 6");                          // value_tag = object
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // value_lo = object pointer
                ctx.emitter.instruction("mov x0, #6");                          // value_tag = object
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        PhpType::Callable => {
            // A Closure reaches the runtime like any tagged value, which throws PHP's catchable
            // `Serialization of 'Closure' is not allowed` there, as it does behind `mixed`.
            ctx.load_value_to_result(value)?;
            if is_x86 {
                ctx.emitter.instruction("mov rsi, rax");                        // value_lo = callable descriptor pointer
                ctx.emitter.instruction("mov rdi, 10");                         // value_tag = Closure
                ctx.emitter.instruction("mov rdx, 0");                          // value_hi unused
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // value_lo = callable descriptor pointer
                ctx.emitter.instruction("mov x0, #10");                         // value_tag = Closure
                ctx.emitter.instruction("mov x2, #0");                          // value_hi unused
            }
            abi::emit_call_label(ctx.emitter, "__rt_serialize_call_value");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "serialize() of {:?} is not yet supported",
                other
            )));
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `unserialize($data, $options?)` into the shared unserialize runtime helper.
///
/// The source string is parsed by `__rt_unserialize_mixed`; a null result pointer
/// (parse error or unsupported wire form) is boxed as PHP `false`. The optional
/// `$options` is installed before parsing so the runtime can prevent blocked
/// classes from instantiating or invoking hydration hooks. Begin/end delimit an
/// owned runtime context so nested calls cannot corrupt the outer policy or refs.
pub(crate) fn lower_unserialize(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "unserialize expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let data = expect_operand(inst, 0)?;
    let options = inst.operands.get(1).copied();
    let options_raw_ty = options
        .map(|value| ctx.raw_value_php_type(value))
        .transpose()?;
    let options_ty = options_raw_ty.clone().map(|ty| ty.codegen_repr());
    if let Some(ty) = &options_ty {
        if !matches!(
            ty,
            PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
        ) {
            super::super::exceptions::emit_type_error(
                ctx,
                &format!(
                    "unserialize(): Argument #2 ($options) must be of type array, {} given",
                    unserialize_static_type_name(
                        options_raw_ty.as_ref().expect("an options type was resolved")
                    )
                ),
            );
            return Ok(());
        }
    }
    // Reset the per-call value registry so r:/R: back-references resolve against
    // this unserialize() call's own pre-order value indices. Static option types
    // are rejected above so every emitted begin has a matching runtime end path.
    abi::emit_call_label(ctx.emitter, "__rt_unserialize_begin");
    if options.is_none() {
        abi::emit_call_label(ctx.emitter, "__rt_unserialize_inherit_policy");
    }
    if let (Some(options), Some(options_ty)) = (options, options_ty) {
        ctx.load_value_to_result(options)?;
        match options_ty {
            PhpType::Mixed | PhpType::Union(_) => {
                // A dynamic Mixed is a tagged cell, not an associative-hash
                // pointer. The runtime verifies tag 5 before dereferencing
                // its payload and raises the PHP TypeError for every other tag.
                abi::emit_call_label(ctx.emitter, "__rt_unserialize_set_options_mixed");
            }
            PhpType::Array(_) => {
                // An indexed PHP array cannot contain the string key
                // `allowed_classes`; it is nevertheless a valid empty/default
                // options map and must not be passed to the hash helper.
                abi::emit_call_label(ctx.emitter, "__rt_unserialize_set_options_indexed");
            }
            PhpType::AssocArray { .. } => {
                abi::emit_call_label(ctx.emitter, "__rt_unserialize_set_options");
            }
            _ => unreachable!("invalid static unserialize options were rejected before begin"),
        }
    }
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    match ctx.value_php_type(data)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_string_value_to_regs(data, ptr_reg, len_reg)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, data)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "unserialize() of {:?} is not supported",
                other
            )));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_unserialize_mixed");
    abi::emit_call_label(ctx.emitter, "__rt_unserialize_end");
    box_false_on_unserialize_failure(ctx);
    store_if_result(ctx, inst)
}

/// Returns PHP's diagnostic spelling for a statically rejected unserialize option.
fn unserialize_static_type_name(ty: &PhpType) -> &str {
    match ty {
        PhpType::Int => "int",
        PhpType::Float => "float",
        PhpType::Str => "string",
        PhpType::Bool | PhpType::False => "bool",
        PhpType::Void | PhpType::Never => "null",
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        PhpType::Object(name) | PhpType::Packed(name) => name,
        PhpType::Resource(_) => "resource",
        PhpType::Callable => "Closure",
        PhpType::Iterable => "object",
        PhpType::Buffer(_) | PhpType::Pointer(_) => "resource",
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar => "unknown",
    }
}

/// Replaces a null `__rt_unserialize_mixed` result with a boxed PHP `false`.
///
/// PHP's `unserialize()` returns `false` on malformed or unsupported input; the
/// runtime helper signals that with a null Mixed pointer, which this boxes into a
/// `Mixed(bool=false)` cell so the EIR result stays a valid boxed value.
fn box_false_on_unserialize_failure(ctx: &mut FunctionContext<'_>) {
    let done = ctx.next_label("unserialize_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", done));             // success returns a boxed Mixed
            ctx.emitter.instruction("mov x0, #3");                              // tag = bool
            ctx.emitter.instruction("mov x1, #0");                              // value_lo = false
            ctx.emitter.instruction("mov x2, #0");                              // value_hi unused
            ctx.emitter.instruction("bl __rt_mixed_from_value");                // box the PHP false result
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // success returns a non-null Mixed pointer
            ctx.emitter.instruction(&format!("jne {}", done));                  // skip false boxing on success
            ctx.emitter.instruction("mov rax, 3");                              // tag = bool
            ctx.emitter.instruction("mov rdi, 0");                              // value_lo = false
            ctx.emitter.instruction("mov rsi, 0");                              // value_hi unused
            ctx.emitter.instruction("call __rt_mixed_from_value");              // box the PHP false result
            ctx.emitter.label(&done);
        }
    }
}
