//! Purpose:
//! Proves that numeric and buffer arguments cannot change a native array_push receiver.
//!
//! Called from:
//! - The array_push checker before admitting a value to native copy/write-back storage.
//!
//! Key details:
//! - Numeric leaves have known types and no external aliases; casts must not warn.
//! - Integer interval proofs keep casts and factory buffer operations free of overflow coercions.
//! - Factories use only their integer value parameters, fresh integer buffers and bounded loops.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Stmt, StmtKind, TypeExpr};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

type Range = (i64, i64);
type Locals = HashMap<String, LocalValue>;

/// Values available to a factory without reaching caller storage or PHP coercion handlers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalValue { Integer(Range), Buffer }

impl Checker {
    /// Admits computation whose operations cannot enter PHP or mutate the selected place.
    pub(crate) fn array_push_value_is_local(&self, value: &Expr, env: &TypeEnv) -> bool {
        if self.array_push_numeric_expression(value, env) { return true; }
        if let ExprKind::Cast { target: CastType::String, expr } = &value.kind {
            return self.array_push_numeric_expression(expr, env);
        }
        if let ExprKind::ArrayAccess { array, index } = &value.kind {
            return matches!(&array.kind, ExprKind::Variable(name)
                if self.array_push_plain_local(name)
                    && matches!(env.get(name), Some(PhpType::Buffer(_))))
                && self.array_push_integer_range(index, env).is_some();
        }
        if let ExprKind::BufferNew { len, .. } = &value.kind {
            // Buffer length lowering has its own silent, exact numeric validation.
            return self.array_push_numeric_expression(len, env);
        }
        let ExprKind::FunctionCall { name, args } = &value.kind else { return false; };
        if crate::builtins::registry::lookup(name.as_str()).is_some() { return false; }
        let Some(decl) = self.fn_decls.get(name.as_str()) else { return false; };
        let Some(sig) = self.functions.get(name.as_str()) else { return false; };
        if !matches!(&sig.return_type, PhpType::Buffer(element) if **element == PhpType::Int)
            || sig.is_generator || sig.by_ref_return || decl.by_ref_return
            || sig.deprecation.is_some() || !decl.attributes.is_empty()
            || decl.param_attributes.iter().any(|attributes| !attributes.is_empty())
            || decl.variadic.is_some() || sig.variadic.is_some()
            || decl.params.len() != args.len() || sig.params.len() != args.len()
            || decl.ref_params.iter().any(|by_ref| *by_ref)
            || sig.ref_params.iter().any(|by_ref| *by_ref)
            || decl.param_types.iter().any(Option::is_none)
            || sig.params.iter().any(|(_, ty)| *ty != PhpType::Int)
            || decl.params.iter().any(|name| !local_name(name))
            || self.function_variant_groups.get(name.as_str()).is_some_and(|variants|
                variants.len() != 1 || variants[0] != name.as_str())
        { return false; }
        let ranges: Option<Vec<Range>> = args.iter().map(|arg| match &arg.kind {
            ExprKind::IntLiteral(_) | ExprKind::Cast { target: CastType::Int, .. } =>
                self.array_push_integer_range(arg, env),
            ExprKind::Variable(name) if env.get(name) == Some(&PhpType::Int) =>
                self.array_push_integer_range(arg, env),
            // Named/spread arguments need the shared planner, not a positional interval zip.
            _ => None,
        }).collect();
        let Some(ranges) = ranges else { return false; };
        let mut locals = decl.params.iter().cloned().zip(ranges.into_iter().map(LocalValue::Integer)).collect();
        local_buffer_block(&decl.body, &mut locals).is_some_and(|returns_buffer| returns_buffer)
    }

