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

use std::collections::HashSet;

use crate::ir::{
    BlockId, CmpPredicate, Immediate, IrType, LocalKind, LocalSlotId, Op, Ownership, SwitchCase,
    RuntimeCallTarget, Terminator,
};
use crate::ir_lower::context::{
    FinallyFrame, LoopCleanup, LoopFrame, LoweredValue, LoweringContext,
};
use crate::ir_lower::effects_lookup;
use crate::ir_lower::expr::{
    array_access_element_result_type, array_access_expr_value_type_for_ir, call_return_type,
    coerce_to_int_at_span, index_expr_key_type, lower_array_access_from_lowered_receiver,
    lower_callable_array_for_assignment,
    lower_array_literal_with_expected_type, lower_closure_for_assignment, lower_expr,
    method_call_expr_type_for_ir, property_access_expr_type_for_ir,
    reflection_arg_array_binding_for_expr, reflection_class_binding_for_expr,
    reflection_function_binding_for_expr, reflection_method_binding_for_expr,
    reflection_property_binding_for_expr, static_callable_binding_for_expr,
    static_method_call_expr_type_for_ir, string_op_uses_scratch_storage,
    type_satisfies_array_access_for_ir,
};
use crate::names::{php_symbol_key, property_hook_set_method};
use crate::parser::ast::{
    is_compound_assignment_self_read, CatchClause, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
};
use crate::span::Span;
use crate::types::{PhpType, ThrowAccessKind};

/// Lowers one AST statement into the current EIR insertion block.
pub(crate) fn lower_stmt(ctx: &mut LoweringContext<'_, '_>, stmt: &Stmt) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    lower_statement_concat_reset(ctx, stmt.span);
    match &stmt.kind {
        StmtKind::Echo(expr) => lower_echo(ctx, expr, stmt.span),
        StmtKind::Assign { name, value } => lower_assign(ctx, name, value, stmt.span),
        StmtKind::RefAssign { target, source } => lower_ref_assign(ctx, target, source, stmt.span),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => lower_if(
            ctx,
            condition,
            then_body,
            elseif_clauses,
            else_body.as_deref(),
            stmt.span,
        ),
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
        } => lower_for(
            ctx,
            init.as_deref(),
            condition.as_ref(),
            update.as_deref(),
            body,
        ),
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
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
        } => lower_foreach(
            ctx,
            array,
            key_var.as_deref(),
            value_var,
            *value_by_ref,
            body,
        ),
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
            let value = lower_expr(ctx, expr);
            release_expr_statement_result(ctx, value, expr.span);
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

/// Releases a discarded expression-statement result when it may own temporary storage.
fn release_expr_statement_result(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Emits the statement-boundary concat-buffer reset expected by the ASM backend.
fn lower_statement_concat_reset(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    if span.line == 0 {
        return;
    }
    ctx.emit_void(
        Op::ConcatReset,
        vec![],
        None,
        Op::ConcatReset.default_effects(),
        Some(span),
    );
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
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    ctx.emit_void(
        Op::EchoValue,
        vec![value.value],
        None,
        Op::EchoValue.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Lowers a plain PHP local assignment.
fn lower_assign(ctx: &mut LoweringContext<'_, '_>, name: &str, value: &Expr, span: Span) {
    // PHP allows compound assignment on an undefined variable (`$x += 1`),
    // treating the undefined variable as null/0 with a warning. The type
    // checker injects the variable as `Void` and emits a warning. At the
    // lowering level, we must initialize the local slot to null/0 before
    // the compound read so the runtime does not read garbage from the stack.
    if is_compound_assignment_self_read(value, name, span) && !ctx.has_local_slot(name) {
        let null_value = ctx.builder.emit_const_null();
        let null_lowered = LoweredValue { value: null_value, ir_type: IrType::I64 };
        ctx.store_local(name, null_lowered, PhpType::Void, Some(span));
        ctx.mark_local_initialized(name);
    }

    // A by-reference `Closure::bind(fn &() => $this->prop, $obj, $obj)` assigned to a variable is
    // tracked as a static callable, like a closure literal, so a later `$b()` lowers to a direct
    // call that carries the property's reference-cell pointer instead of boxing it.
    let bound_closure = crate::ir_lower::expr::is_bound_closure_assignment_shape(ctx, value);
    let direct_closure = matches!(value.kind, ExprKind::Closure { .. }) || bound_closure;
    ctx.clear_pending_static_callable_result();
    let static_callable = static_callable_binding_for_expr(ctx, value);
    let reflected_class = reflection_class_binding_for_expr(ctx, value);
    let reflected_function = reflection_function_binding_for_expr(ctx, value);
    let reflected_property = reflection_property_binding_for_expr(ctx, value);
    let reflected_method = reflection_method_binding_for_expr(ctx, value);
    let reflected_args = reflection_arg_array_binding_for_expr(value);
    let fiber_start_sig = crate::ir_lower::fibers::start_sig_for_expr(ctx, value);
    let callable_array = lower_callable_array_for_assignment(ctx, value, static_callable.as_ref());
    let lowered = callable_array
        .as_ref()
        .map(|assignment| assignment.value)
        .or_else(|| lower_closure_for_assignment(ctx, name, value))
        .or_else(|| {
            bound_closure
                .then(|| crate::ir_lower::expr::lower_bound_closure_for_assignment(ctx, value))
                .flatten()
        })
        .unwrap_or_else(|| lower_expr(ctx, value));
    let (lowered, php_type) = contextualize_array_assignment(ctx, name, value, lowered, span);
    ctx.store_local(name, lowered, php_type, Some(span));
    let callable_result = if direct_closure {
        ctx.take_pending_static_callable_result()
    } else {
        ctx.clear_pending_static_callable_result();
        None
    };
    let static_callable = callable_array
        .map(|assignment| assignment.target)
        .or(static_callable)
        .or(callable_result);
    if !closure_captures_local(value, name) {
        if let Some(target) = static_callable {
            ctx.bind_static_callable_local(name, target);
        }
    }
    if let Some(reflected_class) = reflected_class {
        ctx.bind_reflection_class_local(name, reflected_class);
    }
    if let Some(reflected_function) = reflected_function {
        ctx.bind_reflection_function_local(name, reflected_function);
    }
    if let Some((reflected_class, reflected_property)) = reflected_property {
        ctx.bind_reflection_property_local(name, reflected_class, reflected_property);
    }
    if let Some((reflected_class, reflected_method)) = reflected_method {
        ctx.bind_reflection_method_local(name, reflected_class, reflected_method);
    }
    if let Some(reflected_args) = reflected_args {
        ctx.bind_reflection_arg_array_local(name, reflected_args);
    }
    if let Some(sig) = fiber_start_sig {
        ctx.bind_fiber_start_sig(name, sig);
    }
}

/// Returns whether a closure literal captures the local being assigned.
fn closure_captures_local(value: &Expr, name: &str) -> bool {
    matches!(
        &value.kind,
        ExprKind::Closure { captures, capture_refs, .. }
            if captures.iter().any(|capture| capture == name)
                || capture_refs.iter().any(|capture| capture == name)
    )
}

/// Converts indexed array literals to hash storage when checker facts require an assoc local.
fn contextualize_array_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    value: &Expr,
    lowered: LoweredValue,
    span: Span,
) -> (LoweredValue, PhpType) {
    let php_type = ctx.builder.value_php_type(lowered.value);
    if !matches!(value.kind, ExprKind::ArrayLiteral(_)) {
        return (lowered, php_type);
    }
    if !matches!(php_type.codegen_repr(), PhpType::Array(_)) {
        return (lowered, php_type);
    }
    let contextual_ty = if crate::superglobals::is_superglobal(name) {
        crate::superglobals::superglobal_type().codegen_repr()
    } else {
        ctx.local_type(name).codegen_repr()
    };
    if !matches!(contextual_ty, PhpType::AssocArray { .. }) {
        return (lowered, php_type);
    }
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![lowered.value],
        None,
        contextual_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(span),
    );
    (hash, contextual_ty)
}

/// Lowers a by-reference assignment, dispatching on the kind of reference source.
///
/// - `$a = &$b` aliases two locals to one ref-cell.
/// - `$a = &$obj->prop` binds the local to the object's reference-property cell (write-through).
/// - `$a = &call()` binds the local to the cell returned by a by-reference callee.
/// - `$a = &$arr[idx]` binds the local to the indexed-array element's inline storage.
fn lower_ref_assign(ctx: &mut LoweringContext<'_, '_>, target: &str, source: &Expr, span: Span) {
    match &source.kind {
        ExprKind::Variable(source_name) => {
            let fiber_start_sig = ctx.fiber_start_sig_for_local(source_name);
            ctx.alias_local_ref_cell(target, source_name, Some(span));
            if let Some(sig) = fiber_start_sig {
                ctx.bind_fiber_start_sig(target, sig);
            }
        }
        ExprKind::PropertyAccess { .. } => {
            crate::ir_lower::expr::lower_ref_assign_property(ctx, target, source, span);
        }
        ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. } => {
            crate::ir_lower::expr::lower_ref_assign_call(ctx, target, source, span);
        }
        ExprKind::ArrayAccess { .. } => {
            crate::ir_lower::expr::lower_ref_assign_array_elem(ctx, target, source, span);
        }
        _ => {
            // Other source shapes are rejected by the checker;
            // evaluate for side effects to keep lowering total.
            lower_expr(ctx, source);
        }
    }
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
    let merge_reachable = lower_if_chain(
        ctx,
        condition,
        then_body,
        elseif_clauses,
        else_body,
        merge,
        span,
    );
    ctx.builder.position_at_end(merge);
    if !merge_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
    ctx.clear_static_callable_locals();
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
    let split_initialized = ctx.initialized_slots_snapshot();
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
    ctx.restore_initialized_slots(split_initialized.clone());
    lower_block(ctx, then_body);
    let then_initialized = ctx.initialized_slots_snapshot();
    let mut merge_reachable = false;
    let then_reachable = !ctx.builder.insertion_block_is_terminated();
    if then_reachable {
        merge_reachable = true;
        branch_to(ctx, merge);
    }

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(else_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    let else_reachable =
        if let Some(((next_condition, next_body), rest)) = elseif_clauses.split_first() {
            lower_if_chain(ctx, next_condition, next_body, rest, else_body, merge, span)
        } else if let Some(else_body) = else_body {
            lower_block(ctx, else_body);
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(ctx, merge);
                true
            } else {
                false
            }
        } else {
            lower_noop(ctx, span);
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(ctx, merge);
                true
            } else {
                false
            }
        };
    merge_reachable |= else_reachable;
    let else_initialized = ctx.initialized_slots_snapshot();
    ctx.restore_initialized_slots(merge_initialized_slots(
        &split_initialized,
        then_initialized,
        then_reachable,
        else_initialized,
        else_reachable,
    ));
    merge_reachable
}

/// Merges definitely-initialized locals from the reachable branches of an `if`.
fn merge_initialized_slots(
    split_initialized: &HashSet<LocalSlotId>,
    then_initialized: HashSet<LocalSlotId>,
    then_reachable: bool,
    else_initialized: HashSet<LocalSlotId>,
    else_reachable: bool,
) -> HashSet<LocalSlotId> {
    match (then_reachable, else_reachable) {
        (true, true) => then_initialized
            .intersection(&else_initialized)
            .copied()
            .collect(),
        (true, false) => then_initialized,
        (false, true) => else_initialized,
        (false, false) => split_initialized.clone(),
    }
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
    ctx.clear_static_callable_locals();
}

