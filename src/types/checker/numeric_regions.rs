//! Purpose:
//! Finds disjoint local binding regions whose names are dead between regions.
//!
//! Called from:
//! - Function, method and closure storage setup before statement checking.
//!
//! Key details:
//! - A region starts at a plain assignment or a for initializer and never nests.
//! - Unknown scope access and reference-bearing constructs conservatively veto splitting.

use std::collections::{HashMap, HashSet};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

#[derive(Default)]
struct Facts {
    sites: HashMap<String, HashSet<Span>>,
    outside: HashSet<String>,
    untrusted: HashSet<String>,
    veto: bool,
    dependents: HashMap<String, HashSet<String>>,
}

/// Finds starts only for names whose every occurrence belongs to an initialized region.
pub(super) fn find(body: &[Stmt], incoming: &HashMap<String, crate::types::PhpType>) -> (HashMap<Span, HashSet<String>>, HashMap<String, HashSet<String>>) {
    let mut facts = Facts::default();
    statements(body, &HashSet::new(), &mut facts);
    let mut result: HashMap<Span, HashSet<String>> = HashMap::new();
    if facts.veto { return (result, HashMap::new()); }
    let mut pending: Vec<_> = facts.untrusted.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        for dependent in facts.dependents.get(&name).into_iter().flatten() {
            if facts.untrusted.insert(dependent.clone()) { pending.push(dependent.clone()); }
        }
    }
    for (name, sites) in facts.sites {
        if facts.outside.contains(&name) || facts.untrusted.contains(&name) || incoming.contains_key(&name)
            || crate::superglobals::is_superglobal(&name) { continue; }
        for site in sites {
            if site.identifies_a_node() { result.entry(site).or_default().insert(name.clone()); }
        }
    }
    (result, facts.dependents)
}

/// Records a use outside all regions, including reads before an enclosing loop's initializer.
fn occurrence(name: &str, active: &HashSet<String>, facts: &mut Facts) {
    if !active.contains(name) { facts.outside.insert(name.to_owned()); }
}

/// Scans a list with assignment regions lasting to its end, without exporting them to a join.
fn statements(body: &[Stmt], active: &HashSet<String>, facts: &mut Facts) {
    let mut active = active.clone();
    for stmt in body { statement(stmt, &mut active, facts); }
}

/// Starts a maximal region after scanning the initializer's reads of the previous binding.
fn assignment(name: &str, value: &Expr, span: Span, active: &mut HashSet<String>, facts: &mut Facts) {
    expression(value, active, facts);
    dependencies(value, name, &mut facts.dependents);
    if active.insert(name.to_owned()) {
        facts.sites.entry(name.to_owned()).or_default().insert(span);
    }
}

/// Traverses supported control flow; aliases, exceptional scopes and future statements fail closed.
fn statement(stmt: &Stmt, active: &mut HashSet<String>, facts: &mut Facts) {
    match &stmt.kind {
        StmtKind::Assign { name, value } => assignment(name, value, stmt.span, active, facts),
        StmtKind::For { init, condition, update, body } => {
            let mut region = active.clone();
            if let Some(init) = init { statement(init, &mut region, facts); }
            if let Some(condition) = condition { expression(condition, &region, facts); }
            statements(body, &region, facts);
            if let Some(update) = update { statement(update, &mut region, facts); }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expression(condition, active, facts);
            statements(body, active, facts);
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expression(condition, active, facts);
            statements(then_body, active, facts);
            for (condition, body) in elseif_clauses {
                expression(condition, active, facts);
                statements(body, active, facts);
            }
            if let Some(body) = else_body { statements(body, active, facts); }
        }
        StmtKind::Synthetic(body) => {
            for stmt in body { statement(stmt, active, facts); }
        }
        StmtKind::Echo(value) | StmtKind::ExprStmt(value) | StmtKind::Return(Some(value)) => expression(value, active, facts),
        StmtKind::ArrayAssign { array, index, value } => {
            facts.outside.insert(array.clone()); expression(index, active, facts); expression(value, active, facts);
        }
        StmtKind::ArrayPush { array, value } => { facts.outside.insert(array.clone()); expression(value, active, facts); }
        StmtKind::NestedArrayAssign { target, value } => { expression(target, active, facts); expression(value, active, facts); }
        StmtKind::PropertyAssign { object, value, .. } | StmtKind::PropertyArrayPush { object, value, .. } => {
            expression(object, active, facts); expression(value, active, facts);
        }
        StmtKind::PropertyArrayAssign { object, index, value, .. } => {
            expression(object, active, facts); expression(index, active, facts); expression(value, active, facts);
        }
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
        _ => facts.veto = true,
    }
}

