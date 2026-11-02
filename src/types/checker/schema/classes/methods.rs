//! Purpose:
//! Validates class schema methods rules.
//! Owns one slice of class metadata construction used by object inference and method checking.
//!
//! Called from:
//! - `crate::types::checker::schema::classes`
//! - `crate::types::checker::schema::enums` for shared method shape validation.
//!
//! Key details:
//! - Class metadata is shared globally after construction, so validation must reject inconsistent inheritance early.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Visibility};
use crate::types::traits::FlattenedClass;

use super::super::super::Checker;
use super::super::validation::{
    build_method_sig, matches_global_builtin_attribute, method_contract_is_source,
    validate_override_signature, visibility_rank,
};
use super::state::ClassBuildState;
use super::{collect_attribute_args, collect_attribute_names};

/// Validates and registers all methods of a flattened class into the build state.
/// Enforces abstract/final modifiers, method body presence, and delegates to
/// `apply_static_method` or `apply_instance_method` based on `method.is_static`.
pub(super) fn apply_methods(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
) -> Result<(), CompileError> {
    for method in &class.methods {
        validate_method_shape(&class.name, method)?;
        // A generic method is a TEMPLATE and has no signature of its own: its parameter types
        // name type parameters that only a call site can bind, so resolving it here would report
        // `Unknown type: U`. Its instantiations are ordinary methods on this same class and go
        // through this same loop. Mirrors `resolve_unchecked_functions` for generic functions.
        if !method.type_params.is_empty() {
            continue;
        }
        if method.is_static {
            apply_static_method(state, class, checker, method)?;
        } else {
            apply_instance_method(state, class, checker, method)?;
        }
    }
    Ok(())
}

/// Validates shape constraints on a single method: abstract+final conflict,
/// abstract method with a body, non-abstract method without a body, and
/// private abstract methods are all rejected.
pub(in crate::types::checker::schema) fn validate_method_shape(
    class_name: &str,
    method: &ClassMethod,
) -> Result<(), CompileError> {
    if method.is_abstract && method.is_final {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot use the final modifier on an abstract method: {}::{}",
                class_name, method.name
            ),
        ));
    }
    if method.is_abstract && method.has_body {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Abstract method cannot have a body: {}::{}",
                class_name, method.name
            ),
        ));
    }
    if !method.is_abstract && !method.has_body {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Non-abstract method must have a body: {}::{}",
                class_name, method.name
            ),
        ));
    }
    if method.is_abstract && method.visibility == Visibility::Private {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Private abstract methods are not supported: {}::{}",
                class_name, method.name
            ),
        ));
    }
    Ok(())
}

/// Validates and registers a static method into `ClassBuildState`. Checks for
/// final method conflicts, static/instance kind conflicts, visibility reduction,
/// signature override compatibility, `#[Override]` attribute targets, and registers
/// the method in the static vtable when visibility is non-private.
fn apply_static_method(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    method: &ClassMethod,
) -> Result<(), CompileError> {
    let method_key = php_symbol_key(&method.name);
    let mut sig = build_method_sig(checker, method, &class.name)?;
    if class.name == "WeakReference" && method_key == "create" && method.span == crate::span::Span::dummy() {
        // Box the generic object boundary so native Closure descriptors keep their runtime tag.
        sig.params[0].1 = crate::types::PhpType::Mixed;
    }
    if state.final_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if state.method_sigs.contains_key(&method_key) {
        return Err(method_kind_error(class, method));
    }
    if state.final_static_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .static_method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if let Some(parent_visibility) = state.static_method_visibilities.get(&method_key) {
        if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Cannot reduce visibility when overriding static method: {}::{}",
                    class.name, method.name
                ),
            ));
        }
    }
    if let Some(parent_sig) = state.static_sigs.get(&method_key) {
        let parent_is_source = state
            .static_method_declaring_classes
            .get(&method_key)
            .is_none_or(|owner| method_contract_is_source(checker, owner, &method_key, true));
        validate_override_signature(
            checker,
            class,
            method,
            parent_sig,
            state
                .late_static_static_method_returns
                .get(&method_key),
            true,
            parent_is_source,
        )?;
    } else if has_override_attribute(method)
        && !interface_declares_method(checker, state, class, &method_key, true)
    {
        return Err(missing_override_target(class, method));
    }
    if method.is_abstract && state.static_method_impl_classes.contains_key(&method_key) {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot make concrete static method abstract: {}::{}",
                class.name, method.name
            ),
        ));
    }
    state.static_sigs.insert(method_key.clone(), sig);
    if let Some(return_type) = method
        .return_type
        .as_ref()
        .filter(|return_type| return_type.contains_late_static())
    {
        state
            .late_static_static_method_returns
            .insert(method_key.clone(), return_type.clone());
    } else {
        state
            .late_static_static_method_returns
            .remove(&method_key);
    }
    state
        .static_method_visibilities
        .insert(method_key.clone(), method.visibility.clone());
    if method.is_final {
        state.final_static_methods.insert(method_key.clone());
    } else {
        state.final_static_methods.remove(&method_key);
    }
    state
        .static_method_declaring_classes
        .insert(method_key.clone(), class.name.clone());
    state
        .method_attribute_names
        .insert(method_key.clone(), collect_attribute_names(&method.attributes));
    state
        .method_attribute_args
        .insert(method_key.clone(), collect_attribute_args(&method.attributes));
    if method.is_abstract {
        state.static_method_impl_classes.remove(&method_key);
        state.abstract_static_methods.insert(method_key.clone());
    } else {
        state
            .static_method_impl_classes
            .insert(method_key.clone(), class.name.clone());
        state.abstract_static_methods.remove(&method_key);
    }
    if method.visibility != Visibility::Private
        && !method_key.contains('<')
        && !state.static_vtable_slots.contains_key(&method_key)
    {
        // Same rule as the instance path, and for the same reason: an instantiated generic
        // static method is named exactly by its call site, so it needs no slot — and giving it
        // one lets two instantiations of a class number their statics differently.
        let slot = state.static_vtable_methods.len();
        state.static_vtable_slots.insert(method_key.clone(), slot);
        state.static_vtable_methods.push(method_key);
    }
    Ok(())
}