/// Widens locals whose indexed-array element type joins to `mixed` across the loop body's
/// push sites (issue #452) and materializes the promotion once before the loop. Loop bodies
/// are lowered in a single pass, so without this an early `$a[] = <scalar>` site is emitted
/// as a raw push against the pre-promotion element type even though the back edge brings the
/// promoted `array<mixed>` around, writing an unboxed scalar into boxed-cell storage on
/// iterations >= 2. Converting the array up front (in place, same pointer) and fixing its
/// type to `array<mixed>` makes every push site box its value. `overrides` supplies types
/// for names bound by the loop itself (the `foreach` value/key variables) so a push of the
/// loop variable joins with its real element type. Locals without a materialized slot yet
/// (first assigned inside the body, hence fresh every iteration) are left untouched.
fn widen_loop_grown_arrays(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
    update: Option<&Stmt>,
    overrides: &[(&str, PhpType)],
    span: Option<Span>,
) {
    let names = {
        let lookup = |name: &str| -> Option<PhpType> {
            if let Some((_, ty)) = overrides.iter().find(|(n, _)| *n == name) {
                return Some(ty.clone());
            }
            // A name with no declared slot yet (first assigned inside the loop body) is
            // genuinely unknown at loop entry: report it as such instead of the Mixed
            // fallback `local_type` would return, so the prescan does not take Mixed as
            // widening evidence for it (mirrors the checker's `env.get` lookup).
            if !ctx.local_slots.contains_key(name) {
                return None;
            }
            Some(ctx.local_type(name))
        };
        crate::types::checker::loop_grown_mixed_array_pushes(
            body,
            update,
            &lookup,
            &mut |expr| infer_loop_growth_value_type(ctx, expr),
        )
    };
    for name in names {
        if !ctx.local_slots.contains_key(&name) {
            continue;
        }
        let array_value = ctx.load_local(&name, span);
        if array_value.ir_type != IrType::Heap(crate::ir::IrHeapKind::Array) {
            continue;
        }
        let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
        let converted = ctx.emit_value(
            Op::ArrayToMixed,
            vec![array_value.value],
            None,
            mixed_array_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            span,
        );
        ctx.store_mutated_local(&name, converted, mixed_array_ty, span);
    }
}

/// Returns the value type that EIR lowering already knows before it emits a loop body.
fn infer_loop_growth_value_type(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<PhpType> {
    match &expr.kind {
        ExprKind::Variable(name) if ctx.local_slots.contains_key(name) => {
            Some(ctx.local_type(name))
        }
        ExprKind::FunctionCall { name, .. } => ctx
            .builtin_call_types
            .get(&expr.span)
            .cloned()
            .or_else(|| Some(call_return_type(ctx, name.as_str(), &[]))),
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_call_expr_type_for_ir(ctx, receiver, method),
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property)
        }
        ExprKind::ArrayAccess { array, .. } => {
            array_access_expr_value_type_for_ir(ctx, array)
        }
        ExprKind::This => ctx.current_class.clone().map(PhpType::Object),
        ExprKind::NewObject { class_name, .. } => {
            Some(PhpType::Object(class_name.as_str().to_string()))
        }
        ExprKind::ErrorSuppress(inner) => infer_loop_growth_value_type(ctx, inner),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_loop_growth_value_type(ctx, then_expr)?;
            let else_ty = infer_loop_growth_value_type(ctx, else_expr)?;
            if then_ty == else_ty {
                Some(then_ty)
            } else {
                Some(PhpType::Mixed)
            }
        }
        _ => None,
    }
}

/// Mirrors the checker's `foreach` value binding for the loop-widening prescan, preserving
/// concrete element types from locals, literals, and function-like source expressions.
fn foreach_prescan_value_type(ctx: &LoweringContext<'_, '_>, array: &Expr) -> PhpType {
    let source_ty = match &array.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        _ => infer_loop_growth_value_type(ctx, array)
            .unwrap_or_else(|| crate::types::checker::infer_expr_type_syntactic(array)),
    };
    foreach_value_type(&source_ty)
}

/// Lowers a `while` loop.
fn lower_while(ctx: &mut LoweringContext<'_, '_>, condition: &Expr, body: &[Stmt]) {
    widen_loop_grown_arrays(ctx, body, None, &[], Some(condition.span));
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

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: header,
        cleanup: None,
    });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Lowers a `do while` loop.
fn lower_do_while(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt], condition: &Expr) {
    widen_loop_grown_arrays(ctx, body, None, &[], Some(condition.span));
    let body_block = ctx.builder.create_named_block("do.body", Vec::new());
    let cond_block = ctx.builder.create_named_block("do.cond", Vec::new());
    let exit = ctx.builder.create_named_block("do.exit", Vec::new());
    branch_to(ctx, body_block);

    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: cond_block,
        cleanup: None,
    });
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
    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
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
    let widen_span = condition
        .map(|c| c.span)
        .or_else(|| body.first().map(|s| s.span));
    widen_loop_grown_arrays(ctx, body, update, &[], widen_span);

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

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: update_block,
        cleanup: None,
    });
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, update_block);

    ctx.builder.position_at_end(update_block);
    if let Some(update) = update {
        lower_stmt(ctx, update);
    }
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Releases the value operand of an array/hash element write when it is an owned
/// string. These writes PERSIST (copy) a string value into the container instead
/// of moving it (`__rt_str_persist`), so an owned string operand — e.g. a function
/// or extern call result like `$_ENV[$k] = getenv_value()` — would otherwise never
/// be freed (a per-write heap leak that exhausts the heap under `--web`). Non-string
/// refcounted values (objects, arrays) are moved, or retained only when borrowed,
/// by the write itself, so they must not be released here.
fn release_persisted_string_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    let ty = ctx.builder.value_php_type(value.value);
    // Only release a FRESH owning string temporary (a call/concat result, etc.).
    // A borrowed load of a variable that still owns the string (e.g. the prelude's
    // `$_GET[$k] = $v`) must NOT be released here, or the container's stored copy
    // would be freed out from under it.
    if matches!(ty.codegen_repr(), PhpType::Str) && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Releases an indexed-array write operand when the backend retained or copied it.
pub(super) fn release_indexed_array_write_operand(
    ctx: &mut LoweringContext<'_, '_>,
    container_elem_ty: Option<&PhpType>,
    value: LoweredValue,
    span: Span,
) {
    if !ctx.value_is_owning_temporary(value) {
        return;
    }
    let value_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if matches!(
        container_elem_ty.map(PhpType::codegen_repr),
        Some(PhpType::Mixed)
    ) && !matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return;
    }
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
}

/// Returns the indexed-array element type in effect for a write.
pub(super) fn indexed_array_write_element_type(
    ctx: &LoweringContext<'_, '_>,
    array_value: LoweredValue,
    updated_ty: Option<&PhpType>,
) -> Option<PhpType> {
    let array_ty = updated_ty
        .cloned()
        .unwrap_or_else(|| ctx.builder.value_php_type(array_value.value));
    match array_ty.codegen_repr() {
        PhpType::Array(elem_ty) => Some(elem_ty.codegen_repr()),
        _ => None,
    }
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
    let mut index_value = lower_expr(ctx, index);
    let mut value_value = lower_expr(ctx, value);
    let op = array_set_op(array_value.ir_type);
    // A literal string index always means a hash key, so promote the destination
    // to associative storage like PHP. A boxed Mixed/Union index may hold either
    // an integer or a string key (foreach loop keys are always Mixed in EIR via
    // `Op::IterCurrentKey`), so it goes through `Op::ArraySetMixedKey`, whose
    // runtime helper keeps integer keys on indexed storage (preserving indexed
    // consumers like `implode`) and promotes only string keys to a hash. This
    // stops a `foreach($arr as $k=>$v) $dst[$k]=$v` rebuild from collapsing a
    // string key onto int 0. A foreach key over a concretely-indexed array is
    // known to be int-valued, so it is left on the coerce path to avoid
    // needlessly dispatching.
    if op == Op::ArraySet && index_value.ir_type == IrType::Str {
        lower_string_key_array_promotion(ctx, array, array_value, index_value, value_value, span);
        return;
    }
    if op == Op::ArraySet
        && index_is_boxed_mixed_key(index_value.ir_type)
        && !index_is_foreach_int_key(ctx, index)
    {
        lower_mixed_key_array_set(ctx, array, array_value, index_value, value_value, span);
        return;
    }
    if op == Op::ArraySet {
        index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
        let array_ty = ctx.builder.value_php_type(array_value.value);
        value_value = coerce_indexed_array_set_value(ctx, &array_ty, value_value, Some(value.span));
    }
    if op == Op::BufferSet {
        index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
        let buffer_ty = ctx.builder.value_php_type(array_value.value);
        value_value = coerce_buffer_set_value(ctx, &buffer_ty, value_value, Some(value.span));
    }
    if op == Op::ArraySet {
        let (array_value, updated_ty, needs_storeback) =
            prepare_indexed_array_local_set(ctx, array_value, value_value, span);
        ctx.emit_void(
            op,
            vec![array_value.value, index_value.value, value_value.value],
            None,
            op.default_effects(),
            Some(span),
        );
        let elem_ty = indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
        finish_indexed_array_local_write(
            ctx,
            array,
            array_value,
            updated_ty,
            needs_storeback,
            span,
        );
        release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value_value, span);
        return;
    }
    ctx.emit_void(
        op,
        vec![array_value.value, index_value.value, value_value.value],
        None,
        op.default_effects(),
        Some(span),
    );
    release_persisted_string_operand(ctx, index_value, span);
    release_persisted_string_operand(ctx, value_value, span);
}

/// Coerces a buffer element write value into the scalar storage accepted by `BufferSet`.
fn coerce_buffer_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    buffer_ty: &PhpType,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let coerced = match buffer_ty.codegen_repr() {
        PhpType::Buffer(elem_ty) => match elem_ty.codegen_repr() {
            PhpType::Float => coerce_to_float(ctx, value, span),
            PhpType::Int | PhpType::Bool => coerce_to_int(ctx, value, span),
            _ => value,
        },
        _ => value,
    };
    if coerced.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
}

/// Promotes an indexed local array to a Mixed-valued associative array for string-key writes.
fn lower_string_key_array_promotion(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    span: Span,
) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    let assoc_ty = promoted_assoc_array_type(current_ty, value_ty);
    ctx.prepare_mutated_local_owner(array, array_value, assoc_ty.clone(), Some(span));
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array_value.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::HashSet,
        vec![hash.value, index.value, value.value],
        None,
        Op::HashSet.default_effects(),
        Some(span),
    );
    release_persisted_string_operand(ctx, index, span);
    release_persisted_string_operand(ctx, value, span);
    ctx.store_prepared_mutated_local(array, hash, assoc_ty, Some(span));
}

/// Writes `value` into the indexed local `array` under a boxed Mixed/Union key.
///
/// The destination stays statically `Array(Mixed)` (so indexed consumers such as
/// `implode` keep routing to the indexed path) while `Op::ArraySetMixedKey`
/// dispatches the key tag at runtime: integer keys stay on indexed storage and
/// string keys promote the destination to a hash. This is the Mixed-key analogue
/// of `lower_string_key_array_promotion`, which unconditionally promotes because
/// a literal string key is always a hash key.
fn lower_mixed_key_array_set(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    index: LoweredValue,
    value: LoweredValue,
    span: Span,
) {
    let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let result = ctx.emit_value(
        Op::ArraySetMixedKey,
        vec![array_value.value, index.value, value.value],
        None,
        mixed_array_ty.clone(),
        Op::ArraySetMixedKey.default_effects(),
        Some(span),
    );
    ctx.store_mutated_local(array, result, mixed_array_ty, Some(span));
}

/// Returns the associative type produced by a string-key write to an indexed array.
fn promoted_assoc_array_type(current_ty: PhpType, value_ty: PhpType) -> PhpType {
    let value_ty = normalize_array_write_element_type(value_ty.codegen_repr());
    let assoc_value_ty = match current_ty.codegen_repr() {
        PhpType::Array(elem_ty) if is_empty_indexed_array_element(elem_ty.as_ref()) => value_ty,
        PhpType::Array(elem_ty) => {
            let elem_ty = normalize_array_write_element_type(elem_ty.codegen_repr());
            if elem_ty == value_ty {
                elem_ty
            } else {
                PhpType::Mixed
            }
        }
        _ => PhpType::Mixed,
    };
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(assoc_value_ty),
    }
}

