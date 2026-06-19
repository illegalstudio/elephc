//! Purpose:
//! Lowers PHP diagnostic output builtins for the EIR backend.
//! Handles concrete scalar/resource values and array/hash shells without recursive dumps.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Output must match the legacy backend for the supported concrete types.
//! - Mixed dispatch follows the runtime tag/payload contract from `__rt_mixed_unbox`.
//! - Object dumps read the runtime class id from the object header and map it
//!   through the EIR module's class metadata.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `print_r(value)` for concrete scalar/resource values and array/hash shells.
pub(super) fn lower_print_r(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "print_r", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("print_r()");
    let value = expect_operand(inst, 0)?;
    let ty = loaded_php_semantic_type(ctx, value)?;
    emit_print_r_loaded_value(ctx, &ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `var_dump(value)` for concrete scalar/resource values and array/hash shells.
pub(super) fn lower_var_dump(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "var_dump", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("var_dump()");
    let value = expect_operand(inst, 0)?;
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
        PhpType::Mixed | PhpType::Union(_) => {
            emit_print_r_mixed(ctx);
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
fn emit_print_r_array(ctx: &mut FunctionContext<'_>, walker: &str) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
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
    abi::emit_call_label(ctx.emitter, "__rt_ftoa");
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
fn emit_var_dump_resource(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"resource(");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x0, x0, #1");                          // convert the resource payload into the displayed one-based id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rax, 1");                              // convert the resource payload into the displayed one-based id
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b") of type (stream)\n");
    Ok(())
}

/// Emits `var_dump` output for null, void, or never payloads.
fn emit_var_dump_null(ctx: &mut FunctionContext<'_>) {
    emit_write_literal(ctx, b"NULL\n");
}

/// Emits `var_dump` output for an array/hash payload in the integer result register.
fn emit_var_dump_array(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
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
    abi::emit_pop_reg(ctx.emitter, result_reg);
    if let Some(walker) = var_dump_array_walker(ty) {
        if matches!(ctx.emitter.target.arch, Arch::X86_64) {
            ctx.emitter.instruction("mov rdi, rax");                            // move the array pointer into the SysV first argument register
        }
        abi::emit_call_label(ctx.emitter, walker);
    }
    emit_write_literal(ctx, b"}\n");
    Ok(())
}

/// Returns the runtime var_dump walker for an array/hash element layout.
///
/// Homogeneous indexed arrays use a per-element-type walker; `Array(Mixed)` uses the
/// boxed-cell walker; associative arrays (hashes) use `__rt_var_dump_hash`, which iterates
/// entries and formats string/integer keys plus scalar values (nested containers fall back
/// to `NULL`, matching the indexed Mixed walker).
fn var_dump_array_walker(ty: &PhpType) -> Option<&'static str> {
    match ty {
        PhpType::Array(elem_ty) => match elem_ty.as_ref() {
            PhpType::Int => Some("__rt_var_dump_array_int"),
            PhpType::Str => Some("__rt_var_dump_array_str"),
            PhpType::Bool => Some("__rt_var_dump_array_bool"),
            PhpType::Float => Some("__rt_var_dump_array_float"),
            PhpType::Mixed => Some("__rt_var_dump_array_mixed"),
            _ => None,
        },
        PhpType::AssocArray { .. } => Some("__rt_var_dump_hash"),
        _ => None,
    }
}

/// Emits `var_dump` output for an object pointer in the integer result register.
fn emit_var_dump_dynamic_object(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let mut classes: Vec<_> = ctx
        .module
        .class_infos
        .iter()
        .map(|(class_name, class_info)| (class_name.clone(), class_info.class_id))
        .collect();
    classes.sort_by_key(|(_, class_id)| *class_id);
    let mut cases = Vec::with_capacity(classes.len());
    let null_label = ctx.next_label("var_dump_object_null");
    let fallback = ctx.next_label("var_dump_object_fallback");
    let done = ctx.next_label("var_dump_object_done");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", null_label));        // print NULL for defensive null object payloads
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the object's runtime class id from its header
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // check for defensive null object payloads
            ctx.emitter.instruction(&format!("je {}", null_label));             // print NULL for defensive null object payloads
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the object's runtime class id from its header
        }
    }
    for (class_name, class_id) in classes {
        let case = ctx.next_label("var_dump_object_case");
        cases.push((case.clone(), class_name));
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp x9, #{}", class_id));     // compare the runtime class id against a known class
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp r11, {}", class_id));     // compare the runtime class id against a known class
            }
        }
        emit_branch_if_eq(ctx, &case);
    }
    abi::emit_jump(ctx.emitter, &fallback);
    for (case, class_name) in cases {
        ctx.emitter.label(&case);
        emit_var_dump_object_name(ctx, &class_name);
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&null_label);
    emit_var_dump_null(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fallback);
    emit_write_literal(ctx, b"object\n");
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `object(ClassName)` output for a known runtime class name.
fn emit_var_dump_object_name(ctx: &mut FunctionContext<'_>, class_name: &str) {
    let text = format!("object({})\n", class_name);
    emit_write_literal(ctx, text.as_bytes());
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

/// Verifies that the builtin call has exactly the expected operand count.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}
