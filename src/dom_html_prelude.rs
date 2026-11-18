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
//! - The prelude is split into parts (`extend::Part`): the walker every DOM program gets, the
//!   HTML4 parser/serializer (only when `loadHTML`/`saveHTML` and friends are mentioned), the
//!   libxml error API, and DOMXPath. Packages add code from their own files through the
//!   `extend::Contributions` API (see the top of `extend.rs`) instead of editing `generated.rs`.

mod attributes;
mod chardata;
mod collections;
mod detect;
mod element_document;
mod entities;
pub(crate) mod extend;
mod generated;
mod html_parser;
mod libxml;
mod node_cache;
mod node_tree;
mod text;
mod writer_html;
mod writer_xml;
mod xpath;

use crate::errors::CompileError;
use crate::parser::ast::{Program, TypeExpr};
use crate::synthetic_class::{
    class, e_int, e_method_call, e_new_fq, e_new_static, e_not, e_null, e_prop, e_str, e_var,
    method, s_assign, s_expr, s_if, s_return, s_throw, t_class, t_nullable,
};
use extend::{Part, PartSet};

/// Prelude inventory group holding the declarations.
pub const GROUP: &str = "dom-html";

/// Identifies actual DOM class references for XML receiver detection before injection.
pub(crate) fn program_uses_dom_html(program: &[crate::parser::ast::Stmt]) -> bool {
    detect::program_uses_dom_html(program)
}

/// The generated base of the walker part, before the packages' contributions are merged in.
///
/// Declarations are built by `synthetic_class` builders, one fn per declaration; spans are
/// stamped by `extend::build` (a non-dummy declaration span marks an elephc prelude class as
/// user-declared for PHP's coercive parameter binding, and distinct synthetic spans keep
/// span-keyed checker maps from conflating built nodes).
pub(crate) fn base_declarations() -> Program {
    let mut program = Vec::new();
    program.extend(entities::declarations());
    program.extend(attributes::declarations());
    program.extend(node_cache::declarations());
    program.extend(text::declarations());
    program.extend(generated::declarations());
    program.push(modern_dom_declaration());
    program
}

/// Every declaration of every part (what `inject` and the contract tests see).
pub(crate) fn declarations() -> Program {
    extend::build(PartSet::ALL)
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

/// Prepends the DOM declarations unconditionally, without recording them anywhere.
///
/// Used by the frontend harnesses that type-check a snippet against the surface; the compiler
/// itself goes through `inject_if_used()`.
pub fn inject(program: Program) -> Program {
    let mut combined = declarations();
    combined.extend(program);
    combined
}

/// Prepends `DOMException` alone when it is not declared yet.
///
/// The `Dom\` classes of `crate::dom_modern_prelude` throw it from their mutators; the program
/// that names them need not name `DOMException` itself, so the modern prelude pulls it in here.
/// A later autoload pass can still upgrade this exception-only injection to the whole surface.
pub(crate) fn inject_exception(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    use crate::parser::ast::StmtKind;

    if inventory.groups.get(GROUP).is_some_and(|group| group.classes.contains("domexception")) {
        return program;
    }
    let mut prelude = extend::build(PartSet::of(Part::Walker));
    prelude.retain(|stmt| {
        matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name.eq_ignore_ascii_case("DOMException"))
    });
    inventory.record_program(GROUP, &prelude);
    prelude.extend(program);
    prelude
}

/// Prepends the DOM prelude when the program names a DOM class or calls a `libxml_*` function.
///
/// `force` exists for the codegen harness and future opt-in (every part); ordinary compiles
/// pass `false` and rely on `detect::scan`.
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
    let Some(plan) = pending_prelude(&program, force, inventory) else {
        return program;
    };
    plan.record(inventory);
    let walker = plan.walker;
    let combined = extend::merge_delta(program, plan.prelude, plan.stubs);
    if walker { crate::core_prelude::ensure_weak_reference(combined, inventory) }
    else { combined }
}

/// What one injection pass adds.
struct Plan {
    /// New top-level declarations to prepend.
    prelude: Program,
    /// Members to append to classes the program already holds (a part needed after the walker).
    stubs: Program,
    /// Parts injected by this pass.
    parts: PartSet,
    /// Whether this pass brings the walker (and so needs `WeakReference`).
    walker: bool,
}

