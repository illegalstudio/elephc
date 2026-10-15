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

use super::classes::{
    mixed_method_arity_failures, mixed_method_candidates, mixed_tag_for_php_type,
    MixedMethodArityFailure,
};
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
/// The `Generator` methods this target answers from the frame, and how each one reads it.
///
/// A generator's methods have no BODIES — they read the frame, which is this backend's own
/// structure, so they are dispatched before any method-table lookup. `getReturn` is a plain load;
/// the other five go through `__rt_gen_*` because each has to PRIME the generator first, and php
/// primes on the first call of any of them.
fn generator_method_helper(name: &str) -> Option<(&'static str, GeneratorMethodShape)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "current" => ("$__rt_gen_current", GeneratorMethodShape::AnswersCell),
        "key" => ("$__rt_gen_key", GeneratorMethodShape::AnswersCell),
        "valid" => ("$__rt_gen_valid", GeneratorMethodShape::AnswersBool),
        "next" => ("$__rt_gen_next", GeneratorMethodShape::AnswersNothing),
        "send" => ("$__rt_gen_send", GeneratorMethodShape::TakesValueAnswersCell),
        // `throw($e)` answers what the body yields next, like `send($v)` — but what it takes is
        // an exception OBJECT, not a value. Boxing it the way a sent value is boxed parked a
        // Mixed cell in the frame, and the raise then threw the cell: the landing pad read its
        // first word as a class id, no `catch` clause matched, and the body rethrew to the
        // consumer. php's own `catch (Exception $e)` was right there.
        "throw" => ("$__rt_gen_throw", GeneratorMethodShape::TakesObjectAnswersCell),
        _ => return None,
    })
}

/// What one of those helpers leaves on the stack.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GeneratorMethodShape {
    /// A boxed Mixed cell the caller owns.
    AnswersCell,
    /// A php bool as an i64.
    AnswersBool,
    /// Nothing.
    AnswersNothing,
    /// Takes one boxed argument and answers a cell.
    TakesValueAnswersCell,
    /// Takes one object POINTER and answers a cell.
    TakesObjectAnswersCell,
}

/// Lowers `$gen->current()`, `key()`, `valid()`, `next()` and `send($v)`.
fn lower_generator_protocol_call(
    ctx: &mut FnCtx,
    inst: &Instruction,
    helper: &str,
    shape: GeneratorMethodShape,
) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let frame = super::objects::object_ptr_ref(ctx, receiver)?;
    ctx.fb
        .ins(&format!("local.get {frame}"), "the generator's frame");
    if shape == GeneratorMethodShape::TakesObjectAnswersCell {
        let exception = operand(inst, 1)?;
        let pointer = super::objects::object_ptr_ref(ctx, exception)?;
        ctx.fb
            .ins(&format!("local.get {pointer}"), "the exception to raise in the body");
    }
    if shape == GeneratorMethodShape::TakesValueAnswersCell {
        let value = operand(inst, 1)?;
        let (value_ir, value_php) = ctx
            .function
            .value(value)
            .map(|value| (value.ir_type, value.php_type.codegen_repr()))
            .ok_or_else(|| WasmError::Unsupported("sent value is missing".to_string()))?;
        ctx.emit_load_value(value)?;
        let _ = super::transfer::emit_convert_stack_value(
            ctx,
            value_ir,
            value_php,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )?;
    }
    ctx.fb.ins(&format!("call {helper}"), "the frame answers it");
    if shape == GeneratorMethodShape::AnswersNothing {
        return Ok(());
    }
    super::inst::store_result(ctx, inst)
}

/// Lowers `$gen->rewind()`: prime, or raise php's `Exception` if the generator has moved on.
///
/// php's rule is not "has it started" but "has it been ADVANCED": a rewind of an untouched
/// generator primes it, a rewind AT the first yield is a no-op, and a rewind after `next()` — or
/// after a `foreach` has moved — answers `Cannot rewind a generator that was already run`.
/// `FLAG_ADVANCED` is that distinction, which is why `FLAG_REWOUND` cannot answer it: priming
/// sets `FLAG_REWOUND`, and priming is exactly what the first case does.
///
/// The raise is emitted HERE rather than inside a runtime helper because a catchable raise needs
/// the module's class layout and message bytes — `emit_runtime_failure` is a lowering, not WAT —
/// and php surfaces the exception at the caller anyway.
fn lower_generator_rewind(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let frame = super::objects::object_ptr_ref(ctx, receiver)?;
    let flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS;
    let advanced = super::generators::FLAG_ADVANCED;
    ctx.fb.ins(
        &format!("(i32.and (i32.load offset={flags} (local.get {frame})) (i32.const {advanced}))"),
        "has a consumer already advanced this generator?",
    );
    ctx.fb.ins("if", "php refuses to rewind one that has run");
    super::inst::emit_runtime_failure(
        ctx,
        super::function::REWIND_ADVANCED_FAILURE_CODE,
        "report php's Cannot rewind a generator that was already run",
    );
    ctx.fb.ins("end", "end already-advanced check");
    ctx.fb.ins(
        &format!("(call $__rt_gen_prime (local.get {frame}))"),
        "a rewind of an untouched generator runs it to its first yield",
    );
    Ok(())
}

/// Lowers `$gen->getReturn()`: one load of the frame word the body's `return` filled.
///
/// A generator's methods have no bodies — they read the frame. `return` in a generator body is
/// not the call's answer (the call answered a `Generator` long before), it is what this reads,
/// which is why `function.rs`'s generator return terminator stores it instead of returning it.
///
/// The frame owns the cell, so the read takes a reference of its own.
fn lower_generator_get_return(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let frame = super::objects::object_ptr_ref(ctx, receiver)?;
    // php refuses unless the body ran to a `return`: an unstarted, suspended or exception-killed
    // generator has nothing to give back.
    let flags = crate::codegen_support::runtime::generators::frame::OFF_FLAGS;
    let returned = super::generators::FLAG_RETURNED;
    ctx.fb.ins(
        &format!("(i32.and (i32.load offset={flags} (local.get {frame})) (i32.const {returned}))"),
        "did the body return?",
    );
    ctx.fb.ins("i32.eqz", "");
    ctx.fb.ins("if", "php refuses getReturn() before the generator has returned");
    super::inst::emit_runtime_failure(
        ctx,
        super::function::GENERATOR_NO_RETURN_FAILURE_CODE,
        "report php's Cannot get return value of a generator that hasn't returned",
    );
    ctx.fb.ins("end", "end not-returned check");
    let cell = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!(
            "(i32.wrap_i64 (i64.load offset={} (local.get {frame})))",
            crate::codegen_support::runtime::generators::frame::OFF_RETURN_VALUE
        ),
        "what the generator body returned",
    );
    ctx.fb.ins(&format!("local.set {cell}"), "");
    ctx.fb.ins(&format!("local.get {cell}"), "");
    ctx.fb
        .ins("call $__rt_incref", "the frame keeps its own reference");
    ctx.fb.ins(&format!("local.get {cell}"), "the answer");
    super::inst::store_result(ctx, inst)
}

