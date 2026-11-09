//! Purpose:
//! Validates function call validation semantics for the checker.
//! Keeps call diagnostics and return-flow analysis consistent with signatures and inferred expression types.
//!
//! Called from:
//! - `crate::types::checker::functions`
//!
//! Key details:
//! - Diagnostics should map shared planner errors back to source spans without duplicating call semantics.

use crate::fast_hash::FastSet as HashSet;

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

/// Returns whether an expression denotes writable storage accepted by a by-reference call.
///
/// Named arguments delegate to their value. Container elements remain writable when their
/// base is itself a stable local, property, static property, or container place. Nullsafe and
/// computed-property reads are excluded because they do not guarantee one stable write target.
fn is_writable_call_place(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => is_stable_place_receiver(object),
        // `f($obj->{$name})`: the lowering reads the property into a temporary, passes that, and
        // writes it back under the same (once-evaluated) name, as for a declared property.
        ExprKind::DynamicPropertyAccess { object, .. } => is_stable_place_receiver(object),
        ExprKind::ArrayAccess { array, .. } => is_stable_place_receiver(array),
        ExprKind::NamedArg { value, .. } => is_writable_call_place(value),
        _ => false,
    }
}

impl Checker {
    /// Accepts a call result bound to a by-reference parameter and records its site for the
    /// lowering's notice; returns false for every other non-place shape.
    pub(crate) fn record_by_ref_call_result(&mut self, arg: &Expr) -> bool {
        let value = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        };
        if !is_call_result_argument(value) {
            return false;
        }
        self.by_ref_call_result_sites.insert(value.span);
        true
    }
}

/// Returns whether a by-reference argument is a call result rather than a place.
///
/// php binds such a value to a temporary and only raises the "Only variables should be passed by
/// reference" notice, where a literal or an operator expression stays a compile error. The
/// lowering emits that notice and passes the value through a hidden local.
pub(crate) fn is_call_result_argument(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. } => true,
        ExprKind::NamedArg { value, .. } => is_call_result_argument(value),
        _ => false,
    }
}

/// Returns whether a receiver chain can be read and written without changing its root.
fn is_stable_place_receiver(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => is_stable_place_receiver(object),
        ExprKind::ArrayAccess { array, .. } => is_stable_place_receiver(array),
        _ => false,
    }
}

/// Returns whether a normalized slot still carries the exact declaration
/// default inserted by an earlier planner pass.
pub(super) fn is_injected_declared_default(
    sig: &FunctionSig,
    param_idx: usize,
    arg: &Expr,
) -> bool {
    sig.defaults
        .get(param_idx)
        .and_then(|default| default.as_ref())
        .is_some_and(|default| default.span == arg.span && default == arg)
}

