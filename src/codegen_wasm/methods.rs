//! Purpose:
//! Lowers `Op::MethodCall` and `Op::StaticMethodCall` for the wasm32-wasi backend,
//! and emits the per-(introducer, method) dispatch stubs that route virtual
//! instance-method calls to the runtime class's override.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches the two ops here.
//! - `crate::codegen_wasm::generate` calls `emit_method_dispatch_stubs` after the
//!   class-method lowering loop, so every `call $<stub>` emitted by
//!   `lower_method_call` resolves to a defined function.
//!
//! Key details:
//! - WASM has no call-to-register, so the closed AOT class set is branched
//!   explicitly: each dispatch stub reads the receiver's `class_id` from
//!   `[obj + 0]` and walks an `i64.eq` if-ladder over the concrete subclass ids,
//!   tail-calling the matching implementation. One stub per introducing class +
//!   method key (the topmost class declaring the virtual method), so unrelated
//!   hierarchies that happen to share a method name never collide.
//! - Instance calls take the direct path when the method is non-virtual (no
//!   vtable slot, or `final`); otherwise they call the introducer's stub.
//! - True static calls push a constant `called_class_id` (i64 hidden param 0)
//!   then the user args. Lexical `self::`/`parent::` calls that resolve to an
//!   instance method forward the current `this` (slot 0) instead, which is what
//!   makes `parent::__construct()` chaining work.

use super::classes::{mixed_method_candidates, mixed_tag_for_php_type};
use super::context::{FnCtx, Result};
use super::symbols::{function_symbol, method_dispatch_symbol, method_symbol};
use super::inst::{data_immediate, operand};
use super::objects;
use super::values::WasmRepr;
use super::wat::{ValType, WatModule};
use super::WasmError;
use crate::ir::{Function, Instruction, IrHeapKind, IrType, LocalSlotId, Module, ValueId};
use crate::names::php_symbol_key;
use crate::types::PhpType;
use std::collections::HashMap;

/// Lowers an `Op::MethodCall` to a direct or dispatched instance call.
///
/// `operands[0]` is the receiver; `operands[1..]` are the user arguments. The
/// receiver's `PhpType` must be `Object(class)`; `Mixed`/`Union` receivers are
/// routed to `lower_mixed_method_call` (the P6f class-id if-ladder dispatch).
/// Variadic and by-reference parameters are rejected here (out of P6d scope); the
/// frontend guarantees arity for the rest.
pub(super) fn lower_method_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let method_name = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("method call: unknown data {:?}", data_id)))?
        .clone();
    let method_key = php_symbol_key(&method_name);
    let (method_ptr, method_len) = ctx.str_literal(data_id)?;

    let receiver = operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    let receiver_ir = ctx
        .function
        .value(receiver)
        .map(|value| value.ir_type)
        .ok_or_else(|| {
            WasmError::Unsupported(format!("method receiver {:?} has no EIR value", receiver))
        })?;
    let static_class = (receiver_ir == IrType::Heap(IrHeapKind::Object))
        .then(|| exact_static_object_class(&receiver_ty))
        .flatten();
    let class_name = match static_class {
        Some(class_name) => class_name,
        None if matches!(receiver_ty, PhpType::Mixed | PhpType::Union(_)) => {
            return lower_mixed_method_call(
                ctx,
                inst,
                receiver,
                &method_name,
                &method_key,
                method_ptr,
                method_len,
            );
        }
        None => {
            return Err(WasmError::Unsupported(format!(
                "method call on non-object receiver {:?}",
                receiver_ty
            )));
        }
    };

    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    let callee_sig = ci
        .methods
        .get(&method_key)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown method {}::{}", class_name, method_name)))?;
    // A variadic the EIR already packed arrives as one ordinary `array<T>` argument, so the
    // operand count matches the declared parameter count; anything else is unpacked.
    let packed_variadic = inst.operands.len() == callee_sig.params.len() + 1;
    if callee_sig.variadic.is_some() && !packed_variadic {
        return Err(WasmError::Unsupported(format!(
            "variadic method {}::{} (out of P6d scope)",
            class_name, method_name
        )));
    }
    if callee_sig.ref_params.iter().any(|r| *r) {
        return Err(WasmError::Unsupported(format!(
            "by-reference parameter in {}::{} (out of P6d scope)",
            class_name, method_name
        )));
    }

    let has_slot = ci.vtable_slots.contains_key(&method_key);
    let is_final = ci.final_methods.contains(&method_key);
    let dynamic = has_slot && !is_final;
    let impl_class = ci
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| class_name.clone());

    // A Throwable accessor has a signature but no EIR body on either backend, so it is read off
    // the object here instead of dispatched. `throwable_intrinsic` needs every class the
    // receiver can be at run time, because an overriding subclass must keep winning.
    if let Some(intrinsic) = throwable_intrinsic_for_call(ctx, &class_name, &method_key, dynamic)? {
        if inst.operands.len() != 1 {
            return Err(WasmError::Unsupported(format!(
                "Throwable::{} with {} arguments on wasm32-wasi",
                method_name,
                inst.operands.len()
            )));
        }
        let class_info = ci.clone();
        let obj_ref = objects::object_ptr_ref(ctx, receiver)?;
        objects::emit_throwable_intrinsic(ctx, &obj_ref, &class_info, intrinsic)?;
        if let Some(r) = inst.result {
            ctx.emit_store_value(r)?;
        } else {
            for _ in 0..WasmRepr::val_types(inst.result_type).len() {
                ctx.fb.ins("drop", "discard unused Throwable accessor result");
            }
        }
        return Ok(());
    }
    let callee_symbol = if dynamic {
        let introducer = resolve_vtable_introducer(ctx, &class_name, &method_key)?;
        method_dispatch_symbol(&introducer, &method_key)
    } else {
        method_symbol(&format!("{}::{}", impl_class, method_name))
    };
    let mode = if dynamic { "dispatch" } else { "direct" };

    // `void` bodies push nothing; the call expression's null is supplied after the call.
    let body_returns_void = ctx
        .module
        .class_methods
        .iter()
        .find(|body| body.name == format!("{}::{}", impl_class, method_name))
        .map(|body| body.return_type == IrType::Void)
        .unwrap_or(false);
    let return_arity = if body_returns_void {
        0
    } else {
        WasmRepr::val_types(inst.result_type).len()
    };
    ctx.emit_load_value(receiver)?;
    for &arg in inst.operands.iter().skip(1) {
        ctx.emit_load_value(arg)?;
    }
    ctx.fb.ins(
        &format!("call ${}", callee_symbol),
        &format!("{}::{} ({})", class_name, method_name, mode),
    );

    // A `void` method returns nothing, but PHP still gives its CALL EXPRESSION the value null —
    // which is what the EIR materializes when the result is used. Nothing came back on the
    // stack, so the null is supplied here.
    if body_returns_void && inst.result.is_some() {
        ctx.fb.ins(
            "i64.const 9223372036854775806",
            "null sentinel: a void method call evaluates to null",
        );
    }
    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused method result");
        }
    }
    Ok(())
}

