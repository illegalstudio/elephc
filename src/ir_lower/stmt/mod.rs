//! Purpose:
//! Dispatches AST statement nodes into EIR instructions and CFG terminators.
//!
//! Called from:
//! - `crate::ir_lower::function` for main, user functions, and methods.
//!
//! Key details:
//! - Every `StmtKind` variant has an explicit lowering branch.
//! - Structured control flow creates EIR blocks; complex PHP runtime behavior
//!   uses high-level opcodes with conservative effects.

use crate::ir::{BlockId, Immediate, IrType, LocalKind, Op, Ownership, SwitchCase, Terminator};
use crate::ir_lower::context::{
    type_expr_to_php_type, LoopFrame, LoweredValue, LoweringContext,
};
use crate::ir_lower::effects_lookup;
use crate::ir_lower::expr::lower_expr;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::span::Span;
use crate::types::PhpType;

/// Lowers one AST statement into the current EIR insertion block.
pub(crate) fn lower_stmt(ctx: &mut LoweringContext<'_, '_>, stmt: &Stmt) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    match &stmt.kind {
        StmtKind::Echo(expr) => lower_echo(ctx, expr, stmt.span),
        StmtKind::Assign { name, value } => lower_assign(ctx, name, value, stmt.span),
        StmtKind::RefAssign { target, source } => lower_ref_assign(ctx, target, source, stmt.span),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => lower_if(ctx, condition, then_body, elseif_clauses, else_body.as_deref(), stmt.span),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => lower_ifdef(ctx, symbol, then_body, else_body.as_deref(), stmt.span),
        StmtKind::While { condition, body } => lower_while(ctx, condition, body),
        StmtKind::DoWhile { body, condition } => lower_do_while(ctx, body, condition),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => lower_for(ctx, init.as_deref(), condition.as_ref(), update.as_deref(), body),
        StmtKind::ArrayAssign { array, index, value } => {
            lower_array_assign(ctx, array, index, value, stmt.span);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            lower_nested_array_assign(ctx, target, value, stmt.span);
        }
        StmtKind::ArrayPush { array, value } => lower_array_push(ctx, array, value, stmt.span),
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => lower_typed_assign(ctx, type_expr, name, value, stmt.span),
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => lower_foreach(ctx, array, key_var.as_deref(), value_var, *value_by_ref, body),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => lower_switch(ctx, subject, cases, default.as_deref()),
        StmtKind::Include {
            path,
            once,
            required,
        } => lower_include(ctx, path, *once, *required, stmt.span),
        StmtKind::IncludeOnceMark { label } => lower_include_once_mark(ctx, label, stmt.span),
        StmtKind::IncludeOnceGuard { label, body } => {
            lower_include_once_guard(ctx, label, body, stmt.span);
        }
        StmtKind::Throw(expr) => lower_throw(ctx, expr),
        StmtKind::Synthetic(body) => lower_block(ctx, body),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => lower_try(ctx, try_body, catches, finally_body.as_deref(), stmt.span),
        StmtKind::Break(level) => lower_break(ctx, *level),
        StmtKind::Continue(level) => lower_continue(ctx, *level),
        StmtKind::ExprStmt(expr) => {
            lower_expr(ctx, expr);
        }
        StmtKind::NamespaceDecl { name: _ } => lower_noop(ctx, stmt.span),
        StmtKind::NamespaceBlock { name: _, body } => lower_block(ctx, body),
        StmtKind::UseDecl { imports: _ } => lower_noop(ctx, stmt.span),
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => lower_noop(ctx, stmt.span),
        StmtKind::FunctionVariantGroup { name, variants } => {
            lower_function_variant_group(ctx, name, variants, stmt.span);
        }
        StmtKind::FunctionVariantMark { name, variant } => {
            lower_function_variant_mark(ctx, name, variant, stmt.span);
        }
        StmtKind::Return(value) => lower_return(ctx, value.as_ref(), stmt.span),
        StmtKind::ConstDecl { name, value } => lower_const_decl(ctx, name, value, stmt.span),
        StmtKind::ListUnpack { vars, value } => lower_list_unpack(ctx, vars, value, stmt.span),
        StmtKind::Global { vars } => lower_global(ctx, vars),
        StmtKind::StaticVar { name, init } => lower_static_var(ctx, name, init, stmt.span),
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => lower_property_assign(ctx, object, property, value, stmt.span),
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => lower_static_property_assign(ctx, receiver, property, value, stmt.span),
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => lower_static_property_array_push(ctx, receiver, property, value, stmt.span),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => lower_static_property_array_assign(ctx, receiver, property, index, value, stmt.span),
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => lower_property_array_push(ctx, object, property, value, stmt.span),
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => lower_property_array_assign(ctx, object, property, index, value, stmt.span),
    }
}

