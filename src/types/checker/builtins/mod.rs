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
pub(crate) use callables::callback_dummy_arg_for_type;
pub(crate) mod catalog;
pub(crate) mod io;
mod late_bound;
mod language_constructs;
pub(crate) mod spl;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::Checker;

pub(crate) use catalog::{
    all_supported_builtin_function_names, builtin_absent_by_php_platform_rule,
    canonical_builtin_function_name,
    is_php_visible_builtin_function_for_profile, is_php_visible_builtin_function_for_target,
    is_supported_builtin_function, strict_php_hidden_builtin,
    supported_builtin_function_names_for_profile, supported_builtin_function_names_for_target,
};
#[cfg(test)]
pub(crate) use catalog::is_php_visible_builtin_function;
pub(crate) use callables::{
    array_element_type, array_filter_callback_arg_types, array_key_type,
    callback_supports_complex_descriptor_env,
    check_array_callback_builtin_call, check_array_walk_callback_builtin_call,
    check_call_user_func, check_call_user_func_array, check_function_exists,
    check_preg_replace_callback_first_class_call, preg_operand_may_be_array,
    contextual_callback_arg_positions,
    runtime_callable_array_type,
};
pub(crate) use arrays::array_arg_is_gradually_acceptable;
pub(crate) use late_bound::is_late_bound_undefined_function;

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

    /// Validates writable storage for every fixed or variadic by-reference builtin argument.
    ///
    /// One authority for every builtin that declares a by-reference parameter. Several
    /// builtins used to hand-roll this check, which is a catalogue: the ones nobody
    /// wrote it for silently accepted a literal and ran, where PHP raises an Error.
    ///
    /// An argument past the fixed parameters maps onto the variadic entry, which carries
    /// the shared contract's own passing mode — `sscanf("12", "%d", "not writable")` has
    /// no storage behind its third argument either.
    ///
    /// `plan` is the call's normalized argument plan, and an argument it MATERIALIZED from a
    /// declared default is skipped: `preg_match($p, $s)` reaches here with a literal in the
    /// `$matches` position that the caller never wrote, and PHP has nothing to complain about.
    fn validate_registry_by_ref_args(
        &mut self,
        def: &crate::builtins::registry::BuiltinDef,
        name: &str,
        args: &[Expr],
        plan: Option<&crate::types::call_args::CallArgPlan>,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let regular_param_count = def
            .params
            .len()
            .saturating_sub(usize::from(def.variadic.is_some()));
        for (arg_index, arg) in args.iter().enumerate() {
            let param_index = if arg_index < regular_param_count {
                arg_index
            } else if def.variadic.is_some() {
                def.params.len().saturating_sub(1)
            } else {
                continue;
            };
            if !def.ref_params.get(param_index).copied().unwrap_or(false) {
                continue;
            }
            if matches!(
                plan.and_then(|plan| plan.regular_args.get(arg_index)),
                Some(crate::types::call_args::PlannedRegularArg::Default(_))
            ) {
                continue;
            }
            // `sort($a)`, `preg_match(..., $m)` and friends reach this local through its
            // storage, so the local is never kill/retype eligible in this body.
            //
            // Recorded BEFORE the spread bail-out below, not after it. `sort(...$args)` hands
            // the callee the very same by-reference parameter, so `$args` is aliased just as
            // surely; only the LVALUE-SHAPE diagnostic underneath has nothing to say about a
            // spread, which is what that bail-out is for.
            //
            // Registry builtins do not retain the cell, so this deliberately leaves the narrower
            // non-builtin alias set unchanged.
            self.record_builtin_reference_alias_root(arg);
            // `sort($o->inner->items)` writes the sorted array back through a receiver whose
            // class the checker cannot name, as a runtime-shaped value. Any class's untyped
            // `$items` may be the slot, so none of them can keep a refined raw shape: the backend
            // refuses a boxed value into one at run time. Same widening an `unset()` through such
            // a receiver needs.
            if let ExprKind::PropertyAccess { object, property } = &arg.kind {
                let object_ty = self.infer_type(object, env)?;
                if crate::types::checker::single_object_class_name(&object_ty).is_none() {
                    super::clone_override_storage::record_property_unset_destination(
                        self,
                        &object_ty,
                        Some(property.as_str()),
                    );
                }
            }
            if matches!(arg.kind, ExprKind::Spread(_)) {
                continue;
            }
            if self.is_builtin_by_ref_argument_lvalue(arg) {
                continue;
            }
            let param_name = def
                .params
                .get(param_index)
                .map(|(param_name, _)| param_name.as_str())
                .unwrap_or("arg");
            if param_index >= regular_param_count {
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "{}() variadic parameter ${} must be passed a variable",
                        def.name, param_name
                    ),
                ));
            }
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "{}(): Argument #{} (${}) could not be passed by reference",
                    name,
                    arg_index + 1,
                    param_name
                ),
            ));
        }
        Ok(())
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
        // The declaration lookup scans every declared function, so it runs last: almost every
        // builtin is available on the target, which settles the condition without it.
        if crate::builtins::registry::lookup(name).is_some()
            && !catalog::builtin_is_available_for_target(name, self.target)
            && self.has_function_decl_folded(name)
        {
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
        // The written call, before `plan_builtin_call_args` materializes defaults and rewrites
        // spreads. Only these arguments say how many values a spread of a written-out array
        // actually supplies, which is what the arity check below needs.
        let source_args = args;
        // CUF accepts named variadic entries, then checks them against the callback.
        // Its own callback parameter must still obey duplicate and ordering rules.
        let forwards_callback_names = builtin_key == "call_user_func";
        let normalized_args;
        let mut builtin_arg_plan = None;
        let args = if let Some(sig) =
            (!is_lazy_construct).then(|| crate::types::builtin_call_sig(name)).flatten()
        {
            normalized_args = if forwards_callback_names {
                self.normalize_named_call_args(&sig, args, span, &format!("Builtin '{}'", name), env)?
            } else {
                let plan = self.plan_builtin_call_args(
                    &sig,
                    args,
                    span,
                    &format!("Builtin '{}'", name),
                    env,
                )?;
                let normalized = plan.normalized_args();
                builtin_arg_plan = Some(plan);
                normalized
            };
            normalized_args.as_slice()
        } else {
            args
        };
        // The source planner keeps indexed unpacks visible for runtime checks.
        // Builtin signatures count literal unpacked values instead of containers,
        // but only after the planner has validated ordering and named arguments.
        let static_positional_args = (!is_lazy_construct)
            .then(|| crate::types::call_args::expand_planned_positional_spreads(args))
            .flatten();
        let args = static_positional_args.as_deref().unwrap_or(args);
        let mut spread_type_error = None;
        let indexed_spread_args = (!is_lazy_construct && !forwards_callback_names
            && crate::types::builtin_call_sig(name)
                .is_some_and(|sig| !sig.ref_params.iter().any(|by_ref| *by_ref)))
            .then(|| crate::types::call_args::coalesce_planned_indexed_spreads(args, |source| {
                match self.infer_type(source, env) {
                    Ok(ty) => matches!(ty.codegen_repr(), PhpType::Array(_)),
                    Err(error) => { spread_type_error = Some(error); false }
                }
            }))
            .flatten();
        if let Some(error) = spread_type_error {
            return Err(error);
        }
        let args = indexed_spread_args.as_deref().unwrap_or(args);

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
            // Eval can select any generated method or constructor by a runtime string. Magician
            // collects every source variadic in a boxed-Mixed array, so the native frame must use
            // that same storage even when PHP declared a narrower element type. The declaration's
            // type expression remains authoritative for per-element coercion and Reflection.
            self.promote_eval_native_variadic_containers()?;
            self.infer_type(&args[0], env)?;
            return Ok(Some(PhpType::Mixed));
        }

        // Registry-backed builtins use their spec for arity, requirements,
        // validation, and result typing. Only compiler-resident language
        // constructs continue below this branch.
        if let Some(def) = crate::builtins::registry::lookup(name) {
            // A spread stands for however many elements its array holds, so the WRITTEN argument
            // count is not the call's arity. Counting `...$a` as one argument rejected perfectly
            // valid PHP — `method_exists(...$controller)` in Symfony's `ControllerEvent`,
            // `str_contains(...$args)` — with "takes exactly 2 arguments", even though the
            // backend lowers such a call correctly.
            //
            // Internal functions DO reject a surplus (`tmpfile(...[1])` is an
            // `ArgumentCountError`), unlike userland ones, so a spread whose array is WRITTEN OUT
            // keeps every check it had: the count is knowable and `tmpfile`'s own hook reads the
            // spread's shape directly. Only a spread of unknown length is unanswerable here, and
            // PHP settles that one at the call too.
            let has_spread_arg = args
                .iter()
                .any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
            let written_out_spread = has_spread_arg && spread_expanded_arg_count(source_args).is_some();
            let unknown_length_spread = has_spread_arg && !written_out_spread;
            // An unpack contributes its runtime entries, not one argument. Shared
            // validators accept unknown argument types and leave dynamic bounds to
            // EIR binding. Mbstring checker hooks also accept dynamic spreads;
            // other legacy hooks still require their fixed AST shape.
            let runtime_arity = builtin_arg_plan.as_ref().is_some_and(|plan| plan.has_spread_args())
                || has_spread_arg;
            let runtime_arity = runtime_arity
                && (builtin_key.starts_with("mb_")
                    || !matches!(
                        def.spec.semantics.validation,
                        crate::builtins::semantics::BuiltinValidation::CheckerHook { .. }
                    ));
            if !unknown_length_spread && !(runtime_arity && !written_out_spread) {
                crate::builtins::registry::check_arity(name, args.len(), span)?;
            }
            // A builtin the registry knows but this target does not provide is a hard error,
            // not a fallthrough: the call cannot lower.
            if !catalog::builtin_is_available_for_target(name, self.target) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{}() is not available for the {} target",
                        def.name, self.target
                    ),
                ));
            }
            self.validate_registry_by_ref_args(def, name, args, builtin_arg_plan.as_ref(), env)?;
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
            // Every per-builtin validator and result-type resolver below reads the arguments
            // positionally — `array_map`'s reaches straight for `args[1]` — because `check_arity`
            // guaranteed the shape. A spread removes that guarantee, so the only sound answer is
            // the declared return type. The arguments are still inferred, so narrowing,
            // undefined-variable diagnostics and the library requirements above all still fire.
            //
            // Builtins with a checker hook are the ones lowered through a dedicated EIR shape
            // rather than the polymorphic runtime call, and that shape needs each operand named
            // at compile time. They are refused here, with the actual reason, rather than left to
            // surface as an operand-count mismatch from EIR validation.
            // Mbstring checker hooks read a spread themselves (see `runtime_arity` above), so
            // they keep their own validation instead of this positional bail-out.
            if unknown_length_spread && !builtin_key.starts_with("mb_") {
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                // `array_map(...$params)`, `range(...explode('-', $s))`: the operand count is a
                // run-time value, and `crate::array_gradual_prelude` carries a helper that
                // dispatches on it, so these are answered with the declared (`mixed`) type
                // instead of being refused.
                if matches!(
                    def.spec.semantics.validation,
                    crate::builtins::semantics::BuiltinValidation::CheckerHook { .. }
                ) && crate::array_gradual_prelude::spread_helper(&builtin_key).is_none()
                    && args.len() < crate::builtins::registry::enforced_arity_bounds(name)
                        .map_or(0, |(min, _)| min)
                {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{name}() cannot take a spread argument: it is compiled through a \
                             dedicated lowering that needs every argument written out. Unpack the \
                             array into the call instead."
                        ),
                    ));
                }
                return Ok(Some(def.return_type.clone()));
            }
            if !matches!(
                def.spec.semantics.validation,
                crate::builtins::semantics::BuiltinValidation::CheckerHook { .. }
            ) {
                let mut arg_types = Vec::with_capacity(args.len());
                for arg in args {
                    arg_types.push(self.infer_type(arg, env)?);
                }
                reject_array_at_string_param(name, &def.params, def.variadic.is_some(), args, &arg_types)?;
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
                for (idx, arg) in args.iter().enumerate() {
                    if contextual.contains(&idx) {
                        continue;
                    }
                    self.infer_type(arg, env)?;
                }
            }
            let mut cx = crate::builtins::spec::BuiltinCheckCtx {
                checker: self,
                name,
                args,
                argument_plan: builtin_arg_plan.as_ref(),
                span,
                env,
            };
            let ret = check(&mut cx)?;
            {
                // A callback written as an assignment -- `array_any($tags, $closure = function
                // (array $tag) { ... })` (Symfony monolog-bundle `AddProcessorsPass`), reusing
                // `$closure` on the next line -- was skipped above, so the variable never
                // reached the environment and its later read was `Undefined variable`. The hook
                // has now checked the closure with its contextual parameter types; binding the
                // variable is best-effort and must never turn that valid check into a refusal.
                let contextual = contextual_callback_arg_positions(name);
                for (idx, arg) in args.iter().enumerate() {
                    if contextual.contains(&idx) && matches!(arg.kind, ExprKind::Assignment { .. }) {
                        let _ = self.infer_type(arg, env);
                    }
                }
            }
            return Ok(Some(ret));
        }

        if matches!(builtin_key.as_str(), "exit" | "die" | "empty" | "unset" | "isset") {
            return language_constructs::check(self, &builtin_key, args, span, env).map(Some);
        }
        Ok(None)
    }
}

