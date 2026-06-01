//! Purpose:
//! Translates a generator body's parser AST into the narrow `ResumeNode` IR
//! consumed by the emit pass. Runs three internal stages: expression
//! classifiers (`classify_int_expr`, `classify_mixed_expr`, `classify_bool_expr`),
//! locals inference (`collect_locals`/`visit_assignments`), and the node
//! builder (`build_nodes`/`build_node`/`build_else_chain`).
//!
//! Called from:
//!  - `crate::codegen::functions::generator::emit_generator_function()` after
//!    parameter slots are allocated and before assembly emission begins.
//!
//! Key details:
//!  - Classifiers return `None` for shapes outside the v1 grammar — the
//!    builder turns those into `ResumeNode::Bail` rather than failing, so
//!    the wrapper still compiles and yields nothing past the unsupported
//!    construct.
//!  - State numbering is depth-first in source order; the emit pass relies
//!    on this for resume-label correspondence.

use super::model::*;
use crate::codegen::data_section::DataSection;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use std::collections::HashSet;

/// Records how generator locals are later read so yield-assignment slots
/// can choose between the integer fast path and the boxed Mixed path.
#[derive(Default)]
struct SlotUseHints {
    int: HashSet<String>,
    mixed: HashSet<String>,
}

/// Look up a slot index for `name`, but only if the slot's type matches
/// `expected`. Returns `None` for missing names or type mismatches —
/// classify_int_expr therefore correctly refuses to read a Mixed slot
/// as an int.
fn slot_idx_of_type(name: &str, slots: &[String], types: &[SlotType], expected: SlotType) -> Option<usize> {
    let idx = slots.iter().position(|p| p == name)?;
    if types.get(idx).copied() == Some(expected) {
        Some(idx)
    } else {
        None
    }
}

/// Classify an expression that can be translated to a v1 `IntSource`.
/// Returns `Some(IntSource)` for int literals, int-typed slot reads,
/// binary ops on int operands, and function calls where all args are
/// int-classifiable and arg count ≤ 8 (ARM64 register limit). Returns
/// `None` for unsupported shapes — the builder turns those into
/// `ResumeNode::Bail`.
pub(super) fn classify_int_expr(
    expr: &ExprKind,
    slots: &[String],
    types: &[SlotType],
) -> Option<IntSource> {
    match expr {
        ExprKind::IntLiteral(n) => Some(IntSource::Literal(*n)),
        ExprKind::Variable(name) => {
            slot_idx_of_type(name, slots, types, SlotType::Int).map(IntSource::Slot)
        }
        ExprKind::BinaryOp { left, op, right } => {
            let op = match op {
                BinOp::Add => IntBinOp::Add,
                BinOp::Sub => IntBinOp::Sub,
                BinOp::Mul => IntBinOp::Mul,
                BinOp::Div => IntBinOp::Div,
                _ => return None,
            };
            let l = classify_int_expr(&left.kind, slots, types)?;
            let r = classify_int_expr(&right.kind, slots, types)?;
            Some(IntSource::BinaryOp(Box::new(l), op, Box::new(r)))
        }
        ExprKind::FunctionCall { name, args } => {
            // ARM64 only has 8 int argument registers; v1 doesn't spill
            // arguments onto the stack for the call itself.
            if args.len() > 8 {
                return None;
            }
            let fn_name = name.as_str().to_string();
            let mut arg_sources = Vec::with_capacity(args.len());
            for arg in args {
                arg_sources.push(classify_int_expr(&arg.kind, slots, types)?);
            }
            Some(IntSource::Call { fn_name, args: arg_sources })
        }
        _ => None,
    }
}

