//! Purpose:
//! Preserves closure identities across loop backedges only after checking the
//! entire condition, update, and body before any loop instruction is emitted.
//!
//! Called from:
//! - `stmt::loops` at loop entry and `expr::callable_resolution` before direct calls.
//! - `function::lower_body_into_function` when a numeric proof must be replayed
//!   against final Mixed storage.
//!
//! Key details:
//! - Identity and the numeric domain are separate proofs. A written counter does not
//!   by itself discard a closure whose arguments are proved `int|float`.
//! - The existing unwritten raw-scalar path is unchanged. Do/while never takes the new path.
//! - An incomplete numeric proof keeps the ordinary descriptor invoke.
//! - A numeric-proof binding cannot authorize other calls through the raw-scalar gate.
//! - Compiler-only argument transport needs its own prologue proof and never seeds numeric facts.

mod numeric_proof;
mod argument_transport;

use crate::ir::{Effects, Immediate, IrType, Op, Terminator};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};
use std::collections::{HashMap, HashSet};

use super::context::{LoweringContext, StaticCallableBinding};

/// Tracks direct argument reads and every syntactic local write in an admitted loop.
#[derive(Default)]
struct LoopArguments {
    passed: HashSet<String>,
    written: HashSet<String>,
    reading_argument: bool,
}

/// Installs only entry closure facts proved invariant over every loop path.
pub(crate) fn begin_loop_closure_facts(
    ctx: &mut LoweringContext<'_, '_>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
    new_direct_path: bool,
) -> HashMap<String, StaticCallableBinding> {
    let original = ctx.static_callable_locals_snapshot();
    let mut facts = original.clone();
    facts.retain(|local, binding| {
        let StaticCallableBinding::Closure { loop_guarded, .. } = binding else {
            return false;
        };
        if !ctx.can_track_static_callable_local(local) || ctx.is_ref_bound_local(local)
            || ctx.has_eval_barrier() || ctx.eval_executed()
            || ((new_direct_path || *loop_guarded) && !scalar_nothrow_target(ctx, binding))
        {
            return false;
        }
        let mut arguments = LoopArguments::default();
        let identity = condition.is_none_or(|expr| stable_expr(ctx, local, &mut arguments, expr))
            && update.is_none_or(|stmt| stable_stmt(ctx, local, &mut arguments, stmt))
            && stable_body(ctx, local, &mut arguments, body);
        if !identity {
            return false;
        }
        // The unwritten raw-scalar path stays exactly the #1850 gate. Written
        // arguments fall through to the numeric proof instead of widening this one.
        let scalar_inputs = arguments.passed.is_disjoint(&arguments.written);
        if scalar_inputs {
            if let StaticCallableBinding::Closure { loop_guarded, .. } = binding {
                *loop_guarded |= new_direct_path;
            }
            return true;
        }
        if !new_direct_path {
            return false;
        }
        if ctx.numeric_proof_sealed {
            let admitted = numeric_proof::loop_has_sealed_call(
                &ctx.numeric_closure_sites, local, condition, update, body,
            );
            if admitted {
                if let StaticCallableBinding::Closure { loop_guarded, numeric_loop_guarded, .. } = binding {
                    *loop_guarded = true;
                    *numeric_loop_guarded = true;
                }
            }
            return admitted;
        }
        let Some(proof) = numeric_proof::prove(ctx, local, binding, condition, update, body) else {
            return false;
        };
        ctx.numeric_closure_sites.extend(proof.spans);
        ctx.numeric_tracked_locals.extend(proof.locals);
        if let StaticCallableBinding::Closure { loop_guarded, numeric_loop_guarded, .. } = binding {
            *loop_guarded = true;
            *numeric_loop_guarded = true;
        }
        true
    });
    let mut header_facts = original;
    header_facts.retain(|_, target| !matches!(target, StaticCallableBinding::Closure { .. }));
    header_facts.extend(facts.clone());
    ctx.restore_static_callable_locals(header_facts);
    facts
}

/// Drops fresh condition bindings and retains only still-valid invariant entry closures.
pub(crate) fn retain_loop_body_facts(
    ctx: &mut LoweringContext<'_, '_>,
    entry: &HashMap<String, StaticCallableBinding>,
) {
    let mut current = ctx.static_callable_locals_snapshot();
    current.retain(|name, target| entry.get(name) == Some(target));
    ctx.clear_static_callable_locals();
    ctx.restore_static_callable_locals(current);
}

