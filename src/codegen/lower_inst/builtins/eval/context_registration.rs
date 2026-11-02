//! Purpose:
//! Registers native symbols and configures eval constants, profiles and providers.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Context initialization requests these registrations only when metadata is needed.
//! - Core and user global constants are seeded through separate bridge registries so
//!   PHP's `Core` / `user` categories stay exact inside eval.

use super::*;

/// Registers the whole native catalog into the eval context passed as the first argument.
///
/// This is the body of the module-level catalog helper. The handle is parked in
/// a module-level symbol because this frame is the fixed 32-byte one the
/// shared-helper prologue builds: a frame-local slot would land in the caller's
/// stack.
pub(in crate::codegen::lower_inst::builtins) fn register_eval_inventory(
    ctx: &mut FunctionContext<'_>,
) -> Result<()> {
    let source = EvalContextSource::Symbol(
        crate::codegen::eval_inventory_helper::EVAL_INVENTORY_CONTEXT_SYMBOL,
    );
    // The handle arrives in the FIRST ARGUMENT register, which is not the result
    // register on every target: x86_64 passes it in `rdi` and returns in `rax`.
    // Reading the result register there would store whatever the caller happened
    // to leave in `rax` — `1` on the deferred-catalog path — and the readiness
    // probe would then dereference address 1.
    let handle_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        handle_reg,
        crate::codegen::eval_inventory_helper::EVAL_INVENTORY_CONTEXT_SYMBOL,
        0,
    );
    let registered = ctx.next_label("eval_native_catalog_registered");
    load_eval_context_source_to_arg(ctx, source, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_context_native_catalog_ready");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &registered);
    // -- publish only after the complete generated registration transaction --
    register_eval_declared_symbols(ctx, source);
    register_eval_native_global_constants(ctx, source)?;
    register_eval_native_user_constants(ctx, source);
    register_eval_native_functions(ctx, source)?;
    register_eval_native_method_signatures(ctx, source);
    load_eval_context_source_to_arg(ctx, source, 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_context_publish_native_catalog");
    abi::emit_call_label(ctx.emitter, &symbol);
    ctx.emitter.label(&registered);
    Ok(())
}

/// Loads this function's eval context, or a null handle for request-global fallback lookup.
///
/// A statically typed function can be compiled without an eval operation of its own while a
/// different frame owns the request's registered SPL callbacks.  The dynamic construction ABI
/// accepts a null handle for that case and resolves the retained callback-owner contexts instead
/// of forcing every AOT function to reserve a persistent eval frame slot.
pub(super) fn load_eval_context_or_null(ctx: &mut FunctionContext<'_>) -> Result<()> {
    if ctx
        .function
        .locals
        .iter()
        .any(|local| local.kind == LocalKind::EvalContext)
    {
        return ensure_eval_context(ctx);
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_store_to_sp(ctx.emitter, result_reg, EVAL_CONTEXT_HANDLE_OFFSET);
    Ok(())
}

/// Label of the module routine copying the live native globals into an eval scope.
pub(in crate::codegen) const EVAL_SYNC_NATIVE_TO_EVAL_LABEL: &str = "_eir_shared_eval_sync_native_to_eval";

/// Label of the module routine copying an eval scope's globals back to the native ones.
pub(in crate::codegen) const EVAL_SYNC_EVAL_TO_NATIVE_LABEL: &str = "_eir_shared_eval_sync_eval_to_native";

/// Registers the paired global transfer hooks on the context held at `context_offset`.
fn emit_eval_global_sync_hooks_registration(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), EVAL_SYNC_NATIVE_TO_EVAL_LABEL);
    abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), EVAL_SYNC_EVAL_TO_NATIVE_LABEL);
    let register = ctx.emitter.target.extern_symbol("__elephc_eval_context_set_global_sync_hooks");
    abi::emit_call_label(ctx.emitter, &register);
}

/// Registers the paired global transfer hooks on the context a body has just created.
///
/// Main's native catalog is shared between contexts and carries no hooks, so each fresh context
/// takes the module's transfer routines (emitted ahead of the bodies by
/// `emit_shared_eval_registration_helper`) here. `context_offset` is the frame slot of the handle.
pub(super) fn register_eval_global_sync_hooks_on_new_context(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
) {
    emit_eval_global_sync_hooks_registration(ctx, context_offset);
}

/// Label of the module-wide eval registration helper when it is emitted ahead of the bodies.
pub(in crate::codegen) const EVAL_REGISTRATION_LABEL: &str = "_eir_shared_eval_register_module";

