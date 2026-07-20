//! Purpose:
//! Dispatches type checking for supported PHP builtin function families.
//! Centralizes builtin return inference, arity diagnostics, and target library requirements.
//!
//! Called from:
//! - `crate::types::checker::Checker::infer_type()` for function-call expressions.
//!
//! Key details:
//! - Builtin names must flow through the catalog so case-insensitive lookup and namespace fallback stay coherent.

pub(crate) mod arrays;
mod callables;
pub(crate) mod catalog;
pub(crate) mod io;
mod numeric;
pub(crate) mod spl;

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

pub(crate) use catalog::{
    canonical_builtin_function_name, is_php_visible_builtin_function,
    is_supported_builtin_function, strict_php_hidden_builtin,
    supported_builtin_function_names,
};
pub(crate) use callables::{
    array_element_type, array_filter_callback_arg_types, callback_supports_complex_descriptor_env,
    check_array_callback_builtin_call, check_call_user_func, check_call_user_func_array,
    check_callback_builtin_call, check_function_exists,
    check_preg_replace_callback_first_class_call,
    runtime_callable_array_type,
};

impl Checker {
    /// Records an external link library required on every target.
    pub(crate) fn require_builtin_library(&mut self, library: &str) {
        if !self.required_libraries.iter().any(|lib| lib == library) {
            self.required_libraries.push(library.to_string());
        }
    }

    /// Records that a macOS target requires the given shared library.
    ///
    /// No-op on non-macOS targets. Used for libraries that live in libc on
    /// Linux (glibc/musl) but need explicit linkage on macOS — e.g. `iconv`.
    pub(crate) fn require_macos_builtin_library(&mut self, library: &str) {
        if self.target_platform == crate::codegen::platform::Platform::MacOS
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
        // `isset`/`unset` are lazy language constructs: their operands may be an
        // undeclared property routed to `__isset`/`__unset`, which must not be
        // eagerly inferred by argument normalization. Their handlers inspect the
        // raw operands directly.
        let builtin_key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
        // `--strict-php` hides extension builtins entirely: the call must fall
        // through to user-function resolution and the standard undefined-function
        // diagnostics, mirroring PHP where these names do not exist. This must
        // run before argument normalization so the hidden builtin's signature is
        // never applied to the call.
        if catalog::strict_php_hidden_builtin(&builtin_key) {
            return Ok(None);
        }
        let is_lazy_construct = matches!(builtin_key.as_str(), "isset" | "unset");
        let normalized_args;
        let args = if let Some(sig) =
            (!is_lazy_construct).then(|| crate::types::builtin_call_sig(name)).flatten()
        {
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

        if name == "eval" {
            // eval is not registry-backed, and argument normalization tolerates
            // zero-arg calls (trailing defaults are trimmed), so arity must be
            // enforced here before the fast-path return.
            if args.len() != 1 {
                return Err(CompileError::new(span, "eval() takes exactly 1 argument"));
            }
            // The magician archive contains the encoding-aware `mb_strlen()` implementation;
            // macOS exposes iconv through a separate system library while Linux keeps it in libc.
            self.require_macos_builtin_library("iconv");
            self.infer_type(&args[0], env)?;
            return Ok(Some(PhpType::Mixed));
        }

        // Registry-first: if the builtin is registered, use its spec to check arity
        // and derive the return type (or call the spec's check hook for refined types).
        // Falls through to the legacy per-area dispatch when the name is not registered.
        if let Some(def) = crate::builtins::registry::lookup(name) {
            crate::builtins::registry::check_arity(name, args.len(), span)?;
            // Infer argument types unconditionally so that type-environment side effects
            // (variable narrowing, undefined-variable diagnostics, etc.) fire for every
            // registry builtin — including pure-data builtins that have no check hook.
            // Check hooks may still inspect inferred types; they should not call
            // infer_type again on the same args to avoid redundant inference.
            //
            // Exception: `lazy_check` builtins skip pre-inference so the check hook can
            // control argument inference order (e.g., to supply object-element type hints
            // to an unannotated closure before `infer_type` is called on it). These hooks
            // are responsible for calling `infer_type` on each argument themselves.
            if !def.spec.lazy_check {
                for arg in args.iter() {
                    self.infer_type(arg, env)?;
                }
            }
            let ret = if let Some(check) = def.spec.check {
                let mut cx = crate::builtins::spec::BuiltinCheckCtx {
                    checker: self,
                    name,
                    args,
                    span,
                    env,
                };
                check(&mut cx)?
            } else {
                def.return_type.clone()
            };
            return Ok(Some(ret));
        }

        if let Some(result) = numeric::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        if let Some(result) = arrays::check_builtin(self, name, args, span, env)? {
            return Ok(Some(result));
        }
        Ok(None)
    }
}
