//! Purpose:
//! Adapts private PHP transport values to compiled function arguments and results.
//!
//! Called from:
//! - EIR user assembly finalization for `--emit phpext`.
//!
//! Key details:
//! - All Zend allocation and exceptions happen after this native setjmp boundary returns.
//! - Input boxes remain owned here while user functions borrow their values.

use super::*;
use crate::types::PhpType;
use crate::codegen_support::{try_handlers::TRY_HANDLER_SLOT_SIZE, value_boxing};

/// Emits the private transport entry points while sharing library recovery machinery.
pub(crate) fn emit_phpext_exports(
    emitter: &mut Emitter, data: &mut DataSection, target: Target,
    exports: &[&ExportedFunction], heap_debug: bool, startup: Option<&str>,
) {
    reserve_boundary_data(data);
    data.add_comm("_elephc_phpext_pack_depth".into(), 8);
    data.add_comm(PHPEXT_REPLACEMENT_OWNER_COUNT.into(), 8);
    data.add_comm(PHPEXT_REPLACEMENT_OWNER_BASE.into(), 8);
    data.add_comm(
        PHPEXT_REPLACEMENT_OWNERS.into(),
        PHPEXT_REPLACEMENT_OWNER_LIMIT * 8,
    );
    let error = data.add_string(b"elephc PHP extension runtime failure");
    emit_error_helpers(emitter, target);
    lifecycle::emit(emitter, target, heap_debug, startup, (&error.0, error.1));
    for export in exports { emit_export(emitter, export, heap_debug); }
}