/// Returns whether some body of `module` can build an eval context, which is what calls the
/// registration helper.
///
/// A context is built only through the body's own `EvalContext` local (`eval_context_slot`), so
/// a module with no such local never needs the helper; and every call it contains is to the eval
/// bridge, so without the bridge there is nothing to emit it against.
fn module_needs_eval_registration(module: &Module) -> bool {
    module.required_runtime_features.eval_bridge
        && module
            .functions
            .iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
            .chain(module.fiber_wrappers.iter())
            .chain(module.callback_wrappers.iter())
            .chain(module.extern_callback_trampolines.iter())
            .chain(module.runtime_callable_invokers.iter())
            .any(|function| {
                function
                    .locals
                    .iter()
                    .any(|local| local.kind == LocalKind::EvalContext)
            })
}

/// Emits the eval registration helper ahead of every body, when the module can need it.
///
/// Its body depends on the module alone -- declared symbols, native functions, method
/// signatures, the global sync hooks -- yet it used to be emitted inside the first body that
/// built an eval context, under a label minted by that body. Any other body then had to READ
/// that label from the shared state, which a parallel codegen worker may not do: on the Symfony
/// module it was the first shared access of 1 164 of the 1 541 bodies the workers gave back.
/// Emitted here, its label is a constant and every body can call it from any thread.
pub(in crate::codegen) fn emit_shared_eval_registration_helper(
    module: &Module,
    emitter: &mut crate::codegen::emit::Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    shared: &mut crate::codegen::shared_state::SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module.required_runtime_features.eval_bridge {
        return Ok(());
    }
    let has_helper = module_needs_eval_registration(module);
    emit_eval_register_module_entry(module, emitter, has_helper);
    if !has_helper {
        return Ok(());
    }
    let mut function = crate::ir::Function::new(
        EVAL_REGISTRATION_LABEL.to_string(),
        crate::ir::IrType::Void,
        PhpType::Void,
    );
    function.flags.is_synthetic = true;
    let entry = crate::ir::BlockId::from_raw(0);
    function
        .blocks
        .push(crate::ir::BasicBlock::new(entry, "entry".to_string(), Vec::new()));
    function.entry = entry;
    let layout = crate::codegen::frame::layout_for_function(
        &function,
        emitter.target,
        regalloc_linear,
        false,
        false,
    );
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );
    ctx.emitter.blank();
    ctx.emitter
        .comment(&format!("--- shared eval registration: {EVAL_REGISTRATION_LABEL} ---"));
    emit_eval_registration_helper(&mut ctx, EVAL_REGISTRATION_LABEL, true)?;
    drop(ctx);
    shared.preset_eval_registration_helper(EVAL_REGISTRATION_LABEL.to_string());
    Ok(())
}

/// C symbol through which the eval runtime runs the module registration on a context it made.
pub(in crate::codegen) const EVAL_REGISTER_MODULE_ENTRY: &str =
    "__elephc_eval_register_module_metadata";

