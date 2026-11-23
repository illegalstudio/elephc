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
//! - A method whose type parameters all come from PHPDoc is NOT instantiated: it stays as written,
//!   and its annotated reading is a hidden twin template (`generics::methods::doc_twin_name`) that
//!   `apply_doc_method_overlay` binds at each plain call to type the result. The native result is
//!   narrowed at lowering (`Checker::doc_return_sites`).
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
            // A generic method a TRAIT declares is a template under the trait's name; the
            // instantiation is attached there and flattened into this class with the rest of the
            // trait. Without this, `class C { use T; }` reported `Undefined method: C::id`.
            let info = self.classes.get(&name);
            for used in info.map(|info| info.used_traits.as_slice()).unwrap_or_default() {
                let key = generics::methods::template_key(used, method);
                if let Some(template) = self.method_templates.get(&key) {
                    return Some((key, template.clone()));
                }
            }
            remaining = remaining.checked_sub(1)?;
            current = info.and_then(|info| info.parent.clone());
        }
        None
    }

    /// Checks every bound of `template` against the type arguments one call bound.
    ///
    /// A violated PHPDoc bound only warns (php never reads it); a native bound is an error.
    fn check_method_bounds(
        &mut self,
        template: &generics::methods::MethodTemplate,
        bindings: &generics::Bindings,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        for param in &template.type_params {
            let Some(bound) = param.bound.as_ref() else {
                continue;
            };
            let Some((_, argument)) = bindings.iter().find(|(name, _)| name == &param.name) else {
                continue;
            };
            if Self::skips_docblock_bound(param, argument) {
                continue;
            }
            // Substituted first, as for a generic function: `<U : Cmp<U>>` names `U` itself.
            let bound_ty =
                self.resolve_type_expr(&generics::substitute_value_type(bound, bindings), span)?;
            let argument_ty = self.resolve_generic_bound_argument(argument, span)?;
            if !self.type_accepts(&bound_ty, &argument_ty) {
                self.report_bound_violation(
                    param.from_docblock,
                    span,
                    format!(
                        "Call to generic method '{}::{}' binds type parameter <{}> to {}, which \
                         does not satisfy its bound {}",
                        template.declared_class,
                        // A PHPDoc twin is reported under the method's written name.
                        template
                            .declared_method
                            .strip_suffix(generics::DOC_TWIN_SUFFIX)
                            .unwrap_or(&template.declared_method),
                        param.name,
                        argument_ty,
                        bound_ty
                    ),
                )?;
            }
        }
        Ok(())
    }

    /// Types a call that wrote its type arguments on a PHPDoc method template (`$o->id<int>(7)`).
    ///
    /// The method itself is an ordinary one, so the site is renamed to its written name.
    fn infer_written_doc_method_call(
        &mut self,
        class_name: &str,
        template: &generics::methods::MethodTemplate,
        written_method: &str,
        written: Option<&(String, Vec<crate::parser::ast::TypeExpr>)>,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        let Some((_, arguments)) = written else {
            return Ok(None);
        };
        let bindings = generics::bindings_from_written_arguments(&template.type_params, arguments)
            .map_err(|error| {
                CompileError::new(
                    expr.span,
                    &generics::describe_written_error(
                        "method",
                        &format!("{}::{}", template.declared_class, written_method),
                        &error,
                    ),
                )
            })?;
        let mut bindings = bindings;
        self.settle_native_hints(&template.type_params, &mut bindings, expr.span)?;
        self.check_method_bounds(template, &bindings, expr.span)?;
        let site = (self.current_loop_storage_scope.clone(), expr.span);
        self.generic_method_sites
            .entry(site)
            .or_default()
            .insert(written_method.to_string());
        // The call is renamed to the written method and not checked again, so the arguments are
        // checked here against the method's own signature, and the call's type and narrowing are
        // decided exactly as a plain call's are (from the arguments written, not inferred).
        match &expr.kind {
            crate::parser::ast::ExprKind::StaticMethodCall { receiver, .. } => {
                self.infer_static_method_call_type(receiver, written_method, args, expr, env)?;
            }
            _ => {
                self.infer_method_call_on_class_type(class_name, written_method, args, expr, env)?;
            }
        }
        let method_key = crate::names::php_symbol_key(written_method);
        let native = self
            .classes
            .get(class_name.trim_start_matches('\\'))
            .and_then(|info| {
                info.methods
                    .get(&method_key)
                    .or_else(|| info.static_methods.get(&method_key))
            })
            .map(|sig| sig.return_type.clone())
            .unwrap_or(PhpType::Mixed);
        let site = (self.current_loop_storage_scope.clone(), expr.span);
        self.doc_return_sites.remove(&site);
        let Some(ty) = &template.return_type else {
            return Ok(Some(native));
        };
        let erased = generics::docblock_params_read_bare(&template.type_params, &bindings);
        let substituted = generics::substitute_signature_type(ty, &bindings, &erased);
        let Ok(precise) = self.resolve_type_expr(&substituted, expr.span) else {
            return Ok(Some(native));
        };
        Ok(Some(match self.doc_narrowing(&native, &precise) {
            Some(narrowed) => {
                self.doc_return_sites.insert(site, narrowed.clone());
                narrowed
            }
            None => native,
        }))
    }

    /// Finds the PHPDoc twin of the method `method` a call on `class_name` runs, if it has one.
    ///
    /// The twin is declared under the class that declares the method (or a trait it uses), and a
    /// subclass that redeclares the method hides it: only the declaring class is consulted, never
    /// an ancestor of it.
    fn doc_twin_template_for(
        &self,
        class_name: &str,
        method: &str,
        is_static: bool,
    ) -> Option<generics::methods::MethodTemplate> {
        let method_key = crate::names::php_symbol_key(method);
        let info = self.classes.get(class_name.trim_start_matches('\\'))?;
        let (declaring, implementing) = if is_static {
            (&info.static_method_declaring_classes, &info.static_method_impl_classes)
        } else {
            (&info.method_declaring_classes, &info.method_impl_classes)
        };
        let twin = generics::methods::doc_twin_name(method);
        let mut owners: Vec<&str> = Vec::new();
        for owner in [declaring.get(&method_key), implementing.get(&method_key)].into_iter().flatten() {
            if !owners.contains(&owner.as_str()) {
                owners.push(owner.as_str());
            }
        }
        for owner in owners {
            let key = generics::methods::template_key(owner, &twin);
            if let Some(template) = self.method_templates.get(&key) {
                return Some(template.clone());
            }
            let used = self.classes.get(owner).map(|owner| owner.used_traits.as_slice());
            for trait_name in used.unwrap_or_default() {
                let key = generics::methods::template_key(trait_name, &twin);
                if let Some(template) = self.method_templates.get(&key) {
                    return Some(template.clone());
                }
            }
        }
        None
    }

    /// Types the result of a call through the PHPDoc reading of the method it runs.
    ///
    /// `native` is what the written signature gives the call. When the method carries a PHPDoc
    /// twin, its type parameters are bound from the argument types exactly as a native template's
    /// are, and the substituted return type is the call's type; the call site is recorded so
    /// lowering narrows the native result to it behind a run-time guard (php enforces no
    /// annotation, so a wrong one must not misread storage). Nothing is instantiated and no
    /// further check runs. An annotation that cannot be honoured leaves `native`.
    pub(crate) fn apply_doc_method_overlay(
        &mut self,
        class_name: &str,
        method: &str,
        is_static: bool,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        native: PhpType,
    ) -> Result<PhpType, CompileError> {
        if !self.doc_overlays_present || generics::methods::is_doc_twin(method) {
            return Ok(native);
        }
        let site = (self.current_loop_storage_scope.clone(), expr.span);
        match self.doc_method_narrowing(class_name, method, is_static, args, expr, env, &native)? {
            Some(narrowed) => {
                self.doc_return_sites.insert(site, narrowed.clone());
                Ok(narrowed)
            }
            None => {
                // A site an inner route of the same call already narrowed to `native` stays.
                if self.doc_return_sites.get(&site) != Some(&native) {
                    self.doc_return_sites.remove(&site);
                }
                Ok(native)
            }
        }
    }

    /// The type one call narrows to through the PHPDoc twin of its method, if any.
    fn doc_method_narrowing(
        &mut self,
        class_name: &str,
        method: &str,
        is_static: bool,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
        native: &PhpType,
    ) -> Result<Option<PhpType>, CompileError> {
        let Some(template) = self.doc_twin_template_for(class_name, method, is_static) else {
            return Ok(None);
        };
        let Some(return_type) = template.return_type.clone() else {
            return Ok(None);
        };
        let Ok(mut bindings) = self.infer_method_bindings(&template, args, expr, env)? else {
            return Ok(None);
        };
        if self.settle_native_hints(&template.type_params, &mut bindings, expr.span).is_err() {
            return Ok(None);
        }
        self.check_method_bounds(&template, &bindings, expr.span)?;
        let erased = generics::docblock_params_read_bare(&template.type_params, &bindings);
        let substituted = generics::substitute_signature_type(&return_type, &bindings, &erased);
        let Ok(precise) = self.resolve_type_expr(&substituted, expr.span) else {
            return Ok(None);
        };
        Ok(self.doc_narrowing(native, &precise))
    }

    /// Returns the type a call of a PHPDoc method template may be narrowed to, when `precise`
    /// is one the native result can be guarded into.
    ///
    /// Only a scalar or one declared class: the native result is then a boxed `mixed` or an
    /// object, and lowering converts or guards it. Everything else (containers, unions,
    /// callables) has a storage the native body does not produce, so it keeps the native type.
    fn doc_narrowing(&self, native: &PhpType, precise: &PhpType) -> Option<PhpType> {
        if native == precise {
            return None;
        }
        let eligible = match precise {
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str => {
                native.codegen_repr() == PhpType::Mixed
            }
            PhpType::Object(class) => {
                let class = class.trim_start_matches('\\');
                !class.eq_ignore_ascii_case("object")
                    && !class.eq_ignore_ascii_case("closure")
                    && !crate::parser::ast::is_c_width_type_name(class)
                    && (self.classes.contains_key(class) || self.interfaces.contains_key(class))
                    && !self.extern_classes.contains_key(class)
                    && !self.packed_classes.contains_key(class)
                    && match native {
                        PhpType::Mixed => true,
                        PhpType::Object(_) => self.type_accepts(native, precise),
                        _ => false,
                    }
            }
            _ => false,
        };
        eligible.then(|| precise.clone())
    }

    /// Reads a class-name string handed to a `class-string<T>` parameter as the `Foo::class` it is.
    ///
    /// The front end folds `Foo::class` into the literal `"Foo"` before the check, so a literal
    /// that names a class the program declares binds `T` exactly as the constant would; any other
    /// argument stays as it is and binds nothing.
    fn class_string_argument(&self, declared: Option<&crate::parser::ast::TypeExpr>, arg: Expr) -> Expr {
        let is_class_string = matches!(
            declared,
            Some(crate::parser::ast::TypeExpr::Named(written))
                if crate::docblock::class_string_template(written.as_str()).is_some()
        );
        let crate::parser::ast::ExprKind::StringLiteral(text) = &arg.kind else {
            return arg;
        };
        let class = text.trim_start_matches('\\');
        let declared_class = self
            .classes
            .keys()
            .chain(self.interfaces.keys())
            .find(|name| name.eq_ignore_ascii_case(class));
        if !is_class_string || class.is_empty() {
            return arg;
        }
        let Some(declared_class) = declared_class else {
            return arg;
        };
        let receiver = crate::parser::ast::StaticReceiver::Named(crate::names::Name::from_parts(
            crate::names::NameKind::FullyQualified,
            declared_class.split('\\').map(str::to_string).collect(),
        ));
        Expr::new(crate::parser::ast::ExprKind::ClassConstant { receiver }, arg.span)
    }

    /// Infers the type arguments of `template` from the argument types of one call.
    ///
    /// The outer error is an argument that cannot be typed; the inner one is an inference
    /// failure, which a native template reports and a PHPDoc twin tolerates.
    fn infer_method_bindings(
        &mut self,
        template: &generics::methods::MethodTemplate,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<Result<generics::Bindings, generics::InferError>, CompileError> {
        let _ = expr;
        // Ordered against the declaration for the reason the function path documents: a named
        // argument binds by name, and inference pairs by index. The variadic's declared element
        // type joins the fixed positions past the declared ones.
        let ordered_args = generics::arguments_in_declaration_order(&template.param_names, args);
        let mut declared_params: Vec<Option<crate::parser::ast::TypeExpr>> = Vec::new();
        let mut actual_types: Vec<PhpType> = Vec::new();
        let mut inference_args: Vec<Expr> = Vec::new();
        for (index, arg) in ordered_args.into_iter().enumerate() {
            let Some(arg) = arg else { continue };
            actual_types.push(self.infer_type(&arg, env)?);
            let declared = generics::declared_type_at(
                index,
                template.param_names.len(),
                &template.params,
                template.variadic_type.as_ref(),
            );
            inference_args.push(self.class_string_argument(declared.as_ref(), arg));
            declared_params.push(declared);
        }
        let lexical_class = self.lexical_class_for_generics();
        Ok(generics::infer_bindings_with_args(
            &template.type_params,
            &declared_params,
            &actual_types,
            &inference_args,
            lexical_class
                .as_ref()
                .map(|(class, parent)| (class.as_str(), parent.as_deref())),
        ))
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
        // `id<int>(7)` on a PHPDoc method template names the method as written: the twin carries
        // the annotated reading, and the call is renamed to the real method (no instantiation).
        if written.is_some() {
            let twin = generics::methods::doc_twin_name(base);
            if let Some((_, template)) = self.generic_method_template_for(class_name, &twin) {
                return self.infer_written_doc_method_call(
                    class_name,
                    &template,
                    base,
                    written.as_ref(),
                    args,
                    expr,
                    env,
                );
            }
        }
        let Some((key, template)) = self.generic_method_template_for(class_name, base) else {
            return Ok(None);
        };
        let mut bindings = match &written {
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
            None => self
                .infer_method_bindings(&template, args, expr, env)?
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
                })?,
        };
        self.settle_native_hints(&template.type_params, &mut bindings, expr.span)?;
        self.check_method_bounds(&template, &bindings, expr.span)?;
        let instantiated =
            generics::instantiated_name(&template.declared_method, &bindings);
        self.requested_method_instantiations
            .push((key, bindings.clone()));
        // The enclosing body is half the key, spelled as `current_loop_storage_scope` — the same
        // contract `generic_new_sites` and `generic_call_sites` document. A miss here is loud
        // (`Undefined method`), never silent.
        let site = (self.current_loop_storage_scope.clone(), expr.span);
        self.generic_method_sites
            .entry(site)
            .or_default()
            .insert(instantiated);
        // The instantiation does not exist yet, so nothing can be checked against it this round.
        // Answering with the SUBSTITUTED return type keeps the rest of this round meaningful;
        // the next round sees an ordinary method and checks the call properly.
        match &template.return_type {
            Some(ty) => {
                let erased =
                    generics::docblock_params_read_as_mixed(&template.type_params, &bindings);
                let substituted = generics::substitute_signature_type(ty, &bindings, &erased);
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