/// Resolves the open-coded `Throwable` accessor for one method call, if any.
///
/// A virtual call can land on any concrete class in the introducer's subtree, so the decision
/// uses the same candidate set the capability audit does; a non-virtual one can only reach the
/// receiver's own implementation. Sharing `dynamic_method_candidates` with the audit is what
/// keeps the gate and the emitter from disagreeing about which calls are open-coded.
fn throwable_intrinsic_for_call(
    ctx: &FnCtx,
    class_name: &str,
    method_key: &str,
    dynamic: bool,
) -> Result<Option<objects::ThrowableIntrinsic>> {
    let candidates = if dynamic {
        super::capability::dynamic_method_candidates(ctx.module, class_name, method_key)
            .map_err(WasmError::Unsupported)?
    } else {
        let implementation = ctx
            .module
            .class_infos
            .get(class_name)
            .and_then(|class_info| class_info.method_impl_classes.get(method_key).cloned())
            .unwrap_or_else(|| class_name.to_string());
        vec![(class_name.to_string(), implementation)]
    };
    Ok(objects::throwable_intrinsic(
        ctx.module,
        class_name,
        method_key,
        &candidates,
    ))
}

/// Returns the statically known class carried either by `Object(C)` or by the
/// exact pointer-backed nullable form `Object(C)|null`.
///
/// Capability validation separately proves that nullable values reach a direct
/// method call only through the false edge of `IsNull(receiver)`.
fn exact_static_object_class(receiver: &PhpType) -> Option<String> {
    match receiver {
        PhpType::Object(class_name) => Some(class_name.clone()),
        PhpType::Union(members)
            if members.len() == 2
                && members
                    .iter()
                    .any(|member| matches!(member, PhpType::Void | PhpType::Never)) =>
        {
            members.iter().find_map(|member| match member {
                PhpType::Object(class_name) => Some(class_name.clone()),
                _ => None,
            })
        }
        _ => None,
    }
}

