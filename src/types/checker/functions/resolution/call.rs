//! Purpose:
//! Resolves direct function calls, forward declarations, variants, externs, and builtin fallback.
//!
//! Called from:
//! - Expression inference whenever PHP source invokes a named function.
//!
//! Key details:
//! - Named and spread arguments are normalized before specialization and type validation.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use crate::types::checker::Checker;

impl Checker {
    /// Determines the element type for a variadic parameter's container.
    ///
    /// If the element type is `Iterable`, returns `Mixed` since iterables can
    /// hold heterogeneous values at runtime. Otherwise returns the element type unchanged.
    pub(super) fn variadic_container_elem_ty(elem_ty: PhpType) -> PhpType {
        if matches!(elem_ty, PhpType::Iterable) {
            PhpType::Mixed
        } else {
            elem_ty
        }
    }

    /// Returns true if any named argument in `args` does not correspond to a
    /// declared regular (non-variadic) parameter in `regular_params`.
    ///
    /// Used to decide whether a variadic parameter should receive type `Iterable`
    /// (when unknown named args may flow into it) or `Array<elem>` (when all
    /// named args are known).
    pub(super) fn has_unknown_named_variadic_arg(args: &[Expr], regular_params: &[String]) -> bool {
        args.iter().any(|arg| {
            matches!(
                &arg.kind,
                ExprKind::NamedArg { name, .. } if !regular_params.iter().any(|param| param == name)
            )
        })
    }

    /// Resolves a function call by name, normalizes arguments (named, spread),
    /// validates arity and types, applies specialization if needed, and returns
    /// the function's return type.
    ///
    /// Handles three lookup paths:
    /// - Declared functions already in `self.functions`
    /// - Function variant groups (which may emit a provisional signature)
    /// - Forward declarations from `fn_decls` (where type hints are resolved here)
    ///
    /// Emits a deprecation warning if the function is deprecated.
    /// Re-specializes resolved function parameters when the call site demands it.
    /// Builds the diagnostic for a call that resolved to no declared function, builtin signature,
    /// variant group, or extern. When `name` is a procedural date/time alias (which the name
    /// resolver desugars only at its supported arities), reaching this point means the call's
    /// argument count was out of range, so report a precise arity error — matching
    /// `function_exists()`, which already recognizes these names — instead of the misleading
    /// "Undefined function". Non-alias names keep the plain "Undefined function" diagnostic.
    pub(super) fn unresolved_function_call_error(&self, name: &str, span: crate::span::Span) -> CompileError {
        if let Some((min, max)) = crate::name_resolver::date_procedural_alias_arity(name) {
            let bare = name.rsplit('\\').next().unwrap_or(name);
            let message = if min == max {
                format!(
                    "{}() takes exactly {} argument{}",
                    bare,
                    min,
                    if min == 1 { "" } else { "s" }
                )
            } else if max == min + 1 {
                format!("{}() takes {} or {} arguments", bare, min, max)
            } else {
                format!("{}() takes {} to {} arguments", bare, min, max)
            };
            return CompileError::new(span, &message);
        }
        // Under `--strict-php`, a name that would resolve to an extension builtin
        // lands here as undefined. Name the disabled extension so users understand
        // why a working non-strict program stopped compiling.
        let bare = name.rsplit('\\').next().unwrap_or(name);
        let key = crate::names::php_symbol_key(bare);
        if crate::types::checker::builtins::strict_php_hidden_builtin(&key) {
            return CompileError::new(
                span,
                &format!(
                    "Undefined function: {} ({}() exists as an elephc extension; it is disabled by --strict-php)",
                    name, key
                ),
            );
        }
        CompileError::new(span, &format!("Undefined function: {}", name))
    }