/// Classify an expression that can be translated to a `MixedSource`.
/// Handles: null, string literals (emitted to data section), homogeneous
/// int-array literals, Mixed-typed slot reads, and any int-classifiable
/// expression (mapped to `MixedSource::Int`). Returns `None` for
/// unsupported shapes, which become `ResumeNode::Bail`.
pub(super) fn classify_mixed_expr(
    expr: &ExprKind,
    slots: &[String],
    types: &[SlotType],
    data: &mut DataSection,
) -> Option<MixedSource> {
    if matches!(expr, ExprKind::Null) {
        return Some(MixedSource::Null);
    }
    if let ExprKind::StringLiteral(s) = expr {
        let bytes = crate::string_bytes::literal_bytes(s);
        let (label, len) = data.add_string(&bytes);
        return Some(MixedSource::Str { label, len });
    }
    if let ExprKind::ArrayLiteral(items) = expr {
        // Homogeneous int-array literal: `yield [1, 2, 3]`.
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            if let ExprKind::IntLiteral(n) = &item.kind {
                values.push(*n);
            } else {
                return None;
            }
        }
        return Some(MixedSource::IntArrayLit(values));
    }
    // Reads of Mixed-typed slots (e.g. a local that was assigned a string
    // literal or an array literal earlier in the body).
    if let ExprKind::Variable(name) = expr {
        if let Some(idx) = slot_idx_of_type(name, slots, types, SlotType::Mixed) {
            return Some(MixedSource::MixedSlot(idx));
        }
    }
    classify_int_expr(expr, slots, types).map(MixedSource::Int)
}

/// Classify a boolean expression for v1 generator conditionals.
/// Supports integer comparisons and strict/loose null checks against
/// Mixed-typed slots. Returns `None` for unsupported operands — the
/// builder turns those into `ResumeNode::Bail`.
pub(super) fn classify_bool_expr(
    expr: &ExprKind,
    slots: &[String],
    types: &[SlotType],
) -> Option<BoolExpr> {
    if let ExprKind::BinaryOp { left, op, right } = expr {
        if matches!(op, BinOp::Eq | BinOp::StrictEq | BinOp::NotEq | BinOp::StrictNotEq) {
            if let Some(slot_idx) = mixed_slot_null_cmp(left, right, slots, types) {
                return Some(BoolExpr::MixedSlotNull {
                    slot_idx,
                    is_equal: matches!(op, BinOp::Eq | BinOp::StrictEq),
                });
            }
            if let Some(slot_idx) = mixed_slot_null_cmp(right, left, slots, types) {
                return Some(BoolExpr::MixedSlotNull {
                    slot_idx,
                    is_equal: matches!(op, BinOp::Eq | BinOp::StrictEq),
                });
            }
        }
        let cmp = match op {
            BinOp::Lt => CmpOp::Lt,
            BinOp::LtEq => CmpOp::Le,
            BinOp::Gt => CmpOp::Gt,
            BinOp::GtEq => CmpOp::Ge,
            BinOp::Eq | BinOp::StrictEq => CmpOp::Eq,
            BinOp::NotEq | BinOp::StrictNotEq => CmpOp::Ne,
            _ => return None,
        };
        let l = classify_int_expr(&left.kind, slots, types)?;
        let r = classify_int_expr(&right.kind, slots, types)?;
        return Some(BoolExpr::IntCompare {
            left: l,
            op: cmp,
            right: r,
        });
    }
    None
}

/// Returns the Mixed slot index when `value` is a Mixed variable and
/// `null_candidate` is the PHP null literal.
fn mixed_slot_null_cmp(
    value: &Expr,
    null_candidate: &Expr,
    slots: &[String],
    types: &[SlotType],
) -> Option<usize> {
    if !matches!(null_candidate.kind, ExprKind::Null) {
        return None;
    }
    let ExprKind::Variable(name) = &value.kind else {
        return None;
    };
    slot_idx_of_type(name, slots, types, SlotType::Mixed)
}

/// Collects variable-use hints from the generator body before slot
/// inference. The scan mirrors the narrow generator IR: arithmetic,
/// comparisons, counters, and helper-call arguments are int contexts,
/// while echo, var_dump, return, and yielded values are Mixed contexts.
fn collect_slot_use_hints(body: &[Stmt]) -> SlotUseHints {
    let mut hints = SlotUseHints::default();
    record_stmt_use_hints(body, &mut hints);
    hints
}