/// Lowers an `Op::MethodCall` whose receiver is `Mixed`/`Union` (P6f).
///
/// The closed AOT class set is branched explicitly: the receiver cell is unboxed,
/// and the runtime `class_id` (read from `[obj + 0]`) drives an `i64.eq` if-ladder
/// over the candidate classes whose method arity matches the call. Each arm
/// calls the already-resolved concrete implementation directly, passes the
/// unboxed object pointer as `this`, and boxes the callee's concrete return into
/// a Mixed cell when the result slot is `Mixed`/`Union`.
///
/// The unboxed object pointer is BORROWED from the Mixed cell (never freed here);
/// the receiver cell is released by the EIR ownership pass. No candidates, a
/// A non-object receiver and a runtime class without the requested method both
/// terminate through PHP-style fatal runtime helpers. No candidates are rejected
/// before emission.
pub(super) fn lower_mixed_method_call(
    ctx: &mut FnCtx,
    inst: &Instruction,
    receiver: ValueId,
    method_name: &str,
    method_key: &str,
    method_ptr: u32,
    method_len: u32,
) -> Result<()> {
    let candidates = mixed_method_candidates(ctx.module, method_key);
    if candidates.is_empty() {
        return Err(WasmError::Unsupported(format!(
            "mixed method {}: no candidate class (P6f)",
            method_name
        )));
    }

    // Unbox the receiver once; reuse (tag, lo, hi) across every candidate arm.
    let mhi = ctx.fresh_temp(ValType::I64);
    let mlo = ctx.fresh_temp(ValType::I64);
    let mtag = ctx.fresh_temp(ValType::I64);
    let obj = ctx.fresh_temp(ValType::I32);
    let cid = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(receiver)?;
    ctx.fb.ins("call $__rt_mixed_unbox", "unbox mixed receiver -> (tag, lo, hi)");
    ctx.fb.ins(&format!("local.set {}", mhi), "capture receiver high word");
    ctx.fb.ins(&format!("local.set {}", mlo), "capture receiver low word");
    ctx.fb.ins(&format!("local.set {}", mtag), "capture receiver runtime tag");

    ctx.fb.ins(&format!("local.get {}", mtag), "receiver runtime tag");
    ctx.fb.ins("i64.const 6", "object tag");
    ctx.fb.ins("i64.eq", "is the receiver an object?");
    ctx.fb.ins("if", "receiver is an object");
    // obj = i32.wrap_i64(mlo); cid = i64.load [obj+0]
    ctx.fb.ins(&format!("local.get {}", mlo), "receiver low word");
    ctx.fb.ins("i32.wrap_i64", "object pointer (i32)");
    ctx.fb.ins(&format!("local.set {}", obj), "receiver object pointer");
    ctx.fb.ins(&format!("local.get {}", obj), "receiver object pointer");
    ctx.fb.ins("i64.load offset=0", "runtime class id");
    ctx.fb.ins(&format!("local.set {}", cid), "receiver class id");

    ctx.fb.ins("block $mxdone", "mixed dispatch merge");
    for (class_id, class_name, impl_class) in &candidates {
        ctx.fb.ins(&format!("local.get {}", cid), "receiver class id");
        ctx.fb.ins(&format!("i64.const {}", *class_id as i64), "candidate class id");
        ctx.fb.ins("i64.eq", "matches this candidate?");
        ctx.fb.ins("if", "candidate class id arm");
        emit_candidate_call(ctx, inst, class_name, impl_class, method_key, method_name, &obj)?;
        ctx.fb.ins("br $mxdone", "candidate handled -> merge");
        ctx.fb.ins("end", "end candidate class id arm");
    }
    ctx.fb.ins(&format!("local.get {}", cid), "unmatched receiver class id");
    ctx.fb.ins(&format!("i32.const {}", method_ptr), "method-name pointer");
    ctx.fb.ins(&format!("i32.const {}", method_len), "method-name byte length");
    ctx.fb.ins(
        "call $__rt_fail_undefined_method",
        "raise PHP fatal for undefined object method",
    );
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:post-noreturn:mixed-undefined-method fatal helper does not return",
    );
    ctx.fb.ins("end", "end mixed dispatch merge");
    ctx.fb.ins("else", "receiver is not an object");
    ctx.fb.ins(&format!("i32.const {}", method_ptr), "method-name pointer");
    ctx.fb.ins(&format!("i32.const {}", method_len), "method-name byte length");
    ctx.fb.ins(&format!("local.get {}", mtag), "receiver runtime tag");
    ctx.fb.ins("i32.wrap_i64", "runtime tag as i32");
    ctx.fb.ins(
        "call $__rt_fail_method_call_non_object",
        "raise PHP fatal for non-object receiver",
    );
    ctx.fb.ins(
        "unreachable",
        "elephc-trap:post-noreturn:mixed-non-object-method fatal helper does not return",
    );
    ctx.fb.ins("end", "end receiver object test");
    Ok(())
}

/// Emits one candidate arm of a mixed/union method dispatch.
///
/// Calls one exact implementation selected by the surrounding closed-world
/// Mixed/Union class-id ladder, then boxes or forwards its result.
///
/// The outer ladder has already resolved the runtime class, so calling a shared
/// virtual stub here would dispatch twice and would incorrectly impose one
/// implementation's return ABI on covariant overrides in the same subtree.
fn emit_candidate_call(
    ctx: &mut FnCtx,
    inst: &Instruction,
    class_name: &str,
    impl_class: &str,
    method_key: &str,
    method_name: &str,
    obj_local: &str,
) -> Result<()> {
    let ci = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    // A Throwable accessor has no body to call, so this arm reads it off the receiver. The
    // candidate is its own decision here: the ladder already selected one exact runtime class,
    // so a sibling that overrides the method keeps its own arm and its own body.
    let intrinsic = super::objects::throwable_intrinsic(
        ctx.module,
        class_name,
        method_key,
        &[(class_name.to_string(), impl_class.to_string())],
    );
    let (callee_ret_ir, callee_ret_php) = if let Some(intrinsic) = intrinsic {
        let ci = ci.clone();
        let storage = super::objects::throwable_intrinsic_storage(&ci, intrinsic)?;
        super::objects::emit_throwable_intrinsic(ctx, obj_local, &ci, intrinsic)?;
        storage
    } else {
        let callee_symbol = method_symbol(&format!("{}::{}", impl_class, method_name));

        // Authoritative callee return IR type (for boxing) + PHP type (for the tag).
        let callee_fn = find_method_function(&ctx.module.class_methods, impl_class, method_key)
            .ok_or_else(|| {
                WasmError::Unsupported(format!("no method {}::{}", impl_class, method_name))
            })?;
        let callee_ret_ir = callee_fn.return_type;
        let callee_ret_php = ci
            .methods
            .get(method_key)
            .map(|s| s.return_type.clone())
            .unwrap_or(PhpType::Mixed);

        // Receiver (the unboxed object pointer) as `this`, then user args in order.
        ctx.fb.ins(&format!("local.get {}", obj_local), "receiver object pointer (this)");
        for &arg in inst.operands.iter().skip(1) {
            ctx.emit_load_value(arg)?;
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (closed-world direct)", class_name, method_name),
        );
        (callee_ret_ir, callee_ret_php)
    };

    let result_is_boxed = matches!(inst.result_php_type, PhpType::Mixed | PhpType::Union(_));
    let callee_ret_is_mixed = matches!(callee_ret_ir, IrType::Heap(IrHeapKind::Mixed));
    if result_is_boxed && !callee_ret_is_mixed {
        box_call_result_into_mixed(ctx, callee_ret_ir, &callee_ret_php, inst.result)?;
    } else if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..WasmRepr::val_types(callee_ret_ir).len() {
            ctx.fb.ins("drop", "discard unused mixed method result");
        }
    }
    Ok(())
}

