//! Purpose:
//! Binds, before the first statement of a program runs, the top-level declarations PHP binds
//! when it compiles a file or an `eval()` fragment.
//!
//! Called from:
//! - `crate::interpreter::statements::dispatch::execute_program_statements()`.
//!
//! Key details:
//! - Every rule here was measured on PHP 8.5.12 with autoloading off.
//! - Functions are always bound. A class, interface or trait is bound only when its name is
//!   free and its shape is one PHP links at compile time.
//! - Binding evaluates no member: constants and property defaults are evaluated at their first
//!   use, as in PHP, so a declaration can be bound before the statements its initializers
//!   depend on.
//! - A class bound here copies its parent's default table as it is before the first statement
//!   runs (see `eval_record_declaration_snapshot`); one left to its statement copies it then.
//! - A child whose overrides can only be checked against its parent with a class that does
//!   not exist yet is left to its statement, where those classes may exist (`OverrideTypes`).

use super::dispatch::declare_function;
use super::*;
use crate::eval_ir::{EvalParameterType, EvalParameterTypeVariant};
use std::collections::HashSet;

/// Binds the top-level declarations of `statements` PHP binds at compile time and returns, per
/// statement, whether it is already declared, so execution and resumed jumps skip it.
///
/// PHP binds early:
/// - every function. A name that is taken is a compile error, reported before any statement
///   runs, so the failure is returned from here;
/// - a class with no interfaces, no traits and no `__toString` method (which adds `Stringable`)
///   whose parent, if any, is known at that point: bound earlier in the same list, or already
///   existing. A parent that cannot be extended fails here, as PHP fails while compiling;
/// - an interface with no parents and no `__toString` method, and a trait that uses no traits.
///
/// A `__toString` method PHP refuses to compile is a compile error too, whatever the rest of
/// its class or interface is, so it fails here before any statement runs.
///
/// A child is not bound when checking its overrides against its parent needs a class that does
/// not exist yet: PHP cannot decide then, and links the class when its statement runs.
///
/// A class-like whose name is taken is not bound: its statement reports the redeclaration if it
/// ever runs. Enums and every other shape are declared when their statement runs. Generic
/// templates are hoisted by the caller and only marked here. The result is empty when nothing
/// is marked, so a program with no declarations allocates nothing.
pub(super) fn bind_program_declarations(
    statements: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<bool>, EvalStatus> {
    let mut bound = Vec::new();
    let mut early: HashSet<String> = HashSet::new();
    for (index, stmt) in statements.iter().enumerate() {
        let name = match stmt {
            EvalStmt::GenericDecl(_) => {
                mark_bound(&mut bound, statements.len(), index);
                continue;
            }
            EvalStmt::FunctionDecl { .. } => {
                declare_function(stmt, context)?;
                mark_bound(&mut bound, statements.len(), index);
                continue;
            }
            EvalStmt::ClassDecl(class) => {
                if class_declares_stringable(class)? || !class_binds_early(class, &early, context, values)? {
                    continue;
                }
                class.name()
            }
            EvalStmt::InterfaceDecl(interface) => {
                validate_eval_interface_to_string(interface)?;
                let binds = interface.parents().is_empty()
                    && !interface.methods().iter().any(|method| is_to_string(method.name()));
                if !binds { continue; }
                interface.name()
            }
            EvalStmt::TraitDecl(trait_decl) => {
                if !trait_decl.traits().is_empty() { continue; }
                trait_decl.name()
            }
            _ => continue,
        };
        if class_like_exists(name, context, values)? { continue; }
        match stmt {
            EvalStmt::ClassDecl(class) => {
                if !bind_class(class, context, scope, values)? { continue; }
            }
            EvalStmt::InterfaceDecl(interface) => execute_interface_decl_stmt(interface, context, scope, values)?,
            EvalStmt::TraitDecl(trait_decl) => execute_trait_decl_stmt(trait_decl, context, scope, values)?,
            _ => {}
        }
        early.insert(class_like_key(name));
        mark_bound(&mut bound, statements.len(), index);
    }
    Ok(bound)
}

/// Marks statement `index` of a list of `len` statements as declared, sizing the flags on
/// first use.
fn mark_bound(bound: &mut Vec<bool>, len: usize, index: usize) {
    if bound.is_empty() { bound.resize(len, false); }
    bound[index] = true;
}

/// Binds a class PHP links while compiling, or returns `false` when PHP leaves it to its
/// statement because its overrides cannot be checked against its parent yet.
///
/// PHP checks every override silently before it binds a child. A check that needs a class it
/// cannot find is undecided, and the class is linked at run time instead. A subtype that does
/// not exist settles that at once. A supertype that does not exist only matters when the
/// check then fails, since the subtype may still match another member of a union. Any other
/// failure is PHP's compile error and is returned.
///
/// A class PHP would also reject for an unrelated reason is deferred with the others, and
/// fails when its statement runs.
fn bind_class(
    class: &EvalClass,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let types = override_types(class, context, values)?;
    if types == OverrideTypes::SubtypeMissing {
        return Ok(false);
    }
    match execute_class_decl_stmt(class, context, scope, values) {
        Ok(()) => Ok(true),
        Err(EvalStatus::RuntimeFatal)
            if types == OverrideTypes::SupertypeMissing && !context.has_class(class.name()) =>
        {
            Ok(false)
        }
        Err(status) => Err(status),
    }
}

/// How far the classes named by a child's overrides are known when it is checked against its
/// parent. Ordered from the most to the least decidable.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OverrideTypes {
    /// Every check can be decided with the classes that exist.
    Known,
    /// A type that must accept a known class names another class that does not exist.
    SupertypeMissing,
    /// A class that must be accepted by a different type does not exist.
    SubtypeMissing,
}

