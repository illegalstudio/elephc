//! Purpose:
//! Lowers PHP diagnostic output builtins for the EIR backend.
//! Handles concrete scalar/resource values plus arrays and hashes, which dump
//! RECURSIVELY through the runtime walkers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Output must match PHP-compatible text for the supported concrete types.
//! - Mixed dispatch follows the runtime tag/payload contract from `__rt_mixed_unbox`.
//! - `var_dump` emits only the top-level `array(N) {` / `}` frame here; the body
//!   (and every nested container) comes from `codegen_support::runtime::io::
//!   var_dump_walk`, driven by the `_vd_indent` global this file sets to 2 for
//!   the duration of the walk. `print_r` passes its indent as a call argument
//!   instead — the two schemes are independent.
//! - `var_dump` of an object hands the instance to `__rt_var_dump_value` with
//!   the object tag, the same entry point a nested object reaches, so top-level
//!   and nested dumps share one renderer. The class name, per-property body and
//!   `*RECURSION*` guard all come from `codegen_support::runtime::io::
//!   var_dump_object`. An ENUM case is intercepted inside that shared renderer
//!   and printed as `enum(E::C)`, so this file needs no enum-specific arm.
//! - `print_r` of an object works the same way: `__rt_print_r_object` in
//!   `codegen_support::runtime::objects::print_r_object` owns the header, the
//!   parenthesized body and the recursion guard, and is reached both from here
//!   (base indent 0) and from the tag-6 branch of `__rt_print_r_value`.

use crate::codegen::abi;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `print_r(value, $return = false)` for concrete scalar/resource values
/// and array/hash shells.
///
/// Dispatch follows the call's static result type, which the checker
/// (`src/builtins/io/print_r.rs`) and the EIR return-type override
/// (`print_r_builtin_return_type_for_args`) derive from the `$return` flag:
/// - `Str` (literal `true`): render into the capture buffer and return the owned
///   string finalized by `__rt_pr_finish`.
/// - `Bool` (flag absent or literal `false`): render to stdout and return `true`.
/// - `Mixed` (runtime flag): select the mode at runtime; see
///   `lower_print_r_runtime_flag`.
pub(crate) fn lower_print_r(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::unsupported(
            "print_r() takes 1 or 2 arguments",
        ));
    }
    let value = expect_operand(inst, 0)?;
    match inst.result_php_type.codegen_repr() {
        PhpType::Str => {
            ctx.emitter.blank();
            ctx.emitter.comment("print_r(value, true) — return mode");
            // -- reset the capture offset and enable buffer mode --
            let zero_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, zero_reg, 0);
            abi::emit_store_reg_to_symbol(ctx.emitter, zero_reg, "_print_r_off", 0);
            abi::emit_load_int_immediate(ctx.emitter, zero_reg, 1);
            abi::emit_store_reg_to_symbol(ctx.emitter, zero_reg, "_print_r_mode", 0);
            // -- load the value into result regs and render it into the buffer --
            let ty = loaded_php_semantic_type(ctx, value)?;
            emit_print_r_loaded_value(ctx, &ty)?;
            // -- finalize the captured bytes into an owned heap string --
            abi::emit_call_label(ctx.emitter, "__rt_pr_finish");
            // -- result is in the platform string result regs (x1/x2 or rax/rdx) --
            store_if_result(ctx, inst)
        }
        PhpType::Bool => {
            ctx.emitter.blank();
            ctx.emitter.comment("print_r()");
            let ty = loaded_php_semantic_type(ctx, value)?;
            emit_print_r_loaded_value(ctx, &ty)?;
            // PHP `print_r` echo mode always returns true, regardless of the bytes
            // written. The rendering above leaves the syscall/byte-count in the
            // integer result register, so materialize a literal 1 before storing.
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
            store_if_result(ctx, inst)
        }
        _ => lower_print_r_runtime_flag(ctx, inst, value),
    }
}

