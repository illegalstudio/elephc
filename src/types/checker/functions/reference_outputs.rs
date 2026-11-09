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
    ) -> Result<(), crate::errors::CompileError> {
        // A narrowed value can still occupy a nullable or union slot. Use that slot for
        // layout decisions and restore its full view after a callee can write through it.
        let array_value = actual.is_php_array()
            || matches!(actual, PhpType::Array(_) | PhpType::AssocArray { .. });
        let guarded_storage = self.guarded_local_storage_type(arg, env).cloned();
        let keeps_concrete = guarded_storage.is_none()
            && self.nullable_reference_keeps_concrete_storage(expected, actual, arg);
        self.record_declared_reference_binding(arg, expected, keeps_concrete, call_span)?;
        if keeps_concrete {
            return Ok(());
        }
        let actual = guarded_storage.as_ref().unwrap_or(actual);
        let output_ty = if expected.is_php_array()
            && array_value
        {
            PhpType::php_array()
        } else if (expected.codegen_repr() == PhpType::Mixed
            || matches!(expected, PhpType::Object(_)))
            && (actual.codegen_repr() != PhpType::Mixed
                || (guarded_storage.is_some() && matches!(expected, PhpType::Object(_) | PhpType::Mixed)))
        {
            PhpType::Mixed
        } else if crate::types::is_nullable_pointer_type(expected)
            && matches!(actual.codegen_repr(), PhpType::Void | PhpType::Never)
        {
            expected.clone()
        } else if let Some(storage) = guarded_storage {
            storage
        } else {
            return Ok(());
        };
        if !call_span.identifies_a_node() {
            return Ok(());
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
                    return Ok(());
                };
                let Some(root_ty) = env.get(name) else {
                    return Ok(());
                };
                let widened = match root_ty.codegen_repr() {
                    PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
                    PhpType::AssocArray { key, .. } => PhpType::AssocArray {
                        key,
                        value: Box::new(PhpType::Mixed),
                    },
                    _ => return Ok(()),
                };
                (name, widened)
            }
            _ => return Ok(()),
        };
        self.boxed_reference_outputs
            .entry((self.current_loop_storage_scope.clone(), call_span))
            .or_default()
            .insert(name.clone(), output_ty);
        Ok(())
    }

    /// Records how a declared-type local is bound to by-reference parameter `expected` in the
    /// call at `call_span`, and rejects the call once the same local is bound both through a
    /// boxed writeback cell (a nullable parameter keeping its typed slot) and by its direct
    /// typed address (a concrete parameter, or any of its elements bound in place): the two
    /// cannot alias, so a write through one would be lost to the other, where php shares one
    /// reference.
    fn record_declared_reference_binding(
        &mut self,
        arg: &Expr,
        expected: &PhpType,
        through_cell: bool,
        call_span: Span,
    ) -> Result<(), crate::errors::CompileError> {
        let mut bare = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &bare.kind {
            bare = value;
        }
        // An element (`$a[0]`, `$a[0][1]`) is bound in place, inside the root local's array
        // storage: for the alias rule it is a direct binding of that root, so a boxed
        // whole-array writeback cell of the same local in the same call would lose its writes.
        let mut element = false;
        while let ExprKind::ArrayAccess { array, .. } = &bare.kind {
            element = true;
            bare = array;
        }
        let ExprKind::Variable(name) = &bare.kind else {
            return Ok(());
        };
        if !call_span.identifies_a_node() || !self.typed_local_names.contains_key(name) {
            return Ok(());
        }
        let through_cell = through_cell && !element;
        let direct = element
            || (!through_cell
                && !matches!(expected.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar));
        let entry = self
            .declared_reference_bindings
            .entry((self.current_loop_storage_scope.clone(), call_span))
            .or_default()
            .entry(name.clone())
            .or_insert((false, false));
        entry.0 |= through_cell;
        entry.1 |= direct;
        if entry.0 && entry.1 {
            return Err(crate::errors::CompileError::new(
                arg.span,
                &format!(
                    "${name} is passed by reference to a nullable parameter and to a typed parameter of the same call (itself or one of its elements); elephc cannot share one reference between them, pass a separate variable to one of them"
                ),
            ));
        }
        Ok(())
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

    /// Finds an earlier boxed-layout promotion without reusing it after explicit reference rebinding.
    pub(crate) fn boxed_reference_layout_precedes(&self, arg: &Expr) -> bool {
        let ExprKind::Variable(name) = &arg.kind else { return false; };
        !self.ref_assign_aliased_locals.contains(name) && arg.span.identifies_a_node()
            && self.boxed_reference_promotion_sites.iter().any(|((scope, span), names)|
                scope == &self.current_loop_storage_scope && span.identifies_a_node()
                    && (span.line, span.col) < (arg.span.line, arg.span.col) && names.contains(name))
    }

    /// Applies one successful call's storage changes before checking subsequent expressions.
    pub(crate) fn apply_boxed_reference_outputs(&mut self, span: Span, env: &mut TypeEnv) {
        let key = (self.current_loop_storage_scope.clone(), span);
        if let Some(outputs) = self.boxed_reference_outputs.remove(&key) {
            for (name, output_ty) in outputs {
                self.record_local_storage_effect(&name, &output_ty);
                self.definitely_bound_locals.insert(name.clone());
                env.insert(name, output_ty);
            }
        }
    }
}

