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
        // A class under construction is absent from `classes` but its parent is already known;
        // override covariance is validated during that window.
        let mut current = self.parent_of(class_name);
        let mut seen = std::collections::HashSet::new();
        while let Some(parent_name) = current {
            if parent_name == ancestor_name {
                return true;
            }
            if !seen.insert(parent_name.clone()) {
                return false;
            }
            current = self.parent_of(&parent_name);
        }
        false
    }

    /// The parent of `class_name`: from its built `ClassInfo`, else from the class being built
    /// (`pending_class_parents`), else from its declaration while schemas are still being built
    /// (`declared_class_ancestry`).
    fn parent_of(&self, class_name: &str) -> Option<String> {
        if let Some(class) = self.classes.get(class_name) {
            return class.parent.clone();
        }
        self.pending_class_parents.get(class_name).cloned().or_else(|| {
            self.declared_class_ancestry.get(class_name).and_then(|(parent, _)| parent.clone())
        })
    }

    /// Whether a class that is declared but not built yet implements `interface_name`, through
    /// its own declaration or an ancestor's (built or declared).
    fn declared_class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        let mut current = Some(class_name.to_string());
        let mut seen = HashSet::new();
        while let Some(name) = current {
            if !seen.insert(name.clone()) {
                return false;
            }
            if self.classes.contains_key(&name) {
                return self.class_implements_interface(&name, interface_name);
            }
            let Some((parent, implements)) = self.declared_class_ancestry.get(&name) else {
                return false;
            };
            if implements.iter().any(|implemented| {
                implemented == interface_name
                    || self.interface_extends_interface(implemented, interface_name)
            }) {
                return true;
            }
            current = parent.clone();
        }
        false
    }

    /// Returns true when `class_name` is `ancestor_name` or nominally extends or implements it.
    ///
    /// These are the relations for which `common_object_type` keeps the ancestor side, so an
    /// object slot typed as the ancestor can hold the class without changing its view. Generic
    /// variance and the non-nominal `object` type are deliberately excluded: EIR lowering keeps
    /// an indexed object element only for this same nominal relation
    /// (`ir_lower::stmt::array_write_storage`).
    pub(crate) fn object_is_a(&self, class_name: &str, ancestor_name: &str) -> bool {
        class_name == ancestor_name
            || self.is_subclass_of(class_name, ancestor_name)
            || (self.interfaces.contains_key(ancestor_name)
                && (self.class_implements_interface(class_name, ancestor_name)
                    || self.interface_extends_interface(class_name, ancestor_name)))
    }

    /// Returns true if `class_name` implements `interface_name`: from the built class's
    /// flattened interface list (parents and interface parents included), or, for a class the
    /// build walk has not reached yet, from its declared ancestry.
    pub(crate) fn class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        if !self.classes.contains_key(class_name) {
            return self.declared_class_implements_interface(class_name, interface_name);
        }
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
        self.class_implements_interface(type_name, interface_name)
    }

    /// Accepts ordinary ArrayAccess objects and reviewed internal dimension handlers.
    pub(crate) fn object_type_supports_dimensions(&self, type_name: &str) -> bool {
        crate::types::builtin_classes::has_native_dimensions(type_name, &self.classes)
            || self.object_type_implements_interface(type_name, "ArrayAccess")
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

        // The promoted property has no default expression, so this write types its slot, and
        // the backend builds the slot's reads and writes from it: an `array<int|null>` argument
        // (`new Holder($a)` after `$a[] = $n ?? null`) is stored with boxed elements, as every
        // other untyped slot update does (#1944). The parameter takes the same type so the
        // constructor body's `$this->items = $items` sees no change.
        let stored_ty = if param_is_ref {
            PhpType::Mixed
        } else {
            crate::types::box_tagged_scalar_array_elements(arg_ty.clone())
        };
        for sharer in self.property_sharers(&prop_name, &declaring_class) {
            let Some(class_info) = self.classes.get_mut(&sharer) else {
                continue;
            };

            let property_has_declared_type = class_info.visible_property_is_declared(&prop_name);
            if !property_has_declared_type {
                if let Some(slot) = class_info.visible_property_index(&prop_name) {
                    if let Some(prop) = class_info.properties.get_mut(slot) {
                        prop.1 = stored_ty.clone();
                    }
                }
            }

            // Sharing a property does not erase another constructor's declared contract or
            // make a different parameter position refer to this property's constructor input.
            let maps_same_property = class_info.constructor_param_to_prop.get(param_index)
                .and_then(|mapped| mapped.as_ref()) == Some(&prop_name);
            if !param_has_declared_type && maps_same_property {
                if let Some(sig) = class_info.methods.get_mut("__construct") {
                    if !sig.declared_params.get(param_index).copied().unwrap_or(false) {
                        if let Some((_, param_ty)) = sig.params.get_mut(param_index) {
                            *param_ty = stored_ty.clone();
                        }
                    }
                }
            }
        }
    }
}