/// Boxes a concrete callee return (on the WASM stack) into a Mixed cell and stores
/// the cell pointer into the boxed result slot.
///
/// `__rt_mixed_from_value` does NOT consume the source: it persists a fresh copy of
/// a string and increfs a heap child, leaving the caller's owned source ref in
/// place. Because the callee return is a WAT-stack value (not an EIR value the
/// ownership pass can see), this function must release that source itself: the Str
/// and Heap arms call `__rt_decref_any` on the captured pointer *after* `from_value`
/// (so the cell's incref/persist lands first). `__rt_decref_any` no-ops on static
/// data-segment strings, so a literal-returning callee is unaffected. Callable
/// returns use the scalar-width ABI but own a kind-6 descriptor, so their I64
/// arm also releases the source descriptor after tag-10 boxing. Other I64/F64
/// values are scalars and need no release. The tag mirrors
/// `__rt_mixed_from_value`'s contract (int 0, bool 3, float 2, string 1, array 4,
/// assoc 5, object 6, null/void 8, callable 10), matching `lower_mixed_box`.
fn box_call_result_into_mixed(
    ctx: &mut FnCtx,
    ir: IrType,
    php: &PhpType,
    result: Option<ValueId>,
) -> Result<()> {
    // The runtime mixed-cell tag is derived from the callee's PHP return type
    // (int 0, bool 3, float 2, string 1, array 4, assoc 5, object 6). The IrType
    // only governs the on-stack shape of the callee return.
    let tag = mixed_tag_for_php_type(php).ok_or_else(|| {
        WasmError::Unsupported(format!("box mixed method return php {:?}", php))
    })?;
    match ir {
        IrType::I64 => {
            let t = ctx.fresh_temp(ValType::I64);
            ctx.fb.ins(&format!("local.set {}", t), "capture scalar/callable return");
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (scalar/callable)");
            ctx.fb.ins(&format!("local.get {}", t), "scalar -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box scalar into a mixed cell");
            if php.codegen_repr() == PhpType::Callable {
                ctx.fb
                    .ins(&format!("local.get {}", t), "callee-owned callable descriptor");
                ctx.fb.ins("i32.wrap_i64", "callable descriptor pointer");
                ctx.fb.ins(
                    "call $__rt_decref_any",
                    "release callee's owned callable (cell holds its own ref)",
                );
            }
        }
        IrType::F64 => {
            let t = ctx.fresh_temp(ValType::F64);
            ctx.fb.ins(&format!("local.set {}", t), "capture float return");
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (float)");
            ctx.fb.ins(&format!("local.get {}", t), "float value");
            ctx.fb.ins("i64.reinterpret_f64", "float bits -> lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box float into a mixed cell");
        }
        IrType::Str => {
            let len = ctx.fresh_temp(ValType::I64);
            let ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", len), "capture string length");
            ctx.fb.ins(&format!("local.set {}", ptr), "capture string pointer");
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (string)");
            ctx.fb.ins(&format!("local.get {}", ptr), "string pointer -> lo");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> i64 lo");
            ctx.fb.ins(&format!("local.get {}", len), "string length -> hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box string (persists a copy)");
            ctx.fb.ins(&format!("local.get {}", ptr), "callee-owned string pointer");
            ctx.fb.ins("call $__rt_decref_any", "release callee's owned string (no-op on static)");
        }
        IrType::Heap(kind) => {
            // The caller skips boxing when the callee already returns a Mixed cell,
            // so a Heap kind reaching here is array/hash/object.
            if !matches!(kind, IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object) {
                return Err(WasmError::Unsupported(format!(
                    "box mixed method heap return {:?}",
                    kind
                )));
            }
            let ptr = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.set {}", ptr), "capture heap pointer");
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (heap kind)");
            ctx.fb.ins(&format!("local.get {}", ptr), "heap pointer -> lo");
            ctx.fb.ins("i64.extend_i32_u", "ptr -> i64 lo");
            ctx.fb.ins("i64.const 0", "hi unused");
            ctx.fb.ins("call $__rt_mixed_from_value", "box heap value (increfs the child)");
            ctx.fb.ins(&format!("local.get {}", ptr), "callee-owned heap pointer");
            ctx.fb.ins("call $__rt_decref_any", "release callee's owned return (cell holds its own ref)");
        }
        IrType::TaggedScalar => {
            return Err(WasmError::Unsupported(
                "box mixed method tagged-scalar return (P6f)".to_string(),
            ));
        }
        IrType::Void => {
            ctx.fb.ins(&format!("i64.const {}", tag), "mixed tag (null)");
            ctx.fb.ins("i64.const 0", "lo");
            ctx.fb.ins("i64.const 0", "hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box null (void callee, mixed result)");
        }
    }
    if let Some(r) = result {
        ctx.emit_store_value(r)?;
    }
    Ok(())
}