/// Lowers a sequence of statements until the current block terminates.
fn lower_block(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt]) {
    for stmt in body {
        lower_stmt(ctx, stmt);
        if ctx.builder.insertion_block_is_terminated() {
            break;
        }
    }
}

/// Emits EIR for `echo`.
fn lower_echo(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span) {
    let value = lower_expr(ctx, expr);
    ctx.emit_void(
        Op::EchoValue,
        vec![value.value],
        None,
        Op::EchoValue.default_effects(),
        Some(span),
    );
}

/// Lowers a plain PHP local assignment.
fn lower_assign(ctx: &mut LoweringContext<'_, '_>, name: &str, value: &Expr, span: Span) {
    let lowered = lower_expr(ctx, value);
    let php_type = ctx.builder.value_php_type(lowered.value);
    ctx.store_local(name, lowered, php_type, Some(span));
}

/// Lowers a by-reference assignment as a conservative local rebinding.
fn lower_ref_assign(ctx: &mut LoweringContext<'_, '_>, target: &str, source: &str, span: Span) {
    let value = ctx.load_local(source, Some(span));
    let php_type = ctx.builder.value_php_type(value.value);
    ctx.store_local(target, value, php_type, Some(span));
}

/// Lowers an `if` / `elseif` / `else` chain and terminates unreachable merge blocks explicitly.
fn lower_if(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    span: Span,
) {
    let merge = ctx.builder.create_named_block("if.merge", Vec::new());
    let merge_reachable =
        lower_if_chain(ctx, condition, then_body, elseif_clauses, else_body, merge, span);
    ctx.builder.position_at_end(merge);
    if !merge_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
}

/// Recursively emits one condition node in an `if` chain and reports whether the merge is reachable.
fn lower_if_chain(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    merge: BlockId,
    span: Span,
) -> bool {
    let cond_value = lower_expr(ctx, condition);
    let cond_value = ctx.truthy(cond_value, Some(condition.span));
    let then_block = ctx.builder.create_named_block("if.then", Vec::new());
    let else_block = ctx.builder.create_named_block("if.else", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond_value.value,
        then_target: then_block,
        then_args: Vec::new(),
        else_target: else_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(then_block);
    lower_block(ctx, then_body);
    let mut merge_reachable = false;
    if !ctx.builder.insertion_block_is_terminated() {
        merge_reachable = true;
        branch_to(ctx, merge);
    }

    ctx.builder.position_at_end(else_block);
    if let Some(((next_condition, next_body), rest)) = elseif_clauses.split_first() {
        merge_reachable |=
            lower_if_chain(ctx, next_condition, next_body, rest, else_body, merge, span);
    } else if let Some(else_body) = else_body {
        lower_block(ctx, else_body);
        if !ctx.builder.insertion_block_is_terminated() {
            merge_reachable = true;
            branch_to(ctx, merge);
        }
    } else {
        lower_noop(ctx, span);
        if !ctx.builder.insertion_block_is_terminated() {
            merge_reachable = true;
            branch_to(ctx, merge);
        }
    }
    merge_reachable
}

/// Lowers a residual `ifdef`; normally the conditional pass removes these first.
fn lower_ifdef(
    ctx: &mut LoweringContext<'_, '_>,
    _symbol: &str,
    then_body: &[Stmt],
    else_body: Option<&[Stmt]>,
    _span: Span,
) {
    if !then_body.is_empty() {
        lower_block(ctx, then_body);
    } else if let Some(else_body) = else_body {
        lower_block(ctx, else_body);
    }
}

/// Lowers a `while` loop.
fn lower_while(ctx: &mut LoweringContext<'_, '_>, condition: &Expr, body: &[Stmt]) {
    let header = ctx.builder.create_named_block("while.cond", Vec::new());
    let body_block = ctx.builder.create_named_block("while.body", Vec::new());
    let exit = ctx.builder.create_named_block("while.exit", Vec::new());
    branch_to(ctx, header);

    ctx.builder.position_at_end(header);
    let cond = lower_expr(ctx, condition);
    let cond = ctx.truthy(cond, Some(condition.span));
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame { break_block: exit, continue_block: header });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
}