/// Lowers `print_r(value, $flag)` when the `$return` flag is only known at runtime.
///
/// The flag (0/1) is stored into `_print_r_mode` before rendering, so the shared
/// write indirection (`__rt_pr_write` / `__rt_stdout_write`) routes every rendered
/// byte to stdout (echo mode) or the capture buffer (return mode) on its own. A
/// final branch on the stored mode boxes the result as `Mixed`: the finalized
/// capture string (tag 1) in return mode, or PHP's `true` (tag 3) in echo mode —
/// the call's static result type is `Mixed` because the value shape depends on the
/// runtime flag. A missing flag operand (first-class-callable wrappers lower the
/// one-argument form with a `Mixed` result type) defaults to echo mode, matching
/// PHP's `$return = false`. `__rt_pr_finish` resets the mode and offset; the echo
/// branch leaves them untouched (the stored flag was zero).
///
/// OWNERSHIP: the capture string from `__rt_pr_finish` is owned solely by this lowering.
/// `__rt_mixed_from_value` copies it into the box instead of adopting it, and the EIR
/// `release` for the call site frees the Mixed cell rather than the intermediate, so the
/// return-mode branch frees the capture string itself once the box holds its own copy.
fn lower_print_r_runtime_flag(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    value: ValueId,
) -> Result<()> {
    ctx.emitter.blank();
    ctx.emitter.comment("print_r(value, $flag) — runtime-selected mode");
    // -- reset the capture offset, then store the flag as the capture mode --
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, "_print_r_off", 0);
    match inst.operands.get(1).copied() {
        Some(flag) => {
            let flag_ty = ctx.load_value_to_reg(flag, result_reg)?;
            if !matches!(flag_ty.codegen_repr(), PhpType::Bool | PhpType::Int) {
                return Err(CodegenIrError::unsupported(format!(
                    "print_r $return flag for PHP type {:?}",
                    flag_ty
                )));
            }
        }
        None => abi::emit_load_int_immediate(ctx.emitter, result_reg, 0),
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, "_print_r_mode", 0);
    // -- render the value; the write indirection consults the mode per write --
    let ty = loaded_php_semantic_type(ctx, value)?;
    emit_print_r_loaded_value(ctx, &ty)?;
    // -- branch on the stored mode: finalize the capture or materialize `true` --
    let echo_label = ctx.next_label("print_r_runtime_echo");
    let done_label = ctx.next_label("print_r_runtime_done");
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, "_print_r_mode", 0);
    emit_compare_reg_zero(ctx, result_reg);
    emit_branch_if_eq(ctx, &echo_label);
    abi::emit_call_label(ctx.emitter, "__rt_pr_finish");
    // `__rt_pr_finish` hands back a freshly allocated heap string, and the string arm of
    // `__rt_mixed_from_value` persists (copies) that payload into the box's own storage rather
    // than adopting it. The capture string therefore has exactly one owner — this lowering — and
    // nothing downstream can free it, because the EIR `release` for this call site targets the
    // Mixed cell, not the intermediate. Free it here, once the box owns its copy, or every
    // runtime-flag `print_r($v, $flag)` leaks one block per call.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // spill the capture string pointer across the boxing call
            ctx.emitter.instruction("mov x0, #1");                              // runtime tag 1 = string for the captured bytes
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction("ldr x1, [sp]");                            // reload the capture string pointer
            ctx.emitter.instruction("str x0, [sp]");                            // park the boxed Mixed result across the free helper
            ctx.emitter.instruction("mov x0, x1");                              // pass the capture string to the validating heap-free helper
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // restore the boxed Mixed result and drop the spill slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // captured string pointer → Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // captured string length → Mixed high payload word
            ctx.emitter.instruction("sub rsp, 16");                             // reserve an aligned spill slot pair
            ctx.emitter.instruction("mov QWORD PTR [rsp], rdi");                // spill the capture string pointer across the boxing call
            ctx.emitter.instruction("mov eax, 1");                              // runtime tag 1 = string for the captured bytes
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction("mov QWORD PTR [rsp+8], rax");              // park the boxed Mixed result across the free helper
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // pass the capture string to the validating heap-free helper
            abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp+8]");              // restore the boxed Mixed result
            ctx.emitter.instruction("add rsp, 16");                             // release the spill slot pair
        }
    }
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&echo_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #1");                              // PHP echo mode always returns true
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 1");                              // PHP echo mode always returns true
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = boolean
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `var_dump(value, ...values)` for concrete scalar/resource values and for
/// arrays/hashes, which are dumped recursively (nested containers included).
/// Each operand is dumped independently in source order, matching PHP's variadic var_dump.
pub(crate) fn lower_var_dump(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(
            "var_dump() requires at least 1 argument",
        ));
    }
    for (index, operand) in inst.operands.iter().enumerate() {
        ctx.emitter.blank();
        if index > 0 {
            ctx.emitter.comment(&format!("var_dump() — argument {}", index + 1));
        } else {
            ctx.emitter.comment("var_dump()");
        }
        let value = *operand;
        let ty = loaded_php_semantic_type(ctx, value)?;
        match &ty {
            PhpType::Int => emit_var_dump_int(ctx),
            PhpType::TaggedScalar => emit_var_dump_tagged_scalar(ctx),
            PhpType::Float => emit_var_dump_float(ctx),
            PhpType::Str => emit_var_dump_string(ctx),
            PhpType::Bool => emit_var_dump_bool(ctx),
            PhpType::Resource(_) => emit_var_dump_resource(ctx),
            PhpType::Void | PhpType::Never => {
                emit_var_dump_null(ctx);
                Ok(())
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
                emit_var_dump_array(ctx, &ty)
            }
            PhpType::Object(_) => emit_var_dump_dynamic_object(ctx),
            PhpType::Mixed | PhpType::Union(_) => emit_var_dump_mixed(ctx),
            other => Err(CodegenIrError::unsupported(format!(
                "var_dump for PHP type {:?}",
                other
            ))),
        }?;
    }
    store_if_result(ctx, inst)
}