/// Lowers an `Op::NullsafeMethodCall` (P6f).
///
/// EIR emits this op for `?->` on a `Mixed`/`Union` receiver. The receiver cell is
/// unboxed: a null payload (tag 8) produces a boxed-null result; an object payload
/// (tag 6) reuses the mixed class-id if-ladder (the same candidate arms as
/// `lower_mixed_method_call`); any other tag raises PHP's non-object method-call
/// fatal. The null-result path requires a boxed (`Mixed`/`Union`) result slot; a
/// concrete result slot is the
/// heterogeneous-`?->` case, which is genuinely type-unsafe (null cannot merge into a
/// concrete slot) and is deferred to P6g with a proper nullable result, surfacing
/// here as `Unsupported` rather than miscompiling.
///
/// The unboxed object pointer is BORROWED; the receiver cell is released by the EIR
/// ownership pass (this path does not decref it).
pub(super) fn lower_nullsafe_method_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let method_name = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("nullsafe call: unknown data {:?}", data_id)))?
        .clone();
    let method_key = php_symbol_key(&method_name);
    let (method_ptr, method_len) = ctx.str_literal(data_id)?;
    let receiver = operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    match receiver_ty {
        PhpType::Object(_) => {
            // Defensive: a non-nullable object receiver should not reach the nullsafe
            // op (EIR emits a plain MethodCall). Fall back to the object dispatch path.
            lower_method_call(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let candidates = mixed_method_candidates(ctx.module, &method_key);
            if candidates.is_empty() {
                return Err(WasmError::Unsupported(format!(
                    "nullsafe method {}: no candidate class (P6f)",
                    method_name
                )));
            }
            // A concrete result slot is the heterogeneous-?-> case (type-unsafe);
            // defer it to P6g rather than emitting a null that would mismatch the slot.
            if !matches!(inst.result_php_type, PhpType::Mixed | PhpType::Union(_)) {
                return Err(WasmError::Unsupported(format!(
                    "nullsafe {} with a concrete result slot (heterogeneous ?-> deferred to P6g)",
                    method_name
                )));
            }

            let mhi = ctx.fresh_temp(ValType::I64);
            let mlo = ctx.fresh_temp(ValType::I64);
            let mtag = ctx.fresh_temp(ValType::I64);
            let obj = ctx.fresh_temp(ValType::I32);
            let cid = ctx.fresh_temp(ValType::I64);
            ctx.emit_load_value(receiver)?;
            ctx.fb.ins("call $__rt_mixed_unbox", "unbox nullsafe receiver -> (tag, lo, hi)");
            ctx.fb.ins(&format!("local.set {}", mhi), "capture receiver high word");
            ctx.fb.ins(&format!("local.set {}", mlo), "capture receiver low word");
            ctx.fb.ins(&format!("local.set {}", mtag), "capture receiver runtime tag");

            ctx.fb.ins(&format!("local.get {}", mtag), "receiver runtime tag");
            ctx.fb.ins("i64.const 8", "null tag");
            ctx.fb.ins("i64.eq", "is the receiver null?");
            ctx.fb.ins("if", "receiver is null");
            ctx.fb.ins("i64.const 8", "mixed tag (null)");
            ctx.fb.ins("i64.const 0", "lo");
            ctx.fb.ins("i64.const 0", "hi");
            ctx.fb.ins("call $__rt_mixed_from_value", "box null into a mixed cell");
            if let Some(r) = inst.result {
                ctx.emit_store_value(r)?;
            } else {
                ctx.fb.ins("drop", "discard unused null result");
            }
            ctx.fb.ins("else", "receiver is object-or-other");
            ctx.fb.ins(&format!("local.get {}", mtag), "receiver runtime tag");
            ctx.fb.ins("i64.const 6", "object tag");
            ctx.fb.ins("i64.eq", "is the receiver an object?");
            ctx.fb.ins("if", "receiver is an object");
            ctx.fb.ins(&format!("local.get {}", mlo), "receiver low word");
            ctx.fb.ins("i32.wrap_i64", "object pointer (i32)");
            ctx.fb.ins(&format!("local.set {}", obj), "receiver object pointer");
            ctx.fb.ins(&format!("local.get {}", obj), "receiver object pointer");
            ctx.fb.ins("i64.load offset=0", "runtime class id");
            ctx.fb.ins(&format!("local.set {}", cid), "receiver class id");
            ctx.fb.ins("block $nsdone", "nullsafe dispatch merge");
            for (class_id, class_name, impl_class) in &candidates {
                ctx.fb.ins(&format!("local.get {}", cid), "receiver class id");
                ctx.fb.ins(&format!("i64.const {}", *class_id as i64), "candidate class id");
                ctx.fb.ins("i64.eq", "matches this candidate?");
                ctx.fb.ins("if", "candidate class id arm");
                emit_candidate_call(ctx, inst, class_name, impl_class, &method_key, &method_name, &obj)?;
                ctx.fb.ins("br $nsdone", "candidate handled -> merge");
                ctx.fb.ins("end", "end candidate class id arm");
            }
            ctx.fb.ins(&format!("local.get {}", cid), "unmatched receiver class id");
            ctx.fb.ins(&format!("i32.const {}", method_ptr), "method-name pointer");
            ctx.fb.ins(&format!("i32.const {}", method_len), "method-name byte length");
            ctx.fb.ins(
                "call $__rt_fail_undefined_method",
                "raise PHP fatal for undefined object method",
            );
            ctx.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:nullsafe-undefined-method fatal helper does not return",
            );
            ctx.fb.ins("end", "end nullsafe dispatch merge");
            ctx.fb.ins("else", "receiver is neither null nor object");
            ctx.fb.ins(&format!("i32.const {}", method_ptr), "method-name pointer");
            ctx.fb.ins(&format!("i32.const {}", method_len), "method-name byte length");
            ctx.fb.ins(&format!("local.get {}", mtag), "receiver runtime tag");
            ctx.fb.ins("i32.wrap_i64", "runtime tag as i32");
            ctx.fb.ins(
                "call $__rt_fail_method_call_non_object",
                "raise PHP fatal for non-object receiver",
            );
            ctx.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:nullsafe-non-object-method fatal helper does not return",
            );
            ctx.fb.ins("end", "end receiver object test");
            ctx.fb.ins("end", "end receiver null test");
            Ok(())
        }
        other => Err(WasmError::Unsupported(format!(
            "nullsafe method on {:?} receiver (P6f)",
            other
        ))),
    }
}

