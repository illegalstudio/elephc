//! Purpose:
//! Lowers foreach over objects implementing Iterator-style dispatch.
//! Maintains loop labels and body emission while advancing the iterable source.
//!
//! Called from:
//! - `crate::codegen::stmt::control_flow::foreach`
//!
//! Key details:
//! - Iterator state and iterable heap ownership must stay valid across break, continue, and loop completion.

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

/// Tracks setup work that must be undone after the iterator loop exits.
struct IteratorLoopState {
    deferred_receiver_release: Option<PhpType>,
}

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
    value_storage_override: Option<PhpType>,
    body: &[Stmt],
    loop_start: &str,
    loop_end: &str,
    loop_cont: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emit_iterator_loop(
        class_name,
        loop_start,
        loop_end,
        loop_cont,
        emitter,
        ctx,
        data,
        |dispatch_target, emitter, ctx, _| {
            let value_storage_ty =
                value_storage_override.clone().unwrap_or_else(|| {
                    iterator_value_storage_type(
                        &dispatch_target.method_return_type("current", ctx),
                    )
                });
            let value_is_mixed = matches!(value_storage_ty, PhpType::Mixed);
            let key_is_overwritten_by_value = key_var.as_deref() == Some(value_var);
            let mut deferred_receiver_release = None;
            if let Some(kv) = key_var {
                if !key_is_overwritten_by_value || value_is_mixed {
                    deferred_receiver_release =
                        normalize_iterator_mixed_slot(kv, receiver_var, emitter, ctx);
                }
            }
            if !key_is_overwritten_by_value || !value_is_mixed {
                let release = normalize_iterator_slot_for_type(
                    value_var,
                    receiver_var,
                    &value_storage_ty,
                    emitter,
                    ctx,
                );
                if deferred_receiver_release.is_none() {
                    deferred_receiver_release = release;
                }
            }
            IteratorLoopState {
                deferred_receiver_release,
            }
        },
        |dispatch_target, emitter, ctx, data| {
            let current_return_ty = dispatch_target.method_return_type("current", ctx);
            let value_storage_ty = value_storage_override.clone().unwrap_or_else(|| {
                iterator_value_storage_type(&current_return_ty)
            });
            if let Some(kv) = key_var {
                reload_iterator_receiver(emitter);
                let key_ty = dispatch_target.dispatch("key", emitter, ctx);
                if kv == value_var && !matches!(value_storage_ty, PhpType::Mixed) {
                    // The value assignment immediately overwrites the key binding before
                    // user code can observe it, but key() must still be called for effects.
                } else if let Some(kvar) = ctx.variables.get(kv) {
                    let k_offset = kvar.stack_offset;
                    store_iterator_mixed_result(kv, k_offset, &key_ty, emitter, ctx);
                } else {
                    emitter.comment(&format!("WARNING: undefined foreach key variable ${}", kv));
                }
            }

            reload_iterator_receiver(emitter);
            let current_ty = if dispatch_target.current_is_saved_receiver() {
                emit_current_as_saved_iterator_receiver(emitter)
            } else {
                dispatch_target.dispatch("current", emitter, ctx)
            };
            if let Some(vvar) = ctx.variables.get(value_var) {
                let v_offset = vvar.stack_offset;
                store_iterator_result(
                    value_var,
                    v_offset,
                    &current_ty,
                    &value_storage_ty,
                    emitter,
                    ctx,
                );
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
        },
        |state, emitter, _, _| {
            if let Some(release_ty) = state.deferred_receiver_release.as_ref() {
                release_saved_iterator_receiver(release_ty, emitter);
            }
        },
    );
}

