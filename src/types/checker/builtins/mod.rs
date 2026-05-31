//! Purpose:
//! Dispatches type checking for supported PHP builtin function families.
//! Centralizes builtin return inference, arity diagnostics, and target library requirements.
//!
//! Called from:
//! - `crate::types::checker::Checker::infer_type()` for function-call expressions.
//!
//! Key details:
//! - Builtin names must flow through the catalog so case-insensitive lookup and namespace fallback stay coherent.

mod arrays;
mod callables;
mod catalog;
mod io;
mod numeric;
mod pointers;
mod spl;
mod strings;
mod system;

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

pub(crate) use catalog::{
    canonical_builtin_function_name, is_supported_builtin_function, supported_builtin_function_names,
};
pub(crate) use callables::{
    callback_supports_complex_descriptor_env, check_preg_replace_callback_first_class_call,
    runtime_callable_array_type,
};

impl Checker {
    /// Records that a Linux target requires the given shared library for this compilation.
    ///
    /// No-op on non-Linux targets. Prevents duplicate entries in `required_libraries`.
    fn require_linux_builtin_library(&mut self, library: &str) {
        if self.target_platform == crate::codegen::platform::Platform::Linux
            && !self.required_libraries.iter().any(|lib| lib == library)
        {
            self.required_libraries.push(library.to_string());
        }
    }

    /// Type-checks a PHP builtin function call, returning the inferred return type or `None` if unhandled.
    pub fn check_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        let normalized_args;
        let args = if let Some(sig) = crate::types::builtin_call_sig(name) {
            normalized_args = self.normalize_builtin_call_args(
                &sig,
                args,
                span,
                &format!("Builtin '{}'", name),
                env,
            )?;
            normalized_args.as_slice()
        } else {
            args
        };

        if let Some(result) = strings::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = numeric::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = arrays::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = callables::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = io::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = system::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = pointers::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = spl::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        Ok(None)
    }
}