/// Lowers `Op::MethodCall`: a `Generator` method reads the frame, any other call resolves its
/// target from the receiver's static class, null-checks the receiver, and calls the body
/// directly or through the class-id dispatch ladder when an override can intervene.
pub(super) fn lower_method_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    // A `Generator` method reads the frame rather than running a body, so it is dispatched here
    // before any method-table lookup — there is nothing in the table to find.
    if let Some(name) = ctx
        .module
        .data
        .strings
        .get(data_immediate(inst).map(|data| data.as_raw() as usize).unwrap_or(usize::MAX))
    {
        let name = name.clone();
        if matches!(
            ctx.value_php_type(operand(inst, 0)?)?,
            PhpType::Object(ref class) if class.trim_start_matches('\\') == "Generator"
        ) {
            if name.eq_ignore_ascii_case("getReturn") {
                return lower_generator_get_return(ctx, inst);
            }
            if name.eq_ignore_ascii_case("rewind") {
                return lower_generator_rewind(ctx, inst);
            }
            if let Some((helper, shape)) = generator_method_helper(&name) {
                return lower_generator_protocol_call(ctx, inst, helper, shape);
            }
        }
    }
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

    // An INTERFACE-typed receiver has no class of its own to call into: the body belongs to
    // whichever concrete implementor actually arrives, which the object's own header names.
    // The capability audit has already proved every implementor shares one stub signature.
    if !ctx.module.class_infos.contains_key(&class_name)
        && ctx.module.interface_infos.contains_key(&class_name)
    {
        return lower_interface_method_call(
            ctx,
            inst,
            receiver,
            &class_name,
            &method_name,
            &method_key,
            method_ptr,
            method_len,
        );
    }

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
    // Before ANY of the paths below, including the open-coded accessor that returns without
    // dispatching: a raw object pointer can be 0 now that a missed `array<Object>` element
    // reads as null, and PHP names that case rather than reading through it.
    emit_null_receiver_check(ctx, receiver, method_ptr, method_len)?;

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
    let declared_name =
        declared_method_name(&ctx.module.class_methods, &impl_class, &method_key, &method_name);
    let callee_symbol = if dynamic {
        let introducer = resolve_vtable_introducer(ctx, &class_name, &method_key)?;
        method_dispatch_symbol(&introducer, &method_key)
    } else {
        method_symbol(&format!("{}::{}", impl_class, declared_name))
    };
    let mode = if dynamic { "dispatch" } else { "direct" };

    // `void` bodies push nothing; the call expression's null is supplied after the call.
    let body_returns_void = ctx
        .module
        .class_methods
        .iter()
        .find(|body| body.name == format!("{}::{}", impl_class, declared_name))
        .map(|body| body.return_type == IrType::Void)
        .unwrap_or(false);
    let return_arity = if body_returns_void {
        0
    } else {
        WasmRepr::val_types(inst.result_type).len()
    };
    // A CONCRETE scalar reaching a `mixed` parameter is boxed at the call site; the audit admits
    // exactly the shapes with an exact tag and payload. The caller owns the argument — parameter
    // slots are excluded from the callee's epilogue cleanup — so each minted cell is released
    // after the call rather than leaked per invocation.
    let body_params: Vec<crate::ir::FunctionParam> = ctx
        .module
        .class_methods
        .iter()
        .find(|body| body.name == format!("{}::{}", impl_class, declared_name))
        .map(|body| body.params.clone())
        .unwrap_or_default();
    ctx.emit_load_value(receiver)?;
    let mut minted_cells: Vec<String> = Vec::new();
    for (index, &arg) in inst.operands.iter().skip(1).enumerate() {
        if let Some(cell) = push_call_argument(ctx, arg, body_params.get(index + 1))? {
            minted_cells.push(cell);
        }
    }
    ctx.fb.ins(
        &format!("call ${}", callee_symbol),
        &format!("{}::{} ({})", class_name, method_name, mode),
    );
    release_boxed_arguments(ctx, &minted_cells);

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

/// Emits a no-argument call to `class_name::method_key` on an already-checked receiver, leaving
/// the body's result on the stack.
///
/// Shares `lower_method_call`'s dispatch decision: a method that occupies a vtable slot and is
/// not final is reached through its introducer's stub, so an overriding subclass still wins;
/// anything else is called by symbol. Callers that synthesise such a call — `count()` on a
/// Countable is the one today — must not re-derive that rule.
pub(super) fn emit_no_arg_method_call(
    ctx: &mut FnCtx,
    receiver: crate::ir::ValueId,
    class_name: &str,
    method_key: &str,
    method_name: &str,
    method_ptr: u32,
    method_len: u32,
) -> Result<()> {
    let ci = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    let dynamic = ci.vtable_slots.contains_key(method_key) && !ci.final_methods.contains(method_key);
    let impl_class = ci
        .method_impl_classes
        .get(method_key)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    let declared_name =
        declared_method_name(&ctx.module.class_methods, &impl_class, method_key, method_name);
    let callee_symbol = if dynamic {
        let introducer = resolve_vtable_introducer(ctx, class_name, method_key)?;
        method_dispatch_symbol(&introducer, method_key)
    } else {
        method_symbol(&format!("{}::{}", impl_class, declared_name))
    };
    // The same null check an ordinary call makes: a raw object pointer can be 0, and PHP names
    // that case rather than reading through it.
    emit_null_receiver_check(ctx, receiver, method_ptr, method_len)?;
    ctx.emit_load_value(receiver)?;
    ctx.fb.ins(
        &format!("call ${}", callee_symbol),
        &format!(
            "{}::{} ({})",
            class_name,
            method_name,
            if dynamic { "dispatch" } else { "direct" }
        ),
    );
    Ok(())
}

/// Lowers a method call whose receiver is typed by an INTERFACE.
///
/// The receiver is one object pointer whose header names its real class, so the call goes
/// through the same class-id if-ladder an ordinary virtual call uses — only the arm set
/// differs: the interface's concrete implementors rather than one class's subtree. The
/// capability audit proved that set shares a signature before this runs, and the stub itself
/// is emitted by `emit_method_dispatch_stubs` from the same candidate list.
///
/// `void` needs the same treatment it gets on the direct path: the body pushes nothing, but
/// PHP still gives the call EXPRESSION the value null, so it is supplied after the call.
fn lower_interface_method_call(
    ctx: &mut FnCtx,
    inst: &Instruction,
    receiver: ValueId,
    interface_name: &str,
    method_name: &str,
    method_key: &str,
    method_ptr: u32,
    method_len: u32,
) -> Result<()> {
    let candidates =
        super::capability::interface_dispatch_candidates(ctx.module, interface_name, method_key)
            .map_err(WasmError::Unsupported)?;
    let body_returns_void = candidates
        .first()
        .and_then(|(_, implementation)| {
            find_method_function(&ctx.module.class_methods, implementation, method_key)
        })
        .map(|body| body.return_type == IrType::Void)
        .unwrap_or(false);
    let return_arity = if body_returns_void {
        0
    } else {
        WasmRepr::val_types(inst.result_type).len()
    };

    // The audit proved every implementor shares one signature, so any body's parameters are
    // THE parameters this call is compiled against. An open-coded `Throwable` accessor has no
    // body and takes no arguments, which is why an empty list is a correct answer here.
    let stub_params: Vec<crate::ir::FunctionParam> = candidates
        .iter()
        .find_map(|(_, implementation)| {
            find_method_function(&ctx.module.class_methods, implementation, method_key)
        })
        .map(|body| body.params.clone())
        .unwrap_or_default();

    emit_null_receiver_check(ctx, receiver, method_ptr, method_len)?;
    ctx.emit_load_value(receiver)?;
    let mut minted_cells: Vec<String> = Vec::new();
    for (index, &arg) in inst.operands.iter().skip(1).enumerate() {
        if let Some(cell) = push_call_argument(ctx, arg, stub_params.get(index + 1))? {
            minted_cells.push(cell);
        }
    }
    ctx.fb.ins(
        &format!("call ${}", method_dispatch_symbol(interface_name, method_key)),
        &format!("{}::{} (interface dispatch)", interface_name, method_name),
    );
    release_boxed_arguments(ctx, &minted_cells);

    if body_returns_void && inst.result.is_some() {
        ctx.fb.ins(
            "i64.const 9223372036854775806",
            "null sentinel: a void method call evaluates to null",
        );
    }
    if let Some(result) = inst.result {
        ctx.emit_store_value(result)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused interface method result");
        }
    }
    Ok(())
}