impl Plan {
    fn record(&self, inventory: &mut crate::optimize::reachability::PreludeInventory) {
        inventory.record_program(GROUP, &self.prelude);
        for part in [Part::Html, Part::Libxml, Part::XPath] {
            if self.parts.contains(part) {
                inventory.record_program(extend::part_marker(part), &[]);
            }
        }
    }
}

/// The parts already injected, from the inventory.
fn parts_present(inventory: &crate::optimize::reachability::PreludeInventory) -> PartSet {
    let mut have = PartSet::EMPTY;
    if inventory.groups.get(GROUP).is_some_and(|group| group.classes.contains("domdocument")) {
        have = have.with(Part::Walker);
    }
    for part in [Part::Html, Part::Libxml, Part::XPath] {
        if inventory.groups.contains_key(extend::part_marker(part)) {
            have = have.with(part);
        }
    }
    have
}

/// Selects what is missing, allowing autoload to upgrade an exception-only prelude, to add the
/// walker after `libxml_*` alone, and to add `Html`/`XPath` after the walker.
fn pending_prelude(
    program: &[crate::parser::ast::Stmt],
    force: bool,
    inventory: &crate::optimize::reachability::PreludeInventory,
) -> Option<Plan> {
    use crate::parser::ast::StmtKind;

    let detected = detect::scan(program);
    let want = if force { PartSet::ALL } else { detected.parts() };
    let exception_wanted = force || detected.exception || want.contains(Part::Walker);
    let have = parts_present(inventory);
    let exception_have = have.contains(Part::Walker)
        || inventory.groups.get(GROUP).is_some_and(|group| group.classes.contains("domexception"));
    let missing = want.without(have);
    let is_exception = |stmt: &crate::parser::ast::Stmt| matches!(
        &stmt.kind, StmtKind::ClassDecl { name, .. } if name.eq_ignore_ascii_case("DOMException")
    );

    let (mut prelude, stubs, walker);
    if missing.contains(Part::Walker) {
        prelude = extend::build(missing);
        prelude.retain(|stmt| !(exception_have && is_exception(stmt)));
        stubs = Vec::new();
        walker = true;
    } else {
        let (fresh, delta_stubs) = extend::delta(missing);
        prelude = fresh;
        stubs = delta_stubs;
        walker = false;
        if exception_wanted && !exception_have {
            let mut exception = extend::build(PartSet::of(Part::Walker));
            exception.retain(is_exception);
            exception.extend(prelude);
            prelude = exception;
        }
    }
    if prelude.is_empty() && stubs.is_empty() {
        return None;
    }
    Some(Plan { prelude, stubs, parts: missing, walker })
}

