//! Purpose:
//! Validates function returns semantics for the checker.
//! Keeps call diagnostics and return-flow analysis consistent with signatures and inferred expression types.
//!
//! Called from:
//! - `crate::types::checker::functions`
//!
//! Key details:
//! - Diagnostics should map shared planner errors back to source spans without duplicating call semantics.

use crate::errors::CompileError;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

/// Holds the inferred type and whether a return statement provided a value.
/// Used by return-type checking to collect return type information across all paths.
#[derive(Clone)]
pub(crate) struct ReturnInfo {
    pub ty: PhpType,
    pub has_value: bool,
}

/// Makes an inferred return type nullable, the way a declared `?T` hint resolves.
///
/// `Checker::resolve_type_expr` expands `?T` to `Union([T, Void])`, and
/// `nullable_match_arm_type` builds the same shape for ternary/match joins, so this keeps a
/// hint-less nullable return byte-identical to both. `codegen_repr()` maps such a union to
/// `Mixed` (or `TaggedScalar` for `int|null`), which is why `return null` then boxes instead
/// of being coerced into the other arm's zero value.
///
/// `Mixed` already admits null and is left alone; an existing union gains `Void` as one more
/// member rather than nesting; `Void`/`Never` collapse to plain `Void` so a function whose
/// every path yields null does not become `null|null`.
fn nullable_return_type(other: &PhpType) -> PhpType {
    match other {
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Void | PhpType::Never => PhpType::Void,
        PhpType::Union(members) => {
            if members.iter().any(|member| matches!(member, PhpType::Void)) {
                PhpType::Union(members.clone())
            } else {
                let mut members = members.clone();
                members.push(PhpType::Void);
                PhpType::Union(members)
            }
        }
        other => PhpType::Union(vec![other.clone(), PhpType::Void]),
    }
}