/// Terminates with PHP's own message when the receiver is null, leaving the stack untouched.
///
/// Separate from loading the receiver because not every path loads it the same way: the
/// open-coded `Throwable` accessor takes an address through `object_ptr_ref` rather than a stack
/// value, and it is resolved BEFORE dispatch — so a guard fused to the dispatch load would miss
/// it entirely, and did:
///
/// ```text
///   $a = [new Exception("boom")];  $e = $a[9];  echo $e->getMessage();
///   php-src: Warning: Undefined array key 9
///            Fatal error: Uncaught Error: Call to a member function getMessage() on null
///   fused guard: prints an empty message and CONTINUES — it read address 0 + offset
/// ```
/// Lowers `$obj[$key]` where the receiver is an `ArrayAccess` implementor, dispatching to
/// `offsetGet`.
///
/// The EIR emits this as an UNTYPED runtime call — no immediate names the method — carrying
/// `(receiver, key, warn_flag)`. The native backend reads the same shape in
/// `try_lower_array_access_runtime_call`: a result marks it a read, and the trailing
/// warn-on-missing flag is dropped so `offsetGet` keeps its one-argument PHP signature. Only
/// that flag distinguishes a read from `offsetSet`, which lowers void.
///
/// `lower_method_call` cannot be reused as-is for two reasons, both measured: it needs an
/// `Immediate::Data` naming the method, and `offsetGet` is NOT in the module string table — a
/// program that only ever writes `$obj[$k]` never mentions the name — and it loads arguments
/// raw, whereas `offsetGet(mixed $offset)` takes a boxed cell while the key arrives as a bare
/// string or int. So the name comes from static data laid out by `plan_module`, and the key is
/// boxed here.
pub(super) fn lower_array_access_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    lower_array_access(ctx, inst, true)
}

/// Lowers `$obj[$key] = $value` on an `ArrayAccess` implementor, dispatching to `offsetSet`.
pub(super) fn lower_array_access_set(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    lower_array_access(ctx, inst, false)
}

/// Pushes one call argument, boxing a concrete scalar when the callee's parameter is `mixed`.
///
/// Returns the minted cell when it boxed, so the caller can release it after the call: the
/// caller owns the argument, since parameter slots are excluded from the callee's epilogue.
fn push_call_argument(
    ctx: &mut FnCtx,
    arg: ValueId,
    parameter: Option<&crate::ir::FunctionParam>,
) -> Result<Option<String>> {
    let boxes = ctx
        .function
        .value(arg)
        .zip(parameter)
        .is_some_and(|(value, parameter)| {
            super::capability::argument_boxes_into_a_mixed_parameter(value, parameter)
        });
    // An `array<T>` reaching an `array<mixed>` parameter is a real element-wise conversion,
    // so it goes through the transfer layer rather than a bare load. The fresh array it builds
    // has no other owner — parameter slots are excluded from the callee's epilogue — so it joins
    // the minted cells the call site releases afterwards.
    let widens = ctx
        .function
        .value(arg)
        .zip(parameter)
        .is_some_and(|(value, parameter)| {
            super::capability::argument_widens_into_an_array_parameter(value, parameter)
        });
    if widens {
        let parameter = parameter.expect("widening was decided from a parameter");
        // The fresh array the conversion builds IS the reference the callee owns and releases,
        // so it is neither retained again here nor freed after the call.
        super::transfer::emit_push_call_argument(
            ctx,
            arg,
            parameter.ir_type,
            parameter.php_type.codegen_repr(),
        )?;
        return Ok(None);
    }
    // `f(?int $n)` called as `f(5)` or `f(null)` needs the `(payload, tag)` pair made here:
    // the callee takes two words and a bare load pushes one, which the module validator
    // rejects rather than the audit. Nothing is minted — a tagged pair holds no heap value —
    // so there is nothing to release afterwards either.
    let tags = ctx
        .function
        .value(arg)
        .zip(parameter)
        .is_some_and(|(value, parameter)| {
            super::capability::argument_tags_into_a_nullable_parameter(value, parameter)
        });
    if tags {
        let parameter = parameter.expect("tagging was decided from a parameter");
        super::transfer::emit_push_call_argument(
            ctx,
            arg,
            parameter.ir_type,
            parameter.php_type.codegen_repr(),
        )?;
        return Ok(None);
    }
    // And the other direction: a spread's `int|null` element reaching a declared `int`. Two
    // words arrive and one is wanted, so a bare load pushes the wrong arity and the module
    // fails validation rather than the audit refusing it.
    let narrows = ctx
        .function
        .value(arg)
        .zip(parameter)
        .is_some_and(|(value, parameter)| {
            super::capability::argument_narrows_out_of_a_tagged_value(value, parameter)
        });
    if narrows {
        let parameter = parameter.expect("narrowing was decided from a parameter");
        super::transfer::emit_push_call_argument(
            ctx,
            arg,
            parameter.ir_type,
            parameter.php_type.codegen_repr(),
        )?;
        return Ok(None);
    }
    let boxes_container = ctx
        .function
        .value(arg)
        .zip(parameter)
        .is_some_and(|(value, parameter)| {
            super::capability::argument_boxes_a_container_into_a_mixed_parameter(value, parameter)
        });
    if boxes_container {
        let parameter = parameter.expect("boxing was decided from a parameter");
        // The cell the transfer mints holds its own reference to the container; the callee
        // borrows it, so the call site releases it with the scalar cells afterwards.
        let minted = super::transfer::emit_push_call_argument(
            ctx,
            arg,
            parameter.ir_type,
            parameter.php_type.codegen_repr(),
        )?;
        return Ok(minted.map(|(cell, _)| cell));
    }
    if !boxes {
        ctx.emit_load_value(arg)?;
        let lends = parameter.is_some_and(|parameter| {
            parameter.ir_type == IrType::Heap(IrHeapKind::Array)
                && super::inst::argument_reaches_parameter_unconverted(
                    ctx,
                    arg,
                    &parameter.php_type,
                )
        });
        if lends {
            // The callee OWNS its array parameter and releases it at every exit, so the caller
            // lends a counted reference and never takes it back — the same contract
            // `lower_call` honours for a free function. Without it, every call decremented the
            // CALLER's array: the first still read right, the second read freed memory.
            // Only when the value is passed AS-IS: a converted one is a fresh array whose
            // single reference is already the one being handed over.
            let array = ctx.fresh_temp(ValType::I32);
            ctx.fb.ins(&format!("local.tee {}", array), "array argument");
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", array),
                "the callee owns its array parameter",
            );
        }
        return Ok(None);
    }
    let repr = ctx.value_repr(arg)?.clone();
    let cell = super::inst::box_value_into_mixed_cell(ctx, arg, &repr)?;
    ctx.fb.ins(
        &format!("local.get {}", cell),
        "argument boxed for a `mixed` parameter",
    );
    Ok(Some(cell))
}