/// Emits `__elephc_eval_register_module_metadata(ctx)`, the C-ABI door to the registration helper.
///
/// The AOT metadata snapshot every context without registration of its own syncs from — the
/// fallback contexts a null-handle bridge call makes, the shared null-handle context — is
/// published by the registration helper, and that helper used to run only when a compiled body
/// built its OWN eval context. A compiled `new $class(...)` that reaches the bridge before any
/// `eval()` passes a null handle, so its fallback context synced nothing: no constructor
/// signature, no hidden-collector binding, and the constructor bridge refused the one-argument
/// call on arity. Symfony's router builds `CompiledUrlMatcher` exactly that way, and every route
/// answered "No route found". The runtime now calls this entry once when it finds no snapshot.
///
/// The magician links against this symbol unconditionally, so it is defined for every module that
/// links the bridge; one with no body that builds a context has nothing to register and returns.
/// Rust is the caller, so the entry keeps the callee-saved registers the generated helper uses.
fn emit_eval_register_module_entry(
    module: &Module,
    emitter: &mut crate::codegen::emit::Emitter,
    has_helper: bool,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: module registration entry ---");
    let symbol = module.target.extern_symbol(EVAL_REGISTER_MODULE_ENTRY);
    emitter.label_global(&symbol);
    if !has_helper {
        emitter.instruction("ret");                                             // nothing in this module builds an eval context
        return;
    }
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-160]!");                  // open a frame for the C-ABI registration entry
            emitter.instruction("mov x29, sp");                                 // anchor the entry frame
            emitter.instruction("stp x19, x20, [sp, #16]");                     // keep the Rust caller's callee-saved x19/x20
            emitter.instruction("stp x21, x22, [sp, #32]");                     // keep the Rust caller's callee-saved x21/x22
            emitter.instruction("stp x23, x24, [sp, #48]");                     // keep the Rust caller's callee-saved x23/x24
            emitter.instruction("stp x25, x26, [sp, #64]");                     // keep the Rust caller's callee-saved x25/x26
            emitter.instruction("stp x27, x28, [sp, #80]");                     // keep the Rust caller's callee-saved x27/x28
            emitter.instruction("stp d8, d9, [sp, #96]");                       // keep the Rust caller's callee-saved d8/d9
            emitter.instruction("stp d10, d11, [sp, #112]");                    // keep the Rust caller's callee-saved d10/d11
            emitter.instruction("stp d12, d13, [sp, #128]");                    // keep the Rust caller's callee-saved d12/d13
            emitter.instruction("stp d14, d15, [sp, #144]");                    // keep the Rust caller's callee-saved d14/d15
            abi::emit_call_label(emitter, EVAL_REGISTRATION_LABEL);
            emitter.instruction("ldp d14, d15, [sp, #144]");                    // restore the caller's d14/d15
            emitter.instruction("ldp d12, d13, [sp, #128]");                    // restore the caller's d12/d13
            emitter.instruction("ldp d10, d11, [sp, #112]");                    // restore the caller's d10/d11
            emitter.instruction("ldp d8, d9, [sp, #96]");                       // restore the caller's d8/d9
            emitter.instruction("ldp x27, x28, [sp, #80]");                     // restore the caller's x27/x28
            emitter.instruction("ldp x25, x26, [sp, #64]");                     // restore the caller's x25/x26
            emitter.instruction("ldp x23, x24, [sp, #48]");                     // restore the caller's x23/x24
            emitter.instruction("ldp x21, x22, [sp, #32]");                     // restore the caller's x21/x22
            emitter.instruction("ldp x19, x20, [sp, #16]");                     // restore the caller's x19/x20
            emitter.instruction("ldp x29, x30, [sp], #160");                    // close the entry frame
            emitter.instruction("ret");                                         // registration published the module snapshot
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // open a frame for the C-ABI registration entry
            emitter.instruction("mov rbp, rsp");                                // anchor the entry frame
            emitter.instruction("push rbx");                                    // keep the Rust caller's callee-saved rbx
            emitter.instruction("push r12");                                    // keep the Rust caller's callee-saved r12
            emitter.instruction("push r13");                                    // keep the Rust caller's callee-saved r13
            emitter.instruction("push r14");                                    // keep the Rust caller's callee-saved r14
            emitter.instruction("push r15");                                    // keep the Rust caller's callee-saved r15
            emitter.instruction("sub rsp, 8");                                  // realign the stack to 16 bytes for the call
            abi::emit_call_label(emitter, EVAL_REGISTRATION_LABEL);
            emitter.instruction("add rsp, 8");                                  // drop the alignment pad
            emitter.instruction("pop r15");                                     // restore the caller's r15
            emitter.instruction("pop r14");                                     // restore the caller's r14
            emitter.instruction("pop r13");                                     // restore the caller's r13
            emitter.instruction("pop r12");                                     // restore the caller's r12
            emitter.instruction("pop rbx");                                     // restore the caller's rbx
            emitter.instruction("pop rbp");                                     // close the entry frame
            emitter.instruction("ret");                                         // registration published the module snapshot
        }
    }
}

/// Emits the single module-wide body that finishes the registration of one eval context.
///
/// The helper is called once for the context the module-registration entry makes (and for the
/// `--web` priming context). A dedicated ABI frame
/// preserves both the incoming context handle and the caller's return address. `top_level`
/// says it is its own function rather than a stretch inside another body's text, which is what
/// decides between a plain label and an `.alt_entry`.
fn emit_eval_registration_helper(
    ctx: &mut FunctionContext<'_>,
    label: &str,
    top_level: bool,
) -> Result<()> {
    if top_level {
        ctx.emitter.label(label);
    } else {
        ctx.emitter.label_shared(label);
    }
    abi::emit_frame_prologue(ctx.emitter, EVAL_CONTEXT_HELPER_FRAME_SIZE);
    let context_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::store_at_offset(ctx.emitter, context_arg, EVAL_CONTEXT_HELPER_LOCAL_OFFSET);
    // The catalog itself is main's guarded registration transaction, shared by every context;
    // this helper adds what only the module registration owns on top of it: the paired global
    // transfer hooks and the process-global AOT snapshot null-handle bridge calls sync from.
    load_eval_context_local_to_arg(ctx, EVAL_CONTEXT_HELPER_LOCAL_OFFSET, 0);
    abi::emit_call_label(ctx.emitter, crate::codegen::eval_inventory_helper::EVAL_INVENTORY_LABEL);
    emit_eval_global_sync_hooks_registration(ctx, EVAL_CONTEXT_HELPER_LOCAL_OFFSET);
    publish_eval_aot_metadata(ctx, EVAL_CONTEXT_HELPER_LOCAL_OFFSET);
    abi::emit_frame_restore(ctx.emitter, EVAL_CONTEXT_HELPER_FRAME_SIZE);
    abi::emit_return(ctx.emitter);
    emit_eval_global_sync_helper(ctx, EVAL_SYNC_NATIVE_TO_EVAL_LABEL, true)?;
    emit_eval_global_sync_helper(ctx, EVAL_SYNC_EVAL_TO_NATIVE_LABEL, false)?;
    Ok(())
}