/// Applies strict call gates only to facts newly carried through for/while loops.
///
/// A numeric-proof binding uses the ordinary Mixed `Op::Call` ABI only at admitted spans.
/// Other loop-guarded bindings still require unwritten raw scalar arguments.
pub(crate) fn direct_call_is_eligible(
    ctx: &LoweringContext<'_, '_>,
    binding: &StaticCallableBinding,
    args: &[Expr],
    span: Span,
) -> bool {
    let StaticCallableBinding::Closure {
        signature, loop_guarded: true, numeric_loop_guarded, ..
    } = binding else {
        return true;
    };
    if *numeric_loop_guarded {
        return ctx.numeric_closure_sites.contains(&span);
    }
    scalar_nothrow_target(ctx, binding)
        && Some(args.len()) == argument_transport::visible_arity(signature)
        && args.iter().zip(&signature.params).all(|(arg, (_, parameter))| {
            scalar_argument_type(ctx, arg).is_some_and(|actual| {
                *parameter == PhpType::Mixed || *parameter == actual
                    || (*parameter == PhpType::Bool && actual == PhpType::False)
            })
        })
}

/// Proves scalar inputs/results, closed local state, and the lowered body's nonthrowing effects.
fn scalar_nothrow_target(ctx: &LoweringContext<'_, '_>, binding: &StaticCallableBinding) -> bool {
    let StaticCallableBinding::Closure { name, signature, captures, .. } = binding else {
        return false;
    };
    if !scalar_signature(signature) || captures.iter().any(|capture| {
        capture.by_ref_local.is_some()
            || !scalar_type(&ctx.builder.value_php_type(capture.value))
            || !ctx.builder.function().value(capture.value).is_some_and(|value| {
                value.ir_type == if value.php_type == PhpType::Float { IrType::F64 } else { IrType::I64 }
            })
            || ctx.builder.value_defining_op(capture.value) == Some(Op::LoadRefCell)
    }) {
        return false;
    }
    let Some(function) = ctx.closure_function(name) else { return false };
    if function.flags.is_generator || function.flags.by_ref_return || function.blocks.len() != 1
        || function.return_type == IrType::Void
        || function.params.len() != signature.params.len() + captures.len()
    {
        return false;
    }
    let Some(transport) = argument_transport::prologue_values(function, signature) else {
        return false;
    };
    let fallible = Effects::MAY_THROW | Effects::MAY_FATAL | Effects::MAY_WARN;
    function.instructions.iter().all(|inst| {
        !inst.effects.intersects(fallible)
            && (scalar_type(&inst.result_php_type) || inst.result_php_type == PhpType::Mixed
                || inst.result.is_some_and(|value| transport.contains(&value)))
            && (inst.op == Op::ConcatReset
                || !inst.effects.intersects(Effects::READS_GLOBAL | Effects::WRITES_GLOBAL))
            && scalar_body_primitive(inst.op)
            && (inst.op != Op::MixedNumericBinop || matches!(inst.immediate,
                Some(Immediate::MixedNumericOp(crate::ir::MixedNumericOp::Add
                    | crate::ir::MixedNumericOp::Sub | crate::ir::MixedNumericOp::Mul))))
    }) && function.blocks.iter().all(|block| {
        !matches!(block.terminator, Some(Terminator::Throw { .. } | Terminator::Fatal { .. }))
    })
}

/// Fails closed for every call surface and future opcode, even after effect refinement.
fn scalar_body_primitive(op: Op) -> bool {
    use Op::*;
    matches!(op, ConstI64 | ConstF64 | ConstBool | ConstNull | LoadLocal | StoreLocal
        | ZeroLocalSlot | UnsetLocal | ReleaseLocalSlot | ICheckedAdd | ICheckedSub | ICheckedMul
        | FAdd | FSub | FMul | FNeg | IBitAnd | IBitOr | IBitXor | IBitNot
        | ICmp | FCmp | PhpRelCmp | StrictEq | StrictNotEq | LooseEq | LooseNotEq | Spaceship
        | IsNull | IsTruthy | IsEmpty | TypePredicate | IToF | MixedBox | MixedClone | MixedUnbox
        | MixedTagOf | MixedNumericBinop | Acquire | Release | ReleaseUnlessAliases
        | Move | Borrow | EnsureOwned | ConcatReset | Nop)
}

