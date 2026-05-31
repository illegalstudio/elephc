//! Purpose:
//! Dispatches statement AST nodes into focused lowering modules for assignments, arrays, control flow, IO, and includes.
//! Owns statement-level cleanup and fallthrough behavior for generated program bodies.
//!
//! Called from:
//! - `crate::codegen::main_emission` and `crate::codegen::functions`
//!
//! Key details:
//! - Statements must preserve PHP source order while maintaining local ownership and loop/try context state.

mod assignments;
mod arrays;
mod control_flow;
/// helpers
pub(crate) mod helpers;
mod includes;
mod io;
mod null_coalesce_assign;
mod storage;

use super::abi;
use super::callable_descriptor;
use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::{emit_expr, expr_result_heap_ownership};
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::PhpType;

pub(crate) use null_coalesce_assign::{
    emit_branch_if_result_non_null,
    null_coalesce_array_target,
    null_coalesce_property_array_target,
    null_coalesce_property_target,
    null_coalesce_static_property_array_target,
    null_coalesce_static_property_target,
};
pub(crate) use arrays::{emit_array_assign_stmt, emit_nested_array_assign_stmt};
pub(crate) use assignments::{
    emit_assign_stmt,
    emit_dynamic_property_get,
    emit_property_array_assign_stmt,
    emit_property_assign_stmt,
    emit_static_property_array_assign_stmt,
    emit_static_property_assign_stmt,
};
pub(crate) use io::emit_expr_to_stdout;
pub(crate) use control_flow::{
    emit_iterable_object_loop, emit_iterator_loop, reload_iterator_receiver, IteratorDispatchTarget,
};

/// Extracts the user-facing function name from the context's return label by stripping
/// the internal `_fn_` prefix and `_epilogue` suffix. Returns "main" if no label is set.
fn current_function_name(ctx: &Context) -> String {
    ctx.return_label
        .as_ref()
        .map(|l| l.strip_prefix("_fn_").unwrap_or(l))
        .map(|l| l.strip_suffix("_epilogue").unwrap_or(l))
        .unwrap_or("main")
        .to_string()
}

/// Builds a static storage label by combining the current function name with the variable name.
fn static_storage_label(ctx: &Context, name: &str) -> String {
    format!("_static_{}_{}", current_function_name(ctx), name)
}

/// Returns whether pre-scanned runtime data will emit `data_label`.
fn prescanned_static_storage_exists(ctx: &Context, name: &str, data_label: &str) -> bool {
    ctx.all_static_vars
        .keys()
        .any(|(func_name, var_name)| {
            var_name == name
                && format!("_static_{}_{}", crate::names::mangle_fqn(func_name), var_name)
                    == data_label
        })
}

/// Declares static-local storage for function-like scopes that were not pre-scanned.
fn ensure_static_storage_symbols(
    data: &mut DataSection,
    ctx: &Context,
    name: &str,
    data_label: &str,
    init_label: &str,
) {
    if prescanned_static_storage_exists(ctx, name, data_label) {
        return;
    }
    data.add_comm(data_label.to_string(), 16);
    data.add_comm(init_label.to_string(), 8);
}

/// Emits a static variable store operation, delegating to the storage module.
fn emit_static_store(emitter: &mut Emitter, ctx: &Context, name: &str, ty: &PhpType) {
    storage::emit_static_store(emitter, ctx, name, ty);
}

