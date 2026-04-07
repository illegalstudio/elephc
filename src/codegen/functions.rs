use std::collections::{HashMap, HashSet};

use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use super::stmt;
use crate::names::{function_epilogue_symbol, function_symbol};
use crate::parser::ast::{ExprKind, StmtKind, TypeExpr};
use crate::types::{
    ClassInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo,
    PhpType,
};

fn emit_load_from_caller_stack(emitter: &mut Emitter, reg: &str, offset: usize) {
    if offset <= 4095 {
        emitter.instruction(&format!("ldr {}, [x29, #{}]", reg, offset));       // load spilled incoming argument from the caller stack
    } else {
        emitter.instruction("mov x9, x29");                                     // seed a scratch pointer from the caller frame base
        let mut remaining = offset;
        while remaining > 0 {
            let chunk = remaining.min(4080);
            emitter.instruction(&format!("add x9, x9, #{}", chunk));            // advance the scratch pointer toward the spilled argument slot
            remaining -= chunk;
        }
        emitter.instruction(&format!("ldr {}, [x9]", reg));                     // load spilled incoming argument through the computed address
    }
}

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
    // Pass classes to regular functions so they can resolve Object types
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

    // Track ref params
    for (i, (pname, _pty)) in sig.params.iter().enumerate() {
        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        if is_ref {
            ctx.ref_params.insert(pname.clone());
            // For ref params, allocate 8 bytes (stores a pointer to the actual value)
            ctx.alloc_var(pname, PhpType::Int);
            // Set the variable type to the actual referenced type so loading
            // dereferences correctly (e.g., string ref loads x1/x2, not x0)
            ctx.update_var_type_and_ownership(
                pname,
                _pty.codegen_repr(),
                HeapOwnership::borrowed_alias_for_type(_pty),
            );
        } else if pname == "this" {
            ctx.alloc_var(pname, _pty.codegen_repr());
            ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(_pty));
            ctx.disable_epilogue_cleanup(pname);
        } else {
            ctx.alloc_var(pname, _pty.codegen_repr());
            if matches!(_pty.codegen_repr(), PhpType::Str) {
                ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(_pty));
            } else {
                ctx.set_var_ownership(pname, HeapOwnership::local_owner_for_type(_pty));
            }
        }
    }

    // Pre-allocate stack slots for params with defaults that aren't passed
    // (They'll be filled with default values at the call site or by the function prologue)

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

    // -- function prologue: set up stack frame --
    emitter.raw(".align 2");
    emitter.label_global(label);
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for locals
    if frame_size - 16 <= 504 {
        emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16)); //save caller's frame ptr & return addr
    } else {
        emitter.instruction(&format!("add x9, sp, #{}", frame_size - 16));      // compute address of the saved frame-link area for large frames
        emitter.instruction("stp x29, x30, [x9]");                              // save caller's frame ptr & return addr through the computed address
    }
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));         // set new frame pointer

    // -- save parameters from registers to local stack slots --
    // ARM64 ABI: int/bool/array args in x0-x7, float args in d0-d7
    // Strings use two consecutive int registers (ptr + len)
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    let mut caller_stack_offset = 32usize;
    let mut int_stack_only = false;
    let mut float_stack_only = false;
    for (i, (pname, pty)) in sig.params.iter().enumerate() {
        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        let var = ctx
            .variables
            .get(pname)
            .expect("codegen bug: param was just allocated but not found in variables map");
        let offset = var.stack_offset;
        if is_ref {
            if !int_stack_only && int_reg_idx < 8 {
                emitter.comment(&format!("param &${} from x{} (ref)", pname, int_reg_idx));
                super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save address of referenced variable from an integer register
                int_reg_idx += 1;
            } else {
                emitter.comment(&format!("param &${} from caller stack +{}", pname, caller_stack_offset));
                emit_load_from_caller_stack(emitter, "x10", caller_stack_offset);
                super::abi::store_at_offset(emitter, "x10", offset);            // save the spilled reference argument into the local slot
                caller_stack_offset += 16;
                int_stack_only = true;
            }
        } else {
            match pty {
                PhpType::Bool | PhpType::Int => {
                    if !int_stack_only && int_reg_idx < 8 {
                        emitter.comment(&format!("param ${} from x{}", pname, int_reg_idx));
                        super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save int/bool param from an integer register
                        int_reg_idx += 1;
                    } else {
                        emitter.comment(&format!("param ${} from caller stack +{}", pname, caller_stack_offset));
                        emit_load_from_caller_stack(emitter, "x10", caller_stack_offset);
                        super::abi::store_at_offset(emitter, "x10", offset);    // save the spilled int/bool parameter into the local slot
                        caller_stack_offset += 16;
                        int_stack_only = true;
                    }
                }
                PhpType::Float => {
                    if !float_stack_only && float_reg_idx < 8 {
                        emitter.comment(&format!("param ${} from d{}", pname, float_reg_idx));
                        super::abi::store_at_offset(emitter, &format!("d{}", float_reg_idx), offset); // save float param from a floating-point register
                        float_reg_idx += 1;
                    } else {
                        emitter.comment(&format!("param ${} from caller stack +{}", pname, caller_stack_offset));
                        emit_load_from_caller_stack(emitter, "d15", caller_stack_offset);
                        super::abi::store_at_offset(emitter, "d15", offset);    // save the spilled float parameter into the local slot
                        caller_stack_offset += 16;
                        float_stack_only = true;
                    }
                }
                PhpType::Str => {
                    if !int_stack_only && int_reg_idx + 1 < 8 {
                        emitter.comment(&format!(
                            "param ${} from x{},x{}",
                            pname,
                            int_reg_idx,
                            int_reg_idx + 1
                        ));
                        super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save string pointer from the integer-register pair
                        super::abi::store_at_offset(
                            emitter,
                            &format!("x{}", int_reg_idx + 1),
                            offset - 8,
                        ); // save string length from the integer-register pair
                        int_reg_idx += 2;
                    } else {
                        emitter.comment(&format!("param ${} from caller stack +{}", pname, caller_stack_offset));
                        emit_load_from_caller_stack(emitter, "x10", caller_stack_offset);
                        emit_load_from_caller_stack(emitter, "x11", caller_stack_offset + 8);
                        super::abi::store_at_offset(emitter, "x10", offset);    // save the spilled string pointer into the local slot
                        super::abi::store_at_offset(emitter, "x11", offset - 8); // save the spilled string length into the local slot
                        caller_stack_offset += 16;
                        int_stack_only = true;
                    }
                }
                PhpType::Void => {}
                PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Buffer(_)
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {
                    if !int_stack_only && int_reg_idx < 8 {
                        emitter.comment(&format!("param ${} from x{}", pname, int_reg_idx));
                        super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save heap/object/callable parameter from an integer register
                        int_reg_idx += 1;
                    } else {
                        emitter.comment(&format!("param ${} from caller stack +{}", pname, caller_stack_offset));
                        emit_load_from_caller_stack(emitter, "x10", caller_stack_offset);
                        super::abi::store_at_offset(emitter, "x10", offset);    // save the spilled heap/object/callable parameter into the local slot
                        caller_stack_offset += 16;
                        int_stack_only = true;
                    }
                }
            }
        }
    }

    // -- zero-initialize local variables that may be deep-freed on reassignment --
    // Without this, the first free-on-reassign would see stale stack values
    // (left over from a previous function call at the same stack address)
    // and try to deep-free a random heap pointer.
    let param_names: HashSet<String> = sig.params.iter().map(|(n, _)| n.clone()).collect();
    for (name, var) in &ctx.variables {
        if param_names.contains(name) {
            continue; // Parameters are initialized by register stores above
        }
        if matches!(
            &var.ty,
            PhpType::Str
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
        ) {
            super::abi::store_at_offset(emitter, "xzr", var.stack_offset); // zero-init to prevent stale ptr free
        }
    }
    emit_activation_record_push(emitter, &ctx, &cleanup_label);

    // -- emit function body statements --
    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    // -- function epilogue: save static vars back and restore/return --
    emitter.label(epilogue_label);
    let needs_return_preserve = epilogue_has_side_effects(&ctx);
    if needs_return_preserve {
        preserve_return_registers(emitter, &ctx, &sig.return_type);
    }

    // Save static vars back to global storage before returning
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
            emitter.adrp("x9", &format!("{}", data_label));      // load page of static var storage
            emitter.add_lo12("x9", "x9", &format!("{}", data_label)); //add page offset
                                                                                 // Note: x9 holds the global storage address, so we use x8 as scratch for large offsets
            match &ty {
                PhpType::Bool | PhpType::Int => {
                    if offset <= 255 {
                        emitter.instruction(&format!("ldur x10, [x29, #-{}]", offset)); //load local value
                    } else {
                        emitter.instruction(&format!("sub x8, x29, #{}", offset)); //compute stack address for large offset
                        emitter.instruction("ldr x10, [x8]");                   // load local value via computed address
                    }
                    emitter.instruction("str x10, [x9]");                       // save to static storage
                }
                PhpType::Float => {
                    if offset <= 255 {
                        emitter.instruction(&format!("ldur d0, [x29, #-{}]", offset)); //load local float
                    } else {
                        emitter.instruction(&format!("sub x8, x29, #{}", offset)); //compute stack address for large offset
                        emitter.instruction("ldr d0, [x8]");                    // load local float via computed address
                    }
                    emitter.instruction("str d0, [x9]");                        // save to static storage
                }
                PhpType::Str => {
                    if offset <= 255 {
                        emitter.instruction(&format!("ldur x10, [x29, #-{}]", offset)); //load string ptr
                    } else {
                        emitter.instruction(&format!("sub x8, x29, #{}", offset)); //compute stack address for large offset
                        emitter.instruction("ldr x10, [x8]");                   // load string ptr via computed address
                    }
                    let len_offset = offset - 8;
                    if len_offset <= 255 {
                        emitter.instruction(&format!("ldur x11, [x29, #-{}]", len_offset)); //load string len
                    } else {
                        emitter.instruction(&format!("sub x8, x29, #{}", len_offset)); //compute stack address for large offset
                        emitter.instruction("ldr x11, [x8]");                   // load string len via computed address
                    }
                    emitter.instruction("str x10, [x9]");                       // save ptr to static storage
                    emitter.instruction("str x11, [x9, #8]");                   // save len to static storage
                }
                _ => {
                    if offset <= 255 {
                        emitter.instruction(&format!("ldur x10, [x29, #-{}]", offset)); //load local value
                    } else {
                        emitter.instruction(&format!("sub x8, x29, #{}", offset)); //compute stack address for large offset
                        emitter.instruction("ldr x10, [x8]");                   // load local value via computed address
                    }
                    emitter.instruction("str x10, [x9]");                       // save to static storage
                }
            }
        }
    }

    emit_activation_record_pop(emitter, &ctx);
    emit_owned_local_epilogue_cleanup(emitter, &ctx);

    if needs_return_preserve {
        restore_return_registers(emitter, &ctx, &sig.return_type);
    }
    if frame_size - 16 <= 504 {
        emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16)); //restore frame ptr & return addr
    } else {
        emitter.instruction(&format!("add x9, sp, #{}", frame_size - 16));      // compute address of the saved frame-link area for large frames
        emitter.instruction("ldp x29, x30, [x9]");                              // restore frame ptr & return addr through the computed address
    }
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
    emit_frame_cleanup_callback(emitter, &ctx, &cleanup_label);

    // -- emit any closures deferred during this function's body --
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

