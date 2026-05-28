//! Purpose:
//! Lowers array unpacking assignments from source arrays into destination arrays.
//! Handles statement-level array mutation after expression operands are evaluated.
//!
//! Called from:
//! - `crate::codegen::stmt::arrays`
//!
//! Key details:
//! - Mutation paths must preserve source-order side effects and update heap ownership consistently.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `list($a, $b, ...) = $array` unpack from a source indexed-array expression.
/// Loads each element from the source array by numeric index and stores it into the
/// corresponding destination variable's stack slot. The source array pointer is preserved
/// on the stack across all assignments. Element types are inferred from the source array's
/// element type; unknown types default to integer. Clobbers `x0`/`rax`, `x9`/`r11`.
pub(super) fn emit_list_unpack_stmt(
    vars: &[String],
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("list unpack");

    let arr_ty = emit_expr(value, emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the source indexed-array pointer while each unpack target local is assigned from its element slot

    for (i, var_name) in vars.iter().enumerate() {
        let var = match ctx.variables.get(var_name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                continue;
            }
        };
        let offset = var.stack_offset;

        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp]");                            // peek the preserved indexed-array pointer from the temporary stack slot before loading the requested unpack element
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction("add x9, x9, #24");                 // skip the fixed indexed-array header before addressing the scalar payload region
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", i * 8)); // load the requested scalar unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "x0", offset);
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("add x9, x9, #{}", 24 + i * 16)); // advance from the indexed-array base to the selected 16-byte string slot
                        emitter.instruction("ldr x1, [x9]");                    // load the requested unpack string pointer from the selected indexed-array slot
                        emitter.instruction("ldr x2, [x9, #8]");                // load the requested unpack string length from the selected indexed-array slot
                        abi::store_at_offset(emitter, "x1", offset);
                        abi::store_at_offset(emitter, "x2", offset - 8);
                    }
                    PhpType::Float => {
                        emitter.instruction("add x9, x9, #24");                 // skip the fixed indexed-array header before addressing the floating payload region
                        emitter.instruction(&format!("ldr d0, [x9, #{}]", i * 8)); // load the requested floating unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "d0", offset);
                    }
                    _ => {
                        emitter.instruction("add x9, x9, #24");                 // skip the fixed indexed-array header before addressing the pointer-like payload region
                        emitter.instruction(&format!("ldr x0, [x9, #{}]", i * 8)); // load the requested pointer-like unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "x0", offset);
                    }
                }
            }
            Arch::X86_64 => {
                emitter.instruction("mov r11, QWORD PTR [rsp]");                // peek the preserved indexed-array pointer from the temporary stack slot before loading the requested unpack element
                match &elem_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction("add r11, 24");                     // skip the fixed indexed-array header before addressing the scalar payload region
                        emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", i * 8)); // load the requested scalar unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "rax", offset);
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("add r11, {}", 24 + i * 16)); // advance from the indexed-array base to the selected 16-byte string slot
                        emitter.instruction("mov rax, QWORD PTR [r11]");        // load the requested unpack string pointer from the selected indexed-array slot
                        emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");    // load the requested unpack string length from the selected indexed-array slot
                        abi::store_at_offset(emitter, "rax", offset);
                        abi::store_at_offset(emitter, "rdx", offset - 8);
                    }
                    PhpType::Float => {
                        emitter.instruction("add r11, 24");                     // skip the fixed indexed-array header before addressing the floating payload region
                        emitter.instruction(&format!("movsd xmm0, QWORD PTR [r11 + {}]", i * 8)); // load the requested floating unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "xmm0", offset);
                    }
                    _ => {
                        emitter.instruction("add r11, 24");                     // skip the fixed indexed-array header before addressing the pointer-like payload region
                        emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", i * 8)); // load the requested pointer-like unpack element from the indexed-array payload region
                        abi::store_at_offset(emitter, "rax", offset);
                    }
                }
            }
        }
        ctx.update_var_type_and_ownership(
            var_name,
            elem_ty.clone(),
            super::super::HeapOwnership::borrowed_alias_for_type(&elem_ty),
        );
        update_callable_metadata_for_unpacked_var(var_name, value, &elem_ty, ctx);
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // discard the preserved indexed-array pointer after every list-unpack target local has been assigned
}

/// Updates callable metadata for a variable populated by list unpacking.
///
/// Callable array writes store the element callable's descriptor metadata under
/// the source array variable name. List unpacking turns one of those elements
/// back into a local variable, so it must mirror the same metadata onto the
/// destination local for direct calls and callback builtins.
fn update_callable_metadata_for_unpacked_var(
    dest: &str,
    source_array: &Expr,
    elem_ty: &PhpType,
    ctx: &mut Context,
) {
    if elem_ty != &PhpType::Callable {
        clear_callable_metadata(dest, ctx);
        return;
    }
    if let ExprKind::Variable(src_name) = &source_array.kind {
        copy_callable_metadata(dest, src_name, ctx);
    } else {
        clear_callable_metadata(dest, ctx);
    }
}

/// Copies callable signature, capture, first-class target, and descriptor markers.
fn copy_callable_metadata(dest: &str, src: &str, ctx: &mut Context) {
    if let Some(sig) = ctx.closure_sigs.get(src).cloned() {
        ctx.closure_sigs.insert(dest.to_string(), sig);
    } else {
        ctx.closure_sigs.remove(dest);
    }
    if let Some(captures) = ctx.closure_captures.get(src).cloned() {
        ctx.closure_captures.insert(dest.to_string(), captures);
    } else {
        ctx.closure_captures.remove(dest);
    }
    if let Some(target) = ctx.first_class_callable_targets.get(src).cloned() {
        ctx.first_class_callable_targets
            .insert(dest.to_string(), target);
    } else {
        ctx.first_class_callable_targets.remove(dest);
    }
    if let Some(label) = ctx.variable_fcc_label.get(src).cloned() {
        ctx.variable_fcc_label.insert(dest.to_string(), label);
    } else {
        ctx.variable_fcc_label.remove(dest);
    }
    if ctx.runtime_callable_vars.contains(src) {
        ctx.runtime_callable_vars.insert(dest.to_string());
    } else {
        ctx.runtime_callable_vars.remove(dest);
    }
}

/// Clears callable metadata for a list-unpack destination that is not callable.
fn clear_callable_metadata(dest: &str, ctx: &mut Context) {
    ctx.closure_sigs.remove(dest);
    ctx.closure_captures.remove(dest);
    ctx.first_class_callable_targets.remove(dest);
    ctx.variable_fcc_label.remove(dest);
    ctx.runtime_callable_vars.remove(dest);
}