/// Lowers an `Op::StaticMethodCall` to either a true static call or a lexical
/// instance-method call.
///
/// The immediate carries `"{Receiver}::{method}"` where `Receiver` is the
/// original-case receiver token (`self`, `parent`, a class name, …). True
/// static methods receive a constant `called_class_id` as hidden param 0;
/// `self::`/`parent::` calls that resolve to an instance method forward the
/// current `this` (slot 0) so `parent::__construct()` chains correctly. `static::`
/// late-bound dispatch is deferred (P6d scope) and rejected here.
pub(super) fn lower_static_method_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let target = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("static call: unknown data {:?}", data_id)))?
        .clone();
    let (receiver_label, method_name) = target
        .rsplit_once("::")
        .ok_or_else(|| WasmError::Unsupported(format!("malformed static call {}", target)))?;
    let method_key = php_symbol_key(method_name);

    if receiver_label == "static" {
        return Err(WasmError::Unsupported(format!(
            "static::{} late-bound dispatch (out of P6d scope)",
            method_name
        )));
    }

    let current_class: Option<String> = ctx
        .function
        .name
        .rsplit_once("::")
        .map(|(c, _)| c.to_string());
    let is_instance_fn = ctx.function.flags.is_method && !ctx.function.flags.is_static;

    let receiver_class = match receiver_label {
        "self" => current_class
            .clone()
            .ok_or_else(|| WasmError::Unsupported("self:: outside a method".to_string()))?,
        "parent" => {
            let cur = current_class
                .as_ref()
                .ok_or_else(|| WasmError::Unsupported("parent:: outside a method".to_string()))?;
            ctx.module
                .class_infos
                .get(cur)
                .and_then(|ci| ci.parent.clone())
                .ok_or_else(|| WasmError::Unsupported(format!("class {} has no parent", cur)))?
        }
        named => named.to_string(),
    };

    let ci = ctx
        .module
        .class_infos
        .get(&receiver_class)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", receiver_class)))?;

    let true_static = ci.static_methods.contains_key(&method_key);
    let lexical_instance = !true_static
        && (receiver_label == "self" || receiver_label == "parent")
        && is_instance_fn
        && ci.methods.contains_key(&method_key);

    // A `void` body pushes nothing, but PHP still gives its CALL EXPRESSION the value null
    // — the same rule the instance path applies. The EIR materializes an `I64 php=null`
    // result whenever it is used, so the null is supplied after the call.
    let body_returns_void = ctx
        .module
        .class_methods
        .iter()
        .any(|body| body.name.ends_with(&format!("::{}", method_name)) && body.return_type == IrType::Void
            && body.name.starts_with(&receiver_class));
    let return_arity = if body_returns_void {
        0
    } else {
        WasmRepr::val_types(inst.result_type).len()
    };

    if true_static {
        let impl_class = ci
            .static_method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| receiver_class.clone());
        let callee_symbol = method_symbol(&format!("{}::{}", impl_class, method_name));
        ctx.fb.ins(
            &format!("i64.const {}", ci.class_id as i64),
            &format!("{}::{} called_class_id", receiver_class, method_name),
        );
        for &arg in &inst.operands {
            ctx.emit_load_value(arg)?;
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (static)", receiver_class, method_name),
        );
    } else if lexical_instance {
        let impl_class = ci
            .method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| receiver_class.clone());
        let callee_symbol = method_symbol(&format!("{}::{}", impl_class, method_name));
        // Forward the current `this` (slot 0) as the receiver of the instance method.
        ctx.emit_load_slot(LocalSlotId::from_raw(0))?;
        for &arg in &inst.operands {
            ctx.emit_load_value(arg)?;
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (lexical instance via {}::)", impl_class, method_name, receiver_label),
        );
    } else {
        return Err(WasmError::Unsupported(format!(
            "unresolvable static call {} (static method not found; lexical instance fallback \
             not applicable)",
            target
        )));
    }

    if body_returns_void && inst.result.is_some() {
        ctx.fb.ins(
            "i64.const 9223372036854775806",
            "null sentinel: a void static call evaluates to null",
        );
    }
    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused static method result");
        }
    }
    Ok(())
}

