//! Purpose:
//! The extension mechanism of the DOM prelude: later packages add classes, functions, members
//! and constants onto the classes built in `generated.rs` from their OWN files, and say which
//! optional part of the prelude pays for them.
//!
//! Called from:
//! - `crate::dom_html_prelude` (`build`, `delta`, `merge_delta`) and `detect.rs` (`Part`).
//!
//! # How a package registers code (the API)
//!
//! Every package owns one file under `src/dom_html_prelude/` that is already declared as a
//! module and already listed in [`PACKAGES`] here, each with an empty entry point:
//!
//! ```ignore
//! pub(super) fn contribute(c: &mut Contributions) { ... }
//! ```
//!
//! Inside it, a package calls only these functions:
//! - `c.members(Part::Walker, class("DOMNode").method(method("insertBefore")...).prop(..))`
//!   merges the methods, properties and constants of the (throwaway) `ClassBuilder` onto the
//!   ALREADY BUILT class of the same name (generated.rs or another package's `add_class`). A
//!   member the target class already has is an error (panic at build time, caught by the lib
//!   test `extension_members_do_not_collide`), so a package never silently shadows main's code.
//!   The target class must be present whenever the part is selected.
//! - `c.add_class(Part::Walker, class("DOMAttr").extends("DOMNode")....build())` adds a new
//!   top-level class declaration (use `class(..)` of `crate::synthetic_class`).
//! - `c.add_function(Part::Libxml, function("libxml_get_errors")....build())` adds a new
//!   top-level function declaration.
//!
//! A class added with `add_class` can be extended by `members`; packages run in the order of
//! [`PACKAGES`], so the class must be added by an earlier entry than the one that extends it
//! (or earlier in the same `contribute`).
//!
//! A new public class also needs a row in `crates/elephc-builtin-contract/src/catalog_classes.rs`
//! and a name in `detect.rs`'s `DOM_HTML_CLASSES` (P1 already lists the planned ones).
//!
//! # Parts (what a program pays for)
//!
//! Each registration names a [`Part`]; the compiler injects only the parts a program needs:
//! - `Part::Walker`: what a program that names any DOM class gets (the default).
//! - `Part::Html`: only when the program mentions `loadHTML`, `loadHTMLFile`, `saveHTML` or
//!   `saveHTMLFile` (method name, first-class callable, or a string spelling one): the HTML4
//!   parser and serializer. A `DOMDocument` + `loadXML` program does not pay for it.
//! - `Part::Libxml`: `libxml_*` functions, `LibXMLError` and the shared error state. Injected
//!   alone (without the DOM classes) when a program calls a `libxml_*` function, so it must not
//!   use `members` on walker classes.
//! - `Part::XPath`: `DOMXPath` and helpers, when the program names `DOMXPath`.
//! `Html` and `XPath` imply `Walker`.
//!
//! # Late injection
//!
//! The prelude is injected before name resolution and again after autoload. A part that becomes
//! necessary after the walker is already in the program (a vendor file calls `loadHTML`) is
//! merged in by [`merge_delta`]: new classes/functions are prepended and the members are
//! appended onto the existing class declarations.
//!
//! # What stays shared
//!
//! `__elephc_dom_*` names, `node_cache.rs` and the `__elephc_simplexml` binding are an ABI with
//! SimpleXML: extension code uses them and never renames them.

use std::sync::OnceLock;

use crate::parser::ast::{ClassConst, ClassMethod, ClassProperty, Program, Stmt, StmtKind};
use crate::span::Span;

/// One optional piece of the DOM prelude. See the module comment for what each pays for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Walker,
    Html,
    Libxml,
    XPath,
}

/// A set of [`Part`]s as a bitmask (4 bits, so built programs are cached by value).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct PartSet(u8);

impl PartSet {
    pub(crate) const EMPTY: PartSet = PartSet(0);
    pub(crate) const ALL: PartSet = PartSet(0b1111);

