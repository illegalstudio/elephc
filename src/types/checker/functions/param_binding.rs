//! Purpose:
//! Applies PHP's parameter-binding rules when a declared user-defined parameter is handed an
//! argument whose inferred type does not already satisfy it: coercive scalar binding
//! (`string $s` accepting `42`) and callable-name strings (`callable $f` accepting
//! `"strtoupper"`).
//!
//! Called from:
//! - `crate::types::checker::functions::resolution` (user function and method calls)
//!
//! Key details:
//! - The accept/reject decision lives in `crate::types::param_binding`, shared with the
//!   matching EIR argument rewrite. This file only turns that decision into a diagnostic and
//!   registers the callable signature a bound callable string implies.
//! - Coercive binding runs *after* `types_compatible` / `type_accepts` have already failed, so
//!   it can only widen what is accepted, never narrow it.
//! - The `declare(strict_types=1)` rejection runs *before* them instead, because the widenings
//!   PHP drops in strict mode (`bool`→`int`, `int`→`bool`, …) are ones `types_compatible`
//!   already accepts on its own.

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::span::Span;
use crate::types::param_binding::{
    classify_param_binding, strict_param_binding_rejection, ParamBinding,
};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Validates one declared parameter against an argument, allowing PHP's coercive
    /// parameter binding and callable-name strings before reporting a type mismatch.
    ///
    /// `owner` names the `(function, parameter)` pair used to register the signature of a
    /// callable-name string, so the callee can type-check invocations of that parameter the
    /// same way it does for a first-class callable. Pass `None` where that registration does
    /// not apply (variadic elements, spread-expanded positions).
    ///
    /// `by_ref` must be true for a pass-by-reference parameter. PHP coerces those in place and
    /// writes the converted value back to the caller's variable; elephc's binding produces a
    /// temporary instead, so a by-reference parameter stays on the strict path rather than
    /// silently dropping the callee's writes.
    ///
    /// When the call site's file declared `strict_types=1`, the strict rejection runs first and
    /// no coercive binding is considered at all.
    ///
    /// # Errors
    /// Returns the standard `<context> expects <expected>, got <actual>` mismatch, extended
    /// with the PHP behaviour elephc cannot reproduce when a binding rule exists but is not
    /// statically decidable.
    pub(crate) fn require_bound_param_arg_type(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        context: &str,
        owner: Option<(&str, &str)>,
        by_ref: bool,
    ) -> Result<(), CompileError> {
        self.require_strict_types_param_binding(expected, actual, arg.span, context)?;
        if Self::types_compatible(expected, actual) || self.type_accepts(expected, actual) {
            return Ok(());
        }
        if by_ref {
            return self.require_compatible_arg_type(expected, actual, arg.span, context);
        }
        // Direct user-function calls emit the matching EIR guard after argument
        // evaluation. Other call surfaces must not acquire unchecked acceptance.
        if owner.is_some()
            && crate::types::param_binding::requires_object_argument_guard(expected, actual)
        {
            return Ok(());
        }
        match classify_param_binding(expected, actual, arg) {
            ParamBinding::Identity | ParamBinding::Cast(_) | ParamBinding::Const(_) => Ok(()),
            ParamBinding::Callable(target) => {
                let sig = self
                    .resolve_first_class_callable_sig(&target, arg.span, env)
                    .map_err(|err| {
                        Self::param_binding_error(
                            expected,
                            actual,
                            arg,
                            context,
                            err.message.as_str(),
                        )
                    })?;
                self.register_bound_callable_param_sig(owner, sig);
                Ok(())
            }
            ParamBinding::Deprecated(detail)
            | ParamBinding::TypeError(detail)
            | ParamBinding::NeedsRuntimeCheck(detail) => Err(Self::param_binding_error(
                expected, actual, arg, context, &detail,
            )),
            ParamBinding::Rejected => {
                self.require_compatible_arg_type(expected, actual, arg.span, context)
            }
        }
    }

    /// Rejects a declared-parameter argument that `declare(strict_types=1)` forbids at this
    /// call site.
    ///
    /// No-op unless the statement being checked came from a file that declared the directive,
    /// and no-op for every conversion PHP still performs in strict mode (the `int`→`float`
    /// widening and every non-scalar declared type). It is called from the coercive binding
    /// path *and* from the call surfaces that never had coercive binding — declared variadic
    /// element types and the sig-based closure/first-class-callable path — so the directive
    /// reaches every declared parameter, not only the coercively bound ones.
    ///
    /// # Errors
    /// Returns the standard `<context> expects <expected>, got <actual>` mismatch extended with
    /// the `TypeError` PHP would throw at run time.
    pub(crate) fn require_strict_types_param_binding(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        span: Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if !self.strict_types {
            return Ok(());
        }
        match strict_param_binding_rejection(expected, actual) {
            Some(detail) => Err(Self::binding_mismatch_error(
                expected, actual, span, context, &detail,
            )),
            None => Ok(()),
        }
    }

    /// Runs `operation` with `declare(strict_types=1)` parameter binding suspended.
    ///
    /// PHP invokes a callback handed to an internal function (`array_map`, `usort`,
    /// `array_walk`, `preg_replace_callback`, …) from the engine's own frame, which is never
    /// strict, so the calling file's directive does not reach the callback's parameters —
    /// `array_map('g', [true])` still coerces `true` into `g(int $i)` under `strict_types=1`.
    /// `call_user_func`/`call_user_func_array` are the documented exception: they forward the
    /// caller's frame and therefore stay on the strict path. Verified on PHP 8.4.20.
    pub(crate) fn with_internal_callback_binding<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer_strict_types = self.strict_types;
        self.strict_types = false;
        let result = operation(self);
        self.strict_types = outer_strict_types;
        result
    }

    /// Builds the extended parameter mismatch diagnostic, appending the PHP behaviour that
    /// explains why elephc refuses the binding.
    fn param_binding_error(
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        context: &str,
        detail: &str,
    ) -> CompileError {
        Self::binding_mismatch_error(expected, actual, arg.span, context, detail)
    }

    /// Formats one parameter-binding rejection at `span`, shared by the coercive and strict
    /// paths so both diagnostics read identically apart from the explanation.
    fn binding_mismatch_error(
        expected: &PhpType,
        actual: &PhpType,
        span: Span,
        context: &str,
        detail: &str,
    ) -> CompileError {
        CompileError::new(
            span,
            &format!(
                "{} expects {:?}, got {:?} — {}",
                context, expected, actual, detail
            ),
        )
    }

    /// Records the signature a bound callable-name string gives a declared `callable`
    /// parameter, so the callee resolves `$f(...)` exactly as it does for a first-class
    /// callable argument.
    fn register_bound_callable_param_sig(
        &mut self,
        owner: Option<(&str, &str)>,
        sig: FunctionSig,
    ) {
        let Some((owner_name, param_name)) = owner else {
            return;
        };
        let key = (owner_name.to_string(), param_name.to_string());
        if self.callable_param_sigs.get(&key) != Some(&sig) {
            self.callable_param_sigs.insert(key, sig);
        }
    }
}