    /// Numeric arithmetic/comparisons cannot warn, while int/bool casts need integer operands.
    fn array_push_numeric_expression(&self, value: &Expr, env: &TypeEnv) -> bool {
        match &value.kind {
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) => true,
            ExprKind::Variable(name) => self.array_push_plain_local(name)
                && env.get(name).is_some_and(|ty| matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool)
                    || ty.is_int_float_union()),
            ExprKind::Negate(expr) | ExprKind::Cast { target: CastType::Float, expr } =>
                self.array_push_numeric_expression(expr, env),
            ExprKind::Not(expr)
            | ExprKind::Cast { target: CastType::Int | CastType::Bool, expr } =>
                self.array_push_integer_range(expr, env).is_some(),
            ExprKind::BinaryOp { left, op, right } if local_numeric_operator(op) =>
                self.array_push_numeric_expression(left, env) && self.array_push_numeric_expression(right, env),
            _ => false,
        }
    }

    /// Bounds integer expressions before casts or index conversions can invoke a warning handler.
    fn array_push_integer_range(&self, value: &Expr, env: &TypeEnv) -> Option<Range> {
        match &value.kind {
            ExprKind::IntLiteral(value) => Some((*value, *value)),
            ExprKind::BoolLiteral(value) => Some((i64::from(*value), i64::from(*value))),
            ExprKind::Variable(name) if self.array_push_plain_local(name) => {
                if let Some(max) = self.packed_loop_counter.as_ref()
                    .filter(|counter| counter.name == *name).and_then(|counter| counter.literal_max)
                { return Some((0, max)); }
                match env.get(name) {
                    Some(PhpType::Int) => Some((i64::MIN, i64::MAX)),
                    Some(PhpType::Bool) => Some((0, 1)),
                    _ => None,
                }
            }
            ExprKind::Cast { target: CastType::Int, expr } => self.array_push_integer_range(expr, env),
            ExprKind::Not(expr) | ExprKind::Cast { target: CastType::Bool, expr } =>
                self.array_push_integer_range(expr, env).map(|_| (0, 1)),
            ExprKind::Negate(expr) => negate_range(self.array_push_integer_range(expr, env)?),
            ExprKind::BinaryOp { left, op, right } if comparison(op) =>
                (self.array_push_numeric_expression(left, env) && self.array_push_numeric_expression(right, env)).then_some((0, 1)),
            ExprKind::BinaryOp { left, op, right } => integer_binary(
                self.array_push_integer_range(left, env)?, op, self.array_push_integer_range(right, env)?),
            _ => None,
        }
    }

    /// External aliases, globals, and untyped parameters can defeat the local numeric type fact.
    fn array_push_plain_local(&self, name: &str) -> bool {
        local_name(name) && !self.active_globals.contains(name)
            && !self.active_ref_params.contains(name)
            && !self.active_external_ref_bindings.contains(name)
            && !self.boxed_parameter_locals.contains(name)
            && !self.ref_aliased_locals.contains(name)
            && !self.boxed_ref_aliased_locals.contains(name)
            && !self.eval_barrier_active
            && !(self.current_function.is_none() && self.current_class.is_none()
                && self.closure_depth == 0 && self.program_global_names.contains(name))
    }
}

/// Excludes operators with division, shift, string, or implicit integer-conversion diagnostics.
fn local_numeric_operator(op: &BinOp) -> bool {
    matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) || comparison(op)
}

/// Numeric comparisons produce a boolean without coercing a NaN to boolean.
fn comparison(op: &BinOp) -> bool {
    matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq
        | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq)
}

/// Checks all arithmetic extrema in wider storage before publishing an integer range.
fn integer_binary(left: Range, op: &BinOp, right: Range) -> Option<Range> {
    if comparison(op) { return Some((0, 1)); }
    let mut values = Vec::with_capacity(4);
    for left in [left.0 as i128, left.1 as i128] {
        for right in [right.0 as i128, right.1 as i128] {
            values.push(i64::try_from(match op {
                BinOp::Add => left + right, BinOp::Sub => left - right,
                BinOp::Mul => left * right, _ => return None,
            }).ok()?);
        }
    }
    Some((*values.iter().min()?, *values.iter().max()?))
}

/// Integer negation must not promote INT_MIN to a floating value.
fn negate_range(range: Range) -> Option<Range> {
    Some((range.1.checked_neg()?, range.0.checked_neg()?))
}

/// Implicit globals and $this cannot become factory-owned locals by assignment.
fn local_name(name: &str) -> bool {
    name != "GLOBALS" && name != "this" && !crate::superglobals::is_superglobal(name)
}

/// Tracks definite bindings and buffer returns through the closed statement subset.
fn local_buffer_block(body: &[Stmt], locals: &mut Locals) -> Option<bool> {
    let mut returns_buffer = false;
    for statement in body { returns_buffer |= local_buffer_statement(statement, locals)?; }
    Some(returns_buffer)
}