/// Allows exact scalar calls without coercion, omitted arguments, source variadics or references.
fn scalar_signature(signature: &FunctionSig) -> bool {
    let Some(arity) = argument_transport::visible_arity(signature) else { return false };
    !signature.by_ref_return && !signature.is_generator
        && signature.deprecation.is_none() && !signature.ref_params.iter().any(|by_ref| *by_ref)
        && signature.params.iter().take(arity).all(|(_, ty)| scalar_type(ty) || *ty == PhpType::Mixed)
        && (scalar_type(&signature.return_type) || signature.return_type == PhpType::Mixed)
}

/// Recognizes PHP scalars that cannot own user cleanup hooks.
fn scalar_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Void)
}

/// Requires scalar storage after loop entry has excluded writes to argument locals.
/// A first-iteration type is not proof of the representation on later backedges.
fn scalar_argument_type(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> Option<PhpType> {
    match &expr.kind {
        ExprKind::IntLiteral(_) => Some(PhpType::Int),
        ExprKind::FloatLiteral(_) => Some(PhpType::Float),
        ExprKind::BoolLiteral(false) => Some(PhpType::False),
        ExprKind::BoolLiteral(true) => Some(PhpType::Bool),
        ExprKind::Null => Some(PhpType::Void),
        ExprKind::Variable(name) => {
            if ctx.is_ref_bound_local(name) || !ctx.can_track_static_callable_local(name) {
                return None;
            }
            let ty = ctx.local_type(name);
            let slot = *ctx.local_slots.get(name)?;
            let storage = ctx.builder.local_php_type(slot);
            if !scalar_type(&ty) || !scalar_type(&storage) { return None; }
            let ir = ctx.builder.function().locals.get(slot.as_raw() as usize)?.ir_type;
            ((ir == IrType::I64 && ty != PhpType::Float)
                || (ir == IrType::F64 && ty == PhpType::Float)).then_some(ty)
        }
        _ => None,
    }
}

/// Checks every statement, including dead-looking backedge writers, before lowering.
fn stable_body(
    ctx: &LoweringContext<'_, '_>,
    local: &str,
    arguments: &mut LoopArguments,
    body: &[Stmt],
) -> bool {
    body.iter().all(|stmt| stable_stmt(ctx, local, arguments, stmt))
}

/// Admits a closed scalar statement subset; new syntax defaults to descriptor fallback.
fn stable_stmt(
    ctx: &LoweringContext<'_, '_>,
    local: &str,
    arguments: &mut LoopArguments,
    stmt: &Stmt,
) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            arguments.written.insert(name.clone());
            name != local && (!ctx.local_slots.contains_key(name)
                || harmless_local(ctx, name)) && stable_expr(ctx, local, arguments, value)
        }
        StmtKind::Echo(value) | StmtKind::ExprStmt(value) => stable_expr(ctx, local, arguments, value),
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            stable_expr(ctx, local, arguments, condition) && stable_body(ctx, local, arguments, then_body)
                && elseif_clauses.iter().all(|(condition, body)| {
                    stable_expr(ctx, local, arguments, condition) && stable_body(ctx, local, arguments, body)
                }) && else_body.as_ref().is_none_or(|body| stable_body(ctx, local, arguments, body))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            stable_expr(ctx, local, arguments, condition) && stable_body(ctx, local, arguments, body)
        }
        StmtKind::For { init, condition, update, body } => {
            init.as_ref().is_none_or(|stmt| stable_stmt(ctx, local, arguments, stmt))
                && condition.as_ref().is_none_or(|expr| stable_expr(ctx, local, arguments, expr))
                && update.as_ref().is_none_or(|stmt| stable_stmt(ctx, local, arguments, stmt))
                && stable_body(ctx, local, arguments, body)
        }
        StmtKind::Synthetic(body) => stable_body(ctx, local, arguments, body),
        StmtKind::Return(value) => value.as_ref().is_none_or(|value| stable_expr(ctx, local, arguments, value)),
        StmtKind::Break(_) | StmtKind::Continue(_) => true,
        _ => false,
    }
}

/// Rejects reference, global, and destructor-bearing locals as possible implicit callbacks.
///
/// Locals forced to Mixed by a sealed numeric proof are still `int|float` values.
/// The storage label alone is not what makes them harmless.
fn harmless_local(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    !ctx.is_ref_bound_local(name) && ctx.can_track_static_callable_local(name)
        && (ctx.final_mixed_locals.contains(name)
            || scalar_type(&ctx.local_type(name))
            || ctx.local_type(name) == PhpType::Str)
}