/// Lowers a `do while` loop.
fn lower_do_while(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt], condition: &Expr) {
    let body_block = ctx.builder.create_named_block("do.body", Vec::new());
    let cond_block = ctx.builder.create_named_block("do.cond", Vec::new());
    let exit = ctx.builder.create_named_block("do.exit", Vec::new());
    branch_to(ctx, body_block);

    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame { break_block: exit, continue_block: cond_block });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, cond_block);

    ctx.builder.position_at_end(cond_block);
    let cond = lower_expr(ctx, condition);
    let cond = ctx.truthy(cond, Some(condition.span));
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(exit);
}

/// Lowers a `for` loop.
fn lower_for(
    ctx: &mut LoweringContext<'_, '_>,
    init: Option<&Stmt>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
) {
    if let Some(init) = init {
        lower_stmt(ctx, init);
    }
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }

    let header = ctx.builder.create_named_block("for.cond", Vec::new());
    let body_block = ctx.builder.create_named_block("for.body", Vec::new());
    let update_block = ctx.builder.create_named_block("for.update", Vec::new());
    let exit = ctx.builder.create_named_block("for.exit", Vec::new());
    branch_to(ctx, header);

    ctx.builder.position_at_end(header);
    let cond = if let Some(condition) = condition {
        let cond = lower_expr(ctx, condition);
        ctx.truthy(cond, Some(condition.span))
    } else {
        emit_const_bool(ctx, true, None)
    };
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame { break_block: exit, continue_block: update_block });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, update_block);

    ctx.builder.position_at_end(update_block);
    if let Some(update) = update {
        lower_stmt(ctx, update);
    }
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
}

/// Lowers an indexed array assignment.
fn lower_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let array_value = ctx.load_local(array, Some(span));
    let index = lower_expr(ctx, index);
    let value = lower_expr(ctx, value);
    let op = array_set_op(array_value.ir_type);
    ctx.emit_void(op, vec![array_value.value, index.value, value.value], None, op.default_effects(), Some(span));
}

/// Lowers a nested array assignment that already carries an expression target.
fn lower_nested_array_assign(ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, span: Span) {
    let target = lower_expr(ctx, target);
    let value = lower_expr(ctx, value);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![target.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `$array[] = value`.
fn lower_array_push(ctx: &mut LoweringContext<'_, '_>, array: &str, value: &Expr, span: Span) {
    let array_value = ctx.load_local(array, Some(span));
    let value = lower_expr(ctx, value);
    let op = if array_value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Array) {
        Op::ArrayPush
    } else {
        Op::RuntimeCall
    };
    ctx.emit_void(op, vec![array_value.value, value.value], None, op.default_effects(), Some(span));
    if op == Op::ArrayPush {
        let current_ty = ctx.builder.value_php_type(array_value.value);
        let value_ty = ctx.builder.value_php_type(value.value);
        if let Some(updated_ty) = array_push_updated_type(current_ty, value_ty) {
            ctx.set_local_type(array, updated_ty);
        }
    }
}

/// Returns the refined array type after appending to a statically empty indexed array.
fn array_push_updated_type(current_ty: PhpType, value_ty: PhpType) -> Option<PhpType> {
    match current_ty.codegen_repr() {
        PhpType::Array(elem_ty) if is_empty_indexed_array_element(elem_ty.as_ref()) => {
            Some(PhpType::Array(Box::new(normalize_materialized_element_type(value_ty))))
        }
        _ => None,
    }
}

/// Returns true for the placeholder element type used by empty indexed arrays.
fn is_empty_indexed_array_element(elem_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Void)
}

/// Lowers an assignment with a declared type.
fn lower_typed_assign(
    ctx: &mut LoweringContext<'_, '_>,
    type_expr: &crate::parser::ast::TypeExpr,
    name: &str,
    value: &Expr,
    span: Span,
) {
    let php_type = type_expr_to_php_type(type_expr);
    let lowered = lower_expr(ctx, value);
    ctx.declare_local(name, php_type.clone());
    ctx.store_local(name, lowered, php_type, Some(span));
}