/// Validates and registers an instance method into `ClassBuildState`. Checks for
/// final method conflicts, static/instance kind conflicts, visibility reduction,
/// signature override compatibility, `#[Override]` attribute targets, and registers
/// the method in the instance vtable when visibility is non-private.
fn apply_instance_method(
    state: &mut ClassBuildState,
    class: &FlattenedClass,
    checker: &Checker,
    method: &ClassMethod,
) -> Result<(), CompileError> {
    let method_key = php_symbol_key(&method.name);
    let internal_exception_override = injected_exception_final_override(class, method, &method_key);
    let inherited_declaring_class = state.method_declaring_classes.get(&method_key).cloned();
    let mut sig = build_method_sig(checker, method, &class.name)?;
    demote_array_access_offset_get_reference(class, &method_key, &mut sig);
    let serializable = state.interfaces.iter().chain(class.implements.iter()).any(|name|
        name.eq_ignore_ascii_case("Serializable") || checker.interface_extends_interface(name, "Serializable"));
    if serializable && method_key == "unserialize"
        && method.params.first().is_some_and(|(_, ty, _, _)| ty.is_none())
    {
        if let Some((_, ty)) = sig.params.first_mut() { *ty = crate::types::PhpType::Mixed; }
    }
    if state.final_static_methods.contains(&method_key) {
        return Err(final_method_error(
            state
                .static_method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    if state.static_sigs.contains_key(&method_key) {
        return Err(method_kind_error(class, method));
    }
    if state.final_methods.contains(&method_key) && !internal_exception_override {
        return Err(final_method_error(
            state
                .method_declaring_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class.name.clone()),
            method,
        ));
    }
    // PHP exempts constructors from ordinary override compatibility: a child may
    // deliberately reduce constructor visibility (PDOStatement subclasses are the
    // canonical internal-API example). Signature validation already carries the same
    // `__construct` exemption; keep visibility validation aligned with it.
    if method_key != "__construct" {
        if let Some(parent_visibility) = state.method_visibilities.get(&method_key) {
            if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot reduce visibility when overriding method: {}::{}",
                        class.name, method.name
                    ),
                ));
            }
        }
    }
    if let Some(parent_sig) = state.method_sigs.get(&method_key) {
        if !internal_exception_override {
            let parent_is_source = state
                .method_declaring_classes
                .get(&method_key)
                .is_none_or(|owner| method_contract_is_source(checker, owner, &method_key, false));
            validate_override_signature(
                checker,
                class,
                method,
                parent_sig,
                state.late_static_method_returns.get(&method_key),
                false,
                parent_is_source,
            )?;
        }
    } else if has_override_attribute(method)
        && !interface_declares_method(checker, state, class, &method_key, false)
    {
        return Err(missing_override_target(class, method));
    }
    if method.is_abstract && state.method_impl_classes.contains_key(&method_key) {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot make concrete method abstract: {}::{}",
                class.name, method.name
            ),
        ));
    }
    // A constructor is not an override. PHP exempts `__construct` from signature
    // compatibility entirely — `validate_override_signature` returns early for it above — and
    // nothing ever enters a constructor through an ancestor's entry point: `new C(...)` calls
    // C's, `parent::__construct(...)` calls the parent's. Adopting ancestor storage here would
    // stamp the parent's parameter TYPES onto an untyped child parameter, which PHP reads as
    // `mixed`: `Twig\Node::__construct(array $nodes = [], ...)` turned every
    // `ConstantExpression::__construct($value, int $lineno)` call into "parameter $value
    // expects Array(Mixed)".
    // A BUILTIN ancestor's declaration is not adopted: an untyped override of
    // `DateTime::add(DateInterval $interval)` (Carbon's `add($unit, $value = 1, ...)`) is called
    // directly with strings and ints, which the adopted `DateInterval` storage refused, and the
    // whole Carbon class was left to the interpreter. Ancestor-typed sites still reach such an
    // override with the right representation through the class-id ladder
    // (`override_arity::wider_override_classes`, `stores_site_params_differently`).
    let ancestor_is_source = state
        .method_declaring_classes
        .get(&method_key)
        .is_none_or(|owner| method_contract_is_source(checker, owner, &method_key, false));
    if method_key != "__construct" && ancestor_is_source {
        if let Some(parent_sig) = state.method_sigs.get(&method_key) {
            adopt_ancestor_parameter_storage(&mut sig, parent_sig);
            adopt_ancestor_return_storage(&mut sig, parent_sig);
        }
    } else if method_key != "__construct" {
        if let Some(parent_sig) = state.method_sigs.get(&method_key) {
            widen_undeclared_override_parameters(&mut sig, parent_sig);
        }
    }
    state.method_sigs.insert(method_key.clone(), sig);
    if let Some(return_type) = method
        .return_type
        .as_ref()
        .filter(|return_type| return_type.contains_late_static())
    {
        state
            .late_static_method_returns
            .insert(method_key.clone(), return_type.clone());
    } else {
        state.late_static_method_returns.remove(&method_key);
    }
    state
        .method_visibilities
        .insert(method_key.clone(), method.visibility.clone());
    if method.is_final || internal_exception_override {
        state.final_methods.insert(method_key.clone());
    } else {
        state.final_methods.remove(&method_key);
    }
    let declaring_class = if internal_exception_override {
        inherited_declaring_class.unwrap_or_else(|| class.name.clone())
    } else {
        class.name.clone()
    };
    state
        .method_declaring_classes
        .insert(method_key.clone(), declaring_class);
    state
        .method_attribute_names
        .insert(method_key.clone(), collect_attribute_names(&method.attributes));
    state
        .method_attribute_args
        .insert(method_key.clone(), collect_attribute_args(&method.attributes));
    if method.is_abstract {
        state.method_impl_classes.remove(&method_key);
        state.abstract_methods.insert(method_key.clone());
    } else {
        state
            .method_impl_classes
            .insert(method_key.clone(), class.name.clone());
        state.abstract_methods.remove(&method_key);
    }
    // An INSTANTIATED generic method takes no slot, and that is a soundness rule rather than an
    // optimization. Its call site names it exactly — that is what instantiation means — so there
    // is nothing to dispatch on. And giving it one breaks the invariant two instantiations of a
    // class must keep: `Pair<int>` may be called with `withRight<string>` where `Pair<string>`
    // never is, so the slot after it lands at a different number in each, and a variance
    // widening takes the slot from one class and indexes the other's table.
    // `assert_instantiation_vtable_slots_aligned` fails loudly if this ever stops holding.
    let is_instantiation = method_key.contains('<');
    if method.visibility != Visibility::Private
        && !is_instantiation
        && !state.vtable_slots.contains_key(&method_key)
    {
        let slot = state.vtable_methods.len();
        state.vtable_slots.insert(method_key.clone(), slot);
        state.vtable_methods.push(method_key);
    }
    Ok(())
}