/// Lowers a nested array assignment that already carries an expression target.
fn lower_nested_array_assign(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    span: Span,
) {
    // Lowering the FULL target as an expression routes the write through the
    // read helper (`__rt_mixed_array_get`), which returns a detached fresh box
    // whenever the slot storage is not already a boxed Mixed cell; the
    // two-operand cell replacement then mutated a temporary and the write was
    // silently lost (#529). Splitting off the innermost key writes through the
    // parent cell instead (`__rt_mixed_array_set` for Mixed parents,
    // `offsetSet` for ArrayAccess objects), which mutates the aliased
    // container for every slot representation. The parent chain itself is
    // lowered with fetch-for-write semantics so missing or null intermediate
    // elements autovivify as arrays instead of dropping the write (#555).
    if let ExprKind::ArrayAccess { array, index } = &target.kind {
        let parent = lower_nested_assign_parent(ctx, array, span);
        let key = lower_expr(ctx, index);
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::RuntimeCall,
            vec![parent.value, key.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        release_persisted_string_operand(ctx, key, span);
        release_persisted_string_operand(ctx, value, span);
        // Parent subscript reads of Mixed/refcounted elements are owning
        // temporaries (`ArrayGet`/`HashGet`/`RuntimeCall` return a +1 caller
        // reference — fresh, retained, or installed by autovivification). The
        // set helper mutates through the cell/object without consuming that
        // reference, so release it here. Non-owning parents (plain locals,
        // `$this`) are left to normal scope cleanup.
        if ctx.value_is_owning_temporary(parent) {
            crate::ir_lower::ownership::release_if_owned(ctx, parent, Some(span));
        }
        return;
    }
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

/// Lowers the parent chain of a nested array assignment with write-context
/// (fetch-for-write) semantics (issue #555): missing indexed elements, null
/// gap slots, boxed `Mixed(null)` elements, and missing hash keys autovivify
/// as empty arrays installed into the parent storage, and the STORED cell is
/// returned so the leaf write lands in the parent container. PHP emits no
/// undefined-key warning for these legal writes, and neither does this path.
/// Shapes without a for-write lowering fall back to the plain read used
/// before (ArrayAccess objects, non-container receivers).
fn lower_nested_assign_parent(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
    span: Span,
) -> LoweredValue {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return lower_expr(ctx, expr);
    };
    // Concrete container locals: ensure the element exists through the
    // runtime wrapper and store the possibly reallocated container back.
    if let ExprKind::Variable(name) = &array.kind {
        let name = name.clone();
        if let Some(parent) = lower_local_parent_fetch_for_write(ctx, &name, index, expr) {
            return parent;
        }
    }
    // Boxed Mixed receivers: chains recurse with for-write semantics; other
    // receiver shapes evaluate once as plain reads of the receiver cell.
    let receiver = if matches!(array.kind, ExprKind::ArrayAccess { .. }) {
        lower_nested_assign_parent(ctx, array, span)
    } else {
        lower_expr(ctx, array)
    };
    if ctx.builder.value_php_type(receiver.value).codegen_repr() == PhpType::Mixed {
        let key = lower_expr(ctx, index);
        let parent = ctx.emit_value(
            Op::RuntimeCall,
            vec![receiver.value, key.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
            PhpType::Mixed,
            effects_lookup::runtime_effects(),
            Some(expr.span),
        );
        release_persisted_string_operand(ctx, key, span);
        if ctx.value_is_owning_temporary(receiver) {
            crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
        }
        return parent;
    }
    // The receiver is already evaluated but not a boxed Mixed cell: finish as
    // the plain subscript read the pre-#555 lowering produced.
    lower_array_access_from_lowered_receiver(ctx, receiver, index, expr)
}

/// Lowers `$local[key]` as the parent of a nested assignment when the local
/// holds a concrete container (`array<mixed>` or a Mixed-valued assoc array):
/// `__rt_array_ensure_elem_for_write` autovivifies the element in write
/// context, the possibly promoted/reallocated container is stored back into
/// the local, and the guaranteed-present element is re-read as the parent
/// cell. Returns `None` for shapes without a concrete for-write lowering
/// (typed element arrays, non-Int/Str key expressions).
fn lower_local_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    parent_expr: &Expr,
) -> Option<LoweredValue> {
    let span = parent_expr.span;
    let local_ty = ctx.local_type(name);
    match local_ty.codegen_repr() {
        PhpType::Array(elem_ty)
            if elem_ty.codegen_repr() == PhpType::Mixed
                || is_empty_indexed_array_element(elem_ty.as_ref()) =>
        {
            match index_expr_key_type(ctx, index) {
                PhpType::Int => {
                    let array_value = ctx.load_local(name, Some(span));
                    let key = lower_expr(ctx, index);
                    let key = coerce_to_int_at_span(ctx, key, Some(index.span));
                    // Autovivification makes the element type effectively
                    // Mixed even when the array started empty-typed. The
                    // ensure call consumes the loaded container (in-place
                    // mutation or realloc), so the previous boxed owner of a
                    // Mixed-storage slot must be released up front and the
                    // storeback must not release again.
                    let ensured_ty = PhpType::Array(Box::new(PhpType::Mixed));
                    ctx.prepare_mutated_local_owner(name, array_value, ensured_ty.clone(), Some(span));
                    let ensured = ctx.emit_value(
                        Op::RuntimeCall,
                        vec![array_value.value, key.value],
                        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
                        ensured_ty.clone(),
                        effects_lookup::runtime_effects(),
                        Some(span),
                    );
                    ctx.store_prepared_mutated_local(name, ensured, ensured_ty, Some(span));
                    // The element now exists: the in-bounds read returns the
                    // STORED cell (retained) without an undefined-key warning.
                    let cell = ctx.emit_value(
                        Op::ArrayGet,
                        vec![ensured.value, key.value],
                        None,
                        PhpType::Mixed,
                        Op::ArrayGet.default_effects(),
                        Some(span),
                    );
                    Some(cell)
                }
                PhpType::Str => {
                    // A literal string key on an indexed local is always a
                    // hash key: promote the local to a Mixed-valued hash
                    // first (mirrors `lower_string_key_array_promotion`),
                    // then ensure the element through the hash path. The
                    // promoted hash flows straight into the ensure call and
                    // is stored back exactly once at the end.
                    let array_value = ctx.load_local(name, Some(span));
                    let assoc_ty = promoted_assoc_array_type(local_ty, PhpType::Mixed);
                    ctx.prepare_mutated_local_owner(name, array_value, assoc_ty.clone(), Some(span));
                    let hash = ctx.emit_value(
                        Op::ArrayToHash,
                        vec![array_value.value],
                        None,
                        assoc_ty.clone(),
                        Op::ArrayToHash.default_effects(),
                        Some(span),
                    );
                    Some(lower_hash_parent_fetch_for_write(ctx, name, hash, assoc_ty, index, span))
                }
                _ => None,
            }
        }
        PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed => {
            let hash_value = ctx.load_local(name, Some(span));
            let assoc_ty = ctx.local_type(name);
            ctx.prepare_mutated_local_owner(name, hash_value, assoc_ty.clone(), Some(span));
            Some(lower_hash_parent_fetch_for_write(ctx, name, hash_value, assoc_ty, index, span))
        }
        _ => None,
    }
}

/// Ensures a hash element exists for a nested write parent, stores the
/// possibly reallocated hash back into the local (the previous owner was
/// already released by `prepare_mutated_local_owner`), and re-reads the
/// stored cell (retained by `Op::HashGet`) as the parent of the leaf write.
fn lower_hash_parent_fetch_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    hash_value: LoweredValue,
    assoc_ty: PhpType,
    index: &Expr,
    span: Span,
) -> LoweredValue {
    let key = lower_expr(ctx, index);
    let ensured = ctx.emit_value(
        Op::RuntimeCall,
        vec![hash_value.value, key.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)),
        assoc_ty.clone(),
        effects_lookup::runtime_effects(),
        Some(span),
    );
    ctx.store_prepared_mutated_local(name, ensured, assoc_ty, Some(span));
    ctx.emit_value(
        Op::HashGet,
        vec![ensured.value, key.value],
        None,
        PhpType::Mixed,
        Op::HashGet.default_effects(),
        Some(span),
    )
}

/// Lowers `$array[] = value`.
fn lower_array_push(ctx: &mut LoweringContext<'_, '_>, array: &str, value: &Expr, span: Span) {
    let array_value = ctx.load_local(array, Some(span));
    let value = lower_expr(ctx, value);
    let op = if array_value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Array) {
        Op::ArrayPush
    } else if array_value.ir_type == IrType::Heap(crate::ir::IrHeapKind::Mixed) {
        Op::MixedArrayAppend
    } else {
        Op::RuntimeCall
    };
    if op == Op::ArrayPush {
        let (array_value, updated_ty, needs_storeback) =
            if ref_bound_mixed_indexed_array_write(ctx, array, value) {
                (array_value, Some(ctx.local_type(array)), true)
            } else {
                prepare_indexed_array_local_write(ctx, array_value, value, span)
            };
        ctx.emit_void(
            op,
            vec![array_value.value, value.value],
            None,
            op.default_effects(),
            Some(span),
        );
        let elem_ty = indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
        finish_indexed_array_local_write(
            ctx,
            array,
            array_value,
            updated_ty,
            needs_storeback,
            span,
        );
        release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, span);
        return;
    }
    ctx.emit_void(
        op,
        vec![array_value.value, value.value],
        None,
        op.default_effects(),
        Some(span),
    );
    release_persisted_string_operand(ctx, value, span);
}

/// Prepares an indexed-array local for an offset assignment.
fn prepare_indexed_array_local_set(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<PhpType>, bool) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    if indexed_array_refcounted_set_needs_mixed_conversion(&current_ty, &value_ty) {
        let updated_ty = PhpType::Array(Box::new(PhpType::Mixed));
        let converted = ctx.emit_value(
            Op::ArrayToMixed,
            vec![array_value.value],
            None,
            updated_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(span),
        );
        return (converted, Some(updated_ty), true);
    }
    prepare_indexed_array_local_write(ctx, array_value, value, span)
}

/// Coerces miss-capable scalar reads before writing them into a concrete indexed-array slot.
fn coerce_indexed_array_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    array_ty: &PhpType,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match array_ty.codegen_repr() {
        PhpType::Array(elem_ty)
            if elem_ty.codegen_repr() == PhpType::Int
                && matches!(
                    ctx.builder.value_php_type(value.value).codegen_repr(),
                    PhpType::Mixed | PhpType::TaggedScalar | PhpType::Union(_)
                ) =>
        {
            coerce_to_int(ctx, value, span)
        }
        _ => value,
    }
}

/// Returns true when a refcounted indexed-array assignment should use Mixed slots.
fn indexed_array_refcounted_set_needs_mixed_conversion(
    current_ty: &PhpType,
    value_ty: &PhpType,
) -> bool {
    let PhpType::Array(elem_ty) = current_ty.codegen_repr() else {
        return false;
    };
    let elem_ty = elem_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    elem_ty != value_ty
        && elem_ty != PhpType::Mixed
        && elem_ty.is_refcounted()
        && value_ty.is_refcounted()
}

/// Converts typed indexed arrays to Mixed when a local write would make them heterogeneous.
pub(super) fn prepare_indexed_array_local_write(
    ctx: &mut LoweringContext<'_, '_>,
    array_value: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, Option<PhpType>, bool) {
    let current_ty = ctx.builder.value_php_type(array_value.value);
    let value_ty = ctx.builder.value_php_type(value.value);
    let Some(updated_ty) = indexed_array_write_updated_type(current_ty.clone(), value_ty) else {
        return (array_value, None, false);
    };
    if !indexed_array_write_needs_mixed_conversion(&current_ty, &updated_ty) {
        return (array_value, Some(updated_ty), false);
    }
    let converted = ctx.emit_value(
        Op::ArrayToMixed,
        vec![array_value.value],
        None,
        updated_ty.clone(),
        Op::ArrayToMixed.default_effects(),
        Some(span),
    );
    (converted, Some(updated_ty), true)
}

/// Updates local type facts and emits explicit storeback for converted array writes.
pub(super) fn finish_indexed_array_local_write(
    ctx: &mut LoweringContext<'_, '_>,
    array: &str,
    array_value: LoweredValue,
    updated_ty: Option<PhpType>,
    needs_storeback: bool,
    span: Span,
) {
    let Some(updated_ty) = updated_ty else {
        return;
    };
    if needs_storeback {
        ctx.store_mutated_local(array, array_value, updated_ty, Some(span));
    } else {
        ctx.set_local_type(array, updated_ty);
    }
}

