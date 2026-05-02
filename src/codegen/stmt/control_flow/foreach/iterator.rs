use crate::codegen::context::{Context, HeapOwnership, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::objects::dispatch::{
    emit_dispatch_instance_method, emit_dispatch_interface_method,
};
use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::emit_stmt;
use crate::parser::ast::Stmt;
use crate::types::PhpType;

/// Foreach over an object implementing the Iterator interface.
///
/// On entry, the target integer result register already holds the iterator
/// object pointer (left there by `emit_expr` on the foreach iterable
/// expression).
///
/// Loop shape:
///
/// ```text
/// rewind()
/// loop_start:
///     valid()  ; if !valid jump loop_end
///     key()    ; if requested -> key_var (Mixed)
///     current(); -> value_var (Mixed)
///     <body>
/// loop_cont:
///     next()
///     b loop_start
/// loop_end:
/// ```
///
/// The receiver pointer is parked in a 16-byte stack slot so it survives the
/// nested method calls without burning a callee-saved register. Each method
/// call reloads `x0` from that slot before dispatching through the vtable.
pub(crate) fn emit_iterator_foreach(
    class_name: &str,
    receiver_var: Option<&str>,
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    loop_start: &str,
    loop_end: &str,
    loop_cont: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let mut dispatch_target = iterator_dispatch_target(class_name, ctx);
    if !dispatch_target.implements_iterator(ctx) {
        move_result_to_receiver_arg(emitter);
        let ret_ty = dispatch_target.dispatch("getiterator", emitter, ctx);
        dispatch_target = iterator_return_dispatch_target(&ret_ty, ctx);
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // park iterator receiver pointer in a 16-byte stack slot
    let mut deferred_receiver_release = None;
    if let Some(kv) = key_var {
        deferred_receiver_release =
            normalize_iterator_mixed_slot(kv, receiver_var, emitter, ctx);
    }
    if key_var.as_deref() != Some(value_var) {
        let release = normalize_iterator_mixed_slot(value_var, receiver_var, emitter, ctx);
        if deferred_receiver_release.is_none() {
            deferred_receiver_release = release;
        }
    }

    reload_iterator_receiver(emitter);
    dispatch_target.dispatch("rewind", emitter, ctx);

    emitter.label(loop_start);

    reload_iterator_receiver(emitter);
    dispatch_target.dispatch("valid", emitter, ctx);
    emit_branch_if_invalid_iterator(emitter, loop_end);

    if let Some(kv) = key_var {
        reload_iterator_receiver(emitter);
        let key_ty = dispatch_target.dispatch("key", emitter, ctx);
        if let Some(kvar) = ctx.variables.get(kv) {
            let k_offset = kvar.stack_offset;
            store_iterator_mixed_result(kv, k_offset, &key_ty, emitter, ctx);
        } else {
            emitter.comment(&format!("WARNING: undefined foreach key variable ${}", kv));
        }
    }

    reload_iterator_receiver(emitter);
    let current_ty = dispatch_target.dispatch("current", emitter, ctx);
    if let Some(vvar) = ctx.variables.get(value_var) {
        let v_offset = vvar.stack_offset;
        store_iterator_mixed_result(value_var, v_offset, &current_ty, emitter, ctx);
    } else {
        emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_cont.to_string(),
        break_label: loop_end.to_string(),
        sp_adjust: 16,
    });
    for s in body {
        emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(loop_cont);
    reload_iterator_receiver(emitter);
    dispatch_target.dispatch("next", emitter, ctx);
    abi::emit_jump(emitter, loop_start);                                        // continue the iteration

    emitter.label(loop_end);
    if let Some(release_ty) = deferred_receiver_release.as_ref() {
        release_saved_iterator_receiver(release_ty, emitter);
    }
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the parked receiver slot
}

pub(crate) fn emit_iterable_object_foreach(
    receiver_var: Option<&str>,
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let iterator_id = ctx
        .interfaces
        .get("Iterator")
        .expect("codegen bug: missing builtin Iterator interface")
        .interface_id;
    let aggregate_id = ctx
        .interfaces
        .get("IteratorAggregate")
        .expect("codegen bug: missing builtin IteratorAggregate interface")
        .interface_id;
    let direct_case = ctx.next_label("foreach_iter_object_iterator");
    let aggregate_case = ctx.next_label("foreach_iter_object_aggregate");
    let done = ctx.next_label("foreach_iter_object_done");

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the object-backed iterable while probing its Traversable shape
    emit_branch_if_saved_receiver_implements(iterator_id, &direct_case, emitter);
    emit_branch_if_saved_receiver_implements(aggregate_id, &aggregate_case, emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // discard the unsupported object before raising the foreach diagnostic
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // unsupported object-backed iterables abort with a fatal diagnostic

    emitter.label(&direct_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the Iterator object pointer for the object foreach lowering
    let direct_start = ctx.next_label("foreach_iter_object_iterator_start");
    let direct_end = ctx.next_label("foreach_iter_object_iterator_end");
    let direct_cont = ctx.next_label("foreach_iter_object_iterator_cont");
    emit_iterator_foreach(
        "Iterator",
        receiver_var,
        key_var,
        value_var,
        body,
        &direct_start,
        &direct_end,
        &direct_cont,
        emitter,
        ctx,
        data,
    );
    abi::emit_jump(emitter, &done);                                             // skip the IteratorAggregate branch after direct Iterator iteration

    emitter.label(&aggregate_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the IteratorAggregate object pointer before getIterator()
    move_result_to_receiver_arg(emitter);
    emit_dispatch_interface_method("IteratorAggregate", "getiterator", emitter, ctx);
    let aggregate_start = ctx.next_label("foreach_iter_object_aggregate_start");
    let aggregate_end = ctx.next_label("foreach_iter_object_aggregate_end");
    let aggregate_cont = ctx.next_label("foreach_iter_object_aggregate_cont");
    emit_iterator_foreach(
        "Iterator",
        None,
        key_var,
        value_var,
        body,
        &aggregate_start,
        &aggregate_end,
        &aggregate_cont,
        emitter,
        ctx,
        data,
    );

    emitter.label(&done);
}

#[derive(Clone)]
enum IteratorDispatchTarget {
    Class(String),
    Interface(String),
}

impl IteratorDispatchTarget {
    fn dispatch(&self, method: &str, emitter: &mut Emitter, ctx: &mut Context) -> PhpType {
        match self {
            IteratorDispatchTarget::Class(class_name) => {
                emit_dispatch_instance_method(class_name, method, emitter, ctx)
            }
            IteratorDispatchTarget::Interface(interface_name) => {
                emit_dispatch_interface_method(interface_name, method, emitter, ctx)
            }
        }
    }

    fn implements_iterator(&self, ctx: &Context) -> bool {
        match self {
            IteratorDispatchTarget::Class(class_name) => {
                class_implements_interface(class_name, "Iterator", ctx)
            }
            IteratorDispatchTarget::Interface(interface_name) => {
                interface_extends_interface(interface_name, "Iterator", ctx)
            }
        }
    }
}

fn iterator_dispatch_target(name: &str, ctx: &Context) -> IteratorDispatchTarget {
    if ctx.interfaces.contains_key(name) {
        IteratorDispatchTarget::Interface(name.to_string())
    } else {
        IteratorDispatchTarget::Class(name.to_string())
    }
}

fn iterator_return_dispatch_target(ret_ty: &PhpType, ctx: &Context) -> IteratorDispatchTarget {
    match ret_ty {
        PhpType::Object(name) if ctx.interfaces.contains_key(name) => {
            IteratorDispatchTarget::Interface(name.clone())
        }
        PhpType::Object(name) => IteratorDispatchTarget::Class(name.clone()),
        _ => IteratorDispatchTarget::Interface("Iterator".to_string()),
    }
}

fn class_implements_interface(class_name: &str, interface_name: &str, ctx: &Context) -> bool {
    ctx.classes.get(class_name).is_some_and(|class_info| {
        class_info
            .interfaces
            .iter()
            .any(|name| name == interface_name)
    })
}

fn interface_extends_interface(interface_name: &str, ancestor_name: &str, ctx: &Context) -> bool {
    if interface_name == ancestor_name {
        return true;
    }
    let mut stack = vec![interface_name.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(current_name) = stack.pop() {
        if !seen.insert(current_name.clone()) {
            continue;
        }
        let Some(interface_info) = ctx.interfaces.get(&current_name) else {
            continue;
        };
        for parent_name in &interface_info.parents {
            if parent_name == ancestor_name {
                return true;
            }
            stack.push(parent_name.clone());
        }
    }
    false
}

fn store_iterator_mixed_result(
    var_name: &str,
    offset: usize,
    result_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    crate::codegen::emit_box_current_value_as_mixed(emitter, &result_ty.codegen_repr());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the freshly returned mixed value across previous-value cleanup
            crate::codegen::abi::load_at_offset_scratch(emitter, "x0", offset, "x10");
            emitter.instruction("bl __rt_decref_mixed");                        // release the previous owned foreach mixed value before overwriting it
            emitter.instruction("ldr x0, [sp], #16");                           // restore the new mixed value after cleanup
            crate::codegen::abi::store_at_offset_scratch(emitter, "x0", offset, "x10");
        }
        Arch::X86_64 => {
            crate::codegen::abi::emit_push_reg(emitter, "rax");                 // preserve the freshly returned mixed value across previous-value cleanup
            crate::codegen::abi::load_at_offset_scratch(emitter, "rax", offset, "r10");
            emitter.instruction("call __rt_decref_mixed");                      // release the previous owned foreach mixed value before overwriting it
            crate::codegen::abi::emit_pop_reg(emitter, "rax");                  // restore the new mixed value after cleanup
            crate::codegen::abi::store_at_offset_scratch(emitter, "rax", offset, "r10");
        }
    }
    ctx.update_var_type_and_ownership(var_name, PhpType::Mixed, HeapOwnership::Owned);
}

fn normalize_iterator_mixed_slot(
    var_name: &str,
    receiver_var: Option<&str>,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<PhpType> {
    let Some(var) = ctx.variables.get(var_name) else {
        return None;
    };
    let old_ty = var.ty.codegen_repr();
    let old_static_ty = var.static_ty.clone();
    let old_offset = var.stack_offset;
    let old_ownership = var.ownership;
    let aliases_receiver = receiver_var == Some(var_name);

    let deferred_receiver_release = if aliases_receiver {
        retain_saved_iterator_receiver(emitter);
        Some(PhpType::Object(String::new()))
    } else {
        None
    };

    if matches!(old_ty, PhpType::Mixed | PhpType::Union(_)) {
        return deferred_receiver_release;
    }

    abi::emit_load(emitter, &old_ty, old_offset);
    crate::codegen::emit_box_current_value_as_mixed(emitter, &old_ty);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed previous foreach variable while cleaning up its old slot
    cleanup_replaced_iterator_slot(&old_ty, old_offset, old_ownership, emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the boxed previous foreach variable after cleanup
    crate::codegen::abi::store_at_offset_scratch(
        emitter,
        abi::int_result_reg(emitter),
        old_offset,
        match emitter.target.arch {
            Arch::AArch64 => "x10",
            Arch::X86_64 => "r10",
        },
    );
    ctx.update_var_type_static_and_ownership(
        var_name,
        PhpType::Mixed,
        old_static_ty,
        HeapOwnership::Owned,
    );
    deferred_receiver_release
}

fn cleanup_replaced_iterator_slot(
    old_ty: &PhpType,
    old_offset: usize,
    old_ownership: HeapOwnership,
    emitter: &mut Emitter,
) {
    if old_ownership != HeapOwnership::Owned {
        return;
    }
    let result_reg = abi::int_result_reg(emitter);
    let scratch_reg = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    match old_ty {
        PhpType::Str => {
            if emitter.target.arch == Arch::X86_64 {
                return;
            }
            crate::codegen::abi::load_at_offset_scratch(
                emitter,
                result_reg,
                old_offset,
                scratch_reg,
            );
            abi::emit_call_label(emitter, "__rt_heap_free_safe");               // release the old owned string slot after boxing its PHP value
        }
        ty if ty.is_refcounted() => {
            crate::codegen::abi::load_at_offset_scratch(
                emitter,
                result_reg,
                old_offset,
                scratch_reg,
            );
            crate::codegen::abi::emit_decref_if_refcounted(emitter, ty);
        }
        _ => {}
    }
}

fn retain_saved_iterator_receiver(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // reload the saved iterator receiver before retaining it for alias safety
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // reload the saved iterator receiver before retaining it for alias safety
        }
    }
    crate::codegen::abi::emit_incref_if_refcounted(
        emitter,
        &PhpType::Object(String::new()),
    );
}

fn release_saved_iterator_receiver(release_ty: &PhpType, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // reload the saved iterator receiver before deferred alias cleanup
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // reload the saved iterator receiver before deferred alias cleanup
        }
    }
    crate::codegen::abi::emit_decref_if_refcounted(emitter, release_ty);
}

fn move_result_to_receiver_arg(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the object result into the SysV receiver argument register
    }
}

fn reload_iterator_receiver(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // reload receiver into x0 for the next Iterator method dispatch
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // reload receiver into rdi for the next Iterator method dispatch
        }
    }
}

