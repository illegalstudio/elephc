//! Purpose:
//! Validates function call validation semantics for the checker.
//! Keeps call diagnostics and return-flow analysis consistent with signatures and inferred expression types.
//!
//! Called from:
//! - `crate::types::checker::functions`
//!
//! Key details:
//! - Diagnostics should map shared planner errors back to source spans without duplicating call semantics.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::call_args::{self, CallArgPlanError};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

/// Maps a `CallArgPlanError` from the shared call-argument planner to a typed `CompileError`
/// with a human-readable message that references the callee description and parameter names.
fn call_arg_plan_error(
    sig: &FunctionSig,
    callee_desc: &str,
    err: CallArgPlanError,
) -> CompileError {
    match err {
        CallArgPlanError::UnknownNamed { span, name } => {
            CompileError::new(span, &format!("{} has no parameter ${}", callee_desc, name))
        }
        CallArgPlanError::Duplicate {
            span,
            param_idx,
            name,
        } => {
            let param_name = sig
                .params
                .get(param_idx)
                .map(|(name, _)| name.as_str())
                .unwrap_or(name.as_str());
            CompileError::new(
                span,
                &format!(
                    "{} parameter ${} is already assigned",
                    callee_desc, param_name
                ),
            )
        }
        CallArgPlanError::PositionalAfterNamed { span } => CompileError::new(
            span,
            &format!(
                "{} cannot use positional arguments after named arguments",
                callee_desc
            ),
        ),
        CallArgPlanError::PositionalAfterSpread { span } => CompileError::new(
            span,
            &format!(
                "{} cannot use positional arguments after spread arguments",
                callee_desc
            ),
        ),
        CallArgPlanError::PositionalAfterNamedUnpack { span } => CompileError::new(
            span,
            &format!(
                "{} cannot use positional argument after named argument during unpacking",
                callee_desc
            ),
        ),
        CallArgPlanError::SpreadAfterNamed { span } => {
            spread_after_named_error(span, callee_desc)
        }
        CallArgPlanError::MissingRequired { span, param_idx } => {
            let param_name = sig
                .params
                .get(param_idx)
                .map(|(name, _)| name.as_str())
                .unwrap_or("arg");
            CompileError::new(
                span,
                &format!("{} missing required parameter ${}", callee_desc, param_name),
            )
        }
    }
}

/// Builds the diagnostic for PHP's compile-time
/// "Cannot use argument unpacking after named arguments" fatal, prefixed with
/// the callee description used by the rest of the call diagnostics.
fn spread_after_named_error(span: crate::span::Span, callee_desc: &str) -> CompileError {
    CompileError::new(
        span,
        &format!(
            "{} cannot use argument unpacking after named arguments",
            callee_desc
        ),
    )
}

/// Returns a boolean vector indicating which argument positions contain assoc-spread sources
/// (arrays with string keys that map to named arguments after spread expansion).
fn assoc_spread_sources(args: &[Expr], env: &TypeEnv) -> Vec<bool> {
    call_args::expand_static_assoc_spread_args(args)
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::Spread(inner) => is_assoc_spread_source(inner, env),
            _ => false,
        })
        .collect()
}

/// Marks every non-static spread that reaches a descriptor invoker as a runtime key provider.
///
/// Unlike direct-call lowering, descriptor unpack preserves the source's actual int/string keys
/// and decides positional versus named binding while iterating. A statically indexed source can
/// still provide only positional keys, but treating an unresolved spread as positional during
/// checking would incorrectly report missing or duplicate parameters before that runtime walk.
fn descriptor_spread_sources(args: &[Expr]) -> Vec<bool> {
    call_args::expand_static_assoc_spread_args(args)
        .iter()
        .map(|arg| matches!(arg.kind, ExprKind::Spread(_)))
        .collect()
}

/// Returns true if the expression is or expands to an assoc-array at runtime,
/// which means spread arguments from it should be treated as named arguments.
fn is_assoc_spread_source(expr: &Expr, env: &TypeEnv) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => matches!(env.get(name), Some(PhpType::AssocArray { .. })),
        ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_) => true,
        _ => matches!(
            crate::types::checker::infer_expr_type_syntactic(expr),
            PhpType::AssocArray { .. }
        ),
    }
}

/// Maps shared signature-independent ordering rules for checker and parser call surfaces.
pub(crate) fn validate_unresolved_call_argument_order(args: &[Expr], callee_desc: &str) -> Result<(), CompileError> {
    elephc_chopper::syntax::call_args::validate_call_argument_order(args, callee_desc)
}

/// Strips the named/spread wrappers from a by-reference argument so its storage place is visible.
fn unwrap_ref_argument(mut arg: &Expr) -> &Expr {
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
        arg = value;
    }
    arg
}

impl Checker {
    /// Enforces PHP's syntactic argument ordering on a callee not resolvable at compile time
    /// (string callables, `new $class(...)`), where no signature is available to
    /// run the shared planner against.
    ///
    /// The rule itself stays in `crate::types::call_args`; this only maps its
    /// error onto the same diagnostic the planner-backed surfaces produce.
    pub(crate) fn require_valid_unresolved_call_argument_order(
        &self,
        args: &[Expr],
        callee_desc: &str,
    ) -> Result<(), CompileError> {
        validate_unresolved_call_argument_order(args, callee_desc)
    }

