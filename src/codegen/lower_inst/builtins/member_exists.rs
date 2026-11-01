//! Purpose:
//! Lowers non-literal `method_exists()` and `property_exists()` member-name probes through the
//! closed-world runtime member registry.
//!
//! Called from:
//! - `super::lower_member_exists()` when the member operand is not a constant string.
//!
//! Key details:
//! - Object targets resolve their runtime class before lookup; class-string targets remain strings.
//! - Mixed targets accept only runtime object/string payloads and throw PHP `TypeError` otherwise.
//! - Method names are case-insensitive in the registry; property names remain case-sensitive.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::store_if_result;

/// Lowers a member-existence probe whose member name is only known at runtime.
pub(super) fn lower_dynamic_member_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: ValueId,
    member: ValueId,
    name: &str,
) -> Result<()> {
    save_dynamic_member_name(ctx, member)?;
    materialize_dynamic_target_class_name(ctx, target, name)?;
    call_member_exists_registry(ctx, name);
    store_if_result(ctx, inst)
}

/// Saves the dynamic member string across target class-name resolution calls.
fn save_dynamic_member_name(ctx: &mut FunctionContext<'_>, member: ValueId) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(member, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    Ok(())
}

/// Resolves an object/string/Mixed target to a class-name string in standard result registers.
fn materialize_dynamic_target_class_name(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    match ctx.value_php_type(target)?.codegen_repr() {
        PhpType::Object(_) => {
            push_member_target_kind(ctx, 1);
            ctx.load_value_to_result(target)?;
            super::types::emit_dynamic_object_class_name(ctx, name);
        }
        PhpType::Str => {
            push_member_target_kind(ctx, 0);
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(target, ptr_reg, len_reg)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            materialize_mixed_target_class_name(ctx, target, name)?;
        }
        PhpType::Callable => emit_closure_class_name_query(ctx),
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} target PHP type {:?} with dynamic member name",
                name, other
            )));
        }
    }
    Ok(())
}

/// Unboxes a gradual target and resolves its runtime object/string class query.
fn materialize_mixed_target_class_name(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    name: &str,
) -> Result<()> {
    let object_label = ctx.next_label("member_exists_dynamic_object");
    let string_label = ctx.next_label("member_exists_dynamic_string");
    let ready_label = ctx.next_label("member_exists_dynamic_target_ready");
    let closure_label = ctx.next_label("member_exists_dynamic_closure");
    ctx.load_value_to_result(target)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // boxed object targets resolve through their runtime class id
            ctx.emitter.instruction(&format!("b.eq {}", object_label));
            ctx.emitter.instruction("cmp x0, #1");                              // boxed string targets already carry a class-name query
            ctx.emitter.instruction(&format!("b.eq {}", string_label));
            ctx.emitter.instruction("cmp x0, #10");                             // a boxed callable is a Closure object
            ctx.emitter.instruction(&format!("b.eq {}", closure_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // boxed object targets resolve through their runtime class id
            ctx.emitter.instruction(&format!("je {}", object_label));
            ctx.emitter.instruction("cmp rax, 1");                              // boxed string targets already carry a class-name query
            ctx.emitter.instruction(&format!("je {}", string_label));
            ctx.emitter.instruction("cmp rax, 10");                             // a boxed callable is a Closure object
            ctx.emitter.instruction(&format!("je {}", closure_label));
        }
    }
    super::super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{}(): Argument #1 ($object_or_class) must be of type object|string, mixed given",
            name
        ),
    );

    ctx.emitter.label(&object_label);
    push_member_target_kind(ctx, 1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // object payload becomes get_class input
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // object payload becomes get_class input
    }
    super::types::emit_dynamic_object_class_name(ctx, name);
    abi::emit_jump(ctx.emitter, &ready_label);

    ctx.emitter.label(&closure_label);
    emit_closure_class_name_query(ctx);
    abi::emit_jump(ctx.emitter, &ready_label);

    ctx.emitter.label(&string_label);
    push_member_target_kind(ctx, 0);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rax, rdi");                                // move the unboxed string pointer into result convention
        ctx.emitter.instruction("mov rdx, rsi");                                // move the unboxed string length into result convention
    }
    ctx.emitter.label(&ready_label);
    Ok(())
}

/// Pushes the runtime registry list kind while preserving target ABI stack alignment.
fn push_member_target_kind(ctx: &mut FunctionContext<'_>, kind: i64) {
    let reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, reg, kind);
    abi::emit_push_reg(ctx.emitter, reg);
}

/// Calls `__rt_member_exists` with class/member strings and the requested registry list kind.
fn call_member_exists_registry(ctx: &mut FunctionContext<'_>, name: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // class-name pointer -> registry arg0
            ctx.emitter.instruction("mov x1, x2");                              // class-name length -> registry arg1
            abi::emit_pop_reg(ctx.emitter, "x4");                               // restore class-string/object method-list selector
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");                  // restore member pointer/length as args2/3
            if name == "property_exists" {
                abi::emit_load_int_immediate(ctx.emitter, "x4", 2);
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // class-name pointer -> registry arg0
            ctx.emitter.instruction("mov rsi, rdx");                            // class-name length -> registry arg1
            abi::emit_pop_reg(ctx.emitter, "r8");                               // restore class-string/object method-list selector
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");                // restore member pointer/length as args2/3
            if name == "property_exists" {
                abi::emit_load_int_immediate(ctx.emitter, "r8", 2);
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_member_exists");
}

/// Queries the member registry as for an object of class `Closure`: a callable value is one.
fn emit_closure_class_name_query(ctx: &mut FunctionContext<'_>) {
    push_member_target_kind(ctx, 1);
    let (label, length) = ctx.data.add_string(b"Closure");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, length as i64);
}