/// Emits code for a single statement AST node.
pub fn emit_stmt(stmt: &Stmt, emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    if stmt.span.line > 0 {
        emitter.comment(&format!(
            "@src line={} col={}",
            stmt.span.line, stmt.span.col
        ));
    }

    // -- reset concat buffer at the start of each statement --
    // This is safe because any string that needs to persist beyond the current
    // statement is copied to heap via __rt_str_persist (in emit_store).
    crate::codegen::abi::emit_store_zero_to_symbol(emitter, "_concat_off", 0);

    match &stmt.kind {
        StmtKind::Synthetic(stmts) => {
            for stmt in stmts {
                emit_stmt(stmt, emitter, ctx, data);
            }
        }
        StmtKind::IncludeOnceMark { label } => {
            includes::emit_include_once_mark(label, emitter, data);
        }
        StmtKind::FunctionVariantGroup { .. } => {}
        StmtKind::FunctionVariantMark { name, variant } => {
            includes::emit_function_variant_mark(name, variant, emitter, data);
        }
        StmtKind::IncludeOnceGuard { label, body } => {
            includes::emit_include_once_guard(label, body, emitter, ctx, data);
        }
        StmtKind::IfDef { .. } => {
            emitter.comment("WARNING: unresolved ifdef reached codegen");
        }
        StmtKind::NamespaceDecl { .. }
        | StmtKind::NamespaceBlock { .. }
        | StmtKind::UseDecl { .. } => {
            emitter.comment("WARNING: unresolved namespace/use reached codegen");
        }
        StmtKind::EnumDecl { .. } => {}
        StmtKind::Echo(expr) => {
            io::emit_echo_stmt(expr, emitter, ctx, data);
        }
        StmtKind::Assign { name, value } => {
            assignments::emit_assign_stmt(name, value, emitter, ctx, data);
        }
        StmtKind::RefAssign { target, source } => {
            assignments::emit_ref_assign_stmt(target, source, emitter, ctx);
        }
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            assignments::emit_assign_stmt(name, value, emitter, ctx, data);
            let static_ty = super::functions::codegen_static_type(type_expr, ctx);
            let ty = super::functions::codegen_declared_type(type_expr, ctx).codegen_repr();
            ctx.update_var_type_static_and_ownership(
                name,
                ty.clone(),
                static_ty,
                helpers::local_slot_ownership_after_store(&ty),
            );
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            control_flow::emit_if_stmt(
                condition,
                then_body,
                elseif_clauses,
                else_body,
                emitter,
                ctx,
                data,
            );
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            arrays::emit_array_assign_stmt(array, index, value, emitter, ctx, data);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            arrays::emit_nested_array_assign_stmt(target, value, emitter, ctx, data);
        }
        StmtKind::ArrayPush { array, value } => {
            arrays::emit_array_push_stmt(array, value, emitter, ctx, data);
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            control_flow::emit_foreach_stmt(
                array,
                key_var,
                value_var,
                *value_by_ref,
                body,
                stmt.span,
                emitter,
                ctx,
                data,
            );
        }
        StmtKind::DoWhile { body, condition } => {
            control_flow::emit_do_while_stmt(body, condition, emitter, ctx, data);
        }
        StmtKind::While { condition, body } => {
            control_flow::emit_while_stmt(condition, body, emitter, ctx, data);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            control_flow::emit_for_stmt(init, condition, update, body, emitter, ctx, data);
        }
        StmtKind::Throw(expr) => {
            control_flow::emit_throw_stmt(expr, emitter, ctx, data);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            control_flow::emit_try_stmt(try_body, catches, finally_body, emitter, ctx, data);
        }
        StmtKind::Break(levels) => {
            control_flow::emit_break_stmt(*levels, emitter, ctx);
        }
        StmtKind::FunctionDecl { .. } => {
            // Emitted separately in codegen/mod.rs
        }
        StmtKind::PackedClassDecl { .. } => {
            // Packed classes only contribute static layout metadata.
        }
        StmtKind::Return(expr) => {
            control_flow::emit_return_stmt(expr, emitter, ctx, data);
        }
        StmtKind::ExprStmt(expr) => {
            emitter.blank();
            let ty = emit_expr(expr, emitter, ctx, data);
            release_discarded_expr_result(expr, &ty, emitter);
        }
        StmtKind::Continue(levels) => {
            control_flow::emit_continue_stmt(*levels, emitter, ctx);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            control_flow::emit_switch_stmt(subject, cases, default, emitter, ctx, data);
        }
        StmtKind::ConstDecl { name, value } => {
            // Store constant value in context for later ConstRef resolution
            let ty = match &value.kind {
                crate::parser::ast::ExprKind::IntLiteral(_) => PhpType::Int,
                crate::parser::ast::ExprKind::FloatLiteral(_) => PhpType::Float,
                crate::parser::ast::ExprKind::StringLiteral(_) => PhpType::Str,
                crate::parser::ast::ExprKind::BoolLiteral(_) => PhpType::Bool,
                crate::parser::ast::ExprKind::Null => PhpType::Void,
                _ => PhpType::Int,
            };
            ctx.constants.entry(name.clone()).or_insert((value.kind.clone(), ty));
        }
        StmtKind::ListUnpack { vars, value } => {
            arrays::emit_list_unpack_stmt(vars, value, emitter, ctx, data);
        }
        StmtKind::Global { vars } => {
            emitter.blank();
            emitter.comment("global declaration");
            for var in vars {
                ctx.global_vars.insert(var.clone());
                // Load current value from global storage into local var slot
                let var_info = match ctx.variables.get(var) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!(
                            "WARNING: global variable ${} not pre-allocated",
                            var
                        ));
                        continue;
                    }
                };
                let offset = var_info.stack_offset;
                let ty = var_info.ty.clone();
                emit_global_load(emitter, ctx, var, &ty);
                abi::emit_store(emitter, &ty, offset);
                ctx.update_var_type_and_ownership(
                    var,
                    ty.clone(),
                    HeapOwnership::borrowed_alias_for_type(&ty),
                );
            }
        }
        StmtKind::StaticVar { name, init } => {
            emitter.blank();
            emitter.comment(&format!("static ${}", name));
            let data_label = static_storage_label(ctx, name);
            let init_label = format!("{}_init", data_label);
            ensure_static_storage_symbols(data, ctx, name, &data_label, &init_label);
            let skip_label = ctx.next_label("static_skip");

            // -- check if already initialized --
            helpers::emit_static_init_guard(emitter, &init_label, &skip_label);

            // -- first call: evaluate init expression and store --
            let ty = emit_expr(init, emitter, ctx, data);
            helpers::retain_borrowed_heap_result(emitter, init, &ty);
            // Store init value to static storage
            abi::emit_store_result_to_symbol(emitter, &data_label, &ty, false);
            emitter.label(&skip_label);

            // -- load current value from static storage into local variable --
            let var_info = match ctx.variables.get(name) {
                Some(v) => v,
                None => {
                    emitter.comment(&format!(
                        "WARNING: static variable ${} not pre-allocated",
                        name
                    ));
                    return;
                }
            };
            let offset = var_info.stack_offset;
            let var_ty = var_info.ty.clone();
            abi::emit_load_symbol_to_local_slot(emitter, &data_label, &var_ty, offset);
            ctx.update_var_type_and_ownership(
                name,
                var_ty.clone(),
                HeapOwnership::borrowed_alias_for_type(&var_ty),
            );

            // Mark this variable as static so epilogue saves it back
            ctx.static_vars.insert(name.clone());
        }
        StmtKind::Include { .. } => {
            // Should have been resolved before codegen
            panic!("Unresolved include statement in codegen");
        }
        // Declarations are emitted or recorded during pre-scan, so runtime statement lowering skips them.
        StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. } => {} // already emitted in pre-scan
        StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {} // extern decls processed at compile time
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            assignments::emit_property_assign_stmt(object, property, value, emitter, ctx, data);
        }
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => {
            assignments::emit_static_property_assign_stmt(
                receiver, property, value, emitter, ctx, data,
            );
        }
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => {
            assignments::emit_static_property_array_push_stmt(
                receiver, property, value, emitter, ctx, data,
            );
        }
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => {
            assignments::emit_static_property_array_assign_stmt(
                receiver,
                property,
                index,
                value,
                emitter,
                ctx,
                data,
            );
        }
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => {
            assignments::emit_property_array_push_stmt(object, property, value, emitter, ctx, data);
        }
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            assignments::emit_property_array_assign_stmt(
                object,
                property,
                index,
                value,
                emitter,
                ctx,
                data,
            );
        }
    }
}