/// Discovers DOM references loaded by autoload without reinjecting an existing prelude.
/// Only the new declarations are resolved, preserving already-resolved application names.
pub fn inject_after_autoload(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        let Some(plan) = pending_prelude(&program, false, inventory) else {
            return Ok(program);
        };
        let prelude = crate::name_resolver::resolve(plan.prelude)?;
        let stubs = crate::name_resolver::resolve(plan.stubs)?;
        let plan = Plan { prelude, stubs, parts: plan.parts, walker: plan.walker };
        plan.record(inventory);
        let walker = plan.walker;
        let combined = extend::merge_delta(program, plan.prelude, plan.stubs);
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
            .filter(|class| matches!(class.module, PhpModule::Dom | PhpModule::Libxml))
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
<<<<<<< HEAD
                // package P3
                "DOMNamedNodeMap",
                "DOMCdataSection",
                "DOMAttr",
                "DOMDocumentFragment",
                "DOMDocumentType",
=======
                // Part::Libxml (package P2)
                "__ElephcDomState",
                "LibXMLError",
>>>>>>> 0d2e7ac2fc (feat(dom): libxml state, LibXMLError, libxml_* functions and the missing LIBXML/XML/DOM constants with contract rows)
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

    fn parts_of(source: &str) -> PartSet {
        detect::scan(&crate::parser::parse_test_source(source).unwrap()).parts()
    }

    /// A program that loads XML does not pay for the HTML4 part; naming an HTML method does.
    #[test]
    fn html_part_is_gated_on_html_method_names() {
        let xml = parts_of("<?php $d = new DOMDocument(); $d->loadXML('<a/>'); echo $d->saveXML();");
        assert!(xml.contains(Part::Walker) && !xml.contains(Part::Html));
        for source in [
            "<?php $d = new DOMDocument(); $d->loadHTML('<p>x</p>');",
            "<?php $d = new DOMDocument(); echo $d->saveHTML();",
            "<?php $d = new DOMDocument(); $f = $d->loadHTMLFile(...);",
            "<?php $d = new DOMDocument(); $m = 'loadHTML'; $d->$m('x');",
            "<?php $d = new DOMDocument(); call_user_func([$d, 'saveHTMLFile'], 'f');",
        ] {
            assert!(parts_of(source).contains(Part::Html), "{source}");
        }
        // HTML methods without any DOM class never inject anything.
        assert!(parts_of("<?php $o->loadHTML('x');").is_empty());
    }

    /// `libxml_*` calls and `LibXMLError` select the Libxml part alone, without the walker.
    #[test]
    fn libxml_part_is_independent_of_the_walker() {
        let calls = parts_of("<?php libxml_use_internal_errors(true);");
        assert!(calls.contains(Part::Libxml) && !calls.contains(Part::Walker));
        let class = parts_of("<?php function f(LibXMLError $e) {}");
        assert!(class.contains(Part::Libxml) && !class.contains(Part::Walker));
        let both = parts_of("<?php new DOMDocument(); \\libxml_get_errors();");
        assert!(both.contains(Part::Libxml) && both.contains(Part::Walker));
        assert!(parts_of("<?php $x = 'unrelated_libxml_';").is_empty());
    }

    /// `DOMXPath` implies the walker; a user-owned `DOMXPath` opts out.
    #[test]
    fn xpath_part_implies_walker() {
        let parts = parts_of("<?php $x = new DOMXPath($d);");
        assert!(parts.contains(Part::Walker) && parts.contains(Part::XPath));
        assert!(parts_of("<?php namespace App; class DOMXPath {} new DOMXPath();").is_empty());
    }

    /// Whatever the part set, no declaration appears twice.
    #[test]
    fn building_parts_never_duplicates_a_declaration() {
        for bits in [PartSet::ALL, PartSet::of(Part::Walker), PartSet::of(Part::Libxml)] {
            let program = extend::build(bits);
            let mut names = std::collections::HashSet::new();
            for stmt in &program {
                if let StmtKind::ClassDecl { name, .. } = &stmt.kind {
                    assert!(names.insert(name.to_ascii_lowercase()), "{name} declared twice");
                }
            }
        }
    }

    /// Baseline of the injected prelude, printed for the migration packages to compare against
    /// (`cargo test --lib prelude_size_baseline -- --nocapture`). The walker part must never
    /// shrink below what a part adds on top of it, and a bare `DOMDocument` + `loadXML` program
    /// must not receive the HTML part.
    #[test]
    fn prelude_size_baseline() {
        let count = |program: &Program| {
            let (mut stmts, mut exprs) = (0usize, 0usize);
            crate::parser::ast::visit::walk_nodes(program, &mut |node| match node {
                crate::parser::ast::visit::Node::Stmt(_) => stmts += 1,
                crate::parser::ast::visit::Node::Expr(_) => exprs += 1,
            });
            (stmts, exprs)
        };
        let walker = extend::build(PartSet::of(Part::Walker));
        let with_html = extend::build(PartSet::of(Part::Walker).with(Part::Html));
        let all = extend::build(PartSet::ALL);
        for (label, program) in [("walker", &walker), ("walker+html", &with_html), ("all", &all)] {
            let (stmts, exprs) = count(program);
            println!("dom-html prelude {label}: {} top-level, {stmts} stmts, {exprs} exprs", program.len());
        }
        assert!(count(&walker).0 <= count(&with_html).0 && count(&with_html).0 <= count(&all).0);
        let source = "<?php $d = new DOMDocument(); $d->loadXML('<a/>'); echo $d->saveXML();";
        let program = crate::parser::parse_test_source(source).unwrap();
        let started = std::time::Instant::now();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, false, &mut inventory);
        println!("dom-html bare loadXML program: {} top-level after injection in {:?}", injected.len(), started.elapsed());
        assert!(!inventory.groups.contains_key(extend::part_marker(Part::Html)));
    }
}
