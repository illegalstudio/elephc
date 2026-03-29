use std::collections::{HashMap, HashSet};

use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use super::stmt;
use crate::parser::ast::{ExprKind, StmtKind};
use crate::types::{ClassInfo, FunctionSig, PhpType};

#[allow(clippy::too_many_arguments)]
pub fn emit_function(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (crate::parser::ast::ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    let label = format!("_fn_{}", name);
    let epilogue_label = format!("_fn_{}_epilogue", name);
    emit_function_with_label(
        emitter, data, &label, &epilogue_label, sig, body,
        all_functions, constants, all_global_var_names, all_static_vars,
        classes,
    );
}

pub fn emit_closure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    sig: &FunctionSig,
    body: &[crate::parser::ast::Stmt],
    all_functions: &HashMap<String, FunctionSig>,
    constants: &HashMap<String, (crate::parser::ast::ExprKind, PhpType)>,
) {
    let epilogue_label = format!("{}_epilogue", label);
    let empty_globals = HashSet::new();
    let empty_statics = HashMap::new();
    emit_function_with_label(
        emitter, data, label, &epilogue_label, sig, body,
        all_functions, constants, &empty_globals, &empty_statics,
        None,
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
    constants: &HashMap<String, (crate::parser::ast::ExprKind, PhpType)>,
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
) {
    let empty_globals = HashSet::new();
    let empty_statics = HashMap::new();
    emit_function_with_label_and_class(
        emitter, data, label, epilogue_label, sig, body,
        all_functions, constants, &empty_globals, &empty_statics,
        Some((classes, class_name)),
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
    constants: &HashMap<String, (crate::parser::ast::ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    // Pass classes to regular functions so they can resolve Object types
    let class_ctx = classes.map(|c| (c, "" as &str));
    emit_function_with_label_and_class(
        emitter, data, label, epilogue_label, sig, body,
        all_functions, constants, all_global_var_names, all_static_vars,
        class_ctx,
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
    constants: &HashMap<String, (crate::parser::ast::ExprKind, PhpType)>,
    all_global_var_names: &HashSet<String>,
    all_static_vars: &HashMap<(String, String), PhpType>,
    class_context: Option<(&HashMap<String, ClassInfo>, &str)>,
) {

    let mut ctx = Context::new();
    ctx.return_label = Some(epilogue_label.to_string());
    ctx.functions = all_functions.clone();
    ctx.constants = constants.clone();
    ctx.all_global_var_names = all_global_var_names.clone();
    ctx.all_static_vars = all_static_vars.clone();
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
                _pty.clone(),
                HeapOwnership::borrowed_alias_for_type(_pty),
            );
        } else if pname == "this" {
            ctx.alloc_var(pname, _pty.clone());
            ctx.set_var_ownership(pname, HeapOwnership::borrowed_alias_for_type(_pty));
            ctx.disable_epilogue_cleanup(pname);
        } else {
            ctx.alloc_var(pname, _pty.clone());
            ctx.set_var_ownership(pname, HeapOwnership::local_owner_for_type(_pty));
        }
    }

    // Pre-allocate stack slots for params with defaults that aren't passed
    // (They'll be filled with default values at the call site or by the function prologue)

    collect_local_vars(body, &mut ctx, sig);
    mark_control_flow_epilogue_unsafe(body, &mut ctx, false);

    let vars_size = ctx.stack_offset;
    let frame_size = super::align16(vars_size + 16);

    // -- function prologue: set up stack frame --
    emitter.raw(".align 2");
    emitter.label(label);
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for locals
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16));  // save caller's frame ptr & return addr
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));         // set new frame pointer

    // -- save parameters from registers to local stack slots --
    // ARM64 ABI: int/bool/array args in x0-x7, float args in d0-d7
    // Strings use two consecutive int registers (ptr + len)
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for (i, (pname, pty)) in sig.params.iter().enumerate() {
        let is_ref = sig.ref_params.get(i).copied().unwrap_or(false);
        let var = ctx.variables.get(pname).expect("codegen bug: param was just allocated but not found in variables map");
        let offset = var.stack_offset;
        if is_ref {
            // Ref param: store the address (always comes in an integer register)
            emitter.comment(&format!("param &${} from x{} (ref)", pname, int_reg_idx));
            super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save address of referenced variable
            int_reg_idx += 1;
        } else {
            match pty {
                PhpType::Bool | PhpType::Int => {
                    emitter.comment(&format!("param ${} from x{}", pname, int_reg_idx));
                    super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save int/bool param
                    int_reg_idx += 1;
                }
                PhpType::Float => {
                    emitter.comment(&format!("param ${} from d{}", pname, float_reg_idx));
                    super::abi::store_at_offset(emitter, &format!("d{}", float_reg_idx), offset); // save float param
                    float_reg_idx += 1;
                }
                PhpType::Str => {
                    emitter.comment(&format!("param ${} from x{},x{}", pname, int_reg_idx, int_reg_idx + 1));
                    super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save string pointer
                    super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx + 1), offset - 8); // save string length
                    int_reg_idx += 2;
                }
                PhpType::Void => {}
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable
                | PhpType::Object(_) | PhpType::Pointer(_) => {
                    emitter.comment(&format!("param ${} from x{}", pname, int_reg_idx));
                    super::abi::store_at_offset(emitter, &format!("x{}", int_reg_idx), offset); // save array/callable/object/pointer param
                    int_reg_idx += 1;
                }
            }
        }
    }

    // -- zero-initialize local variables that may be deep-freed on reassignment --
    // Without this, the first free-on-reassign would see stale stack values
    // (left over from a previous function call at the same stack address)
    // and try to deep-free a random heap pointer.
    let param_names: std::collections::HashSet<String> =
        sig.params.iter().map(|(n, _)| n.clone()).collect();
    for (name, var) in &ctx.variables {
        if param_names.contains(name) {
            continue; // Parameters are initialized by register stores above
        }
        if matches!(&var.ty, PhpType::Str | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)) {
            super::abi::store_at_offset(emitter, "xzr", var.stack_offset);       // zero-init to prevent stale ptr free
        }
    }

    // -- emit function body statements --
    for s in body {
        stmt::emit_stmt(s, emitter, &mut ctx, data);
    }

    // -- function epilogue: save static vars back and restore/return --
    emitter.label(epilogue_label);
    preserve_return_registers(emitter, &sig.return_type);

    // Save static vars back to global storage before returning
    let func_name = label.strip_prefix("_fn_").unwrap_or(label);
    for static_var in &ctx.static_vars {
        let data_label = format!("_static_{}_{}", func_name, static_var);
        let var_info = ctx.variables.get(static_var);
        if let Some(var) = var_info {
            let offset = var.stack_offset;
            let ty = var.ty.clone();
            emitter.comment(&format!("save static ${} back", static_var));
            emitter.instruction(&format!("adrp x9, {}@PAGE", data_label));      // load page of static var storage
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", data_label)); //add page offset
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

    emit_owned_local_epilogue_cleanup(emitter, &ctx);

    restore_return_registers(emitter, &sig.return_type);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16));  // restore frame ptr & return addr
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();

    // -- emit any closures deferred during this function's body --
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
            );
        }
    }
}