/// Walks the parent chain from `class_name` upward and returns the topmost class
/// whose `vtable_slots` contains `method_key`.
///
/// That class is the *introducer* of the virtual method: the one whose dispatch
/// stub enumerates the whole subtree of possible runtime receiver class ids. All
/// callers whose static type sits in that subtree resolve to the same stub.
fn resolve_vtable_introducer(ctx: &FnCtx, class_name: &str, method_key: &str) -> Result<String> {
    let mut current = class_name.to_string();
    loop {
        let ci = ctx
            .module
            .class_infos
            .get(&current)
            .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", current)))?;
        match &ci.parent {
            Some(parent) => {
                let parent_ci = ctx
                    .module
                    .class_infos
                    .get(parent)
                    .ok_or_else(|| WasmError::Unsupported(format!("unknown parent {}", parent)))?;
                if parent_ci.vtable_slots.contains_key(method_key) {
                    current = parent.clone();
                    continue;
                }
                return Ok(current);
            }
            None => return Ok(current),
        }
    }
}

/// Emits one dispatch stub per (introducer, method key), for every virtual
/// (non-final) method in the module's class set.
///
/// Each stub's if-ladder covers exactly the concrete classes in the introducer's
/// subtree that carry the slot, tail-calling the implementation resolved via
/// `method_impl_classes`. An incomplete or ABI-heterogeneous subtree is skipped
/// wholesale: the capability gate rejects typed calls that cannot use one exact
/// stub signature, while Mixed/Union dispatch calls each selected implementation
/// directly. Omitting the unusable stub also prevents an unreferenced covariant
/// override from making the final Core module invalid.
pub(super) fn emit_method_dispatch_stubs(wm: &mut WatModule, module: &Module) -> Result<()> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (name, ci) in &module.class_infos {
        if let Some(parent) = &ci.parent {
            children
                .entry(parent.clone())
                .or_default()
                .push(name.clone());
        }
    }
    for class_children in children.values_mut() {
        class_children.sort();
    }

    let mut classes: Vec<_> = module.class_infos.iter().collect();
    classes.sort_by(|(left, left_ci), (right, right_ci)| {
        left_ci
            .class_id
            .cmp(&right_ci.class_id)
            .then_with(|| left.cmp(right))
    });
    for (introducer, ci) in classes {
        let mut method_keys: Vec<_> = ci.vtable_slots.keys().collect();
        method_keys.sort();
        for method_key in method_keys {
            let method_key = method_key.as_str();
            if ci.final_methods.contains(method_key) {
                continue;
            }
            // Only the introducer emits the stub: the parent must not also carry the slot.
            let is_introducer = match &ci.parent {
                None => true,
                Some(parent) => module
                    .class_infos
                    .get(parent)
                    .map(|p| !p.vtable_slots.contains_key(method_key))
                    .unwrap_or(true),
            };
            if !is_introducer {
                continue;
            }

            let mut subtree = collect_concrete_subtree(module, &children, introducer, method_key);
            subtree.sort_by(|left, right| {
                let left_id = module.class_infos.get(left).map(|class| class.class_id);
                let right_id = module.class_infos.get(right).map(|class| class.class_id);
                left_id.cmp(&right_id).then_with(|| left.cmp(right))
            });
            let mut arms: Vec<(u64, String)> = Vec::new();
            let mut sig_fn: Option<&Function> = None;
            let mut missing_body = false;
            let mut heterogeneous_abi = false;
            for class_name in &subtree {
                let class_ci = module
                    .class_infos
                    .get(class_name)
                    .ok_or_else(|| WasmError::Unsupported(format!("missing class {}", class_name)))?;
                let impl_class = class_ci
                    .method_impl_classes
                    .get(method_key)
                    .cloned()
                    .unwrap_or_else(|| class_name.clone());
                let Some(method) = find_method_function(
                    &module.class_methods,
                    &impl_class,
                    method_key,
                ) else {
                    missing_body = true;
                    continue;
                };
                if let Some(signature) = sig_fn {
                    heterogeneous_abi |= signature.return_type != method.return_type
                        || signature.params.len() != method.params.len()
                        || signature
                            .params
                            .iter()
                            .zip(&method.params)
                            .any(|(left, right)| left.ir_type != right.ir_type);
                }
                arms.push((class_ci.class_id, function_symbol(method)));
                if sig_fn.is_none() {
                    sig_fn = Some(method);
                }
            }
            if missing_body || heterogeneous_abi {
                continue;
            }
            arms.sort_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
            });
            let Some(sig_f) = sig_fn else {
                // No concrete implementer in the subtree: the method is never
                // dispatched at runtime, so no stub is needed.
                continue;
            };

            let stub_symbol = method_dispatch_symbol(introducer, method_key);
            let wat = build_dispatch_stub(&stub_symbol, sig_f, &arms);
            wm.add_raw_func(&wat);
        }
    }
    Ok(())
}

