//! Purpose:
//! Answers the type-parameter bound obligations that generic class instantiation left behind.
//!
//! Called from:
//! - `crate::types::checker::check_types_with_options`, once, after the class table is built.
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
        for obligation in obligations {
            let bound = self.resolve_type_expr(&obligation.bound, obligation.span)?;
            let argument = self.resolve_type_expr(&obligation.argument, obligation.span)?;
            // `type_accepts` is the assignability predicate the rest of the checker uses, so a
            // bound admits exactly what a parameter of that type would: subclasses, implemented
            // interfaces, and interface inheritance. Comparing spellings instead would let an
            // unrelated class with the same name through and reject a legitimate subclass.
            if !self.type_accepts(&bound, &argument) {
                return Err(CompileError::new(
                    obligation.span,
                    &format!(
                        "Generic class '{}' binds type parameter <{}> to {}, which does not \
                         satisfy its bound {}",
                        obligation.template, obligation.parameter, argument, bound
                    ),
                ));
            }
        }
        Ok(())
    }
}
