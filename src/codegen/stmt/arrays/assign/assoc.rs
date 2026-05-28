//! Purpose:
//! Lowers associative array element assignment with normalized keys and hash writes.
//! Evaluates assignment operands and writes the coerced value into the selected container.
//!
//! Called from:
//! - `crate::codegen::stmt::arrays::assign`
//!
//! Key details:
//! - Container mutation must follow copy-on-write and element ownership expectations.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::ArrayAssignTarget;

/// Lowers associative array element assignment with normalized keys and hash writes.
/// Evaluates the index expression and value expression, coerces the value to the target
/// element type when needed, and emits a runtime call to insert or update the entry.
///
/// # Arguments
/// - `target`: the array variable being assigned into (carries offset, element type, and ref flag)
/// - `index`: the string-key expression (will be normalized via `emit_normalized_hash_key`)
/// - `value`: the right-hand side expression to evaluate and store
/// - `emitter`: controls output and target architecture
/// - `ctx`: variable layout, ownership state, and compilation metadata
/// - `data`: data section for inline string literals and constants
///
/// # Behavior
/// - Preserves the hash-table pointer and computed key across expression evaluation via stack pushes
/// - Handles copy-on-write for shared arrays before mutation
/// - Persists owned strings via `__rt_str_persist` before transferring ownership to the hash table
/// - Boxes and retains borrowed heap results for `Mixed`/`Union` containers per ownership rules
/// - Calls `__rt_hash_set` to perform the actual insert/update; the runtime may reallocate the table
/// - On by-reference targets, reloads and stores through the reference slot after the call
///
/// # ABI Notes
/// - AArch64: hash-set helper receives table ptr (x0), key ptr/length (x1/x2), value (x3/x4), tag (x5)
/// - X86_64: hash-set helper receives table ptr (rdi), key ptr/length (rsi/rdx), value (rcx/r8), tag (r9)
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
    crate::codegen::emit_normalized_hash_key(index, emitter, ctx, data);
    abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);                       // preserve the computed key pointer and length while evaluating the value expression

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(target.elem_ty, PhpType::Mixed | PhpType::Union(_))
        && crate::codegen::expr::can_coerce_result_to_type(&val_ty, &target.elem_ty)
    {
        coerce_result_to_type(emitter, ctx, data, &val_ty, &target.elem_ty);
        val_ty = target.elem_ty.clone();
    }
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    if !boxed_iterable {
        super::super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    super::super::push::update_callable_array_metadata(target.array, value, &val_ty, ctx);

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
