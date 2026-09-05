//! Purpose:
//! Termwind-facing DOM HTML subset: `DOMDocument::loadHTML()` plus the node
//! types and properties `HtmlRenderer` / `ValueObjects\Node` walk
//! (`getElementsByTagName`, `childNodes`, `nodeName`, `nodeValue`,
//! `getAttribute`, sibling pointers, `saveXML`).
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via
//!   `inject_if_used`, after include resolution and before name resolution.
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
mod surface;

use std::sync::OnceLock;

use crate::parser::ast::Program;

/// Parsed prelude cache. The fragment is declaration-only, so one parse is
/// reused for every injecting compile.
static PARSED_PRELUDE: OnceLock<Program> = OnceLock::new();

/// Tokenizes and parses the DOM HTML prelude exactly once.
fn parsed_prelude() -> Program {
    PARSED_PRELUDE
        .get_or_init(|| {
            let source = format!("<?php\n{}", surface::SRC);
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
    if !force && !detect::program_uses_dom_html(&program) {
        return program;
    }
    let mut combined = parsed_prelude();
    inventory.record_program("dom-html", &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// The prelude must declare the Termwind node types and no others.
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
                "DOMNodeList",
                "DOMDocument",
                "DOMElement",
                "DOMCharacterData",
                "DOMText",
                "DOMComment",
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