/// Loads a value and returns the PHP type needed for user-visible debug output.
fn loaded_php_semantic_type(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
) -> Result<PhpType> {
    let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        Ok(raw_ty)
    } else {
        Ok(loaded_ty)
    }
}

/// Emits `print_r` output for the value currently loaded in result register(s).
fn emit_print_r_loaded_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    match ty {
        PhpType::Void | PhpType::Never => Ok(()),
        PhpType::Bool => {
            let skip_label = ctx.next_label("print_r_skip_false");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Array(_) => emit_print_r_array(ctx, "__rt_print_r_indexed"),
        PhpType::AssocArray { .. } => emit_print_r_array(ctx, "__rt_print_r_hash"),
        PhpType::Iterable => {
            // Iterable's runtime representation is ambiguous (a direct indexed
            // array or a hash), so render only the `Array\n` header rather than
            // risk walking the wrong layout.
            emit_write_literal(ctx, b"Array\n");
            Ok(())
        }
        PhpType::Object(_) => {
            emit_print_r_object(ctx);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_print_r_mixed(ctx);
            Ok(())
        }
        PhpType::Object(_) => {
            emit_print_r_object(ctx);
            Ok(())
        }
        PhpType::TaggedScalar => emit_print_r_tagged_scalar(ctx),
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_write_stdout(ctx.emitter, ty);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "print_r for PHP type {:?}",
            other
        ))),
    }
}

/// Emits `print_r()` output for the current object result through concrete
/// runtime `__debugInfo()` dispatch, with a top-level parenthesis indent of zero.
fn emit_print_r_object(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // top-level object parentheses start at column zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // object pointer → SysV first argument register
            ctx.emitter.instruction("mov esi, 0");                              // top-level object parentheses start at column zero
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_print_r_object");
}

/// Emits `print_r` output for a tagged scalar, matching PHP's empty output for null.
fn emit_print_r_tagged_scalar(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let skip_label = ctx.next_label("print_r_skip_tagged_null");
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &skip_label);
    abi::emit_write_stdout(ctx.emitter, &PhpType::Int);
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Emits `print_r` output for an array/hash: the `Array\n` header followed by
/// the recursive `(\n ... )\n` body emitted by the runtime `walker`. The array
/// pointer is preserved across the header write (the write syscall clobbers the
/// integer result register), then passed with a base indent of 0.
///
/// Both null container shapes — the zero pointer and the in-band null-container
/// sentinel a missed element read materializes — skip the whole rendering (issue
/// #647). This lowering previously had no null branch at all, so `Array` was
/// written and the sentinel was then handed to the walker as a live container.
/// The guard jumps PAST the header write as well as the walk: PHP renders null as
/// EMPTY output here, unlike `var_dump`, which prints `NULL`. Skipping every write
/// is what makes all three modes correct at once — echo mode prints nothing and
/// still returns `true`, and the capture modes leave the buffer empty so
/// `print_r($null, true)` finalizes to `""`.
fn emit_print_r_array(ctx: &mut FunctionContext<'_>, walker: &str) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let skip_label = ctx.next_label("print_r_skip_null_array");
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        result_reg,
        scratch_reg,
        &skip_label,
    );
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"Array\n");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // base indent = 0 for the top-level array
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // array pointer → SysV first argument register
            ctx.emitter.instruction("mov esi, 0");                              // base indent = 0 for the top-level array
        }
    }
    abi::emit_call_label(ctx.emitter, walker);
    ctx.emitter.label(&skip_label);
    Ok(())
}