/// Emits one module transfer routine with an explicit live scope and its own
/// scratch frame. No caller-local eval handle is required by either direction.
fn emit_eval_global_sync_helper(
    ctx: &mut FunctionContext<'_>,
    label: &str,
    native_to_eval: bool,
) -> Result<()> {
    ctx.emitter.label_shared(label);
    let frame_size = EVAL_STACK_BYTES + 16;
    abi::emit_frame_prologue(ctx.emitter, frame_size);
    let scope_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_store_to_sp(ctx.emitter, scope_arg, EVAL_GLOBAL_SCOPE_HANDLE_OFFSET);
    let globals = eval_sync_globals(ctx);
    if native_to_eval {
        flush_eval_global_scope(ctx, &globals)?;
        // The flush is a borrowed snapshot, not an interpreted write: a native call made from
        // the fragment can replace and release the native cell, so an untouched entry must not
        // be replayed at the eval-to-native boundary.
        emit_clear_eval_global_scope_dirty(ctx);
    } else {
        reload_eval_global_scope(ctx, &globals, None)?;
    }
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
    Ok(())
}

/// Publishes the fully registered AOT metadata for bridge calls made without a generated context.
fn publish_eval_aot_metadata(ctx: &mut FunctionContext<'_>, context_offset: usize) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_context_publish_aot_metadata");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Registers the AOT Core constant inventory with a newly allocated eval context.
pub(super) fn register_eval_native_global_constants(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
) -> Result<()> {
    let user_names = ctx
        .module
        .user_defined_constants
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entries = ctx
        .module
        .global_constants
        .iter()
        .filter(|(name, _)| !user_names.contains(*name))
        .map(|(name, (value, ty))| (name.clone(), value.clone(), ty.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, value, ty) in entries {
        register_eval_native_global_constant(ctx, context_source, &name, &value, &ty)?;
    }
    Ok(())
}

/// Emits one scalar AOT constant registration call with its exact PHP runtime type.
fn register_eval_native_global_constant(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
    name: &str,
    value: &ExprKind,
    ty: &PhpType,
) -> Result<()> {
    if name == "PHP_BINARY" {
        register_eval_executable_constant(ctx, context_source, name, "__elephc_eval_register_native_global_constant");
        return Ok(());
    }
    let (kind, word, string_value) = eval_native_global_constant_abi_value(value, ty)?;
    register_eval_native_scalar_constant(
        ctx,
        context_source,
        name,
        kind,
        word,
        string_value.as_deref(),
        "__elephc_eval_register_native_global_constant",
    );
    Ok(())
}

/// Registers the running executable path under either a Core name or a user alias.
fn register_eval_executable_constant(
    ctx: &mut FunctionContext<'_>, context_source: EvalContextSource, name: &str, symbol: &str,
) {
    abi::emit_call_label(ctx.emitter, "__rt_php_binary");
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    load_eval_context_source_to_arg(ctx, context_source, 0);
    emit_eval_constant_name_args(ctx, name);
    let target = ctx.emitter.target;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), NATIVE_GLOBAL_CONSTANT_STRING);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 5), 8);
    abi::emit_call_label(ctx.emitter, &target.extern_symbol(symbol));
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
}