/// Returns true when a ref-bound indexed array should keep its caller-visible element type.
pub(super) fn ref_bound_mixed_indexed_array_write(
    ctx: &LoweringContext<'_, '_>,
    array: &str,
    value: LoweredValue,
) -> bool {
    ctx.is_ref_bound_local(array)
        && matches!(
            ctx.builder.value_php_type(value.value).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
}

/// Returns the refined array type after writing a value into an indexed array.
fn indexed_array_write_updated_type(current_ty: PhpType, value_ty: PhpType) -> Option<PhpType> {
    match current_ty.codegen_repr() {
        PhpType::Array(elem_ty) if is_empty_indexed_array_element(elem_ty.as_ref()) => Some(
            PhpType::Array(Box::new(normalize_empty_array_write_element_type(value_ty))),
        ),
        PhpType::Array(elem_ty) if elem_ty.codegen_repr() == PhpType::Mixed => None,
        PhpType::Array(elem_ty) => {
            let elem_ty = elem_ty.codegen_repr();
            if elem_ty == value_ty.codegen_repr() {
                return None;
            }
            let value_ty = normalize_array_write_element_type(value_ty.codegen_repr());
            if elem_ty == value_ty {
                None
            } else {
                Some(PhpType::Array(Box::new(PhpType::Mixed)))
            }
        }
        _ => None,
    }
}

/// Returns true when an indexed-array write needs runtime conversion to Mixed slots.
fn indexed_array_write_needs_mixed_conversion(current_ty: &PhpType, updated_ty: &PhpType) -> bool {
    let PhpType::Array(current_elem) = current_ty.codegen_repr() else {
        return false;
    };
    let PhpType::Array(updated_elem) = updated_ty.codegen_repr() else {
        return false;
    };
    updated_elem.codegen_repr() == PhpType::Mixed && current_elem.codegen_repr() != PhpType::Mixed
}

/// Returns true for the placeholder element type used by empty indexed arrays.
fn is_empty_indexed_array_element(elem_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Never | PhpType::Void)
}

/// Preserves the first concrete value type written into an empty indexed array.
fn normalize_empty_array_write_element_type(item_type: PhpType) -> PhpType {
    normalize_materialized_element_type(item_type)
}

/// Lowers an assignment with a declared type.
fn lower_typed_assign(
    ctx: &mut LoweringContext<'_, '_>,
    type_expr: &crate::parser::ast::TypeExpr,
    name: &str,
    value: &Expr,
    span: Span,
) {
    let direct_closure = matches!(value.kind, ExprKind::Closure { .. });
    ctx.clear_pending_static_callable_result();
    let php_type = ctx.type_expr_to_php_type_for_value(type_expr);
    let static_callable = static_callable_binding_for_expr(ctx, value);
    let reflected_class = reflection_class_binding_for_expr(ctx, value);
    let reflected_property = reflection_property_binding_for_expr(ctx, value);
    let fiber_start_sig = crate::ir_lower::fibers::start_sig_for_expr(ctx, value);
    let callable_array = lower_callable_array_for_assignment(ctx, value, static_callable.as_ref());
    let lowered = callable_array
        .as_ref()
        .map(|assignment| assignment.value)
        .unwrap_or_else(|| lower_expr(ctx, value));
    let lowered = coerce_typed_assign_value(ctx, lowered, &php_type, span);
    ctx.declare_local(name, php_type.clone());
    ctx.store_local(name, lowered, php_type, Some(span));
    let callable_result = if direct_closure {
        ctx.take_pending_static_callable_result()
    } else {
        ctx.clear_pending_static_callable_result();
        None
    };
    let static_callable = callable_array
        .map(|assignment| assignment.target)
        .or(static_callable)
        .or(callable_result);
    if let Some(target) = static_callable {
        ctx.bind_static_callable_local(name, target);
    }
    if let Some(reflected_class) = reflected_class {
        ctx.bind_reflection_class_local(name, reflected_class);
    }
    if let Some((reflected_class, reflected_property)) = reflected_property {
        ctx.bind_reflection_property_local(name, reflected_class, reflected_property);
    }
    if let Some(sig) = fiber_start_sig {
        ctx.bind_fiber_start_sig(name, sig);
    }
}

/// Coerces a typed local assignment into the storage shape required by the declared type.
fn coerce_typed_assign_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    php_type: &PhpType,
    span: Span,
) -> LoweredValue {
    let target_ty = php_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if source_ty == target_ty {
        return value;
    }
    match target_ty {
        PhpType::Mixed => ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)),
        _ => value,
    }
}

/// Lowers a `foreach` loop using high-level iterator opcodes.
fn lower_foreach(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
) {
    // Widen before the source is lowered so an iterated-and-pushed array is loaded with
    // its fixed-point element type, and bind the loop variables' prescan types so a push
    // of the foreach value joins with its real element type.
    let prescan_value_ty = foreach_prescan_value_type(ctx, array);
    let mut overrides: Vec<(&str, PhpType)> = vec![(value_var, prescan_value_ty)];
    if let Some(key_var) = key_var {
        overrides.push((key_var, PhpType::Mixed));
    }
    widen_loop_grown_arrays(ctx, body, None, &overrides, Some(array.span));
    let source = lower_expr(ctx, array);
    let source_php_ty = ctx.builder.value_php_type(source.value);
    let source_ty = source_php_ty.codegen_repr();
    let key_needs_null_init = key_var.is_some_and(|name| !ctx.local_slots.contains_key(name));
    let value_needs_null_init = !ctx.local_slots.contains_key(value_var);
    // A foreach over a concretely-indexed array (`Array` of a non-Mixed element
    // type) always yields integer keys, even though `Op::IterCurrentKey` lowers
    // the key as Mixed. Tag the key local so a `$dst[$key] = ...` write coerces
    // the int-valued Mixed key to int instead of promoting the destination to a
    // hash. Generic `Array(Mixed)`, `AssocArray`, `Mixed`, and `Union` sources
    // may carry string keys and are left untagged so the write promotes.
    if let Some(key_var) = key_var {
        if let PhpType::Array(elem_ty) = &source_php_ty {
            if !matches!(elem_ty.as_ref(), PhpType::Mixed) {
                ctx.mark_foreach_int_key(key_var);
            }
        }
    }
    let iterator = ctx.emit_value(
        Op::IterStart,
        vec![source.value],
        value_by_ref.then_some(Immediate::Bool(true)),
        PhpType::Iterable,
        Op::IterStart.default_effects(),
        Some(array.span),
    );
    if let Some(key_var) = key_var {
        initialize_foreach_mixed_local_if_needed(ctx, key_var, key_needs_null_init, array.span);
    }
    if value_by_ref {
        let value_ty = foreach_ref_value_type(&source_ty);
        ctx.declare_local(value_var, value_ty.clone());
        ctx.set_local_type(value_var, value_ty);
        if !value_needs_null_init {
            ctx.mark_local_initialized(value_var);
            if !ctx.is_ref_bound_local(value_var) {
                ctx.promote_local_ref_cell(value_var, Some(array.span));
            }
        }
    } else {
        let value_ty = foreach_value_type(&source_ty);
        if value_ty == PhpType::Mixed {
            initialize_foreach_mixed_local_if_needed(
                ctx,
                value_var,
                value_needs_null_init,
                array.span,
            );
        } else if value_needs_null_init {
            ctx.declare_local(value_var, value_ty.clone());
            ctx.set_local_type(value_var, value_ty);
        }
    }
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

    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    let cleanup = ctx
        .value_is_owning_temporary(source)
        .then_some(LoopCleanup {
            value: source,
            span: array.span,
        });
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: header,
        cleanup,
    });
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
    if value_by_ref {
        let slot = ctx.declare_local(value_var, foreach_ref_value_type(&source_ty));
        ctx.release_ref_cell_owner(value_var, Some(array.span));
        ctx.emit_void(
            Op::IterCurrentValueRef,
            vec![iterator.value],
            Some(Immediate::LocalSlot(slot)),
            Op::IterCurrentValueRef.default_effects(),
            Some(array.span),
        );
        ctx.mark_ref_bound_local(value_var);
        ctx.mark_local_initialized(value_var);
    } else {
        let value_ty = foreach_value_type(&source_ty);
        let value = ctx.emit_value(
            Op::IterCurrentValue,
            vec![iterator.value],
            None,
            value_ty.clone(),
            Op::IterCurrentValue.default_effects(),
            Some(array.span),
        );
        ctx.store_local(value_var, value, value_ty, Some(array.span));
    }
    lower_block(ctx, body);
    ctx.loop_stack.pop();
    branch_to(ctx, header);
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
    // Release the source when it is a fresh owning temporary (e.g. `foreach
    // (explode(...) as $p)` or a literal array): the iterator borrows it for the
    // duration of the loop, so nothing else frees it once iteration ends. (For an
    // array the iterator aliases the source, so it must NOT be released separately
    // — that would double-free.)
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(array.span));
    }
}

/// Returns the by-value foreach local type when Phase 04 can keep a concrete element.
fn foreach_value_type(source_ty: &PhpType) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::Array(elem) => match elem.codegen_repr() {
            PhpType::Callable => PhpType::Callable,
            PhpType::Object(class_name) => PhpType::Object(class_name),
            elem @ (PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool) => elem,
            _ => PhpType::Mixed,
        },
        PhpType::Object(class_name) if class_name == "Phar" || class_name == "PharData" => {
            PhpType::Object("PharFileInfo".to_string())
        }
        _ => PhpType::Mixed,
    }
}

/// Returns the local value type used when a foreach binds the value by reference.
fn foreach_ref_value_type(source_ty: &PhpType) -> PhpType {
    match source_ty.codegen_repr() {
        PhpType::Array(elem) => *elem,
        PhpType::AssocArray { value, .. } => *value,
        _ => PhpType::Mixed,
    }
}

/// Initializes a fresh foreach loop variable to boxed null before the first iteration.
fn initialize_foreach_mixed_local_if_needed(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    needs_init: bool,
    span: Span,
) {
    if !needs_init {
        return;
    }
    // This setup can run once per outer-loop iteration at runtime, overwriting
    // the loop variable. `store_local` owns the carried release: it frees the
    // previous runtime occupant when this synthetic store is loop-carried.
    ctx.declare_local(name, PhpType::Mixed);
    ctx.set_local_type(name, PhpType::Mixed);
    let null = emit_null_value(ctx, Some(span));
    let boxed = ctx.box_value_as_mixed(null, PhpType::Mixed, Some(span));
    ctx.store_foreach_initializer_local_only(name, boxed, PhpType::Mixed, Some(span));
}

/// Lowers a `switch` with source-ordered pattern evaluation and PHP fallthrough.
fn lower_switch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
) {
    let subject = lower_expr(ctx, subject);
    let exit = ctx.builder.create_named_block("switch.exit", Vec::new());
    let default_block = ctx.builder.create_named_block("switch.default", Vec::new());
    let blocks = cases
        .iter()
        .map(|_| ctx.builder.create_named_block("switch.case", Vec::new()))
        .collect::<Vec<_>>();

    // The compact integer jump table is valid only for an integer scrutinee with
    // integer case labels. Any other subject (string, float, mixed) takes the
    // source-ordered dynamic path — see `lower_dynamic_switch_dispatch` for how it
    // picks PHP loose-equality vs the integer fast path per subject/case pair.
    if subject.ir_type == IrType::I64 && can_lower_static_switch(cases) {
        let subject = coerce_to_int(ctx, subject, None);
        lower_static_switch_dispatch(ctx, subject, cases, &blocks, default_block);
    } else {
        lower_dynamic_switch_dispatch(ctx, subject, cases, &blocks, default_block);
    }

    lower_switch_bodies(ctx, cases, default, &blocks, default_block, exit);
}

/// Returns true when every switch case pattern can use the static integer switch terminator.
fn can_lower_static_switch(cases: &[(Vec<Expr>, Vec<Stmt>)]) -> bool {
    cases
        .iter()
        .flat_map(|(case_exprs, _)| case_exprs)
        .all(|case_expr| int_case_value(case_expr).is_some())
}

/// Emits the compact integer-switch dispatch for statically-known case values.
fn lower_static_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: LoweredValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    blocks: &[BlockId],
    default_block: BlockId,
) {
    let mut switch_cases = Vec::new();
    for ((case_exprs, _), case_block) in cases.iter().zip(blocks) {
        for case_expr in case_exprs {
            let Some(value) = int_case_value(case_expr) else {
                continue;
            };
            switch_cases.push(SwitchCase {
                value,
                target: *case_block,
                args: Vec::new(),
            });
        }
    }
    ctx.builder.terminate(Terminator::Switch {
        scrutinee: subject.value,
        cases: switch_cases,
        default: default_block,
        default_args: Vec::new(),
    });
    ctx.clear_static_callable_locals();
}