/// Lowers a body, then replays it when an admitted numeric local's final slot is Mixed
/// but an earlier load still used a narrower type.
///
/// The proof is sealed from the discovery pass, whose entry seeds are still the
/// proven `I64`/`F64` types. Replaying must not re-seed those locals from the Mixed
/// label it is about to force. A parameter or a reference cell is not rewritten in
/// place; that call stays on the descriptor path.
pub(crate) fn lower_body_observing_numeric_closures(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
) {
    let checkpoint = ctx.snapshot();
    lower_stmts(ctx, body);
    if ctx.numeric_closure_sites.is_empty() {
        return;
    }
    let sites = ctx.numeric_closure_sites.clone();
    let tracked = ctx.numeric_tracked_locals.clone();
    let params = ctx
        .builder
        .function()
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect::<HashSet<_>>();
    let mut force = HashSet::new();
    let mut suppress = false;
    for name in &tracked {
        if !narrow_load_of_mixed_slot(ctx, name) {
            continue;
        }
        if params.contains(name) || ctx.is_ref_bound_local(name) {
            suppress = true;
        } else {
            force.insert(name.clone());
        }
    }
    if tracked.iter().any(|name| ctx.is_ref_bound_local(name)) {
        suppress = true;
    }
    if !suppress && force.is_empty() {
        return;
    }
    ctx.restore(checkpoint);
    ctx.numeric_proof_sealed = true;
    if !suppress {
        ctx.numeric_closure_sites = sites;
        ctx.final_mixed_locals = force;
    }
    lower_stmts(ctx, body);
}

/// Lowers each statement in source order.
fn lower_stmts(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt]) {
    for stmt in body {
        crate::ir_lower::stmt::lower_stmt(ctx, stmt);
    }
}

/// Reports a load whose result type is narrower than the slot's final Mixed storage.
fn narrow_load_of_mixed_slot(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    let Some(slot) = ctx.local_slots.get(name).copied() else {
        return false;
    };
    if ctx.builder.local_php_type(slot).codegen_repr() != PhpType::Mixed {
        return false;
    }
    ctx.builder.function().instructions.iter().any(|inst| {
        inst.op == Op::LoadLocal
            && matches!(inst.immediate, Some(Immediate::LocalSlot(loaded)) if loaded == slot)
            && inst.result_php_type.codegen_repr() != PhpType::Mixed
    })
}

/// Rejects opaque calls, aliases, declarations, and indirect writes anywhere in the loop.
fn stable_expr(
    ctx: &LoweringContext<'_, '_>,
    local: &str,
    arguments: &mut LoopArguments,
    expr: &Expr,
) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_)
        | ExprKind::Null | ExprKind::StringLiteral(_) | ExprKind::ConstRef(_) => true,
        ExprKind::Variable(name) => {
            if arguments.reading_argument {
                arguments.passed.insert(name.clone());
            }
            name != local && harmless_local(ctx, name)
        },
        ExprKind::BinaryOp { left, right, .. } => {
            stable_expr(ctx, local, arguments, left) && stable_expr(ctx, local, arguments, right)
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. } => stable_expr(ctx, local, arguments, value),
        ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
            arguments.written.insert(name.clone());
            name != local && harmless_local(ctx, name)
        }
        ExprKind::ClosureCall { var, args } => {
            if var != local { return false; }
            let reading_argument = arguments.reading_argument;
            arguments.reading_argument = true;
            let stable = args.iter().all(|arg| stable_expr(ctx, local, arguments, arg));
            arguments.reading_argument = reading_argument;
            stable
        }
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            if let ExprKind::Variable(name) = &target.kind {
                arguments.written.insert(name.clone());
            }
            matches!(&target.kind, ExprKind::Variable(name) if name != local
                && (!ctx.local_slots.contains_key(name) || harmless_local(ctx, name)))
                && stable_body(ctx, local, arguments, prelude) && stable_expr(ctx, local, arguments, value)
                && result_target.as_ref().is_none_or(|target| stable_expr(ctx, local, arguments, target))
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            stable_expr(ctx, local, arguments, condition) && stable_expr(ctx, local, arguments, then_expr)
                && stable_expr(ctx, local, arguments, else_expr)
        }
        _ => false,
    }
}
