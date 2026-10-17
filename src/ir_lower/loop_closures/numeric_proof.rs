//! Purpose:
//! Proves that selected locals stay in the PHP `int|float` domain across a
//! straight-line `for` or `while`, independently of the closure's identity.
//!
//! Called from:
//! - `loop_closures::begin_loop_closure_facts` before a loop body is emitted.
//!
//! Key details:
//! - Entry seeds come from a proven `I64` or `F64` slot. A `Mixed` label is `Unknown`.
//! - Checked integer arithmetic widens to `int|float`, which is not an `I64` proof.
//! - An incomplete walk or an unproven value yields no admitted call. The descriptor path stays.

use crate::ir::{Function, Immediate, IrType, MixedNumericOp, Op, Terminator};
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::PhpType;
use std::collections::{HashMap, HashSet};

use super::super::context::{LoweringContext, StaticCallableBinding};

/// Lattice element for one local or temporary.
///
/// `Numeric` means PHP `int|float` after a checked overflow. It is not a raw `I64`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Domain {
    Int,
    Float,
    Numeric,
    Unknown,
}

/// Call sites a numeric proof admitted, plus the locals whose domains it tracked.
pub(super) struct NumericProof {
    pub(super) spans: HashSet<Span>,
    pub(super) locals: HashSet<String>,
}

/// Fixed-point proof of straight-line numeric arguments for one invariant closure.
pub(super) fn prove(
    ctx: &LoweringContext<'_, '_>,
    closure_local: &str,
    binding: &StaticCallableBinding,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
) -> Option<NumericProof> {
    let (function, capture_domains) = numeric_target(ctx, binding)?;
    let mut state: HashMap<String, Domain> = HashMap::new();
    for _ in 0..64 {
        let mut after = state.clone();
        let mut spans = HashSet::new();
        if !transfer(
            ctx, closure_local, function, &capture_domains, condition, update, body, &mut after,
            &mut spans,
        ) {
            return None;
        }
        let mut next = HashMap::new();
        for (name, domain) in &after {
            next.insert(name.clone(), join(seed(ctx, name), *domain));
        }
        if next == state {
            let locals = next
                .iter()
                .filter(|(_, domain)| matches!(domain, Domain::Int | Domain::Float | Domain::Numeric))
                .map(|(name, _)| name.clone())
                .collect();
            if spans.is_empty() {
                return None;
            }
            return Some(NumericProof { spans, locals });
        }
        state = next;
    }
    None
}

/// Reports whether this loop contains a call of `local` sealed by an earlier proof.
pub(super) fn loop_has_sealed_call(
    sites: &HashSet<Span>,
    local: &str,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
) -> bool {
    condition.is_some_and(|expr| expr_has_sealed_call(sites, local, expr))
        || update.is_some_and(|stmt| stmt_has_sealed_call(sites, local, stmt))
        || body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
}

/// Joins two domains. Distinct numeric elements become `int|float`.
fn join(left: Domain, right: Domain) -> Domain {
    if left == right {
        left
    } else if left == Domain::Unknown || right == Domain::Unknown {
        Domain::Unknown
    } else {
        Domain::Numeric
    }
}

/// Seeds a local from its entry storage. `Mixed` and every unproven shape stay unknown.
fn seed(ctx: &LoweringContext<'_, '_>, name: &str) -> Domain {
    if ctx.is_ref_bound_local(name) || !ctx.can_track_static_callable_local(name) {
        return Domain::Unknown;
    }
    let Some(slot) = ctx.local_slots.get(name).copied() else {
        return Domain::Unknown;
    };
    let logical = ctx.local_type(name);
    let storage = ctx.builder.local_php_type(slot);
    let ir = ctx.builder.function().locals.get(slot.as_raw() as usize).map(|local| local.ir_type);
    if logical == PhpType::Int && storage.codegen_repr() == PhpType::Int && ir == Some(IrType::I64) {
        Domain::Int
    } else if logical == PhpType::Float
        && storage.codegen_repr() == PhpType::Float
        && ir == Some(IrType::F64)
    {
        Domain::Float
    } else {
        Domain::Unknown
    }
}

