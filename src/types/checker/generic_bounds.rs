//! Purpose:
//! Answers the type-parameter bound obligations that generic class instantiation left behind.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl`, once the class table is built and
//!   BEFORE any body is checked, deferring only what a body can still change.
//! - `crate::types::checker::check_types_with_options`, once more after every body, for the
//!   obligations the first pass deferred.
//!
//! Key details:
//! - Instantiating `Box<User>` is pure syntax and happens long before this checker runs, but
//!   deciding that `User` satisfies `T : Entity` is a SUBTYPING question, and the class graph
//!   that answers it exists only here. The two halves are split for that reason alone.
//! - The same split, and the same `type_accepts` call, decide a generic FUNCTION's bounds in
//!   `functions::resolution::instantiate`. Two bound checks that disagreed would accept a class
//!   at a type argument the equivalent function rejects.

use crate::errors::CompileError;
use crate::generics::classes::BoundObligation;

use super::Checker;

impl Checker {
    /// Resolves a generic argument's PHP carrier when checking a value-type bound.
    pub(crate) fn resolve_generic_bound_argument(
        &self, argument: &crate::parser::ast::TypeExpr, span: crate::span::Span,
    ) -> Result<crate::types::PhpType, CompileError> {
        self.resolve_type_expr(&crate::generics::value_carrier_type(argument), span)
    }

    /// Reports a violated type-parameter bound: an error for native syntax, a warning for PHPDoc.
    ///
    /// php-src never reads `@template T of Foo`, so a call that breaks the annotation still runs
    /// there, and rejecting it would refuse a valid PHP program. The instantiation is compiled at
    /// the type the code supplies, exactly as php-src executes it. A native bound (`<T : Foo>`)
    /// is compiler syntax its author opted into, and stays an error.
    pub(crate) fn report_bound_violation(
        &mut self,
        from_docblock: bool,
        span: crate::span::Span,
        message: String,
    ) -> Result<(), CompileError> {
        if !from_docblock {
            return Err(CompileError::new(span, &message));
        }
        let message = format!(
            "{}; the bound is a PHPDoc annotation php-src does not enforce, so the code is \
             compiled as written",
            message
        );
        // A body is re-checked once per call-site specialization, which reaches the same site again.
        if !self.warnings.iter().any(|seen| seen.span == span && seen.message == message) {
            self.warnings.push(crate::errors::CompileWarning::new(span, &message));
        }
        Ok(())
    }

    /// Returns whether a bound is left unchecked: a PHPDoc parameter bound to `mixed`, or to the
    /// native hint it took over.
    ///
    /// Those are what a PHPDoc parameter is read as when its annotation cannot be honoured, and
    /// `mixed` is also what an untyped argument infers. Neither says the annotation was broken
    /// beyond what `settle_native_hints` already reports, so neither is worth a diagnostic.
    pub(crate) fn skips_docblock_bound(
        param: &crate::parser::ast::TypeParam,
        argument: &crate::parser::ast::TypeExpr,
    ) -> bool {
        param.from_docblock
            && (crate::generics::is_mixed_type(argument)
                || param.native_hint.as_ref().is_some_and(|hint| &hint.ty == argument))
    }

    /// Keeps the native hint a PHPDoc parameter took over binding on its type argument.
    ///
    /// `@param T $m` on a native `Model $m` replaces a hint php-src enforces at every call, and
    /// `@return T` on a native `: Model` one it enforces at every return. A type argument the
    /// hint accepts refines it, which is the annotation's whole purpose. One it does not accept
    /// (or `mixed`, which says nothing) is replaced by the hint itself: the instantiation then
    /// has the signature php-src runs, and the ordinary argument and return checks report the
    /// program exactly as they would without the annotation.
    pub(crate) fn settle_native_hints(
        &self,
        type_params: &[crate::parser::ast::TypeParam],
        bindings: &mut crate::generics::Bindings,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        for param in type_params {
            let Some(hint) = param.native_hint.as_ref().map(|hint| &hint.ty) else {
                continue;
            };
            let Some((_, argument)) = bindings.iter_mut().find(|(name, _)| name == &param.name)
            else {
                continue;
            };
            if argument == hint {
                continue;
            }
            let refines = !crate::generics::is_mixed_type(argument) && {
                let hint_ty = self.resolve_type_expr(hint, span)?;
                let argument_ty = self.resolve_generic_bound_argument(argument, span)?;
                self.type_accepts(&hint_ty, &argument_ty)
            };
            if !refines {
                *argument = hint.clone();
            }
        }
        Ok(())
    }

