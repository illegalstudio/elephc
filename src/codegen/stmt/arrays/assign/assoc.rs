use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::ArrayAssignTarget;

pub(super) fn emit_assoc_array_assign(
    target: &ArrayAssignTarget<'_>,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let table_reg = abi::int_result_reg(emitter);
    let ref_reg = abi::symbol_scratch_reg(emitter);
    let (key_ptr_reg, key_len_reg) = abi::string_result_regs(emitter);

    if target.is_ref {
        abi::load_at_offset(emitter, ref_reg, target.offset);                         // load the by-reference slot that points at the hash-table local
        abi::emit_load_from_address(emitter, table_reg, ref_reg, 0);                  // dereference the by-reference slot to get the current hash-table pointer
    } else {
        abi::load_at_offset(emitter, table_reg, target.offset);                       // load the current hash-table pointer from the local slot
    }
    abi::emit_push_reg(emitter, table_reg);                                           // preserve the hash-table pointer while evaluating the string key
    emit_expr(index, emitter, ctx, data);
    abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);                       // preserve the computed key pointer and length while evaluating the value expression

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(target.elem_ty, PhpType::Mixed) && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        super::super::super::super::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else {
        super::super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            let (val_lo, val_hi) = match &val_ty {
                PhpType::Int | PhpType::Bool => ("x0", "xzr"),
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_str_persist");                // persist the inserted string value before handing ownership to the hash table
                    ("x1", "x2")
                }
                PhpType::Float => {
                    emitter.instruction("fmov x9, d0");                         // move the float payload bits into an integer register for the hash runtime ABI
                    ("x9", "xzr")
                }
                _ => ("x0", "xzr"),
            };
            emitter.instruction(&format!("mov x3, {}", val_lo));                // place the low payload word into the hash-set helper value register
            emitter.instruction(&format!("mov x4, {}", val_hi));                // place the high payload word into the hash-set helper value register
            emitter.instruction(&format!(                                       // materialize the runtime value tag that describes the inserted associative-array payload
                "mov x5, #{}",
                super::super::super::super::runtime_value_tag(&val_ty)
            ));
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                              // restore the preserved key pointer and length into the hash-set helper argument registers
            abi::emit_pop_reg(emitter, "x0");                                          // restore the preserved hash-table pointer into the first hash-set helper argument register
        }
        Arch::X86_64 => {
            match &val_ty {
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_str_persist");                // persist the inserted string value before handing ownership to the hash table
                    emitter.instruction("mov rcx, rax");                        // place the owned string pointer into the SysV hash-set helper low-payload register
                    emitter.instruction("mov r8, rdx");                         // place the owned string length into the SysV hash-set helper high-payload register
                }
                PhpType::Float => {
                    emitter.instruction("movq rcx, xmm0");                      // move the float payload bits into the SysV hash-set helper low-payload register
                    emitter.instruction("xor r8, r8");                          // float associative-array payloads only use the low payload word
                }
                _ => {
                    emitter.instruction("mov rcx, rax");                        // place the scalar or pointer payload into the SysV hash-set helper low-payload register
                    emitter.instruction("xor r8, r8");                          // scalar or pointer associative-array payloads only use the low payload word
                }
            }
            abi::emit_load_int_immediate(
                emitter,
                "r9",
                super::super::super::super::runtime_value_tag(&val_ty) as i64,
            ); // materialize the runtime value tag that describes the inserted associative-array payload
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                            // restore the preserved key pointer and length into the SysV hash-set helper registers
            abi::emit_pop_reg(emitter, "rdi");                                        // restore the preserved hash-table pointer into the first SysV hash-set helper register
        }
    }

    abi::emit_call_label(emitter, "__rt_hash_set");                                   // insert or update the associative-array entry and return the possibly-reallocated table pointer
    if target.is_ref {
        abi::load_at_offset(emitter, ref_reg, target.offset);                         // reload the by-reference slot that points at the hash-table local
        abi::emit_store_to_address(emitter, table_reg, ref_reg, 0);                   // store the updated hash-table pointer through the by-reference slot
    } else {
        abi::store_at_offset(emitter, table_reg, target.offset);                      // save the possibly-reallocated hash-table pointer back into the local slot
    }
}