/// Walks statements in source order, adding variable names to the
/// relevant usage set for slot inference. Unsupported statements are
/// ignored here because the builder will still turn them into `Bail`.
fn record_stmt_use_hints(body: &[Stmt], hints: &mut SlotUseHints) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Echo(expr) => record_mixed_expr_use_hints(&expr.kind, hints),
            StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } => {
                record_assignment_rhs_use_hints(&value.kind, hints);
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                record_bool_expr_use_hints(&condition.kind, hints);
                record_stmt_use_hints(then_body, hints);
                for (elseif_cond, elseif_body) in elseif_clauses {
                    record_bool_expr_use_hints(&elseif_cond.kind, hints);
                    record_stmt_use_hints(elseif_body, hints);
                }
                if let Some(body) = else_body {
                    record_stmt_use_hints(body, hints);
                }
            }
            StmtKind::While { condition, body } => {
                record_bool_expr_use_hints(&condition.kind, hints);
                record_stmt_use_hints(body, hints);
            }
            StmtKind::DoWhile { body, condition } => {
                record_stmt_use_hints(body, hints);
                record_bool_expr_use_hints(&condition.kind, hints);
            }
            StmtKind::For { init, condition, update, body } => {
                if let Some(init_stmt) = init.as_deref() {
                    record_stmt_use_hints(std::slice::from_ref(init_stmt), hints);
                }
                if let Some(cond) = condition {
                    record_bool_expr_use_hints(&cond.kind, hints);
                }
                if let Some(update_stmt) = update.as_deref() {
                    record_stmt_use_hints(std::slice::from_ref(update_stmt), hints);
                }
                record_stmt_use_hints(body, hints);
            }
            StmtKind::Switch { subject, cases, default } => {
                record_int_expr_use_hints(&subject.kind, hints);
                for (_, case_body) in cases {
                    record_stmt_use_hints(case_body, hints);
                }
                if let Some(body) = default {
                    record_stmt_use_hints(body, hints);
                }
            }
            StmtKind::Synthetic(stmts) => record_stmt_use_hints(stmts, hints),
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                record_stmt_use_hints(try_body, hints);
                for catch in catches {
                    record_stmt_use_hints(&catch.body, hints);
                }
                if let Some(body) = finally_body {
                    record_stmt_use_hints(body, hints);
                }
            }
            StmtKind::ExprStmt(expr) => record_expr_stmt_use_hints(&expr.kind, hints),
            StmtKind::Return(Some(expr)) => record_mixed_expr_use_hints(&expr.kind, hints),
            _ => {}
        }
    }
}

/// Records uses inside an assignment RHS. Yield values are observed by
/// the caller as Mixed values; arithmetic expressions keep their operand
/// variables on the int path.
fn record_assignment_rhs_use_hints(expr: &ExprKind, hints: &mut SlotUseHints) {
    match expr {
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                record_mixed_expr_use_hints(&k.kind, hints);
            }
            if let Some(v) = value {
                record_mixed_expr_use_hints(&v.kind, hints);
            }
        }
        ExprKind::YieldFrom(inner) => record_mixed_expr_use_hints(&inner.kind, hints),
        ExprKind::BinaryOp { .. } | ExprKind::FunctionCall { .. } => {
            record_int_expr_use_hints(expr, hints);
        }
        _ => record_mixed_expr_use_hints(expr, hints),
    }
}

/// Records uses inside expression statements that the generator IR knows
/// how to lower, including var_dump diagnostics and post-increment style
/// counters.
fn record_expr_stmt_use_hints(expr: &ExprKind, hints: &mut SlotUseHints) {
    match expr {
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                record_mixed_expr_use_hints(&k.kind, hints);
            }
            if let Some(v) = value {
                record_mixed_expr_use_hints(&v.kind, hints);
            }
        }
        ExprKind::YieldFrom(inner) => record_mixed_expr_use_hints(&inner.kind, hints),
        ExprKind::PostIncrement(name)
        | ExprKind::PostDecrement(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PreDecrement(name) => {
            hints.int.insert(name.clone());
        }
        ExprKind::FunctionCall { name, args } if is_var_dump_call(name.as_str()) => {
            for arg in args {
                record_mixed_expr_use_hints(&arg.kind, hints);
            }
        }
        _ => {}
    }
}

