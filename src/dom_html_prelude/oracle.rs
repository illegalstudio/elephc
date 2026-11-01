//! Purpose:
//! The parse-parity oracle for the built `Dom\` surface: `build::dom_declarations` must equal the
//! parse of the PHP form in `fragments`, declaration by declaration.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Spans are stripped because a built node has them synthetic and a parsed one real.
//! - The PHP form spells class names `Dom__X`; the builders spell `Dom\X` (two name parts).
//!   `normalize_parsed` rewrites the parsed rendering accordingly.
//! - `ELEPHC_DOM_ORACLE_OUT=<dir>` writes both renderings of a diverging declaration, and
//!   `ELEPHC_DOM_ORACLE_SRC=<php file>` parses that file instead of `fragments::SRC` (used while
//!   the model is edited and re-transcribed).
//! - The test thread needs a big stack: the declarations are deeply nested. Run with
//!   `RUST_MIN_STACK=536870912`.

use super::*;

fn strip_spans(rendered: &str) -> String {
    let mut cleaned = String::with_capacity(rendered.len());
    let mut rest = rendered;
    while let Some(at) = rest.find("Span {") {
        cleaned.push_str(&rest[..at]);
        cleaned.push_str("Span");
        let after = &rest[at..];
        let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
        rest = &after[close..];
    }
    cleaned.push_str(rest);
    cleaned
}

/// Rewrites the parsed rendering's mangled class names to the builders' spelling.
fn normalize_parsed(rendered: &str) -> String {
    let marker = "parts: [\"Dom__";
    let mut out = String::with_capacity(rendered.len());
    let mut rest = rendered;
    while let Some(at) = rest.find(marker) {
        out.push_str(&rest[..at]);
        let after = &rest[at + marker.len()..];
        let end = after.find("\"]").expect("a name part list must close");
        out.push_str("parts: [\"Dom\", \"");
        out.push_str(&after[..end]);
        out.push_str("\"]");
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out.replace("Dom__", "Dom\\\\")
}

fn parsed_prelude() -> Program {
    let source = match std::env::var("ELEPHC_DOM_ORACLE_SRC") {
        Ok(path) => std::fs::read_to_string(path).expect("read the PHP model"),
        Err(_) => super::fragments::SRC.to_string(),
    };
    let tokens = crate::lexer::tokenize(&source).expect("Dom prelude PHP must tokenize");
    crate::parser::parse_internal(&tokens).expect("Dom prelude PHP must parse")
}

/// The built declarations are the parse of the PHP form, node for node.
#[test]
fn built_declarations_match_the_php() {
    let parsed = parsed_prelude();
    let built = declarations();
    assert_eq!(built.len(), parsed.len(), "declaration COUNT differs");
    for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
        let left = strip_spans(&format!("{built_stmt:?}"));
        let right = normalize_parsed(&strip_spans(&format!("{parsed_stmt:?}")));
        if left != right {
            if let Ok(dir) = std::env::var("ELEPHC_DOM_ORACLE_OUT") {
                std::fs::write(format!("{dir}/built.txt"), left.replace("}, ", "},\n")).expect("dump built");
                std::fs::write(format!("{dir}/parsed.txt"), right.replace("}, ", "},\n")).expect("dump parsed");
            }
            panic!("a built declaration differs from its PHP form");
        }
    }
}

/// The surface: the node classes and the parser, with the two entity tables.
#[test]
fn declares_the_node_classes_and_the_parser() {
    let mut classes = Vec::new();
    let mut functions = Vec::new();
    for stmt in declarations() {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } => classes.push(name.clone()),
            StmtKind::FunctionDecl { name, .. } => functions.push(name.clone()),
            other => panic!("unexpected prelude statement {other:?}"),
        }
    }
    for expected in [
        "Dom\\Node", "Dom\\Element", "Dom\\Text", "Dom\\Comment", "Dom\\Attr", "Dom\\DocumentType",
        "Dom\\Document", "Dom\\HTMLDocument", "Dom\\NodeList", "Dom\\HTMLCollection",
        "Dom\\NamedNodeMap", "Dom\\CharacterData", "Dom\\__ElephcHtmlParser",
    ] {
        assert!(classes.iter().any(|name| name == expected), "{expected} must be declared");
    }
    assert_eq!(functions, vec!["__elephc_dom_entity_table", "__elephc_dom_legacy_entities"]);
}