    pub(crate) fn of(part: Part) -> PartSet {
        PartSet(1 << part as u8)
    }
    pub(crate) fn with(self, part: Part) -> PartSet {
        PartSet(self.0 | PartSet::of(part).0)
    }
    pub(crate) fn without(self, other: PartSet) -> PartSet {
        PartSet(self.0 & !other.0)
    }
    pub(crate) fn contains(self, part: Part) -> bool {
        self.0 & PartSet::of(part).0 != 0
    }
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Inventory group id remembering that a part was injected (the walker is recognised by the
/// `DOMDocument` class of the `dom-html` group instead).
pub(crate) fn part_marker(part: Part) -> &'static str {
    match part {
        Part::Walker => "dom-html",
        Part::Html => "dom-html:html",
        Part::Libxml => "dom-html:libxml",
        Part::XPath => "dom-html:xpath",
    }
}

enum Entry {
    Members { part: Part, class: Stmt },
    Class { part: Part, stmt: Stmt },
    Function { part: Part, stmt: Stmt },
}

/// Collector handed to every package's `contribute`.
#[derive(Default)]
pub(crate) struct Contributions {
    entries: Vec<Entry>,
}

impl Contributions {
    /// Merges the members of `builder`'s class onto the built class of the same name.
    pub(crate) fn members(&mut self, part: Part, builder: crate::synthetic_class::ClassBuilder) {
        self.entries.push(Entry::Members { part, class: builder.build() });
    }

    /// Adds a new top-level class declaration.
    pub(crate) fn add_class(&mut self, part: Part, class: Stmt) {
        assert!(matches!(class.kind, StmtKind::ClassDecl { .. }), "add_class takes a class");
        self.entries.push(Entry::Class { part, stmt: class });
    }

    /// Adds a new top-level function declaration, or an `extern` function declaration (a native
    /// bridge entry point the part's code calls).
    pub(crate) fn add_function(&mut self, part: Part, function: Stmt) {
        assert!(
            matches!(function.kind, StmtKind::FunctionDecl { .. } | StmtKind::ExternFunctionDecl { .. }),
            "add_function takes a function"
        );
        self.entries.push(Entry::Function { part, stmt: function });
    }
}

/// Every package's entry point, in dependency order (each package edits only its own file).
pub(crate) const PACKAGES: &[fn(&mut Contributions)] = &[
    super::libxml::contribute,
    super::collections::contribute,
    super::chardata::contribute,
    super::node_tree::contribute,
    super::element_document::contribute,
    super::html_parser::contribute,
    super::writer_xml::contribute,
    super::writer_html::contribute,
    super::xpath::contribute,
];

fn collect() -> Contributions {
    let mut contributions = Contributions::default();
    for package in PACKAGES {
        package(&mut contributions);
    }
    contributions
}

type ClassParts<'a> = (&'a mut Vec<ClassProperty>, &'a mut Vec<ClassMethod>, &'a mut Vec<ClassConst>);

fn class_parts(stmt: &mut Stmt) -> ClassParts<'_> {
    match &mut stmt.kind {
        StmtKind::ClassDecl { properties, methods, constants, .. } => (properties, methods, constants),
        _ => unreachable!("extension targets are class declarations"),
    }
}

fn class_name(stmt: &Stmt) -> Option<&str> {
    match &stmt.kind {
        StmtKind::ClassDecl { name, .. } => Some(name),
        _ => None,
    }
}

/// Appends `from`'s members to `into`, refusing a collision with a member `into` already has.
fn merge_class(into: &mut Stmt, from: &Stmt) {
    let name = class_name(into).unwrap_or_default().to_string();
    let StmtKind::ClassDecl { properties, methods, constants, .. } = &from.kind else { return };
    let (into_properties, into_methods, into_constants) = class_parts(into);
    for method in methods {
        assert!(
            !into_methods.iter().any(|existing| existing.name.eq_ignore_ascii_case(&method.name)),
            "DOM extension: {name}::{} is already defined", method.name
        );
        into_methods.push(method.clone());
    }
    for property in properties {
        assert!(
            !into_properties.iter().any(|existing| existing.name == property.name),
            "DOM extension: {name}::${} is already defined", property.name
        );
        into_properties.push(property.clone());
    }
    for constant in constants {
        assert!(
            !into_constants.iter().any(|existing| existing.name == constant.name),
            "DOM extension: {name}::{} is already defined", constant.name
        );
        into_constants.push(constant.clone());
    }
}

/// Stamps every top-level declaration with a distinct synthetic span (see `declarations()`):
/// a non-dummy span marks a prelude class as user-declared for PHP's coercive binding.
fn stamp(program: &mut Program) {
    for declaration in program {
        if !declaration.span.identifies_a_node() {
            declaration.span = Span::synthetic();
        }
    }
}

