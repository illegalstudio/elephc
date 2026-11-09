//! Purpose:
//! Checks type compatibility for object types cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::fast_hash::FastSet as HashSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, Visibility};
use crate::types::{EnumInfo, PhpType, TypeEnv};

use super::super::Checker;

/// Reports whether a type can hold an object at runtime despite lacking a concrete object type.
///
/// Concrete objects and callables are object-family values in PHP. Mixed values and unions that
/// contain one of those members defer their actual object contract to the runtime boundary.
pub(crate) fn type_is_gradual_object_family(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Callable | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(|member| {
            matches!(
                member,
                PhpType::Object(_) | PhpType::Callable | PhpType::Mixed
            )
        }),
        _ => false,
    }
}

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
    /// Reports whether a direct construction names a class deliberately left for runtime lookup.
    ///
    /// Function-like scopes allow unresolved nominal classes because executing the construction
    /// raises the ordinary catchable class-not-found `Error`. A surrounding `throw` must not
    /// reject that expression earlier: construction either yields a real runtime object or never
    /// reaches the throw boundary.
    pub(crate) fn unresolved_new_object_defers_to_runtime(
        &self,
        expr: &Expr,
        type_name: &str,
    ) -> bool {
        if self.classes.contains_key(type_name) || self.interfaces.contains_key(type_name) {
            return false;
        }
        let ExprKind::NewObject { class_name, .. } = &expr.kind else {
            return false;
        };
        self.allows_absent_runtime_class()
            && class_name
                .as_str()
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(type_name.trim_start_matches('\\'))
    }

    /// Returns whether `type_name` names a class-like THE CLOSED WORLD DOES NOT HAVE, in a context
    /// that is allowed to leave an absent class to PHP's runtime.
    ///
    /// [`Checker::unresolved_new_object_defers_to_runtime`] answers the same question, but only
    /// when the expression at hand is the `new` itself. A declared return type carries the name
    /// just as far: `AbstractController::denyAccessUnlessGranted` does
    /// `$e = $this->createAccessDeniedException(...); ...; throw $e;`, and
    /// `createAccessDeniedException(): AccessDeniedException` names a class that only ships with
    /// `symfony/security-core`. Without this the checker decided a class it has never seen does not
    /// implement `Throwable` and refused to compile the file — for a branch that is guarded by
    /// `class_exists()` and, in an app without that package, never runs.
    pub(crate) fn absent_class_defers_to_runtime(&self, type_name: &str) -> bool {
        // An instantiation (`Box<Animal>`) is an elephc-compiled class, never one a runtime
        // autoloader supplies: an instantiation the program never constructs is simply not in the
        // table, and treating it as absent would hide an invariant mismatch from the checker.
        if type_name.is_empty() || type_name.contains('<') {
            return false;
        }
        if self.classes.contains_key(type_name) || self.interfaces.contains_key(type_name) {
            return false;
        }
        self.allows_absent_runtime_class()
    }

    /// Checks whether the current class context can access a member with the given visibility
    /// declared in `declaring_class`. Public members are always accessible; protected members
    /// are accessible throughout the same inheritance family; private members are only
    /// accessible if the current class is exactly the declaring class.
    pub(crate) fn can_access_member(
        &self,
        declaring_class: &str,
        visibility: &Visibility,
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Protected => self.current_class.as_deref().is_some_and(|current| {
                current == declaring_class
                    || self.is_subclass_of(current, declaring_class)
                    || self.is_subclass_of(declaring_class, current)
            }),
            Visibility::Private => self.current_class.as_deref() == Some(declaring_class),
        }
    }

    /// Whether `[$object, 'method']` names an instance method the class being checked cannot reach.
    ///
    /// php does not reject such a callable when it is written: `call_user_func()` and the other
    /// callback builtins validate it when they run and throw a `TypeError`, or hand the call to
    /// `__call()`. The checker therefore must not bind the pair to the method as though it were
    /// callable here; a callable it leaves unresolved takes the runtime callable-array path,
    /// which raises php's error. A method the class does not hold at all stays "reachable" so the
    /// existing undefined-method handling is untouched.
    pub(crate) fn instance_callable_method_unreachable(&self, class_name: &str, method: &str) -> bool {
        let method_key = crate::names::php_symbol_key(method);
        let Some(class_info) = self.classes.get(class_name) else {
            return false;
        };
        let Some(visibility) = class_info.method_visibilities.get(&method_key) else {
            return false;
        };
        let declaring_class = class_info
            .method_declaring_classes
            .get(&method_key)
            .map(String::as_str)
            .unwrap_or(class_name);
        !self.can_access_member(declaring_class, visibility)
    }

    /// Returns the string label ("public", "protected", "private") for a visibility level.
    pub(crate) fn visibility_label(visibility: &Visibility) -> &'static str {
        match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }

    /// Returns true when no runtime object can belong to both concrete class families.
    pub(crate) fn classes_are_instanceof_incompatible(&self, a: &str, b: &str) -> bool {
        let a = a.trim_start_matches('\\');
        let b = b.trim_start_matches('\\');
        if a == b || !self.classes.contains_key(a) || !self.classes.contains_key(b) {
            return false;
        }
        !self.is_subclass_of(a, b) && !self.is_subclass_of(b, a)
    }

    /// Returns true if `class_name` inherits from `ancestor_name` (excluding self equality).
    ///
    /// Prefers complete class metadata and falls back to the declaration graph while a sibling
    /// class is still being constructed. A hop bound makes malformed inheritance cycles harmless
    /// to diagnostics: a chain longer than every known class can only be a cycle.
    pub(crate) fn is_subclass_of(&self, class_name: &str, ancestor_name: &str) -> bool {
        #[cfg(test)]
        SUBCLASS_QUERIES.with(|count| count.set(count.get() + 1));
        // Borrowed names instead of an owned parent and a visited set per call: post-fixpoint
        // passes ask this for every site against every class (most of the checker's hashing).
        let parent_of = |name: &str| -> Option<&str> {
            match self.classes.get(name) {
                Some(class) if class.parent.is_some() => class.parent.as_deref(),
                _ => self.declared_class_parents.get(name).and_then(|parent| parent.as_deref()),
            }
        };
        let bound = self.classes.len() + self.declared_class_parents.len() + 1;
        let mut current = parent_of(class_name);
        let mut hops = 0usize;
        while let Some(parent_name) = current {
            if parent_name == ancestor_name {
                return true;
            }
            hops += 1;
            if hops > bound {
                break;
            }
            current = parent_of(parent_name);
        }
        false
    }

    /// Returns the class whose private declaration answers a call to `method_key` on a receiver
    /// typed `class_name`. See [`crate::types::private_scope`] for the rule and its measurement.
    pub(crate) fn lexical_private_method_scope(
        &self,
        class_name: &str,
        method_key: &str,
    ) -> Option<String> {
        crate::types::private_scope::lexical_private_method_scope(
            &self.classes,
            self.current_class.as_deref(),
            class_name,
            method_key,
        )
    }

    /// Returns true if `class_name` directly implements `interface_name` (not via inheritance).
    pub(crate) fn class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        self.classes
            .get(class_name)
            .map(|class_info| class_info.interfaces.as_slice())
            .or_else(|| {
                self.declared_class_interfaces
                    .get(class_name)
                    .map(Vec::as_slice)
            })
            .is_some_and(|interfaces| interfaces.iter().any(|name| name == interface_name))
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

    /// Returns true when a value statically typed `type_name`, which does not itself implement
    /// `ArrayAccess`, can still be an `ArrayAccess` object at run time.
    ///
    /// A parameter typed with a CONTRACT (`Illuminate\Contracts\Container\Container`) or with a
    /// non-final base class says nothing about the object behind it: the concrete class that
    /// implements the contract is the one that adds `ArrayAccess`, and php indexes it without
    /// complaint. The read is then a run-time dispatch, exactly like a `mixed` receiver; only
    /// a type no `ArrayAccess` class can inhabit (a final class, or an interface/class none of
    /// whose known implementers is indexable) keeps the compile-time refusal. The program's class
    /// set is closed unless it can load classes the compiler never saw.
    pub(crate) fn object_type_may_gain_array_access(&self, type_name: &str) -> bool {
        let type_name = type_name.trim_start_matches('\\');
        if self.object_type_implements_interface(type_name, "ArrayAccess") {
            return true;
        }
        // The bare `object` type (an empty class name) names no contract at all: any object,
        // `ArrayAccess` implementers included, can sit behind it. A property declared
        // `private object $component` and read as `$this->component[$name]` under
        // `$this->component instanceof \ArrayAccess && isset(...)` (Symfony UX
        // `ComputedPropertiesProxy`) reads through the same boxed run-time dispatch.
        if type_name.is_empty() {
            return true;
        }
        let open_world = self.program_defers_unknown_classes || self.eval_barrier_active;
        if self.interfaces.contains_key(type_name) {
            return open_world
                || self.classes.iter().any(|(name, info)| {
                    self.class_implements_interface(name, "ArrayAccess")
                        && info.interfaces.iter().any(|interface| {
                            interface == type_name
                                || self.interface_extends_interface(interface, type_name)
                        })
                });
        }
        let Some(info) = self.classes.get(type_name) else {
            // A type nothing in the program declares (an uninstalled package's: Laravel's
            // `QueueConnector` indexes `$command['QueueUrl']` on the AWS SDK's
            // `CommandInterface`, which the skeleton does not install) names no contract to refuse
            // the read with; the run-time object decides, as for `object`.
            return true;
        };
        if info.is_final {
            return false;
        }
        open_world
            || self.classes.keys().any(|name| {
                name != type_name
                    && self.is_subclass_of(name, type_name)
                    && self.class_implements_interface(name, "ArrayAccess")
            })
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
        let mut seen = HashSet::default();
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
        if self.unresolved_catch_types.contains(type_name) {
            return true;
        }
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

        let mut left_ancestors = HashSet::default();
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
    /// `Checker::property_sharers_index`). This runs for every constructor argument of every
    /// `new`, so the (property, declaring class) pairs are indexed once per class-table size
    /// instead of scanning every class per argument.
    fn property_sharers(&mut self, prop_name: &str, declaring_class: &str) -> Vec<String> {
        let class_count = self.classes.len();
        if self
            .property_sharers_index
            .as_ref()
            .is_none_or(|(built_for, _)| *built_for != class_count)
        {
            let mut index: crate::fast_hash::FastMap<(String, String), Vec<String>> =
                crate::fast_hash::FastMap::default();
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

    /// Propagates a resolved constructor argument type into the corresponding property slot for
    /// every class that shares the inherited property, and into constructor parameters that map
    /// to that same property. Declared parameters and unrelated constructor positions remain
    /// unchanged.
    pub(crate) fn propagate_constructor_arg_type(
        &mut self,
        instantiated_class: &str,
        param_index: usize,
        arg_ty: &PhpType,
    ) {
        let param_is_ref = self
            .classes
            .get(instantiated_class)
            .and_then(|class_info| class_info.methods.get("__construct"))
            .and_then(|sig| sig.ref_params.get(param_index))
            .copied()
            .unwrap_or(false);
        // An untyped parameter the constructor tests with `instanceof` stays `mixed`, as it does
        // for methods (`keep_instanceof_tested_params_gradual`). Guzzle's `Request::__construct`
        // does `if (!$uri instanceof UriInterface) { $uri = new Uri($uri); }`: one `new` passed a
        // `Uri`, the parameter became `Uri`, and `new Uri($uri)` was refused in the body.
        let param_is_instanceof_tested =
            self.instanceof_tested_param(instantiated_class, "__construct", param_index);
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
        // Every `new C(...)` used to OVERWRITE the property's type with its own argument's, so the
        // last call site checked won: `new A(new A([1]))` typed `$x` as `A` (the outer call is
        // checked after the inner one) and the inner array then failed the property's nominal guard,
        // a TypeError php never raises for an untyped property. A later argument now keeps the type
        // when compatible and widens to `Mixed` otherwise, the type an untyped property holds.
        // A DECLARED property type is not a guess to widen: the argument keeps its own type, so
        // `$this->value = $value` is still checked against `int $value` (php's TypeError).
        let declared = self
            .classes
            .get(&declaring_class)
            .is_some_and(|class_info| class_info.visible_property_is_declared(&prop_name));
        let current = self
            .classes
            .get(&declaring_class)
            .and_then(|class_info| {
                class_info
                    .visible_property_index(&prop_name)
                    .and_then(|slot| class_info.properties.get(slot))
                    .map(|prop| prop.1.clone())
            })
            .unwrap_or(PhpType::Void);
        let joined = if declared || current == PhpType::Void || current == *arg_ty {
            arg_ty.clone()
        } else if self.type_accepts(&current, arg_ty) {
            current.clone()
        } else {
            PhpType::Mixed
        };
        let arg_ty = &joined;

        for sharer in self.property_sharers(&prop_name, &declaring_class) {
            #[cfg(test)]
            crate::types::checker::SHARED_PROPERTY_CLASSES_VISITED.with(|count| count.set(count.get() + 1));
            let Some(class_info) = self.classes.get_mut(&sharer) else {
                continue;
            };

            let property_has_declared_type = class_info.visible_property_is_declared(&prop_name);
            if !property_has_declared_type {
                if let Some(slot) = class_info.visible_property_index(&prop_name) {
                    if let Some(prop) = class_info.properties.get_mut(slot) {
                        prop.1 = if param_is_ref {
                            PhpType::Mixed
                        } else {
                            arg_ty.clone()
                        };
                    }
                }
            }

            let constructor_maps_same_property = class_info
                .constructor_param_to_prop
                .get(param_index)
                .and_then(|mapped| mapped.as_ref())
                .is_some_and(|mapped| mapped == &prop_name);
            let constructor_param_is_declared = class_info
                .methods
                .get("__construct")
                .and_then(|sig| sig.declared_params.get(param_index))
                .copied()
                .unwrap_or(false);

            if constructor_maps_same_property
                && !constructor_param_is_declared
                && !param_is_instanceof_tested
            {
                if let Some(sig) = class_info.methods.get_mut("__construct") {
                    if !sig.declared_params.get(param_index).copied().unwrap_or(false) {
                        if let Some((_, param_ty)) = sig.params.get_mut(param_index) {
                            *param_ty = if param_is_ref {
                                PhpType::Mixed
                            } else {
                                arg_ty.clone()
                            };
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
std::thread_local! {
    /// `Checker::is_subclass_of` questions asked on this test thread.
    pub(crate) static SUBCLASS_QUERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