/// Lowers a `foreach` loop using high-level iterator opcodes.
fn lower_foreach(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    _value_by_ref: bool,
    body: &[Stmt],
) {
    let source = lower_expr(ctx, array);
    let iterator = ctx.emit_value(
        Op::IterStart,
        vec![source.value],
        None,
        PhpType::Iterable,
        Op::IterStart.default_effects(),
        Some(array.span),
    );
    let header = ctx.builder.create_named_block("foreach.next", Vec::new());
    let body_block = ctx.builder.create_named_block("foreach.body", Vec::new());
    let exit = ctx.builder.create_named_block("foreach.exit", Vec::new());
    branch_to(ctx, header);

    ctx.builder.position_at_end(header);
    let has_next = ctx.emit_value(
        Op::IterNext,
        vec![iterator.value],
        None,
        PhpType::Bool,
        Op::IterNext.default_effects(),
        Some(array.span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body_block);
    if let Some(key_var) = key_var {
        let key = ctx.emit_value(
            Op::IterCurrentKey,
            vec![iterator.value],
            None,
            PhpType::Mixed,
            Op::IterCurrentKey.default_effects(),
            Some(array.span),
        );
        ctx.store_local(key_var, key, PhpType::Mixed, Some(array.span));
    }
    let value = ctx.emit_value(
        Op::IterCurrentValue,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentValue.default_effects(),
        Some(array.span),
    );
    ctx.store_local(value_var, value, PhpType::Mixed, Some(array.span));
    ctx.loop_stack.push(LoopFrame { break_block: exit, continue_block: header });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
}

/// Lowers a `switch` and terminates the shared exit block when every arm exits earlier.
fn lower_switch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) {
    let subject = lower_expr(ctx, subject);
    let subject = coerce_to_int(ctx, subject, None);
    let exit = ctx.builder.create_named_block("switch.exit", Vec::new());
    let default_block = ctx.builder.create_named_block("switch.default", Vec::new());
    let mut blocks = Vec::with_capacity(cases.len());
    let mut switch_cases = Vec::new();
    for (case_exprs, _) in cases {
        let case_block = ctx.builder.create_named_block("switch.case", Vec::new());
        for case_expr in case_exprs {
            if let Some(value) = int_case_value(case_expr) {
                switch_cases.push(SwitchCase { value, target: case_block, args: Vec::new() });
            }
        }
        blocks.push(case_block);
    }
    ctx.builder.terminate(Terminator::Switch {
        scrutinee: subject.value,
        cases: switch_cases,
        default: default_block,
        default_args: Vec::new(),
    });

    ctx.loop_stack.push(LoopFrame { break_block: exit, continue_block: exit });
    let mut exit_reachable = false;
    for ((_, body), block) in cases.iter().zip(blocks) {
        ctx.builder.position_at_end(block);
        lower_block(ctx, body);
        if !ctx.builder.insertion_block_is_terminated() {
            exit_reachable = true;
            branch_to(ctx, exit);
        }
    }
    ctx.builder.position_at_end(default_block);
    if let Some(default) = default {
        lower_block(ctx, default);
    }
    if !ctx.builder.insertion_block_is_terminated() {
        exit_reachable = true;
        branch_to(ctx, exit);
    }
    ctx.loop_stack.pop();
    ctx.builder.position_at_end(exit);
    if !exit_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
}