/// Releases heap ownership for an expression result that is discarded (not stored).
/// For owned strings, moves the pointer to `__rt_heap_free_safe`; for other refcounted
/// types, emits a conditional decref. Non-refcounted or borrowed values are no-ops.
fn release_discarded_expr_result(
    expr: &crate::parser::ast::Expr,
    ty: &PhpType,
    emitter: &mut Emitter,
) {
    if expr_result_heap_ownership(expr) != HeapOwnership::Owned {
        return;
    }
    if matches!(ty, PhpType::Str) {
        let (ptr_reg, _) = abi::string_result_regs(emitter);
        let result_reg = abi::int_result_reg(emitter);
        if ptr_reg != result_reg {
            emitter.instruction(&format!("mov {}, {}", result_reg, ptr_reg));   // pass discarded owned string pointer to heap-free helper
        }
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
    } else if matches!(ty, PhpType::Callable) {
        callable_descriptor::emit_release_current_descriptor(emitter);
    } else if ty.is_refcounted() {
        abi::emit_decref_if_refcounted(emitter, ty);
    }
}

/// Store a value to global variable storage (_gvar_NAME).
fn emit_global_store(emitter: &mut Emitter, ctx: &mut Context, name: &str, ty: &PhpType) {
    storage::emit_global_store(emitter, ctx, name, ty);
}

/// Load a value from global variable storage (_gvar_NAME) into result registers.
pub fn emit_global_load(emitter: &mut Emitter, ctx: &mut Context, name: &str, ty: &PhpType) {
    storage::emit_global_load(emitter, ctx, name, ty);
}

/// Emits a store to an extern global variable, writing the value from the current stack slot.
fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    storage::emit_extern_global_store(emitter, name, ty);
}