/// Accepts an untyped Mixed closure whose captures are by-value int or float.
fn numeric_target<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    binding: &StaticCallableBinding,
) -> Option<(&'a Function, Vec<Domain>)> {
    let StaticCallableBinding::Closure { name, signature, captures, .. } = binding else {
        return None;
    };
    let arity = super::argument_transport::visible_arity(signature)?;
    if signature.declared_return
        || signature.return_type.codegen_repr() != PhpType::Mixed
        || signature.declared_params.iter().take(arity).any(|declared| *declared)
        || signature.params.iter().take(arity).any(|(_, ty)| ty.codegen_repr() != PhpType::Mixed)
    {
        return None;
    }
    let mut capture_domains = Vec::with_capacity(captures.len());
    for capture in captures {
        if capture.by_ref_local.is_some()
            || ctx.builder.value_defining_op(capture.value) == Some(Op::LoadRefCell)
        {
            return None;
        }
        let php_type = ctx.builder.value_php_type(capture.value);
        let ir_type = ctx.builder.function().value(capture.value)?.ir_type;
        let domain = if php_type == PhpType::Int && ir_type == IrType::I64 {
            Domain::Int
        } else if php_type == PhpType::Float && ir_type == IrType::F64 {
            Domain::Float
        } else {
            return None;
        };
        capture_domains.push(domain);
    }
    let function = ctx.closure_function(name)?;
    Some((function, capture_domains))
}

/// Applies the condition, the body, and the update once. `false` means the shape is unsupported.
fn transfer(
    ctx: &LoweringContext<'_, '_>,
    closure_local: &str,
    function: &Function,
    capture_domains: &[Domain],
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
    state: &mut HashMap<String, Domain>,
    spans: &mut HashSet<Span>,
) -> bool {
    condition.is_none_or(|expr| eval_expr(ctx, closure_local, function, capture_domains, state, spans, expr).is_ok())
        && body.iter().all(|stmt| {
            transfer_stmt(ctx, closure_local, function, capture_domains, state, spans, stmt)
        })
        && update.is_none_or(|stmt| {
            transfer_stmt(ctx, closure_local, function, capture_domains, state, spans, stmt)
        })
}

/// Transfers one straight-line statement. Branches and nested loops fail the proof.
fn transfer_stmt(
    ctx: &LoweringContext<'_, '_>,
    closure_local: &str,
    function: &Function,
    capture_domains: &[Domain],
    state: &mut HashMap<String, Domain>,
    spans: &mut HashSet<Span>,
    stmt: &Stmt,
) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            if name == closure_local {
                return false;
            }
            let Ok(domain) = eval_expr(
                ctx, closure_local, function, capture_domains, state, spans, value,
            ) else {
                return false;
            };
            state.insert(name.clone(), domain);
            true
        }
        StmtKind::Echo(value) | StmtKind::ExprStmt(value) => eval_expr(
            ctx, closure_local, function, capture_domains, state, spans, value,
        )
        .is_ok(),
        StmtKind::Return(value) => value.as_ref().is_none_or(|value| {
            eval_expr(ctx, closure_local, function, capture_domains, state, spans, value).is_ok()
        }),
        StmtKind::Synthetic(body) => body.iter().all(|stmt| {
            transfer_stmt(ctx, closure_local, function, capture_domains, state, spans, stmt)
        }),
        _ => false,
    }
}

/// Evaluates one expression. Only literals, locals, `+`/`-`/`*`, and the tracked call are numeric.
fn eval_expr(
    ctx: &LoweringContext<'_, '_>,
    closure_local: &str,
    function: &Function,
    capture_domains: &[Domain],
    state: &mut HashMap<String, Domain>,
    spans: &mut HashSet<Span>,
    expr: &Expr,
) -> Result<Domain, ()> {
    match &expr.kind {
        ExprKind::IntLiteral(_) => Ok(Domain::Int),
        ExprKind::FloatLiteral(_) => Ok(Domain::Float),
        ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::StringLiteral(_) => Ok(Domain::Unknown),
        ExprKind::Variable(name) => Ok(read_local(ctx, state, name)),
        ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
            if name == closure_local {
                return Err(());
            }
            let domain = widen_inc(read_local(ctx, state, name));
            state.insert(name.clone(), domain);
            Ok(domain)
        }
        ExprKind::BinaryOp { op, left, right } => {
            let left = eval_expr(ctx, closure_local, function, capture_domains, state, spans, left)?;
            let right = eval_expr(ctx, closure_local, function, capture_domains, state, spans, right)?;
            arithmetic(op, left, right)
        }
        ExprKind::Negate(value) => {
            let domain = eval_expr(ctx, closure_local, function, capture_domains, state, spans, value)?;
            Ok(match domain {
                Domain::Float => Domain::Float,
                Domain::Int | Domain::Numeric => Domain::Numeric,
                Domain::Unknown => Domain::Unknown,
            })
        }
        ExprKind::Not(value) => {
            eval_expr(ctx, closure_local, function, capture_domains, state, spans, value)?;
            Ok(Domain::Unknown)
        }
        ExprKind::ClosureCall { var, args } => {
            if var != closure_local || Some(args.len()) != function_visible_params(function, capture_domains) {
                return Err(());
            }
            let mut arg_domains = Vec::with_capacity(args.len());
            let mut numeric_args = true;
            for arg in args {
                let domain = bare_numeric_argument(
                    ctx, closure_local, function, capture_domains, state, spans, arg,
                )?;
                numeric_args &= is_numeric(domain);
                arg_domains.push(domain);
            }
            if !numeric_args {
                return Ok(Domain::Unknown);
            }
            let result = closure_result(function, &arg_domains, capture_domains).ok_or(())?;
            if is_numeric(result) {
                spans.insert(expr.span);
                Ok(result)
            } else {
                Ok(Domain::Unknown)
            }
        }
        ExprKind::Assignment { target, value, result_target, prelude, conditional_value_temp } => {
            if conditional_value_temp.is_some() {
                return Err(());
            }
            if !prelude.iter().all(|stmt| {
                transfer_stmt(ctx, closure_local, function, capture_domains, state, spans, stmt)
            }) {
                return Err(());
            }
            let ExprKind::Variable(name) = &target.kind else {
                return Err(());
            };
            if name == closure_local {
                return Err(());
            }
            let domain = eval_expr(ctx, closure_local, function, capture_domains, state, spans, value)?;
            state.insert(name.clone(), domain);
            if let Some(target) = result_target {
                eval_expr(ctx, closure_local, function, capture_domains, state, spans, target)?;
            }
            Ok(domain)
        }
        _ => Err(()),
    }
}