/// Collects the introducer plus all transitive subclasses that are concrete and
/// carry `method_key` in their vtable slots.
///
/// The result is exactly the set of runtime class ids a receiver typed anywhere
/// in the subtree can have, which is what the stub's if-ladder must cover.
fn collect_concrete_subtree(
    module: &Module,
    children: &HashMap<String, Vec<String>>,
    introducer: &str,
    method_key: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![introducer.to_string()];
    while let Some(name) = queue.pop() {
        let ci = match module.class_infos.get(&name) {
            Some(c) => c,
            None => continue,
        };
        if !ci.is_abstract && ci.vtable_slots.contains_key(method_key) {
            out.push(name.clone());
        }
        if let Some(kids) = children.get(&name) {
            for k in kids {
                queue.push(k.clone());
            }
        }
    }
    out
}

/// Finds the class-method `Function` that implements `method_key` for `impl_class`,
/// matching case-insensitively on the method name.
///
/// Returns the `Function` (whose `name` is `"{impl_class}::{original_method}"`)
/// so the caller can both form the call symbol and read the authoritative
/// parameter/result IR types for the stub signature.
pub(super) fn find_method_function<'a>(
    class_methods: &'a [Function],
    impl_class: &str,
    method_key: &str,
) -> Option<&'a Function> {
    class_methods.iter().find(|f| match f.name.rsplit_once("::") {
        Some((cls, m)) => cls == impl_class && php_symbol_key(m) == method_key,
        None => false,
    })
}

/// Builds the raw WAT body of a dispatch stub from the signature function and the
/// concrete (class_id, call symbol) arms.
///
/// The stub re-declares `this` plus the user parameters (skipping the signature's
/// `$this` param 0), reads the runtime class id, and branches to each arm. The
/// fall-through is `unreachable` because the closed class set guarantees a match.
fn build_dispatch_stub(stub_symbol: &str, sig_fn: &Function, arms: &[(u64, String)]) -> String {
    let mut wat = String::new();
    wat.push_str(&format!("(func ${}\n", stub_symbol));

    let mut param_decls: Vec<String> = Vec::new();
    let mut forward_loads: Vec<String> = Vec::new();
    let mut user_counter = 0u32;
    for (pi, p) in sig_fn.params.iter().enumerate() {
        for (vi, vt) in WasmRepr::val_types(p.ir_type).iter().enumerate() {
            let name = if pi == 0 && vi == 0 {
                "$this".to_string()
            } else {
                user_counter += 1;
                format!("$p{}", user_counter)
            };
            param_decls.push(format!("(param {} {})", name, vt.as_str()));
            forward_loads.push(format!("local.get {}", name));
        }
    }
    for pd in &param_decls {
        wat.push_str(&format!("  {}\n", pd));
    }

    let result_types = WasmRepr::val_types(sig_fn.return_type);
    if !result_types.is_empty() {
        let rstr = result_types
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        wat.push_str(&format!("  (result {})\n", rstr));
    }
    wat.push_str("  (local $cid i64)\n");

    wat.push_str("  ;; read the runtime class id from the object payload at +0\n");
    wat.push_str("  local.get $this\n");
    wat.push_str("  i64.load offset=0\n");
    wat.push_str("  local.set $cid\n");

    for (class_id, fn_symbol) in arms {
        wat.push_str(&format!(
            "  ;; dispatch arm for class id {}\n",
            class_id
        ));
        wat.push_str(&format!("  local.get $cid\n  i64.const {}\n  i64.eq\n  (if (then\n", *class_id as i64));
        for load in &forward_loads {
            wat.push_str(&format!("    {}\n", load));
        }
        wat.push_str(&format!("    call ${}\n    return))\n", fn_symbol));
    }

    wat.push_str("  ;; invalid/corrupted runtime class id: terminate through the shared failure boundary\n");
    wat.push_str(
        "  call $__rt_fail_callable_dispatch\n  unreachable ;; elephc-trap:post-noreturn:closed-method-dispatch-failure\n)\n",
    );
    wat
}