/// Registers the AOT user-declared constant inventory with a newly allocated eval context.
///
/// User constants stay in their own bridge registry so eval reports them under PHP's `user`
/// category instead of `Core`. A value AOT itself cannot materialize is skipped rather than
/// failing the compile; see `eval_native_user_constant_value`.
pub(super) fn register_eval_native_user_constants(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
) {
    let user_names = ctx
        .module
        .user_defined_constants
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entries = ctx
        .module
        .global_constants
        .iter()
        .filter(|(name, _)| user_names.contains(*name))
        .map(|(name, (value, ty))| (name.clone(), value.clone(), ty.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, value, ty) in entries {
        if crate::types::predefined_constants::is_executable_path_alias(&value, &ctx.module.global_constants) {
            register_eval_executable_constant(ctx, context_source, &name, "__elephc_eval_register_native_user_constant");
            continue;
        }
        let normalized = if matches!(value, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
            | ExprKind::ArrayLiteralMixed(_)) {
            let context = crate::codegen::const_default_values::ConstDefaultContext {
                module: ctx.module, current_class: None,
            };
            crate::codegen::const_default_values::resolve_const_default(
                &Expr::new(value.clone(), crate::span::Span::synthetic()), &context,
            ).and_then(crate::codegen::const_default_values::resolved_literal_default)
        } else { None };
        let Some(encoded) = eval_native_user_constant_value(normalized.as_ref().unwrap_or(&value), &ty) else {
            continue;
        };
        register_eval_native_user_constant(ctx, context_source, &name, &encoded);
    }
}

/// Emits one user-constant registration call for a scalar or array constant value.
fn register_eval_native_user_constant(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
    name: &str,
    value: &EvalNativeUserConstantValue,
) {
    match value {
        EvalNativeUserConstantValue::Scalar {
            kind,
            word,
            string_value,
        } => register_eval_native_scalar_constant(
            ctx,
            context_source,
            name,
            *kind,
            *word,
            string_value.as_deref(),
            "__elephc_eval_register_native_user_constant",
        ),
        EvalNativeUserConstantValue::Array(elements) => {
            register_eval_native_user_constant_array(ctx, context_source, name, elements)
        }
    }
}

/// Emits one array-valued user-constant registration call with its encoded element spec.
fn register_eval_native_user_constant_array(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
    name: &str,
    elements: &[EvalNativeCallableArrayDefaultElement],
) {
    let spec = encode_eval_native_array_default_elements(elements);
    load_eval_context_source_to_arg(ctx, context_source, 0);
    emit_eval_constant_name_args(ctx, name);
    let (spec_label, spec_len) = ctx.data.add_string(&spec);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        &spec_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        spec_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_user_constant_array");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one scalar constant registration call against the requested bridge symbol.
///
/// The Core and user registries share this kind/payload ABI shape; only the callee differs.
fn register_eval_native_scalar_constant(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
    name: &str,
    kind: i64,
    word: i64,
    string_value: Option<&str>,
    symbol_name: &str,
) {
    load_eval_context_source_to_arg(ctx, context_source, 0);
    emit_eval_constant_name_args(ctx, name);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        kind,
    );
    if let Some(string_value) = string_value {
        let bytes = elephc_builtin_contract::string_literal::literal_bytes(string_value);
        let (value_label, value_len) = ctx.data.add_string(&bytes);
        abi::emit_symbol_address(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 4),
            &value_label,
        );
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 5),
            value_len as i64,
        );
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 4),
            word,
        );
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 5),
            0,
        );
    }
    let symbol = ctx.emitter.target.extern_symbol(symbol_name);
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Materializes one constant name into the shared name pointer/length argument pair.
fn emit_eval_constant_name_args(ctx: &mut FunctionContext<'_>, name: &str) {
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
}

/// Encodes one prescanned scalar constant for the eval registration ABI.
pub(super) fn eval_native_global_constant_abi_value(
    value: &ExprKind,
    ty: &PhpType,
) -> Result<(i64, i64, Option<String>)> {
    match value {
        ExprKind::Null => Ok((NATIVE_GLOBAL_CONSTANT_NULL, 0, None)),
        ExprKind::BoolLiteral(value) => Ok((
            NATIVE_GLOBAL_CONSTANT_BOOL,
            i64::from(*value),
            None,
        )),
        ExprKind::IntLiteral(value) if matches!(ty, PhpType::Resource(_)) => {
            Ok((NATIVE_GLOBAL_CONSTANT_RESOURCE, *value, None))
        }
        ExprKind::IntLiteral(value) => Ok((NATIVE_GLOBAL_CONSTANT_INT, *value, None)),
        ExprKind::FloatLiteral(value) => Ok((
            NATIVE_GLOBAL_CONSTANT_FLOAT,
            value.to_bits() as i64,
            None,
        )),
        ExprKind::StringLiteral(value) => Ok((
            NATIVE_GLOBAL_CONSTANT_STRING,
            0,
            Some(value.clone()),
        )),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => {
                Ok((NATIVE_GLOBAL_CONSTANT_INT, value.wrapping_neg(), None))
            }
            ExprKind::FloatLiteral(value) => Ok((
                NATIVE_GLOBAL_CONSTANT_FLOAT,
                (-value).to_bits() as i64,
                None,
            )),
            other => Err(CodegenIrError::unsupported(format!(
                "eval native global constant expression {:?}",
                other
            ))),
        },
        other => Err(CodegenIrError::unsupported(format!(
            "eval native global constant expression {:?}",
            other
        ))),
    }
}

