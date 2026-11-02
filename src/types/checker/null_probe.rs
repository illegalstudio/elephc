//! Purpose:
//! Models PHP's "null probe" constructs — `isset()`, `empty()`, `unset()` and the left operand
//! of `??` / `??=` — which exist precisely to name storage that may never have been declared.
//! PHP answers all of them without an `Undefined variable` warning.
//!
//! Called from:
//! - `crate::types::checker::builtins::language_constructs` (`isset`/`empty`/`unset` operands)
//! - `crate::types::checker::inference::expr` and `::effects` (`??` left operand)
//! - `crate::types::checker::driver::top_level` (deferred validation of recorded roots)
//!
//! Key details:
//! - Only the *spine* of the access chain is covered. PHP still warns about an undefined index
//!   expression (`isset($a[$b])` warns for `$b`, not for `$a`), so index and property-name
//!   subexpressions keep the ordinary `Undefined variable` diagnostic.
//! - A tolerated root is typed `PhpType::Void` (elephc's `null`), which is exactly what PHP
//!   reports for a never-declared variable.
//! - Acceptance is **deferred, not immediate**, for top-level code. EIR lowering derives main's
//!   local types from `CheckResult::global_env`, so a probed name can only be lowered correctly
//!   when it stays `null` for the whole scope: `global_env` must end the pass without a binding
//!   for it, so the slot is typed `Void` and codegen answers from the type instead of reading
//!   uninitialized storage. A name that is *also* assigned at top level
//!   (`if (!isset($cfg)) { $cfg = 3; }`) would instead get that assigned type on a slot the probe
//!   reads before any store, so `check_top_level_program` re-raises the original diagnostic for
//!   it. See `Checker::pending_null_probe_roots`.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

/// Returns the never-declared root variable of a null-probe operand's access chain.
///
/// Walks the chain spine through `$x[...]`, `$x->p` and `$x?->p` down to the base
/// `ExprKind::Variable`, and reports its name only when that name is absent from `env`.
/// Returns `None` for any other operand shape or when the root is already bound.
pub(crate) fn undefined_probe_root_variable<'a>(arg: &'a Expr, env: &TypeEnv) -> Option<&'a str> {
    let mut current = arg;
    loop {
        match &current.kind {
            ExprKind::Variable(name) => {
                let name = name.as_str();
                return (!env.contains_key(name)).then_some(name);
            }
            ExprKind::ArrayAccess { array, .. } => current = array,
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. } => current = object,
            _ => return None,
        }
    }
}

/// Returns a probe environment for `arg`: a clone of `env` in which a never-declared chain root
/// is bound to `null`, or `None` when `env` already suffices.
///
/// Callers infer the operand against the returned environment so that a probe of a never-declared
/// variable answers `null` instead of raising `Undefined variable`. The clone is deliberately not
/// propagated back to the caller's scope: PHP's probes do not create the variable
/// (`if (isset($z)) {} echo $z;` still warns).
pub(crate) fn null_probe_env(checker: &mut Checker, arg: &Expr, env: &TypeEnv) -> Option<TypeEnv> {
    let name = undefined_probe_root_variable(arg, env)?.to_string();
    record_pending_root(checker, &name, arg.span);
    let mut probed = env.clone();
    probed.insert(name, PhpType::Void);
    Some(probed)
}

/// Installs a temporary `null` binding for `arg`'s never-declared chain root in `env`.
///
/// Returns the bound name, which must be handed to [`end_null_probe_root`] once the operand has
/// been checked. Use this variant on the assignment-effect path, where the caller needs the
/// operand's writes to land in the real environment and therefore cannot infer against a
/// throwaway clone.
pub(crate) fn begin_null_probe_root(
    checker: &mut Checker,
    arg: &Expr,
    env: &mut TypeEnv,
) -> Option<String> {
    let name = undefined_probe_root_variable(arg, env)?.to_string();
    record_pending_root(checker, &name, arg.span);
    env.insert(name.clone(), PhpType::Void);
    Some(name)
}

/// Removes a temporary probe binding installed by [`begin_null_probe_root`].
///
/// The binding is kept when the probed operand itself gave the name a non-`null` type
/// (`empty($x = 5)` is legal PHP), so a real definition is never discarded.
pub(crate) fn end_null_probe_root(name: Option<String>, env: &mut TypeEnv) {
    let Some(name) = name else { return };
    if env.get(&name) == Some(&PhpType::Void) {
        env.remove(&name);
    }
}