fn preserve_return_registers(emitter: &mut Emitter, ctx: &Context, return_ty: &PhpType) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    match return_ty {
        PhpType::Float => {
            super::abi::store_at_offset(emitter, "d0", return_offset);          // preserve the float return value in the hidden frame slot across epilogue side effects
        }
        PhpType::Str => {
            super::abi::store_at_offset(emitter, "x1", return_offset);          // preserve the string return pointer in the hidden frame slot across epilogue side effects
            super::abi::store_at_offset(emitter, "x2", return_offset - 8);      // preserve the string return length in the hidden frame slot across epilogue side effects
        }
        _ => {
            super::abi::store_at_offset(emitter, "x0", return_offset);          // preserve the scalar/object return value in the hidden frame slot across epilogue side effects
        }
    }
}

fn restore_return_registers(emitter: &mut Emitter, ctx: &Context, return_ty: &PhpType) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    match return_ty {
        PhpType::Float => {
            super::abi::load_at_offset(emitter, "d0", return_offset);           // restore the float return value from the hidden frame slot after epilogue cleanup
        }
        PhpType::Str => {
            super::abi::load_at_offset(emitter, "x1", return_offset);           // restore the string return pointer from the hidden frame slot after epilogue cleanup
            super::abi::load_at_offset(emitter, "x2", return_offset - 8);       // restore the string return length from the hidden frame slot after epilogue cleanup
        }
        _ => {
            super::abi::load_at_offset(emitter, "x0", return_offset);           // restore the scalar/object return value from the hidden frame slot after epilogue cleanup
        }
    }
}

