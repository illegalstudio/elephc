//! Purpose:
//! Coordinates user function emission, wrappers, local layout, and return cleanup.
//! Builds function frames from type signatures and statement bodies.
//!
//! Called from:
//! - `crate::codegen::generate()` after top-level metadata is available
//!
//! Key details:
//! - Parameter slots, hidden locals, and cleanup paths must agree with call lowering and ownership tracking.

mod cleanup;
mod callback_wrapper;
mod control_flow;
mod fiber_wrapper;
mod generator;
mod locals;
mod types;

use std::collections::{HashMap, HashSet};

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::stmt;
use crate::names::{function_epilogue_symbol, function_symbol};
use crate::parser::ast::ExprKind;
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType,
};

use self::cleanup::{
    emit_activation_record_pop, emit_activation_record_push, emit_frame_cleanup_callback,
    epilogue_has_side_effects, preserve_return_registers, restore_return_registers,
};
use self::control_flow::{collect_try_slots, mark_control_flow_epilogue_unsafe};
pub use self::locals::collect_local_vars;
pub(crate) use self::types::{codegen_declared_type, codegen_static_type};
pub(crate) use self::callback_wrapper::{
    emit_callback_wrapper, emit_extern_callback_trampoline,
};
pub(crate) use self::fiber_wrapper::emit_fiber_wrapper;
pub use self::types::{infer_contextual_type, infer_local_type_with_ctx};
pub(crate) use self::types::singular_object_class;