/// Records variables that are read through a Mixed-capable generator
/// path. Supported int expressions nested inside a Mixed context keep
/// their operands int-typed because `classify_mixed_expr` boxes int
/// expressions after evaluating them.
fn record_mixed_expr_use_hints(expr: &ExprKind, hints: &mut SlotUseHints) {
    match expr {
        ExprKind::Variable(name) => {
            hints.mixed.insert(name.clone());
        }
        ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
            record_mixed_expr_use_hints(&left.kind, hints);
            record_mixed_expr_use_hints(&right.kind, hints);
        }
        ExprKind::BinaryOp { left, op, right } if is_generator_int_binop(op) => {
            record_int_expr_use_hints(&left.kind, hints);
            record_int_expr_use_hints(&right.kind, hints);
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                record_int_expr_use_hints(&arg.kind, hints);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                record_mixed_expr_use_hints(&item.kind, hints);
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            record_bool_expr_use_hints(&condition.kind, hints);
            record_mixed_expr_use_hints(&then_expr.kind, hints);
            record_mixed_expr_use_hints(&else_expr.kind, hints);
        }
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                record_mixed_expr_use_hints(&k.kind, hints);
            }
            if let Some(v) = value {
                record_mixed_expr_use_hints(&v.kind, hints);
            }
        }
        ExprKind::YieldFrom(inner)
        | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Print(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Cast { expr: inner, .. } => {
            record_mixed_expr_use_hints(&inner.kind, hints);
        }
        _ => {}
    }
}

/// Records variable uses inside boolean expressions. Null checks against
/// variables require Mixed slots; other supported comparisons stay int-typed.
fn record_bool_expr_use_hints(expr: &ExprKind, hints: &mut SlotUseHints) {
    match expr {
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, BinOp::Eq | BinOp::StrictEq | BinOp::NotEq | BinOp::StrictNotEq)
                && matches!(right.kind, ExprKind::Null) =>
        {
            record_mixed_expr_use_hints(&left.kind, hints);
        }
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, BinOp::Eq | BinOp::StrictEq | BinOp::NotEq | BinOp::StrictNotEq)
                && matches!(left.kind, ExprKind::Null) =>
        {
            record_mixed_expr_use_hints(&right.kind, hints);
        }
        _ => record_int_expr_use_hints(expr, hints),
    }
}

/// Records variables that are read through int-only generator paths such
/// as arithmetic, comparisons, loop counters, and integer helper calls.
fn record_int_expr_use_hints(expr: &ExprKind, hints: &mut SlotUseHints) {
    match expr {
        ExprKind::Variable(name) => {
            hints.int.insert(name.clone());
        }
        ExprKind::BinaryOp { left, right, .. } => {
            record_int_expr_use_hints(&left.kind, hints);
            record_int_expr_use_hints(&right.kind, hints);
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                record_int_expr_use_hints(&arg.kind, hints);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::ErrorSuppress(inner) => {
            record_int_expr_use_hints(&inner.kind, hints);
        }
        _ => {}
    }
}

/// Returns true when the operator is one of the arithmetic operators
/// supported by the current generator integer-expression classifier.
fn is_generator_int_binop(op: &BinOp) -> bool {
    matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div)
}

/// Collected locals with their inferred slot types. Each local is
/// assigned a single SlotType for the lifetime of the generator: the
/// type is decided by the *first* assignment seen in source order.
pub(super) fn collect_locals(body: &[Stmt], param_names: &[String]) -> Vec<(String, SlotType)> {
    let mut locals: Vec<(String, SlotType)> = Vec::new();
    let hints = collect_slot_use_hints(body);
    // `probe`/`probe_types` mirror the eventual params+locals slot table
    // so that classify_int_expr can resolve previously-introduced int
    // locals while we walk subsequent assignments.
    let mut probe: Vec<String> = param_names.to_vec();
    let mut probe_types: Vec<SlotType> = vec![SlotType::Int; param_names.len()];
    visit_assignments(
        body,
        &mut probe,
        &mut probe_types,
        &mut locals,
        param_names,
        &hints,
    );
    locals
}