fn emit_branch_if_invalid_iterator(emitter: &mut Emitter, loop_end: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // valid() returned 0 means the iterator is exhausted
            emitter.instruction(&format!("b.eq {}", loop_end));                 // exit foreach when valid() returns false
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // valid() returned 0 means the iterator is exhausted
            emitter.instruction(&format!("je {}", loop_end));                   // exit foreach when valid() returns false
        }
    }
}

fn emit_branch_if_saved_receiver_implements(
    interface_id: u64,
    target_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // load the saved iterable object as matcher argument 1
            abi::emit_load_int_immediate(emitter, "x1", interface_id as i64);
            abi::emit_load_int_immediate(emitter, "x2", 1);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // test whether the object implements the requested Traversable interface
            emitter.instruction("cmp x0, #0");                                  // did the runtime interface matcher succeed?
            emitter.instruction(&format!("b.ne {}", target_label));             // branch to the matching foreach lowering path
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // load the saved iterable object as matcher argument 1
            abi::emit_load_int_immediate(emitter, "rsi", interface_id as i64);
            abi::emit_load_int_immediate(emitter, "rdx", 1);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // test whether the object implements the requested Traversable interface
            emitter.instruction("test rax, rax");                               // did the runtime interface matcher succeed?
            emitter.instruction(&format!("jne {}", target_label));              // branch to the matching foreach lowering path
        }
    }
}
