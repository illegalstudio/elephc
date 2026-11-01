//! Purpose:
//! Computes and emits the closed-world per-class relation payload registry
//! backing non-literal `class_implements()`/`class_parents()`/`class_uses()`.
//! Each row carries the same PHP-declaration-order name lists the literal
//! compile-time fold already materializes, so a runtime search and a literal
//! call agree on content and order for the same target.
//!
//! Called from:
//! - `crate::codegen::emit_program()` when the module's
//!   `class_relation_introspection` runtime feature is set.
//!
//! Key details:
//! - Three sibling tables (`_class_relation_table`, `_interface_relation_table`,
//!   `_trait_relation_table`) share one uniform 64-byte row layout so a single
//!   runtime search routine (`__rt_class_relation_probe`, entry_size = 64) can
//!   search any of them: `{name_ptr, name_len, implements_ptr,
//!   implements_count, parents_ptr, parents_count, uses_ptr, uses_count}`.
//!   Rows are sorted by lowercased name for `__rt_sorted_name_search`'s binary
//!   search, mirroring `_class_table`/`_interface_table`/`_trait_table`
//!   (`const_registry.rs`). A kind that never produces a given relation (e.g. an
//!   interface's `parents`/`uses`) stores a zero `{ptr: 0, count: 0}` pair
//!   rather than omitting the field, so every caller can read any of the three
//!   offsets from any table uniformly.
//! - Each non-empty relation list is a separate flat array of case-preserving
//!   `{name_ptr, name_len}` pairs in PHP declaration/linearization order (NOT
//!   sorted) — the exact order `crate::codegen::lower_inst::builtins::
//!   class_relations` already computes for the literal path. The computation
//!   here is intentionally a parallel implementation over `&Module` (rather
//!   than a shared refactor) so the literal EIR lowering path stays untouched.
//! - `class_implements()`/`class_parents()`/`class_uses()` PHP-verified target
//!   matrix: a class row carries all three relations (implements = all
//!   transitively implemented interfaces, parents = the ancestor chain, uses =
//!   direct trait uses only); an interface row carries only `implements` (its
//!   own transitively extended parent interfaces); a trait row carries only
//!   `uses` (its own direct trait uses). Every other slot on that row is the
//!   zero pair, which `class_implements`/`class_parents`/`class_uses` runtime
//!   dispatch reads back as "target exists, relation is the empty array" —
//!   matching `php -n`'s observed behavior (e.g. `class_parents()` on a known
//!   interface name returns `array()`, not `false`).

use crate::ir::Module;
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, InterfaceInfo};

use super::instanceof::escaped_ascii;

/// One class-like target's precomputed relation payload (PHP declaration order).
struct RelationRow {
    /// Lowercased ASCII sort key used for the binary search over the row's table.
    sort_key: String,
    implements: Vec<String>,
    parents: Vec<String>,
    uses: Vec<String>,
}

/// Emits the `_class_relation_table`/`_interface_relation_table`/
/// `_trait_relation_table` payload registries computed from `module`.
pub(crate) fn emit_class_relation_registry_data(module: &Module) -> String {
    let class_rows = class_relation_rows(module);
    let interface_rows = interface_relation_rows(module);
    let trait_rows = trait_relation_rows(module);

    let mut out = String::new();
    out.push_str(".data\n");
    out.push_str(".p2align 3\n");
    emit_relation_table(&mut out, "_class_relation_table", "_classrel", &class_rows);
    out.push_str(".p2align 3\n");
    emit_relation_table(&mut out, "_interface_relation_table", "_ifacerel", &interface_rows);
    out.push_str(".p2align 3\n");
    emit_relation_table(&mut out, "_trait_relation_table", "_traitrel", &trait_rows);
    out
}

/// Builds the sorted class rows: `implements` (transitive interfaces), `parents`
/// (ancestor chain), and `uses` (direct trait uses).
fn class_relation_rows(module: &Module) -> Vec<RelationRow> {
    let mut rows: Vec<RelationRow> = module
        .class_infos
        .keys()
        .filter(|name| !is_internal_synthetic_class_name(name))
        .map(|name| RelationRow {
            sort_key: php_symbol_key(name.trim_start_matches('\\')),
            implements: class_implements(module, name),
            parents: class_parents(module, name),
            uses: class_uses_direct(module, name),
        })
        .collect();
    rows.sort_by(|a, b| a.sort_key.as_bytes().cmp(b.sort_key.as_bytes()));
    rows
}