/// Installs this binary's OPcache configuration in the eval bridge, which governs the
/// runtime script cache for dynamically included files.
///
/// The runtime cannot derive these values: `--ini` is a compile-time flag, so the
/// effective directive set exists only in the compiler. The bridge defaults to a
/// DISABLED cache, so a binary that never reaches this call — and every consumer
/// linking the archive without elephc's codegen — keeps the uncached behaviour.
///
/// CALLED FROM THE PROLOGUE, not from here. This used to run lazily at the first
/// `ensure_eval_context`, which made the cache's configuration depend on whether the
/// program had already executed an `eval()` — so `opcache_compile_file()` answered `false`
/// before the first one and `true` after, and `opcache_get_configuration()['blacklist']`
/// was empty then populated, for the same binary and the same directives. php-src
/// configures OPcache at startup, before a line of user code runs, and
/// `crate::codegen::frame` now does the same.
pub(crate) fn configure_eval_opcache(ctx: &mut FunctionContext<'_>) {
    let version_id = crate::codegen::compile_php_version().version_id();
    let overrides = crate::codegen_support::ini_overrides();
    let config = crate::opcache::runtime_cache::runtime_cache_config(
        version_id,
        crate::codegen_support::compile_is_web_sapi(),
        &overrides,
    );
    let arguments = [
        i64::from(config.enabled),
        i64::from(config.validate_timestamps),
        config.revalidate_freq as i64,
        config.max_file_size as i64,
        config.memory_consumption as i64,
        // THE RUNTIME TIER'S SHARE of the hash: the prime capacity minus the scripts compiled
        // into this binary, which php-src caches before any dynamic include and which hold a
        // slot each. MEASURED with `max_accelerated_files=200` (223 slots) and an absolute
        // entry path: reference admits 222 dynamic scripts beside the entry, 221 beside the
        // entry and one static `require`; elephc admitted 223 and reported more cached scripts
        // than `max_cached_keys`. (A RELATIVE entry path costs reference one more key — an
        // elephc binary has no invocation path, so the resolved form is the model.)
        config
            .max_accelerated_files
            .saturating_sub(crate::codegen_support::opcache_manifest_len() as u64) as i64,
    ];
    for (index, value) in arguments.into_iter().enumerate() {
        let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, index);
        abi::emit_load_int_immediate(ctx.emitter, arg_reg, value);
    }
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_configure_opcache");
    abi::emit_call_label(ctx.emitter, &symbol);
    configure_eval_opcache_file_cache(ctx, &config);
}

/// Installs the accelerator diagnostic channel and triggers php-src's startup validation
/// of `opcache.file_cache`.
///
/// A SECOND call, not four more arguments on the one above, because that one already
/// spends all six integer argument registers x86_64 provides — a seventh would need a
/// stack slot the emitter cannot express. It is emitted immediately after, because the
/// validation reads the enabled flag the first call installs.
///
/// The bridge fatals (and exits 254) on a bad directory exactly as reference PHP's
/// startup does, so this call can terminate the process before the program runs.
fn configure_eval_opcache_file_cache(
    ctx: &mut FunctionContext<'_>,
    config: &crate::opcache::runtime_cache::RuntimeCacheConfig,
) {
    let (file_cache_label, file_cache_len) = ctx.data.add_string(config.file_cache.as_bytes());
    let (error_log_label, error_log_len) = ctx.data.add_string(config.error_log.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &file_cache_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        file_cache_len as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        i64::from(config.file_cache_read_only),
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 3),
        config.log_verbosity_level,
    );
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 4),
        &error_log_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 5),
        error_log_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_configure_opcache_file_cache");
    abi::emit_call_label(ctx.emitter, &symbol);
    load_eval_opcache_blacklist(ctx, config);
    configure_eval_opcache_swapped_directives(ctx, config);
}