/// Accepts a literal or local argument. `$value + 0` and every other expression stay out.
fn bare_numeric_argument(
    ctx: &LoweringContext<'_, '_>,
    closure_local: &str,
    function: &Function,
    capture_domains: &[Domain],
    state: &mut HashMap<String, Domain>,
    spans: &mut HashSet<Span>,
    expr: &Expr,
) -> Result<Domain, ()> {
    match &expr.kind {
        ExprKind::IntLiteral(_) => Ok(Domain::Int),
        ExprKind::FloatLiteral(_) => Ok(Domain::Float),
        ExprKind::Variable(name) => Ok(read_local(ctx, state, name)),
        _ => {
            // Still walk the expression so a side effect is not ignored, then refuse the call.
            eval_expr(ctx, closure_local, function, capture_domains, state, spans, expr)?;
            Ok(Domain::Unknown)
        }
    }
}

/// Reads a local, recording the entry seed on the first visit of this iteration.
fn read_local(ctx: &LoweringContext<'_, '_>, state: &mut HashMap<String, Domain>, name: &str) -> Domain {
    if let Some(domain) = state.get(name).copied() {
        return domain;
    }
    let domain = seed(ctx, name);
    state.insert(name.to_string(), domain);
    domain
}

/// Widens a checked increment. Integer overflow joins `float`; a float stays a float.
fn widen_inc(domain: Domain) -> Domain {
    match domain {
        Domain::Int | Domain::Numeric => Domain::Numeric,
        Domain::Float => Domain::Float,
        Domain::Unknown => Domain::Unknown,
    }
}

/// Models `+`, `-`, and `*`. Comparisons are reads. Every other operator is unsupported.
fn arithmetic(op: &BinOp, left: Domain, right: Domain) -> Result<Domain, ()> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul => Ok(match (left, right) {
            (Domain::Unknown, _) | (_, Domain::Unknown) => Domain::Unknown,
            (Domain::Float, Domain::Float) => Domain::Float,
            _ => Domain::Numeric,
        }),
        BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq | BinOp::Lt | BinOp::Gt
        | BinOp::LtEq | BinOp::GtEq | BinOp::Spaceship => Ok(Domain::Unknown),
        _ => Err(()),
    }
}

/// Returns whether a domain is still inside PHP `int|float`.
fn is_numeric(domain: Domain) -> bool {
    matches!(domain, Domain::Int | Domain::Float | Domain::Numeric)
}

/// Uses public signature arity while checking the complete physical capture/transport layout.
fn function_visible_params(function: &Function, capture_domains: &[Domain]) -> Option<usize> {
    let signature = function.signature.as_ref()?;
    let arity = super::argument_transport::visible_arity(signature)?;
    (function.params.len() == signature.params.len() + capture_domains.len()).then_some(arity)
}