/// Builds the sorted interface rows: `implements` holds the interface's own
/// transitively extended parent interfaces; `parents`/`uses` are always empty.
fn interface_relation_rows(module: &Module) -> Vec<RelationRow> {
    let mut rows: Vec<RelationRow> = module
        .interface_infos
        .keys()
        .map(|name| RelationRow {
            sort_key: php_symbol_key(name.trim_start_matches('\\')),
            implements: resolve_interface_ancestors(module, name),
            parents: Vec::new(),
            uses: Vec::new(),
        })
        .collect();
    rows.sort_by(|a, b| a.sort_key.as_bytes().cmp(b.sort_key.as_bytes()));
    rows
}

/// Builds the sorted trait rows: `uses` holds the trait's own direct trait
/// uses; `implements`/`parents` are always empty (traits neither implement
/// interfaces nor have ancestors).
fn trait_relation_rows(module: &Module) -> Vec<RelationRow> {
    let mut rows: Vec<RelationRow> = module
        .trait_table
        .names
        .iter()
        .map(|name| RelationRow {
            sort_key: php_symbol_key(name.trim_start_matches('\\')),
            implements: Vec::new(),
            parents: Vec::new(),
            uses: module.declared_trait_uses.get(name).cloned().unwrap_or_default(),
        })
        .collect();
    rows.sort_by(|a, b| a.sort_key.as_bytes().cmp(b.sort_key.as_bytes()));
    rows
}

/// Returns a class's transitively implemented interface names, in the same
/// order as the literal `class_implements()` fold: the class's own declared
/// `interfaces` list (already resolved through parent inheritance by the type
/// checker), unchanged.
fn class_implements(module: &Module, class_name: &str) -> Vec<String> {
    lookup_class(module, class_name)
        .map(|info| info.interfaces.clone())
        .unwrap_or_default()
}

/// Returns a class's ancestor chain, immediate parent first, matching the
/// literal `class_parents()` fold.
fn class_parents(module: &Module, class_name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = class_name.to_string();
    while let Some(info) = lookup_class(module, &current) {
        let Some(parent) = &info.parent else {
            break;
        };
        let parent_name = lookup_class_name(module, parent).unwrap_or_else(|| parent.clone());
        names.push(parent_name.clone());
        current = parent_name;
    }
    names
}

/// Returns a class's own directly declared trait uses, matching the literal
/// `class_uses()` fold (inherited trait uses are intentionally excluded).
fn class_uses_direct(module: &Module, class_name: &str) -> Vec<String> {
    lookup_class(module, class_name)
        .map(|info| info.used_traits.clone())
        .unwrap_or_default()
}

/// Computes `interface_name`'s own transitively extended ancestor interfaces (excluding
/// itself), in PHP's linearization order: this interface's own directly declared `extends`
/// list first (source order, as one contiguous block), then — for each of those parents in
/// that same order — that parent's own ancestor list, individually reversed, appended.
/// Deduplicates by case-insensitive name, keeping the first occurrence.
///
/// `php -n` verified; mirrors
/// `crate::types::checker::schema::classes::interfaces::resolve_interface_ancestors` and
/// `crate::codegen::lower_inst::builtins::class_relations::resolve_interface_ancestors` —
/// kept in sync by hand since none of the three share this helper directly.
fn resolve_interface_ancestors(module: &Module, interface_name: &str) -> Vec<String> {
    let Some(interface) = lookup_interface(module, interface_name) else {
        return Vec::new();
    };
    if interface.parents.is_empty() {
        return Vec::new();
    }
    let direct_parents: Vec<String> = interface
        .parents
        .iter()
        .map(|parent| lookup_interface_name(module, parent).unwrap_or_else(|| parent.clone()))
        .collect();
    let mut names = Vec::new();
    for parent_name in &direct_parents {
        if !names
            .iter()
            .any(|name: &String| php_symbol_key(name) == php_symbol_key(parent_name))
        {
            names.push(parent_name.clone());
        }
    }
    for parent_name in &direct_parents {
        let grandparents = resolve_interface_ancestors(module, parent_name);
        for grandparent in grandparents.into_iter().rev() {
            if !names
                .iter()
                .any(|name| php_symbol_key(name) == php_symbol_key(&grandparent))
            {
                names.push(grandparent);
            }
        }
    }
    names
}

/// Looks up a class by PHP-style case-insensitive name.
fn lookup_class<'a>(module: &'a Module, name: &str) -> Option<&'a ClassInfo> {
    let name = lookup_class_name(module, name)?;
    module.class_infos.get(&name)
}