/// Refuses a statically-known ARRAY at a builtin parameter declared `string`.
///
/// A builtin with no checker hook is validated for arity only, and its arguments then meet their
/// parameters in the IR, where `string <- mixed` is a runtime cast. A bare `array` declaration is
/// `array<mixed>|array<mixed, mixed>`, whose codegen form is `mixed`, so it took that cast and
/// became the string "Array": `str_replace($a, "z", "xyx")` returned `xyx`, searching for "Array",
/// silently. php throws `TypeError` for an array at a string parameter, and a typed `array<int>` was
/// already refused further down; this gives the bare declaration the same answer, at compile time,
/// in php's own words.
///
/// Some of these builtins accept `array|string` in php-src (`substr_replace`, `preg_replace`)
/// where elephc's contract narrows the parameter to `string`. For those this is the honest
/// refusal of a form elephc does not implement, instead of a wrong answer. `str_replace` and
/// `str_ireplace` implement php's array forms (search, replace and subject — the result follows
/// the subject's shape, see `subject_shaped_result_type`) and are never refused here: Symfony's
/// console `ErrorListener` replaces an array of quoted spellings.
fn reject_array_at_string_param(
    name: &str,
    params: &[(String, PhpType)],
    variadic: bool,
    args: &[Expr],
    arg_types: &[PhpType],
) -> Result<(), CompileError> {
    if name.eq_ignore_ascii_case("str_replace") || name.eq_ignore_ascii_case("str_ireplace") {
        return Ok(());
    }
    let fixed = if variadic { params.len().saturating_sub(1) } else { params.len() };
    for (index, (arg, ty)) in args.iter().zip(arg_types).enumerate() {
        // A named argument reaches the same parameter by name, and php numbers it by that
        // parameter's position; a spread's targets are unknown until it is unpacked.
        let index = match &arg.kind {
            ExprKind::Spread(_) => return Ok(()),
            ExprKind::NamedArg { name: written, .. } => {
                match params[..fixed].iter().position(|(param, _)| param == written) {
                    Some(position) => position,
                    None => continue,
                }
            }
            _ if index >= fixed => break,
            _ => index,
        };
        let (param, param_ty) = &params[index];
        let is_array = ty.is_php_array()
            || matches!(ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. });
        if *param_ty == PhpType::Str && is_array {
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "{}(): Argument #{} (${}) must be of type string, array given",
                    name,
                    index + 1,
                    param
                ),
            ));
        }
    }
    Ok(())
}

/// Returns the real argument count of a call whose every spread has a written-out length.
///
/// `tmpfile(...[1])` is one argument, not one spread, and PHP's internal functions reject the
/// surplus with `ArgumentCountError` — so the arity check has to see the expanded count. A spread
/// of anything else (`...$args`) has no count until run time and returns `None`.
fn spread_expanded_arg_count(args: &[Expr]) -> Option<usize> {
    let mut total = 0usize;
    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => match &inner.kind {
                ExprKind::ArrayLiteral(elements)
                    if !elements
                        .iter()
                        .any(|element| matches!(element.kind, ExprKind::Spread(_))) =>
                {
                    total += elements.len();
                }
                _ => return None,
            },
            ExprKind::NamedArg { .. } => return None,
            _ => total += 1,
        }
    }
    Some(total)
}