impl Checker {
    /// Whether a DECLARED-type local bound to a NULLABLE typed by-reference parameter keeps its
    /// own concrete storage across the call (`?array &$read` given `array<Foo> $read = …`,
    /// `?Foo &$o` given `Foo $o = …`, `?float &$f` given `float $f = …`).
    ///
    /// The declaration pins the local's storage, so it cannot take the parameter's boxed cell:
    /// the cell is written back into the typed slot after the call, and the writeback verifies
    /// the callee left a value of the local's own kind (converting an array built from Mixed
    /// values into the typed element layout, and checking object classes and nested array
    /// layouts). `null`, which the declared type does not admit, stops the program there with a
    /// diagnostic. Every other local (parameters included) takes the boxed cell like php, and a
    /// place without a frame slot of its own (a global, a static, a property, an element)
    /// follows the ordinary by-reference storage rules, because the writeback only targets a
    /// local slot.
    ///
    /// An object local keeps its class only when every class the parameter admits is that class
    /// or one of its subclasses: `?Base &$v` given `A $v` may receive a sibling `B`, which `A`'s
    /// property layout cannot describe, so that binding follows the ordinary rules instead.
    pub(crate) fn nullable_reference_keeps_concrete_storage(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
    ) -> bool {
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        if !self.typed_local_names.contains_key(name)
            || self.active_ref_params.contains(name)
            || self.ref_assign_aliased_locals.contains(name)
            || self.by_ref_captured_locals.contains(name)
            || self.boxed_ref_aliased_locals.contains(name)
            || self.active_globals.contains(name)
            || self.static_local_names.contains(name)
            || self.name_is_seeded_program_storage(name)
            || self.top_level_binding_is_program_global(name)
        {
            return false;
        }
        let PhpType::Union(members) = expected else {
            return false;
        };
        if !members.iter().any(|member| matches!(member, PhpType::Void)) {
            return false;
        }
        let non_null: Vec<&PhpType> = members
            .iter()
            .filter(|member| !matches!(member, PhpType::Void))
            .collect();
        match actual {
            PhpType::Array(element) => {
                writeback_element_supported(element, false)
                    && non_null.iter().all(|member| {
                        member.is_php_array()
                            || matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. })
                    })
            }
            PhpType::Object(local_class) => non_null.iter().all(|member| {
                matches!(member, PhpType::Object(member_class)
                    if self.object_class_stays_within(member_class, local_class))
            }),
            PhpType::Float => non_null.iter().all(|member| matches!(member, PhpType::Float)),
            _ => false,
        }
    }

    /// Whether every instance of `member_class` is also an instance of `local_class` (the same
    /// class, or a subclass of it), so a typed `local_class` slot can hold it.
    fn object_class_stays_within(&self, member_class: &str, local_class: &str) -> bool {
        let member = member_class.trim_start_matches('\\');
        let local = local_class.trim_start_matches('\\');
        crate::names::php_symbol_key(member) == crate::names::php_symbol_key(local)
            || self.is_subclass_of(member, local)
    }
}

/// Whether the by-reference writeback can rebuild an `array<element>` slot from the boxed cell:
/// it converts boxed Mixed slots into int, float, bool, object, callable and (nested) string
/// layouts, widens raw slots into `mixed`, and recurses into nested indexed arrays. A nested
/// hash is accepted only with `mixed` values, which hold anything: the writeback checks a
/// hash element's own tag, not its values, so `array<string, Item>` values could carry another
/// class. Other element representations (`?int`'s tagged scalars, pointers, buffers) have no
/// conversion either; such a local follows the ordinary by-reference storage rules instead.
/// A top-level string element is excluded too: the writeback keeps 8-byte payload slots there.
fn writeback_element_supported(element: &PhpType, nested: bool) -> bool {
    match element.codegen_repr() {
        PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::Mixed
        | PhpType::Object(_)
        | PhpType::Callable => true,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        PhpType::Str => nested,
        PhpType::Array(inner) => writeback_element_supported(&inner, true),
        _ => false,
    }
}