/// Emits foreach for objects that may implement Iterator or IteratorAggregate.
///
/// Probes the saved iterable object at runtime to determine whether it implements
/// Iterator directly or requires IteratorAggregate's getIterator() to obtain an
/// Iterator. Branches to the appropriate lowering path; objects implementing
/// neither interface call `__rt_iterable_unsupported_kind` which aborts with a
/// fatal diagnostic.
///
/// # Arguments
/// * `receiver_var` - variable holding the iterable object (parked on the stack)
/// * `key_var` - optional key variable name
/// * `value_var` - value variable name
/// * `body` - foreach body statements
/// * `emitter`, `ctx`, `data` - codegen state
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
        None,
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
        None,
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

/// Emits assembly for iterable object loop.
pub(crate) fn emit_iterable_object_loop<S, P, B, A>(
    label_prefix: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    mut before_rewind: P,
    mut loop_body: B,
    mut after_loop: A,
) where
    P: FnMut(&IteratorDispatchTarget, &mut Emitter, &mut Context, &mut DataSection) -> S,
    B: FnMut(&IteratorDispatchTarget, &str, &mut Emitter, &mut Context, &mut DataSection),
    A: FnMut(S, &mut Emitter, &mut Context, &mut DataSection),
{
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
    let direct_case = ctx.next_label(&format!("{}_iterator", label_prefix));
    let aggregate_case = ctx.next_label(&format!("{}_aggregate", label_prefix));
    let done = ctx.next_label(&format!("{}_done", label_prefix));

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the object-backed iterable while probing its Traversable shape
    emit_branch_if_saved_receiver_implements(iterator_id, &direct_case, emitter);
    emit_branch_if_saved_receiver_implements(aggregate_id, &aggregate_case, emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // discard the unsupported object before raising the iterable diagnostic
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // unsupported object-backed iterables abort with a fatal diagnostic

    emitter.label(&direct_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the Iterator object pointer for the shared loop lowering
    let direct_start = ctx.next_label(&format!("{}_iterator_start", label_prefix));
    let direct_end = ctx.next_label(&format!("{}_iterator_end", label_prefix));
    let direct_cont = ctx.next_label(&format!("{}_iterator_cont", label_prefix));
    emit_iterator_loop(
        "Iterator",
        &direct_start,
        &direct_end,
        &direct_cont,
        emitter,
        ctx,
        data,
        |dispatch_target, emitter, ctx, data| before_rewind(dispatch_target, emitter, ctx, data),
        |dispatch_target, emitter, ctx, data| {
            loop_body(dispatch_target, &direct_end, emitter, ctx, data)
        },
        |state, emitter, ctx, data| after_loop(state, emitter, ctx, data),
    );
    abi::emit_jump(emitter, &done);                                             // skip the IteratorAggregate branch after direct Iterator iteration

    emitter.label(&aggregate_case);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the IteratorAggregate object pointer before getIterator()
    move_result_to_receiver_arg(emitter);
    emit_dispatch_interface_method("IteratorAggregate", "getiterator", emitter, ctx);
    let aggregate_start = ctx.next_label(&format!("{}_aggregate_start", label_prefix));
    let aggregate_end = ctx.next_label(&format!("{}_aggregate_end", label_prefix));
    let aggregate_cont = ctx.next_label(&format!("{}_aggregate_cont", label_prefix));
    emit_iterator_loop(
        "Iterator",
        &aggregate_start,
        &aggregate_end,
        &aggregate_cont,
        emitter,
        ctx,
        data,
        |dispatch_target, emitter, ctx, data| before_rewind(dispatch_target, emitter, ctx, data),
        |dispatch_target, emitter, ctx, data| {
            loop_body(dispatch_target, &aggregate_end, emitter, ctx, data)
        },
        |state, emitter, ctx, data| after_loop(state, emitter, ctx, data),
    );

    emitter.label(&done);
}

/// Emits assembly for iterator loop.
pub(crate) fn emit_iterator_loop<S, P, B, A>(
    class_name: &str,
    loop_start: &str,
    loop_end: &str,
    loop_cont: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    before_rewind: P,
    mut loop_body: B,
    after_loop: A,
) where
    P: FnOnce(&IteratorDispatchTarget, &mut Emitter, &mut Context, &mut DataSection) -> S,
    B: FnMut(&IteratorDispatchTarget, &mut Emitter, &mut Context, &mut DataSection),
    A: FnOnce(S, &mut Emitter, &mut Context, &mut DataSection),
{
    let mut dispatch_target = iterator_dispatch_target(class_name, ctx);
    if !dispatch_target.implements_iterator(ctx) {
        move_result_to_receiver_arg(emitter);
        let ret_ty = dispatch_target.dispatch("getiterator", emitter, ctx);
        dispatch_target = iterator_return_dispatch_target(&ret_ty, ctx);
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // park iterator receiver pointer in a 16-byte stack slot
    let state = before_rewind(&dispatch_target, emitter, ctx, data);

    reload_iterator_receiver(emitter);
    dispatch_target.dispatch("rewind", emitter, ctx);

    emitter.label(loop_start);

    reload_iterator_receiver(emitter);
    dispatch_target.dispatch("valid", emitter, ctx);
    emit_branch_if_invalid_iterator(emitter, loop_end);

    loop_body(&dispatch_target, emitter, ctx, data);

    emitter.label(loop_cont);
    reload_iterator_receiver(emitter);
    dispatch_target.dispatch("next", emitter, ctx);
    abi::emit_jump(emitter, loop_start);                                        // continue the iteration

    emitter.label(loop_end);
    after_loop(state, emitter, ctx, data);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the parked receiver slot
}

#[derive(Clone)]
pub(crate) enum IteratorDispatchTarget {
    Class(String),
    Interface(String),
}

/// Dispatches an Iterator method (rewind/valid/key/current/next) through the
/// appropriate class vtable or interface dispatch path. Returns the PHP return
/// type of the dispatched method.
impl IteratorDispatchTarget {
    /// Provides the Dispatch helper used by the iterator module.
    pub(crate) fn dispatch(
        &self,
        method: &str,
        emitter: &mut Emitter,
        ctx: &mut Context,
    ) -> PhpType {
        match self {
            IteratorDispatchTarget::Class(class_name) => {
                emit_dispatch_instance_method(class_name, method, emitter, ctx)
            }
            IteratorDispatchTarget::Interface(interface_name) => {
                emit_dispatch_interface_method(interface_name, method, emitter, ctx)
            }
        }
    }

    /// Returns the static return type for an iterator method without emitting a call.
    fn method_return_type(&self, method: &str, ctx: &Context) -> PhpType {
        let method_key = crate::names::php_symbol_key(method);
        match self {
            IteratorDispatchTarget::Class(class_name) => {
                if self.current_is_saved_receiver() && method_key == "current" {
                    return PhpType::Object("DirectoryIterator".to_string());
                }
                ctx.classes
                    .get(class_name)
                    .and_then(|class_info| class_info.methods.get(&method_key))
                    .map(|sig| sig.return_type.clone())
                    .unwrap_or(PhpType::Mixed)
            }
            IteratorDispatchTarget::Interface(interface_name) => ctx
                .interfaces
                .get(interface_name)
                .and_then(|interface_info| interface_info.methods.get(&method_key))
                .map(|sig| sig.return_type.clone())
                .unwrap_or(PhpType::Mixed),
        }
    }

    /// Returns true when `current()` should yield the parked receiver object directly.
    fn current_is_saved_receiver(&self) -> bool {
        matches!(self, IteratorDispatchTarget::Class(class_name) if class_name == "DirectoryIterator")
    }

    /// Returns true if the dispatch target implements the Iterator interface.
    /// For classes, checks the class's interface list. For interfaces, checks
    /// the interface's parent hierarchy via BFS.
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

/// Constructs an IteratorDispatchTarget from a class or interface name.
/// Distinguishes between classes and interfaces in the context's type registry.
fn iterator_dispatch_target(name: &str, ctx: &Context) -> IteratorDispatchTarget {
    if ctx.interfaces.contains_key(name) {
        IteratorDispatchTarget::Interface(name.to_string())
    } else {
        IteratorDispatchTarget::Class(name.to_string())
    }
}

/// Constructs an IteratorDispatchTarget from the return type of getIterator().
/// If the return type is a known interface, dispatches via that interface;
/// otherwise dispatches via the Iterator interface directly.
fn iterator_return_dispatch_target(ret_ty: &PhpType, ctx: &Context) -> IteratorDispatchTarget {
    match ret_ty {
        PhpType::Object(name) if name == "Traversable" => {
            IteratorDispatchTarget::Interface("Iterator".to_string())
        }
        PhpType::Object(name) if ctx.interfaces.contains_key(name) => {
            IteratorDispatchTarget::Interface(name.clone())
        }
        PhpType::Object(name) => IteratorDispatchTarget::Class(name.clone()),
        _ => IteratorDispatchTarget::Interface("Iterator".to_string()),
    }
}

/// Returns true if the named class directly implements the named interface,
/// checked via linear scan of the class's interface list.
fn class_implements_interface(class_name: &str, interface_name: &str, ctx: &Context) -> bool {
    ctx.classes.get(class_name).is_some_and(|class_info| {
        class_info
            .interfaces
            .iter()
            .any(|name| name == interface_name)
    })
}

/// Returns true if the named interface extends (directly or transitively)
/// the ancestor interface, checked via breadth-first search through the
/// interface parent hierarchy.
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

/// Emits DirectoryIterator::current() as the saved receiver object.
fn emit_current_as_saved_iterator_receiver(emitter: &mut Emitter) -> PhpType {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // return the saved DirectoryIterator receiver as current()
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // return the saved DirectoryIterator receiver as current()
        }
    }
    crate::codegen::abi::emit_incref_if_refcounted(
        emitter,
        &PhpType::Object("DirectoryIterator".to_string()),
    );
    PhpType::Object("DirectoryIterator".to_string())
}

/// Returns the stack storage type foreach should use for an iterator result.
fn iterator_value_storage_type(result_ty: &PhpType) -> PhpType {
    match result_ty {
        PhpType::Object(name) => PhpType::Object(name.clone()),
        _ => PhpType::Mixed,
    }
}

/// Normalizes a foreach value slot for the chosen iterator result storage.
fn normalize_iterator_slot_for_type(
    var_name: &str,
    receiver_var: Option<&str>,
    storage_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<PhpType> {
    if matches!(storage_ty, PhpType::Mixed) {
        return normalize_iterator_mixed_slot(var_name, receiver_var, emitter, ctx);
    }
    if receiver_var == Some(var_name) {
        retain_saved_iterator_receiver(emitter);
        return Some(PhpType::Object(String::new()));
    }
    None
}

/// Stores an iterator result into the foreach value variable using the best storage type.
fn store_iterator_result(
    var_name: &str,
    offset: usize,
    result_ty: &PhpType,
    storage_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if matches!(storage_ty, PhpType::Mixed) {
        store_iterator_mixed_result(var_name, offset, result_ty, emitter, ctx);
    } else {
        if matches!(result_ty.codegen_repr(), PhpType::Mixed) {
            unbox_owned_mixed_object_result(storage_ty, emitter);
        }
        store_iterator_typed_result(var_name, offset, storage_ty, emitter, ctx);
    }
}

/// Unboxes an owned Mixed result into an owned object result for typed foreach storage.
fn unbox_owned_mixed_object_result(storage_ty: &PhpType, emitter: &mut Emitter) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the owned mixed cell while exposing its object payload
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap the FilesystemIterator current() mixed result
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // promote the unboxed object payload into the result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // promote the unboxed object payload into the result register
        }
    }
    abi::emit_incref_if_refcounted(emitter, storage_ty);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the retained object while releasing the mixed wrapper
    abi::emit_load_temporary_stack_slot(emitter, result_reg, 16);
    abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the retained object result after wrapper cleanup
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved mixed wrapper slot
}