    /// Returns true when a by-reference argument is an object-property place whose declared
    /// parameter and property both carry object storage.
    ///
    /// The direct user method/constructor lowering rewrites such a place through a hidden
    /// boxed temporary and writes it back after the call. Scalar/array places stay rejected:
    /// their write-back needs a conversion the object-only rewrite does not provide.
    pub(crate) fn by_ref_property_place(
        &mut self,
        arg: &Expr,
        expected_ty: &PhpType,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        if !crate::types::param_binding::by_ref_property_writeback_supported(expected_ty) {
            return Ok(false);
        }
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } = &arg.kind {
            arg = value;
        }
        if !matches!(
            arg.kind,
            ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }
        ) {
            return Ok(false);
        }
        let property_ty = self.infer_type(arg, env)?;
        let compatible = crate::types::param_binding::by_ref_property_storage_compatible(expected_ty, &property_ty);
        if compatible && *expected_ty == PhpType::Mixed && property_ty == PhpType::Mixed {
            self.promote_live_mixed_property_argument(arg, env)?;
        }
        Ok(compatible)
    }

    /// Gives plain Mixed property arguments stable cell storage before EIR call lowering.
    fn promote_live_mixed_property_argument(&mut self, arg: &Expr, env: &TypeEnv) -> Result<(), CompileError> {
        let ExprKind::PropertyAccess { object, property } = &arg.kind else { return Ok(()); };
        let Some((class_name, true)) = self.ref_property_slot_addressability(object, property, env)? else {
            return Ok(());
        };
        let (access_class, index) = match crate::types::resolve_property_name(
            &self.classes, &class_name, property, self.current_class.as_deref(),
        ) {
            crate::types::PropertyNameResolution::ScopePrivate { scope, index } => (scope, index),
            crate::types::PropertyNameResolution::Visible => {
                let Some(index) = self.classes[&class_name].visible_property_index(property) else { return Ok(()); };
                (class_name, index)
            }
            _ => return Ok(()),
        };
        let info = &self.classes[&access_class];
        if info.properties.get(index).is_none_or(|(_, ty)| *ty != PhpType::Mixed)
            || info.is_internal || info.readonly_properties.contains(property)
            || info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
        { return Ok(()); }
        let mut owner = info.property_declaring_classes.get(property).cloned().unwrap_or(access_class);
        // Public/protected redeclarations reuse their ancestor's physical slot. Every view
        // of that storage must agree on indirection before inherited method bodies are emitted.
        for _ in 0..self.classes.len() {
            let Some(parent) = self.classes.get(&owner).and_then(|info| info.parent.clone()) else { break; };
            let Some(info) = self.classes.get(&parent) else { break; };
            if info.visible_property_index(property) != Some(index)
                || info.properties.get(index).is_none_or(|(_, ty)| *ty != PhpType::Mixed) { break; }
            if info.is_internal { return Ok(()); }
            owner = parent;
        }
        self.reference_property_promotions.insert((owner, property.clone()));
        Ok(())
    }

    /// Returns whether a direct property argument can bind a by-reference parameter.
    ///
    /// A `mixed`/union parameter receives a boxed temporary of any declared property type.
    /// A concretely typed parameter is accepted only when the property's declared type is
    /// compatible with it, so the temporary can carry the property's own representation and
    /// the callee's writes can be stored back.
    pub(crate) fn property_ref_argument_supported(
        &mut self,
        arg: &Expr,
        param_ty: &PhpType,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let arg = unwrap_ref_argument(arg);
        let property_arg = match &arg.kind {
            ExprKind::PropertyAccess { .. } => arg,
            ExprKind::ArrayAccess { array, .. } => {
                let Some(property_arg) = Self::root_property_access(array) else {
                    return Ok(true);
                };
                // A nested element binds the property's array slot. Only a property whose
                // inferred storage is a PHP array provides one; a declared `mixed`/union slot
                // has no element storage the lowering can address.
                let storage = self.infer_type(property_arg, env)?;
                if !storage.is_php_array()
                    && !matches!(
                        storage.codegen_repr(),
                        PhpType::Array(_) | PhpType::AssocArray { .. }
                    )
                {
                    return Ok(false);
                }
                property_arg
            }
            _ => return Ok(true),
        };
        let ExprKind::PropertyAccess { object, property } = &property_arg.kind else {
            return Ok(true);
        };
        let Some((class_name, addressable)) =
            self.ref_property_slot_addressability(object, property, env)?
        else {
            return Ok(false);
        };
        let Some(info) = self.classes.get(class_name.as_str()) else {
            return Ok(false);
        };
        if !addressable {
            return Ok(false);
        }
        let declared = info.visible_property_is_declared(property);
        let Some(property_ty) = info
            .visible_property(property)
            .map(|(_, (_, ty))| ty.codegen_repr())
        else {
            return Ok(false);
        };
        let param_ty = param_ty.codegen_repr();
        // A declared `mixed` property already has the boxed cell a `mixed`/union parameter
        // needs. Promoting it keeps the live-argument path the untyped-slot branch below
        // provides; union-typed storage stays on the copy-in/out path so its declared type is
        // still enforced on every write-back.
        let exact_property_ty = info.visible_property(property).map(|(_, (_, ty))| ty);
        if declared
            && matches!(arg.kind, ExprKind::PropertyAccess { .. })
            && matches!(param_ty, PhpType::Mixed | PhpType::Union(_))
            && exact_property_ty == Some(&PhpType::Mixed)
            && !info.is_internal
            && !info.readonly_properties.contains(property)
            && info.property_hooks.get(property).is_none_or(|hooks| !hooks.any())
        {
            self.reference_property_promotions
                .insert((class_name.clone(), property.clone()));
        }
        if declared
            && matches!(arg.kind, ExprKind::PropertyAccess { .. })
            && crate::types::param_binding::by_ref_property_storage_compatible(&param_ty, &property_ty)
            && param_ty.codegen_repr() == property_ty.codegen_repr()
            && !matches!(
                param_ty.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)
            )
            && !info.is_internal
            && !info.readonly_properties.contains(property)
            && info.property_hooks.get(property).is_none_or(|hooks| !hooks.any())
        {
            // The callee's writes are typed exactly like the property declaration, so sharing
            // the property's cell is safe and publishes them while the call is still running.
            self.reference_property_promotions
                .insert((class_name.clone(), property.clone()));
        }
        if !declared {
            if matches!(param_ty, PhpType::Mixed | PhpType::Union(_))
                && !info.is_internal
                && !info.readonly_properties.contains(property)
                && info.property_hooks.get(property).is_none_or(|hooks| !hooks.any())
            {
                self.reference_property_storage_widenings
                    .insert((class_name.clone(), property.clone()));
                // A direct property and an array ELEMENT both bind the property's cell rather
                // than using the boxed-temporary write-back, so the widened untyped slot must
                // become an owned reference property for `LoadPropRefCell` to resolve and for
                // the callee's writes to stay visible while the call is still running.
                self.reference_property_promotions
                    .insert((class_name.clone(), property.clone()));
                return Ok(true);
            }
            // An UNTYPED property is PHP's ordinary writable zval: a weak call coerces its
            // current value to the declared parameter, and a strict call raises the call-site
            // `TypeError`. Either way it is a legal PLACE, handled by the temporary
            // read/write-back path rather than refused as "not a variable".
            if matches!(arg.kind, ExprKind::PropertyAccess { .. })
                && crate::types::param_binding::by_ref_property_writeback_supported(&param_ty)
            {
                // A weak conversion can replace the slot's inferred initializer type with the
                // parameter type (and the callee can replace it again), so an untyped slot
                // reached this way needs boxed `Mixed` storage instead of its narrow default.
                if !info.is_internal
                    && !info.readonly_properties.contains(property)
                    && info.property_hooks.get(property).is_none_or(|hooks| !hooks.any())
                {
                    self.reference_property_storage_widenings
                        .insert((class_name.clone(), property.clone()));
                }
                return Ok(true);
            }
            return Ok(false);
        }
        Ok(matches!(param_ty, PhpType::Mixed | PhpType::Union(_))
            || Self::types_compatible(&param_ty, &property_ty)
            || self.type_accepts(&param_ty, &property_ty)
            // A declared property whose current type the parameter cannot accept is still a
            // legal PLACE: PHP binds the reference and raises its argument `TypeError` at the
            // call boundary. `require_bound_param_arg_type` records that exact message, which
            // lowering raises before the callee body runs.
            || (crate::types::param_binding::by_ref_property_writeback_supported(&param_ty)
                && crate::types::param_binding::by_ref_property_writeback_supported(&property_ty)))
    }

    /// Widens an untyped property that a BUILTIN receives by reference to boxed `Mixed` storage.
    ///
    /// Builtins bypass `property_ref_argument_supported`; `settype($obj->prop, ...)` is the
    /// common case. Only a `mixed`/union parameter is eligible, and only an untyped slot: a
    /// declared slot has its own storage contract.
    pub(crate) fn widen_untyped_builtin_ref_property(
        &mut self,
        arg: &Expr,
        param_ty: &PhpType,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let arg = unwrap_ref_argument(arg);
        let ExprKind::PropertyAccess { object, property } = &arg.kind else {
            return Ok(());
        };
        if !matches!(
            param_ty.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        ) {
            return Ok(());
        }
        // A native array property is already served by the dedicated array-place path, which
        // promotes the slot to a reference property without changing its array layout. Widening
        // it to boxed Mixed here would make the array mutator lowering reject the receiver.
        let inferred = self.infer_type(arg, env)?;
        if matches!(
            inferred.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ) || inferred.is_php_array()
        {
            return Ok(());
        }
        let Some((class_name, addressable)) =
            self.ref_property_slot_addressability(object, property, env)?
        else {
            return Ok(());
        };
        let Some(info) = self.classes.get(class_name.as_str()) else {
            return Ok(());
        };
        if addressable && !info.visible_property_is_declared(property) {
            self.reference_property_storage_widenings
                .insert((class_name, property.clone()));
        }
        Ok(())
    }

    /// Returns the property access at the root of a possibly nested array-element expression.
    fn root_property_access<'a>(expr: &'a Expr) -> Option<&'a Expr> {
        match &expr.kind {
            ExprKind::PropertyAccess { .. } => Some(expr),
            ExprKind::ArrayAccess { array, .. } => Self::root_property_access(array),
            _ => None,
        }
    }

    /// Returns whether a direct property is an addressable place WITHOUT promoting it to a
    /// reference property: the boxed-temporary write-back path needs the property's typed slot,
    /// not a reference cell.
    pub(crate) fn is_addressable_direct_property_place(
        &mut self,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let arg = unwrap_ref_argument(arg);
        let ExprKind::PropertyAccess { object, property } = &arg.kind else {
            return Ok(false);
        };
        if !self.is_stable_ref_object_receiver(object, env)? {
            return Ok(false);
        }
        Ok(self
            .ref_property_slot_addressability(object, property, env)?
            .is_some_and(|(_, addressable)| addressable))
    }

    /// Returns true when an argument expression is an l-value supported by by-reference calls.
    pub(crate) fn is_by_ref_argument_lvalue(
        &mut self,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        if self.is_native_selection_reference_argument(arg, env)? {
            return Ok(true);
        }
        match &arg.kind {
            ExprKind::Variable(name) if name == "GLOBALS" => Ok(false),
            ExprKind::Variable(_) => Ok(true),
            // `call_user_func_array('f', [&$x])` expands the literal entry into a direct
            // by-reference argument; the marker's inner expression is the caller's place.
            ExprKind::Reference(inner) => self.is_by_ref_argument_lvalue(inner, env),
            ExprKind::ArrayAccess { array, .. } => {
                self.is_addressable_ref_array_receiver(array, env)
            }
            _ => Ok(false),
        }
    }

    /// Recognizes XML selections before ordinary property/array reference-place validation.
    pub(crate) fn is_native_selection_reference_argument(
        &mut self, arg: &Expr, env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        if let ExprKind::NamedArg { value, .. } = &arg.kind {
            return self.is_native_selection_reference_argument(value, env);
        }
        let Some(receiver) = crate::types::param_binding::native_selection_reference_receiver(arg) else {
            return Ok(false);
        };
        let ty = self.infer_type(receiver, env)?;
        if crate::types::param_binding::native_selection_reference_receiver_type(&ty, &self.classes) {
            self.record_member_receiver_type(arg.span, &ty);
            return Ok(true);
        }
        self.is_native_selection_reference_argument(receiver, env)
    }

    /// Validates one source value collected by a user-visible by-reference variadic.
    ///
    /// The collector transports reference markers in `array<mixed>`, so even an untyped
    /// declaration needs canonical boxed caller storage. Eligibility is captured before the
    /// alias is recorded, then the successful call publishes the widened local type to later
    /// expressions. Named variadic entries retain their source key in the planner, but bind the
    /// value expression underneath that wrapper.
    pub(crate) fn validate_by_ref_variadic_argument(
        &mut self,
        arg: &Expr,
        actual_ty: &PhpType,
        caller_env: &TypeEnv,
        call_span: crate::span::Span,
        callee_desc: &str,
        variadic_name: &str,
        descriptor_invocation: bool,
    ) -> Result<(), CompileError> {
        let can_widen_local = self.by_ref_argument_can_widen_local_to_mixed(arg)
            || self.boxed_reference_promotion_pending(arg, call_span);
        let storage_arg = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        };
        if self.is_native_selection_reference_argument(storage_arg, caller_env)? {
            return Ok(());
        }
        if matches!(storage_arg.kind, ExprKind::ArrayAccess { .. }) {
            let detail = if descriptor_invocation {
                "through callable descriptor dispatch"
            } else {
                "for a by-reference variadic call"
            };
            return Err(CompileError::new(
                storage_arg.span,
                &format!(
                    "{} variadic parameter ${} cannot bind an array element by reference {}; bind the element to a reference variable first",
                    callee_desc, variadic_name, detail
                ),
            ));
        }
        if self.by_ref_property_place(storage_arg, &PhpType::Mixed, caller_env)? {
            // The property rewrite stages the place at its source position and writes it back
            // after the collector runs. That works for array properties too: the collector
            // transports the boxed container through the reference marker, and the write-back
            // repacks the mutated payload into the declared property slot.
            self.record_reference_alias_root(arg);
            return Ok(());
        }
        if !self.is_by_ref_argument_lvalue(storage_arg, caller_env)? {
            return Err(CompileError::new(
                storage_arg.span,
                &format!(
                    "{} variadic parameter ${} must be passed a variable",
                    callee_desc, variadic_name
                ),
            ));
        }
        self.require_boxed_by_ref_storage(
            &PhpType::Mixed,
            actual_ty,
            arg,
            caller_env,
            can_widen_local,
            false,
            &format!("{} variadic parameter ${}", callee_desc, variadic_name),
        )?;
        self.record_boxed_reference_output(
            arg,
            &PhpType::Mixed,
            actual_ty,
            call_span,
            caller_env,
            false,
        )?;
        self.record_reference_alias_root(arg);
        Ok(())
    }

    /// Mirrors the recursive receiver shapes prepared by EIR reference-argument lowering.
    fn is_addressable_ref_array_receiver(
        &mut self,
        receiver: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let inferred_ty = self.infer_type(receiver, env)?;
        let receiver_ty = inferred_ty.codegen_repr();
        if matches!(&receiver_ty, PhpType::Object(class) if class.eq_ignore_ascii_case("WeakMap")) {
            return Ok(true);
        }
        let array_like = matches!(
            &receiver_ty,
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)
        );
        if !array_like {
            return Ok(false);
        }
        // A declared `array` property carries the canonical PHP-array union, whose codegen
        // repr collapses to `Mixed`; ask the inferred type so both spellings of array storage
        // reach the property path instead of only the single-layout ones.
        let holds_array_storage = inferred_ty.is_php_array()
            || matches!(&receiver_ty, PhpType::Array(_) | PhpType::AssocArray { .. });
        match &receiver.kind {
            ExprKind::Variable(_) | ExprKind::StaticPropertyAccess { .. } => Ok(true),
            ExprKind::ArrayAccess { array, .. } => {
                self.is_addressable_ref_array_receiver(array, env)
            }
            ExprKind::PropertyAccess { object, property } if holds_array_storage => {
                self.is_addressable_ref_property(object, property, env)
            }
            _ => Ok(false),
        }
    }

    /// Returns whether a named property is a declared slot under a stable object root.
    fn is_addressable_ref_property(
        &mut self,
        object: &Expr,
        property: &str,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        if !self.is_stable_ref_object_receiver(object, env)? {
            return Ok(false);
        }
        let Some((class_name, addressable)) = self.ref_property_slot_addressability(
            object,
            property,
            env,
        )? else {
            return Ok(false);
        };
        if addressable {
            self.reference_property_promotions
                .insert((class_name, property.to_string()));
        }
        Ok(addressable)
    }

    /// Resolves a stable declared property without changing its storage representation.
    pub(super) fn ref_property_slot_addressability(
        &mut self,
        object: &Expr,
        property: &str,
        env: &TypeEnv,
    ) -> Result<Option<(String, bool)>, CompileError> {
        let object_ty = self.infer_type(object, env)?;
        let Some(class_name) = crate::types::checker::single_object_class_name(&object_ty) else {
            return Ok(None);
        };
        let class_name = class_name.trim_start_matches('\\');
        let addressable = self.classes.get(class_name).is_some_and(|info| {
            !info.methods.contains_key(&crate::names::php_symbol_key(
                &crate::names::property_hook_get_method(property),
            )) && matches!(
                crate::types::resolve_property_name(
                    &self.classes,
                    class_name,
                    property,
                    self.current_class.as_deref(),
                ),
                crate::types::PropertyNameResolution::Visible
                    | crate::types::PropertyNameResolution::ScopePrivate { .. }
            )
        });
        Ok(Some((class_name.to_string(), addressable)))
    }

    /// Accepts only object roots whose declared-slot chain remains writable after evaluation.
    fn is_stable_ref_object_receiver(
        &mut self,
        object: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        match &object.kind {
            ExprKind::Variable(_) | ExprKind::This => Ok(true),
            ExprKind::PropertyAccess {
                object: parent,
                property,
            } => {
                if !self.is_stable_ref_object_receiver(parent, env)? {
                    return Ok(false);
                }
                Ok(self
                    .ref_property_slot_addressability(parent, property, env)?
                    .is_some_and(|(_, addressable)| addressable))
            }
            _ => Ok(false),
        }
    }

    /// Returns whether an argument can be bound to a BUILTIN's by-reference parameter.
    ///
    /// Deliberately separate from `is_by_ref_argument_lvalue`, which answers the same question
    /// for a USER function through the addressable-origin path. That path accepts properties and
    /// static properties only as stable receivers inside recursively addressable array access,
    /// and excludes direct scalar property binding plus dynamic/nullsafe member fetches.
    ///
    /// Builtins reach their by-reference argument through the storage itself, which is why
    /// `array_push($this->items, 9)` already compiles and runs today. What PHP refuses, and
    /// what this rejects, is an argument with NO storage to write back to: a literal, an array
    /// literal, or a call result.
    pub(crate) fn is_builtin_by_ref_argument_lvalue(&self, arg: &Expr) -> bool {
        matches!(
            arg.kind,
            ExprKind::Variable(_)
                | ExprKind::ArrayAccess { .. }
                | ExprKind::PropertyAccess { .. }
                | ExprKind::DynamicPropertyAccess { .. }
                | ExprKind::NullsafePropertyAccess { .. }
                | ExprKind::NullsafeDynamicPropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
        )
    }

    /// Normalizes arguments for a user-defined function call, allowing unknown named arguments
    /// to be collected into the variadic parameter.
    ///
    /// The one exception is the hidden variadic added by `crate::func_args` to collect the
    /// surplus positional arguments `func_get_args()` exposes: the callee declares no
    /// variadic of its own, so PHP still rejects an unknown named argument there.
    pub(crate) fn normalize_named_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
        env: &TypeEnv,
    ) -> Result<Vec<Expr>, CompileError> {
        Ok(self.plan_named_call_args(sig, args, span, callee_desc, env)?.normalized_args())
    }

    /// Retains default-slot provenance while applying user-call named and spread argument rules.
    pub(crate) fn plan_named_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
        env: &TypeEnv,
    ) -> Result<call_args::CallArgPlan, CompileError> {
        let allow_unknown_named_variadic =
            sig.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(sig);
        self.plan_call_args(
            sig,
            args,
            span,
            callee_desc,
            false,
            allow_unknown_named_variadic,
            env,
        )
    }

    /// Plans a descriptor invocation without guessing the keys of a dynamic unpack source.
    ///
    /// The runtime descriptor binder validates key types, order, aliases, duplicates and arity.
    /// The checker still expands fully static associative spreads and retains PHP's syntactic
    /// ordering errors, but leaves every remaining spread as a possible named-key provider.
    pub(crate) fn plan_descriptor_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
    ) -> Result<call_args::CallArgPlan, CompileError> {
        let allow_unknown_named_variadic =
            sig.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(sig);
        let descriptor_spread_sources = descriptor_spread_sources(args);
        call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
            sig,
            args,
            span,
            call_args::regular_param_count(sig),
            false,
            allow_unknown_named_variadic,
            &descriptor_spread_sources,
        )
        .map_err(|err| call_arg_plan_error(sig, callee_desc, err))
    }

    /// Plans builtin arguments while retaining which parameter slots came from caller source.
    pub(crate) fn plan_builtin_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
        env: &TypeEnv,
    ) -> Result<call_args::CallArgPlan, CompileError> {
        self.plan_call_args(sig, args, span, callee_desc, true, false, env)
    }

    /// Produces the shared planner result and maps semantic planning errors to checker diagnostics.
    fn plan_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
        trim_trailing_defaults: bool,
        allow_unknown_named_variadic: bool,
        env: &TypeEnv,
    ) -> Result<call_args::CallArgPlan, CompileError> {
        let assoc_spread_sources = assoc_spread_sources(args, env);
        call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
            sig,
            args,
            span,
            call_args::regular_param_count(sig),
            trim_trailing_defaults,
            allow_unknown_named_variadic,
            &assoc_spread_sources,
        )
        .map_err(|err| call_arg_plan_error(sig, callee_desc, err))
    }

    /// Returns true if `expected` and `actual` are compatible according to PHP assignment
    /// coercion rules (e.g., int is compatible with float, float is compatible with int/bool/void,
    /// `Mixed` is compatible with everything, `Never` is compatible with everything).
    pub(crate) fn types_compatible(expected: &PhpType, actual: &PhpType) -> bool {
        if expected == actual {
            return true;
        }
        // `int|callable` and other unions with a coercible scalar keep the descriptor-typed
        // callable member: only a value-preserving `callable` accepts any string or array.
        if let PhpType::Union(members) = expected {
            if PhpType::callable_union_needs_descriptor(members) {
                let members = members
                    .iter()
                    .map(|member| match member {
                        PhpType::DeclaredCallable => PhpType::Callable,
                        other => other.clone(),
                    })
                    .collect();
                return Self::types_compatible(&PhpType::Union(members), actual);
            }
        }
        match (expected, actual) {
            (PhpType::Mixed, _) => true,
            (_, PhpType::Never) => true, // never is the bottom type — compatible with any expected type
            (PhpType::Bool, PhpType::False) => true,
            // The backend has explicit boxed-Mixed cast funnels for scalar boundaries.
            // Object parameters use the narrower `ParamBinding::ObjectGuard` route below;
            // accepting them here would also admit unchecked Mixed function returns.
            (PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str, PhpType::Mixed) => true,
            (PhpType::Iterable, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable) => true,
            (PhpType::Union(expected_members), PhpType::Union(actual_members)) => actual_members
                .iter()
                .all(|actual_member| {
                    expected_members.iter().any(|expected_member| {
                        Self::types_compatible(expected_member, actual_member)
                    })
                }),
            (PhpType::Union(members), _) => members
                .iter()
                .any(|member| Self::types_compatible(member, actual)),
            (
                PhpType::AssocArray { key, value },
                PhpType::Array(_) | PhpType::AssocArray { .. },
            ) if **key == PhpType::Mixed && **value == PhpType::Mixed => true,
            (PhpType::Float, PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void) => true,
            (PhpType::Int, PhpType::Bool | PhpType::False | PhpType::Void) => true,
            (PhpType::Bool, PhpType::Int | PhpType::Void) => true,
            (PhpType::Pointer(_), PhpType::Pointer(_) | PhpType::Void) => true,
            // `buffer<mixed>` is the internal any-buffer parameter of the `buffer_len(...)` and
            // `buffer_ptr(...)` callable wrappers, which take the raw handle of every buffer; a
            // program cannot declare it, so it never widens a user signature.
            (PhpType::Buffer(expected_elem), PhpType::Buffer(_))
                if **expected_elem == PhpType::Mixed =>
            {
                true
            }
            (PhpType::Resource(_), PhpType::Resource(_)) => {
                PhpType::resource_types_compatible(expected, actual)
            }
            (PhpType::Callable, PhpType::Callable) => true,
            // A declared `callable` keeps the value PHP passed: a Closure, a function or
            // `Class::method` string, a callable array, or an invokable object. Whether it is
            // actually callable is PHP's runtime check; literal names are still resolved where
            // the binding happens (`require_declared_callable_arg`).
            (
                PhpType::DeclaredCallable,
                PhpType::Callable
                | PhpType::Mixed
                | PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_),
            ) => true,
            // A Closure slot materializes a descriptor from the declared value when it is used.
            (PhpType::Callable, PhpType::DeclaredCallable) => true,
            _ => false,
        }
    }

    /// Returns `Ok(())` if `actual` is compatible with `expected` (via `types_compatible` or
    /// `type_accepts`), otherwise returns a `CompileError` describing the type mismatch.
    pub(crate) fn require_compatible_arg_type(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if Self::types_compatible(expected, actual) || self.type_accepts(expected, actual) {
            Ok(())
        } else {
            let mut message = format!("{} expects {}, got {}", context, expected, actual);
            if let Some(hint) = self.variance_refusal_hint(expected, actual) {
                message.push_str(" — ");
                message.push_str(&hint);
            }
            Err(CompileError::new(span, &message))
        }
    }

    /// Formats a parameter-count range as a human-readable string, e.g. `3` or `2 to 5`.
    pub(crate) fn format_fixed_or_range_arity(min_args: usize, max_args: usize) -> String {
        if min_args == max_args {
            format!("{}", min_args)
        } else {
            format!("{} to {}", min_args, max_args)
        }
    }

    /// Validates and type-checks a call to a known callable (user function or builtin with
    /// a known signature): arity constraints, named/spread arguments, ref-param requirements,
    /// and argument-type compatibility. Returns the signature's return type on success.
    pub(crate) fn check_known_callable_call(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<PhpType, CompileError> {
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            false,
            false,
            false,
            false,
            false,
        )
    }

    /// Validates a direct call to a method or constructor of `owner_class`, applying PHP's
    /// coercive parameter binding when that class is declared in user source.
    ///
    /// Only surfaces whose arguments reach EIR through `lower_args_with_signature` may opt in:
    /// that is where the matching argument rewrite runs, and accepting a binding without it
    /// would hand raw storage to a differently typed parameter slot. Compiler-injected classes
    /// (SPL, `Exception`, reflection, …) lower several of their members through bespoke
    /// emitters instead, so they stay on the strict path.
    pub(crate) fn check_user_declared_call(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
        owner_class: &str,
    ) -> Result<PhpType, CompileError> {
        let coercive = self.class_is_user_declared(owner_class);
        let tidy_sig = self.prelude_strict_adapter_signature(None, Some(owner_class), Some(callee_desc), sig);
        let internal_sig = self.internal_operand_binding_signature(owner_class, sig);
        let sig = tidy_sig.as_ref().or(internal_sig.as_ref()).unwrap_or(sig);
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            false,
            coercive,
            false,
            true,
            false,
        )
    }

    /// `check_user_declared_call` for a callee that also accepts spread arguments into
    /// by-reference parameters materialized by descriptor invokers.
    pub(crate) fn check_user_declared_call_allowing_by_ref_spread(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
        owner_class: &str,
    ) -> Result<PhpType, CompileError> {
        let coercive = self.class_is_user_declared(owner_class);
        let tidy_sig = self.prelude_strict_adapter_signature(None, Some(owner_class), Some(callee_desc), sig);
        let internal_sig = self.internal_operand_binding_signature(owner_class, sig);
        let sig = tidy_sig.as_ref().or(internal_sig.as_ref()).unwrap_or(sig);
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            true,
            coercive,
            true,
            true,
            false,
        )
    }

    /// Accepts weak float inputs whose internal Number parameter adapter performs the binding.
    /// The declaration and its boxed union ABI remain authoritative outside this call check.
    fn internal_operand_binding_signature(&self, owner: &str, sig: &FunctionSig) -> Option<FunctionSig> {
        if self.strict_types || !owner.trim_start_matches('\\').eq_ignore_ascii_case("BcMath\\Number") {
            return None;
        }
        let mut accepted = sig.clone();
        for (_, ty) in &mut accepted.params {
            if let PhpType::Union(members) = ty {
                if members.contains(&PhpType::Str) && members.contains(&PhpType::Int) {
                    members.push(PhpType::Float);
                }
            }
        }
        Some(accepted)
    }

    /// Returns true when `class_name` is a class-like symbol declared in user source.
    ///
    /// Compiler-injected classes carry `Span::dummy()` as their declaration span, which is the
    /// only marker distinguishing them from user declarations. An unknown name is treated as
    /// not user-declared so an unresolved receiver never silently gains coercive binding.
    fn class_is_user_declared(&self, class_name: &str) -> bool {
        self.classes
            .get(class_name)
            .is_some_and(|info| info.declaration_span != crate::span::Span::dummy())
    }

    /// Validates a known callable call while allowing spread arguments for by-reference
    /// parameters that will be materialized by descriptor invokers at runtime.
    pub(crate) fn check_known_callable_call_allowing_by_ref_spread(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<PhpType, CompileError> {
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            true,
            false,
            true,
            false,
            false,
        )
    }

    /// Validates a `call_user_func*` callback call, which passes every argument by value.
    ///
    /// The callback signature may declare by-reference parameters, but PHP's helpers never bind
    /// caller storage: each one receives a temporary copy and the engine warns. Skipping the
    /// lvalue and boxed-storage rules here mirrors that runtime contract.
    pub(crate) fn check_known_callable_call_by_value(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<PhpType, CompileError> {
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            true,
            false,
            true,
            false,
            true,
        )
    }

    /// Shared implementation for known callable call validation.
    ///
    /// `coercive_param_binding` opts the callee into PHP's coercive parameter binding for its
    /// declared parameters; see `check_user_declared_call` for when that is sound.
    /// `descriptor_invocation` selects the descriptor binder as one indivisible contract:
    /// Traversable sources, runtime key planning and by-value Mixed projections are enabled
    /// together, so no caller can accidentally request only one part of that representation.
    fn check_known_callable_call_with_options(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
        allow_by_ref_spread: bool,
        coercive_param_binding: bool,
        descriptor_invocation: bool,
        allow_property_ref_places: bool,
        by_value_reference_params: bool,
    ) -> Result<PhpType, CompileError> {
        // Before planning: the plan rewrites a literal unpack into spread-element projections
        // that the binding loop below leaves to runtime.
        self.require_strict_types_static_spread_bindings(sig, args, span, caller_env, callee_desc)?;
        let plan = if descriptor_invocation {
            self.plan_descriptor_call_args(sig, args, span, callee_desc)?
        } else {
            self.plan_named_call_args(sig, args, span, callee_desc, caller_env)?
        };
        self.validate_callable_spread_elements(sig, args, &plan, caller_env, callee_desc)?;
        let source_has_spread = plan.has_spread_args();
        let supports_literal_search_rewrite = !source_has_spread
            && !call_args::has_named_args(args);
        let defaults = plan.default_argument_mask();
        let descriptor_projections = plan.descriptor_projection_mask();
        let normalized_args = plan.normalized_args();
        let args = normalized_args.as_slice();
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = source_has_spread;
        let regular_param_count = call_args::regular_param_count(sig);
        let spread_projects_into_reference = !by_value_reference_params
            && descriptor_projections
                .iter()
                .enumerate()
                .any(|(index, projected)| {
                    *projected && sig.ref_params.get(index).copied().unwrap_or(false)
                });
        // The variadic collector is represented as the signature's final param
        // with no default expression, but it never contributes to minimum arity.
        let required = call_args::required_param_count(sig);

        if sig.ref_params.iter().any(|is_ref| *is_ref) && has_spread && !allow_by_ref_spread {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} cannot be invoked with spread arguments when it has pass-by-reference parameters",
                    callee_desc
                ),
            ));
        }
        if spread_projects_into_reference {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} cannot be invoked with spread arguments when it has pass-by-reference parameters",
                    callee_desc
                ),
            ));
        }

        if !has_spread {
            if sig.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{} expects at least {} arguments, got {}",
                            callee_desc, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > sig.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} expects {} arguments, got {}",
                        callee_desc,
                        Self::format_fixed_or_range_arity(required, sig.params.len()),
                        effective_arg_count
                    ),
                ));
            }
        }

        let variadic_elem_ty = self.variadic_argument_element_type(sig, span, callee_desc)?;
        // Spreads and named arguments decide which parameter a spread element binds, so a
        // possible callable literal anywhere in the planned arguments is recorded.
        let call_declares_callable = Self::call_declares_callable(
            &Self::declared_spread_targets(sig, 0, regular_param_count),
            variadic_elem_ty.as_ref(),
        );

        let mut param_idx = 0usize;
        for arg in args {
            if call_declares_callable {
                self.record_static_callable_literals(arg);
            }
            let descriptor_projected = descriptor_invocation
                && descriptor_projections
                    .get(param_idx)
                    .copied()
                    .unwrap_or(false);
            let supplied_reference_for_inference = !by_value_reference_params
                && param_idx < regular_param_count
                && sig.ref_params.get(param_idx).copied().unwrap_or(false)
                && !defaults.get(param_idx).copied().unwrap_or(false);
            let inferred = if descriptor_projected {
                // Descriptor unpack reads a runtime-keyed Mixed cell. Its binder owns the tag
                // check against the selected parameter after deciding which slot this key fills.
                Ok(PhpType::Mixed)
            } else if descriptor_invocation {
                self.infer_descriptor_call_arg_type(arg, caller_env)
            } else {
                self.infer_type(arg, caller_env)
            };
            // PHP creates an undefined variable when it is passed to a by-reference parameter.
            // The reference writes a fresh value, so there is no undefined read to diagnose.
            let actual_ty = match inferred {
                Ok(ty) => ty,
                Err(error)
                    if supplied_reference_for_inference
                        && error.message.starts_with("Undefined variable: $") =>
                {
                    PhpType::Mixed
                }
                Err(error) => return Err(error),
            };
            let can_widen_by_ref_local = self.by_ref_argument_can_widen_local_to_mixed(arg)
                || self.boxed_reference_promotion_pending(arg, span);
            if matches!(arg.kind, ExprKind::Spread(_)) {
                self.record_spread_scalar_guard_arg_site(sig, param_idx, regular_param_count, arg.span);
                continue;
            }
            if param_idx < regular_param_count {
                let supplied_reference = !by_value_reference_params
                    && sig.ref_params.get(param_idx).copied().unwrap_or(false)
                    && !defaults.get(param_idx).copied().unwrap_or(false);
                if supplied_reference && !self.internal_callback_binding {
                    if descriptor_invocation && matches!(arg.kind, ExprKind::ArrayAccess { .. })
                        && !self.is_native_selection_reference_argument(arg, caller_env)? {
                        let param_name = sig
                            .params
                            .get(param_idx)
                            .map(|(name, _)| name.as_str())
                            .unwrap_or("arg");
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{} parameter ${} cannot bind an array element by reference through callable descriptor dispatch; call the target directly or bind the element to a reference variable first",
                                callee_desc, param_name
                            ),
                        ));
                    }
                    // The callee holds a reference to this local from here on, and it can
                    // escape, so the local is never kill/retype eligible in this body.
                    self.record_reference_alias_root(arg);
                    let lvalue = self.is_by_ref_argument_lvalue(arg, caller_env)?;
                    let reference_expected_ty = if sig.params.get(param_idx).is_some()
                        && sig
                            .declared_params
                            .get(param_idx)
                            .copied()
                            .unwrap_or(false)
                    {
                        sig.params.get(param_idx).map(|(_, ty)| ty)
                    } else if sig.params.get(param_idx).is_some() {
                        // An undeclared by-reference parameter is PHP `mixed`; its property
                        // argument binds the property's live cell even when the inferred
                        // signature narrowed the slot from another call site.
                        Some(&crate::types::PhpType::Mixed)
                    } else {
                        None
                    };
                    let property_place = allow_property_ref_places
                        && !source_has_spread
                        && match reference_expected_ty {
                            Some(expected_ty) => {
                                (matches!(
                                    arg.kind,
                                    ExprKind::PropertyAccess { .. } | ExprKind::ArrayAccess { .. }
                                ) && self.property_ref_argument_supported(
                                    arg,
                                    expected_ty,
                                    caller_env,
                                )?)
                                    || self.by_ref_property_place(arg, expected_ty, caller_env)?
                            }
                            None => false,
                        };
                    if !property_place && !lvalue {
                        let param_name = sig
                            .params
                            .get(param_idx)
                            .map(|(name, _)| name.as_str())
                            .unwrap_or("arg");
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{} parameter ${} must be passed a variable",
                                callee_desc, param_name
                            ),
                        ));
                    }
                }
                if let Some((param_name, expected_ty)) = sig.params.get(param_idx) {
                    let runtime_descriptor_projection = actual_ty.codegen_repr() == PhpType::Mixed
                        && descriptor_projected
                        && !supplied_reference;
                    let tracks_boxed_reference_output = supplied_reference
                        && !self.internal_callback_binding
                        && (sig.declared_params.get(param_idx).copied().unwrap_or(false)
                            || matches!(expected_ty, PhpType::Mixed));
                    if tracks_boxed_reference_output {
                        let layout_contract = crate::types::is_written_reference_array_param(
                            &sig.ref_params,
                            &sig.param_type_exprs,
                            param_idx,
                        );
                        self.require_boxed_by_ref_storage(
                            expected_ty,
                            &actual_ty,
                            arg,
                            caller_env,
                            can_widen_by_ref_local,
                            layout_contract,
                            &format!("{} parameter ${}", callee_desc, param_name),
                        )?;
                        self.record_boxed_reference_output(
                            arg,
                            expected_ty,
                            &actual_ty,
                            span,
                            caller_env,
                            layout_contract,
                        )?;
                    }
                    // `strict_types` applies to every declared parameter type, including the
                    // closure and first-class-callable surfaces that stay off the coercive
                    // path. Builtin signatures carry `declared_params: false` throughout
                    // (`crate::builtins::registry`), so this never fires for an internal
                    // function whose parameter types the checker does not consume.
                    if !runtime_descriptor_projection
                        && sig.declared_params.get(param_idx).copied().unwrap_or(false)
                    {
                        self.require_strict_types_param_binding(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("{} parameter ${}", callee_desc, param_name),
                        )?;
                    }
                    // A proven callable array is accepted only where EIR materializes a real
                    // descriptor for the declared Callable slot. Untracked Array(Mixed) values
                    // and unresolved literal shapes remain ordinary arrays and retain the error.
                    let proven_callable_array = matches!(expected_ty, PhpType::Callable)
                        && !Self::types_compatible(expected_ty, &actual_ty)
                        && !self.type_accepts(expected_ty, &actual_ty)
                        && self
                            .callable_array_param_target(arg, caller_env)?
                            .is_some();
                    // `str_replace([literal, ...], 'x', $s)` is folded into scalar calls by
                    // the AST optimizer before lowering, so its array search is accepted here
                    // even though the runtime helper is string-only.
                    let literal_str_replace_search = supports_literal_search_rewrite
                        && param_idx == 0
                        && callee_desc
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("str_replace")
                        && args.get(1).is_some_and(|replacement| matches!(
                            replacement.kind, ExprKind::StringLiteral(_)
                        ))
                        && matches!(&arg.kind, crate::parser::ast::ExprKind::ArrayLiteral(items)
                            if !items.is_empty()
                                && items.iter().all(|item| matches!(
                                    item.kind,
                                    crate::parser::ast::ExprKind::StringLiteral(_)
                                )));
                    // A by-value list is rebuilt as the declared hash; refuse what cannot be.
                    if sig.declared_params.get(param_idx).copied().unwrap_or(false)
                        && !supplied_reference
                        && !runtime_descriptor_projection
                    {
                        self.require_packed_argument_hash_storage(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("{} parameter ${}", callee_desc, param_name),
                        )?;
                    }
                    if !proven_callable_array && !runtime_descriptor_projection
                        && !literal_str_replace_search
                    {
                        if (coercive_param_binding
                            && sig.declared_params.get(param_idx).copied().unwrap_or(false))
                            || (supplied_reference && matches!(expected_ty, PhpType::Object(_)))
                            || (supplied_reference
                                && expected_ty.codegen_repr() == PhpType::Mixed
                                && actual_ty.codegen_repr() == PhpType::Mixed)
                        {
                            self.require_bound_param_arg_type(
                                expected_ty,
                                &actual_ty,
                                arg,
                                caller_env,
                                &format!("{} parameter ${}", callee_desc, param_name),
                                None,
                                supplied_reference,
                                param_idx + 1,
                            )?;
                        } else if !supplied_reference
                            && *expected_ty == PhpType::DeclaredCallable
                            && sig.declared_params.get(param_idx).copied().unwrap_or(false)
                        {
                            // A closure, arrow function or first-class callable binds without
                            // coercion, but a literal name still has to be resolved and recorded
                            // for the runtime dispatch, as a named function's `callable` does.
                            self.require_declared_callable_arg(
                                expected_ty,
                                &actual_ty,
                                arg,
                                caller_env,
                                &format!("{} parameter ${}", callee_desc, param_name),
                                None,
                            )?;
                        } else {
                            if !supplied_reference && expected_ty.declares_callable() {
                                self.record_declared_callable_literal_name(arg, caller_env);
                            }
                            self.require_compatible_arg_type(
                                expected_ty,
                                &actual_ty,
                                arg.span,
                                &format!("{} parameter ${}", callee_desc, param_name),
                            )?;
                        }
                    }
                }
            } else {
                // An argument collected by a by-REFERENCE variadic (`&...$xs`) is bound by
                // reference exactly like a regular by-ref parameter's, so the local it names is
                // aliased for the rest of the body. Recorded outside the element-type check below
                // because that one only runs when the element type is a known array, which has
                // nothing to do with aliasing.
                //
                // The collector's own slot is addressed through
                // `crate::types::signatures::variadic_param_index`, the one rule the whole
                // descriptor container contract uses. `regular_param_count` is NOT that index: it
                // deliberately hides the synthesized `__elephc_func_argc` parameter, so for a
                // body that calls `func_get_args()` it names the slot BEFORE the collector, and
                // this read answered with the hidden count slot's flag instead.
                let variadic_index = crate::types::signatures::variadic_param_index(sig);
                let variadic_by_ref = variadic_index
                    .and_then(|index| sig.ref_params.get(index))
                    .copied()
                    .unwrap_or(false);
                if variadic_by_ref {
                    if !self.internal_callback_binding {
                        self.validate_by_ref_variadic_argument(
                            arg,
                            &actual_ty,
                            caller_env,
                            span,
                            callee_desc,
                            sig.variadic.as_deref().unwrap_or("args"),
                            descriptor_invocation,
                        )?;
                    }
                }
                if let (Some(vname), Some(expected_ty)) =
                    (sig.variadic.as_ref(), variadic_elem_ty.as_ref())
                {
                    // Gating on the collector's own `declared_params` slot keeps the strict
                    // rejection off builtin variadics, whose registry-derived parameter types the
                    // checker does not otherwise consume. The slot is the one
                    // `variadic_param_index` names, so it stays the collector's flag even when a
                    // hidden parameter shares the signature.
                    if variadic_index
                        .and_then(|index| sig.declared_params.get(index))
                        .copied()
                        .unwrap_or(false)
                    {
                        self.require_strict_types_param_binding(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("{} variadic parameter ${}", callee_desc, vname),
                        )?;
                    }
                    let variadic_by_ref = variadic_index
                        .and_then(|index| sig.ref_params.get(index))
                        .copied()
                        .unwrap_or(false);
                    self.require_declared_callable_variadic_arg(
                        expected_ty,
                        &actual_ty,
                        arg,
                        caller_env,
                        &format!("{} variadic parameter ${}", callee_desc, vname),
                        variadic_by_ref,
                    )?;
                    self.require_compatible_arg_type(
                        expected_ty,
                        &actual_ty,
                        arg.span,
                        &format!("{} variadic parameter ${}", callee_desc, vname),
                    )?;
                    // Same list-to-hash rule as a regular declared parameter: a collected list
                    // is rebuilt as the element hash, or refused when no conversion exists.
                    if variadic_index
                        .and_then(|index| sig.declared_params.get(index))
                        .copied()
                        .unwrap_or(false)
                        && !variadic_by_ref
                    {
                        self.require_packed_argument_hash_storage(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("{} variadic parameter ${}", callee_desc, vname),
                        )?;
                    }
                }
            }
            param_idx += 1;
        }

        Ok(sig.return_type.clone())
    }

    /// The element contract ONE argument collected by `sig`'s variadic collector must satisfy.
    ///
    /// The SOURCE hint first, the collector's storage only as a fallback. A descriptor-reachable
    /// collector is STORED as `array<mixed>` so the invoker may hand it a hash
    /// (`crate::types::signatures::descriptor_variadic_container`), and re-deriving the element
    /// contract from that storage would silently turn `int ...$xs` into an untyped tail that
    /// accepts anything: exactly the direct-call check the transport decision must not cost.
    /// The declaration's own element syntax survives promotion in `param_type_exprs`, so
    /// resolving it here keeps a direct call checked as strictly as before the storage moved,
    /// whether or not the callee was ever handed out as a callable.
    ///
    /// An UNDECLARED collector has no source contract, so its storage element is the answer; that
    /// is also the path builtin variadics take, whose registry-derived parameter types carry no
    /// type syntax.
    pub(super) fn variadic_argument_element_type(
        &self,
        sig: &FunctionSig,
        span: crate::span::Span,
        callee_desc: &str,
    ) -> Result<Option<PhpType>, CompileError> {
        let Some(index) = crate::types::signatures::variadic_param_index(sig) else {
            return Ok(None);
        };
        if let Some(type_expr) = crate::types::signatures::variadic_source_element_type_expr(sig) {
            let variadic_name = sig.params[index].0.clone();
            return self
                .resolve_declared_param_type_hint(
                    type_expr,
                    span,
                    &format!("{} variadic parameter ${}", callee_desc, variadic_name),
                )
                .map(Some);
        }
        Ok(match &sig.params[index].1 {
            PhpType::Array(elem) => Some((**elem).clone()),
            _ => None,
        })
    }

    /// Rejects dynamic spreads that could feed raw array storage into a `Callable` slot.
    ///
    /// Argument unpacking can project existing callable descriptors from `array<Callable>`, but
    /// lowering has no element-wise conversion from PHP callable arrays such as `[$object, 'm']`
    /// into descriptor values. Mixed elements remain valid because Callable parameter lowering
    /// unboxes the cell and validates its descriptor tag at runtime.
    pub(super) fn validate_callable_spread_elements(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        plan: &call_args::CallArgPlan,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<(), CompileError> {
        if !args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
            return Ok(());
        }
        let regular_param_count = call_args::regular_param_count(sig);
        let spread_span = args
            .iter()
            .find(|arg| matches!(arg.kind, ExprKind::Spread(_)))
            .map(|arg| arg.span)
            .unwrap_or_else(crate::span::Span::dummy);
        let variadic_is_callable = self
            .variadic_argument_element_type(sig, spread_span, callee_desc)?
            .is_some_and(|ty| ty.codegen_repr() == PhpType::Callable);
        let mut checked_spreads = HashSet::new();

        if !plan.regular_args.is_empty() {
            for (param_idx, planned) in plan.regular_args.iter().enumerate() {
                let call_args::PlannedRegularArg::SpreadElement {
                    spread_expr,
                    spread_span,
                    ..
                } = planned
                else {
                    continue;
                };
                if sig
                    .params
                    .get(param_idx)
                    .is_some_and(|(_, ty)| ty.codegen_repr() == PhpType::Callable)
                    && checked_spreads.insert(*spread_span)
                {
                    self.require_callable_descriptor_spread(
                        spread_expr,
                        *spread_span,
                        caller_env,
                        callee_desc,
                    )?;
                }
            }
        } else {
            let mut positional_idx = 0usize;
            for arg in args {
                if let ExprKind::Spread(inner) = &arg.kind {
                    let feeds_callable = sig
                        .params
                        .iter()
                        .take(regular_param_count)
                        .skip(positional_idx)
                        .any(|(_, ty)| ty.codegen_repr() == PhpType::Callable)
                        || variadic_is_callable;
                    if feeds_callable && checked_spreads.insert(arg.span) {
                        self.require_callable_descriptor_spread(
                            inner,
                            arg.span,
                            caller_env,
                            callee_desc,
                        )?;
                    }
                } else if !matches!(arg.kind, ExprKind::NamedArg { .. }) {
                    positional_idx = positional_idx.saturating_add(1);
                }
            }
        }

        if variadic_is_callable {
            for arg in args {
                let ExprKind::Spread(inner) = &arg.kind else {
                    continue;
                };
                if checked_spreads.insert(arg.span) {
                    self.require_callable_descriptor_spread(
                        inner,
                        arg.span,
                        caller_env,
                        callee_desc,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Requires one dynamic spread source to expose actual callable descriptor elements.
    fn require_callable_descriptor_spread(
        &mut self,
        source: &Expr,
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<(), CompileError> {
        let spread = Expr::new(ExprKind::Spread(Box::new(source.clone())), span);
        let element_ty = self.infer_descriptor_call_arg_type(&spread, caller_env)?;
        if matches!(
            element_ty.codegen_repr(),
            PhpType::Callable | PhpType::Mixed
        ) {
            return Ok(());
        }
        let detail = if matches!(
            element_ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ) {
            "callable-array elements are not converted to descriptors during argument unpacking"
        } else {
            "the spread element type is not a proven callable descriptor"
        };
        Err(CompileError::new(
            span,
            &format!(
                "{} spread feeding a Callable parameter must contain Callable descriptors: {}",
                callee_desc, detail
            ),
        ))
    }
}