/// Emits `print_r` output for a boxed Mixed payload by delegating to the runtime
/// `__rt_print_r_value` single-value renderer with tag 7 (Mixed cell) and a base
/// indent of 0, so a held array prints its full body and a held scalar prints
/// raw (PHP `print_r` semantics: no type wrapper, `1`/empty for bool, empty for null).
fn emit_print_r_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // boxed Mixed cell pointer → value low argument
            ctx.emitter.instruction("mov x0, #7");                              // tag 7 = boxed Mixed cell
            ctx.emitter.instruction("mov x2, #0");                              // high word unused for the cell pointer
            ctx.emitter.instruction("mov x3, #0");                              // nested base indent = 0
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // boxed Mixed cell pointer → value low argument
            ctx.emitter.instruction("mov edi, 7");                              // tag 7 = boxed Mixed cell
            ctx.emitter.instruction("mov edx, 0");                              // high word unused for the cell pointer
            ctx.emitter.instruction("mov ecx, 0");                              // nested base indent = 0
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_print_r_value");
}

/// Emits `print_r` output for an object pointer in the integer result register.
///
/// Hands the instance to `__rt_print_r_object` with a base indent of 0 — the SAME
/// entry point a nested object reaches from the array, hash and object walkers, so
/// a top-level render and a render at depth cannot drift apart. That helper owns
/// the whole layout: the `ClassName Object` header (or PHP's `ClassName Enum[:t]`
/// for an enum case), the `(` / `)` lines, the per-property body and the
/// `*RECURSION*` guard. A zero pointer or the in-band null-container sentinel from
/// a missed object read skips the walker entirely, matching `print_r(null)`.
fn emit_print_r_object(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let skip_label = ctx.next_label("print_r_skip_null_object");
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        result_reg,
        scratch_reg,
        &skip_label,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // base indent = 0 for the top-level object
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // object pointer → SysV first argument register
            ctx.emitter.instruction("mov esi, 0");                              // base indent = 0 for the top-level object
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_print_r_object");
    ctx.emitter.label(&skip_label);
}