/// Looks up an interface by PHP-style case-insensitive name.
fn lookup_interface<'a>(module: &'a Module, name: &str) -> Option<&'a InterfaceInfo> {
    let name = lookup_interface_name(module, name)?;
    module.interface_infos.get(&name)
}

/// Looks up a class name by PHP-style case-insensitive name.
fn lookup_class_name(module: &Module, raw: &str) -> Option<String> {
    lookup_folded(module.class_infos.keys(), raw)
}

/// Looks up an interface name by PHP-style case-insensitive name.
fn lookup_interface_name(module: &Module, raw: &str) -> Option<String> {
    lookup_folded(module.interface_infos.keys(), raw)
}

/// Returns a matching symbol name using PHP case-insensitive comparison.
fn lookup_folded<'a>(names: impl Iterator<Item = &'a String>, raw: &str) -> Option<String> {
    let clean = raw.trim_start_matches('\\');
    let key = php_symbol_key(clean);
    names
        .into_iter()
        .find(|name| php_symbol_key(name.trim_start_matches('\\')) == key)
        .cloned()
}

/// Returns true for internal helper classes hidden from PHP-visible registries.
///
/// Mirrors `crate::codegen::is_internal_synthetic_class_name` and its
/// siblings so this registry's row set agrees with `_class_table`'s.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}

/// Appends one uniform 64-byte-row, name-sorted relation table to `out`.
///
/// `table_symbol` names the emitted `_<table_symbol>`/`_<table_symbol>_count`
/// pair; `label_prefix` seeds the per-row/per-list interned byte labels so the
/// three sibling tables never collide on symbol names.
fn emit_relation_table(out: &mut String, table_symbol: &str, label_prefix: &str, rows: &[RelationRow]) {
    out.push_str(&format!(".globl {0}_count\n{0}_count:\n", table_symbol));
    out.push_str(&format!("    .quad {}\n", rows.len()));
    out.push_str(&format!(".globl {0}\n{0}:\n", table_symbol));
    for (index, row) in rows.iter().enumerate() {
        out.push_str(&format!("    .quad {}_name_{}\n", label_prefix, index));
        out.push_str(&format!("    .quad {}\n", row.sort_key.len()));
        emit_list_field(out, label_prefix, "impl", index, &row.implements);
        emit_list_field(out, label_prefix, "par", index, &row.parents);
        emit_list_field(out, label_prefix, "use", index, &row.uses);
    }
    for (index, row) in rows.iter().enumerate() {
        out.push_str(&format!("{}_name_{}:\n", label_prefix, index));
        out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(&row.sort_key)));
    }
    for (index, row) in rows.iter().enumerate() {
        emit_name_list(out, label_prefix, "impl", index, &row.implements);
        emit_name_list(out, label_prefix, "par", index, &row.parents);
        emit_name_list(out, label_prefix, "use", index, &row.uses);
    }
}

/// Emits one row's `{list_ptr, list_count}` field, referencing the empty
/// sentinel pair when the relation list is empty.
fn emit_list_field(out: &mut String, label_prefix: &str, field: &str, index: usize, names: &[String]) {
    if names.is_empty() {
        out.push_str("    .quad 0\n");
        out.push_str("    .quad 0\n");
        return;
    }
    out.push_str(&format!("    .quad {}_{}_{}\n", label_prefix, field, index));
    out.push_str(&format!("    .quad {}\n", names.len()));
}

/// Emits one non-empty relation list as a flat `{name_ptr, name_len}` array in
/// declaration order, plus the interned case-preserving name bytes it points at.
///
/// Re-aligns before the label: the preceding list's (or the row-name block's)
/// interned ascii bytes can leave the location counter unaligned, but the
/// `{name_ptr, name_len}` quads that immediately follow this label must land
/// on an 8-byte boundary.
fn emit_name_list(out: &mut String, label_prefix: &str, field: &str, index: usize, names: &[String]) {
    if names.is_empty() {
        return;
    }
    out.push_str(".p2align 3\n");
    out.push_str(&format!("{}_{}_{}:\n", label_prefix, field, index));
    for (entry, name) in names.iter().enumerate() {
        out.push_str(&format!("    .quad {}_{}_{}_{}\n", label_prefix, field, index, entry));
        out.push_str(&format!("    .quad {}\n", name.len()));
    }
    for (entry, name) in names.iter().enumerate() {
        out.push_str(&format!("{}_{}_{}_{}:\n", label_prefix, field, index, entry));
        out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(name)));
    }
}

