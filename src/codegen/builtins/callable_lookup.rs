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

pub(super) enum FunctionLookup {
    Builtin(String),
    Extern(String),
    UserFunction(String),
    IncludeVariant(String),
}

pub(super) fn lookup_function(ctx: &Context, name: &str) -> Option<FunctionLookup> {
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