impl Checker {
    /// Recursively collects ReturnInfo from all return statements in `stmt` and its
    /// nested blocks (if/while/try/etc.), appending each to `returns`. Untyped or unresolvable
    /// expressions are skipped silently — only well-typed returns contribute to the vector.
    pub(crate) fn collect_return_infos(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<ReturnInfo>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                // Prefer the type recorded while this exact statement was checked: it reflects
                // the environment at the return site, whereas `env` here is the body's final
                // environment and would leak a later narrowing backwards. Falls back to
                // re-inference when nothing was recorded (e.g. an unchecked body).
                let recorded = self
                    .flow_typed_returns
                    .get(&(stmt as *const Stmt as usize))
                    .filter(|(span, _)| *span == stmt.span)
                    .map(|(_, ty)| ty.clone());
                if let Some(ty) = recorded.or_else(|| self.infer_type(expr, env).ok()) {
                    returns.push(ReturnInfo {
                        ty,
                        has_value: true,
                    });
                }
            }
            StmtKind::Return(None) => {
                returns.push(ReturnInfo {
                    ty: PhpType::Void,
                    has_value: false,
                });
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_infos(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_infos(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_infos(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_infos(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    /// Collects return callable sigs for the surrounding analysis or metadata result.
    pub(crate) fn collect_return_callable_sigs(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<FunctionSig>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(Some(sig)) = self.resolve_expr_callable_sig(expr, env) {
                    returns.push(sig);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_callable_sigs(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_callable_sigs(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    /// Collects callable element signatures from array-returning statements.
    ///
    /// This records homogeneous `array<callable>` return metadata separately from
    /// direct callable returns so callers can propagate element signatures without
    /// treating the function call expression itself as a callable.
    pub(crate) fn collect_return_callable_array_sigs(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        returns: &mut Vec<FunctionSig>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                if let Ok(Some(sig)) = self.resolve_expr_callable_array_sig(expr, env) {
                    returns.push(sig);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for s in body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.collect_return_callable_array_sigs(s, env, returns);
                }
                for catch_clause in catches {
                    for s in &catch_clause.body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.collect_return_callable_array_sigs(s, env, returns);
                    }
                }
            }
            _ => {}
        }
    }

    /// Returns true if `body` contains at least one Return statement at any nesting depth,
    /// including inside conditionals, loops, try/catch, switch, or synthetic blocks.
    pub(crate) fn body_contains_return(body: &[Stmt]) -> bool {
        body.iter().any(Self::stmt_contains_return)
    }

    /// Checks that a function or closure body ends with a return on every control-flow path
    /// when the declared return type is not Void or Never. Uses the shared function-exit analysis,
    /// extended with checker-known `never` calls, to determine if the body always exits; emits a
    /// "must return a value" error if not.
    pub(crate) fn require_declared_return_coverage(
        &self,
        declared_ret: &PhpType,
        body: &[Stmt],
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if matches!(declared_ret, PhpType::Void | PhpType::Never) {
            return Ok(());
        }

        if crate::termination::block_guarantees_function_exit_with_divergence(body, &|expr| {
            self.expr_is_declared_never_call(expr)
        }) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} must return a value on every path", context),
            ))
        }
    }

    /// Checks that an actual return type is compatible with the declared return type.
    /// Handles three cases: void-returning functions (no value allowed), value-returning
    /// functions (value required and must be assignable to `expected`), and nullability
    /// via `return_type_accepts_null`. Delegates to `require_compatible_arg_type` for
    /// the final assignability check.
    pub(crate) fn require_compatible_return_type(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        has_value: bool,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if !has_value {
            if matches!(expected, PhpType::Void) {
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!("{} must return a value of type {:?}", context, expected),
            ));
        }

        if matches!(expected, PhpType::Void) {
            return Err(CompileError::new(
                span,
                &format!("{} must not return a value", context),
            ));
        }

        // PHP validates declared object returns when the function actually returns. Keep
        // statically precise values fast, but allow every source shape to reach the runtime
        // class/interface boundary where null, scalars, and incompatible objects raise the
        // catchable `TypeError` PHP emits.
        if matches!(expected, PhpType::Object(_)) {
            return Ok(());
        }

        if matches!(actual, PhpType::Void) && !Self::return_type_accepts_null(expected) {
            return Err(CompileError::new(
                span,
                &format!("{} expects {:?}, got Void", context, expected),
            ));
        }

        self.require_compatible_arg_type(expected, actual, span, context)
    }

    /// Returns true if `ty` can accept a null/void value — covers PhpType::Mixed,
    /// PhpType::Void, and PhpType::Union types where any member accepts null.
    fn return_type_accepts_null(ty: &PhpType) -> bool {
        match ty {
            PhpType::Mixed => true,
            PhpType::Union(members) => members.iter().any(Self::return_type_accepts_null),
            PhpType::Void => true,
            _ => false,
        }
    }

    /// Returns true if `stmt` or any nested statement within it contains a Return.
    /// Recurses through If, While, DoWhile, For, Foreach, Try, Switch, Synthetic,
    /// NamespaceBlock, and IfDef. Used by `body_contains_return` for control-flow analysis.
    fn stmt_contains_return(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Return(_) => true,
            StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
                Self::body_contains_return(stmts)
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                Self::body_contains_return(then_body)
                    || elseif_clauses
                        .iter()
                        .any(|(_, body)| Self::body_contains_return(body))
                    || else_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Foreach { body, .. } => Self::body_contains_return(body),
            StmtKind::For {
                init, update, body, ..
            } => {
                init.as_ref()
                    .is_some_and(|stmt| Self::stmt_contains_return(stmt))
                    || update
                        .as_ref()
                        .is_some_and(|stmt| Self::stmt_contains_return(stmt))
                    || Self::body_contains_return(body)
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                Self::body_contains_return(try_body)
                    || catches
                        .iter()
                        .any(|catch_clause| Self::body_contains_return(&catch_clause.body))
                    || finally_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|(_, body)| Self::body_contains_return(body))
                    || default
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                Self::body_contains_return(then_body)
                    || else_body
                        .as_ref()
                        .is_some_and(|body| Self::body_contains_return(body))
            }
            _ => false,
        }
    }

    /// Computes the wider of two PHP types for return-type merging:
    /// - If equal, returns a clone.
    /// - `Never` is absorbed by the other type (it materializes no value).
    /// - `Void` (elephc's spelling of PHP `null`) makes the other type NULLABLE.
    /// - Str + anything → Str; Float + anything → Float; otherwise → Mixed.
    ///
    /// The `Void` arm must be tested BEFORE `Str`/`Float`, and `Never` before `Void`.
    /// `Void` used to resolve to the other type outright, which silently deleted the null
    /// arm of a hint-less union return: `function f($x) { if ($x) { return "s"; } return null; }`
    /// inferred `Str`, so `return null` was lowered as `i_to_str(const_null)` and the caller
    /// saw `""` instead of `NULL`. Writing `: ?string` was already correct, and the ternary
    /// spelling of the same function already inferred `string|null` through
    /// `nullable_match_arm_type` — this makes the multi-`return` fold agree with both.
    pub(crate) fn wider_type(a: &PhpType, b: &PhpType) -> PhpType {
        match (a, b) {
            _ if a == b => a.clone(),
            (PhpType::Never, other) | (other, PhpType::Never) => other.clone(),
            (PhpType::Void, other) | (other, PhpType::Void) => nullable_return_type(other),
            (PhpType::Str, _) | (_, PhpType::Str) => PhpType::Str,
            (PhpType::Float, _) | (_, PhpType::Float) => PhpType::Float,
            _ => PhpType::Mixed,
        }
    }

    /// Unions two inferred parameter types for call-site specialization: identical
    /// types stay, `Void`/`Never` are absorbed by the other type, and any genuine
    /// disagreement widens to `Mixed`. Unlike `wider_type` (which lets `Str`/`Float`
    /// absorb other scalars for coercion), this preserves the distinction between
    /// scalar tags, so a parameter called with both an int and a string is `Mixed`
    /// (boxed) rather than collapsed to one type and mis-tagged at runtime.
    pub(crate) fn union_param_type(a: &PhpType, b: &PhpType) -> PhpType {
        match (a, b) {
            _ if a == b => a.clone(),
            // Under the tagged null representation a null call-site argument makes the
            // parameter genuinely nullable: widen scalar params to int|null instead of
            // riding null in as the in-band sentinel of a plain Int.
            (PhpType::Void, PhpType::Int) | (PhpType::Int, PhpType::Void)
                if crate::codegen::sentinels::null_repr_is_tagged() =>
            {
                PhpType::Union(vec![PhpType::Int, PhpType::Void])
            }
            (PhpType::Void | PhpType::Never, other) | (other, PhpType::Void | PhpType::Never) => {
                other.clone()
            }
            // An object parameter called with various concrete subtypes keeps its
            // object type (e.g. a `Throwable` param invoked with a concrete
            // exception) instead of widening to `Mixed`, which would break
            // object-typed dispatch (`Fiber::throw` / `Generator::throw`).
            (PhpType::Object(_), PhpType::Object(_)) => a.clone(),
            (PhpType::Array(left), PhpType::Array(right)) => {
                PhpType::Array(Box::new(Self::union_array_payload_type(left, right)))
            }
            (
                PhpType::AssocArray {
                    key: left_key,
                    value: left_value,
                },
                PhpType::AssocArray {
                    key: right_key,
                    value: right_value,
                },
            ) => PhpType::AssocArray {
                key: Box::new(Self::union_array_payload_type(left_key, right_key)),
                value: Box::new(Self::union_array_payload_type(left_value, right_value)),
            },
            _ => PhpType::Mixed,
        }
    }

    /// Joins one indexed/associative array payload position without discarding container shape.
    fn union_array_payload_type(left: &PhpType, right: &PhpType) -> PhpType {
        if left == right || left.codegen_repr() == right.codegen_repr() {
            return left.clone();
        }
        match (left, right) {
            (PhpType::Never, other) | (other, PhpType::Never) => other.clone(),
            _ => PhpType::Mixed,
        }
    }
}
