//! Purpose:
//! Type-checks callable extern calls behavior.
//! Infers callable signatures and validates invocation details that affect later lowering and optimizer effects.
//!
//! Called from:
//! - `crate::types::checker::callables`
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - Closure captures, first-class callable syntax, and extern calls must agree with shared call argument planning.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Type-checks an extern function call.
    ///
    /// Looks up both the extern signature (`extern_sig`) and the user-defined function signature
    /// (`sig`) for the given name. Normalizes named/spread arguments using shared call-argument
    /// planning, then validates argument count and each argument's type against the extern signature.
    ///
    /// Callable-typed extern parameters accept only string literals naming a user function;
    /// registers the callback via `register_callback_function` when a match is found.
    ///
    /// Returns the extern's `return_type` on success, or a `CompileError` if the function is
    /// undefined, argument count is wrong, or any argument type is incompatible.
    pub(crate) fn check_extern_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let extern_sig = self.extern_functions.get(name).cloned().ok_or_else(|| {
            CompileError::new(span, &format!("Undefined extern function: {}", name))
        })?;

        let sig = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;

        let normalized_args = self.normalize_named_call_args(
            &sig,
            args,
            span,
            &format!("Extern function '{}'", name),
            env,
        )?;
        let args = normalized_args.as_slice();

        self.check_call_arity("Extern function", name, &sig, args, span)?;

        for (idx, arg) in args.iter().enumerate() {
            let Some((param_name, expected_ty)) = extern_sig.params.get(idx) else {
                break;
            };

            if *expected_ty == PhpType::Callable {
                match &arg.kind {
                    ExprKind::StringLiteral(callback_name) => {
                        self.register_callback_function(callback_name, span)?;
                    }
                    _ => {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "Extern function '{}' parameter ${} expects a string literal naming a user function",
                                name, param_name
                            ),
                        ));
                    }
                }
                continue;
            }

            let actual_ty = self.infer_type(arg, env)?;
            self.require_compatible_arg_type(
                expected_ty,
                &actual_ty,
                arg.span,
                &format!("Extern function '{}' parameter ${}", name, param_name),
            )?;
        }

        Ok(extern_sig.return_type)
    }

    /// Validates that the number of provided arguments matches the callee's arity requirements.
    ///
    /// `kind` and `name` are used only in error messages. The check respects:
    /// - Required parameters (those without defaults)
    /// - Optional parameters (those with defaults)
    /// - Variadic parameters (which absorb any number of additional arguments)
    ///
    /// Spread arguments bypass arity validation entirely. When `variadic` is set, only the
    /// lower bound of required arguments is enforced.
    ///
    /// Returns `Ok(())` if argument count is valid, or a `CompileError` describing the mismatch.
    pub(crate) fn check_call_arity(
        &self,
        kind: &str,
        name: &str,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        if has_spread {
            return Ok(());
        }

        let required = sig.defaults.iter().filter(|d| d.is_none()).count();
        if sig.variadic.is_some() {
            if effective_arg_count < required {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} '{}' expects at least {} arguments, got {}",
                        kind, name, required, effective_arg_count
                    ),
                ));
            }
        } else if effective_arg_count < required || effective_arg_count > sig.params.len() {
            let expected = if required == sig.params.len() {
                format!("{}", required)
            } else {
                format!("{} to {}", required, sig.params.len())
            };
            return Err(CompileError::new(
                span,
                &format!(
                    "{} '{}' expects {} arguments, got {}",
                    kind, name, expected, effective_arg_count
                ),
            ));
        }

        Ok(())
    }
}
