use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::abi;
use super::super::context::HeapOwnership;
use super::super::emit::Emitter;
use super::super::expr::expr_result_heap_ownership;

pub(super) fn retain_borrowed_heap_result(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    if ty.is_refcounted() && expr_result_heap_ownership(expr) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, ty);
    }
}

pub(super) fn local_slot_ownership_after_store(ty: &PhpType) -> HeapOwnership {
    HeapOwnership::local_owner_for_type(ty)
}

pub(super) fn stamp_indexed_array_value_type(
    emitter: &mut Emitter,
    array_reg: &str,
    elem_ty: &PhpType,
) {
    let value_type_tag = match elem_ty {
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        _ => return,
    };
    let (kind_reg, tag_reg, mask_reg) = array_header_scratch_regs(emitter.target.arch, array_reg);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [{}, #-8]", kind_reg, array_reg)); // load the packed array kind word from the heap header
            abi::emit_load_int_immediate(emitter, mask_reg, 0x80ff);
            emitter.instruction(&format!("and {}, {}, {}", kind_reg, kind_reg, mask_reg)); // keep only the indexed-array kind and persistent COW flag bits
            abi::emit_load_int_immediate(emitter, tag_reg, value_type_tag);
            emitter.instruction(&format!("lsl {}, {}, #8", tag_reg, tag_reg));  // move the runtime value_type tag into the packed kind-word byte lane
            emitter.instruction(&format!("orr {}, {}, {}", kind_reg, kind_reg, tag_reg)); // combine the heap kind with the runtime value_type tag
            emitter.instruction(&format!("str {}, [{}, #-8]", kind_reg, array_reg)); // persist the packed array kind word back into the heap header
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [{} - 8]", kind_reg, array_reg)); // load the packed array kind word from the heap header
            emitter.instruction(&format!("mov {}, {}", mask_reg, kind_reg));    // copy the packed array kind word so the x86_64 heap marker and low container bits can be preserved independently
            abi::emit_load_int_immediate(emitter, tag_reg, 0x80ff);
            emitter.instruction(&format!("and {}, {}", kind_reg, tag_reg));     // keep only the low indexed-array kind and persistent COW flag bits from the original header
            abi::emit_load_int_immediate(emitter, tag_reg, -65536);
            emitter.instruction(&format!("and {}, {}", mask_reg, tag_reg));     // keep the high x86_64 heap-marker bits while clearing the low container-kind payload lane
            abi::emit_load_int_immediate(emitter, tag_reg, value_type_tag);
            emitter.instruction(&format!("shl {}, 8", tag_reg));                // move the runtime value_type tag into the packed kind-word byte lane
            emitter.instruction(&format!("or {}, {}", kind_reg, mask_reg));     // combine the preserved x86_64 heap marker bits with the stable low container-kind payload bits
            emitter.instruction(&format!("or {}, {}", kind_reg, tag_reg));      // combine the preserved container metadata with the new runtime value_type tag
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], {}", array_reg, kind_reg)); // persist the packed array kind word back into the heap header without losing the x86_64 heap marker
        }
    }
}

pub(super) fn release_owned_slot(
    emitter: &mut Emitter,
    ty: &PhpType,
    offset: usize,
    result_ty: &PhpType,
) {
    match result_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                        // preserve the incoming string result across old-slot cleanup helpers
        }
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));         // preserve the incoming float result across old-slot cleanup helpers
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the incoming scalar/pointer result across old-slot cleanup helpers
        }
    }

    let result_reg = abi::int_result_reg(emitter);
    if matches!(ty, PhpType::Str) {
        abi::load_at_offset(emitter, result_reg, offset);                                // load the previous string pointer from the local slot before releasing it
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
    } else if ty.is_refcounted() {
        abi::load_at_offset(emitter, result_reg, offset);                                // load the previous heap pointer from the local slot before decreffing it
        abi::emit_decref_if_refcounted(emitter, ty);
    }

    match result_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);                         // restore the incoming string result after old-slot cleanup helpers finish
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));          // restore the incoming float result after old-slot cleanup helpers finish
        }
        _ => {
            abi::emit_pop_reg(emitter, result_reg);                                    // restore the incoming scalar/pointer result after old-slot cleanup helpers finish
        }
    }
}

pub(super) fn emit_static_init_guard(emitter: &mut Emitter, init_label: &str, skip_label: &str) {
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), init_label, 0);
    abi::emit_branch_if_int_result_nonzero(emitter, skip_label);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), init_label, 0);
}

fn array_header_scratch_regs(arch: Arch, array_reg: &str) -> (&'static str, &'static str, &'static str) {
    let candidates = match arch {
        Arch::AArch64 => ["x12", "x13", "x14", "x15", "x16", "x17"].as_slice(),
        Arch::X86_64 => ["r11", "r10", "rcx", "rax", "rdx", "r8", "r9"].as_slice(),
    };
    let regs = candidates
        .iter()
        .copied()
        .filter(|reg| *reg != array_reg)
        .take(3)
        .collect::<Vec<_>>();
    (regs[0], regs[1], regs[2])
}