/// Emits source-ordered dynamic switch pattern checks for non-literal case expressions.
///
/// PHP `switch` compares the subject against each case with loose equality (`==`).
/// String subjects/labels and float/numeric pairs are dispatched through `Op::LooseEq`
/// so the comparison honors PHP string/numeric coercion rules (`switch (1.5)` matching
/// `case 1.5`, not `case 1`); purely integer-like subject-and-case pairs keep the
/// cheaper `coerce_to_int` + `ICmp` fast path.
fn lower_dynamic_switch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    subject: LoweredValue,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    blocks: &[BlockId],
    default_block: BlockId,
) {
    let subject_is_str = subject.ir_type == IrType::Str;
    let subject_is_mixed = matches!(subject.ir_type, IrType::Heap(crate::ir::IrHeapKind::Mixed));
    // Non-string, non-Mixed subjects are coerced to an integer once and reused by the ICmp path.
    // Mixed subjects must use loose equality for every case because the runtime tag may be
    // float, string, bool, etc. — coercing to int would truncate a float (issue #397).
    let int_subject = if subject_is_str || subject_is_mixed {
        None
    } else {
        Some(coerce_to_int(ctx, subject, None))
    };
    for ((case_exprs, _), case_block) in cases.iter().zip(blocks) {
        for case_expr in case_exprs {
            let case_value = lower_expr(ctx, case_expr);
            // Strings and floats must use loose equality: coercing a string to int
            // collapses every case to `0 == 0`, and coercing a float to int would
            // truncate the subject (so `switch (1.5) { case 1.5; }` would wrongly
            // match `case 1`). The cheap ICmp fast path stays for integer-like pairs.
            // Mixed subjects must always use loose equality (tag-aware comparison).
            let use_loose_eq = subject_is_str
                || subject_is_mixed
                || case_value.ir_type == IrType::Str
                || float_loose_eq_pair(subject.ir_type, case_value.ir_type);
            let matched = if use_loose_eq {
                // Loose equality handles string/string, string/scalar, float/numeric,
                // and mixed cases exactly as PHP's `==` would inside an if/elseif chain.
                ctx.emit_value(
                    Op::LooseEq,
                    vec![subject.value, case_value.value],
                    None,
                    PhpType::Bool,
                    Op::LooseEq.default_effects(),
                    Some(case_expr.span),
                )
            } else {
                let case_value = coerce_to_int(ctx, case_value, Some(case_expr.span));
                ctx.emit_value(
                    Op::ICmp,
                    vec![
                        int_subject
                            .expect("non-string subject is pre-coerced")
                            .value,
                        case_value.value,
                    ],
                    Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
                    PhpType::Bool,
                    Op::ICmp.default_effects(),
                    Some(case_expr.span),
                )
            };
            let miss_block = ctx.builder.create_named_block("switch.next", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: matched.value,
                then_target: *case_block,
                then_args: Vec::new(),
                else_target: miss_block,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(miss_block);
        }
    }
    branch_to(ctx, default_block);
    ctx.clear_static_callable_locals();
}

/// Returns true when a switch subject/case pair must compare via float loose equality:
/// at least one side is a statically-typed float and both are numeric (`int`/`float`).
/// These pairs route through `Op::LooseEq`, which promotes both operands to float, so the
/// subject is not truncated to int (the backend supports float-vs-int loose equality).
///
/// An untyped (`Mixed`) subject holding a float is not covered here: it still takes the
/// integer fast path and truncates, a separate pre-existing loose-equality limitation that
/// needs a tag-aware runtime comparison helper (tracked in issue #397).
fn float_loose_eq_pair(subject_ty: IrType, case_ty: IrType) -> bool {
    let numeric = |ty: IrType| matches!(ty, IrType::I64 | IrType::F64);
    (subject_ty == IrType::F64 || case_ty == IrType::F64) && numeric(subject_ty) && numeric(case_ty)
}

/// Lowers switch case/default bodies and preserves PHP fallthrough between adjacent bodies.
fn lower_switch_bodies(
    ctx: &mut LoweringContext<'_, '_>,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    blocks: &[BlockId],
    default_block: BlockId,
    exit: BlockId,
) {
    let default_index = default
        .and_then(|default| switch_default_source_index(cases, default))
        .unwrap_or(cases.len());
    ctx.clear_static_callable_locals();
    ctx.loop_stack.push(LoopFrame {
        break_block: exit,
        continue_block: exit,
        cleanup: None,
    });
    for index in 0..=cases.len() {
        if default.is_some() && default_index == index {
            ctx.builder.position_at_end(default_block);
            if let Some(default) = default {
                lower_block(ctx, default);
            }
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(ctx, blocks.get(index).copied().unwrap_or(exit));
            }
            ctx.clear_static_callable_locals();
        }
        if let Some((_, body)) = cases.get(index) {
            ctx.builder.position_at_end(blocks[index]);
            lower_block(ctx, body);
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(
                    ctx,
                    switch_next_body_block(index + 1, blocks, default_index, default_block, exit),
                );
            }
            ctx.clear_static_callable_locals();
        }
    }
    if default.is_none() {
        ctx.builder.position_at_end(default_block);
        branch_to(ctx, exit);
    }
    ctx.loop_stack.pop();
    ctx.builder.position_at_end(exit);
    ctx.clear_static_callable_locals();
}

/// Returns the source-order insertion point for a non-empty switch default body.
fn switch_default_source_index(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &[Stmt],
) -> Option<usize> {
    if cases.is_empty() {
        return Some(0);
    }
    let default_start = default.first()?.span;
    if default_start == Span::dummy() {
        return None;
    }
    let mut default_index = 0;
    for (conditions, _) in cases {
        let case_start = conditions.first()?.span;
        if case_start == Span::dummy() {
            return None;
        }
        if span_is_before(case_start, default_start) {
            default_index += 1;
        }
    }
    Some(default_index)
}

/// Returns the block that follows one source-ordered switch body.
fn switch_next_body_block(
    next_index: usize,
    blocks: &[BlockId],
    default_index: usize,
    default_block: BlockId,
    exit: BlockId,
) -> BlockId {
    if default_index == next_index {
        default_block
    } else {
        blocks.get(next_index).copied().unwrap_or(exit)
    }
}

/// Returns true when `span` appears before `pivot` in the same source file.
fn span_is_before(span: Span, pivot: Span) -> bool {
    span.line < pivot.line || (span.line == pivot.line && span.col < pivot.col)
}

/// Lowers include/require statements through a high-level runtime call.
fn lower_include(
    ctx: &mut LoweringContext<'_, '_>,
    path: &Expr,
    once: bool,
    required: bool,
    span: Span,
) {
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
    ctx.clear_static_callable_locals();
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
fn lower_include_once_guard(
    ctx: &mut LoweringContext<'_, '_>,
    label: &str,
    body: &[Stmt],
    span: Span,
) {
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
    let body_block = ctx
        .builder
        .create_named_block("include_once_body", Vec::new());
    let after_block = ctx
        .builder
        .create_named_block("include_once_after", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: should_run,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: after_block,
        else_args: Vec::new(),
    });
    ctx.clear_static_callable_locals();
    ctx.builder.position_at_end(body_block);
    lower_block(ctx, body);
    branch_to(ctx, after_block);
    ctx.builder.position_at_end(after_block);
    ctx.clear_static_callable_locals();
}

/// Lowers a throwing statement into a terminator.
fn lower_throw(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) {
    let value = lower_expr(ctx, expr);
    // The in-flight exception cell owns one reference to the thrown object. Throwing
    // an owning temporary (e.g. `throw new E()`, `throw f()`) transfers that
    // reference; throwing a value that still leaves a local slot as owner — a
    // PhpLocal/StaticLocal heap load such as a rethrown catch variable (`throw $e`)
    // — must retain it, so the local's own release (rebind or epilogue) stays
    // balanced with the catch-side release of the in-flight reference (issue #448).
    // Main classifies concrete object local loads as owning temporaries for
    // provisional unbox-release tracking; that must not be mistaken for a transfer.
    let transferable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let value = if transferable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(expr.span))
    };
    terminate_throw(ctx, value.value);
}

/// Lowers a `try`/`catch` statement into a runtime handler and explicit catch-dispatch blocks.
fn lower_try(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: Option<&[Stmt]>,
    span: Span,
) {
    if let Some(finally_body) = finally_body {
        lower_try_with_finally(ctx, try_body, catches, finally_body, span);
        return;
    }

    lower_try_catch(ctx, try_body, catches, span);
}

/// Lowers a `try`/`catch` statement without a `finally` block.
fn lower_try_catch(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    span: Span,
) {
    let handler_block = ctx
        .builder
        .create_named_block("try.catch_dispatch", Vec::new());
    let after_block = ctx.builder.create_named_block("try.after", Vec::new());
    let handler_token = handler_block.as_raw() as i64;
    let mut after_reachable = false;

    ctx.emit_void(
        Op::TryPushHandler,
        Vec::new(),
        Some(Immediate::I64(handler_token)),
        Op::TryPushHandler.default_effects(),
        Some(span),
    );
    lower_block(ctx, try_body);
    if !ctx.builder.insertion_block_is_terminated() {
        emit_try_pop_handler(ctx, handler_token, span);
        branch_to(ctx, after_block);
        after_reachable = true;
    }

    ctx.builder.position_at_end(handler_block);
    ctx.clear_static_callable_locals();
    emit_try_pop_handler(ctx, handler_token, span);
    after_reachable |= lower_catch_dispatch(ctx, catches, after_block, span);
    ctx.builder.position_at_end(after_block);
    if !after_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
    ctx.clear_static_callable_locals();
}

/// Lowers `try`/`catch`/`finally` using duplicated finalizer bodies for explicit exits.
fn lower_try_with_finally(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &[Stmt],
    span: Span,
) {
    if catches.is_empty() {
        lower_try_finally_without_catches(ctx, try_body, finally_body);
    } else {
        lower_try_catch_finally(ctx, try_body, catches, finally_body, span);
    }
}

/// Lowers a `try`/`finally` statement with no catch clauses.
fn lower_try_finally_without_catches(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    finally_body: &[Stmt],
) {
    let depth = push_finally_frame(ctx, finally_body, true, None);
    lower_block(ctx, try_body);
    pop_finally_frame_if_active(ctx, depth);
    if !ctx.builder.insertion_block_is_terminated() {
        lower_block(ctx, finally_body);
    }
}

/// Lowers a `try`/`catch`/`finally` statement while preserving catch-before-finally order.
fn lower_try_catch_finally(
    ctx: &mut LoweringContext<'_, '_>,
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &[Stmt],
    span: Span,
) {
    let handler_block = ctx
        .builder
        .create_named_block("try.catch_dispatch", Vec::new());
    let after_block = ctx.builder.create_named_block("try.after", Vec::new());
    let handler_token = handler_block.as_raw() as i64;
    let mut after_reachable = false;

    ctx.emit_void(
        Op::TryPushHandler,
        Vec::new(),
        Some(Immediate::I64(handler_token)),
        Op::TryPushHandler.default_effects(),
        Some(span),
    );
    let depth = push_finally_frame(ctx, finally_body, false, Some((handler_token, span)));
    lower_block(ctx, try_body);
    pop_finally_frame_if_active(ctx, depth);
    if !ctx.builder.insertion_block_is_terminated() {
        emit_try_pop_handler(ctx, handler_token, span);
        lower_block(ctx, finally_body);
        if !ctx.builder.insertion_block_is_terminated() {
            branch_to(ctx, after_block);
            after_reachable = true;
        }
    }

    ctx.builder.position_at_end(handler_block);
    ctx.clear_static_callable_locals();
    emit_try_pop_handler(ctx, handler_token, span);
    after_reachable |=
        lower_catch_dispatch_with_finally(ctx, catches, after_block, finally_body, span);
    ctx.builder.position_at_end(after_block);
    if !after_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
    ctx.clear_static_callable_locals();
}

/// Emits the runtime cleanup for a pushed try/catch handler.
fn emit_try_pop_handler(ctx: &mut LoweringContext<'_, '_>, handler_token: i64, span: Span) {
    ctx.emit_void(
        Op::TryPopHandler,
        Vec::new(),
        Some(Immediate::I64(handler_token)),
        Op::TryPopHandler.default_effects(),
        Some(span),
    );
}

