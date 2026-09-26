//! Purpose:
//! Infers a generic class's type arguments from the arguments a `new` supplies.
//!
//! Called from:
//! - `crate::types::checker::inference::objects::constructors::infer_new_object_type`
//!
//! Key details:
//! - `new Box<int>(5)` writes its type arguments and is resolved by `generics::classes`, a
//!   pure-syntax pass that runs before this checker exists. `new Box(5)` writes none, so the
//!   only thing that can determine `T` is the constructor's declared parameters matched against
//!   the ARGUMENT TYPES — and those are this checker's to know. That is the whole reason one
//!   half of generic class instantiation lives here and the other half does not.
//! - The inference is the same `generics::infer_bindings` a generic FUNCTION call uses, over
//!   the constructor's parameters instead of the function's. Two inference rules would drift.
//! - Nothing is emitted here. The resolved instantiation is RECORDED, and the next
//!   monomorphization round splices the class and rewrites this `new` to name it.

use crate::errors::{CompileError, CompileWarning};
use crate::generics;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::Checker;

impl Checker {
    /// Resolves `new <template>(args)` to its instantiated class, or `None` when the name is
    /// not a generic class template.
    ///
    /// Returning `None` is the ordinary path: every non-generic construction in every program
    /// takes it, at the cost of one lookup in a list that is empty unless the program declares
    /// a generic class.
    pub(crate) fn infer_generic_construction(
        &mut self,
        class_name: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        let Some(template) = self.matching_class_template(class_name) else {
            return Ok(None);
        };
        // Ordered against the declaration before pairing, for the reason every other call
        // surface documents: a named argument binds by NAME and may be written anywhere, while
        // inference pairs a declared parameter with the argument at the same index. Reading the
        // call's source order bound `T` from the wrong parameter, so `new Box(v: "abc", n: 1)`
        // asked for `Box<int>` and then refused its own argument.
        let ordered_args = generics::arguments_in_declaration_order(&template.constructor_param_names, args);
        let mut declared_params: Vec<Option<crate::parser::ast::TypeExpr>> = Vec::new();
        let mut actual_types: Vec<PhpType> = Vec::new();
        for (index, arg) in ordered_args.into_iter().enumerate() {
            let Some(arg) = arg else { continue };
            actual_types.push(self.infer_type(&arg, env)?);
            declared_params.push(generics::declared_type_at(
                index,
                template.constructor_param_names.len(),
                &template.constructor_params,
                template.constructor_variadic.as_ref(),
            ));
        }
        let bindings = generics::infer_bindings(
            &template.type_params,
            &declared_params,
            &actual_types,
        )
        .map_err(|error| {
            CompileError::new(
                expr.span,
                &describe_construction_error(&template.declared_name, &error),
            )
        })?;
        self.reject_uninformative_bindings(&template, &bindings, expr)?;
        self.verify_construction_bounds(&template, &bindings, expr)?;
        self.warn_on_inferred_mixed(&template, &bindings, expr);

        let instantiated = generics::instantiated_name(&template.declared_name, &bindings);
        self.requested_class_instantiations
            .push((template.key.clone(), bindings));
        // The enclosing function is half the key: one position inside `wrap<T>` is reached once
        // per instantiation of it and means a different class each time. Top-level statements
        // lower inside the synthetic `main` and the checker has no enclosing function for them,
        // so both sides have to agree on that one name or the lookup misses — the same contract
        // `instantiate_generic_call` documents for `generic_call_sites`.
        //
        // Still a SET, for what the key cannot separate: a `Span` has no file identity, so two
        // same-named functions in two included files collide, and that is an error rather than
        // a choice.
        let scope = self
            .current_function
            .clone()
            .unwrap_or_else(|| "main".to_string());
        self.generic_new_sites
            .entry((scope, expr.span))
            .or_default()
            .insert(instantiated.clone());
        Ok(Some(PhpType::Object(instantiated)))
    }