    /// Checks a function call, including externs, declared functions, variants,
    /// builtins, call-argument normalization, and return-type inference.
    pub fn check_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let bare_name = name
            .trim_start_matches('\\')
            .rsplit('\\')
            .next()
            .unwrap_or(name)
            .to_ascii_lowercase();
        if matches!(bare_name.as_str(), "mktime" | "gmmktime")
            && (args.is_empty() || args.len() > 6)
            && !args.iter().any(|arg| {
                matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
            })
        {
            for arg in args {
                self.infer_type(arg, caller_env)?;
            }
            return Ok(PhpType::Int);
        }
        if let Some(extern_name) = self.canonical_extern_function_name_folded(name) {
            return self.check_extern_function_call(&extern_name, args, span, caller_env);
        }
        let canonical_name = self
            .canonical_function_name_folded(name)
            .unwrap_or_else(|| name.to_string());
        let name = canonical_name.as_str();

        if let Some(mut sig) = self.functions.get(name).cloned() {
            if let Some(reason) = sig.deprecation.as_deref() {
                let message = if reason.is_empty() {
                    format!("Call to deprecated function: {}()", name)
                } else {
                    format!("Call to deprecated function: {}() — {}", name, reason)
                };
                self.warnings
                    .push(crate::errors::CompileWarning::new(span, &message));
            }
            let mut effective_sig =
                Self::callable_sig_for_declared_params(&sig, &sig.declared_params);
            let normalized_args = self.normalize_named_call_args(
                &effective_sig,
                args,
                span,
                &format!("Function '{}'", name),
                caller_env,
            )?;
            if self.respecialize_resolved_function_params_if_needed(
                name,
                &normalized_args,
                caller_env,
            )? {
                sig = self.functions.get(name).cloned().ok_or_else(|| {
                    CompileError::new(span, &format!("Undefined function: {}", name))
                })?;
                effective_sig =
                    Self::callable_sig_for_declared_params(&sig, &sig.declared_params);
            }
            return self.check_normalized_resolved_function_call(
                name,
                &sig,
                &effective_sig,
                &normalized_args,
                span,
                caller_env,
            );
        }

        if self.function_variant_groups.contains_key(name) {
            let ensure_error = self
                .ensure_function_variant_group_signature(name, span)
                .err();
            if let Some(error) = ensure_error.as_ref() {
                if !self.functions.contains_key(name) {
                    return Err(error.clone());
                }
            }
            let provisional_sig = self.functions.get(name).cloned();
            let result = self.check_function_call(name, args, span, caller_env);
            if let Some(error) = ensure_error {
                if provisional_sig == self.functions.get(name).cloned() {
                    return Err(error);
                }
            }
            return result;
        }

        if self.eval_barrier_active {
            for arg in args {
                self.infer_type(arg, caller_env)?;
            }
            return Ok(PhpType::Mixed);
        }