/// Interprets the lowered closure once under the current argument and capture domains.
fn closure_result(
    function: &Function,
    arg_domains: &[Domain],
    capture_domains: &[Domain],
) -> Option<Domain> {
    if function.blocks.len() != 1
        || Some(arg_domains.len()) != function_visible_params(function, capture_domains)
    {
        return None;
    }
    let mut slots: HashMap<u32, Domain> = HashMap::new();
    let mut domains = arg_domains.iter().chain(capture_domains.iter());
    for param in &function.params {
        let domain = if param.name == crate::func_args::HIDDEN_ARGS_PARAM {
            Domain::Unknown
        } else {
            *domains.next()?
        };
        let slot = function.locals.iter().find(|local| local.name.as_deref() == Some(param.name.as_str()))?;
        slots.insert(slot.id.as_raw(), domain);
    }
    let mut values: HashMap<u32, Domain> = HashMap::new();
    for inst in &function.instructions {
        let domain = match inst.op {
            Op::ConstI64 => Domain::Int,
            Op::ConstF64 => Domain::Float,
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = inst.immediate.as_ref() else { return None };
                *slots.get(&slot.as_raw())?
            }
            Op::StoreLocal => {
                let (Some(Immediate::LocalSlot(slot)), Some(operand)) =
                    (inst.immediate.as_ref(), inst.operands.first())
                else {
                    return None;
                };
                let domain = *values.get(&operand.as_raw())?;
                slots.insert(slot.as_raw(), domain);
                continue;
            }
            Op::MixedClone | Op::MixedBox | Op::Acquire | Op::Move | Op::Borrow | Op::EnsureOwned => {
                let operand = inst.operands.first()?;
                *values.get(&operand.as_raw())?
            }
            Op::Release | Op::ReleaseUnlessAliases | Op::ReleaseLocalSlot | Op::Nop | Op::ConcatReset
            | Op::ZeroLocalSlot | Op::UnsetLocal => continue,
            Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::MixedNumericBinop => {
                if inst.op == Op::MixedNumericBinop
                    && !matches!(
                        inst.immediate.as_ref(),
                        Some(Immediate::MixedNumericOp(
                            MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul
                        ))
                    )
                {
                    return None;
                }
                let mut numeric = true;
                for operand in &inst.operands {
                    if !is_numeric(*values.get(&operand.as_raw())?) {
                        numeric = false;
                    }
                }
                if numeric { Domain::Numeric } else { return None; }
            }
            Op::FAdd | Op::FSub | Op::FMul | Op::FNeg | Op::IToF => {
                for operand in &inst.operands {
                    if !is_numeric(*values.get(&operand.as_raw())?) {
                        return None;
                    }
                }
                Domain::Float
            }
            _ => return None,
        };
        let result = inst.result?;
        values.insert(result.as_raw(), domain);
    }
    let block = function.blocks.first()?;
    let Some(Terminator::Return { value: Some(value) }) = &block.terminator else {
        return None;
    };
    let domain = *values.get(&value.as_raw())?;
    is_numeric(domain).then_some(domain)
}

/// Finds a sealed call of `local` inside one statement.
fn stmt_has_sealed_call(sites: &HashSet<Span>, local: &str, stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Assign { value, .. } | StmtKind::Echo(value) | StmtKind::ExprStmt(value)
        | StmtKind::TypedAssign { value, .. } => expr_has_sealed_call(sites, local, value),
        StmtKind::Return(value) => value.as_ref().is_some_and(|value| expr_has_sealed_call(sites, local, value)),
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr_has_sealed_call(sites, local, condition)
                || then_body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_has_sealed_call(sites, local, condition)
                        || body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
                })
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_has_sealed_call(sites, local, condition)
                || body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
        }
        StmtKind::For { init, condition, update, body } => {
            init.as_ref().is_some_and(|stmt| stmt_has_sealed_call(sites, local, stmt))
                || condition.as_ref().is_some_and(|expr| expr_has_sealed_call(sites, local, expr))
                || update.as_ref().is_some_and(|stmt| stmt_has_sealed_call(sites, local, stmt))
                || body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
        }
        StmtKind::Synthetic(body) => body.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt)),
        _ => false,
    }
}

/// Finds a sealed call of `local` inside one expression, without entering nested closures.
fn expr_has_sealed_call(sites: &HashSet<Span>, local: &str, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::ClosureCall { var, args } => {
            (var == local && sites.contains(&expr.span))
                || args.iter().any(|arg| expr_has_sealed_call(sites, local, arg))
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_sealed_call(sites, local, left) || expr_has_sealed_call(sites, local, right)
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. } => expr_has_sealed_call(sites, local, value),
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            expr_has_sealed_call(sites, local, target)
                || expr_has_sealed_call(sites, local, value)
                || result_target.as_ref().is_some_and(|target| expr_has_sealed_call(sites, local, target))
                || prelude.iter().any(|stmt| stmt_has_sealed_call(sites, local, stmt))
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            expr_has_sealed_call(sites, local, condition)
                || expr_has_sealed_call(sites, local, then_expr)
                || expr_has_sealed_call(sites, local, else_expr)
        }
        ExprKind::Closure { .. } => false,
        _ => false,
    }
}