/// Handles `yield`-containing bodies by delegating to `generator::emit_generator_function`,
/// otherwise delegates to `emit_function_with_label` with a derived label/epilogue pair.
#[allow(clippy::too_many_arguments)]
pub fn emit_function(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: Option<&HashMap<String, ClassInfo>>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    // A function whose body contains `yield` is a generator. The wrapper
    // allocates a `GeneratorFrame`, stamps it as a Generator object, and
    // returns it; a separate `<f>__resume` symbol holds the state machine
    // that drives the body across `yield` points.
    if crate::types::checker::yield_validation::body_contains_yield(body) {
        generator::emit_generator_function(emitter, data, name, sig, body, classes);
        return;
    }

    let label = function_symbol(name);
    let epilogue_label = function_epilogue_symbol(name);
    emit_function_with_label(
        emitter,
        data,
        &label,
        &epilogue_label,
        name,
        sig,
        body,
        all_functions,
        callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
        function_variant_groups,
        constants,
        all_global_var_names,
        all_static_vars,
        interfaces,
        traits,
        classes,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

/// Handles `yield`-containing closure bodies by delegating to `generator::emit_generator_closure`,
/// otherwise delegates to `emit_function_with_label_and_class` with an empty globals/statics set.
pub fn emit_closure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    sig: &FunctionSig,
    hidden_params: &[(String, PhpType, bool)],
    body: &[crate::parser::ast::Stmt],
    current_class: Option<&str>,
    all_functions: &HashMap<String, FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    if crate::types::checker::yield_validation::body_contains_yield(body) {
        generator::emit_generator_closure(
            emitter,
            data,
            label,
            sig,
            hidden_params,
            body,
            Some(classes),
        );
        return;
    }

    let epilogue_label = format!("{}_epilogue", label);
    let empty_globals = HashSet::new();
    let empty_statics = HashMap::new();
    let empty_callable_param_sigs = HashMap::new();
    let closure_class_name = current_class.unwrap_or("");
    emit_function_with_label_and_class(
        emitter,
        data,
        label,
        &epilogue_label,
        None,
        sig,
        hidden_params,
        body,
        all_functions,
        &empty_callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
        function_variant_groups,
        constants,
        &empty_globals,
        &empty_statics,
        interfaces,
        traits,
        Some((classes, closure_class_name)),
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

/// Delegates to `emit_function_with_label_and_class` with an empty globals/statics set,
/// using the given `label` as both the entry point and epilogue label.
#[allow(clippy::too_many_arguments)]
pub fn emit_method(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    class_name: &str,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    let empty_globals = HashSet::new();
    let empty_statics = HashMap::new();
    emit_function_with_label_and_class(
        emitter,
        data,
        label,
        epilogue_label,
        Some(label),
        sig,
        &[],
        body,
        all_functions,
        callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
        function_variant_groups,
        constants,
        &empty_globals,
        &empty_statics,
        interfaces,
        traits,
        Some((classes, class_name)),
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

/// Wraps `emit_function_with_label_and_class` with `callable_param_scope` set to `Some(scope)`,
/// passing `None` for class context.
#[allow(clippy::too_many_arguments)]
fn emit_function_with_label(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    callable_param_scope: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: Option<&HashMap<String, ClassInfo>>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    let class_ctx = classes.map(|c| (c, "" as &str));
    emit_function_with_label_and_class(
        emitter,
        data,
        label,
        epilogue_label,
        Some(callable_param_scope),
        sig,
        &[],
        body,
        all_functions,
        callable_param_sigs,
        callable_return_sigs,
        callable_array_return_sigs,
        function_variant_groups,
        constants,
        all_global_var_names,
        all_static_vars,
        interfaces,
        traits,
        class_ctx,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

/// Allocates a local variable slot for an incoming call parameter and sets its ownership
/// and cleanup mode based on the parameter name and type.
///
/// For reference parameters (`is_ref` true), allocates an `Int` slot and registers the
/// parameter name in `ctx.ref_params`. For `$this` and `__elephc_fcc_*` hidden params,
/// disables epilogue cleanup. For `Str` types, uses borrowed ownership; otherwise uses
/// local ownership for the given `PhpType`.
fn allocate_incoming_param(ctx: &mut Context, pname: &str, pty: &PhpType, is_ref: bool) {
    if is_ref {
        ctx.ref_params.insert(pname.to_string());
        ctx.alloc_var_with_static_type(pname, PhpType::Int, pty.clone());
        ctx.update_var_type_static_and_ownership(
            pname,
            pty.codegen_repr(),
            pty.clone(),
            HeapOwnership::borrowed_alias_for_type(pty),
        );
    } else if pname == "this" || pname.starts_with("__elephc_fcc_") {
        ctx.alloc_var_with_static_type(pname, pty.codegen_repr(), pty.clone());
        ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(pty));
        ctx.disable_epilogue_cleanup(pname);
    } else {
        ctx.alloc_var_with_static_type(pname, pty.codegen_repr(), pty.clone());
        if matches!(pty.codegen_repr(), PhpType::Str) {
            ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(pty));
        } else {
            ctx.set_var_ownership(pname, HeapOwnership::local_owner_for_type(pty));
        }
    }
}

/// Records declared callable parameters and seeds known callable parameter signatures.
///
/// Looks up callable-typed parameters in `ctx.callable_param_sigs` by scope and
/// populates `ctx.closure_sigs` so captures can resolve signatures while callsites
/// route through descriptor metadata only for source-declared `callable` params.
fn seed_callable_param_sigs(ctx: &mut Context, scope: Option<&str>, sig: &FunctionSig) {
    for (idx, (pname, pty)) in sig.params.iter().enumerate() {
        if pty != &PhpType::Callable && !is_callable_array_type(pty) {
            continue;
        }
        if pty == &PhpType::Callable && sig.declared_params.get(idx).copied().unwrap_or(false) {
            ctx.callable_param_names.insert(pname.clone());
        }
        if let Some(scope) = scope {
            if let Some(callable_sig) = ctx
                .callable_param_sigs
                .get(&(scope.to_string(), pname.clone()))
                .cloned()
            {
                ctx.closure_sigs.insert(pname.clone(), callable_sig);
                if is_callable_array_type(pty) {
                    ctx.runtime_callable_vars.insert(pname.clone());
                }
            }
        }
    }
}

/// Returns true when a parameter is an array whose elements are callable descriptors.
fn is_callable_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Core function/method/closure emitter. Sets up the context, frame layout, incoming
/// parameters, hidden locals, static variables, try slots, and control flow metadata,
/// then emits the statement body and handles the epilogue including deferred closures,
/// fiber wrappers, and callback wrappers.
#[allow(clippy::too_many_arguments)]
fn emit_function_with_label_and_class(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    callable_param_scope: Option<&str>,
    sig: &FunctionSig,
    hidden_params: &[(String, PhpType, bool)],
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    callable_array_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    class_context: Option<(&HashMap<String, ClassInfo>, &str)>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    let mut ctx = Context::new();
    ctx.return_label = Some(epilogue_label.to_string());
    ctx.return_type = sig.return_type.clone();
    ctx.functions = all_functions.clone();
    ctx.callable_param_sigs = callable_param_sigs.clone();
    ctx.callable_return_sigs = callable_return_sigs.clone();
    ctx.callable_array_return_sigs = callable_array_return_sigs.clone();
    ctx.function_variant_groups = function_variant_groups.clone();
    ctx.constants = constants.clone();
    ctx.all_global_var_names = all_global_var_names.clone();
    ctx.all_static_vars = all_static_vars.clone();
    ctx.interfaces = interfaces.clone();
    ctx.traits = traits.clone();
    ctx.enums = enums.clone();
    ctx.packed_classes = packed_classes.clone();
    ctx.extern_functions = extern_functions.clone();
    ctx.extern_classes = extern_classes.clone();
    ctx.extern_globals = extern_globals.clone();
    if let Some((classes, class_name)) = class_context {
        ctx.classes = classes.clone();
        ctx.current_class = Some(class_name.to_string());
    }

    for (i, (pname, pty)) in sig.params.iter().enumerate() {
        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        allocate_incoming_param(&mut ctx, pname, pty, is_ref);
    }
    seed_callable_param_sigs(&mut ctx, callable_param_scope, sig);
    for (pname, pty, is_ref) in hidden_params {
        allocate_incoming_param(&mut ctx, pname, pty, *is_ref);
        if *is_ref && matches!(pty, PhpType::Callable) {
            ctx.closure_sigs.insert(pname.clone(), sig.clone());
            ctx.closure_captures
                .insert(pname.clone(), hidden_params.to_vec());
        }
    }

    collect_local_vars(body, &mut ctx, sig);
    collect_try_slots(body, &mut ctx);
    mark_control_flow_epilogue_unsafe(body, &mut ctx, sig, false);
    let cleanup_label = ctx.next_label("cleanup_frame");
    ctx.activation_frame_base_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.activation_cleanup_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.activation_prev_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_action_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_target_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.nested_concat_offset_offset = Some(ctx.alloc_hidden_slot(8));
    ctx.pending_return_value_offset = Some(ctx.alloc_hidden_slot(16));

    let vars_size = ctx.stack_offset;
    let frame_size = super::align16(vars_size + 16);

    emitter.raw(".align 2");
    emitter.label_global(label);
    super::abi::emit_frame_prologue(emitter, frame_size);

    let mut incoming_args = super::abi::IncomingArgCursor::for_target(emitter.target, 0);
    for (i, (pname, pty)) in sig.params.iter().enumerate() {
        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        let var = ctx
            .variables
            .get(pname)
            .expect("codegen bug: param was just allocated but not found in variables map");
        let offset = var.stack_offset;
        super::abi::emit_store_incoming_param(
            emitter,
            pname,
            pty,
            offset,
            is_ref,
            &mut incoming_args,
        );
    }
    for (pname, pty, is_ref) in hidden_params {
        let var = ctx
            .variables
            .get(pname)
            .expect("codegen bug: hidden param was just allocated but not found in variables map");
        let offset = var.stack_offset;
        super::abi::emit_store_incoming_param(
            emitter,
            pname,
            pty,
            offset,
            *is_ref,
            &mut incoming_args,
        );
    }

    let param_names: HashSet<String> = sig
        .params
        .iter()
        .map(|(n, _)| n.clone())
        .chain(hidden_params.iter().map(|(n, _, _)| n.clone()))
        .collect();
    for (name, var) in &ctx.variables {
        if param_names.contains(name) {
            continue;
        }
        if matches!(&var.ty, PhpType::Str | PhpType::Callable) || var.ty.is_refcounted() {
            super::abi::emit_store_zero_to_local_slot(emitter, var.stack_offset); // zero-init to prevent stale ptr free
        }
    }
    self::cleanup::emit_local_ref_cell_flag_zero_init(emitter, &ctx);
    emit_activation_record_push(emitter, &ctx, &cleanup_label);

    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    emitter.label(epilogue_label);
    if matches!(sig.return_type, PhpType::Never) {
        emit_never_implicit_return_abort(emitter, data);
    }

    let needs_return_preserve = epilogue_has_side_effects(&ctx);
    if needs_return_preserve {
        preserve_return_registers(emitter, &ctx, &sig.return_type);
    }

    let func_name = label.strip_prefix("_fn_").unwrap_or(label);
    let mut static_vars: Vec<_> = ctx.static_vars.iter().collect();
    static_vars.sort();
    for static_var in static_vars {
        let data_label = format!("_static_{}_{}", func_name, static_var);
        let var_info = ctx.variables.get(static_var);
        if let Some(var) = var_info {
            let offset = var.stack_offset;
            let ty = var.ty.clone();
            emitter.comment(&format!("save static ${} back", static_var));
            super::abi::emit_store_local_slot_to_symbol(emitter, &data_label, &ty, offset);
        }
    }

    emit_activation_record_pop(emitter, &ctx);
    self::cleanup::emit_owned_local_epilogue_cleanup(emitter, &ctx, epilogue_label);

    if needs_return_preserve {
        restore_return_registers(emitter, &ctx, &sig.return_type);
    }
    super::abi::emit_frame_restore(emitter, frame_size);
    super::abi::emit_return(emitter);
    emitter.blank();
    emit_frame_cleanup_callback(emitter, &ctx, &cleanup_label);

    let empty_classes = HashMap::new();
    let classes = class_context
        .map(|(classes, _)| classes)
        .unwrap_or(&empty_classes);
    while !ctx.deferred_closures.is_empty()
        || !ctx.deferred_fiber_wrappers.is_empty()
        || !ctx.deferred_callback_wrappers.is_empty()
        || !ctx.deferred_extern_callback_trampolines.is_empty()
        || !ctx.deferred_runtime_callable_invokers.is_empty()
    {
        let closures: Vec<_> = ctx.deferred_closures.drain(..).collect();
        for closure in closures {
            if closure.needed {
                emit_closure(
                    emitter,
                    data,
                    &closure.label,
                    &closure.sig,
                    &closure.hidden_params,
                    &closure.body,
                    closure.current_class.as_deref(),
                    all_functions,
                    &ctx.callable_return_sigs,
                    &ctx.callable_array_return_sigs,
                    &ctx.function_variant_groups,
                    constants,
                    interfaces,
                    &ctx.traits,
                    classes,
                    &ctx.enums,
                    packed_classes,
                    extern_functions,
                    extern_classes,
                    extern_globals,
                );
            } else {
                // The FCC value never escapes a short-circuited call, so the
                // wrapper body is unreachable. Emit a stub that keeps the
                // symbol resolvable (the FCC assignment still loads its
                // address) and returns 0 if reached at runtime — a defensive
                // floor in case the escape analysis ever missed a site.
                emitter.blank();
                emitter.comment(&format!("uninvoked FCC wrapper {} (stubbed)", closure.label));
                emitter.label_global(&closure.label);
                super::abi::emit_load_int_immediate(
                    emitter,
                    super::abi::int_result_reg(emitter),
                    0,
                );
                super::abi::emit_return(emitter);
            }
        }
        let wrappers: Vec<_> = ctx.deferred_fiber_wrappers.drain(..).collect();
        for wrapper in wrappers {
            emit_fiber_wrapper(emitter, &wrapper);
        }
        let callback_wrappers: Vec<_> = ctx.deferred_callback_wrappers.drain(..).collect();
        for wrapper in callback_wrappers {
            emit_callback_wrapper(emitter, &wrapper);
        }
        let extern_trampolines: Vec<_> =
            ctx.deferred_extern_callback_trampolines.drain(..).collect();
        for trampoline in extern_trampolines {
            emit_extern_callback_trampoline(emitter, &trampoline);
        }
        let invokers: Vec<_> = ctx.deferred_runtime_callable_invokers.drain(..).collect();
        for invoker in invokers {
            crate::codegen::runtime_callable_invoker::emit_runtime_callable_invoker(
                emitter,
                data,
                &ctx,
                &invoker,
            );
        }
    }
}

pub(crate) use self::cleanup::{
    emit_local_ref_cell_flag_zero_init, emit_owned_local_epilogue_cleanup,
};

/// Emits an abort sequence for functions with `PhpType::Never` return type that
/// implicitly return. Writes a fatal diagnostic to stderr and exits with code 1
/// using platform-specific syscall conventions.
fn emit_never_implicit_return_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: A never-returning function must not implicitly return\n");

    emitter.comment("never: abort implicit return");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the fatal never diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the fatal never diagnostic byte length to write
            emitter.syscall(4);
            super::abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the fatal never diagnostic to the Linux stderr descriptor
            super::abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the fatal never diagnostic byte length to write
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal never diagnostic before terminating
            super::abi::emit_exit(emitter, 1);
        }
    }
}