/// Emits one fixed-signature native thunk with complete input-owner cleanup.
fn emit_export(emitter: &mut Emitter, export: &ExportedFunction, heap_debug: bool) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let name = label_suffix(&export.c_name);
    let escaped = format!("L_{name}_escaped");
    let cleanup = format!("L_{name}_cleanup");
    let rollback = format!("L_{name}_rollback");
    let record_error = format!("L_{name}_record_error");
    let args = 8;
    let status = 16;
    let concat = 24;
    let saved_boundary_status = 32;
    let saved_result = 40;
    let boxes = (0..export.source_sig.params.len()).map(|i| 48 + i * 8).collect::<Vec<_>>();
    let mut used = 40 + boxes.len() * 8;
    used += 8;
    let exception_cursor = used;
    used += 8;
    let replacement_owner_base = used;
    used += 8;
    let saved_replacement_owner_base = used;
    let mut params = Vec::new();
    for (_, ty) in &export.source_sig.params {
        let mut slots = Vec::new();
        for _ in 0..ty.codegen_repr().stack_size().max(8) / 8 { used += 8; slots.push(used); }
        params.push(slots);
    }
    let hidden = boundary::internal_has_hidden_collector(export).then(|| { used += 8; used });
    let preserved = match emitter.target.arch {
        Arch::X86_64 => vec!["rbx", "r12", "r13", "r14", "r15"],
        Arch::AArch64 => vec!["x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28",
            "d8", "d9", "d10", "d11", "d12", "d13", "d14", "d15"],
    }.into_iter().map(|reg| { used += 8; (reg, used) }).collect::<Vec<_>>();
    let handler = boundary::align_16(used + TRY_HANDLER_SLOT_SIZE);
    let frame = boundary::align_16(handler + 16);
    // -- preserve the host ABI and establish the contained native boundary --
    emitter.label_global(&emitter.target.extern_symbol(&export.c_name));
    abi::emit_frame_prologue(emitter, frame);
    for (reg, offset) in &preserved { abi::store_at_offset(emitter, reg, *offset); }
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), args);
    for slot in boxes.iter().copied().chain([saved_result]) {
        abi::emit_store_zero_to_local_slot(emitter, slot);
    }
    abi::emit_load_symbol_to_reg(emitter, result, BOUNDARY_STATUS, 0);
    abi::store_at_offset(emitter, result, saved_boundary_status);
    abi::emit_load_symbol_to_reg(emitter, result, PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
    abi::store_at_offset(emitter, result, replacement_owner_base);
    abi::emit_load_symbol_to_reg(emitter, result, PHPEXT_REPLACEMENT_OWNER_BASE, 0);
    abi::store_at_offset(emitter, result, saved_replacement_owner_base);
    abi::load_at_offset(emitter, result, replacement_owner_base);
    abi::emit_store_reg_to_symbol(emitter, result, PHPEXT_REPLACEMENT_OWNER_BASE, 0);
    boundary::emit_initialize_hidden_collector_owner(emitter, hidden);
    emitter.bl_c("elephc_php_stack_floor");                                      // use the current PHP thread or Fiber boundary instead of process stack limits
    abi::emit_store_reg_to_symbol(emitter, result, "_stack_limit", 0);
    abi::emit_store_reg_to_symbol(emitter, result, "_stack_limit_main", 0);
    if heap_debug { abi::emit_enable_heap_debug_flag(emitter); }
    emit_clear_error_inline(emitter);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
    boundary::emit_enter_boundary(emitter, concat, &name);
    match emitter.target.arch {
        Arch::AArch64 => emit_boundary_push_aarch64(emitter, &escaped, handler),
        Arch::X86_64 => emit_boundary_push_x86_64(emitter, &escaped, handler),
    }
    // -- copy transport arguments into owned native boxes and physical slots --
    for (index, ((_, ty), slots)) in export.internal_sig.params.iter()
        .take(boxes.len()).zip(&params).enumerate() {
        abi::load_at_offset(emitter, scratch, args);
        abi::emit_load_from_address(emitter, result, scratch, index * 8);
        abi::emit_call_label(emitter, "__rt_zval_unpack");
        abi::store_at_offset(emitter, result, boxes[index]);
        match ty.codegen_repr() {
            PhpType::Mixed => abi::store_at_offset(emitter, result, slots[0]),
            PhpType::Str => {
                let (ptr, len) = abi::string_result_regs(emitter);
                abi::emit_reg_move(emitter, scratch, result);
                abi::emit_load_from_address(emitter, ptr, scratch, 8);
                abi::emit_load_from_address(emitter, len, scratch, 16);
                abi::store_at_offset(emitter, ptr, slots[0]);
                abi::store_at_offset(emitter, len, slots[1]);
            }
            PhpType::TaggedScalar => {
                abi::emit_load_from_address(emitter, scratch, result, 0);
                abi::store_at_offset(emitter, scratch, slots[1]);
                abi::emit_load_from_address(emitter, result, result, 8);
                abi::store_at_offset(emitter, result, slots[0]);
            }
            ty => {
                let dest = if ty == PhpType::Float { abi::float_result_reg(emitter) } else { result };
                abi::emit_load_from_address(emitter, dest, result, 8);
                abi::store_at_offset(emitter, dest, slots[0]);
            }
        }
    }
    // -- invoke the compiled function with its physical EIR signature --
    boundary::emit_prepare_hidden_collector(emitter, hidden);
    emit_call(emitter, export, &params, hidden);
    // -- copy the owned result into independent transport storage --
    value_boxing::emit_box_current_owned_value_as_mixed(emitter, &export.internal_sig.return_type);
    abi::store_at_offset(emitter, result, saved_result);
    abi::emit_call_label(emitter, "__rt_zval_pack");
    abi::emit_push_reg(emitter, result);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    emitter.bl_c("elephc_php_result");
    abi::emit_pop_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_zval_free");
    boundary::emit_release_hidden_collector(emitter, hidden);
    abi::emit_store_zero_to_local_slot(emitter, status);
    abi::emit_jump(emitter, &cleanup);

    // -- capture native failures before releasing exception and argument owners --
    emitter.label(&escaped);
    // The internal unwinder already detached the hidden collector's cleanup record.
    abi::emit_store_zero_to_local_slot(emitter, hidden.unwrap_or(status));
    abi::emit_load_symbol_to_reg(emitter, abi::int_arg_reg_name(emitter.target, 0), "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, abi::int_arg_reg_name(emitter.target, 1), BOUNDARY_STATUS, 0);
    emitter.bl_c("elephc_php_exception");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbnz w0, {rollback}")),  // inspect the C int result at its declared width
        Arch::X86_64 => {
            emitter.instruction("test eax, eax");                               // distinguish infrastructure rollback from ordinary PHP exceptions
            emitter.instruction(&format!("jnz {rollback}"));                    // let the adapter roll back a failed native conversion without recursive frees
        }
    }
    let copy_previous = format!("L_{name}_copy_exception_previous");
    let copied_previous = format!("L_{name}_copied_exception_previous");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_throwable_previous");
    emitter.label(&copy_previous);
    abi::emit_branch_if_int_result_zero(emitter, &copied_previous);
    abi::store_at_offset(emitter, result, exception_cursor);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    emitter.bl_c("elephc_php_exception_previous");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbnz w0, {rollback}")),  // roll back if copying any previous node exceeds the transport budget
        Arch::X86_64 => {
            emitter.instruction("test eax, eax");                               // inspect the continuation callback's C int status
            emitter.instruction(&format!("jnz {rollback}"));                    // abandon partial transport without releasing rolled-back native owners
        }
    }
    abi::load_at_offset(emitter, result, exception_cursor);
    abi::emit_call_label(emitter, "__rt_throwable_previous");
    abi::emit_jump(emitter, &copy_previous);
    emitter.label(&copied_previous);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_jump(emitter, &record_error);
    // -- defer partial native allocation cleanup to request-image rollback --
    emitter.label(&rollback);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    for slot in boxes.iter().copied().chain([saved_result]) {
        abi::emit_store_zero_to_local_slot(emitter, slot);
    }
    emitter.label(&record_error);
    abi::emit_load_int_immediate(emitter, result, STATUS_PHP_EXCEPTION as i64);
    abi::store_at_offset(emitter, result, status);

    // -- retire call owners and restore every host callee-saved register --
    emitter.label(&cleanup);
    match emitter.target.arch {
        Arch::AArch64 => emit_boundary_pop_aarch64(emitter, handler),
        Arch::X86_64 => emit_boundary_pop_x86_64(emitter, handler),
    }
    for slot in boxes.into_iter().chain([saved_result]) {
        abi::load_at_offset(emitter, result, slot);
        abi::emit_call_label(emitter, "__rt_decref_mixed");
    }
    emit_release_replacement_owners(emitter, replacement_owner_base, &name);
    abi::load_at_offset(emitter, result, saved_replacement_owner_base);
    abi::emit_store_reg_to_symbol(emitter, result, PHPEXT_REPLACEMENT_OWNER_BASE, 0);
    boundary::emit_leave_boundary(emitter, concat);
    abi::load_at_offset(emitter, result, saved_boundary_status);
    abi::emit_store_reg_to_symbol(emitter, result, BOUNDARY_STATUS, 0);
    abi::load_at_offset(emitter, result, status);
    for (reg, offset) in &preserved { abi::load_at_offset(emitter, reg, *offset); }
    abi::emit_frame_restore(emitter, frame);
    abi::emit_return(emitter);
}