/// Lowers ordered catch matching and reports whether any catch reaches the post-try join.
fn lower_catch_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    catches: &[CatchClause],
    after_block: BlockId,
    span: Span,
) -> bool {
    let mut after_reachable = false;
    for catch in catches {
        let catch_body = ctx.builder.create_named_block("try.catch_body", Vec::new());
        let next_catch = ctx.builder.create_named_block("try.catch_next", Vec::new());
        lower_catch_match(ctx, catch, catch_body, next_catch, span);
        ctx.builder.position_at_end(catch_body);
        lower_catch_bind(ctx, catch, span);
        lower_block(ctx, &catch.body);
        if !ctx.builder.insertion_block_is_terminated() {
            branch_to(ctx, after_block);
            after_reachable = true;
        }
        ctx.clear_static_callable_locals();
        ctx.builder.position_at_end(next_catch);
    }

    let current = lower_current_exception(ctx, span);
    ctx.builder.terminate(Terminator::Throw {
        value: current.value,
    });
    after_reachable
}

/// Lowers catch dispatch with finalizers and reports whether any catch reaches the post-try join.
fn lower_catch_dispatch_with_finally(
    ctx: &mut LoweringContext<'_, '_>,
    catches: &[CatchClause],
    after_block: BlockId,
    finally_body: &[Stmt],
    span: Span,
) -> bool {
    let mut after_reachable = false;
    for catch in catches {
        let catch_body = ctx.builder.create_named_block("try.catch_body", Vec::new());
        let next_catch = ctx.builder.create_named_block("try.catch_next", Vec::new());
        lower_catch_match(ctx, catch, catch_body, next_catch, span);
        ctx.builder.position_at_end(catch_body);
        lower_catch_bind(ctx, catch, span);
        let depth = push_finally_frame(ctx, finally_body, true, None);
        lower_block(ctx, &catch.body);
        pop_finally_frame_if_active(ctx, depth);
        if !ctx.builder.insertion_block_is_terminated() {
            lower_block(ctx, finally_body);
            if !ctx.builder.insertion_block_is_terminated() {
                branch_to(ctx, after_block);
                after_reachable = true;
            }
        }
        ctx.clear_static_callable_locals();
        ctx.builder.position_at_end(next_catch);
    }

    let current = lower_current_exception(ctx, span);
    lower_block(ctx, finally_body);
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Throw {
            value: current.value,
        });
    }
    after_reachable
}

/// Emits the match tests for one catch clause and branches to body or next clause.
fn lower_catch_match(
    ctx: &mut LoweringContext<'_, '_>,
    catch: &CatchClause,
    catch_body: BlockId,
    next_catch: BlockId,
    span: Span,
) {
    if catch.exception_types.is_empty() {
        branch_to(ctx, next_catch);
        return;
    }

    for (idx, catch_type) in catch.exception_types.iter().enumerate() {
        let mismatch = if idx + 1 == catch.exception_types.len() {
            next_catch
        } else {
            ctx.builder
                .create_named_block("try.catch_type_next", Vec::new())
        };
        let current = lower_current_exception(ctx, span);
        let data = ctx.intern_class_name(catch_type.as_str());
        let matched = ctx.emit_value(
            Op::InstanceOf,
            vec![current.value],
            Some(Immediate::Data(data)),
            PhpType::Bool,
            Op::InstanceOf.default_effects(),
            Some(span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: matched.value,
            then_target: catch_body,
            then_args: Vec::new(),
            else_target: mismatch,
            else_args: Vec::new(),
        });
        if idx + 1 != catch.exception_types.len() {
            ctx.builder.position_at_end(mismatch);
        }
    }
}

/// Emits the current exception value as an object-typed SSA value.
fn lower_current_exception(ctx: &mut LoweringContext<'_, '_>, span: Span) -> LoweredValue {
    ctx.emit_value(
        Op::CatchCurrent,
        Vec::new(),
        None,
        PhpType::Object("Throwable".to_string()),
        Op::CatchCurrent.default_effects(),
        Some(span),
    )
}

/// Takes and clears the active exception for a matched catch clause, then stores the
/// owned result through the ordinary variable-storage planner. This keeps local,
/// global, static, and reference-cell destinations consistent while preserving the
/// single in-flight reference transferred by the runtime (issue #448). A variable-less
/// catch consumes the reference through a hidden owned temporary so it follows the
/// same lifecycle instead of leaking.
fn lower_catch_bind(ctx: &mut LoweringContext<'_, '_>, catch: &CatchClause, span: Span) {
    let php_type = catch_variable_type(catch);
    let variable = match catch.variable.as_ref() {
        Some(variable) => variable.clone(),
        None => ctx.declare_owned_hidden_temp(php_type.clone()),
    };
    let caught = ctx.emit_owned_value(
        Op::CatchBind,
        Vec::new(),
        None,
        php_type.clone(),
        Op::CatchBind.default_effects(),
        Some(span),
    );
    ctx.store_local(&variable, caught, php_type, Some(span));
}

/// Returns the local type to use for a catch variable.
fn catch_variable_type(catch: &CatchClause) -> PhpType {
    if catch.exception_types.len() == 1 {
        return PhpType::Object(
            catch.exception_types[0]
                .trim_start_matches('\\')
                .to_string(),
        );
    }
    PhpType::Object("Throwable".to_string())
}

/// Lowers a `break` terminator.
fn lower_break(ctx: &mut LoweringContext<'_, '_>, level: usize) {
    let Some(frame) = loop_target(ctx, level) else {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    };
    terminate_branch(ctx, frame.break_block, loop_cleanup_count_for_branch(level));
}

/// Lowers a `continue` terminator.
fn lower_continue(ctx: &mut LoweringContext<'_, '_>, level: usize) {
    let Some(frame) = loop_target(ctx, level) else {
        ctx.builder.terminate(Terminator::Unreachable);
        return;
    };
    terminate_branch(
        ctx,
        frame.continue_block,
        loop_cleanup_count_for_branch(level),
    );
}

/// Lowers a return statement using the current function return contract.
fn lower_return(ctx: &mut LoweringContext<'_, '_>, value_expr: Option<&Expr>, span: Span) {
    // A by-reference-returning function hands the caller the ref-cell pointer of the
    // returned property (`function &f() { return $obj->prop; }`), so `$x = &f()` aliases
    // it. The cell pointer is materialized as the declared return type so the ABI return
    // convention matches the caller's expectation for pointer-sized property types.
    if ctx.by_ref_return {
        if let Some(Expr { kind: ExprKind::PropertyAccess { object, property }, .. }) = value_expr {
            let object = lower_expr(ctx, object);
            let data = ctx.intern_string(property);
            let result_ty = ctx.return_php_type.clone();
            let cell_ptr = ctx.emit_value(
                Op::LoadPropRefCell,
                vec![object.value],
                Some(Immediate::Data(data)),
                result_ty,
                Op::LoadPropRefCell.default_effects(),
                Some(span),
            );
            terminate_return(ctx, Some(cell_ptr.value));
            return;
        }
    }
    if ctx.return_type == IrType::Void {
        if let Some(value_expr) = value_expr {
            lower_expr(ctx, value_expr);
        }
        terminate_return(ctx, None);
        return;
    }
    let value = if let Some(value_expr) = value_expr {
        lower_return_expr(ctx, value_expr)
    } else {
        emit_null_value(ctx, Some(span))
    };
    let value = coerce_to_return_type(ctx, value, Some(span));
    let value = acquire_borrowed_return_value(ctx, value, span);
    let value = acquire_returned_this(ctx, value_expr, value, span);
    let value = persist_scratch_return_string(ctx, value, span);
    terminate_return(ctx, Some(value.value));
}

/// Lowers a return expression with contextual array-literal element storage when available.
fn lower_return_expr(ctx: &mut LoweringContext<'_, '_>, value_expr: &Expr) -> LoweredValue {
    if matches!(value_expr.kind, ExprKind::ArrayLiteral(_)) {
        if let PhpType::Array(elem_ty) = ctx.return_php_type.codegen_repr() {
            return lower_array_literal_with_expected_type(ctx, value_expr, *elem_ty);
        }
    }
    lower_expr(ctx, value_expr)
}

/// Acquires the receiver when a method does `return $this`.
///
/// `$this` is a borrowed reference to the receiver the caller still owns. A return
/// value is handed to the caller as owned, so without an extra reference the
/// caller's release of the (often discarded, as in fluent `$obj->setX(...)->setY()`)
/// result drops the object's refcount to zero and runs its destructor while the
/// original binding is still live — a use-after-free for any class with a
/// destructor. Incrementing the refcount here balances that release.
fn acquire_returned_this(
    ctx: &mut LoweringContext<'_, '_>,
    value_expr: Option<&Expr>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !matches!(value_expr.map(|expr| &expr.kind), Some(ExprKind::This)) {
        return value;
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
}

/// Copies scratch-backed string results before they cross a function boundary.
fn persist_scratch_return_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if value.ir_type != IrType::Str {
        return value;
    }
    let Some(op) = ctx.builder.value_defining_op(value.value) else {
        return value;
    };
    if !string_op_uses_scratch_storage(op) {
        return value;
    }
    ctx.emit_value(
        Op::StrPersist,
        vec![value.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(span),
    )
}

/// Acquires return values read from heap containers before local cleanup runs.
///
/// Function-static slots are included: the slot keeps owning its boxed value across
/// calls, so `return $static_local` must hand the caller an extra reference — the
/// caller releases call results after consuming them, and without the retain that
/// release frees the box the slot still points to.
fn acquire_borrowed_return_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if ctx.value_is_owning_temporary(value) {
        return value;
    }
    let php_type = ctx.builder.value_php_type(value.value);
    if !Ownership::php_type_needs_lifetime_tracking(&php_type) {
        return value;
    }
    if !matches!(
        ctx.builder.value_defining_op(value.value),
        Some(
            Op::ArrayGet
                | Op::HashGet
                | Op::HashGetSilent
                | Op::PropGet
                | Op::DynamicPropGet
                | Op::NullsafePropGet
                | Op::LoadStaticLocal
        )
    ) {
        return value;
    }
    crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
}

/// Terminates with a return after running active finally bodies from inner to outer.
fn terminate_return(ctx: &mut LoweringContext<'_, '_>, value: Option<crate::ir::ValueId>) {
    if run_innermost_finally(ctx, false) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_return(ctx, value);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, ctx.loop_stack.len());
    ctx.emit_eval_scope_finalizer(None);
    ctx.builder.terminate(Terminator::Return { value });
}

/// Terminates with a branch after running active finally bodies from inner to outer.
fn terminate_branch(ctx: &mut LoweringContext<'_, '_>, target: BlockId, loop_cleanup_count: usize) {
    if run_innermost_finally(ctx, false) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_branch(ctx, target, loop_cleanup_count);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, loop_cleanup_count);
    ctx.builder.terminate(Terminator::Br {
        target,
        args: Vec::new(),
    });
}

/// Terminates with a throw after running finally bodies that apply to uncaught throws.
fn terminate_throw(ctx: &mut LoweringContext<'_, '_>, value: crate::ir::ValueId) {
    if run_innermost_finally(ctx, true) {
        if !ctx.builder.insertion_block_is_terminated() {
            terminate_throw(ctx, value);
        }
        return;
    }
    emit_innermost_loop_cleanups(ctx, ctx.loop_stack.len());
    ctx.builder.terminate(Terminator::Throw { value });
}

/// Lowers a statically-decided access violation as a catchable `Error` throw.
///
/// Builds a synthetic `new Error($message)` expression at `span`, lowers it to an
/// EIR object value, then terminates the current block with a throw. Mirrors PHP,
/// which raises these conditions as catchable `Error` exceptions instead of fatal
/// compile-time rejections. Used in statement positions where no value is needed.
pub(crate) fn lower_throw_access_error(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
        },
        span,
    );
    let error_value = crate::ir_lower::expr::lower_expr(ctx, &error_expr);
    terminate_throw(ctx, error_value.value);
}

