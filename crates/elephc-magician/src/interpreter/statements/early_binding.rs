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

use super::dispatch::declare_function;
use super::*;
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
                if !class_binds_early(class, &early, context, values)? { continue; }
                class.name()
            }
            EvalStmt::InterfaceDecl(interface) => {
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
            EvalStmt::ClassDecl(class) => execute_class_decl_stmt(class, context, scope, values)?,
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

/// Returns whether PHP links this class while compiling, given the classes bound so far.
fn class_binds_early(
    class: &EvalClass,
    early: &HashSet<String>,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !class.interfaces().is_empty()
        || !class.traits().is_empty()
        || class.methods().iter().any(|method| is_to_string(method.name()))
    {
        return Ok(false);
    }
    match class.parent() {
        None => Ok(true),
        Some(parent) => Ok(early.contains(&class_like_key(parent)) || class_like_exists(parent, context, values)?),
    }
}

/// Returns whether a method name is `__toString`, which makes its class or interface
/// implement `Stringable` and so keeps PHP from binding it early.
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
