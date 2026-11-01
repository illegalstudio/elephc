//! Purpose:
//! The parse-parity oracle for the built classic DOM surface: `build::dom_declarations` must equal
//! the parse of the PHP form in `fragments`, declaration by declaration.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Spans are stripped because a built node has them synthetic and a parsed one real.
//! - `ELEPHC_DOM_ORACLE_OUT=<dir>` writes both renderings of a diverging declaration, and
//!   `ELEPHC_DOM_ORACLE_SRC=<php file>` parses that file instead of `fragments::SRC` (used while
//!   the model is edited and re-transcribed).
//! - The test thread needs a big stack: the declarations are deeply nested. Run with
//!   `RUST_MIN_STACK=536870912`.

use super::*;

/// Removes `Span { ... }` renderings so parsed and built nodes compare equal.
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

/// Parses the PHP model the way an internal prelude is parsed.
fn parsed_prelude() -> Program {
    let source = match std::env::var("ELEPHC_DOM_ORACLE_SRC") {
        Ok(path) => std::fs::read_to_string(path).expect("read the PHP model"),
        Err(_) => super::fragments::SRC.to_string(),
    };
    let tokens = crate::lexer::tokenize(&source).expect("DOM prelude PHP must tokenize");
    crate::parser::parse_internal(&tokens).expect("DOM prelude PHP must parse")
}

/// The built declarations are the parse of the PHP form, node for node.
#[test]
fn built_declarations_match_the_php() {
    let parsed = parsed_prelude();
    let built = declarations();
    assert_eq!(built.len(), parsed.len(), "declaration COUNT differs");
    for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
        let left = strip_spans(&format!("{built_stmt:?}"));
        let right = strip_spans(&format!("{parsed_stmt:?}"));
        if left != right {
            if let Ok(dir) = std::env::var("ELEPHC_DOM_ORACLE_OUT") {
                std::fs::write(format!("{dir}/built.txt"), left.replace("}, ", "},\n")).expect("dump built");
                std::fs::write(format!("{dir}/parsed.txt"), right.replace("}, ", "},\n")).expect("dump parsed");
            }
            panic!("a built declaration differs from its PHP form");
        }
    }
}

/// The surface: the node classes, the constants, and the `libxml_*` functions.
#[test]
fn declares_the_classic_surface() {
    let mut classes = Vec::new();
    let mut functions = Vec::new();
    let mut constants = Vec::new();
    for stmt in declarations() {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } => classes.push(name.clone()),
            StmtKind::FunctionDecl { name, .. } => functions.push(name.clone()),
            StmtKind::ConstDecl { name, .. } => constants.push(name.clone()),
            other => panic!("unexpected prelude statement {other:?}"),
        }
    }
    for expected in [
        "DOMException", "DOMNode", "DOMDocument", "DOMElement", "DOMText", "DOMComment",
        "DOMCdataSection", "DOMAttr", "DOMDocumentFragment", "DOMDocumentType",
        "DOMProcessingInstruction", "DOMCharacterData", "DOMNodeList", "DOMNamedNodeMap", "LibXMLError",
    ] {
        assert!(classes.iter().any(|name| name == expected), "{expected} must be declared");
    }
    for expected in ["libxml_use_internal_errors", "libxml_clear_errors", "libxml_get_errors"] {
        assert!(functions.iter().any(|name| name == expected), "{expected} must be declared");
    }
    let mut declared: Vec<&str> = constants.iter().map(String::as_str).collect();
    declared.sort_unstable();
    let mut listed: Vec<&str> = PRELUDE_CONSTANTS.to_vec();
    listed.sort_unstable();
    assert_eq!(declared, listed, "PRELUDE_CONSTANTS must list exactly the declared constants");
}

/// A program that names a DOM class gets the surface, recorded in the `dom` group; a program that
/// never does is returned untouched.
#[test]
fn injection_is_gated_on_use() {
    let parse = |source: &str| {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    };
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let plain = parse("<?php echo strlen('x');");
    assert_eq!(inject_if_used(plain.clone(), &mut inventory).len(), plain.len());
    assert!(!inventory.groups.contains_key(GROUP));

    let named = parse("<?php $d = new \\DOMDocument(); echo $d->saveXML();");
    let injected = inject_if_used(named.clone(), &mut inventory);
    assert!(injected.len() > named.len());
    assert!(inventory.groups.contains_key(GROUP));

    // Injecting twice (the early and the late gate) must not duplicate the declarations.
    let again = inject_if_used(injected.clone(), &mut inventory);
    assert_eq!(again.len(), injected.len());
}

/// A function-only user of the libxml error switches also pulls the surface in.
#[test]
fn a_libxml_function_call_injects_the_surface() {
    let tokens = crate::lexer::tokenize("<?php libxml_use_internal_errors(true);").expect("tokenize");
    let program = crate::parser::parse(&tokens).expect("parse");
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    let injected = inject_if_used(program.clone(), &mut inventory);
    assert!(injected.len() > program.len());
}

/// A program that declares one of the classes itself keeps its own and gets no prelude.
#[test]
fn a_program_declaring_a_dom_class_is_left_alone() {
    let tokens = crate::lexer::tokenize("<?php class DOMDocument {} new DOMDocument();").expect("tokenize");
    let program = crate::parser::parse(&tokens).expect("parse");
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    assert_eq!(inject_if_used(program.clone(), &mut inventory).len(), program.len());
}