/// Returns the parameter list of one compiled method body, for the boxing decision above.
fn method_body_params(ctx: &FnCtx, qualified_name: &str) -> Vec<crate::ir::FunctionParam> {
    ctx.module
        .class_methods
        .iter()
        .find(|body| body.name == qualified_name)
        .map(|body| body.params.clone())
        .unwrap_or_default()
}

/// Releases every cell minted for one call's arguments.
fn release_boxed_arguments(ctx: &mut FnCtx, minted: &[String]) {
    for cell in minted {
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {}))", cell),
            "release the boxed argument",
        );
    }
}

/// Lowers a read of an UNDECLARED property to the class's `__get($name)`.
///
/// PHP does not read storage here at all — it calls the magic accessor with the property name
/// as a string. The name is already interned, since the source spells it, but `__get` itself is
/// laid out by `plan_module` for the null-receiver check, exactly as `offsetGet` is.
pub(super) fn lower_magic_property_get(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let receiver = operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    let class_name = exact_static_object_class(&receiver_ty).ok_or_else(|| {
        WasmError::Unsupported(format!("__get on receiver {:?}", receiver_ty))
    })?;
    let property = ctx
        .module
        .data
        .strings
        .get(data_immediate(inst)?.as_raw() as usize)
        .cloned()
        .ok_or_else(|| WasmError::Unsupported("__get without a property name".to_string()))?;
    let (name_ptr, name_len) = ctx.str_literal(data_immediate(inst)?)?;
    let method_key = php_symbol_key("__get");
    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    let dynamic =
        ci.vtable_slots.contains_key(&method_key) && !ci.final_methods.contains(&method_key);
    let impl_class = ci
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let (method_ptr, method_len) = ctx.default_str_literal("__get")?;
    emit_null_receiver_check(ctx, receiver, method_ptr, method_len)?;
    let callee_symbol = if dynamic {
        let introducer = resolve_vtable_introducer(ctx, &class_name, &method_key)?;
        method_dispatch_symbol(&introducer, &method_key)
    } else {
        method_symbol(&format!("{}::__get", impl_class))
    };
    // `__get($name)` with an untyped parameter is inferred `string`, so the literal is passed
    // as a pointer and a length; only a parameter actually declared `mixed` needs a box.
    let name_is_mixed = ctx
        .module
        .class_methods
        .iter()
        .find(|body| body.name == format!("{}::__get", impl_class))
        .and_then(|body| body.params.get(1))
        .is_some_and(|parameter| parameter.ir_type == IrType::Heap(IrHeapKind::Mixed));
    let cell = name_is_mixed.then(|| ctx.fresh_temp(super::wat::ValType::I32));
    if let Some(cell) = &cell {
        ctx.fb.ins(
            &format!("(call $__rt_mixed_from_value (i64.const 1) (i64.extend_i32_u (i32.const {name_ptr})) (i64.const {name_len}))"),
            "the property name, boxed for `mixed $name`",
        );
        ctx.fb.ins(&format!("local.set {}", cell), "hold the name box");
    }
    ctx.emit_load_value(receiver)?;
    match &cell {
        Some(cell) => ctx.fb.ins(&format!("local.get {}", cell), "the property name"),
        None => {
            ctx.fb
                .ins(&format!("i32.const {name_ptr}"), "property name pointer");
            ctx.fb
                .ins(&format!("i64.const {name_len}"), "property name length");
        }
    }
    ctx.fb.ins(
        &format!("call ${}", callee_symbol),
        &format!(
            "{}::__get(\"{}\") ({})",
            class_name,
            property,
            if dynamic { "dispatch" } else { "direct" }
        ),
    );
    let result = inst
        .result
        .ok_or_else(|| WasmError::Unsupported("__get has no result".to_string()))?;
    ctx.emit_store_value(result)?;
    if let Some(cell) = &cell {
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {}))", cell),
            "release the name box",
        );
    }
    Ok(())
}

/// Lowers an `ArrayAccess` read (`offsetGet`) or write (`offsetSet`) on an object receiver of one
/// exact static class by calling that class's method with the key (and value).
fn lower_array_access(ctx: &mut FnCtx, inst: &Instruction, is_read: bool) -> Result<()> {
    let method_name = if is_read { "offsetGet" } else { "offsetSet" };
    let receiver = operand(inst, 0)?;
    let key = operand(inst, 1)?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    let class_name = exact_static_object_class(&receiver_ty).ok_or_else(|| {
        WasmError::Unsupported(format!("ArrayAccess read on receiver {:?}", receiver_ty))
    })?;
    let method_key = php_symbol_key(method_name);
    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    let dynamic = ci.vtable_slots.contains_key(&method_key) && !ci.final_methods.contains(&method_key);
    let impl_class = ci
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let (method_ptr, method_len) = ctx.default_str_literal(method_name)?;
    emit_null_receiver_check(ctx, receiver, method_ptr, method_len)?;
    let callee_symbol = if dynamic {
        let introducer = resolve_vtable_introducer(ctx, &class_name, &method_key)?;
        method_dispatch_symbol(&introducer, &method_key)
    } else {
        method_symbol(&format!("{}::{}", impl_class, method_name))
    };
    // Both parameters are declared `mixed`, so every argument is boxed here.
    let mut minted: Vec<String> = Vec::new();
    ctx.emit_load_value(receiver)?;
    let key_repr = ctx.value_repr(key)?.clone();
    let key_cell = super::inst::box_value_into_mixed_cell(ctx, key, &key_repr)?;
    ctx.fb.ins(
        &format!("local.get {}", key_cell),
        "the offset, boxed for `mixed $offset`",
    );
    minted.push(key_cell);
    if !is_read {
        let value = operand(inst, 2)?;
        let value_repr = ctx.value_repr(value)?.clone();
        let value_cell = super::inst::box_value_into_mixed_cell(ctx, value, &value_repr)?;
        ctx.fb.ins(
            &format!("local.get {}", value_cell),
            "the value, boxed for `mixed $value`",
        );
        minted.push(value_cell);
    }
    ctx.fb.ins(
        &format!("call ${}", callee_symbol),
        &format!(
            "{}::{} ({})",
            class_name,
            method_name,
            if dynamic { "dispatch" } else { "direct" }
        ),
    );
    if is_read {
        let result = inst
            .result
            .ok_or_else(|| WasmError::Unsupported("ArrayAccess read has no result".to_string()))?;
        ctx.emit_store_value(result)?;
    }
    // Each cell was minted for this call alone. The callee borrows its arguments — it acquires
    // whatever it keeps — so the caller's single reference is dropped here rather than leaked
    // once per subscript.
    for cell in &minted {
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {}))", cell),
            "release the boxed subscript argument",
        );
    }
    Ok(())
}

/// Emits the guard that raises php's own `Error` (call to a member function on null) when
/// `receiver` is a null pointer, naming the method from the data segment at `method_ptr`.
fn emit_null_receiver_check(
    ctx: &mut FnCtx,
    receiver: ValueId,
    method_ptr: u32,
    method_len: u32,
) -> Result<()> {
    let pointer = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(receiver)?;
    ctx.fb
        .ins(&format!("local.set {}", pointer), "the method receiver");
    ctx.fb.ins(
        &format!("(if (i32.eqz (local.get {}))", pointer),
        "a null receiver is PHP's own Error, not a dispatch failure",
    );
    ctx.fb.ins(
        &format!(
            "(then (call $__rt_fail_method_call_non_object (i32.const {method_ptr}) (i32.const {method_len}) (i32.const 8))))"
        ),
        "Call to a member function X() on null",
    );
    Ok(())
}


