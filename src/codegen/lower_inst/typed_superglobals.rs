//! Purpose:
//! Publishes a PHP value into a `--web` request superglobal's typed Hash storage, used by the
//! literal (`StoreGlobal`) and computed (`StoreGlobalDynamic`) `$GLOBALS` write paths.
//!
//! Called from:
//! - `crate::codegen::lower_inst::globals_constants::{lower_store_global,
//!   lower_store_global_dynamic}`.
//!
//! Key details:
//! - A superglobal slot holds a raw `Heap(Array)` hash pointer, while every ordinary global holds
//!   a boxed Mixed cell. Both entry points therefore unpack a Mixed source, accept an indexed
//!   array through `__rt_array_to_hash`, take the slot's own reference, and retire the previous
//!   hash after publishing the new one so a destructor observes the assigned value.
//! - A non-array value raises a catchable `TypeError`: the request prelude and the static
//!   superglobal accesses require hash storage, while PHP would re-type the superglobal.

use super::*;

/// Publishes one array value into a web superglobal's typed Hash symbol.
///
/// The literal `$GLOBALS['_SERVER'] = $value` path reaches `StoreGlobal` directly, without the
/// dynamic table's kind marker. A Mixed value has to be unpacked exactly like the computed path;
/// otherwise the box pointer lands in a slot every static `$_SERVER` read treats as a Hash.
pub(super) fn lower_typed_superglobal_store(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    value: ValueId,
    source_ty: &PhpType,
) -> Result<()> {
    use crate::codegen::lower_inst::exceptions::emit_type_error;

    let owns_value = ctx.value_can_transfer_ownership_to_consumer(value)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let publish = ctx.next_label("typed_sg_publish");
            let type_error = ctx.next_label("typed_sg_type_error");
            let after = ctx.next_label("typed_sg_after");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // x0 = tag, x1 = payload
                    let keep = ctx.next_label("typed_sg_keep");
                    let convert = ctx.next_label("typed_sg_convert");
                    ctx.emitter.instruction("cmp x0, #5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("b.eq {keep}"));
                    ctx.emitter.instruction("cmp x0, #4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("b.eq {convert}"));
                    ctx.emitter.instruction(&format!("b {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov x0, x1");                      // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov x0, x1");                      // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // x0 = fresh owned hash
                    ctx.emitter.instruction("mov x2, x0");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                _ => {
                    if source_ty == &PhpType::Str {
                        abi::emit_store_to_sp(ctx.emitter, "x1", 8);            // string results carry the owned payload in the length-adjacent register
                    } else {
                        abi::emit_store_to_sp(ctx.emitter, "x0", 8);            // preserve a non-array owner for the TypeError cleanup
                    }
                    ctx.emitter.instruction(&format!("b {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_symbol_to_reg(ctx.emitter, "x10", symbol, 0);
            abi::emit_store_reg_to_symbol(ctx.emitter, "x2", symbol, 0);
            let done = ctx.next_label("typed_sg_done");
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // nothing to release when the superglobal was unset
            ctx.emitter.instruction("mov x0, x10");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.label(&done);
            ctx.emitter.instruction(&format!("b {after}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.label(&after);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        Arch::X86_64 => {
            let publish = ctx.next_label("typed_sg_publish");
            let type_error = ctx.next_label("typed_sg_type_error");
            let after = ctx.next_label("typed_sg_after");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // rax = tag, rdi = payload
                    let keep = ctx.next_label("typed_sg_keep");
                    let convert = ctx.next_label("typed_sg_convert");
                    ctx.emitter.instruction("cmp rax, 5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("je {keep}"));
                    ctx.emitter.instruction("cmp rax, 4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("je {convert}"));
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov rax, rdi");                    // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov rax, rdi");                    // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // rax = fresh owned hash
                    ctx.emitter.instruction("mov rcx, rax");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                _ => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve a non-array owner for the TypeError cleanup
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", symbol, 0);
            abi::emit_store_reg_to_symbol(ctx.emitter, "rcx", symbol, 0);
            let done = ctx.next_label("typed_sg_done");
            ctx.emitter.instruction("test r11, r11");
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction("mov rax, r11");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.label(&done);
            ctx.emitter.instruction(&format!("jmp {after}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.label(&after);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
    }
    Ok(())
}

/// Publishes one array value into a typed web-superglobal Hash slot.
///
/// The dynamic global table marks a web superglobal with kind 1, because its storage word is a
/// raw `Heap(Array)` pointer instead of the boxed Mixed cell every ordinary global uses. A
/// computed write has to unpack the Mixed value, accept either array representation, take the
/// slot's own reference, and publish it with the same release-before-retire ordering as a typed
/// property store. A non-array value is rejected with a catchable TypeError; PHP would re-type
/// the superglobal, but elephc's request prelude and static accesses require hash storage.
pub(super) fn lower_hash_superglobal_store(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    source_ty: &PhpType,
    done: &str,
) -> Result<()> {
    use crate::codegen::lower_inst::exceptions::emit_type_error;

    let owns_value = ctx.value_can_transfer_ownership_to_consumer(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let publish = ctx.next_label("dyn_hash_publish");
            let type_error = ctx.next_label("dyn_hash_type_error");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // x0 = tag, x1 = payload
                    let keep = ctx.next_label("dyn_hash_keep");
                    let convert = ctx.next_label("dyn_hash_convert");
                    ctx.emitter.instruction("cmp x0, #5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("b.eq {keep}"));
                    ctx.emitter.instruction("cmp x0, #4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("b.eq {convert}"));
                    ctx.emitter.instruction(&format!("b {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov x0, x1");                      // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov x0, x1");                      // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // x0 = fresh owned hash
                    ctx.emitter.instruction("mov x2, x0");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                _ => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve a non-array owner for the TypeError cleanup
                    ctx.emitter.instruction(&format!("b {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction(&format!("cbz x9, {done}"));
            ctx.emitter.instruction("ldr x10, [x9]");                           // capture the previous hash owner
            ctx.emitter.instruction("str x2, [x9]");                            // publish the assigned hash before retiring the old one
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // nothing to release when the superglobal was unset
            ctx.emitter.instruction("mov x0, x10");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction(&format!("b {done}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.instruction(&format!("b {done}"));                      // unreachable: the TypeError unwind leaves the helper
        }
        Arch::X86_64 => {
            let publish = ctx.next_label("dyn_hash_publish");
            let type_error = ctx.next_label("dyn_hash_type_error");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // rax = tag, rdi = payload
                    let keep = ctx.next_label("dyn_hash_keep");
                    let convert = ctx.next_label("dyn_hash_convert");
                    ctx.emitter.instruction("cmp rax, 5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("je {keep}"));
                    ctx.emitter.instruction("cmp rax, 4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("je {convert}"));
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov rax, rdi");                    // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov rax, rdi");                    // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // rax = fresh owned hash
                    ctx.emitter.instruction("mov rcx, rax");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                _ => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve a non-array owner for the TypeError cleanup
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction(&format!("test r10, r10"));
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // capture the previous hash owner
            ctx.emitter.instruction("mov QWORD PTR [r10], rcx");                // publish the assigned hash before retiring the old one
            ctx.emitter.instruction(&format!("test r11, r11"));
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction("mov rax, r11");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction(&format!("jmp {done}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // unreachable: the TypeError unwind leaves the helper
        }
    }
    Ok(())
}