    /// Returns true when this class name is one THIS ROUND has asked to be instantiated.
    ///
    /// `new Box(5)` infers `Box<int>` and records the request, but the class is spliced by the
    /// NEXT monomorphization round — so within this one the name is legitimately absent from the
    /// class table. Reading a property off such an object reported `Undefined class: Box<int>`
    /// for a construction the checker had just resolved itself, while a method call on the same
    /// object was already tolerated.
    ///
    /// The same deferral `infer_generic_method_call` documents: nothing can be checked against an
    /// instantiation that does not exist yet, and the next round sees an ordinary class and
    /// checks it properly. Narrow on purpose — the name must carry type arguments AND have been
    /// produced by this checker — so no genuine undefined class is ever waved through.
    pub(crate) fn awaits_generic_instantiation(&self, class_name: &str) -> bool {
        class_name.contains('<')
            && self
                .generic_new_sites
                .values()
                .any(|names| names.contains(class_name))
    }

    /// Resolves `Box::of(5)` — a static call on a template — to the method's return type at the
    /// instantiation its arguments determine, or `None` when the receiver is not a template.
    ///
    /// The same inference `new Box(5)` uses, over the static method's declared parameters. It
    /// exists because PHP allows exactly one `__construct`: a codebase that offers two ways to
    /// build something does it with named static factories, so inference that fired only at
    /// `new` would miss where most construction actually happens.
    pub(crate) fn infer_generic_static_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        let Some(template) = self.matching_class_template(class_name) else {
            return Ok(None);
        };
        let Some(signature) = template
            .static_methods
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(method))
            .cloned()
        else {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "'{}' has no static method '{}'",
                    template.declared_name, method
                ),
            ));
        };
        // Ordered against the declaration before pairing, for the reason every other call
        // surface documents: a named argument binds by NAME and may be written anywhere, while
        // inference pairs a declared parameter with the argument at the same index. Reading the
        // call's source order bound `T` from the wrong parameter, so `new Box(v: "abc", n: 1)`
        // asked for `Box<int>` and then refused its own argument.
        let ordered_args = generics::arguments_in_declaration_order(&signature.param_names, args);
        let mut declared_params: Vec<Option<crate::parser::ast::TypeExpr>> = Vec::new();
        let mut actual_types: Vec<PhpType> = Vec::new();
        for (index, arg) in ordered_args.into_iter().enumerate() {
            let Some(arg) = arg else { continue };
            actual_types.push(self.infer_type(&arg, env)?);
            declared_params.push(generics::declared_type_at(
                index,
                signature.param_names.len(),
                &signature.params,
                signature.variadic_type.as_ref(),
            ));
        }
        let bindings =
            generics::infer_bindings(&template.type_params, &declared_params, &actual_types)
                .map_err(|error| {
                    CompileError::new(
                        expr.span,
                        &describe_static_call_error(
                            &template.declared_name,
                            &signature.name,
                            &error,
                        ),
                    )
                })?;
        self.reject_uninformative_bindings(&template, &bindings, expr)?;
        self.verify_construction_bounds(&template, &bindings, expr)?;
        self.warn_on_inferred_mixed(&template, &bindings, expr);

        let instantiated = generics::instantiated_name(&template.declared_name, &bindings);
        self.requested_class_instantiations
            .push((template.key.clone(), bindings.clone()));
        let scope = self
            .current_function
            .clone()
            .unwrap_or_else(|| "main".to_string());
        self.generic_new_sites
            .entry((scope, expr.span))
            .or_default()
            .insert(instantiated.clone());

        // The return type is written in the TEMPLATE's vocabulary (`Box<T>`), so it is
        // substituted before being resolved. A method with no declared return says nothing, and
        // `mixed` is the only honest answer for it.
        let Some(declared) = signature.return_type else {
            return Ok(Some(PhpType::Mixed));
        };
        let substituted = declared.substitute_type_params(&bindings);
        Ok(Some(self.resolve_type_expr(&substituted, expr.span)?))
    }

    /// Resolves `new Box<int>(…)` — a construction whose type arguments ARE written — to its
    /// instantiated class, or `None` when the name is not a template.
    ///
    /// Written arguments are normally rewritten by `generics::classes` long before this checker
    /// runs, so this looks redundant. It is not: `function mk<T>(T $v) { new Box<T>($v); }`
    /// leaves the construction alone, because `T` means nothing until a call binds it — and the
    /// checker then instantiates `mk<int>` ON THE FLY to type that call, walking a body whose
    /// `new Box<int>` no pass has rewritten yet. That walk happens a round before the AST copy
    /// is rewritten, so the answer has to be available here too.
    pub(crate) fn infer_written_generic_construction(
        &mut self,
        class_type: &crate::parser::ast::TypeExpr,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        let crate::parser::ast::TypeExpr::GenericClass { name, args: written } = class_type else {
            return Ok(None);
        };
        let Some(template) = self.matching_class_template(name.as_str()) else {
            return Ok(None);
        };
        if written.len() > template.type_params.len() {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "'{}' takes {} type argument(s) but {} were given",
                    template.declared_name,
                    template.type_params.len(),
                    written.len()
                ),
            ));
        }
        let mut bindings: generics::Bindings = Vec::with_capacity(template.type_params.len());
        for (index, param) in template.type_params.iter().enumerate() {
            let argument = match written.get(index) {
                Some(argument) => argument.clone(),
                None => match &param.default {
                    Some(default) => default.substitute_type_params(&bindings),
                    None => {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "'{}' needs a type argument for <{}>, which has no default",
                                template.declared_name, param.name
                            ),
                        ))
                    }
                },
            };
            bindings.push((param.name.clone(), argument));
        }
        // The arguments still have to type-check against the constructor, which the ordinary
        // path would have done; walking them here keeps their own diagnostics reachable.
        for arg in args {
            self.infer_type(arg, env)?;
        }
        self.verify_construction_bounds(&template, &bindings, expr)?;

        let instantiated = generics::instantiated_name(&template.declared_name, &bindings);
        self.requested_class_instantiations
            .push((template.key.clone(), bindings));
        Ok(Some(PhpType::Object(instantiated)))
    }

    /// Returns the template `class_name` names, matching the way PHP compares class names.
    fn matching_class_template(
        &self,
        class_name: &str,
    ) -> Option<generics::classes::TemplateSignature> {
        if self.class_templates.is_empty() {
            return None;
        }
        let key = generics::classes::template_key(class_name);
        self.class_templates
            .iter()
            .find(|template| template.key == key)
            .cloned()
    }

    /// Rejects an inference that landed on a type no declaration can be written in.
    ///
    /// `new Box(null)` binds `T` to the null type, and substituting it produces
    /// `private null $v` — reported, before this check existed, as
    /// `parameter $v cannot use type void`: a type the programmer never wrote, in a declaration
    /// they did not write either. `null` carries no information about what the container holds,
    /// so the honest answer is that inference cannot do this one.
    ///
    /// `never` is the same case from the other end: an argument of type `never` means the
    /// expression does not return, and nothing can be stored.
    fn reject_uninformative_bindings(
        &mut self,
        template: &generics::classes::TemplateSignature,
        bindings: &generics::Bindings,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        for (param, argument) in bindings {
            let uninformative = match argument {
                crate::parser::ast::TypeExpr::Void => "null",
                crate::parser::ast::TypeExpr::Never => "never",
                _ => continue,
            };
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "Constructing '{}' cannot determine <{}> from a {} argument, which says \
                     nothing about what it holds; write it: new {}<...>(...)",
                    template.declared_name, param, uninformative, template.declared_name
                ),
            ));
        }
        Ok(())
    }

    /// Rejects an inferred type argument that does not satisfy its parameter's declared bound.
    ///
    /// Checked HERE rather than handed to `verify_class_type_argument_bounds` with the written
    /// mentions: this binding was inferred during the walk, and reporting it at the `new` is
    /// what lets the message name the construction that produced it.
    fn verify_construction_bounds(
        &mut self,
        template: &generics::classes::TemplateSignature,
        bindings: &generics::Bindings,
        expr: &Expr,
    ) -> Result<(), CompileError> {
        for param in &template.type_params {
            let Some(bound) = &param.bound else { continue };
            let Some((_, argument)) = bindings.iter().find(|(name, _)| name == &param.name) else {
                continue;
            };
            let bound_ty = self.resolve_type_expr(bound, expr.span)?;
            let argument_ty = self.resolve_type_expr(argument, expr.span)?;
            if !self.type_accepts(&bound_ty, &argument_ty) {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Constructing '{}' binds type parameter <{}> to {}, which does not \
                         satisfy its bound {}",
                        template.declared_name, param.name, argument_ty, bound_ty
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Warns when inference lands on `mixed`, because that instantiation is erasure.
    ///
    /// `new Box($anything)` binds `T = mixed` and compiles — correctly, and at twice the
    /// allocation cost of every other instantiation, since a `mixed` field is a boxed cell.
    /// Silently giving up the storage the feature exists to pin is worth a word; writing
    /// `new Box<mixed>(…)` says the same thing on purpose and warns about nothing.
    fn warn_on_inferred_mixed(
        &mut self,
        template: &generics::classes::TemplateSignature,
        bindings: &generics::Bindings,
        expr: &Expr,
    ) {
        for (param, argument) in bindings {
            if !matches!(
                argument,
                crate::parser::ast::TypeExpr::Named(name) if name.as_str() == "mixed"
            ) {
                continue;
            }
            self.warnings.push(CompileWarning::new(
                expr.span,
                &format!(
                    "Constructing '{}' infers <{}> as mixed, so this instantiation is boxed; \
                     write the type argument to keep unboxed storage",
                    template.declared_name, param
                ),
            ));
        }
    }
}

/// Renders an inference failure at a static factory call.
fn describe_static_call_error(
    class_name: &str,
    method: &str,
    error: &generics::InferError,
) -> String {
    match error {
        generics::InferError::Unconstrained(param) => format!(
            "'{}::{}' does not determine type parameter <{}>; no parameter mentions it, so \
             write it: {}<...>::{}(...)",
            class_name, method, param, class_name, method
        ),
        generics::InferError::Conflict {
            param,
            first,
            second,
        } => format!(
            "'{}::{}' binds type parameter <{}> to both {} and {}; write the one you meant: \
             {}<...>::{}(...)",
            class_name, method, param, first, second, class_name, method
        ),
        generics::InferError::Unspellable { param, ty } => format!(
            "'{}::{}' would bind type parameter <{}> to {}, which has no generic spelling",
            class_name, method, param, ty
        ),
        generics::InferError::TooDeep { param, ty } => format!(
            "'{}::{}' instantiates its class at an ever-deeper type: <{}> reached {}",
            class_name, method, param, ty
        ),
    }
}

/// Renders an inference failure at a construction, in terms of what the programmer can write.
///
/// The function-call version of these messages points at the call's arguments, because that is
/// all a call has. A construction has a second way out — writing the type arguments — and every
/// message here names it, because it is the fix in each case.
fn describe_construction_error(class_name: &str, error: &generics::InferError) -> String {
    match error {
        generics::InferError::Unconstrained(param) => format!(
            "Constructing '{}' does not determine type parameter <{}>; no constructor parameter \
             mentions it, so write it: new {}<...>(...)",
            class_name, param, class_name
        ),
        generics::InferError::Conflict {
            param,
            first,
            second,
        } => format!(
            "Constructing '{}' binds type parameter <{}> to both {} and {}; write the one you \
             meant: new {}<...>(...)",
            class_name, param, first, second, class_name
        ),
        generics::InferError::Unspellable { param, ty } => format!(
            "Constructing '{}' would bind type parameter <{}> to {}, which has no generic \
             spelling; write a type that does: new {}<...>(...)",
            class_name, param, ty, class_name
        ),
        generics::InferError::TooDeep { param, ty } => format!(
            "Constructing '{}' instantiates itself at an ever-deeper type: <{}> reached {}. A \
             class whose own constructor builds it at a type made from its type parameter has \
             no finite set of instantiations",
            class_name, param, ty
        ),
    }
}