    /// Rejects any type argument that does not satisfy its parameter's declared bound.
    ///
    /// Reports the FIRST violation rather than collecting them: an instantiation that violates
    /// its bound has already been spliced into the program, so everything after this point is
    /// checking a class the program should not contain.
    pub(crate) fn verify_class_type_argument_bounds(
        &mut self,
        obligations: &[BoundObligation],
    ) -> Result<(), CompileError> {
        self.verify_class_type_argument_bounds_in(obligations, false)
    }

    /// The same check, run before any body so a violated bound is THE error rather than whatever
    /// the instantiated body trips over first — `Undefined method: Plain::label` says nothing
    /// about the `Shows<Plain>` that caused it.
    ///
    /// One fact is not settled yet at that point: a class with a public `__toString` implements
    /// `Stringable` implicitly, and that is decided from the method's return type, which a body
    /// check may still infer. An obligation that fails for such an argument is left to the pass
    /// after the bodies, which sees the final interface list.
    pub(crate) fn verify_class_type_argument_bounds_before_bodies(
        &mut self,
        obligations: &[BoundObligation],
    ) -> Result<(), CompileError> {
        self.verify_class_type_argument_bounds_in(obligations, true)
    }

    /// Validates bound obligations, optionally deferring classes whose implicit Stringable status needs body inference.
    fn verify_class_type_argument_bounds_in(
        &mut self,
        obligations: &[BoundObligation],
        defer_implicit_stringable: bool,
    ) -> Result<(), CompileError> {
        for obligation in obligations {
            let bound = self.resolve_type_expr(&obligation.bound, obligation.span)?;
            let argument = self.resolve_generic_bound_argument(&obligation.argument, obligation.span)?;
            // `type_accepts` is the assignability predicate the rest of the checker uses, so a
            // bound admits exactly what a parameter of that type would: subclasses, implemented
            // interfaces, and interface inheritance. Comparing spellings instead would let an
            // unrelated class with the same name through and reject a legitimate subclass.
            if obligation.from_docblock && crate::generics::is_mixed_type(&obligation.argument) {
                continue;
            }
            if !self.type_accepts(&bound, &argument) {
                if defer_implicit_stringable && self.declares_public_tostring(&argument) {
                    continue;
                }
                let reason = if obligation.native_hint {
                    "is not accepted by the native type"
                } else {
                    "does not satisfy its bound"
                };
                self.report_bound_violation(
                    obligation.from_docblock,
                    obligation.span,
                    format!(
                        "Generic {} '{}' binds type parameter <{}> to {}, which {} {}",
                        obligation.kind,
                        obligation.template,
                        obligation.parameter,
                        argument,
                        reason,
                        bound
                    ),
                )?;
            }
        }
        Ok(())
    }

    /// Whether `ty` is a class declaring a public `__toString`, the one thing that can still add
    /// an interface to it once the class table is built.
    fn declares_public_tostring(&self, ty: &crate::types::PhpType) -> bool {
        let crate::types::PhpType::Object(name) = ty else {
            return false;
        };
        let key = crate::names::php_symbol_key("__toString");
        self.classes.get(name).is_some_and(|info| {
            info.method_visibilities.get(&key)
                == Some(&crate::parser::ast::Visibility::Public)
        })
    }
}
