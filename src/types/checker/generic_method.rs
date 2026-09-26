//! Purpose:
//! Resolves a call to a generic METHOD: infers its type arguments from the call's argument
//! types, checks the declared bounds, and records the instantiation for the next round to splice.
//!
//! Called from:
//! - `infer_method_call_on_class_type`, just before it would report `Undefined method`.
//!
//! Key details:
//! - A generic method has no signature of its own, so it is not in the class table at all and
//!   every ordinary lookup has already missed by the time this runs. That is the same shape as a
//!   generic function, whose call is redirected in `functions::resolution::call`.
//! - Bounds are checked HERE rather than inside inference, for the reason the function path
//!   documents: satisfying a bound is a subtyping question and only the checker holds the class
//!   table.

use crate::errors::CompileError;
use crate::generics;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

impl Checker {
    /// Finds a generic method template on `class_name`, or on the nearest ancestor declaring one.
    ///
    /// A generic method is INHERITED like any other, but its template is recorded under the class
    /// that declares it — there is no signature to copy into the subclass, because a template has
    /// no signature until a call site gives it type arguments. Asking only the receiver's own name
    /// made `class B extends A {}` report `Undefined method: B::id` for a method the object has.
    ///
    /// Returns the key it was found under rather than the one asked for: the instantiation is
    /// spliced into the DECLARING class, which every subclass then inherits exactly as it inherits
    /// an ordinary method.
    fn generic_method_template_for(
        &self,
        class_name: &str,
        method: &str,
    ) -> Option<((String, String), generics::methods::MethodTemplate)> {
        let mut current = Some(class_name.to_string());
        // A hierarchy the class table has not rejected can still be cyclic while this pass runs,
        // so the walk is bounded by the table itself rather than trusting it to terminate.
        let mut remaining = self.classes.len() + 1;
        while let Some(name) = current {
            let key = generics::methods::template_key(&name, method);
            if let Some(template) = self.method_templates.get(&key) {
                return Some((key, template.clone()));
            }
            remaining = remaining.checked_sub(1)?;
            current = self.classes.get(&name).and_then(|info| info.parent.clone());
        }
        None
    }

    /// Resolves `$object->method(args)` against a generic method template, if one exists.
    ///
    /// Answers `Ok(None)` when the class declares no such template, which leaves the caller to
    /// report its own `Undefined method` — this is one more lookup, never the whole answer.
    pub(crate) fn infer_generic_method_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        // `$box->map<string>($f)` writes its type arguments, and the parser put them in the
        // method name. The template is declared under the BASE name, so the lookup uses that;
        // everything after the bindings — bounds, the recorded site, the substituted return
        // type — is the same work the inferred form does.
        let written = generics::split_written_instantiation(method);
        let base = written
            .as_ref()
            .map(|(base, _)| base.as_str())
            .unwrap_or(method);
        let Some((key, template)) = self.generic_method_template_for(class_name, base) else {
            return Ok(None);
        };
        let bindings = match &written {
            Some((_, arguments)) => generics::bindings_from_written_arguments(
                &template.type_params,
                arguments,
            )
            .map_err(|error| {
                CompileError::new(
                    expr.span,
                    &generics::describe_written_error(
                        "method",
                        &format!("{}::{}", template.declared_class, template.declared_method),
                        &error,
                    ),
                )
            })?,
            None => {
                // Ordered against the declaration for the reason the function path documents: a
                // named argument binds by name, and inference pairs by index. The variadic's
                // declared element type joins the fixed positions past the declared ones.
                let ordered_args =
                    generics::arguments_in_declaration_order(&template.param_names, args);
                let mut declared_params: Vec<Option<crate::parser::ast::TypeExpr>> = Vec::new();
                let mut actual_types: Vec<PhpType> = Vec::new();
                let mut inference_args: Vec<Expr> = Vec::new();
                for (index, arg) in ordered_args.into_iter().enumerate() {
                    let Some(arg) = arg else { continue };
                    actual_types.push(self.infer_type(&arg, env)?);
                    declared_params.push(generics::declared_type_at(
                        index,
                        template.param_names.len(),
                        &template.params,
                        template.variadic_type.as_ref(),
                    ));
                    inference_args.push(arg);
                }
                generics::infer_bindings_with_args(
                    &template.type_params,
                    &declared_params,
                    &actual_types,
                    &inference_args,
                )
                .map_err(|error| {
                    CompileError::new(
                        expr.span,
                        &format!(
                            "Call to generic method '{}::{}' {}",
                            template.declared_class,
                            template.declared_method,
                            describe(&error)
                        ),
                    )
                })?
            }
        };
        for param in &template.type_params {
            let Some(bound) = param.bound.as_ref() else {
                continue;
            };
            let Some((_, argument)) = bindings.iter().find(|(name, _)| name == &param.name) else {
                continue;
            };
            let bound_ty = self.resolve_type_expr(bound, expr.span)?;
            let argument_ty = self.resolve_type_expr(argument, expr.span)?;
            if !self.type_accepts(&bound_ty, &argument_ty) {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Call to generic method '{}::{}' binds type parameter <{}> to {}, which \
                         does not satisfy its bound {}",
                        template.declared_class,
                        template.declared_method,
                        param.name,
                        argument_ty,
                        bound_ty
                    ),
                ));
            }
        }
        let instantiated =
            generics::instantiated_name(&template.declared_method, &bindings);
        self.requested_method_instantiations
            .push((key, bindings.clone()));
        // The enclosing function is half the key, and `main` is the name both sides give to a
        // top-level statement — the same contract `generic_new_sites` and `generic_call_sites`
        // document. A miss here is loud (`Undefined method`), never silent.
        let site = (
            self.current_function
                .clone()
                .unwrap_or_else(|| "main".to_string()),
            expr.span,
        );
        self.generic_method_sites
            .entry(site)
            .or_default()
            .insert(instantiated);
        // The instantiation does not exist yet, so nothing can be checked against it this round.
        // Answering with the SUBSTITUTED return type keeps the rest of this round meaningful;
        // the next round sees an ordinary method and checks the call properly.
        match &template.return_type {
            Some(ty) => {
                let substituted = ty.substitute_type_params(&bindings);
                Ok(Some(self.resolve_type_expr(&substituted, expr.span)?))
            }
            None => Ok(Some(PhpType::Mixed)),
        }
    }
}

/// Renders an inference failure in the words the call site can act on.
fn describe(error: &generics::InferError) -> String {
    match error {
        generics::InferError::Unconstrained(param) => format!(
            "does not determine type parameter <{}>; no parameter mentions it, so nothing at the \
             call can bind it — a bare `callable` carries no types",
            param
        ),
        other => format!("{:?}", other),
    }
}
