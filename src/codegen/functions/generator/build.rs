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
        let (label, len) = data.add_string(s.as_bytes());
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

pub(super) fn classify_bool_expr(
    expr: &ExprKind,
    slots: &[String],
    types: &[SlotType],
) -> Option<BoolExpr> {
    if let ExprKind::BinaryOp { left, op, right } = expr {
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
        return Some(BoolExpr { left: l, op: cmp, right: r });
    }
    None
}

/// Collected locals with their inferred slot types. Each local is
/// assigned a single SlotType for the lifetime of the generator: the
/// type is decided by the *first* assignment seen in source order.
pub(super) fn collect_locals(body: &[Stmt], param_names: &[String]) -> Vec<(String, SlotType)> {
    let mut locals: Vec<(String, SlotType)> = Vec::new();
    // `probe`/`probe_types` mirror the eventual params+locals slot table
    // so that classify_int_expr can resolve previously-introduced int
    // locals while we walk subsequent assignments.
    let mut probe: Vec<String> = param_names.to_vec();
    let mut probe_types: Vec<SlotType> = vec![SlotType::Int; param_names.len()];
    visit_assignments(body, &mut probe, &mut probe_types, &mut locals, param_names);
    locals
}

fn visit_assignments(
    body: &[Stmt],
    probe: &mut Vec<String>,
    probe_types: &mut Vec<SlotType>,
    locals: &mut Vec<(String, SlotType)>,
    param_names: &[String],
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
                let inferred = infer_slot_type(&value.kind, probe, probe_types);
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
                visit_assignments(then_body, probe, probe_types, locals, param_names);
                for (_, b) in elseif_clauses {
                    visit_assignments(b, probe, probe_types, locals, param_names);
                }
                if let Some(eb) = else_body {
                    visit_assignments(eb, probe, probe_types, locals, param_names);
                }
            }
            StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
                visit_assignments(body, probe, probe_types, locals, param_names);
            }
            StmtKind::For { init, body, .. } => {
                if let Some(init_stmt) = init.as_deref() {
                    visit_assignments(
                        std::slice::from_ref(init_stmt),
                        probe,
                        probe_types,
                        locals,
                        param_names,
                    );
                }
                visit_assignments(body, probe, probe_types, locals, param_names);
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
    rhs: &ExprKind,
    probe: &[String],
    probe_types: &[SlotType],
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
        // `$local = yield <expr>;` keeps the historical Int behaviour:
        // the unbox path stores the sent int into the slot. Mixed-typed
        // sends are deferred to a later iteration.
        ExprKind::Yield { .. } => Some(SlotType::Int),
        _ => None,
    }
}

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
            // `$local = yield <expr>;` — translate as YieldAssign. v1
            // unboxes the int sent_value into the slot, so the slot must
            // be Int-typed.
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
                if let ExprKind::ArrayLiteral(items) = &inner.kind {
                    let mut stmts = Vec::new();
                    for item in items {
                        let value = classify_mixed_expr(&item.kind, slots, types, data)?;
                        let state = num.next();
                        stmts.push(ResumeNode::Yield(YieldEntry { key: None, value }, state));
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
                    });
                }
                if let ExprKind::Variable(name) = &inner.kind {
                    // `yield from $local` — the slot holds either a raw
                    // Generator pointer (Int-typed slot) or a boxed
                    // Mixed cell wrapping an Object payload (Mixed slot).
                    if let Some(idx) = slot_idx_of_type(name, slots, types, SlotType::Int) {
                        let state_idx = num.next();
                        return Some(ResumeNode::YieldFromGenerator {
                            source: YieldFromSource::IntSlot(idx),
                            state_idx,
                        });
                    }
                    if let Some(idx) = slot_idx_of_type(name, slots, types, SlotType::Mixed) {
                        let state_idx = num.next();
                        return Some(ResumeNode::YieldFromGenerator {
                            source: YieldFromSource::MixedSlot(idx),
                            state_idx,
                        });
                    }
                }
                None
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
            _ => None,
        },
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
        _ => None,
    }
}

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
