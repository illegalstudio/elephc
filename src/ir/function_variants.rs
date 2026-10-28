//! Purpose:
//! Canonical helpers for PHP function variant groups (include-loaded overloads).
//! Provides parse, collection over a Module, and resolution from FunctionVariantRef
//! indices to concrete callee names/functions using the authoritative
//! `module.functions` table and `normalized_function_key` (php_symbol_key).
//!
//! Called from:
//! - `crate::codegen::function_variants` (for emission/dispatch)
//! - `crate::ir_passes::inline` for FVC inlining decisions (via the variant resolvers).
//!
//! Key details:
//! - Must stay in sync with resolver lowering of FunctionVariantMark/Group.
//! - Uses normalized case-insensitive keys for matching.
//! - Resolution for {group, variant} indexes into the collected dispatch groups
//!   (order determined by discovery over data.strings + functions).

use std::collections::HashSet;

use crate::ir::Module;
use crate::names::php_symbol_key;

/// Parsed representation of one `name:variant[,variant...]` metadata label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionVariantLabel {
    pub name: String,
    pub variants: Vec<String>,
}

/// Parses the string payload used by EIR function-variant metadata.
pub fn parse_variant_label(label: &str) -> Option<FunctionVariantLabel> {
    let fields = split_metadata(label, ':')?;
    let [name, variants] = fields.as_slice() else { return None };
    if name.is_empty() || variants.is_empty() {
        return None;
    }
    let variants = split_metadata(variants, ',')?
        .into_iter()
        .filter(|variant| !variant.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if variants.is_empty() {
        return None;
    }
    Some(FunctionVariantLabel {
        name: name.to_string(),
        variants,
    })
}

/// Splits dispatch fields outside generic arguments, which can contain commas and colons.
fn split_metadata(value: &str, delimiter: char) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in value.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.checked_sub(1)?,
            _ if ch == delimiter && depth == 0 => {
                parts.push(&value[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 { return None; }
    parts.push(&value[start..]);
    Some(parts)
}

/// Normalizes a PHP function name for case-insensitive comparisons (matches resolver/codegen).
fn normalized_function_key(name: &str) -> String {
    php_symbol_key(name.trim_start_matches('\\'))
}

/// Collects public function names that need an include-variant dispatcher.
/// Uses `module.functions` (the concrete bodies) for keys, consistent with codegen.
pub fn collect_dispatch_groups(module: &Module) -> Vec<FunctionVariantLabel> {
    let function_keys = module
        .functions
        .iter()
        .map(|function| normalized_function_key(&function.name))
        .collect::<HashSet<_>>();
    let mut seen_groups = HashSet::new();
    let mut groups = Vec::new();
    for value in &module.data.strings {
        let Some(mut label) = parse_variant_label(value) else {
            continue;
        };
        label
            .variants
            .retain(|variant| function_keys.contains(&normalized_function_key(variant)));
        if label.variants.is_empty()
            || function_keys.contains(&normalized_function_key(&label.name))
            || !seen_groups.insert(normalized_function_key(&label.name))
        {
            continue;
        }
        groups.push(label);
    }
    groups
}

/// Finds a user function by (normalized) PHP name.
pub fn function_by_php_name<'a>(module: &'a Module, name: &str) -> Option<&'a crate::ir::Function> {
    let key = normalized_function_key(name);
    module
        .functions
        .iter()
        .find(|function| normalized_function_key(&function.name) == key)
}

/// Returns a representative concrete variant function for a public function group by name.
pub fn variant_callee_for_group<'a>(module: &'a Module, name: &str) -> Option<&'a crate::ir::Function> {
    let requested = normalized_function_key(name);
    collect_dispatch_groups(module)
        .into_iter()
        .find(|group| normalized_function_key(&group.name) == requested)
        .and_then(|group| {
            group
                .variants
                .iter()
                .find_map(|variant| function_by_php_name(module, variant))
        })
}

/// Resolves a FunctionVariantRef {group, variant} (indices into collected groups)
/// to the concrete callee PHP name (if resolvable to a user function).
pub fn resolve_variant_callee_name(module: &Module, group: u32, variant: u32) -> Option<String> {
    let groups = collect_dispatch_groups(module);
    let g = groups.get(group as usize)?;
    g.variants.get(variant as usize).cloned().or_else(|| {
        if g.variants.len() == 1 {
            Some(g.variants[0].clone())
        } else {
            None
        }
    })
}

/// Resolves a FunctionVariantRef directly to the concrete callee Function (for inlining).
pub fn resolve_variant_callee<'a>(module: &'a Module, group: u32, variant: u32) -> Option<&'a crate::ir::Function> {
    resolve_variant_callee_name(module, group, variant)
        .and_then(|name| function_by_php_name(module, &name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generic argument separators and callable return colons belong to the function name.
    #[test]
    fn generic_variant_metadata_keeps_nested_type_delimiters() {
        let args = "array<string, int>, callable(int, string): string";
        let name = format!("choose<{}>", args);
        let variants = vec![format!("left<{}>", args), format!("right<{}>", args)];
        let label = format!("{}:{}", name, variants.join(","));
        assert_eq!(parse_variant_label(&label), Some(FunctionVariantLabel { name, variants }));
    }
}