/// Emits `var_dump` output for a boxed Mixed payload in the integer result register.
fn emit_var_dump_mixed(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let int_case = ctx.next_label("var_dump_mixed_int");
    let string_case = ctx.next_label("var_dump_mixed_string");
    let float_case = ctx.next_label("var_dump_mixed_float");
    let bool_case = ctx.next_label("var_dump_mixed_bool");
    let resource_case = ctx.next_label("var_dump_mixed_resource");
    let array_case = ctx.next_label("var_dump_mixed_array");
    let assoc_case = ctx.next_label("var_dump_mixed_assoc");
    let object_case = ctx.next_label("var_dump_mixed_object");
    let null_case = ctx.next_label("var_dump_mixed_null");
    let done = ctx.next_label("var_dump_mixed_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_mixed_tag(ctx, 0, &int_case);
    emit_branch_on_mixed_tag(ctx, 1, &string_case);
    emit_branch_on_mixed_tag(ctx, 2, &float_case);
    emit_branch_on_mixed_tag(ctx, 3, &bool_case);
    emit_branch_on_mixed_tag(ctx, 9, &resource_case);
    emit_branch_on_mixed_tag(ctx, 4, &array_case);
    emit_branch_on_mixed_tag(ctx, 5, &assoc_case);
    emit_branch_on_mixed_tag(ctx, 6, &object_case);
    abi::emit_jump(ctx.emitter, &null_case);

    ctx.emitter.label(&int_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_int(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&string_case);
    move_mixed_payload_to_string_result(ctx);
    emit_var_dump_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&float_case);
    move_mixed_payload_to_float_result(ctx);
    emit_var_dump_float(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&bool_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_bool(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&resource_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_resource(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&array_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_array(ctx, &PhpType::Array(Box::new(PhpType::Mixed)))?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&assoc_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_array(
        ctx,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
    )?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_dynamic_object(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null_case);
    emit_var_dump_null(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `var_dump` output for an integer payload in the integer result register.
fn emit_var_dump_int(ctx: &mut FunctionContext<'_>) -> Result<()> {
    if crate::codegen::sentinels::null_repr_is_tagged() {
        emit_var_dump_int_payload(ctx);
        return Ok(());
    }
    let not_null = ctx.next_label("var_dump_not_null");
    let done = ctx.next_label("var_dump_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    let scratch_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, crate::codegen::sentinels::NULL_SENTINEL);
    emit_compare_regs(ctx, result_reg, scratch_reg);
    emit_branch_if_ne(ctx, &not_null);
    emit_var_dump_null(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&not_null);
    emit_var_dump_int_payload(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `var_dump` output for a tagged scalar payload/tag pair in the result registers.
fn emit_var_dump_tagged_scalar(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let null_case = ctx.next_label("var_dump_tagged_null");
    let done = ctx.next_label("var_dump_tagged_done");
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_case);
    emit_var_dump_int_payload(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&null_case);
    emit_var_dump_null(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `int(N)` for the integer payload in the result register without a null check.
fn emit_var_dump_int_payload(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"int(");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b")\n");
}

/// Emits `var_dump` output for a float payload in the floating result register.
fn emit_var_dump_float(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_ftoa_repr");
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_literal(ctx, b"float(");
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b")\n");
    Ok(())
}

/// Emits `var_dump` output for a string payload in the string result register pair.
fn emit_var_dump_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_literal(ctx, b"string(");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // load the preserved string length for decimal formatting
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // load the preserved string length for decimal formatting
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b") \"");
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b"\"\n");
    Ok(())
}

/// Emits `var_dump` output for a boolean payload in the integer result register.
fn emit_var_dump_bool(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let true_label = ctx.next_label("var_dump_true");
    let done = ctx.next_label("var_dump_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    emit_compare_reg_zero(ctx, result_reg);
    emit_branch_if_nonzero(ctx, &true_label);
    emit_write_literal(ctx, b"bool(false)\n");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&true_label);
    emit_write_literal(ctx, b"bool(true)\n");
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `var_dump` output for a stream/generic resource payload.
///
/// The number comes from the resource-id registry, never from the payload. The
/// previous `payload + 1` happened to look right for a descriptor and printed a
/// raw heap address for anything else — `var_dump(hash_init('md5'))` rendered
/// `resource(4318862849) of type (stream)`, a different number on every run.
///
/// The type name comes from `__rt_resource_type_name`, never from a literal. It used
/// to be baked into the single closing literal `") of type (stream)\n"`, so a closed
/// handle kept advertising its original type where PHP 8.5.6 prints
/// `resource(5) of type (Unknown)`. Splitting that literal in two is what lets the
/// runtime-computed name sit between the halves.
///
/// The payload must therefore survive `__rt_resource_id_of`, which consumes it, so it
/// is pushed a second time. `abi::emit_push_reg` reserves 16 bytes on both targets, so
/// nesting the two pushes keeps the stack aligned across the intervening calls; the
/// two pops below balance the two pushes exactly.
fn emit_var_dump_resource(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_var_dump_resource_asm(ctx.emitter, ctx.data);
    Ok(())
}

/// Emits the WHOLE `resource(N) of type (T)` line, split out of `emit_var_dump_resource`
/// so both target variants can be pinned at assembly level without a `FunctionContext`
/// (the precedent is `emit_resource_release_sentinel` in
/// `crate::codegen::lower_inst::builtins::io`).
///
/// Splitting only the type-name FIELD out was not enough: a sentinel that deleted the
/// call from the line and restored the old single literal left every field-level pin
/// green, because the field helper itself was untouched. The whole line therefore lives
/// here, where a pin sees the literals and the call together — the only shape that fails
/// when the call site is removed rather than when the callee is broken. `ctx` supplies
/// nothing else, so this loses no information.
///
/// Entry state: the native resource payload is in the int result register. Exit state:
/// the line has been written and the stack is balanced — two pushes, two pops.
fn emit_var_dump_resource_asm(emitter: &mut Emitter, data: &mut DataSection) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);
    emit_literal_to_stdout(emitter, data, b"resource(");
    abi::emit_pop_reg(emitter, result_reg);
    abi::emit_push_reg(emitter, result_reg);                                    // __rt_resource_id_of consumes the register copy; the name needs it back
    abi::emit_call_label(emitter, "__rt_resource_id_of");
    abi::emit_call_label(emitter, "__rt_itoa");
    abi::emit_write_stdout(emitter, &PhpType::Str);
    emit_literal_to_stdout(emitter, data, b") of type (");
    abi::emit_pop_reg(emitter, result_reg);                                     // recover the payload __rt_resource_id_of consumed
    abi::emit_call_label(emitter, "__rt_resource_type_name");                   // stream while the handle is open, Unknown once it is closed
    abi::emit_write_stdout(emitter, &PhpType::Str);
    emit_literal_to_stdout(emitter, data, b")\n");
}

/// Writes a compile-time literal byte string to stdout without a `FunctionContext`.
fn emit_literal_to_stdout(emitter: &mut Emitter, data: &mut DataSection, bytes: &[u8]) {
    let (label, len) = data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);
    abi::emit_write_stdout(emitter, &PhpType::Str);
}

/// Emits `var_dump` output for null, void, or never payloads.
fn emit_var_dump_null(ctx: &mut FunctionContext<'_>) {
    emit_write_literal(ctx, b"NULL\n");
}

/// Emits `var_dump` output for an array/hash payload in the integer result register.
///
/// Writes the `array(N) {\n` header and the closing `}\n` around the runtime body
/// walk. The body lines are indented by the runtime through the `_vd_indent`
/// global: it is set to 2 (PHP's one nesting level) for the duration of the walk
/// and back to 0 before the closing brace, which sits at the header's indent.
/// Nested containers manage their own deeper indents inside `__rt_var_dump_value`.
///
/// The container payload is checked for BOTH null shapes before the header walk
/// (issue #581). A zero pointer is what an untyped null-defaulted property rebound
/// to array storage reads before its first write. The in-band null-container
/// sentinel is what a missed element read materializes: it is non-zero, so the
/// plain zero check let it through and the header load dereferenced it after
/// `array(` had already been written. PHP dumps both as `NULL`.
fn emit_var_dump_array(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let null_label = ctx.next_label("var_dump_array_null");
    let done_label = ctx.next_label("var_dump_array_done");
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    crate::codegen::sentinels::emit_branch_if_null_container(
        ctx.emitter,
        result_reg,
        scratch_reg,
        &null_label,
    );
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"array(");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_push_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the array or hash element count from the heap header
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the array or hash element count from the heap header
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b") {\n");
    // -- body lines sit one PHP nesting level (2 spaces) in --
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 2);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, "_vd_indent", 0);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    if let Some(walker) = var_dump_array_walker(ty) {
        if matches!(ctx.emitter.target.arch, Arch::X86_64) {
            ctx.emitter.instruction("mov rdi, rax");                            // move the array pointer into the SysV first argument register
        }
        abi::emit_call_label(ctx.emitter, walker);
    }
    // -- the closing brace aligns with the header, so drop back to column 0 --
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, "_vd_indent", 0);
    emit_write_literal(ctx, b"}\n");
    ctx.emit_branch(&done_label);
    ctx.emitter.label(&null_label);
    emit_var_dump_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Returns the runtime var_dump walker for an array/hash element layout.
