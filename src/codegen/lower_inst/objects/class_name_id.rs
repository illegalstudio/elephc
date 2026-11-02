//! Purpose:
//! Lowers a runtime class-string lookup to the closed-world class-id registry.
//! Supports concrete string values, concrete objects, boxed `mixed` strings and boxed `mixed` objects.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `Op::ClassNameToId`.
//!
//! Key details:
//! - The shared helper returns `-1` for an absent or non-string receiver.
//! - It uses PHP's case-insensitive class-name comparison and never constructs an object.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::{expect_operand, store_if_result};

/// Resolves a string or boxed mixed string operand to a runtime class id, or `-1` on a miss.
pub(in crate::codegen::lower_inst) fn lower_class_name_to_id(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name = expect_operand(inst, 0)?;
    match ctx.value_php_type(class_name)?.codegen_repr() {
        PhpType::Str => lower_string_class_name_to_id(ctx, class_name)?,
        PhpType::Mixed | PhpType::Union(_) => lower_mixed_class_name_to_id(ctx, class_name)?,
        // `Closure::bind($f, $t, $object)` names its scope by an instance: the object's own class.
        PhpType::Object(_) => emit_class_id_of_value(ctx, class_name)?,
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "class_name_to_id for PHP type {:?}",
                other
            )));
        }
    }
    store_if_result(ctx, inst)
}

/// Leaves the runtime class id of an object, class-name string, or boxed Mixed value in the
/// integer result register, or `-1` when the value names no compiled class.
pub(in crate::codegen::lower_inst) fn emit_class_id_of_value(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => ctx.emitter.instruction("ldr x0, [x0]"),         // the object header's first word is its class id
                Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [rax]"), // the object header's first word is its class id
            }
            Ok(())
        }
        PhpType::Str => lower_string_class_name_to_id(ctx, value),
        PhpType::Mixed | PhpType::Union(_) => lower_mixed_class_name_to_id(ctx, value),
        other => Err(CodegenIrError::unsupported(format!(
            "class id of PHP type {:?}",
            other
        ))),
    }
}

/// Calls the runtime lookup helper with Elephc's concrete string ABI.
fn lower_string_class_name_to_id(
    ctx: &mut FunctionContext<'_>,
    class_name: crate::ir::ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(class_name, "x0", "x1")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(class_name, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_class_id_by_name");
    Ok(())
}

/// Unboxes a gradual class-string boundary: a string resolves through the registry and an
/// object answers its own class id (`$value::CONST` accepts both), anything else is a miss.
fn lower_mixed_class_name_to_id(
    ctx: &mut FunctionContext<'_>,
    class_name: crate::ir::ValueId,
) -> Result<()> {
    ctx.load_value_to_result(class_name)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let string = ctx.next_label("class_name_to_id_string");
    let object = ctx.next_label("class_name_to_id_object");
    let done = ctx.next_label("class_name_to_id_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                            // runtime tag 1 is string
            ctx.emitter.instruction(&format!("b.eq {string}"));               // resolve the string payload through the class registry
            ctx.emitter.instruction("cmp x0, #6");                            // runtime tag 6 is object
            ctx.emitter.instruction(&format!("b.eq {object}"));               // an object answers its own class id
            ctx.emitter.instruction("mov x0, #-1");                           // absent/non-string values have no class id
            ctx.emitter.instruction(&format!("b {done}"));                    // skip the string lookup on a non-string value
            ctx.emitter.label(&object);
            ctx.emitter.instruction("ldr x0, [x1]");                          // the object header's first word is its class id
            ctx.emitter.instruction(&format!("b {done}"));                    // the class id is already the result
            ctx.emitter.label(&string);
            ctx.emitter.instruction("mov x0, x1");                            // string payload pointer becomes helper arg0
            ctx.emitter.instruction("mov x1, x2");                            // string payload length becomes helper arg1
            abi::emit_call_label(ctx.emitter, "__rt_class_id_by_name");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                            // runtime tag 1 is string
            ctx.emitter.instruction(&format!("je {string}"));                 // resolve the string payload through the class registry
            ctx.emitter.instruction("cmp rax, 6");                            // runtime tag 6 is object
            ctx.emitter.instruction(&format!("je {object}"));                 // an object answers its own class id
            ctx.emitter.instruction("mov rax, -1");                           // absent/non-string values have no class id
            ctx.emitter.instruction(&format!("jmp {done}"));                  // skip the string lookup on a non-string value
            ctx.emitter.label(&object);
            ctx.emitter.instruction("mov rax, QWORD PTR [rdi]");              // the object header's first word is its class id
            ctx.emitter.instruction(&format!("jmp {done}"));                  // the class id is already the result
            ctx.emitter.label(&string);
            ctx.emitter.instruction("mov rsi, rdx");                          // string payload length becomes helper arg1
            // `__rt_mixed_unbox` already left the payload pointer in rdi (arg0).
            abi::emit_call_label(ctx.emitter, "__rt_class_id_by_name");
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}
