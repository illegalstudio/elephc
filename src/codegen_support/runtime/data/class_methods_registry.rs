//! Purpose:
//! Emits the class-id-indexed method rows `__rt_class_method_names` walks to answer
//! `get_class_methods()`.
//!
//! Called from:
//! - `crate::codegen::emit_program()` when the module's `class_methods_introspection`
//!   runtime feature is set.
//!
//! Key details:
//! - `_class_method_rows_ptrs[class_id]` points at `{count, rows...}`; each row is 32 bytes:
//!   `{name_ptr, name_len, visibility, declaring_class_id}` with visibility 0 public,
//!   1 protected, 2 private. `_class_method_rows_count` bounds the class id.
//! - Rows follow php's method table: the class's own methods in declaration order (trait
//!   methods after them), then the parent's rows it does not redeclare, INCLUDING the
//!   parent's private ones. A private ancestor method is listed from that ancestor's own
//!   scope, so the runtime filter needs the row even though other scopes never see it.
//! - The calling scope is only known at run time for the filter, never for the rows, so one
//!   row list per class serves every call site.

use std::collections::{HashMap, HashSet};

use crate::ir::Module;
use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::ClassInfo;

use super::instanceof::escaped_ascii;

/// One method as the runtime filter sees it.
struct MethodRow {
    name: String,
    visibility: u64,
    declaring_class_id: i64,
}

/// Emits `_class_method_rows_count`, `_class_method_rows_ptrs` and every class's row list.
pub(crate) fn emit_class_methods_registry_data(module: &Module) -> String {
    let class_ids: HashMap<String, i64> = module
        .class_infos
        .iter()
        .map(|(name, info)| {
            (
                php_symbol_key(name.trim_start_matches('\\')),
                info.class_id as i64,
            )
        })
        .collect();
    let mut by_id: Vec<(u64, &String, &ClassInfo)> = module
        .class_infos
        .iter()
        .map(|(name, info)| (info.class_id, name, info))
        .collect();
    by_id.sort_by_key(|(class_id, _, _)| *class_id);
    let slot_count = by_id.last().map_or(0, |(class_id, _, _)| class_id + 1);

    let mut out = String::new();
    let mut name_labels: HashMap<String, String> = HashMap::new();
    let mut tables: HashMap<u64, Vec<MethodRow>> = HashMap::new();
    for (class_id, class_name, _) in &by_id {
        tables.insert(*class_id, method_table(module, &class_ids, class_name));
    }
    out.push_str(".data\n");
    for (class_id, _, _) in &by_id {
        for row in &tables[class_id] {
            if !name_labels.contains_key(&row.name) {
                let label = format!("_class_method_name_{}", name_labels.len());
                out.push_str(&format!(
                    "{}:\n    .ascii \"{}\"\n",
                    label,
                    escaped_ascii(&row.name)
                ));
                name_labels.insert(row.name.clone(), label);
            }
        }
    }
    out.push_str(".p2align 3\n");
    out.push_str("_class_method_rows_empty:\n    .quad 0\n");
    for (class_id, _, _) in &by_id {
        let rows = &tables[class_id];
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("_class_method_rows_{}:\n", class_id));
        out.push_str(&format!("    .quad {}\n", rows.len()));
        for row in rows {
            out.push_str(&format!("    .quad {}\n", name_labels[&row.name]));
            out.push_str(&format!("    .quad {}\n", row.name.len()));
            out.push_str(&format!("    .quad {}\n", row.visibility));
            out.push_str(&format!("    .quad {}\n", row.declaring_class_id));
        }
    }
    out.push_str(".globl _class_method_rows_count\n_class_method_rows_count:\n");
    out.push_str(&format!("    .quad {}\n", slot_count));
    out.push_str(".globl _class_method_rows_ptrs\n_class_method_rows_ptrs:\n");
    let present: HashSet<u64> = by_id
        .iter()
        .filter(|(class_id, _, _)| !tables[class_id].is_empty())
        .map(|(class_id, _, _)| *class_id)
        .collect();
    for class_id in 0..slot_count {
        if present.contains(&class_id) {
            out.push_str(&format!("    .quad _class_method_rows_{}\n", class_id));
        } else {
            out.push_str("    .quad _class_method_rows_empty\n");
        }
    }
    out
}

/// Returns `class_name`'s method table in php's order: own methods, then the parent's rows
/// the class does not redeclare (the parent's private ones included).
fn method_table(
    module: &Module,
    class_ids: &HashMap<String, i64>,
    class_name: &str,
) -> Vec<MethodRow> {
    let mut chain = Vec::new();
    let mut seen_classes = HashSet::new();
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if !seen_classes.insert(php_symbol_key(&name)) {
            break;
        }
        let Some((resolved, info)) = module.class_infos.get_key_value(name.trim_start_matches('\\'))
        else {
            break;
        };
        chain.push((resolved.as_str(), info));
        current = info.parent.clone();
    }
    let mut rows = Vec::new();
    let mut claimed = HashSet::new();
    for (level_name, info) in chain {
        for name in own_method_names(level_name, info) {
            let key = php_symbol_key(&name);
            if !claimed.insert(key.clone()) {
                continue;
            }
            let visibility = info
                .method_visibilities
                .get(&key)
                .or_else(|| info.static_method_visibilities.get(&key))
                .unwrap_or(&Visibility::Public);
            rows.push(MethodRow {
                name,
                visibility: match visibility {
                    Visibility::Public => 0,
                    Visibility::Protected => 1,
                    Visibility::Private => 2,
                },
                declaring_class_id: class_ids
                    .get(&php_symbol_key(level_name.trim_start_matches('\\')))
                    .copied()
                    .unwrap_or(-1),
            });
        }
    }
    rows
}

/// Returns the methods one class declares itself, in declaration order and declared case.
///
/// A property hook's compiler-generated accessor is not a method PHP lists, so it is skipped.
///
/// `reflection_methods` carries php's order; a method only the signature maps know (an
/// interface contract an abstract class leaves open) follows it, sorted so the table is stable.
fn own_method_names(class_name: &str, info: &ClassInfo) -> Vec<String> {
    let declares = |key: &str| {
        info.method_declaring_classes
            .get(key)
            .or_else(|| info.static_method_declaring_classes.get(key))
            .is_none_or(|declaring| declaring.eq_ignore_ascii_case(class_name.trim_start_matches('\\')))
    };
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for name in &info.reflection_methods {
        let key = php_symbol_key(name);
        let known = info.methods.contains_key(&key) || info.static_methods.contains_key(&key);
        if known && declares(&key) && !info.is_property_hook_method(&key) && seen.insert(key) {
            names.push(name.clone());
        }
    }
    let mut remaining: Vec<String> = info
        .methods
        .keys()
        .chain(info.static_methods.keys())
        .filter(|key| {
            !seen.contains(key.as_str()) && declares(key) && !info.is_property_hook_method(key)
        })
        .map(|key| {
            info.method_decls
                .iter()
                .find(|method| php_symbol_key(&method.name) == *key)
                .map_or_else(|| key.clone(), |method| method.name.clone())
        })
        .collect();
    remaining.sort();
    remaining.dedup();
    names.extend(remaining);
    names
}