impl Checker {
    /// Infers a null-probe operand: the operand of `isset`/`empty`/`unset` or the left side of
    /// `??`.
    ///
    /// Raises [`Checker::null_probe_depth`] for the duration so index and property access on a
    /// `null` base yield `null` instead of a diagnostic, matching PHP — `isset($n['k'])` and
    /// `$n->p ?? $d` answer `false` / the default rather than faulting. Index and property-name
    /// subexpressions are inferred inside the same context but are unaffected: only a `Void`
    /// base changes behavior, so an undefined `$b` in `isset($a[$b])` still reports.
    pub(crate) fn infer_null_probe_operand(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.null_probe_depth += 1;
        let result = self.infer_type(expr, env);
        self.null_probe_depth -= 1;
        result
    }

    /// Infers a null-probe operand while propagating its assignment effects into `env`.
    ///
    /// Same contract as [`Checker::infer_null_probe_operand`], for the statement-effect walk.
    pub(crate) fn infer_null_probe_operand_with_effects(
        &mut self,
        expr: &Expr,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.null_probe_depth += 1;
        let result = self.infer_type_with_assignment_effects(expr, env);
        self.null_probe_depth -= 1;
        result
    }
}

/// Records a tolerated root for the end-of-pass check, but only for top-level scopes.
///
/// Function, method, and closure bodies do not contribute their locals to `global_env`, so there
/// is nothing to validate the tolerance against there and nothing to seed for lowering.
fn record_pending_root(checker: &mut Checker, name: &str, span: Span) {
    if !checker.null_probe_scope_is_top_level {
        return;
    }
    checker
        .pending_null_probe_roots
        .push((name.to_string(), span));
}

/// Builds the diagnostic re-raised when a deferred probe root turns out to be assigned elsewhere
/// in the same scope, so the tolerance is not backed by a representable `null` slot.
pub(crate) fn unrepresentable_probe_root_error(name: &str, span: Span) -> CompileError {
    CompileError::new(span, &format!("Undefined variable: ${}", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a point-span expression for the probe-shape unit tests.
    fn expr(kind: ExprKind) -> Expr {
        Expr::new(kind, Span::new(1, 1))
    }

    /// A bare undefined variable is reported as the probe root.
    #[test]
    fn bare_undefined_variable_is_a_probe_root() {
        let env = TypeEnv::default();
        let arg = expr(ExprKind::Variable("never".to_string()));
        assert_eq!(undefined_probe_root_variable(&arg, &env), Some("never"));
    }

    /// A variable that is already bound is not a probe root, so normal inference applies.
    #[test]
    fn defined_variable_is_not_a_probe_root() {
        let mut env = TypeEnv::default();
        env.insert("known".to_string(), PhpType::Int);
        let arg = expr(ExprKind::Variable("known".to_string()));
        assert_eq!(undefined_probe_root_variable(&arg, &env), None);
    }

    /// `$never['k']->p` resolves through the chain spine down to `$never`.
    #[test]
    fn chain_spine_resolves_to_the_base_variable() {
        let env = TypeEnv::default();
        let arg = expr(ExprKind::PropertyAccess {
            object: Box::new(expr(ExprKind::ArrayAccess {
                array: Box::new(expr(ExprKind::Variable("never".to_string()))),
                index: Box::new(expr(ExprKind::StringLiteral("k".to_string()))),
            })),
            property: "p".to_string(),
        });
        assert_eq!(undefined_probe_root_variable(&arg, &env), Some("never"));
    }

    /// Only the spine is reported: `isset($a[$b])` yields `$a`, leaving `$b` to ordinary
    /// inference so it keeps PHP's undefined-variable diagnostic.
    #[test]
    fn index_subexpression_is_not_part_of_the_spine() {
        let env = TypeEnv::default();
        let arg = expr(ExprKind::ArrayAccess {
            array: Box::new(expr(ExprKind::Variable("a".to_string()))),
            index: Box::new(expr(ExprKind::Variable("b".to_string()))),
        });
        assert_eq!(undefined_probe_root_variable(&arg, &env), Some("a"));
    }

    /// A non-lvalue operand shape yields no probe root.
    #[test]
    fn non_lvalue_operand_has_no_probe_root() {
        let env = TypeEnv::default();
        let arg = expr(ExprKind::IntLiteral(1));
        assert_eq!(undefined_probe_root_variable(&arg, &env), None);
    }

    /// The deferred diagnostic keeps the original wording, so a genuinely undefined variable is
    /// reported identically whether or not a probe deferred the decision.
    #[test]
    fn deferred_diagnostic_matches_the_undefined_variable_wording() {
        let error = unrepresentable_probe_root_error("cfg", Span::new(2, 5));
        assert_eq!(error.message, "Undefined variable: $cfg");
    }
}
