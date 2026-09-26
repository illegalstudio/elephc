//! Purpose:
//! Turns a call to a generic function template into a call to one monomorphic instantiation.
//!
//! Called from:
//! - `crate::types::checker::functions::resolution::call::check_function_call`
//!
//! Key details:
//! - The instantiated declaration is an ORDINARY `FnDecl` under a synthetic name, so every
//!   existing checker path — signature resolution, argument validation, return inference,
//!   specialization — applies to it with no knowledge that generics exist.
//! - The call site is what supplies the type arguments, so the same template yields as many
//!   functions as there are distinct argument shapes, which is the whole point: each keeps the
//!   storage its own type arguments imply instead of collapsing to boxed `mixed`.

use crate::errors::CompileError;
use crate::generics::{self, InferError};
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    /// Returns the instantiated function name a call to generic `name` resolves to, creating
    /// the instantiation the first time it is requested.
    ///
    /// Inference runs against the argument types the call site actually has, so the bindings
    /// are exactly what the caller passed. A failure is reported against the CALL, not the
    /// declaration: the template is fine, it is this use of it that does not determine its
    /// type parameters.
    pub(crate) fn instantiate_generic_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<String, CompileError> {
        let decl = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;
        // Inference pairs a declared parameter with the argument at the SAME INDEX, so the call's
        // source order has to become the declaration's first: a named argument binds by name and
        // may be written anywhere. Positions no argument fills are dropped from all three vectors
        // together, which keeps that pairing intact while contributing nothing to inference.
        let ordered_args = generics::arguments_in_declaration_order(&decl.params, args);
        let mut declared_params: Vec<Option<crate::parser::ast::TypeExpr>> = Vec::new();
        let mut actual_types: Vec<PhpType> = Vec::new();
        let mut inference_args: Vec<Expr> = Vec::new();
        for (index, arg) in ordered_args.into_iter().enumerate() {
            let Some(arg) = arg else { continue };
            actual_types.push(self.infer_type(&arg, caller_env)?);
            declared_params.push(generics::declared_type_at(
                index,
                decl.params.len(),
                &decl.param_types,
                decl.variadic_type.as_ref(),
            ));
            inference_args.push(arg);
        }
        let bindings = generics::infer_bindings_with_args(
            &decl.type_params,
            &declared_params,
            &actual_types,
            &inference_args,
        )
        .map_err(|error| CompileError::new(span, &describe_infer_error(name, &error)))?;
        self.finish_generic_instantiation(name, &decl, bindings, span)
    }

    /// Returns the instantiated name for a call that WRITES its type arguments.
    ///
    /// `identity<int>(41)` says what inference would otherwise have to discover, and says it
    /// even where inference cannot: a type parameter no argument position mentions
    /// (`make<T>(): T`), or one the arguments determine differently from what the programmer
    /// wants (`identity<float>(1)`). The parser has already put the arguments in the name, so
    /// `name` here is the instantiated spelling and `written` its decoded arguments.
    ///
    /// Everything after the bindings is shared with the inferred path: same bound checking,
    /// same call-site record, same splice request, same substitution. Only where the bindings
    /// COME FROM differs.
    pub(crate) fn instantiate_written_generic_call(
        &mut self,
        base: &str,
        written: &[crate::parser::ast::TypeExpr],
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        let decl = self
            .fn_decls
            .get(base)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", base)))?;
        let bindings = generics::bindings_from_written_arguments(&decl.type_params, written)
            .map_err(|error| {
                CompileError::new(span, &generics::describe_written_error("function", base, &error))
            })?;
        self.finish_generic_instantiation(base, &decl, bindings, span)
    }

    /// Checks the bounds, records the call site, requests the splice, and substitutes the body.
    ///
    /// The half of instantiation that does not care where the bindings came from.
    fn finish_generic_instantiation(
        &mut self,
        name: &str,
        decl: &crate::types::checker::FnDecl,
        bindings: generics::Bindings,
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        // Bounds are checked HERE rather than inside inference, because satisfying a bound is a
        // subtyping question and only the checker holds the class table: `type_accepts` already
        // answers it for subclasses, implemented interfaces and interface inheritance. Inference
        // knows nothing about the class graph and could only compare spellings, which would let
        // an unrelated class through.
        for (param, bound) in decl
            .type_params
            .iter()
            .filter_map(|param| param.bound.as_ref().map(|bound| (param, bound)))
        {
            let Some((_, argument)) = bindings.iter().find(|(name, _)| name == &param.name) else {
                continue;
            };
            let bound_ty = self.resolve_type_expr(bound, span)?;
            let argument_ty = self.resolve_type_expr(argument, span)?;
            if !self.type_accepts(&bound_ty, &argument_ty) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Call to generic function '{}' binds type parameter <{}> to {}, which does not satisfy its bound {}",
                        name, param.name, argument_ty, bound_ty
                    ),
                ));
            }
        }
        let instantiated = generics::instantiated_name(name, &bindings);
        // The ENCLOSING FUNCTION is half the key. One source position inside a template is
        // reached once per instantiation of that template and legitimately resolves differently
        // each time — `twice<int>` and `twice<string>` both call `identity` at the same line and
        // column — so keying on the position alone made the second call invoke the first's
        // function.
        //
        // Top-level statements lower inside the synthetic `main` and the checker has no
        // enclosing function for them, so both sides have to agree on that one name or the
        // lookup misses and the call is never rewritten.
        let site = (
            self.current_function
                .clone()
                .unwrap_or_else(|| "main".to_string()),
            span,
        );
        // What the enclosing function cannot separate is two calls at the same position inside
        // same-named functions in two included files: a `Span` carries no file identity, and
        // include resolution splices without rebasing line numbers. Lowering reads this map by
        // that key, so a collision would make one call silently invoke the other's function
        // with arguments of the wrong type. It is a hard error instead — the same "no silent
        // wrong output" invariant `binding_decision_ambiguity` enforces for local bindings.
        //
        // The checker re-walks a body once per call-site specialization, so the same key being
        // re-recorded with the SAME answer is normal and must not be flagged.
        if let Some(previous) = self.generic_call_sites.get(&site) {
            if previous != &instantiated {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Two different generic instantiations, '{}' and '{}', resolve at the same \
                         position inside the same function. A source position carries no file \
                         identity, so the two calls cannot be told apart; move one of them to a \
                         different line",
                        previous, instantiated
                    ),
                ));
            }
        }
        self.generic_call_sites.insert(site, instantiated.clone());
        // Recorded on EVERY call, not just the first. The pipeline re-checks after splicing, and
        // on that pass the declaration already exists — skipping the request then left the final
        // result with an empty list, and the pruner dropped every instantiation as unreferenced
        // because no call site names one.
        self.requested_instantiations
            .push((name.to_string(), bindings.clone()));
        if self.fn_decls.contains_key(&instantiated) {
            return Ok(instantiated);
        }

        // Substituting the annotations is what makes the copy monomorphic. The BODY goes
        // through the same exhaustive walker the AST splice uses, so a type parameter named in
        // a typed local, a `buffer<T>`, or a nested closure's signature is substituted too —
        // and the signature this resolves describes exactly the function the backend emits.
        let mut instantiated_decl = decl.clone();
        instantiated_decl.body = generics::substitute_in_body(decl.body.clone(), &bindings);
        instantiated_decl.type_params = Vec::new();
        instantiated_decl.param_types = decl
            .param_types
            .iter()
            .map(|declared| {
                declared
                    .as_ref()
                    .map(|ty| ty.substitute_type_params(&bindings))
            })
            .collect();
        instantiated_decl.return_type = decl
            .return_type
            .as_ref()
            .map(|ty| ty.substitute_type_params(&bindings));
        instantiated_decl.variadic_type = decl
            .variadic_type
            .as_ref()
            .map(|ty| ty.substitute_type_params(&bindings));
        self.fn_decls
            .insert(instantiated.clone(), instantiated_decl);
        Ok(instantiated)
    }
}

/// Renders an inference failure as a diagnostic about the call that caused it.
fn describe_infer_error(name: &str, error: &InferError) -> String {
    match error {
        InferError::Unconstrained(param) => format!(
            "Call to generic function '{}' does not determine type parameter <{}>; no argument position mentions it",
            name, param
        ),
        InferError::Conflict {
            param,
            first,
            second,
        } => format!(
            "Call to generic function '{}' binds type parameter <{}> to both {} and {}",
            name, param, first, second
        ),
        InferError::Unspellable { param, ty } => format!(
            "Call to generic function '{}' would bind type parameter <{}> to {}, which has no generic spelling",
            name, param, ty
        ),
        InferError::TooDeep { param, ty } => format!(
            "Generic function '{}' instantiates itself at an ever-deeper type: <{}> reached {}. \
             A generic function whose own body calls it at a type built from its type parameter \
             has no finite set of instantiations",
            name, param, ty
        ),
    }
}