/// Recursively collects variable assignments within `body`, inferring
/// each local's `SlotType` from the first assignment seen (source order).
/// Skips parameters in `param_names`. Populates `probe`/`probe_types`
/// incrementally so later assignments can reference previously-introduced
/// locals during type inference.
fn visit_assignments(
    body: &[Stmt],
    probe: &mut Vec<String>,
    probe_types: &mut Vec<SlotType>,
    locals: &mut Vec<(String, SlotType)>,
    param_names: &[String],
    hints: &SlotUseHints,
) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                if param_names.iter().any(|p| p == name) {
                    continue;
                }
                if locals.iter().any(|(l, _)| l == name) {
                    continue;
                }
                let inferred = infer_slot_type(name, &value.kind, probe, probe_types, hints);
                if let Some(ty) = inferred {
                    locals.push((name.clone(), ty));
                    probe.push(name.clone());
                    probe_types.push(ty);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                visit_assignments(then_body, probe, probe_types, locals, param_names, hints);
                for (_, b) in elseif_clauses {
                    visit_assignments(b, probe, probe_types, locals, param_names, hints);
                }
                if let Some(eb) = else_body {
                    visit_assignments(eb, probe, probe_types, locals, param_names, hints);
                }
            }
            StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
                visit_assignments(body, probe, probe_types, locals, param_names, hints);
            }
            StmtKind::For { init, body, .. } => {
                if let Some(init_stmt) = init.as_deref() {
                    visit_assignments(
                        std::slice::from_ref(init_stmt),
                        probe,
                        probe_types,
                        locals,
                        param_names,
                        hints,
                    );
                }
                visit_assignments(body, probe, probe_types, locals, param_names, hints);
            }
            StmtKind::Synthetic(stmts) => {
                visit_assignments(stmts, probe, probe_types, locals, param_names, hints);
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                visit_assignments(try_body, probe, probe_types, locals, param_names, hints);
                for catch in catches {
                    visit_assignments(&catch.body, probe, probe_types, locals, param_names, hints);
                }
                if let Some(body) = finally_body {
                    visit_assignments(body, probe, probe_types, locals, param_names, hints);
                }
            }
            _ => {}
        }
    }
}

/// Decide whether an assignment RHS makes the LHS local an Int slot or a
/// Mixed slot. Returns `None` if neither classifier accepts the RHS, in
/// which case the local stays unallocated and any later use bails the
/// generator at that point.
fn infer_slot_type(
    name: &str,
    rhs: &ExprKind,
    probe: &[String],
    probe_types: &[SlotType],
    hints: &SlotUseHints,
) -> Option<SlotType> {
    if classify_int_expr(rhs, probe, probe_types).is_some() {
        return Some(SlotType::Int);
    }
    // String literal / int-array literal / Mixed slot read on the RHS
    // means this local is Mixed-typed.
    match rhs {
        ExprKind::StringLiteral(_) | ExprKind::ArrayLiteral(_) => Some(SlotType::Mixed),
        ExprKind::Variable(name) => {
            let idx = probe.iter().position(|p| p == name)?;
            probe_types.get(idx).copied()
        }
        // `$local = yield <expr>;` receives the value supplied by
        // `Generator::send()`, not the yielded expression. Use the boxed
        // path unless later integer-only operations require the historical
        // int fast path.
        ExprKind::Yield { .. } => {
            if hints.int.contains(name) {
                Some(SlotType::Int)
            } else {
                Some(SlotType::Mixed)
            }
        }
        // `yield from` evaluates to the delegated generator's return
        // value. Store that boxed result in a Mixed slot so it remains
        // available after the delegation completes.
        ExprKind::YieldFrom(_) => Some(SlotType::Mixed),
        _ => None,
    }
}

/// Walks the statement list, building a vector of `ResumeNode`s.
/// Stops on the first `Bail` node and returns what was accumulated
/// up to that point.
pub(super) fn build_nodes(
    body: &[Stmt],
    slots: &[String],
    types: &[SlotType],
    num: &mut StateNumberer,
    data: &mut DataSection,
) -> Vec<ResumeNode> {
    let mut out = Vec::new();
    for stmt in body {
        match build_node(stmt, slots, types, num, data) {
            Some(node) => {
                let bail = matches!(node, ResumeNode::Bail);
                out.push(node);
                if bail {
                    return out;
                }
            }
            None => {
                out.push(ResumeNode::Bail);
                return out;
            }
        }
    }
    out
}