impl Checker {
    /// Enforces PHP's syntactic "no argument unpacking after named arguments"
    /// rule on a call surface whose callee is not resolvable at compile time
    /// (string callables, `new $class(...)`), where no signature is available to
    /// run the shared planner against.
    ///
    /// The rule itself stays in `crate::types::call_args`; this only maps its
    /// error onto the same diagnostic the planner-backed surfaces produce.
    pub(crate) fn require_no_spread_after_named_args(
        &self,
        args: &[Expr],
        callee_desc: &str,
    ) -> Result<(), CompileError> {
        call_args::validate_no_spread_after_named(args).map_err(|err| match err {
            CallArgPlanError::SpreadAfterNamed { span } => {
                spread_after_named_error(span, callee_desc)
            }
            // The rule only reports `SpreadAfterNamed`; the remaining variants
            // need a signature to plan against and cannot originate here. Keep
            // the match exhaustive so a future rule still reports a real span.
            CallArgPlanError::UnknownNamed { span, .. }
            | CallArgPlanError::Duplicate { span, .. }
            | CallArgPlanError::PositionalAfterNamed { span }
            | CallArgPlanError::PositionalAfterSpread { span }
            | CallArgPlanError::PositionalAfterNamedUnpack { span }
            | CallArgPlanError::MissingRequired { span, .. } => {
                CompileError::new(span, &format!("{} has invalid arguments", callee_desc))
            }
        })
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
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        if !matches!(
            arg.kind,
            ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }
        ) {
            return Ok(false);
        }
        let property_ty = self.infer_type(arg, env)?;
        Ok(crate::types::param_binding::by_ref_property_storage_compatible(
            expected_ty,
            &property_ty,
        ))
    }

    /// Returns whether the named constructor binds its by-reference parameter into a property.
    ///
    /// `callee_desc` is `Constructor 'Class::__construct'`, the spelling the constructor checks
    /// pass; the answer comes from the program-wide escape pre-pass.
    fn constructor_keeps_parameter_reference(&self, callee_desc: &str, param: Option<&str>) -> bool {
        let (Some(param), Some(rest)) = (param, callee_desc.strip_prefix("Constructor '")) else {
            return false;
        };
        let Some(scope) = rest.strip_suffix('\'') else {
            return false;
        };
        let scope = crate::types::checker::ref_param_widening::folded_scope_key(scope);
        self.property_bound_ref_params.contains(&(scope, param.to_string()))
    }

    /// Refuses binding a builtin Throwable payload slot (`$this->message`, `$this->code`, ...)
    /// to a user by-reference parameter: the slot cannot become the reference cell php shares
    /// (see `is_native_throwable_payload_property`), and a silent copy would lose the callee's
    /// writes. The refusal hands the class file to the runtime instead.
    fn reject_throwable_payload_reference(
        &mut self,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let place = match &arg.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => arg,
        };
        let ExprKind::PropertyAccess { object, property } = &place.kind else {
            return Ok(());
        };
        let object_ty = self.infer_type(object, env)?;
        let Some(class) = crate::types::checker::single_object_class_name(&object_ty) else {
            return Ok(());
        };
        if !self.is_native_throwable_payload_property(&class, property) {
            return Ok(());
        }
        Err(CompileError::new(
            place.span,
            &format!(
                "The builtin Throwable property ${} cannot be passed by reference",
                property
            ),
        ))
    }

    /// Returns true when an argument expression is a writable PHP l-value.
    pub(crate) fn is_by_ref_argument_lvalue(
        &mut self,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let writable = is_writable_call_place(arg);
        match &arg.kind {
            ExprKind::Variable(_) => Ok(true),
            ExprKind::NamedArg { value, .. } => {
                let addressable = self.is_by_ref_argument_lvalue(value, env)?;
                Ok(writable || addressable)
            }
            // The receiver walk is not only a predicate: an element of a declared property
            // (`f($holder->items[0])`) promotes that property to a reference property, so every
            // later write publishes through its cell before releasing the old value. Answering
            // from the writable-place shape alone skipped the promotion, and a destructor run by
            // `$holder->items = [...]` read the slot while its old array was being freed.
            ExprKind::ArrayAccess { array, .. } => {
                let addressable = self.is_addressable_ref_array_receiver(array, env)?;
                Ok(writable || addressable)
            }
            _ => Ok(writable),
        }
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
            // A property place is rewritten through its own temporary and write-back; it has no
            // caller local to widen or to publish as a boxed reference output. Array properties
            // still need their container payload repacked by the variadic collector, which the
            // property temporary does not provide.
            let property_ty = self.infer_type(storage_arg, caller_env)?;
            if property_ty.is_php_array()
                || matches!(
                    property_ty.codegen_repr(),
                    PhpType::Array(_) | PhpType::AssocArray { .. }
                )
            {
                return Err(CompileError::new(
                    storage_arg.span,
                    &format!(
                        "{} variadic parameter ${} cannot bind an array property by reference; bind the property to a local variable first",
                        callee_desc, variadic_name
                    ),
                ));
            }
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
            &format!("{} variadic parameter ${}", callee_desc, variadic_name),
        )?;
        self.record_boxed_reference_output(
            arg,
            &PhpType::Mixed,
            actual_ty,
            call_span,
            caller_env,
        );
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
        match (expected, actual) {
            (PhpType::Mixed, _) => true,
            (_, PhpType::Never) => true, // never is the bottom type — compatible with any expected type
            (PhpType::Bool, PhpType::False) => true,
            // The backend has explicit boxed-gradual cast funnels for scalar boundaries. Most
            // unions use the same boxed representation as `Mixed`; compact nullable integers use
            // `TaggedScalar` instead and deliberately remain excluded until their ABI conversion
            // is explicit.
            (
                PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str,
                actual @ (PhpType::Mixed | PhpType::Union(_)),
            ) if actual.codegen_repr() == PhpType::Mixed => true,
            // Boxed unions keep the same representation. Concrete heap boundaries below use
            // shared EIR narrowing with runtime checks before their payload is exposed.
            (PhpType::Union(_), PhpType::Mixed) => true,
            (
                PhpType::Object(_) | PhpType::Callable,
                actual @ (PhpType::Mixed | PhpType::Union(_)),
            ) if actual.codegen_repr() == PhpType::Mixed => true,
            // A parameter declared as a UNION of heap shapes -- classes, `callable`, or both --
            // accepts a boxed value for the same reason the single-shape arm above does: the
            // representation is the identical boxed cell and the class is narrowed with a
            // runtime check before the payload is exposed. Without this, the union destination
            // was stricter than either of its members would have been alone.
            //
            // `$container->get($id)` is declared `?object` by PSR-11, so every eager service
            // fetch handed to a `callable|SomeClass` parameter arrives as `Object("")|Void`.
            // php compiles that and raises its TypeError at run time if the object turns out
            // not to match; refusing it at compile time rejects a program php accepts.
            // A plain `Mixed` actual is already accepted by the `(Union, Mixed)` arm above.
            (
                PhpType::Union(expected_members),
                actual @ PhpType::Union(_),
            ) if actual.codegen_repr() == PhpType::Mixed
                && expected_members
                    .iter()
                    .all(|member| matches!(member, PhpType::Object(_) | PhpType::Callable)) =>
            {
                true
            }
            // The same destination fed an object whose CLASS is statically unknown. A single
            // class destination already accepts one; a union of them refusing it would make the
            // union stricter than any of its members, which is not what a union means.
            (PhpType::Union(expected_members), PhpType::Object(actual_name))
                if actual_name.is_empty()
                    && expected_members
                        .iter()
                        .all(|member| matches!(member, PhpType::Object(_) | PhpType::Callable)) =>
            {
                true
            }
            (PhpType::Iterable, actual @ (PhpType::Mixed | PhpType::Union(_)))
                if actual.codegen_repr() == PhpType::Mixed => true,
            (PhpType::Array(element), actual @ (PhpType::Mixed | PhpType::Union(_)))
                if element.codegen_repr() == PhpType::Mixed
                    && actual.codegen_repr() == PhpType::Mixed => true,
            (
                PhpType::AssocArray { key, value },
                actual @ (PhpType::Mixed | PhpType::Union(_)),
            )
                if key.codegen_repr() == PhpType::Mixed
                    && value.codegen_repr() == PhpType::Mixed
                    && actual.codegen_repr() == PhpType::Mixed => true,
            // A declared `array` is the boxed packed-or-hash union, and it accepts a boxed value
            // the way the concrete `array<mixed>` arm above does: the boundary narrows it with a
            // runtime check. `$this->metadata = unpack(...)` stores `array|false` into an
            // `array` property, and a `?array &$failed` out-parameter is returned as `array`.
            (expected @ PhpType::Union(_), actual @ PhpType::Union(_))
                if expected.is_php_array() && actual.codegen_repr() == PhpType::Mixed => true,
            (PhpType::Iterable, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable) => true,
            // The other direction, which php also accepts once the value really is an array: the
            // argument lowering materialises the iterable first, exactly as `iterator_to_array()`
            // would, so the callee never sees a Traversable. `twig/twig`'s
            // `SandboxExtension::ensureSpreadAllowed(iterable $obj)` converts its Traversable half
            // and then hands the result to a parameter declared `array`.
            (PhpType::Array(_) | PhpType::AssocArray { .. }, PhpType::Iterable) => true,
            // A BOXED union that overlaps the destination is php's runtime question, not a
            // compile-time refusal: php checks the value the call actually passes. Symfony's
            // `getParameter()` is declared `array|bool|string|int|float|\UnitEnum|null`, and
            // `cache:clear` hands the cache directory it returns to `Filesystem::remove(string|
            // iterable $files)` -- refusing that kept the command out of the build.
            (PhpType::Union(expected_members), actual @ PhpType::Union(actual_members))
                if actual.codegen_repr() == PhpType::Mixed
                    && actual_members.iter().any(|actual_member| {
                        !matches!(actual_member, PhpType::Void)
                            && expected_members.iter().any(|expected_member| {
                                Self::types_compatible(expected_member, actual_member)
                            })
                    }) =>
            {
                true
            }
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
        let member_accepted = |member: &PhpType| {
            Self::types_compatible(expected, member)
                || self.type_accepts(expected, member)
                || self.object_argument_may_narrow_to(expected, member)
        };
        // An argument UNION is accepted member by member, each through whichever rule fits it:
        // Carbon's `setTimezone(static::safeCreateDateTimeZone(...))` passes `?CarbonTimeZone` to
        // `DateTimeZone|string|int`, where `null` is the coercion `types_compatible` admits and
        // `CarbonTimeZone` the subclass only `type_accepts` knows; neither rule took both.
        // php tests a nullable argument when the call runs, so a union whose only unaccepted
        // member is null stays a run-time TypeError: symfony's `HttplugWaitLoop` hands a
        // `?SplObjectStorage` it tested just before to `iterable`.
        let member_may_pass = |member: &PhpType| *member == PhpType::Void || member_accepted(member);
        if member_accepted(actual)
            || matches!(actual, PhpType::Union(members)
                if members.iter().any(|member| member_accepted(member))
                    && members.iter().all(member_may_pass))
        {
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

    /// Whether an object argument of a SUPERTYPE can satisfy a class-typed parameter.
    ///
    /// php checks a class parameter against the object that actually arrives, so passing a
    /// `Node` (or an interface, `Phase2OptimizableConditional`) to a parameter declared
    /// `ConditionalTerm|ConditionalPrimary|ConditionalFactor` is valid whenever the runtime class
    /// is one of them, and a `TypeError` otherwise. The closed world cannot disprove it, so it is
    /// accepted here when some expected class is a SUBTYPE of the argument's static type; the
    /// call lowering guards the narrowing at run time (Doctrine's `Parser` and `SqlWalker`, the
    /// type-info `Type::collection()` family).
    fn object_argument_may_narrow_to(&self, expected: &PhpType, actual: &PhpType) -> bool {
        let PhpType::Object(actual_name) = actual else {
            return false;
        };
        if actual_name.is_empty() {
            return false;
        }
        let narrows_to = |expected_name: &str| {
            !expected_name.is_empty()
                && (self.is_subclass_of(expected_name, actual_name)
                    || self.class_implements_interface(expected_name, actual_name)
                    || self.interface_extends_interface(expected_name, actual_name))
        };
        // The object behind an interface may declare `__toString()`, which outside strict types
        // converts to the `string` php then checks at run time: symfony's `CommandTester` hands
        // an `InputInterface` to `ExecutionResult::$input`, a `string`.
        let actual_is_interface = self.interfaces.contains_key(actual_name);
        match expected {
            PhpType::Str => actual_is_interface,
            PhpType::Object(expected_name) => narrows_to(expected_name),
            PhpType::Union(members)
                if actual_is_interface && members.contains(&PhpType::Str) =>
            {
                true
            }
            PhpType::Union(members) => {
                members.iter().all(|member| matches!(member, PhpType::Object(_) | PhpType::Void))
                    && members.iter().any(|member| {
                        matches!(member, PhpType::Object(name) if narrows_to(name))
                    })
            }
            _ => false,
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

    /// `check_known_callable_call` for a callback the ENGINE invokes rather than the source.
    ///
    /// `usort()`, `uksort()`, `array_map()` and the rest do not call their callback from the
    /// calling file; the engine calls it, under PHP's COERCIVE parameter binding whatever
    /// `strict_types` that file declared — which is why the callback checks already run inside
    /// `with_internal_callback_binding`. The argument types the checker synthesizes for such a
    /// call are the container's element or KEY types, and an array key is `int|string` at
    /// runtime however the array was inferred, so a comparator declared with `string`
    /// parameters has to accept the `Int` key type an indexed array reports. PHP binds it; the
    /// runtime callable invoker performs the matching adaptation.
    pub(crate) fn check_engine_invoked_callback_call(
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
            true,
            false,
            false,
            false,
        )
    }

    /// `check_known_callable_call` for a closure literal the engine invokes with a fixed argument
    /// list: surplus arguments are ignored, as PHP ignores them for any userland function.
    pub(crate) fn check_known_userland_callable_call(
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
            true,
            false,
            false,
        )
    }

    /// `check_engine_invoked_callback_call` for a callback known to be USERLAND code.
    ///
    /// PHP passes a userland function every argument the engine supplies and lets it ignore the
    /// ones it does not declare: `preg_replace_callback('/x/', fn() => 'y', $s)` and
    /// `array_map(fn() => 1, $a)` are ordinary PHP. Only an INTERNAL function rejects surplus
    /// arguments, which is why this is not the default for every callback.
    pub(crate) fn check_engine_invoked_userland_callback_call(
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
            true,
            true,
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
        let coercive = self.class_supports_coercive_param_binding(owner_class);
        let internal_sig = self.internal_operand_binding_signature(owner_class, sig);
        let sig = internal_sig.as_ref().unwrap_or(sig);
        // php ignores surplus arguments to any userland function (`$strictValidator->isValid($v,
        // false, true)` in Symfony's `EmailValidator`, written for egulias 1's wider signature).
        // Only an internal callee rejects them, which compiler-injected classes stand in for.
        // A constructor stays strict: a fixed `new` has no lowering that evaluates and drops the
        // surplus yet (only the dynamic-`new` ladder does, for side-effect-free extras).
        let userland = self.class_is_user_declared(owner_class)
            && !callee_desc.starts_with("Constructor");
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            false,
            coercive,
            userland,
            false,
            true,
        )
    }

    /// `check_user_declared_call` for a receiver whose SUBCLASSES declare more parameters than the
    /// signature in hand.
    ///
    /// PHP dispatches on the runtime class and ignores arguments a userland function does not
    /// declare, so a call that looks over-supplied for the type in hand is exactly right for an
    /// override. `twig/twig`'s `IncludeNode` is the shape: it declares
    /// `addGetTemplate(Compiler $compiler/* , string $template = '' */)` with the second parameter
    /// COMMENTED OUT for backwards compatibility, `EmbedNode` overrides it with two, and
    /// `IncludeNode` itself calls it with two.
    pub(crate) fn check_user_declared_call_with_advisory_arity(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
        owner_class: &str,
    ) -> Result<PhpType, CompileError> {
        let coercive = self.class_supports_coercive_param_binding(owner_class);
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            false,
            coercive,
            true,
            false,
            true,
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
        let coercive = self.class_supports_coercive_param_binding(owner_class);
        let internal_sig = self.internal_operand_binding_signature(owner_class, sig);
        let sig = internal_sig.as_ref().unwrap_or(sig);
        self.check_known_callable_call_with_options(
            sig,
            args,
            span,
            caller_env,
            callee_desc,
            true,
            coercive,
            false,
            true,
            true,
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
    ///
    /// An interface counts too: a call dispatched through a user interface binds the arguments
    /// with the interface's signature, and php coerces them exactly as for a class method.
    /// Symfony's `ConstraintViolationBuilderInterface::setParameter(string $key, string $value)`
    /// takes ints from ten validators, and refusing them interpreted every one of those files.
    fn class_is_user_declared(&self, class_name: &str) -> bool {
        self.classes
            .get(class_name)
            .is_some_and(|info| info.declaration_span != crate::span::Span::dummy())
            || self
                .interfaces
                .get(class_name)
                .is_some_and(|info| info.declaration_span != crate::span::Span::dummy())
    }

    /// Returns whether ordinary EIR argument lowering can apply PHP scalar parameter coercions.
    ///
    /// `IteratorIterator` is a compiled prelude class: psysh's Hoa `FileFinder` passes the int
    /// `RecursiveIteratorIterator` mode as its `?string $class`, which php coerces.
    fn class_supports_coercive_param_binding(&self, class_name: &str) -> bool {
        self.class_is_user_declared(class_name)
            || matches!(
                class_name.trim_start_matches('\\'),
                "DateTime" | "DateTimeImmutable" | "IteratorIterator"
            )
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
            true,
            false,
        )
    }

    /// Validates a call made THROUGH a callable value, whose arity is advisory.
    ///
    /// The signature is inferred from the callables elephc has seen flow into the slot, not
    /// declared by the callee, and PHP binds a user function's declared parameters while ignoring
    /// the surplus. Only a shortfall below the required count is a defect in every shape.
    pub(crate) fn check_callable_value_call(
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
            true,
            false,
            false,
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
        callee_arity_is_advisory: bool,
        descriptor_invocation: bool,
        allow_property_ref_places: bool,
    ) -> Result<PhpType, CompileError> {
        let plan = if descriptor_invocation {
            self.plan_descriptor_call_args(sig, args, span, callee_desc)?
        } else {
            self.plan_named_call_args(sig, args, span, callee_desc, caller_env)?
        };
        self.validate_callable_spread_elements(sig, args, &plan, caller_env, callee_desc)?;
        let source_has_spread = plan.has_spread_args();
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
        let spread_projects_into_reference = descriptor_projections
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
        // Under the descriptor contract the invoker binds each unpacked entry to its
        // by-reference parameter at run time, from the reference marker the unpack keeps.
        if spread_projects_into_reference && !allow_by_ref_spread {
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
            } else if effective_arg_count < required
                || (effective_arg_count > sig.params.len() && !callee_arity_is_advisory)
            {
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

        let mut param_idx = 0usize;
        for arg in args {
            let descriptor_projected = descriptor_invocation
                && descriptor_projections
                    .get(param_idx)
                    .copied()
                    .unwrap_or(false);
            let actual_ty = if descriptor_projected {
                // Descriptor unpack reads a runtime-keyed Mixed cell. Its binder owns the tag
                // check against the selected parameter after deciding which slot this key fills.
                PhpType::Mixed
            } else if descriptor_invocation {
                self.infer_descriptor_call_arg_type(arg, caller_env)?
            } else {
                self.infer_call_argument_type(arg, caller_env)?
            };
            let can_widen_by_ref_local = self.by_ref_argument_can_widen_local_to_mixed(arg)
                || self.boxed_reference_promotion_pending(arg, span);
            if matches!(arg.kind, ExprKind::Spread(_)) {
                continue;
            }
            if param_idx < regular_param_count {
                // An omitted trailing default the planner materialized is not a caller
                // expression at all, so neither the l-value shape nor the reference binding
                // has anything to say about it.
                let omitted_default = defaults.get(param_idx).copied().unwrap_or(false)
                    || is_injected_declared_default(sig, param_idx, arg);
                let supplied_reference = sig.ref_params.get(param_idx).copied().unwrap_or(false)
                    && !omitted_default;
                if supplied_reference && !self.internal_callback_binding {
                    if descriptor_invocation && matches!(arg.kind, ExprKind::ArrayAccess { .. }) {
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
                    // A constructor that STORES the reference (`$this->message = &$message`) and
                    // is handed the payload slot (`new class($this->message, ...)`, Symfony's
                    // `AutowiringFailedException`) binds it to a copy: the slot cannot become a
                    // reference cell, and the lazy message idiom only needs the final text. The one
                    // divergence is timing -- the message callback runs when the object is
                    // assigned to the slot (as php does itself under xdebug) instead of at the
                    // first `getMessage()`. A callee that WRITES through the parameter would
                    // silently lose its writes, so those stay refused.
                    let param_name = sig.params.get(param_idx).map(|(name, _)| name.as_str());
                    if !self.constructor_keeps_parameter_reference(callee_desc, param_name) {
                        self.reject_throwable_payload_reference(arg, caller_env)?;
                    }
                    let property_place = allow_property_ref_places
                        && !source_has_spread
                        && match sig.params.get(param_idx).map(|(_, ty)| ty) {
                            Some(expected_ty) => {
                                self.by_ref_property_place(arg, expected_ty, caller_env)?
                            }
                            None => false,
                        };
                    if !property_place
                        && !self.is_by_ref_argument_lvalue(arg, caller_env)?
                        && !self.record_by_ref_call_result(arg)
                    {
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
                    self.binding_position = if descriptor_invocation { 0 } else { param_idx + 1 };
                    self.forget_definite_param_mismatch(arg);
                    let runtime_descriptor_projection = actual_ty.codegen_repr() == PhpType::Mixed
                        && descriptor_projected
                        && !supplied_reference;
                    let tracks_boxed_reference_output = supplied_reference
                        && !self.internal_callback_binding
                        && (sig.declared_params.get(param_idx).copied().unwrap_or(false)
                            || matches!(expected_ty, PhpType::Mixed));
                    if tracks_boxed_reference_output {
                        self.require_boxed_by_ref_storage(
                            expected_ty,
                            &actual_ty,
                            arg,
                            caller_env,
                            can_widen_by_ref_local,
                            &format!("{} parameter ${}", callee_desc, param_name),
                        )?;
                        self.record_boxed_reference_output(
                            arg,
                            expected_ty,
                            &actual_ty,
                            span,
                            caller_env,
                        );
                    }
                    // `strict_types` applies to every declared parameter type, including the
                    // closure and first-class-callable surfaces that stay off the coercive
                    // path. Builtin signatures carry `declared_params: false` throughout
                    // (`crate::builtins::registry`), so this never fires for an internal
                    // function whose parameter types the checker does not consume.
                    if !runtime_descriptor_projection
                        && sig.declared_params.get(param_idx).copied().unwrap_or(false)
                    {
                        let context = format!("{} parameter ${}", callee_desc, param_name);
                        if let Err(err) = self.require_strict_types_param_binding(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &context,
                        ) {
                            if self.defer_definite_param_mismatch(
                                expected_ty,
                                &actual_ty,
                                arg,
                                &context,
                                supplied_reference,
                            ) {
                                // php binds this argument when the call RUNS and throws there.
                                param_idx += 1;
                                continue;
                            }
                            return Err(err);
                        }
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
                    if !proven_callable_array && !runtime_descriptor_projection {
                        if (coercive_param_binding
                            && sig.declared_params.get(param_idx).copied().unwrap_or(false))
                            || (supplied_reference && matches!(expected_ty, PhpType::Object(_)))
                        {
                            self.require_bound_param_arg_type(
                                expected_ty,
                                &actual_ty,
                                arg,
                                caller_env,
                                &format!("{} parameter ${}", callee_desc, param_name),
                                None,
                                supplied_reference,
                            )?;
                        } else {
                            let declared_value_param = sig
                                .declared_params
                                .get(param_idx)
                                .copied()
                                .unwrap_or(false)
                                && !sig.ref_params.get(param_idx).copied().unwrap_or(false);
                            // The same four guards `require_bound_param_arg_type` consults on the
                            // coercive path. The gradual pair was missing here, so a value PHP checks
                            // at the call -- `$container->getParameter()`, declared
                            // `array|bool|string|int|float|UnitEnum|null`, reaching `?string $buildDir`
                            // in Symfony's `WarmableInterface::warmUp()` -- was refused statically on an
                            // INTERFACE-dispatched call while the identical call on the concrete class
                            // was accepted.
                            let has_runtime_guard = crate::types::param_binding::object_requires_runtime_nominal_guard(
                                expected_ty,
                                &actual_ty,
                            ) || crate::types::param_binding::gradual_object_requires_runtime_nominal_guard(
                                expected_ty,
                                &actual_ty,
                            ) || crate::types::param_binding::nullable_int_requires_runtime_param_guard(
                                expected_ty,
                                &actual_ty,
                            ) || crate::types::param_binding::gradual_union_requires_runtime_param_guard(
                                expected_ty,
                                &actual_ty,
                            );
                            if !declared_value_param || !has_runtime_guard {
                                self.require_compatible_arg_type(
                                    expected_ty,
                                    &actual_ty,
                                    arg.span,
                                    &format!("{} parameter ${}", callee_desc, param_name),
                                )?;
                            }
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
                self.binding_position = 0;
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
                    let declared_variadic = variadic_index
                        .and_then(|index| sig.declared_params.get(index))
                        .copied()
                        .unwrap_or(false);
                    if coercive_param_binding && declared_variadic {
                        self.require_bound_param_arg_type(
                            expected_ty,
                            &actual_ty,
                            arg,
                            caller_env,
                            &format!("{} variadic parameter ${}", callee_desc, vname),
                            None,
                            variadic_by_ref,
                        )?;
                    } else {
                        if declared_variadic {
                            self.require_strict_types_param_binding(
                                expected_ty,
                                &actual_ty,
                                arg.span,
                                &format!("{} variadic parameter ${}", callee_desc, vname),
                            )?;
                        }
                        self.require_compatible_arg_type(
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
        let mut checked_spreads = HashSet::default();

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