///
/// Homogeneous indexed arrays of a scalar element type keep their per-element-type
/// walker (nothing there can nest). EVERY other indexed element type — `Mixed`
/// boxed cells, nested `Array`/`AssocArray`, objects — goes through
/// `__rt_var_dump_indexed`, which self-dispatches on the array's runtime
/// value_type stamp and recurses through `__rt_var_dump_value`. Associative
/// arrays (hashes) use `__rt_var_dump_hash`, which iterates entries, formats
/// string/integer keys and delegates each value to the same recursive renderer.
///
/// `Iterable` is deliberately absent: its runtime representation is ambiguous
/// (a direct indexed array or a hash), so the body is left empty rather than
/// risking a walk of the wrong layout — the same choice `print_r` makes.
fn var_dump_array_walker(ty: &PhpType) -> Option<&'static str> {
    match ty {
        PhpType::Array(elem_ty) => match elem_ty.as_ref() {
            PhpType::Int => Some("__rt_var_dump_array_int"),
            PhpType::Str => Some("__rt_var_dump_array_str"),
            PhpType::Bool => Some("__rt_var_dump_array_bool"),
            PhpType::Float => Some("__rt_var_dump_array_float"),
            _ => Some("__rt_var_dump_indexed"),
        },
        PhpType::AssocArray { .. } => Some("__rt_var_dump_hash"),
        _ => None,
    }
}

/// Emits `var_dump` output for an object pointer in the integer result register.
///
/// Hands the instance straight to `__rt_var_dump_value` with the object tag (6),
/// which is the SAME entry point a nested object reaches from the array, hash and
/// object walkers — so a top-level dump and a dump at depth cannot drift apart.
/// That branch owns the whole rendering: the `object(C) (n) {` header, the
/// per-property body, the closing `}`, and the `*RECURSION*` guard.
///
/// `_vd_indent` is reset to 0 first so the header sits at column 0. The runtime
/// walkers restore the indent themselves around every nested container, but a
/// preceding dump that aborted mid-walk would otherwise leak its indent into
/// this one.
fn emit_var_dump_dynamic_object(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, "_vd_indent", 0);
    abi::emit_pop_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // object pointer → the value renderer's low payload word
            ctx.emitter.instruction("mov x2, #0");                              // objects carry no high payload word
            ctx.emitter.instruction("mov x0, #6");                              // runtime value tag 6 = object
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // object pointer → the value renderer's low payload word
            ctx.emitter.instruction("xor rdx, rdx");                            // objects carry no high payload word
            ctx.emitter.instruction("mov rdi, 6");                              // runtime value tag 6 = object
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_var_dump_value");
    Ok(())
}