/// Open-codes `Enum::cases()` and `Enum::tryFrom()`, which PHP synthesizes and no body backs.
///
/// `cases()` materializes every case, in DECLARATION order, into a pointer-slot array under
/// `value_type` 4 — the same shape any `array<Object>` uses, so `count()`, `foreach` and an
/// indexed read all reach it without a special case. The array takes a SHARE of each singleton.
///
/// `tryFrom()` walks the same cases as an equality ladder over the BACKING value and boxes the
/// winner into a Mixed cell under tag 6; a miss boxes null under tag 8, which is exactly what
/// makes `tryFrom(...) ?? Default` and `is_null(...)` answer the way php-src does.
fn lower_enum_static_intrinsic(
    ctx: &mut FnCtx,
    inst: &Instruction,
    enum_name: &str,
    method_key: &str,
) -> Result<()> {
    let (cases, backing_type) = ctx
        .module
        .enum_infos
        .get(enum_name)
        .map(|info| (info.cases.clone(), info.backing_type.clone()))
        .ok_or_else(|| WasmError::Unsupported(format!("{enum_name} is not an enum")))?;

    // Each case's singleton slot is placed by `statics`; resolving here keeps the emitter and
    // `Op::ScopedConstantGet` reading the same address for the same case.
    let mut placed = Vec::with_capacity(cases.len());
    for case in &cases {
        let label = format!("{enum_name}::{}", case.name);
        let (slot, _, _) = super::statics::resolve_enum_case(ctx.module, ctx.static_slots, &label)
            .ok_or_else(|| {
                WasmError::Unsupported(format!("enum case {label} has no singleton slot"))
            })?;
        placed.push((case.clone(), slot.address));
    }

    if method_key == "from" {
        return lower_enum_from(ctx, inst, enum_name, backing_type.as_ref(), &placed);
    }

    if method_key == "cases" {
        let array = ctx.fresh_temp(ValType::I32);
        ctx.fb.ins(
            &format!("(i64.const {})", placed.len()),
            "one slot per declared case",
        );
        ctx.fb.ins("i64.const 8", "pointer slots");
        ctx.fb
            .ins("call $__rt_array_new", "fresh array for the cases");
        ctx.fb.ins(&format!("local.set {}", array), "the cases array");
        for (case, address) in &placed {
            let singleton =
                super::inst::emit_enum_case_singleton(ctx, enum_name, &case.name, case.value.as_ref(), *address)?;
            ctx.fb.ins(
                &format!("(call $__rt_incref (local.get {}))", singleton),
                "the array shares the case singleton",
            );
            ctx.fb.ins(&format!("local.get {}", array), "the cases array");
            ctx.fb
                .ins(&format!("local.get {}", singleton), "the case singleton");
            ctx.fb.ins("i64.const 4", "value_type 4 (object) for the slot");
            ctx.fb.ins(
                "call $__rt_array_push_ptr",
                "append the case (may reallocate)",
            );
            ctx.fb.ins(&format!("local.set {}", array), "keep the live pointer");
        }
        ctx.fb.ins(&format!("local.get {}", array), "the completed cases array");
        return super::inst::store_result(ctx, inst);
    }

    // tryFrom: an equality ladder over the backing value, boxing the winner or null.
    //
    // The needle is read into locals ONCE, before the ladder: a string arrives as a
    // (pointer, length) pair, and re-evaluating the operand per case would both duplicate the
    // work and re-run any effects the operand carries.
    let needle = super::inst::operand(inst, 0)?;
    let needle_ptr = ctx.fresh_temp(ValType::I32);
    let needle_len = ctx.fresh_temp(ValType::I64);
    let needle_int = ctx.fresh_temp(ValType::I64);
    // The needle's WIDTH comes from the enum's DECLARED backing type, never from the cases: a
    // string arrives as a (pointer, length) pair and an int as one value, and `enum E: string {}`
    // with no cases at all is legal PHP. Reading the width off the case list made that enum take
    // the int path, popping one operand from a two-operand push and leaving the pointer behind —
    // `values remaining on stack at end of block`, rejected by wasm validation, for a program
    // php-src answers with NULL.
    let string_backed = matches!(
        backing_type.as_ref().map(PhpType::codegen_repr),
        Some(PhpType::Str)
    );
    ctx.emit_load_value(needle)?;
    if string_backed {
        ctx.fb.ins(&format!("local.set {}", needle_len), "needle length");
        ctx.fb.ins(&format!("local.set {}", needle_ptr), "needle bytes");
    } else {
        ctx.fb.ins(&format!("local.set {}", needle_int), "needle value");
    }

    let result = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        "(call $__rt_mixed_from_value (i64.const 8) (i64.const 0) (i64.const 0))",
        "start from php-src's answer for no match: null",
    );
    ctx.fb.ins(&format!("local.set {}", result), "the tryFrom result");
    let matched = ctx.fresh_temp(ValType::I32);
    for (case, address) in &placed {
        let Some(value) = case.value.as_ref() else {
            return Err(WasmError::Unsupported(format!(
                "enum case {enum_name}::{} has no backing value",
                case.name
            )));
        };
        match value {
            crate::types::EnumCaseValue::Int(number) => {
                ctx.fb.ins(&format!("local.get {}", needle_int), "the needle");
                ctx.fb.ins(&format!("i64.const {number}"), "this case's backing value");
                ctx.fb.ins("i64.eq", "does the needle match this case?");
            }
            crate::types::EnumCaseValue::Str(text) => {
                let (pointer, length) = ctx.default_str_literal(text)?;
                // The length is checked FIRST and separately: `__rt_str_region_eq` reads
                // `length` bytes of the needle, so comparing a shorter needle against a longer
                // case would read past its end.
                ctx.fb.ins(&format!("(local.set {} (i32.const 0))", matched), "assume no match");
                ctx.fb.ins(
                    &format!(
                        "(if (i64.eq (local.get {}) (i64.const {length}))",
                        needle_len
                    ),
                    "same length?",
                );
                ctx.fb.ins(
                    &format!(
                        "(then (local.set {} (i32.wrap_i64 (call $__rt_str_region_eq (local.get {}) (i32.const {pointer}) (i64.const {length}) (i64.const 0))))))",
                        matched, needle_ptr
                    ),
                    "then compare the bytes",
                );
                ctx.fb.ins(&format!("local.get {}", matched), "does the needle match this case?");
            }
        }
        ctx.fb.ins("(if (then", "this case is the match");
        let singleton = super::inst::emit_enum_case_singleton(
            ctx,
            enum_name,
            &case.name,
            case.value.as_ref(),
            *address,
        )?;
        // The cell takes a SHARE of the singleton, and the null cell built above is dropped.
        ctx.fb.ins(
            &format!("(call $__rt_decref_any (local.get {}))", result),
            "release the placeholder this match replaces",
        );
        ctx.fb.ins("i64.const 6", "Mixed object tag");
        ctx.fb.ins(
            &format!("(i64.extend_i32_u (local.get {}))", singleton),
            "the matching case singleton",
        );
        ctx.fb.ins("i64.const 0", "unused high payload");
        ctx.fb.ins(
            "call $__rt_mixed_from_value",
            "box the case as the tryFrom result",
        );
        ctx.fb.ins(&format!("local.set {}", result), "keep the boxed case");
        ctx.fb.ins("))", "close this case's match");
    }
    ctx.fb.ins(&format!("local.get {}", result), "the tryFrom result");
    super::inst::store_result(ctx, inst)
}

