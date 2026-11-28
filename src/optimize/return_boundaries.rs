//! Purpose:
//! Models runtime verification at declared scalar, callable and class-like return boundaries.
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
        let declared_object = hint.is_some_and(object_hint);
        let declared_result = match hint {
            Some(TypeExpr::Never) => PhpType::Never,
            _ => parameter_type(hint),
        };
        let declared_callable =
            hint.is_some_and(crate::types::checker::type_expr_declares_callable);
        if declared_scalar.is_none() && !declared_object {
            return Rc::new(Self {
                declared_scalar,
                declared_callable,
                declared_object,
                declared_result,
                stable_parameters: TypeEnv::new(),
                stable_object_parameters: Vec::new(),
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
        Rc::new(Self {
            declared_scalar, declared_callable, declared_object, declared_result,
            stable_parameters, stable_object_parameters,
        })
    }

    /// Returns whether a value of `source` type reaching this boundary is checked at run time
    /// under the declaring file's `strict` mode.
    fn checks_source(&self, source: &PhpType, strict: bool) -> bool {
        self.declared_callable
            || self.implicit_may_throw(source)
            || (self.declared_object && crate::types::object_boundary::runtime_checked_object_source(source))
            || match &self.declared_scalar {
                Some(PhpType::Int) => declared_int_return_requires_validation(true, &PhpType::Int, source),
                Some(declared) => crate::types::param_binding::runtime_scalar_guard_needed(declared, source, strict),
                None => false,
            }
    }

    /// Exposes only the representation promised by the source declaration.
    pub(super) fn declared_result(&self) -> PhpType { self.declared_result.clone() }

    /// Tests the same decision EIR lowering makes for this returned expression.
    ///
    /// An `int` return applies the representation predicate, with unknown values treated as
    /// boxed. A `string`/`float`/`bool` return is checked only where the checker recorded the
    /// returned expression (`Checker::record_scalar_guard_arg_site`), which lowering consults too.
    /// A declared `callable` return can throw on every return.
    fn may_throw(&self, expr: &Expr) -> bool {
        let source = self.source_type(expr);
        self.declared_callable
            || self.implicit_may_throw(&source)
            || (self.declared_object && !self.object_source(expr))
            || match &self.declared_scalar {
                Some(PhpType::Int) => declared_int_return_requires_validation(
                    true, &PhpType::Int, &source,
                ),
                Some(_) => super::binding_decisions::is_scalar_guard_site(expr.span),
                None => false,
            }
    }

    /// Retains an exact-kind failure independently of ordinary PHP return verification.
    fn implicit_may_throw(&self, source: &PhpType) -> bool {
        if super::semantic_context::enabled(elephc_semantics::StrictFeature::Nullability)
            && crate::semantics::nullability::non_null_contract(&self.declared_result) {
            // A useful static heap type can still carry a logical null from its producer.
            return true;
        }
        matches!(crate::semantics::conversions::implicit(
            super::semantic_context::active_profile(), super::semantic_context::active_source_mode(),
            &self.declared_result, source),
            elephc_semantics::conversions::ConversionPlan::Guard { .. }
                | elephc_semantics::conversions::ConversionPlan::Reject(_))
    }

    /// Recognizes returned values that are objects before the class-like boundary checks them.
    fn object_source(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::This | ExprKind::NewObject { .. } => true,
            ExprKind::Variable(name) => self.stable_object_parameters.iter().any(|stable| stable == name),
            _ => false,
        }
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
                CastType::String | CastType::StringFormat => PhpType::Str,
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
    hint?;
    match parameter_type(hint) {
        scalar @ (PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool) => Some(scalar),
        _ => None,
    }
}

/// Resolves only representations whose syntax is authoritative before checker specialization.
pub(super) fn parameter_type(hint: Option<&TypeExpr>) -> PhpType {
    hint.and_then(crate::semantics::conversions::implicit_hint).unwrap_or(PhpType::Mixed)
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

/// Returns whether the active boundary releases a refused returned temporary while its
/// `TypeError` unwinds (declared `string`, `float` and `bool`; the `int` check does not).
pub(crate) fn active_return_boundary_releases_refused_value() -> bool {
    ACTIVE_RETURN_BOUNDARY.with(|slot| slot.borrow().as_ref().is_some_and(|facts|
        matches!(facts.declared_scalar, Some(PhpType::Str | PhpType::Float | PhpType::Bool))
            || facts.implicit_may_throw(&PhpType::Mixed)))
}

/// Uses the checker's source-point type once inference has established the current read view,
/// under the declaring file's `strict` mode.
pub(crate) fn active_return_boundary_checks_source(source: &PhpType, strict: bool) -> bool {
    ACTIVE_RETURN_BOUNDARY.with(|slot| slot.borrow().as_ref().is_some_and(|facts|
        facts.checks_source(source, strict)))
}

/// Returns whether the active `string`/`float`/`bool` return boundary checks a returned value
/// of checked type `source` under the declaring file's `strict` mode (the checker records it).
pub(crate) fn active_scalar_return_needs_guard(source: &PhpType, strict: bool) -> bool {
    ACTIVE_RETURN_BOUNDARY.with(|slot| slot.borrow().as_ref().is_some_and(|facts|
        matches!(&facts.declared_scalar, Some(declared @ (PhpType::Str | PhpType::Float | PhpType::Bool))
            if crate::types::param_binding::runtime_scalar_guard_needed(declared, source, strict))))
}

/// Recognizes a parameter hint whose value the class-like return boundary never re-checks.
///
/// A nullable or union hint that names the bare `object` type (`?object`, `object|null`) is a
/// source the boundary checks at run time (`types::object_boundary`), so returning it may throw.
fn statically_object_parameter_hint(hint: &TypeExpr) -> bool {
    let bare_object = |hint: &TypeExpr| matches!(hint, TypeExpr::Named(name)
        if name.as_str().eq_ignore_ascii_case("object"));
    match hint {
        TypeExpr::Nullable(inner) => !bare_object(inner) && object_hint(inner),
        TypeExpr::Union(members) => !members.iter().any(bare_object) && object_hint(hint),
        _ => object_hint(hint),
    }
}

/// Recognizes a class, interface, `object` or nullable-class return hint.
fn object_hint(hint: &TypeExpr) -> bool {
    const BUILTIN: [&str; 14] = ["int", "float", "bool", "false", "true", "string", "array",
        "callable", "iterable", "mixed", "null", "void", "never", "resource"];
    match hint {
        TypeExpr::Named(name) => !BUILTIN.iter().any(|builtin| name.as_str().eq_ignore_ascii_case(builtin)),
        TypeExpr::GenericClass { .. } => true,
        TypeExpr::Nullable(inner) => object_hint(inner),
        TypeExpr::Union(members) => members.iter().any(object_hint) && members.iter().all(|member|
            object_hint(member) || matches!(member, TypeExpr::Void)
                || matches!(member, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null"))),
        _ => false,
    }
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