/// Lowers include/require statements through a high-level runtime call.
fn lower_include(ctx: &mut LoweringContext<'_, '_>, path: &Expr, once: bool, required: bool, span: Span) {
    let path = lower_expr(ctx, path);
    let label = format!("include once={} required={}", once, required);
    let data = ctx.intern_string(&label);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![path.value],
        Some(Immediate::Data(data)),
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers an include-once marker.
fn lower_include_once_mark(ctx: &mut LoweringContext<'_, '_>, label: &str, span: Span) {
    let data = ctx.intern_string(label);
    ctx.emit_void(
        Op::IncludeOnceMark,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::IncludeOnceMark.default_effects(),
        Some(span),
    );
}

/// Lowers an include-once guarded body.
fn lower_include_once_guard(ctx: &mut LoweringContext<'_, '_>, label: &str, body: &[Stmt], span: Span) {
    let data = ctx.intern_string(label);
    let should_run = ctx
        .builder
        .emit_with_effects(
            Op::IncludeOnceGuard,
            Vec::new(),
            Some(Immediate::Data(data)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::IncludeOnceGuard.default_effects(),
            Some(span),
        )
        .expect("include_once_guard produces a branch condition");
    let body_block = ctx.builder.create_named_block("include_once_body", Vec::new());
    let after_block = ctx.builder.create_named_block("include_once_after", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: should_run,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: after_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(body_block);
    lower_block(ctx, body);
    branch_to(ctx, after_block);
    ctx.builder.position_at_end(after_block);
}

/// Lowers a throwing statement into a terminator.
fn lower_throw(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) {
    let value = lower_expr(ctx, expr);
    ctx.builder.terminate(Terminator::Throw { value: value.value });
}

/// Lowers a `try` statement with conservative high-level handler opcodes.
fn lower_try(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: Option<&[Stmt]>,
    span: Span,
) {
    ctx.emit_void(Op::TryPushHandler, Vec::new(), None, Op::TryPushHandler.default_effects(), Some(span));
    lower_block(ctx, try_body);
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    ctx.emit_void(Op::TryPopHandler, Vec::new(), None, Op::TryPopHandler.default_effects(), Some(span));
    for catch in catches {
        if let Some(variable) = &catch.variable {
            let caught = ctx.emit_value(
                Op::RuntimeCall,
                Vec::new(),
                None,
                PhpType::Mixed,
                effects_lookup::runtime_effects(),
                Some(span),
            );
            ctx.store_local(variable, caught, PhpType::Mixed, Some(span));
        }
        lower_block(ctx, &catch.body);
        if ctx.builder.insertion_block_is_terminated() {
            return;
        }
    }
    if let Some(finally_body) = finally_body {
        ctx.emit_void(Op::FinallyEnter, Vec::new(), None, Op::FinallyEnter.default_effects(), Some(span));
        lower_block(ctx, finally_body);
        if !ctx.builder.insertion_block_is_terminated() {
            ctx.emit_void(Op::FinallyExit, Vec::new(), None, Op::FinallyExit.default_effects(), Some(span));
        }
    }
}

/// Lowers a `break` terminator.
fn lower_break(ctx: &mut LoweringContext<'_, '_>, level: usize) {
    let Some(frame) = loop_target(ctx, level) else {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    };
    ctx.builder.terminate(Terminator::Br { target: frame.break_block, args: Vec::new() });
}

/// Lowers a `continue` terminator.
fn lower_continue(ctx: &mut LoweringContext<'_, '_>, level: usize) {
    let Some(frame) = loop_target(ctx, level) else {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    };
    ctx.builder.terminate(Terminator::Br { target: frame.continue_block, args: Vec::new() });
}

/// Lowers a return statement using the current function return contract.
fn lower_return(ctx: &mut LoweringContext<'_, '_>, value: Option<&Expr>, span: Span) {
    if ctx.return_type == IrType::Void {
        if let Some(value) = value {
            lower_expr(ctx, value);
        }
        ctx.builder.terminate(Terminator::Return { value: None });
        return;
    }
    let value = if let Some(value) = value {
        lower_expr(ctx, value)
    } else {
        emit_null_value(ctx, Some(span))
    };
    let value = coerce_to_return_type(ctx, value, Some(span));
    ctx.builder.terminate(Terminator::Return { value: Some(value.value) });
}

/// Lowers a global constant declaration.
fn lower_const_decl(ctx: &mut LoweringContext<'_, '_>, name: &str, value: &Expr, span: Span) {
    let value = lower_expr(ctx, value);
    let data = ctx.intern_global_name(name);
    ctx.emit_void(
        Op::StoreGlobal,
        vec![value.value],
        Some(Immediate::GlobalName(data)),
        Op::StoreGlobal.default_effects(),
        Some(span),
    );
}

/// Lowers simple positional list destructuring into indexed reads plus local writes.
fn lower_list_unpack(ctx: &mut LoweringContext<'_, '_>, vars: &[String], value: &Expr, span: Span) {
    let source = lower_expr(ctx, value);
    let item_type = list_unpack_item_type(ctx, source.value);
    let get_op = list_unpack_get_op(source.ir_type);
    for (index, var) in vars.iter().enumerate() {
        let index_value = lower_list_unpack_index(ctx, index, span);
        let item = ctx.emit_value(
            get_op,
            vec![source.value, index_value.value],
            None,
            item_type.clone(),
            get_op.default_effects(),
            Some(span),
        );
        ctx.store_local(var, item, item_type.clone(), Some(span));
    }
}

/// Emits the positional integer key used to read one list-unpack element.
fn lower_list_unpack_index(ctx: &mut LoweringContext<'_, '_>, index: usize, span: Span) -> LoweredValue {
    ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(index as i64)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    )
}

/// Returns the element-read opcode for a list-unpack source value.
fn list_unpack_get_op(source_type: IrType) -> Op {
    match source_type {
        IrType::Heap(crate::ir::IrHeapKind::Array) => Op::ArrayGet,
        IrType::Heap(crate::ir::IrHeapKind::Hash) => Op::HashGet,
        _ => Op::RuntimeCall,
    }
}

/// Returns the PHP type assigned to each simple list-unpack destination.
fn list_unpack_item_type(ctx: &LoweringContext<'_, '_>, source: crate::ir::ValueId) -> PhpType {
    let item_type = match ctx.builder.value_php_type(source).codegen_repr() {
        PhpType::Array(elem_ty) => *elem_ty,
        PhpType::AssocArray { value, .. } => *value,
        _ => PhpType::Mixed,
    };
    normalize_materialized_element_type(item_type)
}

/// Normalizes non-materializable element metadata to the null sentinel.
fn normalize_materialized_element_type(item_type: PhpType) -> PhpType {
    match item_type {
        PhpType::Never => PhpType::Void,
        other => other,
    }
}

/// Declares global aliases in the local slot table.
fn lower_global(ctx: &mut LoweringContext<'_, '_>, vars: &[String]) {
    for var in vars {
        ctx.declare_local_with_kind(var, PhpType::Mixed, LocalKind::GlobalAlias);
    }
}

/// Lowers a static local variable initialization.
fn lower_static_var(ctx: &mut LoweringContext<'_, '_>, name: &str, init: &Expr, span: Span) {
    let value = lower_expr(ctx, init);
    let slot = ctx.declare_local_with_kind(name, ctx.builder.value_php_type(value.value), LocalKind::StaticLocal);
    ctx.builder.emit_with_effects(
        Op::StoreStaticLocal,
        vec![value.value],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
        Op::StoreStaticLocal.default_effects(),
        Some(span),
    );
}

/// Lowers an object property write.
fn lower_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    let value = lower_expr(ctx, value);
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::PropSet,
        vec![object.value, value.value],
        Some(Immediate::Data(data)),
        Op::PropSet.default_effects(),
        Some(span),
    );
}