/// Translates a single statement into a `ResumeNode`. Returns `None`
/// for unsupported constructs — the caller converts this to
/// `ResumeNode::Bail`. Handles assign, expr-stmt, if/while/do-while/for
/// loops, break/continue, return, switch, try/finally, echo, and yield/yield-from.
fn build_node(
    stmt: &Stmt,
    slots: &[String],
    types: &[SlotType],
    num: &mut StateNumberer,
    data: &mut DataSection,
) -> Option<ResumeNode> {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let idx = slots.iter().position(|p| p == name)?;
            // `$local = yield <expr>;` — translate as YieldAssign. The
            // slot type decides whether the sent value is unboxed to int
            // or moved as an owned Mixed cell.
            if let ExprKind::Yield { key, value } = &value.kind {
                let local_ty = types.get(idx).copied()?;
                let yield_value = match value.as_deref() {
                    Some(v) => classify_mixed_expr(&v.kind, slots, types, data)?,
                    None => MixedSource::Null,
                };
                let yield_key = match key.as_deref() {
                    None => None,
                    Some(Expr { kind: k, .. }) => Some(classify_mixed_expr(k, slots, types, data)?),
                };
                let state_idx = num.next();
                return Some(ResumeNode::YieldAssign {
                    local_idx: idx,
                    local_ty,
                    yield_entry: YieldEntry { key: yield_key, value: yield_value },
                    state_idx,
                });
            }
            if let ExprKind::YieldFrom(inner) = &value.kind {
                if types.get(idx).copied() != Some(SlotType::Mixed) {
                    return None;
                }
                return build_yield_from_node(
                    inner,
                    YieldFromResult::Local(idx),
                    slots,
                    types,
                    num,
                    data,
                );
            }
            // Otherwise: dispatch on the slot's type.
            match types.get(idx).copied() {
                Some(SlotType::Int) => {
                    let src = classify_int_expr(&value.kind, slots, types)?;
                    Some(ResumeNode::Stmt(BodyStmt::AssignInt(idx, src)))
                }
                Some(SlotType::Mixed) => {
                    let src = classify_mixed_expr(&value.kind, slots, types, data)?;
                    Some(ResumeNode::Stmt(BodyStmt::AssignMixed(idx, src)))
                }
                None => None,
            }
        }
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::YieldFrom(inner) => {
                build_yield_from_node(
                    inner,
                    YieldFromResult::Discard,
                    slots,
                    types,
                    num,
                    data,
                )
            }
            ExprKind::Yield { key, value } => {
                let value = match value.as_deref() {
                    Some(v) => classify_mixed_expr(&v.kind, slots, types, data)?,
                    None => MixedSource::Null,
                };
                let key = match key.as_deref() {
                    None => None,
                    Some(Expr { kind: k, .. }) => Some(classify_mixed_expr(k, slots, types, data)?),
                };
                let state = num.next();
                Some(ResumeNode::Yield(YieldEntry { key, value }, state))
            }
            ExprKind::PostIncrement(name) => {
                let idx = slots.iter().position(|p| p == name)?;
                if types.get(idx).copied() != Some(SlotType::Int) {
                    return None;
                }
                Some(ResumeNode::Stmt(BodyStmt::PostIncrement(idx)))
            }
            ExprKind::PostDecrement(name) => {
                let idx = slots.iter().position(|p| p == name)?;
                if types.get(idx).copied() != Some(SlotType::Int) {
                    return None;
                }
                Some(ResumeNode::Stmt(BodyStmt::PostDecrement(idx)))
            }
            ExprKind::FunctionCall { name, args } if is_var_dump_call(name.as_str()) => {
                let mut stmts = Vec::with_capacity(args.len());
                for arg in args {
                    let src = classify_mixed_expr(&arg.kind, slots, types, data)?;
                    stmts.push(ResumeNode::Stmt(BodyStmt::VarDumpMixed(src)));
                }
                Some(ResumeNode::Block { stmts })
            }
            _ => None,
        },
        StmtKind::Echo(expr) => build_echo_node(expr, slots, types, data),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let cond = classify_bool_expr(&condition.kind, slots, types)?;
            let then_nodes = build_nodes(then_body, slots, types, num, data);
            let else_nodes = build_else_chain(elseif_clauses, else_body, slots, types, num, data)?;
            Some(ResumeNode::If {
                cond,
                then_body: then_nodes,
                else_body: else_nodes,
            })
        }
        StmtKind::While { condition, body } => {
            let cond = classify_bool_expr(&condition.kind, slots, types)?;
            let body_nodes = build_nodes(body, slots, types, num, data);
            Some(ResumeNode::While { cond, body: body_nodes })
        }
        StmtKind::DoWhile { body, condition } => {
            let cond = classify_bool_expr(&condition.kind, slots, types)?;
            let body_nodes = build_nodes(body, slots, types, num, data);
            Some(ResumeNode::DoWhile { cond, body: body_nodes })
        }
        StmtKind::For { init, condition, update, body } => {
            let init_nodes = match init.as_deref() {
                Some(s) => build_nodes(std::slice::from_ref(s), slots, types, num, data),
                None => Vec::new(),
            };
            let cond = match condition {
                Some(c) => classify_bool_expr(&c.kind, slots, types)?,
                None => return None,
            };
            let body_nodes = build_nodes(body, slots, types, num, data);
            let update_nodes = match update.as_deref() {
                Some(s) => build_nodes(std::slice::from_ref(s), slots, types, num, data),
                None => Vec::new(),
            };
            Some(ResumeNode::For {
                init: init_nodes,
                cond,
                update: update_nodes,
                body: body_nodes,
            })
        }
        StmtKind::Break(_) => Some(ResumeNode::Break),
        StmtKind::Continue(_) => Some(ResumeNode::Continue),
        StmtKind::Return(opt) => {
            if let Some(expr) = opt {
                if let ExprKind::YieldFrom(inner) = &expr.kind {
                    return build_yield_from_node(
                        inner,
                        YieldFromResult::Return,
                        slots,
                        types,
                        num,
                        data,
                    );
                }
            }
            let value = match opt {
                Some(expr) => Some(classify_mixed_expr(&expr.kind, slots, types, data)?),
                None => None,
            };
            Some(ResumeNode::Return(value))
        }
        StmtKind::Switch { subject, cases, default } => {
            let subject_src = classify_int_expr(&subject.kind, slots, types)?;
            let mut translated_cases: Vec<(Vec<i64>, Vec<ResumeNode>)> = Vec::new();
            for (values, body) in cases {
                let mut int_values = Vec::with_capacity(values.len());
                for v in values {
                    if let ExprKind::IntLiteral(n) = &v.kind {
                        int_values.push(*n);
                    } else {
                        return None;
                    }
                }
                let body_nodes = build_nodes(body, slots, types, num, data);
                translated_cases.push((int_values, body_nodes));
            }
            let default_nodes = match default {
                Some(d) => build_nodes(d, slots, types, num, data),
                None => Vec::new(),
            };
            Some(ResumeNode::Switch {
                subject: subject_src,
                cases: translated_cases,
                default: default_nodes,
            })
        }
        StmtKind::Try {
            try_body,
            finally_body,
            ..
        } => {
            let try_nodes = build_nodes(try_body, slots, types, num, data);
            let finally_nodes = finally_body
                .as_ref()
                .map(|body| build_nodes(body, slots, types, num, data))
                .unwrap_or_default();
            Some(ResumeNode::Try {
                try_body: try_nodes,
                finally_body: finally_nodes,
            })
        }
        StmtKind::Synthetic(stmts) => Some(ResumeNode::Block {
            stmts: build_nodes(stmts, slots, types, num, data),
        }),
        _ => None,
    }
}

