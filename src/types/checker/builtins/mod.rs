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
mod language_constructs;
pub(crate) mod spl;

use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

pub(crate) use catalog::{
    builtin_available_on_platform, canonical_builtin_function_name,
    canonical_builtin_function_name_on_platform, is_php_visible_builtin_function,
    is_supported_builtin_function, strict_php_hidden_builtin,
    supported_builtin_function_names_on_platform,
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

    /// Records that a Windows target requires the given shared library.
    ///
    /// No-op on non-Windows targets. Used for libraries that live in libc on
    /// Linux (glibc/musl) and libSystem on macOS but need explicit linkage on
    /// Windows because msvcrt does not ship them — e.g. `iconv`, which the
    /// `convert.iconv.*` stream filter lowers to `iconv_open`/`iconv`/
    /// `iconv_close` C symbols resolved by a cross-built libiconv in CI.
    pub(crate) fn require_windows_builtin_library(&mut self, library: &str) {
        if self.target_platform == crate::codegen::platform::Platform::Windows
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
        if !catalog::builtin_available_on_platform(&builtin_key, self.target_platform) {
            return Ok(None);
        }
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

        // Registry-backed builtins use their spec for arity, requirements,
        // validation, and result typing. Only compiler-resident language
        // constructs continue below this branch.
        if let Some(def) = crate::builtins::registry::lookup(name) {
            crate::builtins::registry::check_arity(name, args.len(), span)?;
            let requirement_input = crate::builtins::semantics::BuiltinRequirementInput {
                args,
            };
            let requirements = match def.spec.semantics.requirements {
                crate::builtins::semantics::BuiltinRequirements::Static(requirements) => {
                    requirements.to_vec()
                }
                crate::builtins::semantics::BuiltinRequirements::Shared(resolve) => {
                    resolve(&requirement_input)
                }
            };
            for requirement in requirements {
                match requirement {
                    crate::builtins::semantics::BuiltinRequirement::Bridge(library)
                    | crate::builtins::semantics::BuiltinRequirement::SystemLibrary(library) => {
                        self.require_builtin_library(library);
                    }
                    crate::builtins::semantics::BuiltinRequirement::MacOsLibrary(library) => {
                        self.require_macos_builtin_library(library);
                    }
                    crate::builtins::semantics::BuiltinRequirement::WindowsLibrary(library) => {
                        self.require_windows_builtin_library(library);
                    }
                    crate::builtins::semantics::BuiltinRequirement::RuntimeFeature(_) => {}
                }
            }
            if !matches!(
                def.spec.semantics.validation,
                crate::builtins::semantics::BuiltinValidation::CheckerHook { .. }
            ) {
                let mut arg_types = Vec::with_capacity(args.len());
                for arg in args {
                    arg_types.push(self.infer_type(arg, env)?);
                }
                let semantic_input = crate::builtins::semantics::BuiltinSemanticInput {
                    name: &builtin_key,
                    args,
                    arg_types: &arg_types,
                    span,
                };
                if let crate::builtins::semantics::BuiltinValidation::Shared(validate) =
                    def.spec.semantics.validation
                {
                    validate(&semantic_input)?;
                }
                let ret = match def.spec.semantics.result_type {
                    crate::builtins::semantics::BuiltinResultType::Declared => {
                        def.return_type.clone()
                    }
                    crate::builtins::semantics::BuiltinResultType::Shared(resolve) => {
                        resolve(&semantic_input)
                    }
                    crate::builtins::semantics::BuiltinResultType::Checked => {
                        return Err(CompileError::new(
                            span,
                            "shared builtin validation must define a shared or declared result type",
                        ));
                    }
                };
                return Ok(Some(ret));
            }
            // Infer argument types unconditionally so that type-environment side effects
            // (variable narrowing, undefined-variable diagnostics, etc.) fire for every
            // registry builtin — including pure-data builtins that have no check hook.
            // Check hooks may still inspect inferred types; they should not call
            // infer_type again on the same args to avoid redundant inference.
            //
            // Exception: lazy checker hooks skip pre-inference so the hook can
            // control argument inference order (e.g., to supply object-element type hints
            // to an unannotated closure before `infer_type` is called on it). These hooks
            // are responsible for calling `infer_type` on each argument themselves.
            let crate::builtins::semantics::BuiltinValidation::CheckerHook {
                check,
                lazy,
            } = def.spec.semantics.validation
            else {
                unreachable!("non-checker builtin returned from semantic validation branch");
            };
            if !lazy {
                for arg in args.iter() {
                    self.infer_type(arg, env)?;
                }
            }
            let mut cx = crate::builtins::spec::BuiltinCheckCtx {
                checker: self,
                name,
                args,
                span,
                env,
            };
            let ret = check(&mut cx)?;
            return Ok(Some(ret));
        }

        if matches!(builtin_key.as_str(), "exit" | "die" | "empty" | "unset" | "isset") {
            return language_constructs::check(self, &builtin_key, args, span, env).map(Some);
        }
        Ok(None)
    }
}