/// Builds the program of `parts`: the generated base (when `Walker` is selected) with the members
/// of the selected parts merged in, then the selected new classes and functions.
fn build_uncached(parts: PartSet) -> Program {
    let mut program = if parts.contains(Part::Walker) { super::base_declarations() } else { Vec::new() };
    let mut fresh = Vec::new();
    for entry in collect().entries {
        match entry {
            Entry::Members { part, class } if parts.contains(part) => {
                let name = class_name(&class).unwrap_or_default().to_string();
                let target = program.iter_mut().chain(fresh.iter_mut())
                    .find(|stmt| class_name(stmt).is_some_and(|n| n.eq_ignore_ascii_case(&name)));
                match target {
                    Some(target) => merge_class(target, &class),
                    None => panic!("DOM extension: no class {name} to extend in {parts:?}"),
                }
            }
            Entry::Class { part, stmt } | Entry::Function { part, stmt } if parts.contains(part) => {
                fresh.push(stmt);
            }
            _ => {}
        }
    }
    program.extend(fresh);
    stamp(&mut program);
    program
}

/// The built program for `parts`, cached per distinct set.
pub(crate) fn build(parts: PartSet) -> Program {
    static CACHE: [OnceLock<Program>; 16] = [const { OnceLock::new() }; 16];
    CACHE[parts.0 as usize].get_or_init(|| build_uncached(parts)).clone()
}

/// The pieces of `parts` to add to a program that already holds the walker: `(fresh, stubs)`.
/// `fresh` are new classes and functions; `stubs` carry only the members added to existing
/// classes. Name-resolve both, then pass them to [`merge_delta`].
pub(crate) fn delta(parts: PartSet) -> (Program, Program) {
    let mut stubs = Vec::new();
    let mut fresh = Vec::new();
    for entry in collect().entries {
        match entry {
            Entry::Members { part, class } if parts.contains(part) => stubs.push(class),
            Entry::Class { part, stmt } | Entry::Function { part, stmt } if parts.contains(part) => {
                fresh.push(stmt);
            }
            _ => {}
        }
    }
    stamp(&mut fresh);
    stamp(&mut stubs);
    (fresh, stubs)
}

/// Merges a [`delta`] (already name-resolved) into `program`: stubs append their members to the
/// class of the same name (in `program`, or fresh in the same delta); fresh declarations are
/// prepended.
pub(crate) fn merge_delta(mut program: Program, mut fresh: Program, stubs: Program) -> Program {
    for stub in &stubs {
        let name = class_name(stub).unwrap_or_default().to_string();
        let target = program.iter_mut().chain(fresh.iter_mut())
            .find(|stmt| class_name(stmt).is_some_and(|n| n.eq_ignore_ascii_case(&name)));
        if let Some(target) = target {
            merge_class(target, stub);
        }
    }
    fresh.extend(program);
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic_class::{class, method};

    fn probe(name: &str) -> Stmt {
        class("DOMNode").method(method(name)).build()
    }

    /// Members of an extension land on the existing class; a collision is refused.
    #[test]
    fn extension_members_do_not_collide() {
        let mut target = class("DOMNode").method(method("existing")).build();
        merge_class(&mut target, &probe("added"));
        let StmtKind::ClassDecl { methods, .. } = &target.kind else { unreachable!() };
        assert_eq!(methods.len(), 2);
        let collision = std::panic::catch_unwind(|| {
            let mut again = class("DOMNode").method(method("added")).build();
            merge_class(&mut again, &probe("Added"));
        });
        assert!(collision.is_err());
    }

    /// A late delta prepends fresh declarations and appends stub members to existing classes.
    #[test]
    fn merge_delta_appends_members_and_prepends_fresh() {
        let program = vec![class("DOMNode").method(method("existing")).build()];
        let merged = merge_delta(program, vec![class("DOMAttr").build()], vec![probe("added")]);
        assert_eq!(class_name(&merged[0]), Some("DOMAttr"));
        let StmtKind::ClassDecl { methods, .. } = &merged[1].kind else { unreachable!() };
        assert_eq!(methods.len(), 2);
    }

    /// Part sets cache by value and the parts are separate bits.
    #[test]
    fn part_sets_are_distinct_bits() {
        let set = PartSet::EMPTY.with(Part::Html).with(Part::XPath);
        assert!(set.contains(Part::Html) && set.contains(Part::XPath));
        assert!(!set.contains(Part::Walker) && !set.contains(Part::Libxml));
        assert!(set.without(PartSet::of(Part::Html)).contains(Part::XPath));
    }
}