/// Translates `echo <expr>` into generator IR.
///
/// The narrow generator IR lowers concat echo expressions by emitting each
/// operand in source order, and lowers ternary echo expressions as an `If`
/// whose branches each emit one echo. Other expressions use the normal Mixed
/// boxing path.
fn build_echo_node(
    expr: &Expr,
    slots: &[String],
    types: &[SlotType],
    data: &mut DataSection,
) -> Option<ResumeNode> {
    if let ExprKind::BinaryOp {
        left,
        op: BinOp::Concat,
        right,
    } = &expr.kind
    {
        return Some(ResumeNode::Block {
            stmts: vec![
                build_echo_node(left, slots, types, data)?,
                build_echo_node(right, slots, types, data)?,
            ],
        });
    }

    if let ExprKind::Ternary {
        condition,
        then_expr,
        else_expr,
    } = &expr.kind
    {
        let cond = classify_bool_expr(&condition.kind, slots, types)?;
        let then_node = build_echo_node(then_expr, slots, types, data)?;
        let else_node = build_echo_node(else_expr, slots, types, data)?;
        return Some(ResumeNode::If {
            cond,
            then_body: vec![then_node],
            else_body: vec![else_node],
        });
    }

    let src = classify_mixed_expr(&expr.kind, slots, types, data)?;
    Some(ResumeNode::Stmt(BodyStmt::EchoMixed(src)))
}

