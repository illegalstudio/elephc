//! Purpose:
//! Termwind-facing DOM HTML subset: `DOMDocument::loadHTML()` plus the node
//! types and properties `HtmlRenderer` / `ValueObjects\Node` walk
//! (`getElementsByTagName`, `childNodes`, `nodeName`, `nodeValue`,
//! `getAttribute`, sibling pointers, `saveXML`).
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via
//!   `inject_if_used`, before name resolution, and `inject_after_autoload`.
//!
//! Key details:
//! - Choice (B) versus draft PR #654: that PR is a 178k-line PHP 8.5
//!   libxml2+Lexbor bridge (`crates/elephc-dom`) still in progress. This
//!   prelude is a pay-for-use HTML fragment walker that does not add a
//!   second native DOM engine. Same PHP class names and `LIBXML_*`
//!   integers so #654 can replace this surface when it lands.
//! - Injected only when the program names a DOM class. No `--with-dom`
//!   flag and no `elephc-dom` crate.
//! - Delivered as parsed PHP (like mysqli), not a native builtin, so every
//!   supported target gets the same walk with no new assembly.

mod detect;
mod entities;
mod surface;

use std::sync::OnceLock;

use crate::errors::CompileError;
use crate::parser::ast::Program;

/// Parsed prelude cache. The fragment is declaration-only, so one parse is
/// reused for every injecting compile.
static PARSED_PRELUDE: OnceLock<Program> = OnceLock::new();

/// Tokenizes and parses the DOM HTML prelude exactly once.
fn parsed_prelude() -> Program {
    PARSED_PRELUDE
        .get_or_init(|| {
            let source = format!("<?php\n{}\n{}", entities::SRC, surface::SRC);
            let tokens = crate::lexer::tokenize(&source).expect("dom html prelude must tokenize");
            crate::parser::parse_internal(&tokens).expect("dom html prelude must parse")
        })
        .clone()
}

/// Prepends the Termwind DOM HTML prelude when the program names a DOM class.
///
/// `force` exists for the codegen harness and future opt-in; ordinary compiles
/// pass `false` and rely on `program_uses_dom_html`.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_on_compiler_stack(program, force, inventory)
    })
}

/// Keeps detector cloning, recursive scanning and prelude construction inside the phase budget.
fn inject_if_used_on_compiler_stack(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_dom_html(&program) {
        return program;
    }
    let mut combined = parsed_prelude();
    inventory.record_program("dom-html", &combined);
    combined.extend(program);
    combined
}

/// Discovers DOM references loaded by autoload without reinjecting an existing prelude.
/// Only the new declarations are resolved, preserving already-resolved application names.
pub fn inject_after_autoload(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        if inventory.groups.contains_key("dom-html")
            || !detect::program_uses_dom_html(&program)
        {
            return Ok(program);
        }
        let mut combined = crate::name_resolver::resolve(parsed_prelude())?;
        inventory.record_program("dom-html", &combined);
        combined.extend(program);
        Ok(combined)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// Every public DOM class has one shared prelude contract and no catalog-only extras.
    #[test]
    fn shared_class_contracts_match_dom_declarations() {
        use std::collections::BTreeSet;
        use elephc_builtin_contract::{ClassRoute, PhpModule};

        let declared: BTreeSet<String> = parsed_prelude().iter()
            .filter_map(|stmt| {
                if let StmtKind::ClassDecl { name, .. } = &stmt.kind {
                    let contract = elephc_builtin_contract::lookup_class(name)
                        .unwrap_or_else(|| panic!("uncontracted DOM declaration: {name}"));
                    assert_eq!(contract.aot, ClassRoute::Prelude, "{name}");
                    (!contract.internal).then(|| name.to_ascii_lowercase())
                } else {
                    None
                }
            }).collect();
        let catalogued: BTreeSet<String> = elephc_builtin_contract::classes().iter()
            .filter(|class| class.module == PhpModule::Dom)
            .map(|class| {
                assert_eq!(class.aot, ClassRoute::Prelude, "{}", class.name);
                assert!(!class.internal && !class.extension, "{}", class.name);
                class.name.to_ascii_lowercase()
            }).collect();
        assert_eq!(declared, catalogued);
    }

    /// The late pass injects resolved declarations once and retains user-owned DOM classes.
    #[test]
    fn late_injection_is_idempotent_and_preserves_user_classes() {
        let parse_resolved = |source| {
            let tokens = crate::lexer::tokenize(source).unwrap();
            crate::name_resolver::resolve(crate::parser::parse(&tokens).unwrap()).unwrap()
        };
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse_resolved("<?php namespace App; use DOMDocument as Doc; new Doc();");
        let program = inject_after_autoload(program, &mut inventory).unwrap();
        assert!(inventory.groups.contains_key("dom-html"));
        let len = program.len();
        assert_eq!(inject_after_autoload(program, &mut inventory).unwrap().len(), len);

        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse_resolved("<?php class DOMDocument {} new DOMDocument();");
        assert_eq!(inject_after_autoload(program, &mut inventory).unwrap().len(), 2);
        assert!(inventory.groups.is_empty());
    }

    /// The prelude must declare the Termwind node types and its private traversal helper.
    #[test]
    fn declares_termwind_dom_classes() {
        let declared: Vec<String> = parsed_prelude()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![
                "DOMNode",
                "__ElephcDOMNodeListIterator",
                "DOMNodeList",
                "DOMDocument",
                "DOMElement",
                "DOMCharacterData",
                "DOMText",
                "DOMComment",
                "DOMProcessingInstruction",
            ]
        );
    }

    /// `loadHTML` keeps the two-parameter PHP signature Termwind calls.
    #[test]
    fn load_html_takes_source_and_options() {
        let load = parsed_prelude()
            .into_iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "DOMDocument"))
            .expect("DOMDocument must be declared");
        let StmtKind::ClassDecl { methods, .. } = &load.kind else {
            unreachable!("filtered above");
        };
        let method = methods
            .iter()
            .find(|method| method.name == "loadHTML")
            .expect("loadHTML must be declared");
        assert_eq!(method.params.len(), 2);
        assert_eq!(method.params[0].0, "source");
        assert_eq!(method.params[1].0, "options");
    }
}