fn preserve_return_registers(emitter: &mut Emitter, return_ty: &PhpType) {
    match return_ty {
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                         // preserve float return value across epilogue side effects
        }
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve string return registers across epilogue side effects
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve scalar/heap return value across epilogue side effects
        }
    }
}

fn restore_return_registers(emitter: &mut Emitter, return_ty: &PhpType) {
    match return_ty {
        PhpType::Float => {
            emitter.instruction("ldr d0, [sp], #16");                           // restore float return value after epilogue cleanup
        }
        PhpType::Str => {
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore string return registers after epilogue cleanup
        }
        _ => {
            emitter.instruction("ldr x0, [sp], #16");                           // restore scalar/heap return value after epilogue cleanup
        }
    }
}

fn emit_owned_local_epilogue_cleanup(emitter: &mut Emitter, ctx: &Context) {
    for (name, var) in &ctx.variables {
        if ctx.global_vars.contains(name) || ctx.static_vars.contains(name) || ctx.ref_params.contains(name) {
            continue;
        }
        if !var.epilogue_cleanup_safe || var.ownership != HeapOwnership::Owned {
            continue;
        }
        match &var.ty {
            PhpType::Str => {
                emitter.comment(&format!("epilogue cleanup ${}", name));
                super::abi::load_at_offset(emitter, "x0", var.stack_offset);     // load owned string pointer from local slot
                emitter.instruction("bl __rt_heap_free_safe");                  // release owned string storage before returning
            }
            ty if ty.is_refcounted() => {
                emitter.comment(&format!("epilogue cleanup ${}", name));
                super::abi::load_at_offset(emitter, "x0", var.stack_offset);     // load owned heap pointer from local slot
                super::abi::emit_decref_if_refcounted(emitter, ty);
            }
            _ => {}
        }
    }
}