/// Admits local integer/buffer operations and bounded for loops without loop-carried rebindings.
fn local_buffer_statement(statement: &Stmt, locals: &mut Locals) -> Option<bool> {
    match &statement.kind {
        StmtKind::Synthetic(body) => local_buffer_block(body, locals),
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let kind = local_buffer_expression(value, locals)?;
            if !local_name(name) || locals.get(name).is_some_and(|old|
                matches!(old, LocalValue::Buffer) != matches!(kind, LocalValue::Buffer))
            { return None; }
            locals.insert(name.clone(), kind);
            Some(false)
        }
        StmtKind::ArrayAssign { array, index, value } =>
            (locals.get(array) == Some(&LocalValue::Buffer)
                && local_integer(index, locals).is_some() && local_integer(value, locals).is_some()).then_some(false),
        StmtKind::ExprStmt(value) => { local_buffer_expression(value, locals)?; Some(false) }
        StmtKind::Return(Some(value)) =>
            (local_buffer_expression(value, locals) == Some(LocalValue::Buffer)).then_some(true),
        StmtKind::For { init, condition, update, body } => {
            let counter = super::super::packed_counter::packed_for_counter(
                init.as_deref(), condition.as_ref(), update.as_deref(), body, 0)?;
            // Re-evaluating the body must not widen an existing local on each iteration.
            if counter.rebound_locals.iter().any(|name| locals.contains_key(name)) { return None; }
            local_buffer_statement(init.as_deref()?, locals)?;
            let ExprKind::BinaryOp { left, op: BinOp::Lt, right } = &condition.as_ref()?.kind else { return None; };
            if !matches!(&left.kind, ExprKind::Variable(name) if *name == counter.name) { return None; }
            let (_, limit) = local_integer(right, locals)?;
            let mut iteration = locals.clone();
            iteration.insert(counter.name.clone(), LocalValue::Integer((0, limit.saturating_sub(1).max(0))));
            let returns = local_buffer_block(body, &mut iteration)?;
            locals.insert(counter.name, LocalValue::Integer((0, limit.max(0))));
            Some(returns)
        }
        _ => None,
    }
}

/// Numeric buffer operations cannot enter PHP; only fresh int buffers carry integer reads.
fn local_buffer_expression(value: &Expr, locals: &Locals) -> Option<LocalValue> {
    if let Some(range) = local_integer(value, locals) { return Some(LocalValue::Integer(range)); }
    match &value.kind {
        ExprKind::Variable(name) => locals.get(name).copied(),
        ExprKind::BufferNew { element_type: TypeExpr::Int, len }
            if local_integer(len, locals).is_some() => Some(LocalValue::Buffer),
        _ => None,
    }
}

/// Every intermediate stays in the integer range, including implicit buffer-write conversions.
fn local_integer(value: &Expr, locals: &Locals) -> Option<Range> {
    match &value.kind {
        ExprKind::IntLiteral(value) => Some((*value, *value)),
        ExprKind::BoolLiteral(value) => Some((i64::from(*value), i64::from(*value))),
        ExprKind::Variable(name) => match locals.get(name)? { LocalValue::Integer(range) => Some(*range), _ => None },
        ExprKind::Negate(value) => negate_range(local_integer(value, locals)?),
        ExprKind::BinaryOp { left, op, right } =>
            integer_binary(local_integer(left, locals)?, op, local_integer(right, locals)?),
        ExprKind::ArrayAccess { array, index }
            if local_buffer_expression(array, locals) == Some(LocalValue::Buffer)
                && local_integer(index, locals).is_some() => Some((i64::MIN, i64::MAX)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Undefined names, callbacks, overflow and loop-carried changes cannot hide warning handlers.
    #[test]
    fn local_buffer_factories_require_definite_local_bindings() {
        for source in [
            "<?php function f(int $n): buffer<int> { $page = buffer_new<int>($later); $later = $n; return $page; }",
            "<?php function f(int $n): buffer<int> { for ($i = 0; $i < $n; $i++) { $page = buffer_new<int>(1); } return $page; }",
            "<?php function f(int $n): buffer<int> { global $page; return $page; }",
            "<?php function f(int $n): buffer<int> { $page = buffer_new<int>($n); outside(); return $page; }",
            "<?php function f(int $n): buffer<int> { $page = buffer_new<int>(1); $page[0] = 1e309; return $page; }",
            "<?php function f(int $n): buffer<int> { $page = buffer_new<int>(1); $x = 1; for ($i = 0; $i < $n; $i++) { $x = $x * 2; $page[0] = $x; } return $page; }",
        ] {
            let tokens = crate::lexer::tokenize(source).unwrap();
            let program = crate::parser::parse(&tokens).unwrap();
            let StmtKind::FunctionDecl { body, .. } = &program[0].kind else { panic!("expected function"); };
            let mut locals = HashMap::from([("n".to_string(), LocalValue::Integer((1, 3)))]);
            assert!(local_buffer_block(body, &mut locals).is_none(), "{source}");
        }
    }
}
