//! Purpose:
//! Models the runtime verification at declared `int`, `float`, `string`, `bool` and
//! `callable` return boundaries.
//!
//! Called from:
//! - Callable effect and exception summaries, DCE body scopes, and checker body scopes.
//!
//! Key details:
//! - Unknown source representations stay conservative; parameter facts survive only without writes.
//! - Lexical scopes isolate nested functions, methods, and closures.
//! - A class-like return checks a `mixed` or `object|null` source (`types::object_boundary`);
//!   only an object-hinted parameter that the body never writes, and whose hint does not
//!   combine the bare `object` type with `null`, is known to pass it.

use super::*;
use crate::parser::ast::CastType;
use crate::types::{declared_int_return_requires_validation, PhpType, TypeEnv};

/// Immutable source facts for one function-like return contract.
#[derive(Clone, Debug)]
pub(crate) struct ReturnBoundaryFacts {
    /// The declared non-nullable scalar return type whose boundary checks a boxed value.
    declared_scalar: Option<PhpType>,
    /// A declared `callable` return checks the returned value with `is_callable` and raises
    /// `TypeError`; every return is treated as a possible throw, which is conservative.
    declared_callable: bool,
    declared_object: bool,
    declared_result: PhpType,
    stable_parameters: TypeEnv,
    stable_object_parameters: Vec<String>,
}

thread_local! {
    static ACTIVE_RETURN_BOUNDARY: RefCell<Option<Rc<ReturnBoundaryFacts>>> = const { RefCell::new(None) };
}

impl ReturnBoundaryFacts {
    /// Captures declared parameter representations without trusting bindings that the body writes.
    pub(crate) fn new<'a>(
        hint: Option<&TypeExpr>,
        parameters: impl IntoIterator<Item = (&'a str, Option<&'a TypeExpr>)>,
        body: &[Stmt],
    ) -> Rc<Self> {
        let declared_scalar = declared_scalar_return(hint);
        let declared_result = match hint {
            Some(TypeExpr::Never) => PhpType::Never,
            _ => parameter_type(hint),
        };
        let declared_callable =
            hint.is_some_and(crate::types::checker::type_expr_declares_callable);
        if declared_scalar.is_none() {
            return Rc::new(Self {
                declared_scalar,
                declared_callable,
                declared_result,
                stable_parameters: TypeEnv::new(),
            });
        }
        let invalidated = body.iter().fold(Invalidation::none(),
            |writes, stmt| writes.union(stmt_invalidation(stmt)));
        let stable: Vec<_> = parameters.into_iter().filter(|(name, _)|
            matches!(&invalidated, Invalidation::Names(names) if !names.contains(*name))).collect();
        let stable_object_parameters = stable.iter()
            .filter(|(_, hint)| hint.is_some_and(statically_object_parameter_hint))
            .map(|(name, _)| name.to_string()).collect();
        let stable_parameters = stable.into_iter()
            .map(|(name, hint)| (name.to_string(), parameter_type(hint))).collect();
        Rc::new(Self { declared_scalar, declared_callable, declared_result, stable_parameters })
    }

    /// Returns whether a value of `source` type reaching this boundary is checked at run time.
    fn checks_source(&self, source: &PhpType) -> bool {
        self.declared_callable || match &self.declared_scalar {
            Some(PhpType::Int) => declared_int_return_requires_validation(true, &PhpType::Int, source),
            // `string`, `float` and `bool` returns check a value that can hold a refused type;
            // the strict rule is the wider one, so it is the conservative answer here.
            Some(declared) => crate::types::param_binding::runtime_scalar_guard_needed(declared, source, true),
            None => false,
        }
    }

    /// Exposes only the representation promised by the source declaration.
    pub(super) fn declared_result(&self) -> PhpType { self.declared_result.clone() }

    /// Tests the same representation predicate used by EIR, with unknown values treated as boxed.
    fn may_throw(&self, expr: &Expr) -> bool {
        self.checks_source(&self.source_type(expr))
    }

    /// Recognizes native values without assuming a mutable local keeps its original hint.
    fn source_type(&self, expr: &Expr) -> PhpType {
        match &expr.kind {
            ExprKind::IntLiteral(_) => PhpType::Int,
            ExprKind::FloatLiteral(_) => PhpType::Float,
            ExprKind::BoolLiteral(_) => PhpType::Bool,
            ExprKind::StringLiteral(_) => PhpType::Str,
            ExprKind::Null => PhpType::Void,
            ExprKind::Variable(name) => self.stable_parameters.get(name).cloned().unwrap_or(PhpType::Mixed),
            ExprKind::ErrorSuppress(value) | ExprKind::NamedArg { value, .. } => self.source_type(value),
            ExprKind::Cast { target, .. } => match target {
                CastType::Int => PhpType::Int,
                CastType::Float => PhpType::Float,
                CastType::Bool => PhpType::Bool,
                CastType::String => PhpType::Str,
                _ => PhpType::Mixed,
            },
            ExprKind::FunctionCall { name, .. } => with_active_function_effects(|functions|
                functions.and_then(|functions| functions.result_type(name.as_str())))
                .or_else(|| crate::builtins::registry::lookup(name.as_str())
                    .filter(|def| matches!(def.spec.semantics.result_type,
                        crate::builtins::semantics::BuiltinResultType::Declared))
                    .map(|def| def.return_type.clone())).unwrap_or(PhpType::Mixed),
            ExprKind::MethodCall { object, method, .. } =>
                effects::instance_method_result_type(object, method).unwrap_or(PhpType::Mixed),
            ExprKind::StaticMethodCall { receiver, method, .. } =>
                effects::static_method_result_type(receiver, method).unwrap_or(PhpType::Mixed),
            ExprKind::BinaryOp { op: BinOp::Concat, .. } => PhpType::Str,
            ExprKind::BinaryOp { op, .. } if matches!(op,
                BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq
                | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq
                | BinOp::And | BinOp::Or | BinOp::Xor) => PhpType::Bool,
            ExprKind::Assignment { value, result_target: None, .. } => self.source_type(value),
            ExprKind::Ternary { then_expr, else_expr, .. } => {
                let left = self.source_type(then_expr);
                if left == self.source_type(else_expr) { left } else { PhpType::Mixed }
            }
            _ => PhpType::Mixed,
        }
    }
}