/// Loads `opcache.blacklist_filename`, the paths that run but are never cached.
///
/// A THIRD call for the same reason there is a second one: the two above already spend
/// every integer argument register. Emitted AFTER the file-cache call because a directive
/// value matching no file logs through the accelerator channel that call installs.
///
/// Skipped entirely when the directive is unset, which is the overwhelmingly common case —
/// the bridge would return immediately, so the call would be pure code size.
fn load_eval_opcache_blacklist(
    ctx: &mut FunctionContext<'_>,
    config: &crate::opcache::runtime_cache::RuntimeCacheConfig,
) {
    if config.blacklist_filename.is_empty() {
        return;
    }
    let (label, len) = ctx.data.add_string(config.blacklist_filename.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        &label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_load_blacklist");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// `opcache.file_update_protection`, as the bridge's `swap_directive` addresses it.
///
/// The id is a wire contract with `elephc_magician::script_cache::config`; it is matched
/// by number across the C ABI, so it may never be reordered.
const OPCACHE_DIRECTIVE_FILE_UPDATE_PROTECTION: i64 = 2;

/// Whether `opcache.restrict_api` denies this binary's OPcache API calls, as the bridge's
/// `swap_directive` addresses it. A compile-time verdict (see
/// `opcache_prelude::restrict_api_denies`), never an `ini_set()` target — reference makes the
/// directive `PHP_INI_SYSTEM`, and the prelude only maps the three `PHP_INI_ALL` ones to ids.
const OPCACHE_DIRECTIVE_API_RESTRICTED: i64 = 3;

/// Carries the settings that did not fit either configure call's argument budget.
///
/// Both calls above spend all six integer argument registers x86_64 SysV provides, so
/// rather than a third fixed-shape call this reuses the same id/value setter `ini_set()`
/// uses at run time — one symbol serving the initial install and every later change.
fn configure_eval_opcache_swapped_directives(
    ctx: &mut FunctionContext<'_>,
    config: &crate::opcache::runtime_cache::RuntimeCacheConfig,
) {
    emit_opcache_directive_swap(
        ctx,
        OPCACHE_DIRECTIVE_FILE_UPDATE_PROTECTION,
        config.file_update_protection,
    );
    emit_opcache_directive_swap(
        ctx,
        OPCACHE_DIRECTIVE_API_RESTRICTED,
        i64::from(crate::codegen_support::opcache_api_restricted()),
    );
}

/// Emits one compiled-install `__elephc_eval_opcache_swap_directive` call.
///
/// Passes `as_override = 0`: this is the value `--ini` baked, not an `ini_set()`, so it
/// writes the base configuration and leaves any override in place.
fn emit_opcache_directive_swap(ctx: &mut FunctionContext<'_>, id: i64, value: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), id);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        value,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), 0);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_swap_directive");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Registers managed PCRE2 shim callbacks when regex is enabled for this binary.
pub(super) fn register_eval_regex_provider(ctx: &mut FunctionContext<'_>) {
    if ctx.module.required_runtime_features.mbregex {
        abi::emit_call_label(ctx.emitter, "__rt_mbregex_init");
    }
    if !ctx.module.required_runtime_features.regex {
        return;
    }
    for (index, provider_symbol) in [
        "elephc_pcre2_v1_compile",
        "elephc_pcre2_v1_exec",
        "elephc_pcre2_v1_free",
        "elephc_pcre2_v1_name_count",
        "elephc_pcre2_v1_group_name",
        "elephc_pcre2_v1_last_mark",
    ]
    .into_iter()
    .enumerate()
    {
        let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, index);
        let symbol = ctx.emitter.target.extern_symbol(provider_symbol);
        abi::emit_symbol_address(ctx.emitter, arg_reg, &symbol);
    }
    let register = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_regex_provider");
    abi::emit_call_label(ctx.emitter, &register);
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let symbol = ctx.emitter.target.extern_symbol("elephc_pcre2_v1_last_error");
    abi::emit_symbol_address(ctx.emitter, arg_reg, &symbol);
    let register = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_regex_last_error");
    abi::emit_call_label(ctx.emitter, &register);
    // Pattern-failure reporting: eval warns about a rejected pattern in the words, and through
    // the shim state, compiled code uses.
    for (index, shim_symbol) in [
        "elephc_pcre2_v1_record_pattern_error",
        "elephc_pcre2_v1_compile_warning",
    ]
    .into_iter()
    .enumerate()
    {
        let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, index);
        let symbol = ctx.emitter.target.extern_symbol(shim_symbol);
        abi::emit_symbol_address(ctx.emitter, arg_reg, &symbol);
    }
    let register = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_regex_diagnostics");
    abi::emit_call_label(ctx.emitter, &register);
}

/// Writes the physical eval call site's strict profile before every runtime dispatch.
///
/// Writing both true and false prevents a strict eval from leaking its profile into
/// a later LFC eval that reuses the same persistent bridge context.
pub(super) fn mark_eval_strict_php(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    let strict_php = matches!(
        inst.immediate,
        Some(Immediate::ProfiledData {
            strict_php: true,
            ..
        })
    );
    mark_eval_strict_php_value(ctx, strict_php);
}

/// Writes an explicitly preserved source-profile flag into the active eval context.
pub(super) fn mark_eval_strict_php_value(ctx: &mut FunctionContext<'_>, strict_php: bool) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, i64::from(strict_php));
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_set_strict_php");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Writes the compilation's PHP profile before every runtime dispatch.
///
/// Without this, `PHP_VERSION` and its siblings fork at the eval boundary: a binary compiled
/// `--php-version 8.2` would report `8.2.0` natively and `8.5.0` from inside `eval()`. The
/// bridge defaults to the newest profile, so this call is what makes the older ones true.
pub(super) fn mark_eval_php_version(ctx: &mut FunctionContext<'_>) {
    let version_id = i64::from(crate::codegen::compile_php_version().version_id());
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, version_id);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_set_php_version_id");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Writes the compilation's SAPI mode before every runtime dispatch.
///
/// Without this, `PHP_SAPI` forks at the eval boundary: a `--web` binary reports `cli-server`
/// natively and `cli` from inside `eval()`. Interpreted library code branches on exactly that
/// constant — Symfony's DUMPED container picks web-versus-console mode with
/// `in_array(PHP_SAPI, ['cli', 'phpdbg', 'embed'], true)` — so the fork put an HTTP request on
/// the console path and wired a CLI error renderer that cannot write an HTTP response.
pub(super) fn mark_eval_web_sapi(ctx: &mut FunctionContext<'_>) {
    let web = crate::codegen_support::compile_is_web_sapi();
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_int_immediate(ctx.emitter, arg_reg, i64::from(web));
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_set_web_sapi");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Returns the hidden frame slot that owns this function's persistent eval context.
pub(super) fn eval_context_slot(ctx: &FunctionContext<'_>) -> Result<LocalSlotId> {
    ctx.function
        .locals
        .iter()
        .find(|local| local.kind == LocalKind::EvalContext)
        .map(|local| local.id)
        .ok_or_else(|| CodegenIrError::invalid_module("eval call missing eval context local"))
}

/// Registers eligible AOT global functions with a newly allocated eval context.
pub(super) fn register_eval_native_functions(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
) -> Result<()> {
    let registrations = eval_native_function_registrations(ctx);
    for registration in registrations {
        register_eval_native_function(ctx, context_source, &registration)?;
    }
    Ok(())
}

/// Registers eligible AOT method and constructor signatures with a newly allocated eval context.
pub(super) fn register_eval_native_method_signatures(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
) {
    for registration in eval_native_method_registrations(ctx) {
        register_eval_native_method(ctx, context_source, &registration);
    }
    for registration in eval_native_constructor_registrations(ctx) {
        register_eval_native_constructor(ctx, context_source, &registration);
    }
    for registration in eval_native_property_type_registrations(ctx) {
        register_eval_native_property_type(ctx, context_source, &registration);
    }
    for registration in eval_native_abstract_property_registrations(ctx) {
        register_eval_native_abstract_property(ctx, context_source, &registration);
    }
    for registration in eval_native_interface_property_registrations(ctx) {
        register_eval_native_interface_property(ctx, context_source, &registration);
    }
    for registration in eval_native_property_default_registrations(ctx) {
        register_eval_native_property_default(ctx, context_source, &registration);
    }
    super::property_attribute_registration::register_eval_native_class_default_errors(ctx, context_source);
    for registration in eval_native_class_constant_default_registrations(ctx) {
        register_eval_native_class_constant_default(ctx, context_source, &registration);
    }
    for registration in eval_native_member_attribute_registrations(ctx) {
        register_eval_native_member_attribute(ctx, context_source, &registration);
    }
    register_eval_native_class_parents(ctx, context_source);
}

/// Registers generated declared-name metadata with a newly allocated eval context.
pub(super) fn register_eval_declared_symbols(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
) {
    let class_names = ctx.module.declared_class_names.clone();
    let interface_names = ctx.module.declared_interface_names.clone();
    let trait_names = ctx.module.declared_trait_names.clone();
    for name in class_names {
        register_eval_declared_symbol_name(
            ctx,
            context_source,
            "__elephc_eval_register_declared_class_name",
            &name,
        );
    }
    for name in interface_names {
        register_eval_declared_symbol_name(
            ctx,
            context_source,
            "__elephc_eval_register_declared_interface_name",
            &name,
        );
    }
    for name in trait_names {
        register_eval_declared_symbol_name(
            ctx,
            context_source,
            "__elephc_eval_register_declared_trait_name",
            &name,
        );
    }
}

/// Emits one declared-name metadata registration call into the eval context.
pub(super) fn register_eval_declared_symbol_name(
    ctx: &mut FunctionContext<'_>,
    context_source: EvalContextSource,
    symbol_name: &str,
    name: &str,
) {
    load_eval_context_source_to_arg(ctx, context_source, 0);
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &name_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        name_len as i64,
    );
    let symbol = ctx.emitter.target.extern_symbol(symbol_name);
    abi::emit_call_label(ctx.emitter, &symbol);
}

#[cfg(test)]
#[path = "native_catalog_tests.rs"]
mod native_catalog_tests;