/// Lowers a static property write.
fn lower_static_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let value = lower_expr(ctx, value);
    store_static_property(ctx, receiver, property, value.value, span);
}

/// Lowers `Class::$prop[] = value`.
fn lower_static_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let property_value = load_static_property(ctx, receiver, property, span);
    let value = lower_expr(ctx, value);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `Class::$prop[index] = value`.
fn lower_static_property_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let property_value = load_static_property(ctx, receiver, property, span);
    let index = lower_expr(ctx, index);
    let value = lower_expr(ctx, value);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, index.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `$object->prop[] = value`.
fn lower_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    let value = lower_expr(ctx, value);
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![object.value, value.value],
        Some(Immediate::Data(data)),
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Lowers `$object->prop[index] = value`.
fn lower_property_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    let object = lower_expr(ctx, object);
    let index = lower_expr(ctx, index);
    let value = lower_expr(ctx, value);
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![object.value, index.value, value.value],
        Some(Immediate::Data(data)),
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Emits a no-op marker for declaration-only or frontend-only statements.
fn lower_noop(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    ctx.emit_void(Op::Nop, Vec::new(), None, Op::Nop.default_effects(), Some(span));
}

/// Records a function variant group in high-level EIR metadata form.
fn lower_function_variant_group(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    variants: &[String],
    span: Span,
) {
    let label = format!("{}:{}", name, variants.join(","));
    let data = ctx.intern_string(&label);
    ctx.emit_void(
        Op::FunctionVariantDispatch,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::FunctionVariantDispatch.default_effects(),
        Some(span),
    );
}

/// Records one selected function variant.
fn lower_function_variant_mark(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    variant: &str,
    span: Span,
) {
    let label = format!("{}:{}", name, variant);
    let data = ctx.intern_string(&label);
    ctx.emit_void(
        Op::FunctionVariantMark,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::FunctionVariantMark.default_effects(),
        Some(span),
    );
}

