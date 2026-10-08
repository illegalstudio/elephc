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
            let argument = self.resolve_type_expr(&obligation.argument, obligation.span)?;
            // `type_accepts` is the assignability predicate the rest of the checker uses, so a
            // bound admits exactly what a parameter of that type would: subclasses, implemented
            // interfaces, and interface inheritance. Comparing spellings instead would let an
            // unrelated class with the same name through and reject a legitimate subclass.
            if !self.type_accepts(&bound, &argument) {
                if defer_implicit_stringable && self.declares_public_tostring(&argument) {
                    continue;
                }
                return Err(CompileError::new(
                    obligation.span,
                    &format!(
                        "Generic {} '{}' binds type parameter <{}> to {}, which does not \
                         satisfy its bound {}",
                        obligation.kind, obligation.template, obligation.parameter, argument, bound
                    ),
                ));
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
