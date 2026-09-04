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
pub(crate) mod out_params;
pub(crate) mod spl;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::Checker;

pub(crate) use catalog::{
    all_supported_builtin_function_names, canonical_builtin_function_name,
    is_php_visible_builtin_function_for_profile, is_supported_builtin_function,
    strict_php_hidden_builtin, supported_builtin_function_names_for_profile,
};
#[cfg(test)]
pub(crate) use catalog::is_php_visible_builtin_function;
pub(crate) use callables::{
    array_element_type, array_filter_callback_arg_types, array_key_type,
    array_walk_callback_arg_types, callback_supports_complex_descriptor_env,
    check_array_callback_builtin_call, check_call_user_func, check_call_user_func_array,
    check_function_exists,
    check_preg_replace_callback_first_class_call,
    contextual_callback_arg_positions,
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
        if self.target.platform == crate::codegen::platform::Platform::MacOS
            && !self.required_libraries.iter().any(|lib| lib == library)
        {
            self.required_libraries.push(library.to_string());
        }
    }

    /// Records the link requirements of a builtin reached through first-class callable syntax.
    ///
    /// A direct call records them while checking its arguments, but `iconv_strlen(...)`
    /// never takes that path even though the emitted callable wrapper references the same
    /// bridge entry points. A first-class callable has no arguments to inspect, so a
    /// source-dependent resolver is asked with an empty argument list, which is exactly
    /// the conservative branch every resolver already answers for a non-literal argument.
    pub(crate) fn require_first_class_callable_builtin_libraries(&mut self, name: &str) {
        let Some(def) = crate::builtins::registry::lookup(name) else {
            return;
        };
        let requirements = match def.spec.semantics.requirements {
            crate::builtins::semantics::BuiltinRequirements::Static(requirements) => {
                requirements.to_vec()
            }
            crate::builtins::semantics::BuiltinRequirements::Shared(resolve) => {
                resolve(&crate::builtins::semantics::BuiltinRequirementInput { args: &[] })
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
                crate::builtins::semantics::BuiltinRequirement::RuntimeFeature(_) => {}
            }
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
        // Which slots the NORMALIZER filled rather than the program. Empty when nothing was
        // normalized, which reads as "the program wrote all of them" — the safe direction, since
        // an unmarked slot is CHECKED rather than skipped.
        let defaulted_slots;
        let args = if let Some(sig) =
            (!is_lazy_construct).then(|| crate::types::builtin_call_sig(name)).flatten()
        {
            let (normalized, defaulted) = self.normalize_builtin_call_args_with_defaults(
                &sig,
                args,
                span,
                &format!("Builtin '{}'", name),
                env,
            )?;
            normalized_args = normalized;
            defaulted_slots = defaulted;
            normalized_args.as_slice()
        } else {
            defaulted_slots = Vec::new();
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
            // One authority for every builtin that declares a by-reference parameter. Several
            // builtins used to hand-roll this check, which is a catalogue: the ones nobody
            // wrote it for silently accepted a literal and ran, where PHP raises an Error.
            for (index, arg) in args.iter().enumerate() {
                if !def.ref_params.get(index).copied().unwrap_or(false) {
                    continue;
                }
                // A slot normalization filled is a parameter the call OMITTED, which php accepts
                // — `f(error_message: $why)` skips `$error_code` and is not an error. Only what
                // the program actually wrote is checked, so an explicit `null` there is still
                // refused, exactly as php refuses it.
                //
                // Ahead of the alias record below: a slot the caller never wrote names no local,
                // so there is nothing there to alias.
                if defaulted_slots.get(index).copied().unwrap_or(false) {
                    continue;
                }
                // `sort($a)`, `preg_match(..., $m)` and friends reach this local through its
                // storage, so the local is never kill/retype eligible in this body.
                //
                // Recorded BEFORE the spread bail-out below, not after it. `sort(...$args)` hands
                // the callee the very same by-reference parameter, so `$args` is aliased just as
                // surely; only the LVALUE-SHAPE diagnostic underneath has nothing to say about a
                // spread, which is what that bail-out is for.
                self.record_reference_alias_root(arg);
                if matches!(arg.kind, ExprKind::Spread(_)) {
                    continue;
                }
                if self.is_builtin_by_ref_argument_lvalue(arg) {
                    continue;
                }
                let param_name = def
                    .params
                    .get(index)
                    .map(|(param_name, _)| param_name.as_str())
                    .unwrap_or("arg");
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "{}(): Argument #{} (${}) could not be passed by reference",
                        name,
                        index + 1,
                        param_name
                    ),
                ));
            }
            // A by-reference output needs storage to write back into. Derived from the `ref(T)`
            // declaration so the rule holds for every such builtin without being restated.
            out_params::check_write_only_args(name, args)?;
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
                    crate::builtins::semantics::BuiltinRequirement::RuntimeFeature(_) => {}
                }
            }
            if !matches!(
                def.spec.semantics.validation,
                crate::builtins::semantics::BuiltinValidation::CheckerHook { .. }
            ) {
                // A write-only by-ref argument is a definition, not a use, so it is not read
                // here; its type comes from the parameter's declaration. `Mixed` stands in for
                // the argument type the shared validators see, matching PHP's pre-call `null`.
                let write_only = out_params::write_only_variable_args(name, args);
                let mut arg_types = Vec::with_capacity(args.len());
                for (idx, arg) in args.iter().enumerate() {
                    if write_only.iter().any(|out| out.index == idx) {
                        arg_types.push(PhpType::Mixed);
                        continue;
                    }
                    let inferred = self.infer_type(arg, env)?;
                    arg_types.push(null_coerced_builtin_arg_type(def, idx, inferred));
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
                // A contextual callback position is deliberately left to the hook, which
                // types the closure's unannotated parameters from the array element/key
                // before checking its body. Pre-inferring it here would check that body
                // once against the unhinted parameter fallback and reject valid PHP.
                let contextual = contextual_callback_arg_positions(name);
                // Likewise for a write-only by-ref position: reading it would reject the
                // undeclared variable PHP's own out-parameter idiom passes there.
                let write_only = out_params::write_only_variable_args(name, args);
                for (idx, arg) in args.iter().enumerate() {
                    if contextual.contains(&idx) || write_only.iter().any(|out| out.index == idx) {
                        continue;
                    }
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

/// Applies php's null-to-scalar coercion to ONE builtin argument type, for validation.
///
/// php coerces a written `null` into a non-nullable scalar parameter of an INTERNAL function
/// rather than refusing: `strlen(null)` and `strlen($x)` with `$x = null` both answer `int(0)`,
/// after a `Passing null to parameter #1 ($string) of type string is deprecated` notice.
/// MEASURED on `php -n` 8.5.6.
///
/// The LOWERING already knows this — `coerce_null_operands_to_builtin_params` replaces the
/// operand with the parameter's zero value — but the checker ran first and saw a bare `Void`,
/// so the shared validators refused the program before the coercion could happen:
/// `error: strlen() argument must be string`. The two layers now read the same rule.
///
/// The conditions are the lowering's, for the same reasons it states them: a BY-REFERENCE
/// parameter receives storage rather than a value, and a parameter php spells `?T $x = null`
/// accepts null as a VALUE of its own rather than coercing it.
fn null_coerced_builtin_arg_type(
    def: &crate::builtins::registry::BuiltinDef,
    index: usize,
    inferred: PhpType,
) -> PhpType {
    if !matches!(inferred, PhpType::Void) {
        return inferred;
    }
    let Some(param) = def.spec.params.get(index) else {
        return inferred;
    };
    if param.by_ref
        || matches!(param.default, Some(crate::builtins::spec::DefaultSpec::Null))
    {
        return inferred;
    }
    match crate::builtins::convert::type_spec_to_php(&param.ty) {
        scalar @ (PhpType::Int | PhpType::Str | PhpType::Float | PhpType::Bool) => scalar,
        _ => inferred,
    }
}
