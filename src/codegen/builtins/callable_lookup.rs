//! Purpose:
//! Resolves string-literal function names used by callable/introspection builtins.
//! Shares PHP case-insensitive lookup between string-callback and introspection builtins.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::function_exists`
//! - `crate::codegen::builtins::types::is_callable`
//!
//! Key details:
//! - Include variants, externs, builtins, and user functions stay distinguishable so callers can choose the right lowering path.

use crate::codegen::context::Context;
use crate::names::php_symbol_key;
use crate::types::checker::builtins::canonical_builtin_function_name;

/// Discriminates include variants, externs, builtins, and user functions
/// so callers can pick the correct codegen lowering path.
///
/// Variants carry the canonical (original-case) function name as reported by PHP:
/// - `Builtin`: a PHP builtin, lowercased by `canonical_builtin_function_name`
/// - `Extern`: an `extern` declaration
/// - `UserFunction`: a user-defined function from any included file
/// - `IncludeVariant`: a function variant discovered inside an include context
pub(crate) enum FunctionLookup {
    Builtin(String),
    Extern(String),
    UserFunction(String),
    IncludeVariant(String),
}

/// Resolves a string-literal function name to a `FunctionLookup` variant.
///
/// Checks order: include variants → externs → user functions → builtins.
/// The first match wins; builtin lookup is case-insensitive via `canonical_builtin_function_name`.
/// Returns `None` if the name does not resolve to any known variant.
pub(crate) fn lookup_function(ctx: &Context, name: &str) -> Option<FunctionLookup> {
    if let Some(name) = lookup_folded(ctx.function_variant_groups.iter(), name) {
        return Some(FunctionLookup::IncludeVariant(name));
    }
    if let Some(name) = lookup_folded(ctx.extern_functions.keys(), name) {
        return Some(FunctionLookup::Extern(name));
    }
    if let Some(name) = lookup_folded(ctx.functions.keys(), name) {
        return Some(FunctionLookup::UserFunction(name));
    }
    canonical_builtin_function_name(name).map(FunctionLookup::Builtin)
}

/// Case-insensitive lookup over an iterable of names using PHP's symbol key.
///
/// `names` is any iterable of `String` candidates. `name` is the lookup key.
/// Both are compared via `php_symbol_key` (lowercased, unescaped) to emulate PHP's
/// case-insensitive function resolution. Returns the canonical (original-case) name
/// from `names` on the first match, or `None` if no candidate matches.
fn lookup_folded<'a, I>(names: I, name: &str) -> Option<String>
where
    I: IntoIterator<Item = &'a String>,
{
    let key = php_symbol_key(name);
    names
        .into_iter()
        .find(|candidate| php_symbol_key(candidate) == key)
        .cloned()
}
