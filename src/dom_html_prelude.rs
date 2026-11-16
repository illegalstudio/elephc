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
//! - Delivered as built synthetic AST (like the other Rust preludes), not a native
//!   builtin, so every supported target gets the same walk with no new assembly.
//! - Attribute-name validation and DOMException remain available without libxml2.
//! - A standalone DOMException reference injects only its declaration; walker
//!   references add the HTML helpers and weak-cache dependency when needed.

mod attributes;
mod detect;
mod entities;
mod generated;
mod node_cache;
mod text;

use std::sync::OnceLock;

use crate::errors::CompileError;
use crate::parser::ast::{Program, TypeExpr};
use crate::synthetic_class::{
    class, e_int, e_method_call, e_new_fq, e_new_static, e_not, e_null, e_prop, e_str, e_var,
    method, s_assign, s_expr, s_if, s_return, s_throw, t_class, t_nullable,
};

/// Built prelude cache. The declarations are built once and reused for every injecting
/// compile.
static PRELUDE: OnceLock<Program> = OnceLock::new();

/// Identifies actual DOM class references for XML receiver detection before injection.
pub(crate) fn program_uses_dom_html(program: &[crate::parser::ast::Stmt]) -> bool {
    detect::program_uses_dom_html(program)
}

/// Builds the cached DOM HTML prelude exactly once.
pub(crate) fn declarations() -> Program {
    PRELUDE
        .get_or_init(|| {
            let mut program = Vec::new();
            program.extend(entities::declarations());
            program.extend(attributes::declarations());
            program.extend(node_cache::declarations());
            program.extend(text::declarations());
            program.extend(generated::declarations());
            program.push(modern_dom_declaration());
            // A non-dummy declaration span marks an elephc prelude class as user-declared for
            // PHP's coercive parameter binding; `Span::dummy()` would make `new DOMDocument(1, …)`
            // reject the integer instead of coercing it to a string. Distinct synthetic spans also
            // keep span-keyed checker maps from conflating built nodes.
            for declaration in &mut program {
                if !declaration.span.identifies_a_node() {
                    declaration.span = crate::span::Span::synthetic();
                }
            }
            program
        })
        .clone()
}

/// Builds the PHP 8.4 `Dom\XMLDocument::createFromFile()` interop surface.
///
/// SimpleXML's `simplexml_import_dom()` tests construct a modern document and mutate it
/// through the imported view. Extending the prelude's legacy `DOMDocument` keeps one live
/// libxml tree, which is what the import path already shares.
fn modern_dom_declaration() -> crate::parser::ast::Stmt {
    class("Dom\\XMLDocument")
        .extends("\\DOMDocument")
        .method(method("createFromFile")
            .static_()
            .param("filename", TypeExpr::Str)
            .param_default("options", TypeExpr::Int, e_int(0))
            .param_default("overrideEncoding", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("static"))
            .body(vec![
                s_assign("document", e_new_static(Vec::new())),
                s_if(
                    e_not(e_method_call(
                        e_var("document"),
                        "load",
                        vec![e_var("filename"), e_var("options")],
                    )),
                    vec![s_throw(e_new_fq(
                        "Exception",
                        vec![e_str("Failed to load XML from file")],
                    ))],
                    Vec::new(),
                    None,
                ),
                s_if(
                    e_prop(e_var("document"), "__elephc_simplexml"),
                    vec![s_expr(e_method_call(
                        e_prop(e_var("document"), "__elephc_simplexml"),
                        "__elephc_set_document_encoding",
                        vec![e_str("UTF-8")],
                    ))],
                    Vec::new(),
                    None,
                ),
                s_return(e_var("document")),
            ]))
        .method(method("createFromString")
            .static_()
            .param("source", TypeExpr::Str)
            .param_default("options", TypeExpr::Int, e_int(0))
            .param_default("overrideEncoding", t_nullable(TypeExpr::Str), e_null())
            .returns(t_class("static"))
            .body(vec![
                s_assign("document", e_new_static(Vec::new())),
                s_if(
                    e_not(e_method_call(
                        e_var("document"),
                        "loadXML",
                        vec![e_var("source"), e_var("options")],
                    )),
                    vec![s_throw(e_new_fq(
                        "Exception",
                        vec![e_str("Failed to load XML from string")],
                    ))],
                    Vec::new(),
                    None,
                ),
                s_if(
                    e_prop(e_var("document"), "__elephc_simplexml"),
                    vec![s_expr(e_method_call(
                        e_prop(e_var("document"), "__elephc_simplexml"),
                        "__elephc_set_document_encoding",
                        vec![e_str("UTF-8")],
                    ))],
                    Vec::new(),
                    None,
                ),
                s_return(e_var("document")),
            ]))
        .build()
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
    let Some((mut combined, walker)) = pending_prelude(&program, force, inventory) else {
        return program;
    };
    inventory.record_program("dom-html", &combined);
    combined.extend(program);
    if walker { crate::core_prelude::ensure_weak_reference(combined, inventory) }
    else { combined }
}