/// Emits a branch to `target` if the current block can still fall through.
fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}

/// Finds the active loop target for a one-based break/continue level.
fn loop_target(ctx: &LoweringContext<'_, '_>, level: usize) -> Option<LoopFrame> {
    let level = level.max(1);
    ctx.loop_stack
        .len()
        .checked_sub(level)
        .and_then(|index| ctx.loop_stack.get(index).copied())
}

/// Selects the strongest array write opcode valid for a lowered array value.
fn array_set_op(ir_type: IrType) -> Op {
    match ir_type {
        IrType::Heap(crate::ir::IrHeapKind::Array) => Op::ArraySet,
        IrType::Heap(crate::ir::IrHeapKind::Hash) => Op::HashSet,
        IrType::Heap(crate::ir::IrHeapKind::Buffer) => Op::BufferSet,
        _ => Op::RuntimeCall,
    }
}

/// Extracts an integer switch case value from literal cases.
fn int_case_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        _ => None,
    }
}

/// Emits a boolean constant value.
fn emit_const_bool(
    ctx: &mut LoweringContext<'_, '_>,
    value: bool,
    span: Option<Span>,
) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::ConstBool.default_effects(),
            span,
        )
        .expect("const_bool produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Emits a null sentinel value.
fn emit_null_value(ctx: &mut LoweringContext<'_, '_>, span: Option<Span>) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            span,
        )
        .expect("const_null produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Coerces a value to the current function return storage type when needed.
fn coerce_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    if value.ir_type == ctx.return_type {
        return value;
    }
    match ctx.return_type {
        IrType::I64 => coerce_to_int(ctx, value, span),
        IrType::F64 => coerce_to_float(ctx, value, span),
        IrType::Str => coerce_to_string(ctx, value, span),
        IrType::Heap(_) => ctx.emit_value(
            Op::RuntimeCall,
            vec![value.value],
            None,
            ctx.return_php_type.clone(),
            effects_lookup::runtime_effects(),
            span,
        ),
        IrType::Void => value,
    }
}

/// Coerces a value to integer storage.
fn coerce_to_int(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(
            Op::FToI,
            vec![value.value],
            None,
            PhpType::Int,
            Op::FToI.default_effects(),
            span,
        ),
        IrType::Str => ctx.emit_value(
            Op::StrToI,
            vec![value.value],
            None,
            PhpType::Int,
            Op::StrToI.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            PhpType::Int,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

/// Coerces a value to float storage.
fn coerce_to_float(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::F64 => value,
        IrType::I64 => ctx.emit_value(
            Op::IToF,
            vec![value.value],
            None,
            PhpType::Float,
            Op::IToF.default_effects(),
            span,
        ),
        IrType::Str => ctx.emit_value(
            Op::StrToF,
            vec![value.value],
            None,
            PhpType::Float,
            Op::StrToF.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::F64)),
            PhpType::Float,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

/// Coerces a value to string storage.
fn coerce_to_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::Str => value,
        IrType::I64 => ctx.emit_value(
            Op::IToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::IToStr.default_effects(),
            span,
        ),
        IrType::F64 => ctx.emit_value(
            Op::FToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::FToStr.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::RuntimeCall,
            vec![value.value],
            None,
            PhpType::Str,
            effects_lookup::runtime_effects(),
            span,
        ),
    }
}

/// Loads a static property value through a high-level EIR read.
fn load_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    span: Span,
) -> LoweredValue {
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    ctx.emit_value(
        Op::LoadStaticProperty,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Mixed,
        Op::LoadStaticProperty.default_effects(),
        Some(span),
    )
}

/// Stores a static property value through a high-level EIR write.
fn store_static_property(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: crate::ir::ValueId,
    span: Span,
) {
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    ctx.emit_void(
        Op::StoreStaticProperty,
        vec![value],
        Some(Immediate::Data(data)),
        Op::StoreStaticProperty.default_effects(),
        Some(span),
    );
}

/// Formats a static receiver for metadata immediates.
fn receiver_name(receiver: &StaticReceiver) -> String {
    match receiver {
        StaticReceiver::Named(name) => name.as_str().to_string(),
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Static => "static".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
    }
}