/// php-src's own suffix for the `ValueError` a failed `Enum::from()` raises.
///
/// Byte-for-byte php-src 8.5.6's, and the reason `from()` could not simply answer null: the
/// class, the offending value and this text together are what a `catch (ValueError $e)` reads.
const ENUM_FROM_SUFFIX: &str = " is not a valid backing value for enum ";

/// Lowers `Enum::from()`: the `tryFrom` ladder whose no-match arm RAISES instead of answering.
///
/// Splitting this from `tryFrom` rather than sharing the ladder is deliberate — the two differ
/// in more than the miss:
///
/// * the result is the singleton POINTER, not a boxed cell, because the EIR types `from()`
///   `Heap(Object)` and `tryFrom()` Mixed. So the match increfs the singleton and hands it back
///   directly;
/// * and the miss composes a message. php names the value: `"zz" is not a valid backing value
///   for enum E` for a string backing, quotes included, and `3 is not …` for an int one.
///
/// The compose is a concat of a static prefix, the needle rendered as php renders it, and a
/// static suffix carrying the enum's own name — which is why the enum name is laid out as a
/// default string rather than read from anywhere at run time.
fn lower_enum_from(
    ctx: &mut FnCtx,
    inst: &Instruction,
    enum_name: &str,
    backing_type: Option<&PhpType>,
    placed: &[(crate::types::EnumCaseInfo, u32)],
) -> Result<()> {
    let string_backed = matches!(
        backing_type.map(PhpType::codegen_repr),
        Some(PhpType::Str)
    );
    let needle = super::inst::operand(inst, 0)?;
    let needle_ptr = ctx.fresh_temp(ValType::I32);
    let needle_len = ctx.fresh_temp(ValType::I64);
    let needle_int = ctx.fresh_temp(ValType::I64);
    ctx.emit_load_value(needle)?;
    if string_backed {
        ctx.fb.ins(&format!("local.set {}", needle_len), "needle length");
        ctx.fb.ins(&format!("local.set {}", needle_ptr), "needle bytes");
    } else {
        ctx.fb.ins(&format!("local.set {}", needle_int), "needle value");
    }

    let found = ctx.fresh_temp(ValType::I32);
    let matched = ctx.fresh_temp(ValType::I32);
    ctx.fb
        .ins(&format!("(local.set {} (i32.const 0))", found), "no case yet");
    for (case, address) in placed {
        let Some(value) = case.value.as_ref() else {
            return Err(WasmError::Unsupported(format!(
                "enum case {enum_name}::{} has no backing value",
                case.name
            )));
        };
        match value {
            crate::types::EnumCaseValue::Int(number) => {
                ctx.fb.ins(&format!("local.get {}", needle_int), "the needle");
                ctx.fb
                    .ins(&format!("i64.const {number}"), "this case's backing value");
                ctx.fb.ins("i64.eq", "does the needle match this case?");
            }
            crate::types::EnumCaseValue::Str(text) => {
                let (pointer, length) = ctx.default_str_literal(text)?;
                // Length first and separately, as the tryFrom ladder does: the byte comparison
                // reads `length` bytes of the needle, so a shorter needle would be read past.
                ctx.fb
                    .ins(&format!("(local.set {} (i32.const 0))", matched), "assume no match");
                ctx.fb.ins(
                    &format!("(if (i64.eq (local.get {}) (i64.const {length}))", needle_len),
                    "same length?",
                );
                ctx.fb.ins(
                    &format!(
                        "(then (local.set {} (i32.wrap_i64 (call $__rt_str_region_eq (local.get {}) (i32.const {pointer}) (i64.const {length}) (i64.const 0))))))",
                        matched, needle_ptr
                    ),
                    "then compare the bytes",
                );
                ctx.fb
                    .ins(&format!("local.get {}", matched), "does the needle match this case?");
            }
        }
        ctx.fb.ins("(if (then", "this case is the match");
        let singleton = super::inst::emit_enum_case_singleton(
            ctx,
            enum_name,
            &case.name,
            case.value.as_ref(),
            *address,
        )?;
        // The EIR types this result `own=owned` and releases it, so the caller's reference is
        // its own — the singleton itself stays alive for the program's life either way.
        ctx.fb.ins(
            &format!("(call $__rt_incref (local.get {}))", singleton),
            "the caller owns what it receives",
        );
        ctx.fb
            .ins(&format!("(local.set {} (local.get {}))", found, singleton), "the matching case");
        ctx.fb.ins("))", "close this case's match");
    }

    // The miss. php raises here rather than answering, which is the whole difference from
    // `tryFrom`, and the message names the value that did not match.
    ctx.fb
        .ins(&format!("(if (i32.eqz (local.get {}))", found), "nothing matched?");
    ctx.fb.ins("(then", "then raise php's ValueError");
    //
    // Every step of the compose ALLOCATES an owned string — `__rt_fmt_int` through
    // `__rt_str_alloc`, and `__rt_concat` through `__rt_heap_alloc` — so each intermediate is
    // freed once the next step has copied out of it. Leaving them grew 142 bytes per raise,
    // where a hand-written `throw new ValueError("…" . $v)` in the same loop was flat: the
    // control is what said the leak was this lowering's and not the catch path's.
    //
    // The last concat's result is already the owned bytes the message slot takes, so there is
    // no `__rt_str_persist` here — a copy at the end would strand the concat's own block.
    let suffix = format!("{ENUM_FROM_SUFFIX}{enum_name}");
    let part_ptr = ctx.fresh_temp(ValType::I32);
    let part_len = ctx.fresh_temp(ValType::I64);
    if string_backed {
        let (quote_ptr, quote_len) = ctx.default_str_literal("\"")?;
        let (tail_ptr, tail_len) = ctx.default_str_literal(&format!("\"{suffix}"))?;
        ctx.fb.ins(
            &format!("(call $__rt_concat (i32.const {quote_ptr}) (i64.const {quote_len}) (local.get {needle_ptr}) (local.get {needle_len}))"),
            "php quotes a string backing value",
        );
        ctx.fb.ins(&format!("local.set {part_len}"), "the opening quote and the value");
        ctx.fb.ins(&format!("local.set {part_ptr}"), "its owned bytes");
        ctx.fb.ins(
            &format!("(call $__rt_concat (local.get {part_ptr}) (local.get {part_len}) (i32.const {tail_ptr}) (i64.const {tail_len}))"),
            "then the closing quote and the rest of php's sentence",
        );
    } else {
        let (tail_ptr, tail_len) = ctx.default_str_literal(&suffix)?;
        ctx.fb.ins(
            &format!("(call $__rt_fmt_int (local.get {needle_int}) (i32.const 0) (i64.const 0) (i32.const 0) (i32.const 0))"),
            "an int backing value is rendered as php prints it",
        );
        ctx.fb.ins(&format!("local.set {part_len}"), "the rendered digits");
        ctx.fb.ins(&format!("local.set {part_ptr}"), "their owned bytes");
        ctx.fb.ins(
            &format!("(call $__rt_concat (local.get {part_ptr}) (local.get {part_len}) (i32.const {tail_ptr}) (i64.const {tail_len}))"),
            "then the rest of php's sentence",
        );
    }
    // The concat copied the part out; its block has no other owner.
    let message_len = ctx.fresh_temp(ValType::I64);
    let message_ptr = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.set {message_len}"), "the message length");
    ctx.fb.ins(&format!("local.set {message_ptr}"), "the message bytes");
    ctx.fb.ins(
        &format!("(call $__rt_heap_free_safe (local.get {part_ptr}))"),
        "the intermediate the concat copied out of",
    );
    ctx.fb.ins(&format!("local.get {message_ptr}"), "the message bytes");
    ctx.fb.ins(&format!("local.get {message_len}"), "the message length");
    // The same diagnostic a user `throw` claims, and for the same reason: it prints the class
    // and message off the OBJECT rather than from a laid-out constant, which is the only kind
    // that can carry a message composed at run time. So this raise owes no new failure code.
    super::objects::emit_runtime_error_throw_of_computed_message(
        ctx,
        "ValueError",
        super::function::UNCAUGHT_EXCEPTION_FAILURE_CODE,
    )?;
    ctx.fb.ins("))", "close the no-match raise");

    ctx.fb
        .ins(&format!("local.get {}", found), "the matching case singleton");
    super::inst::store_result(ctx, inst)
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
    let receiver_php = ctx.value_php_type(receiver)?;
    let candidates = mixed_method_candidates(
        ctx.module,
        method_key,
        &receiver_php,
        inst.operands.len().saturating_sub(1),
    );
    if candidates.is_empty() {
        return Err(WasmError::Unsupported(format!(
            "mixed method {}: no candidate class (P6f)",
            method_name
        )));
    }
    let arity_failures = mixed_method_arity_failures(
        ctx.module,
        method_key,
        &receiver_php,
        inst.operands.len().saturating_sub(1),
    );

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
    emit_arity_failure_arms(
        ctx,
        &arity_failures,
        &cid,
        inst.operands.len().saturating_sub(1),
        method_ptr,
        method_len,
    );
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

/// Emits the ladder arms for classes php-src would not enter for lack of arguments.
///
/// These sit between the callable arms and the undefined-method fallthrough, so a class the arity
/// filter removed still gets a decision of its own. Without them the fallthrough would answer
/// `Call to undefined method C::m()` for a method that plainly exists — a different error class
/// from php-src's `ArgumentCountError`, on a program php-src also ends fatally.
///
/// Each arm is non-returning, so nothing needs to reach the merge label and the ladder's result
/// slot stays untouched on this path.
fn emit_arity_failure_arms(
    ctx: &mut FnCtx,
    failures: &[MixedMethodArityFailure],
    cid_local: &str,
    passed: usize,
    method_ptr: u32,
    method_len: u32,
) {
    for failure in failures {
        ctx.fb
            .ins(&format!("local.get {}", cid_local), "receiver class id");
        ctx.fb.ins(
            &format!("i64.const {}", failure.class_id as i64),
            &format!("{} cannot be entered with this arity", failure.class_name),
        );
        ctx.fb.ins("i64.eq", "matches this class?");
        ctx.fb.ins("if", "too-few-arguments arm");
        ctx.fb
            .ins(&format!("local.get {}", cid_local), "receiver class id");
        ctx.fb
            .ins(&format!("i32.const {}", method_ptr), "method-name pointer");
        ctx.fb.ins(
            &format!("i32.const {}", method_len),
            "method-name byte length",
        );
        ctx.fb
            .ins(&format!("i64.const {}", passed), "arguments passed");
        ctx.fb.ins(
            &format!("i64.const {}", failure.required),
            "arguments php-src requires",
        );
        ctx.fb.ins(
            &format!("i32.const {}", i32::from(failure.exact)),
            "1 = php-src words the count `exactly`",
        );
        ctx.fb.ins(
            "call $__rt_fail_too_few_arguments",
            "raise PHP ArgumentCountError",
        );
        ctx.fb.ins(
            "unreachable",
            "elephc-trap:post-noreturn:too-few-arguments fatal helper does not return",
        );
        ctx.fb.ins("end", "end too-few-arguments arm");
    }
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
        let declared_name =
            declared_method_name(&ctx.module.class_methods, impl_class, method_key, method_name);
        let callee_symbol = method_symbol(&format!("{}::{}", impl_class, declared_name));

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

        // Receiver (the unboxed object pointer) as `this`, then user args in order. The args go
        // through the same conversion the direct path uses: this arm calls ONE body, so its
        // parameters are exactly as binding here as they are there — a raw load handed a
        // narrower array straight to a body expecting `array<mixed>`.
        let callee_params = callee_fn.params.clone();
        ctx.fb.ins(&format!("local.get {}", obj_local), "receiver object pointer (this)");
        let mut minted: Vec<String> = Vec::new();
        for (index, &arg) in inst.operands.iter().skip(1).enumerate() {
            if let Some(cell) = push_call_argument(ctx, arg, callee_params.get(index + 1))? {
                minted.push(cell);
            }
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (closed-world direct)", class_name, method_name),
        );
        release_boxed_arguments(ctx, &minted);
        (callee_ret_ir, callee_ret_php)
    };

    let result_is_boxed = matches!(inst.result_php_type, PhpType::Mixed | PhpType::Union(_));
    let callee_ret_is_mixed = matches!(callee_ret_ir, IrType::Heap(IrHeapKind::Mixed));
    if result_is_boxed && !callee_ret_is_mixed {
        box_call_result_into_mixed(ctx, callee_ret_ir, &callee_ret_php, inst.result)?;
    } else if let Some(r) = inst.result {
        // A `void` body pushes nothing, but PHP still gives its CALL EXPRESSION the value null —
        // and when every candidate agrees on `void`, the checker types that expression `I64
        // php=null` rather than boxing it. The direct and interface paths already supply the null
        // the callee did not push; without it here the arm stored from an empty stack and the
        // module failed WebAssembly validation outright.
        if callee_ret_ir == IrType::Void {
            ctx.fb.ins(
                "i64.const 9223372036854775806",
                "null sentinel: a void method call evaluates to null",
            );
        }
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
            let candidates = mixed_method_candidates(
                ctx.module,
                &method_key,
                &receiver_ty,
                inst.operands.len().saturating_sub(1),
            );
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
            emit_arity_failure_arms(
                ctx,
                &mixed_method_arity_failures(
                    ctx.module,
                    &method_key,
                    &receiver_ty,
                    inst.operands.len().saturating_sub(1),
                ),
                &cid,
                inst.operands.len().saturating_sub(1),
                method_ptr,
                method_len,
            );
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

    // `cases()` and `tryFrom()` are SYNTHESIZED by PHP for every enum and have no body to call,
    // so they are open-coded here against the case singletons. The capability audit already
    // proved the shapes; `from()` is not among them and falls through to fail as a missing body,
    // because it must raise php-src's `ValueError` on no match.
    if ctx.module.enum_infos.contains_key(&receiver_class)
        && matches!(method_key.as_str(), "cases" | "tryfrom" | "from")
    {
        return lower_enum_static_intrinsic(ctx, inst, &receiver_class, &method_key);
    }

    let true_static = ci.static_methods.contains_key(&method_key);
    let lexical_instance = !true_static
        && (receiver_label == "self" || receiver_label == "parent")
        && is_instance_fn
        && ci.methods.contains_key(&method_key);

    // A `void` body pushes nothing, but PHP still gives its CALL EXPRESSION the value null
    // — the same rule the instance path applies. The EIR materializes an `I64 php=null`
    // result whenever it is used, so the null is supplied after the call.
    let body_returns_void;

    if true_static {
        let impl_class = ci
            .static_method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| receiver_class.clone());
        let declared_name =
            declared_method_name(&ctx.module.class_methods, &impl_class, &method_key, method_name);
        body_returns_void = body_is_void(ctx, &impl_class, &method_key);
        let callee_symbol = method_symbol(&format!("{}::{}", impl_class, declared_name));
        ctx.fb.ins(
            &format!("i64.const {}", ci.class_id as i64),
            &format!("{}::{} called_class_id", receiver_class, method_name),
        );
        let params = method_body_params(ctx, &format!("{}::{}", impl_class, declared_name));
        let mut minted = Vec::new();
        for (index, &arg) in inst.operands.iter().enumerate() {
            if let Some(cell) = push_call_argument(ctx, arg, params.get(index + 1))? {
                minted.push(cell);
            }
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (static)", receiver_class, method_name),
        );
        release_boxed_arguments(ctx, &minted);
    } else if lexical_instance {
        let impl_class = ci
            .method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| receiver_class.clone());
        let declared_name =
            declared_method_name(&ctx.module.class_methods, &impl_class, &method_key, method_name);
        body_returns_void = body_is_void(ctx, &impl_class, &method_key);
        let callee_symbol = method_symbol(&format!("{}::{}", impl_class, declared_name));
        // Forward the current `this` (slot 0) as the receiver of the instance method.
        ctx.emit_load_slot(LocalSlotId::from_raw(0))?;
        let params = method_body_params(ctx, &format!("{}::{}", impl_class, declared_name));
        let mut minted = Vec::new();
        for (index, &arg) in inst.operands.iter().enumerate() {
            if let Some(cell) = push_call_argument(ctx, arg, params.get(index + 1))? {
                minted.push(cell);
            }
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (lexical instance via {}::)", impl_class, declared_name, receiver_label),
        );
        release_boxed_arguments(ctx, &minted);
    } else {
        return Err(WasmError::Unsupported(format!(
            "unresolvable static call {} (static method not found; lexical instance fallback \
             not applicable)",
            target
        )));
    }

    let return_arity = if body_returns_void {
        0
    } else {
        WasmRepr::val_types(inst.result_type).len()
    };
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

    emit_interface_dispatch_stubs(wm, module)
}