/// Lowers a statically-decided access violation as a catchable `Error` throw in
/// expression position and returns a placeholder null value.
///
/// Builds a synthetic `new Error($message)` expression at `span`, lowers it to an
/// EIR object value, emits `Op::ThrowException`, then returns a null placeholder so
/// the surrounding expression lowering keeps producing well-formed EIR after the
/// (unreachable) throw.
pub(crate) fn lower_throw_access_error_expr(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
) -> LoweredValue {
    let error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![Expr::new(ExprKind::StringLiteral(message.to_string()), span)],
        },
        span,
    );
    let error_value = crate::ir_lower::expr::lower_expr(ctx, &error_expr);
    ctx.emit_void(
        Op::ThrowException,
        vec![error_value.value],
        None,
        Op::ThrowException.default_effects(),
        Some(span),
    );
    LoweredValue {
        value: ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                IrType::I64,
                PhpType::Void,
                Ownership::NonHeap,
                Op::ConstNull.default_effects(),
                Some(span),
            )
            .expect("const_null produces a value"),
        ir_type: IrType::I64,
    }
}

/// Returns how many inner loop cleanups a multi-level branch skips.
fn loop_cleanup_count_for_branch(level: usize) -> usize {
    level.max(1).saturating_sub(1)
}

/// Emits cleanup for the innermost active loops that will not reach their exit block.
fn emit_innermost_loop_cleanups(ctx: &mut LoweringContext<'_, '_>, count: usize) {
    let frames = ctx
        .loop_stack
        .iter()
        .rev()
        .take(count)
        .copied()
        .collect::<Vec<_>>();
    for frame in frames {
        if let Some(cleanup) = frame.cleanup {
            crate::ir_lower::ownership::release_if_owned(ctx, cleanup.value, Some(cleanup.span));
        }
    }
}

/// Runs and removes the innermost applicable finally frame.
fn run_innermost_finally(ctx: &mut LoweringContext<'_, '_>, is_throw: bool) -> bool {
    let Some(frame) = ctx.finally_stack.last() else {
        return false;
    };
    if is_throw && !frame.run_on_throw {
        return false;
    }
    let frame = ctx
        .finally_stack
        .pop()
        .expect("finally frame disappeared after last() check");
    if let Some((handler_token, span)) = frame.handler_cleanup {
        emit_try_pop_handler(ctx, handler_token, span);
    }
    lower_block(ctx, &frame.body);
    true
}

/// Pushes a finalizer and returns the stack depth before the push.
fn push_finally_frame(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
    run_on_throw: bool,
    handler_cleanup: Option<(i64, Span)>,
) -> usize {
    let depth = ctx.finally_stack.len();
    ctx.finally_stack.push(FinallyFrame {
        body: body.to_vec(),
        run_on_throw,
        handler_cleanup,
    });
    depth
}