/// Releases every native replacement pin added by this export, including nested failures.
fn emit_release_replacement_owners(emitter: &mut Emitter, base: usize, suffix: &str) {
    let loop_label = format!("L_{suffix}_release_replacement_owner");
    let done = format!("L_{suffix}_released_replacement_owners");
    emitter.label(&loop_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x10", base);
            abi::emit_load_symbol_to_reg(emitter, "x9", PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
            emitter.instruction("cmp x9, x10");                                 // retire only pins acquired below this export boundary
            emitter.instruction(&format!("b.ls {done}"));                       // stop at the owner count saved by this export
            emitter.instruction("sub x9, x9, #1");                              // claim the newest replacement pin for release
            abi::emit_store_reg_to_symbol(emitter, "x9", PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
            abi::emit_extern_symbol_address(emitter, "x10", PHPEXT_REPLACEMENT_OWNERS);
            emitter.instruction("ldr x0, [x10, x9, lsl #3]");                   // consume the last retained replacement owner
            emitter.instruction("str xzr, [x10, x9, lsl #3]");                  // make an escaping destructor path idempotent
            abi::emit_call_label(emitter, "__rt_decref_any");
            emitter.instruction(&format!("b {loop_label}"));                    // retire every pin acquired by this export
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r11", base);
            abi::emit_load_symbol_to_reg(emitter, "r10", PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
            emitter.instruction("cmp r10, r11");                                // retire only pins acquired below this export boundary
            emitter.instruction(&format!("jbe {done}"));                        // stop at the owner count saved by this export
            emitter.instruction("sub r10, 1");                                  // claim the newest replacement pin for release
            abi::emit_store_reg_to_symbol(emitter, "r10", PHPEXT_REPLACEMENT_OWNER_COUNT, 0);
            abi::emit_extern_symbol_address(emitter, "r11", PHPEXT_REPLACEMENT_OWNERS);
            emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");            // consume the last retained replacement owner
            emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");              // make an escaping destructor path idempotent
            abi::emit_call_label(emitter, "__rt_decref_any");
            emitter.instruction(&format!("jmp {loop_label}"));                  // retire every pin acquired by this export
        }
    }
    emitter.label(&done);
}

/// Materializes physical EIR arguments, including boxed arrays and nullable scalars.
fn emit_call(emitter: &mut Emitter, export: &ExportedFunction, params: &[Vec<usize>], hidden: Option<usize>) {
    // -- stage scalar, string and tagged arguments in their physical shapes --
    for ((_, ty), slots) in export.internal_sig.params.iter().zip(params) {
        match ty.codegen_repr() {
            PhpType::Float => abi::load_at_offset(emitter, abi::float_result_reg(emitter), slots[0]),
            PhpType::Str => {
                let (ptr, len) = abi::string_result_regs(emitter);
                abi::load_at_offset(emitter, ptr, slots[0]);
                abi::load_at_offset(emitter, len, slots[1]);
            }
            PhpType::TaggedScalar => {
                abi::load_at_offset(emitter, abi::int_result_reg(emitter), slots[0]);
                abi::load_at_offset(emitter, crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter), slots[1]);
            }
            _ => abi::load_at_offset(emitter, abi::int_result_reg(emitter), slots[0]),
        }
        abi::emit_push_result_value(emitter, ty);
    }
    if let Some(slot) = hidden {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), slot);
        abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
    }
    // -- materialize the target ABI, invoke the body and release outgoing slots --
    let types = export.internal_sig.params.iter().map(|(_, ty)| ty.codegen_repr()).collect::<Vec<_>>();
    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &types, 0);
    let overflow = abi::materialize_outgoing_args(emitter, &assignments);
    let pad = abi::outgoing_call_stack_pad_bytes(emitter.target, overflow);
    abi::emit_reserve_temporary_stack(emitter, pad);
    abi::emit_call_label(emitter, &crate::names::function_symbol(&export.name));
    abi::emit_release_temporary_stack(emitter, pad + overflow);
}
