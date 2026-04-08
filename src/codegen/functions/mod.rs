mod cleanup;
mod control_flow;
mod locals;
mod types;

use std::collections::{HashMap, HashSet};

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::stmt;
use crate::names::{function_epilogue_symbol, function_symbol};
use crate::parser::ast::ExprKind;
use crate::types::{
    ClassInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo,
    PhpType,
};

use self::cleanup::{
    emit_activation_record_pop, emit_activation_record_push, emit_frame_cleanup_callback,
    epilogue_has_side_effects, preserve_return_registers, restore_return_registers,
};
use self::control_flow::{collect_try_slots, mark_control_flow_epilogue_unsafe};
pub use self::locals::collect_local_vars;
pub(crate) use self::types::codegen_declared_type;
pub use self::types::{infer_contextual_type, infer_local_type_pub, infer_local_type_with_ctx};

#[allow(clippy::too_many_arguments)]
pub fn emit_function(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: Option<&HashMap<String, ClassInfo>>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    let label = function_symbol(name);
    let epilogue_label = function_epilogue_symbol(name);
    emit_function_with_label(
        emitter,
        data,
        &label,
        &epilogue_label,
        sig,
        body,
        all_functions,
        constants,
        all_global_var_names,
        all_static_vars,
        interfaces,
        classes,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

pub fn emit_closure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    let epilogue_label = format!("{}_epilogue", label);
    let empty_globals = HashSet::new();
    let empty_statics = HashMap::new();
    emit_function_with_label(
        emitter,
        data,
        label,
        &epilogue_label,
        sig,
        body,
        all_functions,
        constants,
        &empty_globals,
        &empty_statics,
        interfaces,
        Some(classes),
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn emit_method(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: &HashMap<String, ClassInfo>,
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
        sig,
        body,
        all_functions,
        constants,
        &empty_globals,
        &empty_statics,
        interfaces,
        Some((classes, class_name)),
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_function_with_label(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    classes: Option<&HashMap<String, ClassInfo>>,
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
        sig,
        body,
        all_functions,
        constants,
        all_global_var_names,
        all_static_vars,
        interfaces,
        class_ctx,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_function_with_label_and_class(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    epilogue_label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    interfaces: &HashMap<String, InterfaceInfo>,
    class_context: Option<(&HashMap<String, ClassInfo>, &str)>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    let mut ctx = Context::new();
    ctx.return_label = Some(epilogue_label.to_string());
    ctx.return_type = sig.return_type.clone();
    ctx.functions = all_functions.clone();
    ctx.constants = constants.clone();
    ctx.all_global_var_names = all_global_var_names.clone();
    ctx.all_static_vars = all_static_vars.clone();
    ctx.interfaces = interfaces.clone();
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
        if is_ref {
            ctx.ref_params.insert(pname.clone());
            ctx.alloc_var(pname, PhpType::Int);
            ctx.update_var_type_and_ownership(
                pname,
                pty.codegen_repr(),
                HeapOwnership::borrowed_alias_for_type(pty),
            );
        } else if pname == "this" {
            ctx.alloc_var(pname, pty.codegen_repr());
            ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(pty));
            ctx.disable_epilogue_cleanup(pname);
        } else {
            ctx.alloc_var(pname, pty.codegen_repr());
            if matches!(pty.codegen_repr(), PhpType::Str) {
                ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(pty));
            } else {
                ctx.set_var_ownership(pname, HeapOwnership::local_owner_for_type(pty));
            }
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

    let param_names: HashSet<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
    for (name, var) in &ctx.variables {
        if param_names.contains(name) {
            continue;
        }
        if matches!(
            &var.ty,
            PhpType::Str
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
        ) {
            super::abi::emit_store_zero_to_local_slot(emitter, var.stack_offset); // zero-init to prevent stale ptr free
        }
    }
    emit_activation_record_push(emitter, &ctx, &cleanup_label);

    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    emitter.label(epilogue_label);
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
    self::cleanup::emit_owned_local_epilogue_cleanup(emitter, &ctx);

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
    while !ctx.deferred_closures.is_empty() {
        let closures: Vec<_> = ctx.deferred_closures.drain(..).collect();
        for closure in closures {
            emit_closure(
                emitter,
                data,
                &closure.label,
                &closure.sig,
                &closure.body,
                all_functions,
                constants,
                interfaces,
                classes,
                packed_classes,
                extern_functions,
                extern_classes,
                extern_globals,
            );
        }
    }
}

pub(crate) use self::cleanup::emit_owned_local_epilogue_cleanup;
