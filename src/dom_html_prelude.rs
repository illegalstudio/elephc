//! Purpose:
//! PHP 8.4's `Dom\HTMLDocument` and the node classes of its tree (`Dom\Node`, `Dom\Element`,
//! `Dom\Text`, `Dom\Comment`, `Dom\Attr`, `Dom\DocumentType`, `Dom\NodeList`,
//! `Dom\NamedNodeMap`, `Dom\HTMLCollection`), built as AST, with an HTML5 tokenizer and tree
//! builder behind `Dom\HTMLDocument::createFromString()`.
//!
//! Called from:
//! - `crate::pipeline::compile()`, twice like the other class-bearing preludes: once before name
//!   resolution and once after autoload expansion, since the program that names the surface is
//!   usually an autoloaded vendor class (`symfony/html-sanitizer`'s `NativeParser`).
//!
//! Key details:
//! - SCOPE. The classes cover what PHP's DOM exposes for an HTML document that code READS:
//!   parsing, the node and element properties, `childNodes` / `children` / `attributes`,
//!   `getAttribute()` and friends, `getElementsByTagName()`, `getElementById()`, `textContent`,
//!   and the mutators `appendChild()` / `insertBefore()` / `removeChild()` / `setAttribute()`.
//!   Serialization (`saveHtml()`, `innerHTML`), selectors (`querySelector*`), `XMLDocument`,
//!   `DocumentFragment` parsing and live collections are NOT provided: a `NodeList` and an
//!   `HTMLCollection` are snapshots rebuilt when the tree changes.
//! - THE PARSER. A WHATWG tree builder: every insertion mode, the adoption agency algorithm, the
//!   active formatting list, foster parenting, foreign content for svg and math, the character
//!   reference table (2,125 named references plus the 106 legacy forms that need no `;`).
//!   Differences from php's bundled lexbor that were measured and copied: `sup` is not a
//!   breakout element inside foreign content, and `<select>` content is inserted as written
//!   (the customizable-select parsing). Not copied: the reconstruction of formatting elements
//!   inside a `<textarea>` / `<title>`, and select content containing formatting elements.
//! - HOW IT WAS VERIFIED. The PHP model was run against `Dom\HTMLDocument` in one process over a
//!   curated list of 276 inputs and some 14,000 generated ones: identical trees (node types,
//!   names, attributes, text, doctype ids). The same source, compiled by elephc, produced output
//!   byte-identical to php's own classes on the curated list.
//! - ONE `final` PARSER CLASS, `Dom\__ElephcHtmlParser`, holds the tokenizer and the tree builder;
//!   it is internal and not part of PHP's surface.
//! - Pay-for-use: injected only when the program names a `Dom\` class, php >= 8.4, and the
//!   program declares none of its own `Dom\` classes. A program that reaches the surface only
//!   through `eval()` opaque source is not detected (there is no `--with-dom` flag yet).

mod build;
#[cfg(test)]
mod fragments;
#[cfg(test)]
mod oracle;

use crate::parser::ast::{Program, Stmt, StmtKind};
use elephc_builtin_contract::PhpVersion;

/// Prelude inventory group holding the declarations.
pub const GROUP: &str = "dom_html";

/// Builds the `Dom\` classes, one declaration per helper.
pub(crate) fn declarations() -> Program {
    build::dom_declarations()
}

/// Whether a top-level, namespace-less or synthetic statement declares a class in `Dom\`.
fn program_declares_dom_class(program: &[Stmt]) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. } | StmtKind::InterfaceDecl { name, .. } => {
            name.to_ascii_lowercase().starts_with("dom\\")
        }
        StmtKind::NamespaceBlock { name: Some(namespace), body } => {
            namespace.as_canonical().eq_ignore_ascii_case("Dom") || program_declares_dom_class(body)
        }
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_dom_class(body),
        _ => false,
    })
}

/// Whether the program names a class of the `Dom\` namespace, however the reference is written.
fn program_names_dom_class(program: &[Stmt]) -> bool {
    let used = crate::prelude_prune::usage::collect(program);
    used.classes
        .iter()
        .chain(used.literals.iter())
        .any(|name| name.trim_start_matches('\\').to_ascii_lowercase().starts_with("dom\\"))
}

/// Prepends the declarations when the program names a `Dom\` class and declares none itself.
///
/// `force` injects unconditionally (the codegen harness); below php 8.4 the classes do not
/// exist and nothing is injected.
pub fn inject_if_used(
    program: Program,
    php_version: PhpVersion,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if php_version < PhpVersion::Php84 {
        return program;
    }
    if inventory.groups.contains_key(GROUP) {
        return program;
    }
    if !force && !program_names_dom_class(&program) {
        return program;
    }
    if program_declares_dom_class(&program) {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod injection_tests {
    use super::*;

    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// A program naming `\Dom\HTMLDocument` gets the surface, recorded in the `dom_html` group.
    #[test]
    fn a_program_naming_a_dom_class_is_injected() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = parse("<?php $d = \\Dom\\HTMLDocument::createFromString('<p>x</p>');");
        let injected = inject_if_used(program.clone(), PhpVersion::Php85, false, &mut inventory);
        assert!(injected.len() > program.len());
        assert!(inventory.groups.contains_key(GROUP));
    }

    /// A program that never names the surface is returned untouched, and so is any program below
    /// php 8.4, where the classes do not exist.
    #[test]
    fn unrelated_programs_and_old_versions_are_not_injected() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let plain = parse("<?php echo strlen('x');");
        assert_eq!(
            inject_if_used(plain.clone(), PhpVersion::Php85, false, &mut inventory).len(),
            plain.len()
        );
        let named = parse("<?php new \\Dom\\Element('p');");
        assert_eq!(
            inject_if_used(named.clone(), PhpVersion::Php83, false, &mut inventory).len(),
            named.len()
        );
        assert!(!inventory.groups.contains_key(GROUP));
    }
}