/// Constructs a `CompileError` for overriding a `final` method.
/// Allows the compiler-injected PDOException getCode body to model its internal SQLSTATE
/// behavior while retaining the inherited final flag and declaring class. Source declarations
/// with the same class or method name never receive this exception.
fn injected_exception_final_override(
    class: &FlattenedClass,
    method: &ClassMethod,
    method_key: &str,
) -> bool {
    class.span.line == 0
        && class.span.col == 0
        && method.span.line == 0
        && method.span.col == 0
        && php_symbol_key(&class.name) == "pdoexception"
        && method_key == "getcode"
}

/// Builds the diagnostic for an attempt to override the selected final method.
fn final_method_error(declaring_class: String, method: &ClassMethod) -> CompileError {
    CompileError::new(
        method.span,
        &format!(
            "Cannot override final method {}::{}",
            declaring_class, method.name
        ),
    )
}

/// Constructs a `CompileError` for attempting to change a static method to
/// instance or vice versa when overriding a parent method.
fn method_kind_error(class: &FlattenedClass, method: &ClassMethod) -> CompileError {
    CompileError::new(
        method.span,
        &format!(
            "Cannot change method kind when overriding {}::{}",
            class.name, method.name
        ),
    )
}

/// Returns `true` if the method carries the PHP 8.3 `#[\Override]` marker
/// attribute (in any group, in either qualified form). Match is case-insensitive
/// to mirror PHP's class-name lookup rules.
fn has_override_attribute(method: &ClassMethod) -> bool {
    method.attributes.iter().any(|group| {
        group
            .attributes
            .iter()
            .any(|attr| matches_global_builtin_attribute(attr, "Override"))
    })
}