/// Emits one dispatch stub per (interface, method key), over the interface's implementors.
///
/// Same ladder, different arm set: a class subtree is what a virtual call can land on, and
/// the set of concrete implementors is what an interface-typed call can land on. Interfaces
/// and classes share one PHP name namespace, so the two stub families cannot collide.
///
/// An interface whose implementors disagree on the ABI, or one with an implementor whose
/// body is missing, is skipped wholesale rather than emitted half-right — the capability
/// gate refuses those calls, so no `call` will reference the stub that was not emitted.
fn emit_interface_dispatch_stubs(wm: &mut WatModule, module: &Module) -> Result<()> {
    let mut interfaces: Vec<_> = module.interface_infos.iter().collect();
    interfaces.sort_by(|(left, left_info), (right, right_info)| {
        left_info
            .interface_id
            .cmp(&right_info.interface_id)
            .then_with(|| left.cmp(right))
    });
    for (interface_name, interface_info) in interfaces {
        let mut method_keys: Vec<_> = interface_info.methods.keys().collect();
        method_keys.sort();
        for method_key in method_keys {
            let method_key = method_key.as_str();
            let Ok(candidates) = super::capability::interface_dispatch_candidates(
                module,
                interface_name,
                method_key,
            ) else {
                continue;
            };

            // A `Throwable` accessor has no body for ANY implementor, so there is nothing to
            // forward to; the stub reads the receiver's slot instead. Checked before the body
            // loop because that loop's first act is to refuse the missing body.
            if let Some(intrinsic) = objects::interface_throwable_intrinsic(
                module,
                method_key,
                &candidates,
            ) {
                let stub_symbol = method_dispatch_symbol(interface_name, method_key);
                match objects::throwable_intrinsic_dispatch_stub(
                    module,
                    &stub_symbol,
                    intrinsic,
                    &candidates,
                ) {
                    // A stub the audit refuses is one no `call` will reference, so a failure
                    // here is skipped rather than propagated, exactly as a mismatched ABI is.
                    Ok(wat) => wm.add_raw_func(&wat),
                    Err(_) => {}
                }
                continue;
            }

            let mut arms: Vec<(u64, String)> = Vec::new();
            let mut sig_fn: Option<&Function> = None;
            let mut unusable = false;
            for (class_name, implementation) in &candidates {
                let Some(class_ci) = module.class_infos.get(class_name) else {
                    unusable = true;
                    break;
                };
                let Some(method) =
                    find_method_function(&module.class_methods, implementation, method_key)
                else {
                    unusable = true;
                    break;
                };
                if let Some(signature) = sig_fn {
                    if signature.return_type != method.return_type
                        || signature.params.len() != method.params.len()
                        || signature
                            .params
                            .iter()
                            .zip(&method.params)
                            .any(|(left, right)| left.ir_type != right.ir_type)
                    {
                        unusable = true;
                        break;
                    }
                }
                arms.push((class_ci.class_id, function_symbol(method)));
                if sig_fn.is_none() {
                    sig_fn = Some(method);
                }
            }
            if unusable {
                continue;
            }
            let Some(sig_f) = sig_fn else {
                continue;
            };
            arms.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

            // PHP puts classes and interfaces in ONE name namespace, so an interface stub can
            // never collide with a class stub, and each (interface, method key) pair is
            // visited once. `render_checked` rejects a duplicate identifier regardless.
            let stub_symbol = method_dispatch_symbol(interface_name, method_key);
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
        // A class this module cannot construct is not a possible receiver, so it gets no arm —
        // the emitter has to agree with the audit here or the stub would call a body the audit
        // never checked, or omit an arm the audit expected.
        if !ci.is_abstract
            && ci.vtable_slots.contains_key(method_key)
            && super::capability::class_is_constructible(module, &name)
        {
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

/// Reports whether the body implementing `method_key` on exactly `impl_class` returns `void`.
fn body_is_void(ctx: &FnCtx, impl_class: &str, method_key: &str) -> bool {
    find_method_function(&ctx.module.class_methods, impl_class, method_key)
        .map(|function| function.return_type == IrType::Void)
        .unwrap_or(false)
}

/// Returns the method name exactly as `impl_class` declares it, falling back to `spelled`.
///
/// PHP method names are case-insensitive, so a call site may spell `A::F` for a method declared
/// `f`. The body's symbol and its parameter list are keyed by the DECLARED spelling, so every
/// call lowering must resolve through this rather than reuse the call-site text.
fn declared_method_name(
    class_methods: &[Function],
    impl_class: &str,
    method_key: &str,
    spelled: &str,
) -> String {
    find_method_function(class_methods, impl_class, method_key)
        .and_then(|function| function.name.rsplit_once("::"))
        .map(|(_, declared)| declared.to_string())
        .unwrap_or_else(|| spelled.to_string())
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
