mod assignments;
mod arrays;
mod control_flow;
mod io;
mod storage;

use super::abi;
use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::{emit_expr, expr_result_heap_ownership};
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::PhpType;

fn retain_borrowed_heap_result(
    emitter: &mut Emitter,
    expr: &crate::parser::ast::Expr,
    ty: &PhpType,
) {
    if ty.is_refcounted() && expr_result_heap_ownership(expr) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, ty);
    }
}

fn local_slot_ownership_after_store(ty: &PhpType) -> HeapOwnership {
    HeapOwnership::local_owner_for_type(ty)
}

fn stamp_indexed_array_value_type(emitter: &mut Emitter, array_reg: &str, elem_ty: &PhpType) {
    let value_type_tag = match elem_ty {
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        _ => return,
    };
    emitter.instruction(&format!("ldr x12, [{}, #-8]", array_reg));             // load the packed array kind word from the heap header
    emitter.instruction("mov x14, #0x80ff");                                    // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x12, x12, x14");                                   // keep only the persistent indexed-array metadata bits
    emitter.instruction(&format!("mov x13, #{}", value_type_tag));              // materialize the runtime array value_type tag
    emitter.instruction("lsl x13, x13, #8");                                    // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("orr x12, x12, x13");                                   // combine the heap kind with the array value_type tag
    emitter.instruction(&format!("str x12, [{}, #-8]", array_reg));             // persist the packed array kind word in the heap header
}

fn release_owned_slot(emitter: &mut Emitter, ty: &PhpType, offset: usize, preserve_x0: bool) {
    if matches!(ty, PhpType::Str) {
        if preserve_x0 {
            emitter.instruction("mov x8, x0");                                  // preserve incoming value while old string is released
        }
        abi::load_at_offset(emitter, "x0", offset); // load previous string pointer from stack slot
        emitter.instruction("bl __rt_heap_free_safe");                          // release previous string storage if it lives on the heap
        if preserve_x0 {
            emitter.instruction("mov x0, x8");                                  // restore incoming value after string release
        }
    } else if ty.is_refcounted() {
        if preserve_x0 {
            emitter.instruction("mov x8, x0");                                  // preserve incoming value while old heap slot is decreffed
        }
        abi::load_at_offset(emitter, "x0", offset); // load previous heap pointer from stack slot
        abi::emit_decref_if_refcounted(emitter, ty);
        if preserve_x0 {
            emitter.instruction("mov x0, x8");                                  // restore incoming value after decref
        }
    }
}

fn current_function_name(ctx: &Context) -> String {
    ctx.return_label
        .as_ref()
        .map(|l| l.strip_prefix("_fn_").unwrap_or(l))
        .map(|l| l.strip_suffix("_epilogue").unwrap_or(l))
        .unwrap_or("main")
        .to_string()
}

fn static_storage_label(ctx: &Context, name: &str) -> String {
    format!("_static_{}_{}", current_function_name(ctx), name)
}

fn emit_static_store(emitter: &mut Emitter, ctx: &Context, name: &str, ty: &PhpType) {
    storage::emit_static_store(emitter, ctx, name, ty);
}

pub fn emit_stmt(stmt: &Stmt, emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    // -- reset concat buffer at the start of each statement --
    // This is safe because any string that needs to persist beyond the current
    // statement is copied to heap via __rt_str_persist (in emit_store).
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str xzr, [x9]");                                       // reset concat buffer offset to 0

    match &stmt.kind {
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
        StmtKind::TypedAssign {
            type_expr: _,
            name,
            value,
        } => {
            assignments::emit_assign_stmt(name, value, emitter, ctx, data);
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
        StmtKind::ArrayPush { array, value } => {
            arrays::emit_array_push_stmt(array, value, emitter, ctx, data);
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => {
            control_flow::emit_foreach_stmt(array, key_var, value_var, body, emitter, ctx, data);
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
        StmtKind::Break => {
            control_flow::emit_break_stmt(emitter, ctx);
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
            emit_expr(expr, emitter, ctx, data);
            // result discarded
        }
        StmtKind::Continue => {
            control_flow::emit_continue_stmt(emitter, ctx);
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
            ctx.constants.insert(name.clone(), (value.kind.clone(), ty));
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
            let func_name = current_function_name(ctx);
            let init_label = format!("_static_{}_{}_init", func_name, name);
            let data_label = format!("_static_{}_{}", func_name, name);
            let skip_label = ctx.next_label("static_skip");

            // -- check if already initialized --
            emitter.adrp("x9", &format!("{}", init_label));      // load page of init flag
            emitter.add_lo12("x9", "x9", &format!("{}", init_label)); //add page offset
            emitter.instruction("ldr x10, [x9]");                               // load init flag value
            emitter.instruction(&format!("cbnz x10, {}", skip_label));          // skip init if already done

            // -- first call: evaluate init expression and store --
            emitter.instruction("mov x10, #1");                                 // set init flag to 1
            emitter.instruction("str x10, [x9]");                               // write init flag
            let ty = emit_expr(init, emitter, ctx, data);
            retain_borrowed_heap_result(emitter, init, &ty);
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
        // OOP stubs — not yet implemented, skip
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

fn emit_extern_global_store(emitter: &mut Emitter, name: &str, ty: &PhpType) {
    storage::emit_extern_global_store(emitter, name, ty);
}