fn epilogue_has_side_effects(ctx: &Context) -> bool {
    !ctx.static_vars.is_empty()
        || ctx.variables.iter().any(|(name, var)| {
            !ctx.global_vars.contains(name)
                && !ctx.static_vars.contains(name)
                && !ctx.ref_params.contains(name)
                && var.epilogue_cleanup_safe
                && var.ownership == HeapOwnership::Owned
                && (matches!(var.ty, PhpType::Str) || var.ty.is_refcounted())
        })
}

pub(crate) fn emit_owned_local_epilogue_cleanup(emitter: &mut Emitter, ctx: &Context) {
    let mut cleanup_vars: Vec<_> = ctx
        .variables
        .iter()
        .filter(|(name, var)| {
            !ctx.global_vars.contains(*name)
                && !ctx.static_vars.contains(*name)
                && !ctx.ref_params.contains(*name)
                && var.epilogue_cleanup_safe
                && var.ownership == HeapOwnership::Owned
        })
        .collect();
    cleanup_vars.sort_by_key(|(_, var)| var.stack_offset);

    for (name, var) in cleanup_vars {
        match &var.ty {
            PhpType::Str => {
                emitter.comment(&format!("epilogue cleanup ${}", name));
                super::abi::load_at_offset(emitter, "x0", var.stack_offset); // load owned string pointer from local slot
                emitter.instruction("bl __rt_heap_free_safe");                  // release owned string storage before returning
            }
            ty if ty.is_refcounted() => {
                emitter.comment(&format!("epilogue cleanup ${}", name));
                super::abi::load_at_offset(emitter, "x0", var.stack_offset); // load owned heap pointer from local slot
                super::abi::emit_decref_if_refcounted(emitter, ty);
            }
            _ => {}
        }
    }
}

fn emit_activation_record_push(emitter: &mut Emitter, ctx: &Context, cleanup_label: &str) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing activation prev slot");
    let cleanup_offset = ctx
        .activation_cleanup_offset
        .expect("codegen bug: missing activation cleanup slot");
    let frame_base_offset = ctx
        .activation_frame_base_offset
        .expect("codegen bug: missing activation frame-base slot");

    emitter.comment("register exception cleanup frame");
    emitter.adrp("x9", "_exc_call_frame_top");                   // load page of the call-frame stack top
    emitter.add_lo12("x9", "x9", "_exc_call_frame_top");             // resolve the call-frame stack top address
    emitter.instruction("ldr x10, [x9]");                                       // load the previous call-frame pointer
    super::abi::store_at_offset(emitter, "x10", prev_offset); // save the previous call-frame pointer in this frame record
    emitter.adrp("x10", &format!("{}", cleanup_label));          // load page of the cleanup callback label
    emitter.add_lo12("x10", "x10", &format!("{}", cleanup_label));   // resolve the cleanup callback label address
    super::abi::store_at_offset(emitter, "x10", cleanup_offset); // save the cleanup callback address in this frame record
    emitter.instruction("mov x10, x29");                                        // x10 = current frame pointer for cleanup callbacks
    super::abi::store_at_offset(emitter, "x10", frame_base_offset); // save the current frame pointer in this frame record
    super::abi::store_at_offset(
        emitter,
        "xzr",
        ctx.pending_action_offset
            .expect("codegen bug: missing pending-action slot"),
    ); // clear pending finally action for this activation
    emitter.instruction(&format!("sub x10, x29, #{}", prev_offset));            // x10 = address of this activation record's first slot
    emitter.adrp("x9", "_exc_call_frame_top");                   // reload page of the call-frame stack top after stack-slot stores may clobber x9
    emitter.add_lo12("x9", "x9", "_exc_call_frame_top");             // resolve the call-frame stack top address again
    emitter.instruction("str x10, [x9]");                                       // publish this activation record as the new call-frame stack top
}