/// Stores a concrete object iterator result without boxing it into Mixed.
fn store_iterator_typed_result(
    var_name: &str,
    offset: usize,
    storage_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let (old_ty, old_offset, old_ownership) = ctx
        .variables
        .get(var_name)
        .map(|var| (var.ty.codegen_repr(), var.stack_offset, var.ownership))
        .unwrap_or((storage_ty.clone(), offset, HeapOwnership::NonHeap));
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the freshly returned iterator value across previous-value cleanup
    cleanup_replaced_iterator_slot(&old_ty, old_offset, old_ownership, emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the new iterator value after cleanup
    crate::codegen::abi::store_at_offset_scratch(
        emitter,
        abi::int_result_reg(emitter),
        offset,
        match emitter.target.arch {
            Arch::AArch64 => "x10",
            Arch::X86_64 => "r10",
        },
    );
    ctx.update_var_type_static_and_ownership(
        var_name,
        storage_ty.codegen_repr(),
        storage_ty.clone(),
        HeapOwnership::Owned,
    );
}

/// Stores the result of `key()` or `current()` into the foreach variable's
/// stack slot as a Mixed value. Preserves the previous slot value (if owned)
/// by calling `__rt_decref_mixed` before overwriting. Updates the variable's
/// type to Mixed and ownership to Owned.
///
/// # Arguments
/// * `var_name` - name of the foreach variable
/// * `offset` - stack offset of the variable slot
/// * `result_ty` - PHP type returned by key() or current()
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

/// Normalizes a foreach variable's stack slot to Mixed type before iteration.
/// If the variable aliases the iterator receiver (e.g. `foreach ($obj as $obj)`),
/// retains the receiver to prevent it being freed prematurely and returns the
/// type to release later. Otherwise returns None. Boxing and cleanup of the
/// old slot value are handled here before the slot is reused as Mixed.
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

/// Releases the old value in a slot before it is overwritten by a Mixed
/// foreach variable. Only releases if the previous ownership was Owned.
/// For refcounted types (strings, arrays, objects), calls the runtime
/// decref helper. For non-refcounted types (int, float, bool, null),
/// this is a no-op.
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

/// Reloads the parked iterator receiver from the stack slot and increments
/// its refcount. Called when a foreach variable aliases the receiver to
/// prevent the receiver being freed when the alias's old slot value is
/// cleaned up in the next iteration.
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

/// Reloads the parked iterator receiver from the stack slot and decrements
/// its refcount. Called after the foreach loop completes when a variable
/// aliased the receiver (deferred release after loop body finishes).
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

/// Moves the object result from the integer result register (rax/x0) into
/// the receiver argument register (rdi on SysV x86_64; x0 already holds it
/// on ARM64). On ARM64 this is a no-op since x0 is already the result reg.
fn move_result_to_receiver_arg(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the object result into the SysV receiver argument register
    }
}

/// Reloads the parked iterator receiver from the 16-byte stack slot into
/// the receiver argument register (x0 on ARM64, rdi on x86_64) before each
/// Iterator method dispatch (rewind/valid/key/current/next).
pub(crate) fn reload_iterator_receiver(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // reload receiver into x0 for the next Iterator method dispatch
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // reload receiver into rdi for the next Iterator method dispatch
        }
    }
}

/// Emits a conditional branch to `loop_end` when the iterator is exhausted.
/// Tests the integer result register (x0/rax) after a `valid()` call: if zero,
/// the iterator has no more elements and the foreach exits.
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

/// Emits a runtime interface check via `__rt_exception_matches` to test
/// whether the saved iterable object implements the given interface (Iterator
/// or IteratorAggregate). Branches to `target_label` if the check succeeds.
/// The saved object is loaded from the stack slot; the interface ID and
/// exception flag (1) are passed as immediate arguments.
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