/// Writes a compile-time literal byte string to stdout.
fn emit_write_literal(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    emit_write_current_string(ctx);
}

/// Writes the current string result register pair to stdout.
fn emit_write_current_string(ctx: &mut FunctionContext<'_>) {
    abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
}

/// Branches to `label` when the unboxed Mixed tag equals `tag`.
fn emit_branch_on_mixed_tag(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed runtime tag against this formatter case
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching Mixed formatter case
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed runtime tag against this formatter case
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching Mixed formatter case
        }
    }
}

/// Moves the unboxed Mixed low payload word into the integer result register.
fn move_mixed_payload_to_int_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed Mixed low payload into the integer result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed Mixed low payload into the integer result register
        }
    }
}

/// Moves the unboxed Mixed string payload words into the string result register pair.
fn move_mixed_payload_to_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed Mixed string pointer into the string result register
        }
    }
}

/// Moves the unboxed Mixed float bits into the floating-point result register.
fn move_mixed_payload_to_float_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d0, x1");                             // reinterpret the unboxed Mixed payload bits as the float result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq xmm0, rdi");                          // reinterpret the unboxed Mixed payload bits as the float result
        }
    }
}

/// Emits a comparison between two general-purpose registers.
fn emit_compare_regs(ctx: &mut FunctionContext<'_>, lhs: &str, rhs: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs, rhs));          // compare two integer-like register payloads
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs, rhs));          // compare two integer-like register payloads
        }
    }
}

/// Emits a comparison between a general-purpose register and zero.
fn emit_compare_reg_zero(ctx: &mut FunctionContext<'_>, reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", reg));               // compare the integer-like register payload against zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", reg));                // compare the integer-like register payload against zero
        }
    }
}

/// Emits a branch when the previous comparison was non-zero/non-equal.
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.ne {}", label));                // branch when the compared integer-like payload is non-zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jne {}", label));                 // branch when the compared integer-like payload is non-zero
        }
    }
}

/// Emits a branch when the previous comparison found different values.
fn emit_branch_if_ne(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.ne {}", label));                // branch when the compared register payloads are different
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jne {}", label));                 // branch when the compared register payloads are different
        }
    }
}

/// Emits a branch when the previous comparison found equal values.
fn emit_branch_if_eq(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch when the compared register payloads are equal
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("je {}", label));                  // branch when the compared register payloads are equal
        }
    }
}

#[cfg(test)]
mod var_dump_resource_line_tests {
    use super::emit_var_dump_resource_asm;
    use crate::codegen::data_section::DataSection;
    use crate::codegen::emit::Emitter;
    use crate::codegen::platform::{Arch, Platform, Target};