fn emit_activation_record_pop(emitter: &mut Emitter, ctx: &Context) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing activation prev slot");

    emitter.comment("unregister exception cleanup frame");
    emitter.adrp("x9", "_exc_call_frame_top");                   // load page of the call-frame stack top
    emitter.add_lo12("x9", "x9", "_exc_call_frame_top");             // resolve the call-frame stack top address
    super::abi::load_at_offset(emitter, "x10", prev_offset); // reload the previous call-frame pointer from this activation
    emitter.adrp("x9", "_exc_call_frame_top");                   // reload page of the call-frame stack top after the load helper may clobber x9
    emitter.add_lo12("x9", "x9", "_exc_call_frame_top");             // resolve the call-frame stack top address again
    emitter.instruction("str x10, [x9]");                                       // restore the previous call-frame stack top before returning
}

fn emit_frame_cleanup_callback(emitter: &mut Emitter, ctx: &Context, cleanup_label: &str) {
    emitter.label(cleanup_label);
    emitter.instruction("sub sp, sp, #16");                                     // reserve callback spill space for x29/x30
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save the caller frame pointer and return address
    emitter.instruction("mov x29, x0");                                         // treat the unwound activation's frame pointer as our temporary base
    emit_owned_local_epilogue_cleanup(emitter, ctx);
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the callback frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the callback spill space
    emitter.instruction("ret");                                                 // finish unwound-frame cleanup callback
    emitter.blank();
}

fn mark_control_flow_epilogue_unsafe(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
    in_control_flow: bool,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, .. } => {
                if in_control_flow {
                    ctx.disable_epilogue_cleanup(name);
                }
            }
            StmtKind::ListUnpack { vars, .. } => {
                if in_control_flow {
                    for var in vars {
                        ctx.disable_epilogue_cleanup(var);
                    }
                }
            }
            StmtKind::Global { vars } => {
                for var in vars {
                    ctx.disable_epilogue_cleanup(var);
                }
            }
            StmtKind::StaticVar { name, .. } => {
                ctx.disable_epilogue_cleanup(name);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                let direct_assigns = exhaustive_if_direct_heap_assignments(
                    then_body,
                    elseif_clauses,
                    else_body,
                    ctx,
                    sig,
                );
                mark_control_flow_epilogue_unsafe(then_body, ctx, sig, true);
                for (_, body) in elseif_clauses {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
                if let Some(body) = else_body {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
                for (name, ty) in direct_assigns {
                    if ctx.global_vars.contains(&name)
                        || ctx.static_vars.contains(&name)
                        || ctx.ref_params.contains(&name)
                    {
                        continue;
                    }
                    let Some(var) = ctx.variables.get(&name) else {
                        continue;
                    };
                    if var.ty != ty {
                        continue;
                    }
                    ctx.update_var_type_and_ownership(
                        &name,
                        ty.clone(),
                        HeapOwnership::local_owner_for_type(&ty),
                    );
                    ctx.enable_epilogue_cleanup(&name);
                }
            }
            StmtKind::Foreach {
                body,
                key_var,
                value_var,
                ..
            } => {
                ctx.disable_epilogue_cleanup(value_var);
                if let Some(key_var) = key_var {
                    ctx.disable_epilogue_cleanup(key_var);
                }
                mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
            }
            StmtKind::DoWhile { body, .. } | StmtKind::While { body, .. } => {
                mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
            }
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(stmt) = init {
                    mark_control_flow_epilogue_unsafe(
                        std::slice::from_ref(stmt.as_ref()),
                        ctx,
                        sig,
                        true,
                    );
                }
                if let Some(stmt) = update {
                    mark_control_flow_epilogue_unsafe(
                        std::slice::from_ref(stmt.as_ref()),
                        ctx,
                        sig,
                        true,
                    );
                }
                mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
                if let Some(body) = default {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                mark_control_flow_epilogue_unsafe(try_body, ctx, sig, true);
                for catch_clause in catches {
                    mark_control_flow_epilogue_unsafe(&catch_clause.body, ctx, sig, true);
                }
                if let Some(body) = finally_body {
                    mark_control_flow_epilogue_unsafe(body, ctx, sig, true);
                }
            }
            _ => {}
        }
    }
}