/// Classifies the declared types PHP compares when it checks `class` against its parent: the
/// return type of each overriding method against the inherited one, each inherited parameter
/// type against the overriding one, and a redeclared property's type both ways.
///
/// Only an eval-declared parent is inspected. Private members and concrete constructors carry
/// no contract, as in PHP.
fn override_types(
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<OverrideTypes, EvalStatus> {
    let Some(parent) = class.parent() else {
        return Ok(OverrideTypes::Known);
    };
    let mut types = OverrideTypes::Known;
    for method in class.methods() {
        let Some((_, inherited)) = context.class_method(parent, method.name()) else { continue };
        let unchecked = inherited.visibility() == EvalVisibility::Private
            || (method.name().eq_ignore_ascii_case("__construct") && !inherited.is_abstract());
        if unchecked { continue; }
        types = types.max(subtype_lookup(method.return_type(), inherited.return_type(), class, context, values)?);
        for (position, inherited_type) in inherited.parameter_types().iter().enumerate() {
            let own_type = method.parameter_types().get(position).and_then(Option::as_ref);
            types = types.max(subtype_lookup(inherited_type.as_ref(), own_type, class, context, values)?);
        }
    }
    for property in class.properties() {
        let Some((_, inherited)) = context.class_property(parent, property.name()) else { continue };
        if inherited.visibility() == EvalVisibility::Private { continue; }
        let (own_type, inherited_type) = (property.property_type(), inherited.property_type());
        types = types.max(subtype_lookup(own_type, inherited_type, class, context, values)?);
        types = types.max(subtype_lookup(inherited_type, own_type, class, context, values)?);
    }
    Ok(types)
}

/// Classifies one check that `subtype` is accepted by `supertype`, following php-src: a class
/// named on both sides needs no lookup, a builtin supertype other than `object` and `iterable`
/// decides without one, and anything else looks up the subtype class and the supertype classes.
fn subtype_lookup(
    subtype: Option<&EvalParameterType>,
    supertype: Option<&EvalParameterType>,
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<OverrideTypes, EvalStatus> {
    let (Some(subtype), Some(supertype)) = (subtype, supertype) else {
        return Ok(OverrideTypes::Known);
    };
    let mut types = OverrideTypes::Known;
    for variant in subtype.variants() {
        let EvalParameterTypeVariant::Class(name) = variant else { continue };
        if is_relative_class_name(name) { continue; }
        let (mut named, mut needs_lookup, mut supertype_missing) = (false, false, false);
        for candidate in supertype.variants() {
            match candidate {
                EvalParameterTypeVariant::Class(other) if class_like_key(other) == class_like_key(name) => named = true,
                EvalParameterTypeVariant::Class(other) => {
                    needs_lookup = true;
                    supertype_missing |= !type_class_exists(other, class, context, values)?;
                }
                EvalParameterTypeVariant::Object | EvalParameterTypeVariant::Iterable => needs_lookup = true,
                _ => {}
            }
        }
        if named || !needs_lookup { continue; }
        if !type_class_exists(name, class, context, values)? {
            return Ok(OverrideTypes::SubtypeMissing);
        }
        if supertype_missing { types = OverrideTypes::SupertypeMissing; }
    }
    Ok(types)
}

/// Returns whether a type names a class relative to the declaration: `self`, `parent`, `static`.
fn is_relative_class_name(name: &str) -> bool {
    ["self", "parent", "static"].iter().any(|relative| name.eq_ignore_ascii_case(relative))
}

/// Returns whether the class a declared type names exists, counting the class being bound and
/// the relative names, which always resolve.
fn type_class_exists(
    name: &str,
    class: &EvalClass,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if is_relative_class_name(name) || class_like_key(name) == class_like_key(class.name()) {
        return Ok(true);
    }
    class_like_exists(name, context, values)
}

/// Returns whether a class declares `__toString`, which makes it implement `Stringable` and so
/// keeps PHP from binding it early.
///
/// Fails for a `__toString` PHP refuses to compile: a signature it rejects, or an abstract one
/// in a class that is not abstract. PHP reports those before it decides how to bind the class.
fn class_declares_stringable(class: &EvalClass) -> Result<bool, EvalStatus> {
    let Some(method) = class.methods().iter().find(|method| is_to_string(method.name())) else {
        return Ok(false);
    };
    let rejected = (method.is_abstract() && !class.is_abstract())
        || eval_to_string_signature_is_rejected(
            method.is_static(),
            method.params(),
            method.parameter_is_variadic(),
            method.return_type(),
        );
    if rejected { Err(EvalStatus::RuntimeFatal) } else { Ok(true) }
}

/// Returns whether PHP links this class while compiling, given the classes bound so far. The
/// caller has already set aside a class that declares `__toString`.
fn class_binds_early(
    class: &EvalClass,
    early: &HashSet<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !class.interfaces().is_empty() || !class.traits().is_empty() {
        return Ok(false);
    }
    match class.parent() {
        None => Ok(true),
        Some(parent) => Ok(early.contains(&class_like_key(parent)) || class_like_exists(parent, context, values)?),
    }
}

/// Returns whether a method name is `__toString`.
fn is_to_string(name: &str) -> bool {
    name.eq_ignore_ascii_case("__toString")
}

/// Returns the case-insensitive key of a class-like name, without a leading namespace separator.
fn class_like_key(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Returns whether a class, interface, trait, enum or generic template of this name exists,
/// declared by eval code or known to the runtime. Nothing is autoloaded.
fn class_like_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = name.trim_start_matches('\\');
    Ok(context.generic_template(name).is_some()
        || context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
        || values.enum_exists(name)?)
}