fn mark_control_flow_epilogue_unsafe(
    stmts: &[crate::parser::ast::Stmt],
    ctx: &mut Context,
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
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                mark_control_flow_epilogue_unsafe(then_body, ctx, true);
                for (_, body) in elseif_clauses {
                    mark_control_flow_epilogue_unsafe(body, ctx, true);
                }
                if let Some(body) = else_body {
                    mark_control_flow_epilogue_unsafe(body, ctx, true);
                }
            }
            StmtKind::Foreach { body, key_var, value_var, .. } => {
                ctx.disable_epilogue_cleanup(value_var);
                if let Some(key_var) = key_var {
                    ctx.disable_epilogue_cleanup(key_var);
                }
                mark_control_flow_epilogue_unsafe(body, ctx, true);
            }
            StmtKind::DoWhile { body, .. } | StmtKind::While { body, .. } => {
                mark_control_flow_epilogue_unsafe(body, ctx, true);
            }
            StmtKind::For { init, update, body, .. } => {
                if let Some(stmt) = init {
                    mark_control_flow_epilogue_unsafe(&[*stmt.clone()], ctx, true);
                }
                if let Some(stmt) = update {
                    mark_control_flow_epilogue_unsafe(&[*stmt.clone()], ctx, true);
                }
                mark_control_flow_epilogue_unsafe(body, ctx, true);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    mark_control_flow_epilogue_unsafe(body, ctx, true);
                }
                if let Some(body) = default {
                    mark_control_flow_epilogue_unsafe(body, ctx, true);
                }
            }
            _ => {}
        }
    }
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
                    let ty = infer_local_type(value, sig, Some(ctx));
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
                    let ty = infer_local_type(init, sig, Some(ctx));
                    ctx.alloc_var(name, ty);
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_local_vars(then_body, ctx, sig);
                for (_, body) in elseif_clauses {
                    collect_local_vars(body, ctx, sig);
                }
                if let Some(body) = else_body {
                    collect_local_vars(body, ctx, sig);
                }
            }
            StmtKind::Foreach { value_var, body, array, key_var, .. } => {
                let arr_ty = infer_local_type(array, sig, Some(ctx));
                if let Some(k) = key_var {
                    if !ctx.variables.contains_key(k) {
                        // Assoc array keys are strings; indexed array keys are ints
                        let key_ty = if matches!(&arr_ty, PhpType::AssocArray { .. }) {
                            PhpType::Str
                        } else {
                            PhpType::Int
                        };
                        ctx.alloc_var(k, key_ty);
                    }
                }
                if !ctx.variables.contains_key(value_var) {
                    let elem_ty = match &arr_ty {
                        PhpType::Array(t) => *t.clone(),
                        PhpType::AssocArray { value, .. } => *value.clone(),
                        _ => PhpType::Int,
                    };
                    ctx.alloc_var(value_var, elem_ty);
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
                        ctx.alloc_var(var, elem_ty.clone());
                    }
                }
            }
            StmtKind::ArrayAssign { .. } | StmtKind::ArrayPush { .. } | StmtKind::ClassDecl { .. } => {}
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
            StmtKind::For { init, update, body, .. } => {
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

/// Public wrapper for infer_local_type, used by closure return type inference.
pub fn infer_local_type_pub(
    expr: &crate::parser::ast::Expr,
    sig: &FunctionSig,
) -> PhpType {
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
pub fn infer_contextual_type(
    expr: &crate::parser::ast::Expr,
    ctx: &Context,
) -> PhpType {
    let empty_sig = FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Void,
        ref_params: Vec::new(),
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
            PhpType::Array(t) => *t,
            PhpType::AssocArray { value, .. } => *value,
            _ => PhpType::Int,
        },
        ExprKind::Negate(inner) => {
            let inner_ty = infer_local_type(inner, sig, ctx);
            if inner_ty == PhpType::Float { PhpType::Float } else { PhpType::Int }
        }
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::BitNot(_) => PhpType::Int,
        ExprKind::NullCoalesce { value, default } => {
            let left = infer_local_type(value, sig, ctx);
            let right = infer_local_type(default, sig, ctx);
            wider_of(&left, &right)
        }
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            let then_ty = infer_local_type(then_expr, sig, ctx);
            let else_ty = infer_local_type(else_expr, sig, ctx);
            wider_of(&then_ty, &else_ty)
        }
        ExprKind::BinaryOp { left, op, right } => {
            use crate::parser::ast::BinOp;
            match op {
                BinOp::Concat => PhpType::Str,
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt
                | BinOp::LtEq | BinOp::GtEq | BinOp::StrictEq
                | BinOp::StrictNotEq | BinOp::And | BinOp::Or => PhpType::Bool,
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor
                | BinOp::ShiftLeft | BinOp::ShiftRight | BinOp::Spaceship => PhpType::Int,
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
                "strtolower" | "strtoupper" | "ucfirst" | "lcfirst" | "ucwords"
                | "trim" | "ltrim" | "rtrim" | "substr" | "str_repeat" | "strrev"
                | "str_replace" | "str_ireplace" | "substr_replace" | "str_pad"
                | "chr" | "implode" | "join" | "sprintf" | "number_format"
                | "nl2br" | "wordwrap" | "addslashes" | "stripslashes"
                | "htmlspecialchars" | "html_entity_decode" | "htmlentities"
                | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode"
                | "base64_encode" | "base64_decode" | "bin2hex" | "hex2bin"
                | "md5" | "sha1" | "hash" | "gettype" | "strstr"
                | "readline" | "date" | "json_encode" | "php_uname" | "phpversion"
                | "file_get_contents" | "tempnam" | "getcwd"
                | "shell_exec" => PhpType::Str,
                // Array-returning builtins
                "explode" | "str_split" | "file" | "scandir" | "glob"
                | "array_keys" | "array_values" | "array_merge" | "array_slice"
                | "array_reverse" | "array_unique" | "array_chunk" | "array_pad"
                | "array_fill" | "array_fill_keys" | "array_diff" | "array_intersect"
                | "array_diff_key" | "array_intersect_key" | "array_flip"
                | "array_combine" | "array_splice" | "array_column"
                | "array_map" | "array_filter" | "range" | "array_rand"
                | "sscanf" | "fgetcsv" | "preg_split" => {
                    // Try to infer element type from arguments
                    if name == "explode" || name == "str_split" || name == "file"
                        || name == "scandir" || name == "glob" || name == "fgetcsv"
                        || name == "preg_split"
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
                "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow"
                | "fmod" | "fdiv" | "microtime"
                | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
                | "sinh" | "cosh" | "tanh" | "log" | "log2" | "log10" | "exp"
                | "hypot" | "pi" | "deg2rad" | "rad2deg" => PhpType::Float,
                // Bool-returning builtins
                "is_int" | "is_float" | "is_string" | "is_bool" | "is_null"
                | "is_numeric" | "is_nan" | "is_finite" | "is_infinite"
                | "is_array" | "empty" | "isset" | "is_file" | "is_dir"
                | "is_readable" | "is_writable" | "file_exists"
                | "in_array" | "array_key_exists" | "str_contains"
                | "str_starts_with" | "str_ends_with" | "ctype_alpha"
                | "ctype_digit" | "ctype_alnum" | "ctype_space"
                | "function_exists" | "ptr_is_null" => PhpType::Bool,
                "abs" => {
                    if !args.is_empty() {
                        let t = infer_local_type(&args[0], sig, ctx);
                        if t == PhpType::Float { PhpType::Float } else { PhpType::Int }
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
                        if let Some(fn_sig) = c.functions.get(name) {
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
        ExprKind::Spread(inner) => infer_local_type(inner, sig, ctx),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.clone()),
        ExprKind::PropertyAccess { object, property } => {
            if let Some(c) = ctx {
                let obj_ty = infer_local_type(object, sig, Some(c));
                if let PhpType::Object(cn) = &obj_ty {
                    if let Some(ci) = c.classes.get(cn) {
                        if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == property) {
                            return ty.clone();
                        }
                    }
                }
                if let PhpType::Pointer(Some(cn)) = &obj_ty {
                    if let Some(ci) = c.extern_classes.get(cn) {
                        if let Some(field) = ci.fields.iter().find(|field| field.name == *property) {
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
        ExprKind::StaticMethodCall { class_name, method, .. } => {
            if let Some(c) = ctx {
                if let Some(ci) = c.classes.get(class_name) {
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