/// Returns `true` if any interface implemented by the class (directly or
/// transitively via parent interfaces or inherited parent-class contracts)
/// declares the method with the requested static/instance kind.
///
/// Seeds from `class.implements` because `apply_methods` runs before
/// `collect_interfaces` has added the class's own clause to `state.interfaces`;
/// also scans `state.interfaces`, which already carries interfaces inherited
/// from the parent class chain.
fn interface_declares_method(
    checker: &Checker,
    state: &ClassBuildState,
    class: &FlattenedClass,
    method_key: &str,
    is_static: bool,
) -> bool {
    let mut visited = std::collections::HashSet::new();
    let mut queue: Vec<String> = class.implements.clone();
    queue.extend(state.interfaces.iter().cloned());
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(info) = checker.interfaces.get(&name) else {
            continue;
        };
        let declares_method = if is_static {
            info.static_methods.contains_key(method_key)
        } else {
            info.methods.contains_key(method_key)
        };
        if declares_method {
            return true;
        }
        queue.extend(info.parents.iter().cloned());
    }
    false
}

/// Constructs a `CompileError` when a method carries `#[\Override]` but no
/// matching parent method exists to override.
fn missing_override_target(class: &FlattenedClass, method: &ClassMethod) -> CompileError {
    CompileError::new(
        method.span,
        &format!(
            "{}::{}() has #[\\Override] attribute, but no matching parent method was found",
            class.name, method.name
        ),
    )
}

/// Gives an override the RETURN storage of the ancestor its vtable slot belongs to.
///
/// The return-side twin of [`adopt_ancestor_parameter_storage`]. A virtual call typed against the
/// ancestor reads the result in the ancestor's representation, but PHP lets an override NARROW its
/// return type, and the override's frame returned in its own: `FileLoader::import()` calls
/// `$loader->load()` on a receiver typed `FileLoader`, whose `load()` comes from
/// `LoaderInterface::load(): mixed`, and the routing `YamlFileLoader::load(): RouteCollection`
/// answered with a raw object pointer where a boxed cell belongs -- read back as `null`. Every
/// route file imported through a glob vanished.
///
/// Covariance is what makes the wider storage safe: whatever the override returns satisfies the
/// ancestor's declaration too. The cost is precision for a caller typed at the override itself,
/// which now sees the ancestor's type. Only a change of REPRESENTATION is adopted; a narrowing that
/// keeps the storage (`?Foo` to `Foo`, one object type to another) is left exactly as declared.
fn adopt_ancestor_return_storage(
    sig: &mut crate::types::FunctionSig,
    ancestor: &crate::types::FunctionSig,
) {
    if !ancestor.declared_return || sig.by_ref_return != ancestor.by_ref_return {
        return;
    }
    let ancestor_repr = ancestor.return_type.codegen_repr();
    if !matches!(ancestor_repr, crate::types::PhpType::Mixed | crate::types::PhpType::Union(_)) {
        return;
    }
    if sig.return_type.codegen_repr() == ancestor_repr {
        return;
    }
    sig.return_type = ancestor.return_type.clone();
}