        let decl = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| self.unresolved_function_call_error(name, span))?;
        let normalization_sig = FunctionSig {
            params: decl
                .params
                .iter()
                .enumerate()
                .map(|(idx, param_name)| {
                    let ty = decl
                        .param_types
                        .get(idx)
                        .and_then(|type_ann| type_ann.as_ref())
                        .and_then(|type_ann| self.resolve_type_expr(type_ann, decl.span).ok())
                        .unwrap_or(PhpType::Int);
                    (param_name.clone(), ty)
                })
                .chain(
                    decl.variadic
                        .iter()
                        .cloned()
                        .map(|name| {
                            let elem_ty = if decl.variadic_by_ref {
                                PhpType::Mixed
                            } else {
                                PhpType::Int
                            };
                            (name, PhpType::Array(Box::new(elem_ty)))
                        }),
                )
                .collect(),
            param_type_exprs: decl
                .param_types
                .iter()
                .cloned()
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.clone()))
                .collect(),
            param_attributes: decl.param_attributes.clone(),
            defaults: decl.defaults.clone(),
            return_type: PhpType::Int,
            declared_return: decl.return_type.is_some(),
            by_ref_return: false,
            ref_params: decl.ref_params.clone(),
            declared_params: decl
                .param_types
                .iter()
                .map(|type_ann| type_ann.is_some())
                .chain(decl.variadic.iter().map(|_| decl.variadic_type.is_some()))
                .collect(),
            variadic: decl.variadic.clone(),
            deprecation: None,
        };
        let normalized_args = self.normalize_named_call_args(
            &normalization_sig,
            args,
            span,
            &format!("Function '{}'", name),
            caller_env,
        )?;
        let args = normalized_args.as_slice();
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        let required = decl.defaults.iter().filter(|d| d.is_none()).count();
        if decl.ref_params.iter().any(|is_ref| *is_ref) && has_spread {
            return Err(CompileError::new(
                span,
                &format!(
                    "Function '{}' cannot be invoked with spread arguments when it has pass-by-reference parameters",
                    name
                ),
            ));
        }
        if !has_spread {
            if decl.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects at least {} arguments, got {}",
                            name, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > decl.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        Self::format_fixed_or_range_arity(required, decl.params.len()),
                        effective_arg_count
                    ),
                ));
            }
        }

        let mut param_types = Vec::new();
        let mut arg_idx = 0;
        for arg in args {
            let ty = self.infer_type(arg, caller_env)?;
            if let ExprKind::Spread(_) = &arg.kind {
                for i in arg_idx..decl.params.len() {
                    if let Some(type_ann) = decl.param_types.get(i).and_then(|t| t.as_ref()) {
                        let declared_ty = self.resolve_declared_param_type_hint(
                            type_ann,
                            decl.span,
                            &format!("Function '{}' parameter ${}", name, decl.params[i]),
                        )?;
                        self.require_compatible_arg_type(
                            &declared_ty,
                            &ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, decl.params[i]),
                        )?;
                        param_types.push((decl.params[i].clone(), declared_ty));
                    } else if matches!(ty, PhpType::Never) {
                        let param_ty = decl
                            .defaults
                            .get(i)
                            .and_then(|default| default.as_ref())
                            .map(|default| self.infer_type(default, caller_env))
                            .transpose()?
                            .unwrap_or(PhpType::Int);
                        param_types.push((decl.params[i].clone(), param_ty));
                    } else {
                        param_types.push((decl.params[i].clone(), ty.clone()));
                    }
                }
                arg_idx = decl.params.len();
            } else if arg_idx < decl.params.len() {
                if ty == PhpType::Callable {
                    if let Some(sig) = self.resolve_expr_callable_sig(arg, caller_env)? {
                        self.callable_param_sigs.insert(
                            (name.to_string(), decl.params[arg_idx].clone()),
                            sig,
                        );
                    }
                }
                if is_callable_array_type(&ty) {
                    if let Some(sig) = self.resolve_expr_callable_array_sig(arg, caller_env)? {
                        self.callable_param_sigs.insert(
                            (name.to_string(), decl.params[arg_idx].clone()),
                            sig,
                        );
                    }
                }
                if decl.ref_params.get(arg_idx).copied().unwrap_or(false) {
                    // The callee holds a reference to this local from here on, and it can
                    // escape, so the local is never kill/retype eligible in this body.
                    self.record_reference_alias_root(arg);
                    if !self.is_by_ref_argument_lvalue(arg, caller_env)? {
                        let param_name = decl
                            .params
                            .get(arg_idx)
                            .map(String::as_str)
                            .unwrap_or("arg");
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "Function '{}' parameter ${} must be passed a variable",
                                name, param_name
                            ),
                        ));
                    }
                }
                if let Some(type_ann) = decl.param_types.get(arg_idx).and_then(|t| t.as_ref()) {
                    let param_name = decl
                        .params
                        .get(arg_idx)
                        .map(String::as_str)
                        .unwrap_or("arg");
                    let declared_ty = self.resolve_declared_param_type_hint(
                        type_ann,
                        decl.span,
                        &format!("Function '{}' parameter ${}", name, param_name),
                    )?;
                    if decl.ref_params.get(arg_idx).copied().unwrap_or(false) {
                        self.require_boxed_by_ref_storage(
                            &declared_ty,
                            &ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, param_name),
                        )?;
                    }
                    self.require_bound_param_arg_type(
                        &declared_ty,
                        &ty,
                        arg,
                        caller_env,
                        &format!("Function '{}' parameter ${}", name, param_name),
                        Some((name, decl.params[arg_idx].as_str())),
                        decl.ref_params.get(arg_idx).copied().unwrap_or(false),
                    )?;
                    let specialized_ty =
                        Self::specialize_generic_array_param_hint(&declared_ty, &ty);
                    param_types.push((decl.params[arg_idx].clone(), specialized_ty));
                    arg_idx += 1;
                    continue;
                }
                param_types.push((decl.params[arg_idx].clone(), ty));
                arg_idx += 1;
            } else {
                // A by-REFERENCE variadic (`&...$xs`) binds every collected argument by
                // reference, so each names a local that is aliased for the rest of the body.
                // `decl.params` excludes the variadic, which is why this cannot be handled by
                // the regular-parameter branch above.
                if decl.variadic_by_ref {
                    self.record_reference_alias_root(arg);
                }
                // Argument collected into the variadic parameter: enforce its declared element
                // type (`int ...$xs`) against every passed argument, matching PHP.
                if let Some(declared) = &decl.variadic_type {
                    let vname = decl.variadic.as_deref().unwrap_or_default();
                    let elem_ty = self.resolve_declared_param_type_hint(
                        declared,
                        decl.span,
                        &format!("Function '{}' variadic parameter ${}", name, vname),
                    )?;
                    self.require_compatible_arg_type(
                        &elem_ty,
                        &ty,
                        arg.span,
                        &format!("Function '{}' variadic parameter ${}", name, vname),
                    )?;
                    // PHP applies `strict_types` to a variadic element exactly like a regular
                    // declared parameter, so the strict rejection runs here too; the coercive
                    // widenings `require_compatible_arg_type` allows are unchanged otherwise.
                    self.require_strict_types_param_binding(
                        &elem_ty,
                        &ty,
                        arg.span,
                        &format!("Function '{}' variadic parameter ${}", name, vname),
                    )?;
                }
                arg_idx += 1;
            }
        }

        for i in arg_idx..decl.params.len() {
            if let Some(default_expr) = &decl.defaults[i] {
                if let Some(type_ann) = decl.param_types.get(i).and_then(|t| t.as_ref()) {
                    let declared_ty = self.resolve_declared_param_type_hint(
                        type_ann,
                        decl.span,
                        &format!("Function '{}' parameter ${}", name, decl.params[i]),
                    )?;
                    let default_ty = self.infer_type(default_expr, caller_env)?;
                    self.require_compatible_arg_type(
                        &declared_ty,
                        &default_ty,
                        default_expr.span,
                        &format!("Function '{}' parameter ${}", name, decl.params[i]),
                    )?;
                    param_types.push((decl.params[i].clone(), declared_ty));
                } else {
                    let ty = self.infer_type(default_expr, caller_env)?;
                    param_types.push((decl.params[i].clone(), ty));
                }
            }
        }

        if let Some(ref vp) = decl.variadic {
            if decl.variadic_by_ref {
                param_types.push((vp.clone(), PhpType::Array(Box::new(PhpType::Mixed))));
            } else if let Some(declared) = &decl.variadic_type {
                // A declared element type (`int ...$xs`) constrains every collected argument;
                // it takes precedence over inference so call validation enforces the hint.
                let elem_ty = self.resolve_declared_param_type_hint(
                    declared,
                    decl.span,
                    &format!("Variadic parameter ${}", vp),
                )?;
                param_types.push((vp.clone(), PhpType::Array(Box::new(elem_ty))));
            } else if Self::has_unknown_named_variadic_arg(args, &decl.params) {
                param_types.push((vp.clone(), PhpType::Iterable));
            } else {
                let variadic_elem_ty = if args.len() > decl.params.len() {
                    self.infer_type(&args[decl.params.len()], caller_env)
                        .unwrap_or(PhpType::Int)
                } else {
                    PhpType::Int
                };
                let variadic_elem_ty = Self::variadic_container_elem_ty(variadic_elem_ty);
                param_types.push((vp.clone(), PhpType::Array(Box::new(variadic_elem_ty))));
            }
        }

        self.resolve_function_signature(name, &decl, param_types)
    }
}

/// Returns true when a call argument is an array whose elements are callable descriptors.
fn is_callable_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}