/// Returns true for PHP's global `var_dump` builtin name as it may appear
/// after name resolution. Generator lowering uses this to keep simple
/// diagnostic expression statements from bailing the narrow generator IR.
fn is_var_dump_call(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case("var_dump")
}

/// Translates a `yield from` expression into a `ResumeNode`. Handles
/// three shapes: array literal (unpacked into individual yields),
/// function call (yield-from-generator with Call source, arg count ≤ 8),
/// and variable (yield-from-generator with IntSlot or MixedSlot source).
/// `result` indicates how the final value is consumed (Discard, Local,
/// or Return). Returns `None` for unsupported shapes.
fn build_yield_from_node(
    inner: &Expr,
    result: YieldFromResult,
    slots: &[String],
    types: &[SlotType],
    num: &mut StateNumberer,
    data: &mut DataSection,
) -> Option<ResumeNode> {
    if let ExprKind::ArrayLiteral(items) = &inner.kind {
        let mut stmts = Vec::new();
        for item in items {
            let value = classify_mixed_expr(&item.kind, slots, types, data)?;
            let state = num.next();
            stmts.push(ResumeNode::Yield(YieldEntry { key: None, value }, state));
        }
        match result {
            YieldFromResult::Discard => {}
            YieldFromResult::Local(idx) => {
                stmts.push(ResumeNode::Stmt(BodyStmt::AssignMixed(idx, MixedSource::Null)));
            }
            YieldFromResult::Return => {
                stmts.push(ResumeNode::Return(Some(MixedSource::Null)));
            }
        }
        return Some(ResumeNode::Block { stmts });
    }
    if let ExprKind::FunctionCall { name, args } = &inner.kind {
        if args.len() > 8 {
            return None;
        }
        let mut arg_sources = Vec::with_capacity(args.len());
        for arg in args {
            arg_sources.push(classify_int_expr(&arg.kind, slots, types)?);
        }
        let state_idx = num.next();
        return Some(ResumeNode::YieldFromGenerator {
            source: YieldFromSource::Call {
                fn_name: name.as_str().to_string(),
                args: arg_sources,
            },
            state_idx,
            result,
        });
    }
    if let ExprKind::Variable(name) = &inner.kind {
        // `yield from $local` — the slot holds either a raw Generator
        // pointer (Int-typed slot) or a boxed Mixed cell wrapping an
        // Object payload (Mixed slot).
        if let Some(idx) = slot_idx_of_type(name, slots, types, SlotType::Int) {
            let state_idx = num.next();
            return Some(ResumeNode::YieldFromGenerator {
                source: YieldFromSource::IntSlot(idx),
                state_idx,
                result,
            });
        }
        if let Some(idx) = slot_idx_of_type(name, slots, types, SlotType::Mixed) {
            let state_idx = num.next();
            return Some(ResumeNode::YieldFromGenerator {
                source: YieldFromSource::MixedSlot(idx),
                state_idx,
                result,
            });
        }
    }
    None
}

/// Recursively translates if/else-if/else chains into a flat vector of
/// nested `ResumeNode::If` nodes. The else branch is resolved by
/// re-invoking `build_nodes`. Returns `Some(nodes)` or `None` if any
/// condition fails to classify.
fn build_else_chain(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    slots: &[String],
    types: &[SlotType],
    num: &mut StateNumberer,
    data: &mut DataSection,
) -> Option<Vec<ResumeNode>> {
    if let Some(((cond_expr, then_body), rest)) = elseif_clauses.split_first() {
        let cond = classify_bool_expr(&cond_expr.kind, slots, types)?;
        let then_nodes = build_nodes(then_body, slots, types, num, data);
        let rest_vec: Vec<(Expr, Vec<Stmt>)> = rest.to_vec();
        let else_nodes = build_else_chain(&rest_vec, else_body, slots, types, num, data)?;
        Some(vec![ResumeNode::If {
            cond,
            then_body: then_nodes,
            else_body: else_nodes,
        }])
    } else if let Some(eb) = else_body {
        Some(build_nodes(eb, slots, types, num, data))
    } else {
        Some(Vec::new())
    }
}