/// Gives an override's UNDECLARED parameters the storage the ancestor declared for them.
///
/// A virtual call site is typed against the ancestor: `$container->load($file)` on a receiver
/// typed `Container` materializes whatever `Container::load(string $file)` declares — a raw
/// string. If the override writes `load($file)` with no type, its own frame expects a boxed
/// `mixed`, and the raw string arrives where a cell pointer belongs. Symfony's generated DI
/// container is exactly this pair, and the argument came back EMPTY: every service lookup asked
/// the container to `require` a file whose name it had lost.
///
/// PHP's contravariance is what makes adopting the ancestor's type safe: an override may only
/// WIDEN a parameter, so every value that can reach it already satisfies the ancestor's
/// declaration. Only undeclared parameters are touched — an override that declares its own type
/// has already been checked against the ancestor by `validate_override_signature`, and its
/// declaration is the one its body was checked against.
///
/// An ancestor's VARIADIC parameter is never adopted: its storage is the packed array of every
/// surplus argument, not the value of the override's parameter at that position. Twig's
/// `Node::getAttribute(string $name)` reads `func_get_arg(1)`, so it carries the hidden variadic
/// tail in position 1, and a trait override `getAttribute($name, $default = null)` took that
/// array storage for `$default`.
fn adopt_ancestor_parameter_storage(
    sig: &mut crate::types::FunctionSig,
    ancestor: &crate::types::FunctionSig,
) {
    for (index, (name, ancestor_ty)) in ancestor.params.iter().enumerate() {
        if ancestor.variadic.as_deref() == Some(name.as_str()) {
            break;
        }
        if sig.declared_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        if !ancestor.declared_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some((_, param_ty)) = sig.params.get_mut(index) else {
            continue;
        };
        *param_ty = ancestor_ty.clone();
        // Marked DECLARED as well: the storage is now the ancestor's contract, and an
        // undeclared parameter is otherwise promoted to a boxed cell by the gradual-storage
        // pass — which would put the mismatch straight back.
        if let Some(declared) = sig.declared_params.get_mut(index) {
            *declared = true;
        }
    }
}

/// Declares `mixed` every untyped override parameter whose BUILTIN ancestor declares a type.
///
/// The parameter is reached both by direct calls on the subclass (Carbon's `add('day', 3)`) and,
/// through the class-id ladder, by sites typed against the builtin (`DateTime::add($interval)`).
/// Inferring it from the direct calls alone fabricated `string`, and the `DateInterval` branch
/// of the same body was refused. PHP reads an untyped parameter as `mixed`, which holds both.
fn widen_undeclared_override_parameters(
    sig: &mut crate::types::FunctionSig,
    ancestor: &crate::types::FunctionSig,
) {
    for (index, (name, _)) in ancestor.params.iter().enumerate() {
        if ancestor.variadic.as_deref() == Some(name.as_str()) {
            break;
        }
        if sig.declared_params.get(index).copied().unwrap_or(true)
            || !ancestor.declared_params.get(index).copied().unwrap_or(false)
            || sig.ref_params.get(index).copied().unwrap_or(false)
        {
            continue;
        }
        if let Some((_, param_ty)) = sig.params.get_mut(index) {
            *param_ty = crate::types::PhpType::Mixed;
        }
        if let Some(declared) = sig.declared_params.get_mut(index) {
            *declared = true;
        }
    }
}

/// Returns `&offsetGet()` of an `ArrayAccess` class by value.
///
/// `ArrayAccess::offsetGet()` is declared by value, and every `$obj[$k]` read dispatches through
/// that contract; a reference-returning implementation (Monolog's `LogRecord::&offsetGet()`)
/// would hand those call sites a reference cell where they expect the value, which the method
/// ABI adapter cannot bridge. Compiling the implementation by value keeps every read correct;
/// only writes through the returned reference (`$rec['extra']['k'] = …`) stay local.
fn demote_array_access_offset_get_reference(
    class: &FlattenedClass,
    method_key: &str,
    sig: &mut crate::types::FunctionSig,
) {
    if sig.by_ref_return
        && method_key == "offsetget"
        && class
            .implements
            .iter()
            .any(|name| name.trim_start_matches('\\').eq_ignore_ascii_case("ArrayAccess"))
    {
        sig.by_ref_return = false;
    }
}