/// Selects missing declarations, allowing autoload to upgrade an exception-only prelude.
fn pending_prelude(
    program: &[crate::parser::ast::Stmt],
    force: bool,
    inventory: &crate::optimize::reachability::PreludeInventory,
) -> Option<(Program, bool)> {
    use crate::parser::ast::StmtKind;

    let injected = inventory.groups.get("dom-html");
    if injected.is_some_and(|group| group.classes.contains("domdocument")) {
        return None;
    }
    if !force && !detect::program_uses_dom_html(program) {
        return None;
    }
    let exception = injected.is_some_and(|group| group.classes.contains("domexception"));
    let walker = force || detect::program_uses_dom_walker(program);
    if exception && !walker {
        return None;
    }
    let mut prelude = declarations();
    prelude.retain(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. } if name.eq_ignore_ascii_case("DOMException") => !exception,
        _ => walker,
    });
    Some((prelude, walker))
}

/// Discovers DOM references loaded by autoload without reinjecting an existing prelude.
/// Only the new declarations are resolved, preserving already-resolved application names.
pub fn inject_after_autoload(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        let Some((prelude, walker)) = pending_prelude(&program, false, inventory) else {
            return Ok(program);
        };
        let mut combined = crate::name_resolver::resolve(prelude)?;
        inventory.record_program("dom-html", &combined);
        combined.extend(program);
        Ok(if walker { crate::core_prelude::ensure_weak_reference(combined, inventory) }
            else { combined })
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

        let declared: BTreeSet<String> = declarations().iter()
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
            crate::name_resolver::resolve(crate::parser::parse_test_source(source).unwrap()).unwrap()
        };
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse_resolved("<?php namespace App; use DOMDocument as Doc; new Doc();");
        let program = inject_after_autoload(program, &mut inventory).unwrap();
        assert!(inventory.groups.contains_key("dom-html"));
        assert!(inventory.groups.contains_key("core"));
        assert_eq!(program.iter().filter(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "WeakReference")).count(), 1);
        let len = program.len();
        assert_eq!(inject_after_autoload(program, &mut inventory).unwrap().len(), len);

        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse_resolved("<?php class DOMDocument {} new DOMDocument();");
        assert_eq!(inject_after_autoload(program, &mut inventory).unwrap().len(), 2);
        assert!(inventory.groups.is_empty());
    }

    /// Exception-only programs retain one throwable declaration without HTML helpers or weak caches.
    #[test]
    fn exception_only_injection_omits_walker_declarations() {
        for source in [
            "<?php $failure = new DOMException('failure', 5); $code = $argc > 0 ? 'echo $failure->getCode();' : ''; eval($code);",
            "<?php namespace App; use DOMException as Failure; throw new Failure('failure', 5);",
            "<?php try {} catch (\\DOMException $failure) {}",
        ] {
            let program = crate::parser::parse_test_source(source).unwrap();
            let original_len = program.len();
            let mut inventory = crate::optimize::reachability::PreludeInventory::new();
            let program = inject_if_used(program, false, &mut inventory);
            assert_eq!(program.len(), original_len + 1, "{source}");
            assert!(matches!(&program[0].kind, StmtKind::ClassDecl { name, .. } if name == "DOMException"));
            let group = &inventory.groups["dom-html"];
            assert_eq!(group.classes.len(), 1, "{source}");
            assert!(group.classes.contains("domexception"), "{source}");
            assert!(group.functions.is_empty(), "{source}");
            assert!(!inventory.groups.contains_key("core"), "{source}");
            assert_eq!(inject_if_used(program, false, &mut inventory).len(), original_len + 1);
        }
    }

    /// Autoload can add the walker after an earlier exception-only injection without duplicating it.
    #[test]
    fn late_walker_injection_upgrades_exception_declarations() {
        let parse = |source| crate::parser::parse_test_source(source).unwrap();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject_if_used(parse("<?php new DOMException('failure');"), false, &mut inventory);
        let mut program = crate::name_resolver::resolve(program).unwrap();
        program.extend(crate::name_resolver::resolve(parse(
            "<?php namespace Loaded; use DOMDocument as Doc; new Doc();"
        )).unwrap());
        let program = inject_after_autoload(program, &mut inventory).unwrap();
        for expected in ["DOMException", "DOMDocument", "WeakReference"] {
            assert_eq!(program.iter().filter(|stmt| matches!(
                &stmt.kind, StmtKind::ClassDecl { name, .. } if name == expected
            )).count(), 1, "{expected}");
        }
        assert!(inventory.groups["dom-html"].classes.contains("domdocument"));
        assert!(inventory.groups.contains_key("core"));
        let len = program.len();
        assert_eq!(inject_after_autoload(program, &mut inventory).unwrap().len(), len);
    }

    /// Forced injection retains the complete walker even when only its exception was requested first.
    #[test]
    fn forced_walker_injection_upgrades_exception_declarations() {
        let program = crate::parser::parse_test_source("<?php new DOMException('failure');").unwrap();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject_if_used(program, false, &mut inventory);
        let program = inject_if_used(program, true, &mut inventory);
        assert!(inventory.groups["dom-html"].classes.contains("domdocument"));
        assert_eq!(program.iter().filter(|stmt| matches!(
            &stmt.kind, StmtKind::ClassDecl { name, .. } if name == "DOMException"
        )).count(), 1);
        assert!(inventory.groups.contains_key("core"));
    }

    /// DOM injection supplies its weak-cache dependency even without a user WeakReference mention.
    #[test]
    fn dom_injection_declares_weak_cache_dependency_once() {
        let program = crate::parser::parse_test_source("<?php new DOMDocument();").unwrap();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject_if_used(program, false, &mut inventory);
        assert!(inventory.groups.contains_key("core"));
        assert_eq!(program.iter().filter(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "WeakReference")).count(), 1);
    }

    /// Every top-level declaration carries a distinct synthetic span.
    ///
    /// `class_is_user_declared` treats a non-dummy declaration span as user-declared, which is
    /// what keeps PHP's weak scalar coercion on the constructor (`new DOMDocument(1, …)`); a
    /// dummy span would make the string parameters reject the integers. Distinct synthetic
    /// spans also keep span-keyed checker maps from conflating built nodes.
    #[test]
    fn dom_prelude_declarations_carry_distinct_synthetic_spans() {
        let prelude = declarations();
        let mut seen = std::collections::HashSet::new();
        for statement in &prelude {
            assert!(
                statement.span.line >= crate::span::SYNTHETIC_LINE_BASE,
                "declaration {:?} does not carry a synthetic span",
                statement.kind
            );
            assert!(seen.insert(statement.span), "duplicate declaration span");
        }
    }

    /// The prelude must declare the Termwind node types and its private traversal helper.
    #[test]
    fn declares_termwind_dom_classes() {
        let declared: Vec<String> = declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![
                "DOMException",
                "DOMNode",
                "__ElephcDOMNodeListIterator",
                "DOMNodeList",
                "DOMDocument",
                "DOMElement",
                "DOMCharacterData",
                "DOMText",
                "DOMComment",
                "DOMProcessingInstruction",
                "Dom\\XMLDocument",
            ]
        );
    }

    /// `loadHTML` keeps the two-parameter PHP signature Termwind calls.
    #[test]
    fn load_html_takes_source_and_options() {
        let load = declarations()
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