/// Reuses the executable-child walker, while accounting for names embedded directly in nodes.
fn expression(expr: &Expr, active: &HashSet<String>, facts: &mut Facts) {
    match &expr.kind {
        ExprKind::Variable(name) | ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => occurrence(name, active, facts),
        ExprKind::FunctionCall { name, .. } if ["eval", "compact", "extract", "get_defined_vars", "unset"]
            .iter().any(|candidate| name.as_str().trim_start_matches('\\').eq_ignore_ascii_case(candidate)) => facts.veto = true,
        // Captures can outlive a binding, even when their creation is inside its region.
        ExprKind::Closure { captures, capture_refs, .. } => {
            facts.outside.extend(captures.iter().chain(capture_refs).cloned());
            facts.untrusted.extend(capture_refs.iter().cloned());
        }
        ExprKind::ClosureCall { .. } | ExprKind::ExprCall { .. } | ExprKind::Pipe { .. } => facts.veto = true,
        ExprKind::Assignment { .. } | ExprKind::Yield { .. } | ExprKind::YieldFrom(_)
        | ExprKind::IncludeValue { .. } => facts.veto = true,
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::This | ExprKind::ConstRef(_)
        | ExprKind::BinaryOp { .. } | ExprKind::InstanceOf { .. } | ExprKind::Negate(_)
        | ExprKind::Not(_) | ExprKind::BitNot(_) | ExprKind::Throw(_) | ExprKind::ErrorSuppress(_)
        | ExprKind::Print(_) | ExprKind::NullCoalesce { .. } | ExprKind::FunctionCall { .. }
        | ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_)
        | ExprKind::Match { .. } | ExprKind::ArrayAccess { .. } | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. } | ExprKind::Cast { .. } | ExprKind::NamedArg { .. }
        | ExprKind::Spread(_) | ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. }
        | ExprKind::Clone(_) | ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. } | ExprKind::NullsafeDynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } | ExprKind::MethodCall { .. } | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. } | ExprKind::StaticMethodCall { .. }
        | ExprKind::FirstClassCallable(_) | ExprKind::PtrCast { .. } | ExprKind::BufferNew { .. }
        | ExprKind::ClassConstant { .. } | ExprKind::ObjectClassName { .. }
        | ExprKind::ScopedConstantAccess { .. } | ExprKind::MagicConstant(_) => {}
        _ => facts.veto = true,
    }
    // Without consulting call signatures, a directly passed local may escape by reference.
    match &expr.kind {
        ExprKind::FunctionCall { args, .. } | ExprKind::MethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } | ExprKind::NullsafeMethodCall { args, .. }
        | ExprKind::NullsafeDynamicMethodCall { args, .. } => {
            for arg in args {
                let arg = match &arg.kind { ExprKind::NamedArg { value, .. } => value, _ => arg };
                if let ExprKind::Variable(name) = &arg.kind {
                    facts.outside.insert(name.clone()); facts.untrusted.insert(name.clone());
                }
            }
        }
        ExprKind::NewGeneric { .. } | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. } => facts.veto = true,
        _ => {}
    }
    super::loop_storage::visit_child_expressions(expr, &mut |child| expression(child, active, facts));
}

/// Accepts concrete numeric values and arithmetic over closed numeric binding dependencies.
/// Unknown Mixed origins veto their complete transitive dependency graph.
pub(super) fn numeric_assignment(checker: &super::Checker, value: &Expr,
    ty: &crate::types::PhpType, env: &crate::types::TypeEnv) -> bool {
    use crate::types::PhpType;
    if matches!(ty, PhpType::Int | PhpType::Float) || ty.is_int_float_union() { return true; }
    if *ty != PhpType::Mixed { return false; }
    numeric_expression(checker, value, env)
}

/// Proves the numeric result of an overflow-capable expression without repeating checker inference.
fn numeric_expression(checker: &super::Checker, expr: &Expr, env: &crate::types::TypeEnv) -> bool {
    use crate::parser::ast::{BinOp, CastType};
    use crate::types::PhpType;
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) => true,
        ExprKind::Variable(name) => env.get(name).is_some_and(|ty| {
            matches!(ty, PhpType::Int | PhpType::Float) || ty.is_int_float_union()
                || (*ty == PhpType::Mixed && !checker.mixed_storage_locals.contains(name)
                    && checker.numeric_region_sites.values().any(|names| names.contains(name)))
        }),
        ExprKind::Cast { target: CastType::Int | CastType::Float, .. } => true,
        ExprKind::Negate(value) => numeric_expression(checker, value, env),
        ExprKind::BinaryOp { left, op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow, right } => {
            numeric_expression(checker, left, env) && numeric_expression(checker, right, env)
        }
        _ => numeric_leaf_type(checker, expr, env).is_some_and(|ty| {
            matches!(ty, PhpType::Int | PhpType::Float) || ty.is_int_float_union()
        }),
    }
}

/// Reads authoritative property and buffer element types without inferring expressions again.
fn numeric_leaf_type(checker: &super::Checker, expr: &Expr, env: &crate::types::TypeEnv) -> Option<crate::types::PhpType> {
    use crate::types::PhpType;
    match &expr.kind {
        ExprKind::Variable(name) => env.get(name).cloned(),
        ExprKind::This => checker.current_class.clone().map(PhpType::Object),
        ExprKind::PropertyAccess { object, property } => {
            let PhpType::Object(class) = numeric_leaf_type(checker, object, env)? else { return None; };
            if !checker.classes.get(&class)?.visible_property_is_declared(property) { return None; }
            checker.infer_property_on_class_type(&class, property, expr).ok()
        }
        ExprKind::ArrayAccess { array, .. } => {
            let PhpType::Buffer(element) = numeric_leaf_type(checker, array, env)? else { return None; };
            Some(*element)
        }
        _ => None,
    }
}

/// Records transitive numeric dependencies so a later unknown writer retracts dependent proofs.
fn dependencies(expr: &Expr, assigned: &str, out: &mut HashMap<String, HashSet<String>>) {
    match &expr.kind {
        // Declared property types constrain the value independently of the receiver's local type.
        ExprKind::PropertyAccess { .. } | ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. } | ExprKind::StaticMethodCall { .. }
        | ExprKind::Cast { target: crate::parser::ast::CastType::Int | crate::parser::ast::CastType::Float, .. } => return,
        ExprKind::Variable(name) | ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
            out.entry(name.clone()).or_default().insert(assigned.to_owned());
        }
        _ => {}
    }
    super::loop_storage::visit_child_expressions(expr, &mut |child| dependencies(child, assigned, out));
}