/// Removes a finalizer when the protected body fell through normally.
fn pop_finally_frame_if_active(ctx: &mut LoweringContext<'_, '_>, depth: usize) {
    if ctx.finally_stack.len() > depth {
        ctx.finally_stack.pop();
    }
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
fn lower_list_unpack_index(
    ctx: &mut LoweringContext<'_, '_>,
    index: usize,
    span: Span,
) -> LoweredValue {
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
///
/// Indexed-array reads use `Op::ArrayGet`, whose runtime OOB fallback produces a
/// null in the result shape (tagged scalar or sentinel). To preserve that null
/// for `??` and `IsNull`, the destination type is widened the same way as a
/// direct array index read (see `array_access_element_result_type`). Without
/// this widening an `Array(Int)` element would lower to `PhpType::Int`, whose
/// null fallback is the in-band `NULL_SENTINEL` i64, and `$b ?? 'n'` would see
/// a non-null integer instead of null for missing keys (#337).
fn list_unpack_item_type(ctx: &LoweringContext<'_, '_>, source: crate::ir::ValueId) -> PhpType {
    let item_type = match ctx.builder.value_php_type(source).codegen_repr() {
        PhpType::Array(elem_ty) => array_access_element_result_type(elem_ty.codegen_repr()),
        PhpType::AssocArray { value, .. } => {
            array_access_element_result_type(value.codegen_repr())
        }
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

/// Normalizes indexed-array write payloads to storage shapes Phase 04 can lower.
fn normalize_array_write_element_type(item_type: PhpType) -> PhpType {
    let item_type = normalize_materialized_element_type(item_type);
    if item_type.is_refcounted() && !matches!(item_type, PhpType::Str) {
        PhpType::Mixed
    } else {
        item_type
    }
}

/// Declares global aliases in the local slot table.
fn lower_global(ctx: &mut LoweringContext<'_, '_>, vars: &[String]) {
    for var in vars {
        let php_type = ctx.global_alias_type(var);
        ctx.declare_local_with_kind(var, php_type, LocalKind::GlobalAlias);
    }
}

/// Lowers a static local variable initialization.
fn lower_static_var(ctx: &mut LoweringContext<'_, '_>, name: &str, init: &Expr, span: Span) {
    let value = lower_expr(ctx, init);
    let slot = ctx.declare_local_with_kind(
        name,
        ctx.builder.value_php_type(value.value),
        LocalKind::StaticLocal,
    );
    ctx.builder.emit_with_effects(
        Op::InitStaticLocal,
        vec![value.value],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
        Op::InitStaticLocal.default_effects(),
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
    // A statically-decided readonly-property write outside the declaring
    // constructor raises a catchable `Error` in PHP rather than a compile-time
    // error, but the object and RHS expressions must still be evaluated first.
    let throw_access_message = ctx.throw_access_sites.get(&span).and_then(|info| {
        if let ThrowAccessKind::ReadonlyProperty { class_name, property } = &info.kind {
            Some(format!("Cannot modify readonly property {}::${}", class_name, property))
        } else {
            None
        }
    });
    let object = lower_expr(ctx, object);
    let value_expr = value;
    let lowered_value = lower_expr(ctx, value_expr);
    if let Some(message) = throw_access_message {
        if ctx.value_is_owning_temporary(object) {
            crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
        }
        if ctx.value_is_owning_temporary(lowered_value) {
            crate::ir_lower::ownership::release_if_owned(ctx, lowered_value, Some(span));
        }
        lower_throw_access_error(ctx, &message, span);
        return;
    }
    let value = contextualize_property_array_assignment(
        ctx,
        object.value,
        property,
        lowered_value,
        value_expr,
        span,
    );
    if magic_set_receiver_has_method(ctx, object.value, property) {
        lower_magic_property_set(ctx, object.value, property, value, span);
        return;
    }
    // Route a write to a set-hooked property to its `__propset_<p>($value)` accessor, except inside
    // that property's own accessor where `$this->prop = v` must write the raw backing slot.
    if set_hook_receiver_has_accessor(ctx, object.value, property)
        && !ctx.in_own_property_accessor(property)
    {
        lower_property_hook_set(ctx, object.value, property, value, span);
        return;
    }
    let data = ctx.intern_string(property);
    ctx.emit_void(
        Op::PropSet,
        vec![object.value, value.value],
        Some(Immediate::Data(data)),
        Op::PropSet.default_effects(),
        Some(span),
    );
    if let Some(property_ty) = object_property_type(ctx, object.value, property) {
        release_property_assignment_source_after_retaining_store(ctx, &property_ty, value, span);
    }
}

/// Returns true when a property write should dispatch to `__set`.
fn magic_set_receiver_has_method(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return false;
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(normalized) else {
        return false;
    };
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
    {
        return false;
    }
    class_info.methods.contains_key(&php_symbol_key("__set"))
}

/// Lowers an undeclared property write to a normal `__set` instance-method call.
fn lower_magic_property_set(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) {
    let property_data = ctx.intern_string(property);
    let property_name = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(property_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(span),
    );
    let method_data = ctx.intern_string("__set");
    ctx.emit_void(
        Op::MethodCall,
        vec![object, property_name.value, value.value],
        Some(Immediate::Data(method_data)),
        Op::MethodCall.default_effects(),
        Some(span),
    );
    release_magic_set_value_after_call(ctx, value, span);
}

/// Releases an owning RHS temporary after the `__set` call has consumed it.
fn release_magic_set_value_after_call(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns true when the runtime class of `object` declares a `__propset_<property>` set-hook
/// accessor, meaning a write to `property` should be routed through it.
fn set_hook_receiver_has_accessor(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return false;
    };
    let normalized = class_name.trim_start_matches('\\');
    ctx.classes.get(normalized).is_some_and(|info| {
        info.methods
            .contains_key(&php_symbol_key(&property_hook_set_method(property)))
    })
}

/// Lowers a write to a set-hooked property as a call to its `__propset_<p>($value)` accessor,
/// passing the assigned value as the single argument and releasing it if it was an owning temporary.
fn lower_property_hook_set(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) {
    let method_data = ctx.intern_string(&property_hook_set_method(property));
    ctx.emit_void(
        Op::MethodCall,
        vec![object, value.value],
        Some(Immediate::Data(method_data)),
        Op::MethodCall.default_effects(),
        Some(span),
    );
    release_magic_set_value_after_call(ctx, value, span);
}

/// Converts array literals to hash storage when a declared object property requires assoc storage.
fn contextualize_property_array_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    lowered: LoweredValue,
    value_expr: &Expr,
    span: Span,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(lowered.value);
    if !matches!(value_expr.kind, ExprKind::ArrayLiteral(_)) {
        return lowered;
    }
    if !matches!(php_type.codegen_repr(), PhpType::Array(_)) {
        return lowered;
    }
    let Some(contextual_ty) = object_property_type(ctx, object, property) else {
        return lowered;
    };
    let contextual_ty = contextual_ty.codegen_repr();
    if !matches!(contextual_ty, PhpType::AssocArray { .. }) {
        return lowered;
    }
    ctx.emit_value(
        Op::ArrayToHash,
        vec![lowered.value],
        None,
        contextual_ty,
        Op::ArrayToHash.default_effects(),
        Some(span),
    )
}

/// Lowers a static property write.
///
/// A static property outlives the enclosing scope, so it must hold its own
/// reference to a refcounted value. There are two storage disciplines, matched
/// to what the codegen store actually does:
///
/// - **Boxing store** (a Mixed/Union slot receiving a non-Mixed value, e.g.
///   `Class::$h = new C()`): codegen boxes the value with `__rt_mixed_from_value`,
///   which takes its *own* retained reference to the child. The slot therefore
///   keeps a reference independent of the source, so an owning temporary must be
///   *released* after the store (its reference is not the one the slot holds), and
///   a borrowed source must be left untouched. Acquiring here would leak the extra
///   reference on top of the box's retained one.
/// - **Moving store** (every other case: concrete-typed slot, or a Mixed→Mixed
///   move): the store consumes (moves) its value operand. An owning temporary is
///   moved in as-is, but a *borrowed* value (a parameter, local, or container read)
///   must be `Acquire`d first. Without this, storing a borrowed `Mixed`
///   (e.g. `Class::$h = $handler` where `$handler` is a `?SessionHandlerInterface`
///   parameter) leaves the property dangling once the borrow's owner releases its
///   reference, so a later read dispatches on freed memory (a fatal "on null").
fn lower_static_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    let value = lower_expr(ctx, value);
    if static_property_store_retains_independent_value(ctx, receiver, property, value) {
        store_static_property(ctx, receiver, property, value.value, span);
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        return;
    }
    let value = if ctx.value_is_owning_temporary(value) {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    store_static_property(ctx, receiver, property, value.value, span);
}

/// Returns true when codegen gives the static-property slot an independently retained value.
///
/// This covers both concrete values boxed into Mixed/Union slots and boxed Mixed values
/// unboxed into object slots. Both backend paths retain the stored child independently,
/// so borrowed sources need no `Acquire` and owning temporary sources are released after
/// the store. Unknown metadata conservatively keeps the moving-store discipline.
fn static_property_store_retains_independent_value(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: LoweredValue,
) -> bool {
    let Some(slot_ty) = static_property_type(ctx, receiver, property) else {
        return false;
    };
    let value_ty = ctx.builder.value_php_type(value.value);
    let slot_ty = slot_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    let boxes_into_mixed = matches!(slot_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    let unboxes_into_object = matches!(slot_ty, PhpType::Object(_))
        && matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    boxes_into_mixed || unboxes_into_object
}

/// Lowers `Class::$prop[] = value`.
fn lower_static_property_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    span: Span,
) {
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_indexed_array_type)
    {
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::ArrayPush,
            vec![property_value.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }

    let property_value = load_static_property(ctx, receiver, property, span);
    let value = lower_expr(ctx, value);
    if static_property_may_be_eval_dynamic(ctx, receiver) {
        ctx.emit_void(
            Op::MixedArrayAppend,
            vec![property_value.value, value.value],
            None,
            Op::MixedArrayAppend.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }
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
    if let Some(property_ty) =
        static_property_type(ctx, receiver, property).filter(is_indexed_array_type)
    {
        let array_ty = property_ty.clone();
        let property_value = load_static_property_as(ctx, receiver, property, property_ty, span);
        let index = lower_expr(ctx, index);
        let value = lower_expr(ctx, value);
        let value = coerce_indexed_array_set_value(ctx, &array_ty, value, Some(span));
        ctx.emit_void(
            Op::ArraySet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::ArraySet.default_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }

    let property_value = if let Some(property_ty) = static_property_type(ctx, receiver, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty))
    {
        load_static_property_as(ctx, receiver, property, property_ty, span)
    } else {
        load_static_property(ctx, receiver, property, span)
    };
    let index = lower_expr(ctx, index);
    let value = lower_expr(ctx, value);
    if static_property_may_be_eval_dynamic(ctx, receiver) {
        ctx.emit_void(
            Op::RuntimeCall,
            vec![property_value.value, index.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        store_static_property(ctx, receiver, property, property_value.value, span);
        return;
    }
    ctx.emit_void(
        Op::RuntimeCall,
        vec![property_value.value, index.value, value.value],
        None,
        effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Returns true when a named static-property receiver may resolve through eval metadata.
fn static_property_may_be_eval_dynamic(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> bool {
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    ctx.has_eval_barrier()
        && !ctx
            .classes
            .contains_key(class_name.as_str().trim_start_matches('\\'))
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
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::ArrayPush,
            vec![property_value.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(span),
        );
        release_property_array_insert_value_after_retain(ctx, &property_ty, value, span);
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }

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
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_indexed_array_type)
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        let index = lower_expr(ctx, index);
        let value = lower_expr(ctx, value);
        let value = coerce_indexed_array_set_value(ctx, &property_ty, value, Some(span));
        ctx.emit_void(
            Op::ArraySet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::ArraySet.default_effects(),
            Some(span),
        );
        release_property_array_insert_value_after_retain(ctx, &property_ty, value, span);
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }
    if let Some(property_ty) =
        object_property_type(ctx, object.value, property).filter(is_assoc_array_type)
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty.clone(),
            Op::PropGet.default_effects(),
            Some(span),
        );
        let property_value =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, property_value, Some(span));
        let index = lower_expr(ctx, index);
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::HashSet,
            vec![property_value.value, index.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(span),
        );
        release_property_array_insert_value_after_retain(ctx, &property_ty, value, span);
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, property_value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
        release_rewritten_property_value_after_retaining_store(
            ctx,
            &property_ty,
            property_value,
            span,
        );
        return;
    }

    if let Some(property_ty) = object_property_type(ctx, object.value, property)
        .filter(|ty| type_satisfies_array_access_for_ir(ctx, ty))
    {
        let data = ctx.intern_string(property);
        let property_value = ctx.emit_value(
            Op::PropGet,
            vec![object.value],
            Some(Immediate::Data(data)),
            property_ty,
            Op::PropGet.default_effects(),
            Some(span),
        );
        let index = lower_expr(ctx, index);
        let value = lower_expr(ctx, value);
        ctx.emit_void(
            Op::RuntimeCall,
            vec![property_value.value, index.value, value.value],
            None,
            effects_lookup::runtime_effects(),
            Some(span),
        );
        return;
    }

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

/// Releases a temporary assigned into an object property after `PropSet` retains or boxes it.
fn release_property_assignment_source_after_retaining_store(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
) {
    if !ctx.value_is_owning_temporary(value) {
        return;
    }
    if !property_store_keeps_independent_ref(property_ty, &ctx.builder.value_php_type(value.value))
    {
        return;
    }
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
}

/// Releases an element temporary after a property-array write retains it for storage.
fn release_property_array_insert_value_after_retain(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
    span: Span,
) {
    let Some(elem_ty) = indexed_property_array_element_type(property_ty) else {
        return;
    };
    if matches!(elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Callable) {
        return;
    }
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Releases the loaded property value after rewriting it through a retaining `PropSet`.
fn release_rewritten_property_value_after_retaining_store(
    ctx: &mut LoweringContext<'_, '_>,
    property_ty: &PhpType,
    property_value: LoweredValue,
    span: Span,
) {
    if property_ty.codegen_repr().is_refcounted() {
        crate::ir_lower::ownership::release_if_owned(ctx, property_value, Some(span));
    }
}

/// Returns whether a property store creates a distinct retained/boxed owner for the value.
fn property_store_keeps_independent_ref(property_ty: &PhpType, value_ty: &PhpType) -> bool {
    let property_ty = property_ty.codegen_repr();
    let value_ty = value_ty.codegen_repr();
    if matches!((&property_ty, &value_ty), (PhpType::Mixed, PhpType::Mixed)) {
        return false;
    }
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_))
        && matches!(property_ty, PhpType::Int | PhpType::Bool | PhpType::Float)
    {
        return true;
    }
    if matches!(property_ty, PhpType::Str) {
        return true;
    }
    property_ty.is_refcounted()
}

/// Returns the element type for property arrays that use retaining indexed/hash helpers.
fn indexed_property_array_element_type(property_ty: &PhpType) -> Option<PhpType> {
    match property_ty.codegen_repr() {
        PhpType::Array(elem_ty) => Some(elem_ty.codegen_repr()),
        PhpType::AssocArray { value, .. } => Some(value.codegen_repr()),
        _ => None,
    }
}

/// Emits a no-op marker for declaration-only or frontend-only statements.
fn lower_noop(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    ctx.emit_void(
        Op::Nop,
        Vec::new(),
        None,
        Op::Nop.default_effects(),
        Some(span),
    );
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
        ctx.builder.terminate(Terminator::Br {
            target,
            args: Vec::new(),
        });
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

/// Returns true when a lowered index value is a boxed `Mixed`/`Union` cell that
/// may hold either an integer or a string array key (e.g. a foreach loop key,
/// which `Op::IterCurrentKey` always produces as Mixed). Such writes go through
/// `Op::ArraySetMixedKey` so the key tag is dispatched at runtime instead of
/// coercing it to int (which would collapse a string key onto int 0).
fn index_is_boxed_mixed_key(ir_type: IrType) -> bool {
    matches!(
        ir_type,
        IrType::Heap(crate::ir::IrHeapKind::Mixed)
            | IrType::Heap(crate::ir::IrHeapKind::Union)
    )
}

/// Returns true when the index expression is a foreach loop key known to hold an
/// integer at runtime (its source was a concretely-indexed array), so the
/// destination write can keep the indexed `ArraySet` path with int coercion
/// instead of promoting to a hash. See `LoweringContext::mark_foreach_int_key`.
fn index_is_foreach_int_key(ctx: &LoweringContext<'_, '_>, index: &Expr) -> bool {
    if let ExprKind::Variable(name) = &index.kind {
        return ctx.is_foreach_int_key(name);
    }
    false
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
    LoweredValue {
        value,
        ir_type: IrType::I64,
    }
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
    LoweredValue {
        value,
        ir_type: IrType::I64,
    }
}

/// Coerces a value to the current function return storage type when needed.
fn coerce_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    if let Some(value) = coerce_container_to_return_type(ctx, value, span) {
        return value;
    }
    if value.ir_type == ctx.return_type {
        return value;
    }
    match ctx.return_type {
        IrType::I64 => coerce_return_scalar_source(ctx, value, span, coerce_to_int),
        IrType::F64 => coerce_return_scalar_source(ctx, value, span, coerce_to_float),
        IrType::Str => coerce_return_scalar_source(ctx, value, span, coerce_to_string),
        IrType::TaggedScalar => {
            coerce_return_scalar_source(ctx, value, span, coerce_to_tagged_scalar)
        }
        IrType::Heap(_) if ctx.return_php_type.codegen_repr() == PhpType::Mixed => {
            ctx.box_value_as_mixed(value, ctx.return_php_type.clone(), span)
        }
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

/// Coerces a return value and releases the old owning temporary when replaced.
fn coerce_return_scalar_source(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
    coerce: fn(&mut LoweringContext<'_, '_>, LoweredValue, Option<Span>) -> LoweredValue,
) -> LoweredValue {
    let coerced = coerce(ctx, value, span);
    if coerced.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
}

/// Coerces an integer-or-null value into the two-word tagged-scalar return shape.
fn coerce_to_tagged_scalar(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    if value.ir_type == IrType::TaggedScalar {
        return value;
    }
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Void
    ) {
        return ctx.emit_value(
            Op::ConstNull,
            Vec::new(),
            None,
            PhpType::TaggedScalar,
            Op::ConstNull.default_effects(),
            span,
        );
    }
    ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        PhpType::TaggedScalar,
        effects_lookup::runtime_effects(),
        span,
    )
}

/// Widens returned container payload storage to the current function return contract.
fn coerce_container_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let return_ty = ctx.return_php_type.codegen_repr();
    let op = match (source_ty, return_ty.clone()) {
        (PhpType::Array(source_elem), PhpType::Array(return_elem))
            if source_elem.codegen_repr() != PhpType::Mixed
                && return_elem.codegen_repr() == PhpType::Mixed =>
        {
            Op::ArrayToMixed
        }
        (
            PhpType::AssocArray {
                value: source_value,
                ..
            },
            PhpType::AssocArray {
                value: return_value,
                ..
            },
        ) if source_value.codegen_repr() != PhpType::Mixed
            && return_value.codegen_repr() == PhpType::Mixed =>
        {
            Op::HashToMixed
        }
        (PhpType::Array(source_elem), PhpType::AssocArray { .. })
            if source_elem.as_ref() == &PhpType::Never =>
        {
            Op::ArrayToHash
        }
        _ => return None,
    };
    Some(ctx.emit_value(
        op,
        vec![value.value],
        None,
        return_ty,
        op.default_effects(),
        span,
    ))
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
        IrType::I64 | IrType::TaggedScalar => ctx.emit_value(
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
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            PhpType::Str,
            Op::Cast.default_effects(),
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
    load_static_property_as(ctx, receiver, property, PhpType::Mixed, span)
}

/// Loads a static property value using known PHP metadata.
fn load_static_property_as(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let name = format!("{}::{}", receiver_name(receiver), property);
    let data = ctx.intern_string(&name);
    ctx.emit_value(
        Op::LoadStaticProperty,
        Vec::new(),
        Some(Immediate::Data(data)),
        php_type,
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

/// Resolves the declared PHP type of a static property for statement lowering.
fn static_property_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
) -> Option<PhpType> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    ctx.classes
        .get(class_name.as_str())?
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, property_ty)| normalize_value_php_type(property_ty.codegen_repr()))
}

/// Resolves a static receiver to a concrete class name when lexical metadata is available.
fn static_receiver_class_name(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().trim_start_matches('\\').to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => ctx.current_class.clone(),
        StaticReceiver::Parent => {
            let current = ctx.current_class.as_deref()?;
            ctx.classes
                .get(current)
                .and_then(|class_info| class_info.parent.clone())
        }
    }
}

/// Resolves the declared PHP type of an object property for statement lowering.
fn object_property_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> Option<PhpType> {
    let object_ty = ctx.builder.value_php_type(object);
    let PhpType::Object(class_name) = object_ty else {
        return None;
    };
    ctx.classes
        .get(class_name.trim_start_matches('\\'))?
        .visible_property(property)
        .map(|(_, (_, property_ty))| normalize_value_php_type(property_ty.codegen_repr()))
}

/// Returns true when a property type uses concrete indexed-array storage.
fn is_indexed_array_type(php_type: &PhpType) -> bool {
    matches!(php_type.codegen_repr(), PhpType::Array(_))
}

/// Returns true when a property type uses concrete associative-array storage.
fn is_assoc_array_type(php_type: &PhpType) -> bool {
    matches!(php_type.codegen_repr(), PhpType::AssocArray { .. })
}

/// Normalizes non-materializable statement metadata to the EIR null sentinel type.
fn normalize_value_php_type(php_type: PhpType) -> PhpType {
    if matches!(php_type, PhpType::Never) {
        PhpType::Void
    } else {
        php_type
    }
}
