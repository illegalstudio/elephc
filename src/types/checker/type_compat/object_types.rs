//! Purpose:
//! Checks type compatibility for object types cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, Visibility};
use crate::types::{EnumInfo, PhpType, TypeEnv};

use super::super::Checker;

/// Returns PHP's canonical spelling for a global builtin iterable interface name.
///
/// Matching the complete normalized name keeps a namespaced user interface such as
/// `Domain\Traversable` nominal and prevents it from acquiring builtin foreach semantics.
pub(super) fn canonical_builtin_iterable_interface_name(
    type_name: &str,
) -> Option<&'static str> {
    match php_symbol_key(type_name.trim_start_matches('\\')).as_str() {
        "traversable" => Some("Traversable"),
        "iterator" => Some("Iterator"),
        "iteratoraggregate" => Some("IteratorAggregate"),
        _ => None,
    }
}

impl Checker {
    /// Checks whether the current class context can access a member with the given visibility
    /// declared in `declaring_class`. Public members are always accessible; protected members
    /// are accessible if the current class is the declaring class or a subclass; private
    /// members are only accessible if the current class is exactly the declaring class.
    pub(crate) fn can_access_member(
        &self,
        declaring_class: &str,
        visibility: &Visibility,
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Protected => self.current_class.as_deref().is_some_and(|current| {
                current == declaring_class || self.is_subclass_of(current, declaring_class)
            }),
            Visibility::Private => self.current_class.as_deref() == Some(declaring_class),
        }
    }

    /// Returns the string label ("public", "protected", "private") for a visibility level.
    pub(crate) fn visibility_label(visibility: &Visibility) -> &'static str {
        match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }

    /// Returns true if `class_name` is or inherits from `ancestor_name` (excluding self equality).
    /// Walks the parent chain via `class_info.parent`.
    pub(crate) fn is_subclass_of(&self, class_name: &str, ancestor_name: &str) -> bool {
        let mut current = self
            .classes
            .get(class_name)
            .and_then(|class| class.parent.clone());
        while let Some(parent_name) = current {
            if parent_name == ancestor_name {
                return true;
            }
            current = self
                .classes
                .get(&parent_name)
                .and_then(|class| class.parent.clone());
        }
        false
    }

    /// Returns true if `class_name` directly implements `interface_name` (not via inheritance).
    pub(crate) fn class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        self.classes.get(class_name).is_some_and(|class_info| {
            class_info
                .interfaces
                .iter()
                .any(|name| name == interface_name)
        })
    }

    /// Returns true if `type_name` (a class or interface) implements `interface_name`,
    /// checking direct implementation and interface inheritance chains.
    pub(crate) fn object_type_implements_interface(
        &self,
        type_name: &str,
        interface_name: &str,
    ) -> bool {
        if self.classes.contains_key(type_name) {
            return self.class_implements_interface(type_name, interface_name);
        }
        if self.interfaces.contains_key(type_name) {
            return type_name == interface_name
                || self.interface_extends_interface(type_name, interface_name);
        }
        false
    }

    /// Returns true if `interface_name` is or transitively extends `ancestor_name`.
    /// Uses a DFS with cycle detection to walk interface parent chains.
    pub(crate) fn interface_extends_interface(
        &self,
        interface_name: &str,
        ancestor_name: &str,
    ) -> bool {
        if interface_name == ancestor_name {
            return true;
        }
        let mut stack = vec![interface_name.to_string()];
        let mut seen = HashSet::new();
        while let Some(current_name) = stack.pop() {
            if !seen.insert(current_name.clone()) {
                continue;
            }
            let Some(interface_info) = self.interfaces.get(&current_name) else {
                continue;
            };
            for parent_name in &interface_info.parents {
                if parent_name == ancestor_name {
                    return true;
                }
                stack.push(parent_name.clone());
            }
        }
        false
    }

    /// Returns true when a name is unresolved after eval made the runtime class set open.
    /// Such an object needs the runtime throw check instead of a closed-world diagnostic.
    pub(crate) fn object_type_may_be_eval_declared(&self, type_name: &str) -> bool {
        self.eval_barrier_active
            && !self.classes.contains_key(type_name)
            && !self.interfaces.contains_key(type_name)
    }

    /// Validates the static type of a `throw` operand (statement and expression forms).
    ///
    /// Besides a single Throwable object, this accepts boxed operands, which codegen throws
    /// through the runtime check that raises PHP's `Error("Can only throw objects")` /
    /// `Error("Cannot throw objects that do not implement Throwable")`:
    /// - a union whose members are all Throwable objects or null: a join of sibling exception
    ///   classes (`$a ? new A : new B`) or a `?Throwable` parameter;
    /// - `mixed`, such as a local joined from `null` and an exception across a loop
    ///   (`$e = null; foreach (...) { $e ??= new E; } throw $e ?? new F;`).
    ///
    /// A read of a local a join point may have left undefined is typed `mixed` by
    /// `condition_edges`, so its own type is checked instead: a maybe-undefined scalar is still
    /// rejected statically.
    pub(crate) fn check_thrown_type(
        &self,
        operand: &Expr,
        thrown_ty: PhpType,
        env: &TypeEnv,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let thrown_ty = match (&thrown_ty, &operand.kind) {
            (PhpType::Mixed, crate::parser::ast::ExprKind::Variable(name))
                if self
                    .maybe_undefined_result
                    .reads
                    .contains_key(&(operand.span, name.clone())) =>
            {
                env.get(name).cloned().unwrap_or(PhpType::Mixed)
            }
            _ => thrown_ty,
        };
        let is_throwable_object = |type_name: &str| {
            self.object_type_implements_throwable(type_name)
                || self.object_type_may_be_eval_declared(type_name)
        };
        let accepted = match &thrown_ty {
            PhpType::Object(type_name) => is_throwable_object(type_name),
            PhpType::Mixed => true,
            PhpType::Union(members) => {
                let mut objects = members.iter().filter_map(|member| match member {
                    PhpType::Object(type_name) => Some(type_name),
                    _ => None,
                });
                let only_objects_or_null = members
                    .iter()
                    .all(|member| matches!(member, PhpType::Object(_) | PhpType::Void));
                if !only_objects_or_null || members.iter().all(|m| *m == PhpType::Void) {
                    false
                } else if objects.all(|type_name| is_throwable_object(type_name)) {
                    true
                } else {
                    return Err(CompileError::new(
                        span,
                        "Type error: throw requires an object implementing Throwable",
                    ));
                }
            }
            _ => false,
        };
        if accepted {
            return Ok(());
        }
        Err(CompileError::new(
            span,
            if matches!(thrown_ty, PhpType::Object(_)) {
                "Type error: throw requires an object implementing Throwable"
            } else {
                "Type error: throw requires an object value"
            },
        ))
    }

    /// Returns true if `type_name` (a class or interface) implements `Throwable`,
    /// checking both direct implementation and interface extension chains.
    pub(crate) fn object_type_implements_throwable(&self, type_name: &str) -> bool {
        if self.classes.contains_key(type_name) {
            return self.class_implements_interface(type_name, "Throwable");
        }
        if self.interfaces.contains_key(type_name) {
            return self.interface_extends_interface(type_name, "Throwable");
        }
        false
    }

    /// Returns true if `type_name` (a class or interface) implements `Iterator` or
    /// `IteratorAggregate`, which are the interfaces that make a type usable in `foreach`.
    pub(crate) fn object_type_implements_iterable(&self, type_name: &str) -> bool {
        let type_name = type_name.trim_start_matches('\\');
        if canonical_builtin_iterable_interface_name(type_name).is_some() {
            return true;
        }
        if self.classes.contains_key(type_name) {
            return self.class_implements_interface(type_name, "Iterator")
                || self.class_implements_interface(type_name, "IteratorAggregate");
        }
        if self.interfaces.contains_key(type_name) {
            return self.interface_extends_interface(type_name, "Iterator")
                || self.interface_extends_interface(type_name, "IteratorAggregate");
        }
        false
    }

    /// Finds the most specific common ancestor object type shared by all type names in
    /// `type_names`. Falls back to "Throwable" if no common type is found or the list is empty.
    pub(crate) fn common_catch_type_name(&self, type_names: &[String]) -> String {
        let mut iter = type_names.iter();
        let Some(first_name) = iter.next() else {
            return "Throwable".to_string();
        };
        let mut common = first_name.clone();
        for type_name in iter {
            match self.common_object_type(&common, type_name) {
                Some(PhpType::Object(next_common)) => common = next_common,
                _ => return "Throwable".to_string(),
            }
        }
        common
    }

    /// Resolves a raw type name used in a `catch` clause to its concrete class name.
    /// Handles "self" (current class), "parent" (current class's parent), or a plain name.
    pub(crate) fn resolve_catch_type_name(
        &self,
        raw_name: &crate::names::Name,
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        match raw_name.as_str() {
            "self" => self.current_class.clone().ok_or_else(|| {
                CompileError::new(span, "Cannot use self in catch outside of a class context")
            }),
            "parent" => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(
                        span,
                        "Cannot use parent in catch outside of a class context",
                    )
                })?;
                self.classes
                    .get(current_class)
                    .and_then(|class_info| class_info.parent.clone())
                    .ok_or_else(|| CompileError::new(span, "Class has no parent class"))
            }
            _ => Ok(raw_name.to_string()),
        }
    }

    /// Type-checks and resolves a static method call on an enum (`cases`, `from`, `tryfrom`).
    /// Returns the resolved `PhpType` for the call or an error if argument count or types mismatch.
    pub(crate) fn check_enum_static_call(
        &mut self,
        enum_info: &EnumInfo,
        class_name: &str,
        method: &str,
        args: &[Expr],
        env: &TypeEnv,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        match method {
            "cases" => {
                if !args.is_empty() {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Static method '{}::cases' expects 0 arguments, got {}",
                            class_name,
                            args.len()
                        ),
                    ));
                }
                Ok(PhpType::Array(Box::new(PhpType::Object(
                    class_name.to_string(),
                ))))
            }
            "from" | "tryfrom" => {
                let Some(backing_ty) = enum_info.backing_type.as_ref() else {
                    return Err(CompileError::new(
                        span,
                        &format!("Undefined method: {}::{}", class_name, method),
                    ));
                };
                if args.len() != 1 {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Static method '{}::{}' expects 1 argument, got {}",
                            class_name,
                            method,
                            args.len()
                        ),
                    ));
                }
                let arg_ty = self.infer_type(&args[0], env)?;
                // PHP's int-backed enum `from()`/`tryFrom()` accepts a numeric string and
                // coerces it to the integer backing value at runtime: a numeric string with
                // no matching case throws `ValueError`, a non-numeric string throws
                // `TypeError`. A `Mixed`/dynamic argument (e.g. a `foreach` value or untyped
                // parameter) coerces on its runtime tag. Accept these here and defer the
                // coercion and error to codegen instead of rejecting at compile time
                // (issues #349, #449).
                let accepts_runtime_coercion = matches!(backing_ty, PhpType::Int)
                    && matches!(arg_ty, PhpType::Str | PhpType::Mixed | PhpType::Union(_));
                if !accepts_runtime_coercion && !self.type_accepts(backing_ty, &arg_ty) {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Type error: {}::{} expects {}, got {}",
                            class_name, method, backing_ty, arg_ty
                        ),
                    ));
                }
                if method == "from" {
                    Ok(PhpType::Object(class_name.to_string()))
                } else {
                    Ok(self.normalize_union_type(vec![
                        PhpType::Object(class_name.to_string()),
                        PhpType::Void,
                    ]))
                }
            }
            _ => {
                // User-declared static methods on the enum dispatch like class static methods.
                let key = crate::names::php_symbol_key(method);
                if let Some(sig) = self
                    .classes
                    .get(class_name)
                    .and_then(|class_info| class_info.static_methods.get(&key))
                    .cloned()
                {
                    return Ok(sig.return_type);
                }
                Err(CompileError::new(
                    span,
                    &format!("Undefined method: {}::{}", class_name, method),
                ))
            }
        }
    }

    /// Finds the most specific common object type between `left` and `right` class names.
    /// Returns `Some(PhpType::Object(...))` with the common ancestor, or `None` if they share no
    /// common type. Checks interface relationships, class inheritance, and ancestor chains.
    pub(crate) fn common_object_type(&self, left: &str, right: &str) -> Option<PhpType> {
        if left == right {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(left) && self.interface_extends_interface(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(right) && self.interface_extends_interface(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.interfaces.contains_key(left) && self.class_implements_interface(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(right) && self.class_implements_interface(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }

        let mut left_ancestors = HashSet::new();
        let mut current = Some(left.to_string());
        while let Some(class_name) = current {
            left_ancestors.insert(class_name.clone());
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        let mut current = Some(right.to_string());
        while let Some(class_name) = current {
            if left_ancestors.contains(&class_name) {
                return Some(PhpType::Object(class_name));
            }
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        None
    }

    /// The classes whose `prop_name` slot is the one `declaring_class` declares (see
    /// `Checker::property_sharers_index`).
    fn property_sharers(&mut self, prop_name: &str, declaring_class: &str) -> Vec<String> {
        let class_count = self.classes.len();
        if self
            .property_sharers_index
            .as_ref()
            .is_none_or(|(built_for, _)| *built_for != class_count)
        {
            let mut index: std::collections::HashMap<(String, String), Vec<String>> =
                std::collections::HashMap::new();
            for (name, info) in &self.classes {
                for (property, owner) in &info.property_declaring_classes {
                    index
                        .entry((property.clone(), owner.clone()))
                        .or_default()
                        .push(name.clone());
                }
            }
            self.property_sharers_index = Some((class_count, index));
        }
        self.property_sharers_index
            .as_ref()
            .and_then(|(_, index)| index.get(&(prop_name.to_string(), declaring_class.to_string())))
            .cloned()
            .unwrap_or_default()
    }

    /// Propagates a resolved constructor argument type into the corresponding constructor
    /// parameter and property slot for all classes that share an inherited property from
    /// `declaring_class`. Used to sharpen types across an inheritance hierarchy after
    /// constructor argument type inference.
    pub(crate) fn propagate_constructor_arg_type(
        &mut self,
        instantiated_class: &str,
        param_index: usize,
        arg_ty: &PhpType,
        param_has_declared_type: bool,
    ) {
        let param_is_ref = self
            .classes
            .get(instantiated_class)
            .and_then(|class_info| class_info.methods.get("__construct"))
            .and_then(|sig| sig.ref_params.get(param_index))
            .copied()
            .unwrap_or(false);
        let Some((prop_name, declaring_class)) =
            self.classes.get(instantiated_class).and_then(|class_info| {
                class_info
                    .constructor_param_to_prop
                    .get(param_index)
                    .and_then(|mapped| mapped.as_ref())
                    .map(|prop_name| {
                        let declaring_class = class_info
                            .property_declaring_classes
                            .get(prop_name)
                            .cloned()
                            .unwrap_or_else(|| instantiated_class.to_string());
                        (prop_name.clone(), declaring_class)
                    })
            })
        else {
            return;
        };

        // Every write below is a JOIN (`crate::types::checker::lattice`), with a one-time
        // adoption over the slot's pre-observation type. The last constructor call used to
        // REPLACE the shared slot: `new A(1); new B("s"); new C(1.5)` typed the inherited `$v`
        // `float` for all three (gettype printed `double` for the int and the string), and on
        // Laravel `new HttpResponseException($symfonyResponse)` in one body and
        // `new HttpResponseException($illuminateResponse)` in another alternated
        // `$response` (and `getResponse()`'s return) between the two classes every round.
        let incoming = if param_is_ref { PhpType::Mixed } else { arg_ty.clone() };
        for sharer in self.property_sharers(&prop_name, &declaring_class) {
            let (property_slot, param_current, internal) = {
                let Some(class_info) = self.classes.get(&sharer) else {
                    continue;
                };
                // Compiler-owned (prelude/builtin) classes keep the historical rebind: their slot
                // storage is pinned again after every method pass by the builtin patches
                // (`patch_builtin_spl_storage_signatures` forces `CallbackFilterIterator::$callback`
                // to `callable`), so they are not a lattice writer, and the descriptor lowering of
                // a runtime callable array relies on the constructor's last argument shape.
                let internal = class_info.is_internal;
                let property_slot = (!class_info.visible_property_is_declared(&prop_name))
                    .then(|| class_info.visible_property_index(&prop_name))
                    .flatten()
                    .and_then(|slot| class_info.properties.get(slot).map(|prop| (slot, prop.1.clone())));
                // Sharing a property does not erase another constructor's declared contract or
                // make a different parameter position refer to this property's constructor input.
                let maps_same_property = class_info
                    .constructor_param_to_prop
                    .get(param_index)
                    .and_then(|mapped| mapped.as_ref())
                    == Some(&prop_name);
                let param_current = (!param_has_declared_type && maps_same_property)
                    .then(|| class_info.methods.get("__construct"))
                    .flatten()
                    .filter(|sig| !sig.declared_params.get(param_index).copied().unwrap_or(false))
                    .and_then(|sig| sig.params.get(param_index).map(|(_, ty)| ty.clone()));
                (property_slot, param_current, internal)
            };
            let property_key = (format!("ctor-prop:{}::${}", sharer, prop_name), 0usize);
            let new_property = property_slot.map(|(slot, current)| {
                let adopt = internal || !self.param_specialization_seen.contains(&property_key);
                let joined = if adopt { incoming.clone() } else { self.join_observed_type(&current, &incoming) };
                (slot, joined)
            });
            let param_key = (format!("ctor-param:{}", sharer), param_index);
            let new_param = param_current.map(|current| {
                if !internal && self.param_specialization_seen.contains(&param_key) {
                    self.join_observed_type(&current, &incoming)
                } else {
                    incoming.clone()
                }
            });
            if new_property.is_some() {
                self.param_specialization_seen.insert(property_key);
            }
            if new_param.is_some() {
                self.param_specialization_seen.insert(param_key);
            }
            let Some(class_info) = self.classes.get_mut(&sharer) else {
                continue;
            };
            if let Some((slot, ty)) = new_property {
                if let Some(prop) = class_info.properties.get_mut(slot) {
                    prop.1 = ty;
                }
            }
            if let Some(ty) = new_param {
                if let Some(sig) = class_info.methods.get_mut("__construct") {
                    if let Some((_, param_ty)) = sig.params.get_mut(param_index) {
                        *param_ty = ty;
                    }
                }
            }
        }
    }
}