/// Returns the declared non-nullable scalar return type whose boundary verifies its value.
fn declared_scalar_return(hint: Option<&TypeExpr>) -> Option<PhpType> {
    match hint? {
        TypeExpr::Int => Some(PhpType::Int),
        TypeExpr::Float => Some(PhpType::Float),
        TypeExpr::Str => Some(PhpType::Str),
        TypeExpr::Bool => Some(PhpType::Bool),
        TypeExpr::Named(name) => match name.as_str().to_ascii_lowercase().as_str() {
            "int" => Some(PhpType::Int),
            "float" => Some(PhpType::Float),
            "string" => Some(PhpType::Str),
            "bool" => Some(PhpType::Bool),
            _ => None,
        },
        _ => None,
    }
}

/// Resolves only representations whose syntax is authoritative before checker specialization.
fn parameter_type(hint: Option<&TypeExpr>) -> PhpType {
    match hint {
        Some(TypeExpr::Int) => PhpType::Int,
        Some(TypeExpr::Float) => PhpType::Float,
        Some(TypeExpr::Bool) => PhpType::Bool,
        Some(TypeExpr::False) => PhpType::False,
        Some(TypeExpr::Str) => PhpType::Str,
        Some(TypeExpr::Nullable(inner)) => PhpType::Union(vec![parameter_type(Some(inner)), PhpType::Void]),
        Some(TypeExpr::Union(members)) => PhpType::Union(members.iter().map(|hint| parameter_type(Some(hint))).collect()),
        _ => PhpType::Mixed,
    }
}

/// Installs one lexical return contract and restores the caller's context afterwards.
pub(crate) fn with_return_boundary_facts<R>(facts: Option<&Rc<ReturnBoundaryFacts>>, f: impl FnOnce() -> R) -> R {
    ACTIVE_RETURN_BOUNDARY.with(|slot| {
        let previous = slot.replace(facts.cloned());
        let result = f();
        slot.replace(previous);
        result
    })
}

/// Reports possible verification errors while an optimizer walks a return statement.
pub(super) fn active_return_boundary_may_throw(expr: &Expr) -> bool {
    ACTIVE_RETURN_BOUNDARY.with(|slot| slot.borrow().as_ref().is_some_and(|facts| facts.may_throw(expr)))
}

/// Uses the checker's source-point type once inference has established the current read view.
pub(crate) fn active_return_boundary_checks_source(source: &PhpType) -> bool {
    ACTIVE_RETURN_BOUNDARY.with(|slot| slot.borrow().as_ref().is_some_and(|facts|
        facts.checks_source(source)))
}

/// Returns whether the active `string`/`float`/`bool` return boundary checks a returned value
/// of checked type `source` under the declaring file's `strict` mode (the checker records it).
pub(crate) fn active_scalar_return_needs_guard(source: &PhpType, strict: bool) -> bool {
    ACTIVE_RETURN_BOUNDARY.with(|slot| slot.borrow().as_ref().is_some_and(|facts|
        matches!(&facts.declared_scalar, Some(declared @ (PhpType::Str | PhpType::Float | PhpType::Bool))
            if crate::types::param_binding::runtime_scalar_guard_needed(declared, source, strict))))
}

/// Collects contracts using the same free-function and method keys as the summary tables.
pub(super) fn collect_return_boundaries(stmts: &[Stmt]) -> HashMap<String, Rc<ReturnBoundaryFacts>> {
    let mut out = HashMap::new();
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, params, return_type, body, .. } => {
                out.insert(name.clone(), ReturnBoundaryFacts::new(return_type.as_ref(),
                    params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), body));
            }
            StmtKind::ClassDecl { name, methods, .. } => {
                for method in methods.iter().filter(|method| method.has_body) {
                    out.insert(method_effect_key(name, &method.name), ReturnBoundaryFacts::new(method.return_type.as_ref(),
                        method.params.iter().map(|(name, hint, ..)| (name.as_str(), hint.as_ref())), &method.body));
                }
            }
            StmtKind::NamespaceBlock { body, .. } => out.extend(collect_return_boundaries(body)),
            _ => {}
        }
    }
    out
}
