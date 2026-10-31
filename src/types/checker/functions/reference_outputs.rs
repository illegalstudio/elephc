//! Purpose:
//! Carries boxed by-reference storage effects from signature validation to caller inference.
//!
//! Called from:
//! - Function/callable argument validation and expression assignment-effect inference.
//!
//! Key details:
//! - Arguments are already normalized by the shared call planner.
//! - A referenced array element widens its local root to boxed Mixed element storage.
//! - A `null` local bound to a `?ptr<T>` parameter becomes a `?ptr<T>` local.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Records the hash layout that live nested push references install in their local root.
    pub(crate) fn record_array_push_parent_storage(
        &mut self, receiver: &Expr, call_span: Span, env: &TypeEnv,
    ) {
        if !call_span.identifies_a_node() { return; }
        let mut root = receiver;
        let mut indexed = false;
        loop {
            match &root.kind {
                ExprKind::ArrayAccess { array, .. } => { indexed = true; root = array; }
                ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) => root = value,
                _ => break,
            }
        }
        if !indexed { return; }
        let ExprKind::Variable(name) = &root.kind else { return; };
        if self.local_parameter_names.contains(name) || self.active_globals.contains(name)
            || self.active_statics.contains(name) || self.mixed_storage_locals.contains(name)
            || self.local_has_non_builtin_reference_alias(receiver)
        { return; }
        let Some(ty) = env.get(name) else { return; };
        let boxed_value = |value: &PhpType| {
            if matches!(value, PhpType::Array(_) | PhpType::AssocArray { .. }) || value.is_php_array() {
                PhpType::php_array()
            } else { PhpType::Mixed }
        };
        let storage = match ty.codegen_repr() {
            PhpType::Array(value) =>
                PhpType::AssocArray { key: Box::new(PhpType::Int), value: Box::new(boxed_value(&value)) },
            PhpType::AssocArray { key, value } =>
                PhpType::AssocArray { key, value: Box::new(boxed_value(&value)) },
            _ => return,
        };
        self.boxed_reference_outputs
            .entry((self.current_loop_storage_scope.clone(), call_span))
            .or_default().insert(name.clone(), storage);
    }

    /// Records property storage before argument effects erase EIR class facts.
    /// Returns true when reference binding itself raises Error, including a nested Mixed read.
    pub(crate) fn record_array_push_reference_place(
        &mut self, receiver: &Expr, env: &TypeEnv,
    ) -> Result<bool, crate::errors::CompileError> {
        match &receiver.kind {
            ExprKind::PropertyAccess { object, property } => {
                if matches!(&object.kind, ExprKind::Variable(name)
                    if self.boxed_parameter_locals.contains(name))
                {
                    return Err(crate::errors::CompileError::new(receiver.span,
                        "array_push() property reference has ambiguous receiver classes; declare the receiver parameter class"));
                }
                let ty = self.infer_type(object, env)?;
                let key = (self.current_loop_storage_scope.clone(), receiver.span);
                let class = super::super::single_object_class_name(&ty);
                let concrete = matches!(ty.codegen_repr(), PhpType::Object(_))
                    && class.as_ref().is_some_and(|class| self.classes.contains_key(class));
                if !concrete && self.array_push_property_plans.get(&key)
                    .is_some_and(|plan| plan.class_name.is_some())
                {
                    return Err(crate::errors::CompileError::new(receiver.span,
                        "array_push() property reference has ambiguous receiver classes at this source position"));
                }
                if !concrete
                    || self.array_push_property_plans.get(&key)
                        .is_some_and(|plan| plan.class_name.is_none())
                {
                    self.array_push_property_plans.insert(key,
                        crate::types::ArrayPushPropertyPlan {
                            class_name: None, rejection: None, live_addressable: false,
                        });
                    return Ok(false);
                }
                let class = class.expect("single receiver class checked above");
                let info = &self.classes[class.as_str()];
                let owner = info.property_declaring_classes.get(property)
                    .cloned().unwrap_or_else(|| class.clone());
                let get = crate::names::php_symbol_key(&crate::names::property_hook_get_method(property));
                let set = crate::names::php_symbol_key(&crate::names::property_hook_set_method(property));
                let in_accessor = self.current_class.as_deref() == Some(owner.as_str())
                    && self.current_method.as_ref().is_some_and(|method| method == &get || method == &set);
                let reference_getter = !in_accessor
                    && info.methods.get(&get).is_some_and(|sig| sig.by_ref_return);
                let reference_property = info.reference_properties.contains(property);
                let property_ty = info.visible_property(property)
                    .map(|(_, (_, ty))| ty.clone());
                let denied_hook = !in_accessor && !reference_getter
                    && (info.methods.contains_key(&set)
                        || info.methods.get(&get).is_some_and(|sig| !sig.by_ref_return));
                let setter = info.property_set_visibilities.get(property)
                    .or_else(|| info.property_visibilities.get(property))
                    .cloned().unwrap_or(crate::parser::ast::Visibility::Public);
                let rejection = if denied_hook {
                    Some(format!("Indirect modification of {owner}::${property} is not allowed"))
                } else if info.readonly_properties.contains(property) {
                    Some(format!("Cannot indirectly modify readonly property {owner}::${property}"))
                } else if !reference_getter && !self.can_access_member(&owner, &setter) {
                    let scope = self.current_class.as_ref().map_or_else(
                        || "global scope".to_string(), |class| format!("scope {class}"));
                    Some(format!("Cannot indirectly modify {}(set) property {owner}::${property} from {scope}",
                        Self::visibility_label(&setter)))
                } else { None };
                let structurally_addressable = self
                    .ref_property_slot_addressability(object, property, env)?
                    .is_some_and(|(_, addressable)| addressable);
                let live_addressable = rejection.is_none()
                    && (reference_getter
                        || reference_property
                        || (structurally_addressable
                            && property_ty.as_ref()
                                .is_some_and(PhpType::supports_live_array_push_place)));
                let plan = crate::types::ArrayPushPropertyPlan {
                    class_name: Some(class.clone()), rejection, live_addressable,
                };
                if self.array_push_property_plans.get(&key).is_some_and(|existing| existing != &plan) {
                    return Err(crate::errors::CompileError::new(receiver.span,
                        "array_push() property reference has ambiguous receiver classes at this source position"));
                }
                let permitted = plan.rejection.is_none();
                self.array_push_property_plans.insert(key, plan);
                if permitted && live_addressable && structurally_addressable
                    && !reference_getter && !reference_property {
                    self.reference_property_promotions.insert((class, property.clone()));
                }
                return Ok(!permitted);
            }
            ExprKind::ArrayAccess { array, .. }
            | ExprKind::NamedArg { value: array, .. }
            | ExprKind::ErrorSuppress(array) => return self.record_array_push_reference_place(array, env),
            _ => {},
        }
        Ok(false)
    }

    /// Returns whether a runtime spread can safely mutate this `array_push` receiver in place.
    pub(crate) fn array_push_runtime_spread_supported(
        &mut self,
        receiver: &Expr,
        receiver_ty: &PhpType,
        env: &TypeEnv,
    ) -> Result<bool, crate::errors::CompileError> {
        if self.local_has_non_builtin_reference_alias(receiver)
            && self.array_push_place_has_native_array_container(receiver, receiver_ty, env)?
        {
            return Ok(false);
        }
        let mut root = receiver;
        while let ExprKind::ArrayAccess { array, .. }
        | ExprKind::NamedArg { value: array, .. }
        | ExprKind::ErrorSuppress(array) = &root.kind
        {
            root = array;
        }
        Ok(match &root.kind {
            ExprKind::Variable(name) => !self.active_statics.contains(name)
                && (!self.active_globals.contains(name)
                    || matches!(receiver.kind, ExprKind::Variable(_))),
            ExprKind::PropertyAccess { .. } => self
                .array_push_property_plans
                .get(&(self.current_loop_storage_scope.clone(), root.span))
                .is_some_and(|plan| plan.live_addressable),
            ExprKind::StaticPropertyAccess { .. } => {
                matches!(receiver.kind, ExprKind::StaticPropertyAccess { .. })
                    && receiver_ty.supports_live_array_push_place()
            }
            _ => false,
        })
    }

    /// Returns whether the receiver or an indexed parent uses native packed-array storage.
    fn array_push_place_has_native_array_container(
        &mut self,
        receiver: &Expr,
        receiver_ty: &PhpType,
        env: &TypeEnv,
    ) -> Result<bool, crate::errors::CompileError> {
        if matches!(receiver_ty.codegen_repr(), PhpType::Array(_)) {
            return Ok(true);
        }
        let mut current = receiver;
        loop {
            match &current.kind {
                ExprKind::ArrayAccess { array, .. } => {
                    if matches!(self.infer_type(array, env)?.codegen_repr(), PhpType::Array(_)) {
                        return Ok(true);
                    }
                    current = array;
                }
                ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) => {
                    current = value;
                }
                _ => return Ok(false),
            }
        }
    }

    /// Records a local whose reference binding requires canonical boxed storage after the call.
    ///
    /// A `null` local bound to a `?ptr<T>` parameter leaves the call as that `?ptr<T>`: the
    /// callee may store a live address through it, and lowering gives the slot that pointer word.
    pub(crate) fn record_boxed_reference_output(
        &mut self,
        arg: &Expr,
        expected: &PhpType,
        actual: &PhpType,
        call_span: Span,
        env: &TypeEnv,
    ) {
        let undefined_source = match &arg.kind {
            ExprKind::Variable(name) => env.get(name).is_none(),
            ExprKind::NamedArg { value, .. } => matches!(
                &value.kind,
                ExprKind::Variable(name) if env.get(name).is_none()
            ),
            _ => false,
        };
        let output_ty = if undefined_source {
            PhpType::Mixed
        } else if expected.is_php_array()
            && matches!(actual, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            PhpType::php_array()
        } else if (expected.codegen_repr() == PhpType::Mixed
            || matches!(expected, PhpType::Object(_)))
            && actual.codegen_repr() != PhpType::Mixed
        {
            PhpType::Mixed
        } else if crate::types::is_nullable_pointer_type(expected)
            && matches!(actual.codegen_repr(), PhpType::Void | PhpType::Never)
        {
            expected.clone()
        } else {
            return;
        };
        if !call_span.identifies_a_node() {
            return;
        }
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        let (name, output_ty) = match &arg.kind {
            ExprKind::Variable(name) => {
                if output_ty == PhpType::Mixed && arg.span.identifies_a_node() {
                    self.boxed_reference_promotion_sites
                        .entry((self.current_loop_storage_scope.clone(), arg.span))
                        .or_default()
                        .insert(name.clone());
                }
                (name, output_ty)
            }
            ExprKind::ArrayAccess { array, .. } if output_ty == PhpType::Mixed => {
                let mut root = array.as_ref();
                while let ExprKind::ArrayAccess { array, .. } = &root.kind {
                    root = array;
                }
                let ExprKind::Variable(name) = &root.kind else {
                    return;
                };
                let Some(root_ty) = env.get(name) else {
                    return;
                };
                let widened = match root_ty.codegen_repr() {
                    PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
                    PhpType::AssocArray { key, .. } => PhpType::AssocArray {
                        key,
                        value: Box::new(PhpType::Mixed),
                    },
                    _ => return,
                };
                (name, widened)
            }
            _ => return,
        };
        self.boxed_reference_outputs
            .entry((self.current_loop_storage_scope.clone(), call_span))
            .or_default()
            .insert(name.clone(), output_ty);
    }

    /// Returns whether this call has already approved boxing the same local for an earlier slot.
    pub(crate) fn boxed_reference_promotion_pending(
        &self,
        arg: &Expr,
        call_span: Span,
    ) -> bool {
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        self.boxed_reference_outputs
            .get(&(self.current_loop_storage_scope.clone(), call_span))
            .and_then(|outputs| outputs.get(name))
            .is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed)
    }

    /// Applies one successful call's storage changes before checking subsequent expressions.
    pub(crate) fn apply_boxed_reference_outputs(&mut self, span: Span, env: &mut TypeEnv) {
        let key = (self.current_loop_storage_scope.clone(), span);
        if let Some(outputs) = self.boxed_reference_outputs.remove(&key) {
            for (name, output_ty) in outputs {
                env.insert(name, output_ty);
            }
        }
    }
}