fn collect_try_slots(stmts: &[crate::parser::ast::Stmt], ctx: &mut Context) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let slot_offset = ctx.alloc_hidden_slot(208);
                ctx.try_slot_offsets.push(slot_offset);
                collect_try_slots(try_body, ctx);
                for catch_clause in catches {
                    collect_try_slots(&catch_clause.body, ctx);
                }
                if let Some(body) = finally_body {
                    collect_try_slots(body, ctx);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_try_slots(then_body, ctx);
                for (_, body) in elseif_clauses {
                    collect_try_slots(body, ctx);
                }
                if let Some(body) = else_body {
                    collect_try_slots(body, ctx);
                }
            }
            StmtKind::Foreach { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. } => collect_try_slots(body, ctx),
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(s) = init {
                    collect_try_slots(&[*s.clone()], ctx);
                }
                if let Some(s) = update {
                    collect_try_slots(&[*s.clone()], ctx);
                }
                collect_try_slots(body, ctx);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_try_slots(body, ctx);
                }
                if let Some(body) = default {
                    collect_try_slots(body, ctx);
                }
            }
            _ => {}
        }
    }
}

fn collect_straight_line_direct_assignments(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &Context,
    sig: &FunctionSig,
) -> (HashMap<String, PhpType>, bool) {
    let mut assignments = HashMap::new();
    let mut may_fall_through = true;

    for stmt in stmts {
        if !may_fall_through {
            break;
        }
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                assignments.insert(name.clone(), infer_local_type(value, sig, Some(ctx)));
            }
            StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => {
                may_fall_through = false;
            }
            _ => {}
        }
    }

    (assignments, may_fall_through)
}

fn exhaustive_if_direct_heap_assignments(
    then_body: &[crate::parser::ast::Stmt],
    elseif_clauses: &[(crate::parser::ast::Expr, Vec<crate::parser::ast::Stmt>)],
    else_body: &Option<Vec<crate::parser::ast::Stmt>>,
    ctx: &Context,
    sig: &FunctionSig,
) -> HashMap<String, PhpType> {
    let Some(else_body) = else_body.as_ref() else {
        return HashMap::new();
    };

    let mut branch_assignments = Vec::new();
    let (then_assigns, then_falls_through) =
        collect_straight_line_direct_assignments(then_body, ctx, sig);
    if then_falls_through {
        branch_assignments.push(then_assigns);
    }
    for (_, body) in elseif_clauses {
        let (assigns, falls_through) = collect_straight_line_direct_assignments(body, ctx, sig);
        if falls_through {
            branch_assignments.push(assigns);
        }
    }
    let (else_assigns, else_falls_through) =
        collect_straight_line_direct_assignments(else_body, ctx, sig);
    if else_falls_through {
        branch_assignments.push(else_assigns);
    }

    let Some((first_branch, remaining_branches)) = branch_assignments.split_first() else {
        return HashMap::new();
    };
    let mut definitely_assigned = first_branch.clone();
    definitely_assigned.retain(|name, ty| {
        (matches!(ty, PhpType::Str) || ty.is_refcounted())
            && remaining_branches
                .iter()
                .all(|assigns| assigns.get(name) == Some(ty))
    });
    definitely_assigned
}

