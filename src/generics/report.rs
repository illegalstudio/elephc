//! Purpose:
//! Reports how many monomorphized instantiations a program produced, grouped by the template each
//! came from, for the `--report-generics` diagnostic.
//!
//! Called from:
//! - `crate::pipeline::compile()` after `generics::monomorphize` has stripped every template.
//!
//! Key details:
//! - It reads the program AFTER monomorphization. A template is gone by then, and every remaining
//!   declaration whose NAME carries `<...>` is a concrete instantiation — the ground truth the
//!   emitted object will contain, rather than a per-round request list that can be incomplete.
//! - The rendering is deterministic (count descending, then kind, then template) so a test can
//!   assert it exactly.

use std::collections::BTreeMap;

use crate::parser::ast::{Program, Stmt, StmtKind};

/// Renders the generic-instantiation report for an already-monomorphized program.
///
/// The report is a header and one line per template: the number of instantiations and their
/// names. It is what tells whether a program's generic cost is concentrated in a few heavily
/// instantiated templates or spread across many, which is the input the dedup work needs.
pub fn report(program: &Program) -> String {
    let mut groups: BTreeMap<(u8, String), Vec<String>> = BTreeMap::new();
    collect(program, &mut groups);
    render(&groups)
}

/// Walks a program, recording one entry per instantiated declaration.
fn collect(program: &Program, groups: &mut BTreeMap<(u8, String), Vec<String>>) {
    for stmt in program {
        collect_stmt(stmt, groups);
    }
}

/// Records the instantiations one statement declares, recursing into namespace blocks.
fn collect_stmt(stmt: &Stmt, groups: &mut BTreeMap<(u8, String), Vec<String>>) {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => {
            if let Some(base) = base_of(name) {
                add(groups, 0, base, name.clone());
            }
        }
        StmtKind::ClassDecl { name, methods, .. }
        | StmtKind::InterfaceDecl { name, methods, .. }
        | StmtKind::TraitDecl { name, methods, .. }
        | StmtKind::EnumDecl { name, methods, .. } => {
            if let Some(base) = base_of(name) {
                add(groups, 1, base, name.clone());
            }
            // A generic METHOD is instantiated per call site even on a non-generic host, so the
            // owner's base name is only the grouping label, never a filter.
            let owner = base_of(name).unwrap_or_else(|| name.clone());
            for method in methods {
                if let Some(method_base) = base_of(&method.name) {
                    add(
                        groups,
                        2,
                        format!("{owner}::{method_base}"),
                        format!("{name}::{}", method.name),
                    );
                }
            }
        }
        StmtKind::NamespaceBlock { body, .. } => collect(body, groups),
        _ => {}
    }
}

/// Adds one instantiation under its template, ignoring a repeat of the same instantiation.
fn add(
    groups: &mut BTreeMap<(u8, String), Vec<String>>,
    kind: u8,
    base: String,
    instantiation: String,
) {
    let names = groups.entry((kind, base)).or_default();
    if !names.contains(&instantiation) {
        names.push(instantiation);
    }
}

/// Returns the template name of an instantiated name (`Box<int>` → `Box`), or `None` when the name
/// carries no type arguments and is therefore an ordinary declaration.
fn base_of(name: &str) -> Option<String> {
    name.split_once('<').map(|(base, _)| base.to_string())
}

/// Renders the collected groups, largest first, as the report text.
fn render(groups: &BTreeMap<(u8, String), Vec<String>>) -> String {
    let total: usize = groups.values().map(Vec::len).sum();
    let mut rows: Vec<(&(u8, String), &Vec<String>)> = groups.iter().collect();
    rows.sort_by(|a, b| {
        let (left, right) = (a.0, b.0);
        b.1.len()
            .cmp(&a.1.len())
            .then(left.0.cmp(&right.0))
            .then(left.1.cmp(&right.1))
    });
    let mut lines = vec![format!(
        "generics: {} instantiation{} across {} template{}",
        total,
        if total == 1 { "" } else { "s" },
        groups.len(),
        if groups.len() == 1 { "" } else { "s" },
    )];
    for ((kind, base), names) in rows {
        let kind = match kind {
            0 => "function",
            1 => "class",
            2 => "method",
            _ => "other",
        };
        let mut names = names.clone();
        names.sort();
        lines.push(format!(
            "  {kind:<8} x{:<3} {base}  [{}]",
            names.len(),
            names.join(", ")
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The template of an instantiated name is its part before the first `<`.
    #[test]
    fn base_of_splits_an_instantiated_name() {
        assert_eq!(base_of("Box<int>").as_deref(), Some("Box"));
        assert_eq!(base_of("Box<Box<int>>").as_deref(), Some("Box"));
        assert_eq!(base_of("map<string>").as_deref(), Some("map"));
        assert_eq!(base_of("ordinary"), None);
    }

    /// The report counts each template, splits the three kinds, and lists names deterministically.
    #[test]
    fn render_groups_and_orders_by_count() {
        let mut groups: BTreeMap<(u8, String), Vec<String>> = BTreeMap::new();
        groups.insert(
            (1, "Box".to_string()),
            vec!["Box<int>".to_string(), "Box<string>".to_string()],
        );
        groups.insert((0, "identity".to_string()), vec!["identity<int>".to_string()]);
        groups.insert(
            (2, "Box::map".to_string()),
            vec!["Box<int>::map<string>".to_string()],
        );
        let text = render(&groups);
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some("generics: 4 instantiations across 3 templates")
        );
        // Box has two, so it is first; the two single-instantiation groups follow by kind then
        // name (function before method).
        assert_eq!(
            lines.next(),
            Some("  class    x2   Box  [Box<int>, Box<string>]")
        );
        assert_eq!(
            lines.next(),
            Some("  function x1   identity  [identity<int>]")
        );
        assert_eq!(
            lines.next(),
            Some("  method   x1   Box::map  [Box<int>::map<string>]")
        );
        assert_eq!(lines.next(), None);
    }

    /// A single instantiation uses the singular header form.
    #[test]
    fn render_singular_header() {
        let mut groups: BTreeMap<(u8, String), Vec<String>> = BTreeMap::new();
        groups.insert((0, "id".to_string()), vec!["id<int>".to_string()]);
        let text = render(&groups);
        assert!(text.starts_with("generics: 1 instantiation across 1 template\n"));
    }
}