    /// Emits the whole `var_dump` resource line for one target.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        emit_var_dump_resource_asm(&mut emitter, &mut data);
        emitter.output()
    }

    /// Pins the whole AArch64 line as an ordered call/branch sequence.
    ///
    /// The load-bearing pair is `bl __rt_resource_type_name` sitting BETWEEN two literal
    /// writes. Before the fix the line ended in one 19-byte literal
    /// `") of type (stream)\n"`, so a closed handle kept advertising `stream` where PHP
    /// 8.5.6 prints `(Unknown)`. Pinning the field helper alone was NOT enough: a sentinel
    /// that deleted the call from this line and restored the old literal left the field
    /// helper untouched and every field-level pin green.
    #[test]
    fn aarch64_writes_the_type_name_between_the_two_closing_literals() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        let expected = concat!(
            "    str x0, [sp, #-16]!\n",
            "    adrp x1, _str_0@PAGE\n",
            "    add x1, x1, _str_0@PAGEOFF\n",
            "    mov x2, #9\n",
            "    mov x0, x1\n",
            "    mov x1, x2\n",
            "    bl __rt_stdout_write\n",
            "    ldr x0, [sp], #16\n",
            "    str x0, [sp, #-16]!\n",
            "    bl __rt_resource_id_of\n",
            "    bl __rt_itoa\n",
            "    mov x0, x1\n",
            "    mov x1, x2\n",
            "    bl __rt_stdout_write\n",
            "    adrp x1, _str_1@PAGE\n",
            "    add x1, x1, _str_1@PAGEOFF\n",
            "    mov x2, #11\n",
            "    mov x0, x1\n",
            "    mov x1, x2\n",
            "    bl __rt_stdout_write\n",
            "    ldr x0, [sp], #16\n",
            "    bl __rt_resource_type_name\n",
            "    mov x0, x1\n",
            "    mov x1, x2\n",
            "    bl __rt_stdout_write\n",
            "    adrp x1, _str_2@PAGE\n",
            "    add x1, x1, _str_2@PAGEOFF\n",
            "    mov x2, #2\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Pins the same line on x86_64, where the pushes and pops are two-instruction
    /// sequences and the payload register doubles as the string-result pointer. An
    /// aarch64-only pin has already let an x86 fix be deleted silently on this branch.
    #[test]
    fn x86_64_writes_the_type_name_between_the_two_closing_literals() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        let expected = concat!(
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], rax\n",
            "    lea rax, [rip + _str_0]\n",
            "    mov rdx, 9\n",
            "    mov rsi, rdx\n",
            "    mov rdi, rax\n",
            "    call __rt_stdout_write\n",
            "    mov rax, QWORD PTR [rsp]\n",
            "    add rsp, 16\n",
            "    sub rsp, 16\n",
            "    mov QWORD PTR [rsp], rax\n",
            "    call __rt_resource_id_of\n",
            "    call __rt_itoa\n",
            "    mov rsi, rdx\n",
            "    mov rdi, rax\n",
            "    call __rt_stdout_write\n",
            "    lea rax, [rip + _str_1]\n",
            "    mov rdx, 11\n",
            "    mov rsi, rdx\n",
            "    mov rdi, rax\n",
            "    call __rt_stdout_write\n",
            "    mov rax, QWORD PTR [rsp]\n",
            "    add rsp, 16\n",
            "    call __rt_resource_type_name\n",
            "    mov rsi, rdx\n",
            "    mov rdi, rax\n",
            "    call __rt_stdout_write\n",
            "    lea rax, [rip + _str_2]\n",
            "    mov rdx, 2\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// The three literals must be exactly `resource(`, `) of type (` and `)\n` — and in
    /// particular the pre-fix 19-byte `") of type (stream)\n"` must be gone.
    ///
    /// A pin on the assembly alone cannot see this: the literals live in the data section
    /// and the code only names `_str_N`. Restoring the old single literal would leave the
    /// instruction stream shorter but still plausible.
    #[test]
    fn the_line_interns_no_hardcoded_type_name_on_either_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            emit_var_dump_resource_asm(&mut emitter, &mut data);
            assert_eq!(
                data.add_string(b"resource(").0,
                "_str_0",
                "the first literal must be the opening one ({target:?})"
            );
            assert_eq!(
                data.add_string(b") of type (").0,
                "_str_1",
                "the second literal must stop before the type name ({target:?})"
            );
            assert_eq!(
                data.add_string(b")\n").0,
                "_str_2",
                "the third literal must be the closing paren alone ({target:?})"
            );
            // `add_string` interns: a label of `_str_3` means this byte string was NOT
            // already in the section, i.e. the pre-fix hardcoded literal is gone. Had it
            // still been emitted, the call would have returned its existing label.
            assert_eq!(
                data.add_string(b") of type (stream)\n").0,
                "_str_3",
                "the pre-fix hardcoded type literal must not be interned ({target:?})"
            );
        }
    }

    /// The line must push and pop exactly twice on both targets.
    ///
    /// The payload is stashed once around the `resource(` write and once around
    /// `__rt_resource_id_of`, which consumes the register copy. An unbalanced pop here
    /// corrupts the caller frame silently on macOS.
    #[test]
    fn the_line_balances_two_pushes_with_two_pops_on_both_targets() {
        for (target, push, pop) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "str x0, [sp, #-16]!",
                "ldr x0, [sp], #16",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "mov QWORD PTR [rsp], rax",
                "mov rax, QWORD PTR [rsp]",
            ),
        ] {
            let asm = emit_for(target);
            assert_eq!(
                asm.matches(push).count(),
                2,
                "exactly two pushes expected ({target:?}):\n{asm}"
            );
            assert_eq!(
                asm.matches(pop).count(),
                2,
                "exactly two pops expected ({target:?}):\n{asm}"
            );
        }
    }
}