/// Pre-scan function body for variable assignments to allocate stack slots.
pub fn collect_local_vars(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
    sig: &FunctionSig,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if !ctx.variables.contains_key(name) {
                    let ty = infer_local_type(value, sig, Some(ctx)).codegen_repr();
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::TypedAssign {
                type_expr, name, ..
            } => {
                if !ctx.variables.contains_key(name) {
                    let ty = codegen_declared_type(type_expr, ctx).codegen_repr();
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::Global { vars } => {
                // Allocate local slots for global vars (they'll be loaded from global storage)
                for name in vars {
                    if !ctx.variables.contains_key(name) {
                        ctx.alloc_var(name, PhpType::Int);
                    }
                }
            }
            StmtKind::StaticVar { name, init } => {
                // Allocate local slot for the static var
                if !ctx.variables.contains_key(name) {
                    let ty = infer_local_type(init, sig, Some(ctx)).codegen_repr();
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_local_vars(then_body, ctx, sig);
                for (_, body) in elseif_clauses {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = else_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_local_vars(try_body, ctx, sig);
                for catch_clause in catches {
                    let catch_type_name = resolve_codegen_catch_type_name(
                        ctx,
                        catch_clause
                            .exception_types
                            .first()
                            .map(|name| name.as_str())
                            .unwrap_or("Throwable"),
                    );
                    if let Some(variable) = &catch_clause.variable {
                        if !ctx.variables.contains_key(variable) {
                            ctx.alloc_var(variable, PhpType::Object(catch_type_name));
                        }
                    }
                    collect_local_vars(&catch_clause.body, ctx, sig);
                }
                if let Some(body) = finally_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::Foreach {
                value_var,
                body,
                array,
                key_var,
                ..
            } => {
                let arr_ty = infer_local_type(array, sig, Some(ctx));
                if let Some(k) = key_var {
                    if !ctx.variables.contains_key(k) {
                        // Assoc array keys are strings; indexed array keys are ints
                        let key_ty = if matches!(&arr_ty, PhpType::AssocArray { .. }) {
                            PhpType::Str
                        } else {
                            PhpType::Int
                        };
                        ctx.alloc_var(k, key_ty.codegen_repr());
                    }
                }
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match &arr_ty {
                        PhpType::Array(t) => *t.clone(),
                        PhpType::AssocArray { value, .. } => *value.clone(),
                        _ => PhpType::Int,
                    };
                    ctx.alloc_var(value_var, elem_ty.codegen_repr());
                }
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = default {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::ConstDecl { .. } => {}
            StmtKind::ListUnpack { vars, value, .. } => {
                let elem_ty = match infer_local_type(value, sig, Some(ctx)) {
                    PhpType::Array(t) => *t,
                    _ => PhpType::Int,
                };
                for var in vars {
                    if !ctx.variables.contains_key(var) {
                        ctx.alloc_var(var, elem_ty.codegen_repr());
                    }
                }
            }
            StmtKind::ArrayAssign { .. }
            | StmtKind::ArrayPush { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => {}
            StmtKind::PropertyAssign { value, .. } => {
                // Just recurse into value to pick up any nested assignments
                if let ExprKind::Variable(_) = &value.kind {
                    // nothing to allocate
                } else {
                    // Look for nested function calls or closures that might need temp vars
                }
            }
            StmtKind::DoWhile { body, .. } | StmtKind::While { body, .. } => {
                collect_local_vars(body, ctx, sig);
            }
            StmtKind::For {
                init, update, body, ..
            } => {
                if let Some(s) = init {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                if let Some(s) = update {
                    collect_local_vars(&[*s.clone()], ctx, sig);
                }
                collect_local_vars(body, ctx, sig);
            }
            _ => {}
        }
    }
}

fn resolve_codegen_catch_type_name(ctx: &Context, raw_name: &str) -> String {
    match raw_name {
        "self" => ctx
            .current_class
            .clone()
            .unwrap_or_else(|| raw_name.to_string()),
        "parent" => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.clone())
            .unwrap_or_else(|| raw_name.to_string()),
        _ => raw_name.to_string(),
    }
}

/// Public wrapper for infer_local_type, used by closure return type inference.
pub fn infer_local_type_pub(expr: &crate::parser::ast::Expr, sig: &FunctionSig) -> PhpType {
    infer_local_type(expr, sig, None)
}

/// Public wrapper for infer_local_type with codegen context access.
/// Used by ternary codegen to infer branch types using variable/class info.
pub fn infer_local_type_with_ctx(
    expr: &crate::parser::ast::Expr,
    sig: &FunctionSig,
    ctx: &Context,
) -> PhpType {
    infer_local_type(expr, sig, Some(ctx))
}

/// Infer an expression type using only the current codegen context.
/// Useful in expression codegen where stack locals, closures, functions, and
/// class metadata are available, but the enclosing function signature is not.
pub fn infer_contextual_type(expr: &crate::parser::ast::Expr, ctx: &Context) -> PhpType {
    let empty_sig = FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Void,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
    };
    infer_local_type(expr, &empty_sig, Some(ctx))
}

/// Returns the wider of two types for stack slot allocation.
/// Str (16 bytes) is wider than everything else (8 bytes).
fn wider_of(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if matches!(a, PhpType::Mixed | PhpType::Union(_))
        || matches!(b, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if matches!(a, PhpType::Array(_)) || matches!(b, PhpType::Array(_)) {
        return a.clone();
    }
    if matches!(a, PhpType::Object(_)) || matches!(b, PhpType::Object(_)) {
        return a.clone();
    }
    a.clone()
}

fn resolve_buffer_element_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Named(name) => {
            if ctx.packed_classes.contains_key(name.as_str()) {
                PhpType::Packed(name.as_str().to_string())
            } else {
                PhpType::Int
            }
        }
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Int,
    }
}

pub(crate) fn codegen_declared_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Named(name) => match name.as_str() {
            "string" => PhpType::Str,
            "mixed" => PhpType::Mixed,
            "callable" => PhpType::Callable,
            "void" => PhpType::Void,
            "array" => PhpType::Array(Box::new(PhpType::Int)),
            _ if ctx.packed_classes.contains_key(name.as_str()) => {
                PhpType::Packed(name.as_str().to_string())
            }
            _ if ctx.classes.contains_key(name.as_str())
                || ctx.interfaces.contains_key(name.as_str())
                || ctx.extern_classes.contains_key(name.as_str()) =>
            {
                PhpType::Object(name.as_str().to_string())
            }
            _ => PhpType::Int,
        },
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => PhpType::Mixed,
    }
}

fn infer_local_type(
    expr: &crate::parser::ast::Expr,
    sig: &FunctionSig,
    ctx: Option<&Context>,
) -> PhpType {
    match &expr.kind {
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::Variable(name) => {
            // Check if it's a known parameter — use its type from the signature
            for (pname, pty) in &sig.params {
                if pname == name {
                    return pty.clone();
                }
            }
            // Check if it's an already-allocated local variable
            if let Some(c) = ctx {
                if let Some(var) = c.variables.get(name) {
                    return var.ty.clone();
                }
            }
            PhpType::Int
        }
        ExprKind::ArrayLiteral(elems) => {
            let elem_ty = if elems.is_empty() {
                PhpType::Int
            } else {
                infer_local_type(&elems[0], sig, ctx)
            };
            PhpType::Array(Box::new(elem_ty))
        }
        ExprKind::ArrayAccess { array, .. } => match infer_local_type(array, sig, ctx) {
            PhpType::Str => PhpType::Str,
            PhpType::Array(t) => *t,
            PhpType::AssocArray { value, .. } => *value,
            PhpType::Buffer(t) => match *t {
                PhpType::Packed(name) => PhpType::Pointer(Some(name)),
                other => other,
            },
            _ => PhpType::Int,
        },
        ExprKind::Negate(inner) => {
            let inner_ty = infer_local_type(inner, sig, ctx);
            if inner_ty == PhpType::Float {
                PhpType::Float
            } else {
                PhpType::Int
            }
        }
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::BitNot(_) => PhpType::Int,
        ExprKind::NullCoalesce { value, default } => {
            let left = infer_local_type(value, sig, ctx);
            let right = infer_local_type(default, sig, ctx);
            wider_of(&left, &right)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_local_type(then_expr, sig, ctx);
            let else_ty = infer_local_type(else_expr, sig, ctx);
            wider_of(&then_ty, &else_ty)
        }
        ExprKind::BinaryOp { left, op, right } => {
            use crate::parser::ast::BinOp;
            match op {
                BinOp::Concat => PhpType::Str,
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::StrictEq
                | BinOp::StrictNotEq
                | BinOp::And
                | BinOp::Or => PhpType::Bool,
                BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::ShiftLeft
                | BinOp::ShiftRight
                | BinOp::Spaceship => PhpType::Int,
                BinOp::NullCoalesce => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    wider_of(&lt, &rt)
                }
                BinOp::Div | BinOp::Pow => PhpType::Float,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Mod => {
                    let lt = infer_local_type(left, sig, ctx);
                    let rt = infer_local_type(right, sig, ctx);
                    if lt == PhpType::Float || rt == PhpType::Float {
                        PhpType::Float
                    } else {
                        PhpType::Int
                    }
                }
            }
        }
        ExprKind::FunctionCall { name, args } => {
            match name.as_str() {
                // String-returning builtins
                "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords" | "trim"
                | "ltrim" | "rtrim" | "substr" | "str_repeat" | "strrev" | "str_replace"
                | "str_ireplace" | "substr_replace" | "str_pad" | "chr" | "implode" | "join"
                | "sprintf" | "number_format" | "nl2br" | "wordwrap" | "addslashes"
                | "stripslashes" | "htmlspecialchars" | "html_entity_decode" | "htmlentities"
                | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode" | "base64_encode"
                | "base64_decode" | "bin2hex" | "hex2bin" | "md5" | "sha1" | "hash" | "gettype"
                | "strstr" | "readline" | "date" | "json_encode" | "php_uname" | "phpversion"
                | "file_get_contents" | "tempnam" | "getcwd" | "shell_exec" => PhpType::Str,
                // Array-returning builtins
                "explode"
                | "str_split"
                | "file"
                | "scandir"
                | "glob"
                | "array_keys"
                | "array_values"
                | "array_merge"
                | "array_slice"
                | "array_reverse"
                | "array_unique"
                | "array_chunk"
                | "array_pad"
                | "array_fill"
                | "array_fill_keys"
                | "array_diff"
                | "array_intersect"
                | "array_diff_key"
                | "array_intersect_key"
                | "array_flip"
                | "array_combine"
                | "array_splice"
                | "array_column"
                | "array_map"
                | "array_filter"
                | "range"
                | "array_rand"
                | "sscanf"
                | "fgetcsv"
                | "preg_split" => {
                    // Try to infer element type from arguments
                    if name.as_str() == "explode"
                        || name.as_str() == "str_split"
                        || name.as_str() == "file"
                        || name.as_str() == "scandir"
                        || name.as_str() == "glob"
                        || name.as_str() == "fgetcsv"
                        || name.as_str() == "preg_split"
                    {
                        PhpType::Array(Box::new(PhpType::Str))
                    } else if !args.is_empty() {
                        let arr_ty = infer_local_type(&args[0], sig, ctx);
                        match arr_ty {
                            PhpType::Array(t) => PhpType::Array(t),
                            _ => PhpType::Array(Box::new(PhpType::Int)),
                        }
                    } else {
                        PhpType::Array(Box::new(PhpType::Int))
                    }
                }
                // Float-returning builtins
                "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow" | "fmod" | "fdiv"
                | "microtime" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
                | "sinh" | "cosh" | "tanh" | "log" | "log2" | "log10" | "exp" | "hypot" | "pi"
                | "deg2rad" | "rad2deg" => PhpType::Float,
                // Bool-returning builtins
                "is_int" | "is_float" | "is_string" | "is_bool" | "is_null" | "is_numeric"
                | "is_nan" | "is_finite" | "is_infinite" | "is_array" | "empty" | "isset"
                | "is_file" | "is_dir" | "is_readable" | "is_writable" | "file_exists"
                | "in_array" | "array_key_exists" | "str_contains" | "str_starts_with"
                | "str_ends_with" | "ctype_alpha" | "ctype_digit" | "ctype_alnum"
                | "ctype_space" | "function_exists" | "ptr_is_null" => PhpType::Bool,
                "abs" => {
                    if !args.is_empty() {
                        let t = infer_local_type(&args[0], sig, ctx);
                        if t == PhpType::Float {
                            PhpType::Float
                        } else {
                            PhpType::Int
                        }
                    } else {
                        PhpType::Int
                    }
                }
                "min" | "max" => {
                    if args.len() >= 2 {
                        let t0 = infer_local_type(&args[0], sig, ctx);
                        let t1 = infer_local_type(&args[1], sig, ctx);
                        if t0 == PhpType::Float || t1 == PhpType::Float {
                            PhpType::Float
                        } else {
                            PhpType::Int
                        }
                    } else {
                        PhpType::Int
                    }
                }
                // Pointer-returning builtins
                "ptr" | "ptr_null" => PhpType::Pointer(None),
                "buffer_len" => PhpType::Int,
                "ptr_offset" => {
                    if let Some(first_arg) = args.first() {
                        match infer_local_type(first_arg, sig, ctx) {
                            PhpType::Pointer(tag) => PhpType::Pointer(tag),
                            _ => PhpType::Pointer(None),
                        }
                    } else {
                        PhpType::Pointer(None)
                    }
                }
                "ptr_get" | "ptr_read8" | "ptr_read32" | "ptr_sizeof" => PhpType::Int,
                // User-defined functions — check signature if available
                _ => {
                    if let Some(c) = ctx {
                        if let Some(fn_sig) = c.functions.get(name.as_str()) {
                            return fn_sig.return_type.clone();
                        }
                    }
                    PhpType::Int
                }
            }
        }
        ExprKind::Cast { target, .. } => {
            use crate::parser::ast::CastType;
            match target {
                CastType::Int => PhpType::Int,
                CastType::Float => PhpType::Float,
                CastType::String => PhpType::Str,
                CastType::Bool => PhpType::Bool,
                CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
            }
        }
        ExprKind::Closure { .. } => PhpType::Callable,
        ExprKind::ClosureCall { var, .. } => {
            if let Some(c) = ctx {
                if let Some(sig) = c.closure_sigs.get(var) {
                    return sig.return_type.clone();
                }
            }
            PhpType::Int
        }
        ExprKind::ExprCall { callee, .. } => {
            if let Some(c) = ctx {
                match &callee.kind {
                    ExprKind::Variable(var_name) => {
                        if let Some(sig) = c.closure_sigs.get(var_name) {
                            return sig.return_type.clone();
                        }
                    }
                    ExprKind::ArrayAccess { array, .. } => {
                        if let ExprKind::Variable(arr_name) = &array.kind {
                            if let Some(sig) = c.closure_sigs.get(arr_name) {
                                return sig.return_type.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let ExprKind::Closure { body, .. } = &callee.kind {
                return crate::types::checker::infer_return_type_syntactic(body);
            }
            PhpType::Int
        }
        ExprKind::ConstRef(_) => PhpType::Int, // constants resolved at emit time
        ExprKind::EnumCase { enum_name, .. } => PhpType::Object(enum_name.as_str().to_string()),
        ExprKind::Spread(inner) => infer_local_type(inner, sig, ctx),
        ExprKind::NamedArg { value, .. } => infer_local_type(value, sig, ctx),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.as_str().to_string()),
        ExprKind::BufferNew { element_type, .. } => {
            if let Some(c) = ctx {
                let elem_ty = resolve_buffer_element_type(element_type, c);
                PhpType::Buffer(Box::new(elem_ty))
            } else {
                PhpType::Buffer(Box::new(PhpType::Int))
            }
        }
        ExprKind::PropertyAccess { object, property } => {
            if let Some(c) = ctx {
                let obj_ty = infer_local_type(object, sig, Some(c));
                if let PhpType::Object(cn) = &obj_ty {
                    if let Some(ci) = c.classes.get(cn) {
                        if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == property) {
                            return ty.clone();
                        }
                        if let Some(sig) = ci.methods.get("__get") {
                            return sig.return_type.clone();
                        }
                    }
                }
                if let PhpType::Pointer(Some(cn)) = &obj_ty {
                    if let Some(ci) = c.extern_classes.get(cn) {
                        if let Some(field) = ci.fields.iter().find(|field| field.name == *property)
                        {
                            return field.php_type.clone();
                        }
                    }
                    if let Some(ci) = c.packed_classes.get(cn) {
                        if let Some(field) = ci.fields.iter().find(|field| field.name == *property)
                        {
                            return field.php_type.clone();
                        }
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::MethodCall { object, method, .. } => {
            if let Some(c) = ctx {
                let obj_ty = infer_local_type(object, sig, Some(c));
                if let PhpType::Object(cn) = &obj_ty {
                    if let Some(ci) = c.classes.get(cn) {
                        if let Some(msig) = ci.methods.get(method) {
                            return msig.return_type.clone();
                        }
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => {
            if let Some(c) = ctx {
                let class_name = match receiver {
                    crate::parser::ast::StaticReceiver::Named(class_name) => {
                        class_name.as_str().to_string()
                    }
                    crate::parser::ast::StaticReceiver::Self_
                    | crate::parser::ast::StaticReceiver::Static => {
                        if let Some(current_class) = &c.current_class {
                            current_class.clone()
                        } else {
                            return PhpType::Int;
                        }
                    }
                    crate::parser::ast::StaticReceiver::Parent => {
                        if let Some(current_class) = &c.current_class {
                            if let Some(parent_name) = c
                                .classes
                                .get(current_class)
                                .and_then(|ci| ci.parent.as_ref())
                            {
                                parent_name.clone()
                            } else {
                                return PhpType::Int;
                            }
                        } else {
                            return PhpType::Int;
                        }
                    }
                };
                if let Some(ci) = c.classes.get(&class_name) {
                    if let Some(msig) = ci.static_methods.get(method) {
                        return msig.return_type.clone();
                    }
                }
            }
            PhpType::Int
        }
        ExprKind::This => {
            if let Some(c) = ctx {
                if let Some(cn) = &c.current_class {
                    return PhpType::Object(cn.clone());
                }
            }
            PhpType::Object(String::new())
        }
        ExprKind::PtrCast { target_type, .. } => PhpType::Pointer(Some(target_type.clone())),
        _ => PhpType::Int,
    }
}
