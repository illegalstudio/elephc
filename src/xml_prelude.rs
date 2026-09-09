//! Purpose:
//! PHP's `ext/xml` and `ext/xmlwriter` surfaces in elephc-PHP: the `XMLParser` and
//! `XMLWriter` classes plus every `xml_*` / `xmlwriter_*` function, built as AST on top of
//! the `elephc_xml` bridge's `extern` block. PHP 8 models both a parser and a writer as an
//! OBJECT, and this prelude is what makes `xml_parser_create()` / `xmlwriter_open_memory()`
//! return real objects that `is_object`, `get_class`, `instanceof` and `var_dump` agree about.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, after
//!   the curl prelude and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A NATIVE CLASS. Declaring the `extern "elephc_xml"` block is what
//!   links `libelephc_xml.a`, so the surface is bridge-gated: a program that never touches
//!   XML declares neither class and links nothing. The whole feature then compiles through
//!   the ordinary class/function pipeline with NO new assembly, on every supported target
//!   at once, and `eval()` reaches the very same compiled declarations through the
//!   native-function/native-class bridge instead of a second implementation.
//! - WHY IT IS BUILT, NOT PARSED. `build` carries the declarations as `synthetic_class`
//!   builders transcribed from the PHP form in `fragments`, which stays in the tree under
//!   `cfg(test)` as the node-by-node parse-parity oracle.
//! - ONE REGISTRY BUILTIN. `xml_parse_into_struct()` writes two by-reference outputs that
//!   PHP code never predeclares; only a registry builtin's checker hook can accept that, so
//!   `crate::builtins::xml::xml_parse_into_struct` composes the prelude's
//!   `__elephc_xml_parse_into_struct` / `__elephc_xml_struct_values` /
//!   `__elephc_xml_struct_index` helpers, and the reachability scan keeps those helpers
//!   alive whenever the builtin is called.
//! - HANDLE OWNERSHIP. Each object holds the bridge handle as a plain `int` and frees it
//!   from `__destruct` (the `GdImage` model); the bridge's free is idempotent, so there is
//!   no resource-kind cleanup ladder to extend.
//! - EVERY wrapper BINDS `$object->__elephc_handle` TO A LOCAL (`$raw`) BEFORE CALLING, the
//!   same rule the hash and curl preludes document for `mixed` properties.

mod build;
mod detect;
#[cfg(test)]
mod fragments;

use crate::parser::ast::Program;

/// Builds the xml surface: the extern block, `XMLParser`, the `xml_*` functions,
/// `XMLWriter` and the `xmlwriter_*` functions.
pub(crate) fn xml_declarations() -> Program {
    build::xml_declarations()
}

/// Injects the xml prelude when the program references the `ext/xml` / `ext/xmlwriter`
/// surface, leaving every other program untouched.
///
/// `force` comes from `--with-xml` (or the codegen harness); otherwise the decision is
/// `detect::program_uses_xml`. The prelude carries only declarations, so prepending it is
/// order-independent — PHP hoists them.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_xml(&program) {
        return program;
    }
    let mut combined = xml_declarations();
    inventory.record_program("xml", &combined);
    combined.extend(program);
    combined
}

/// Parses the PHP form of the prelude exactly as the compiler would at injection time;
/// the oracle's reference side.
#[cfg(test)]
pub(crate) fn parsed_prelude() -> Program {
    let tokens = crate::lexer::tokenize(fragments::SRC).expect("xml prelude PHP must tokenize");
    crate::parser::parse_internal(&tokens).expect("xml prelude PHP must parse")
}

#[cfg(test)]
mod oracle_tests {
    //! Purpose:
    //! The parse-parity oracle for the built xml surface: `build::xml_declarations` must
    //! equal the parse of the PHP form, declaration by declaration.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Spans are stripped because a built node has none and a parsed one does;
    //!   everything else — order, types, nesting, name qualification — must match.
    //! - `ELEPHC_XML_ORACLE_DUMP=<dir>` writes both renderings of a diverging declaration
    //!   out to be diffed, since one enormous line is undiffable.

    use super::*;
    use crate::parser::ast::{Stmt, StmtKind};

    /// The built declarations are the parse of the PHP form, node for node.
    #[test]
    fn built_declarations_match_the_php() {
        let parsed = parsed_prelude();
        let built = build::xml_declarations();
        assert_eq!(
            built.len(),
            parsed.len(),
            "declaration COUNT differs — built {} vs parsed {}",
            built.len(),
            parsed.len()
        );
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            let decl = declaration_label(parsed_stmt);
            let left = strip_spans(&format!("{built_stmt:?}"));
            let right = strip_spans(&format!("{parsed_stmt:?}"));
            if left != right {
                if let Ok(dir) = std::env::var("ELEPHC_XML_ORACLE_DUMP") {
                    std::fs::write(format!("{dir}/built_{decl}.txt"), left.replace("}, ", "},\n"))
                        .expect("dump built");
                    std::fs::write(
                        format!("{dir}/parsed_{decl}.txt"),
                        right.replace("}, ", "},\n"),
                    )
                    .expect("dump parsed");
                }
                panic!("built declaration `{decl}` differs from its PHP form");
            }
        }
    }

    /// The surface is fixed: the extern block, the two classes, the 54 prelude-provided
    /// functions and the twelve `__elephc_xml_*` twins the registry builtins lower to.
    #[test]
    fn declares_the_classes_and_every_function() {
        let mut classes = Vec::new();
        let mut functions = Vec::new();
        let mut externs = 0;
        for stmt in xml_declarations() {
            match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => classes.push(name.clone()),
                StmtKind::FunctionDecl { name, .. } => functions.push(name.clone()),
                StmtKind::ExternFunctionDecl { .. } => externs += 1,
                other => panic!("unexpected prelude statement {other:?}"),
            }
        }
        assert_eq!(classes, vec!["XMLParser", "XMLWriter"]);
        assert_eq!(externs, 69);
        assert_eq!(functions.len(), 66);
        let catalog: std::collections::BTreeSet<&str> = elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| contract.area == elephc_builtin_contract::Area::Xml)
            .filter(|contract| contract.kind == elephc_builtin_contract::BuiltinKind::PreludeProvided)
            .map(|contract| contract.name)
            .collect();
        let declared: std::collections::BTreeSet<&str> = functions
            .iter()
            .map(String::as_str)
            .filter(|name| !name.starts_with("__elephc_"))
            .collect();
        assert_eq!(declared, catalog, "prelude functions must be exactly the prelude-provided contracts");
        let helpers: std::collections::BTreeSet<&str> = elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| contract.area == elephc_builtin_contract::Area::Xml)
            .filter(|contract| contract.kind == elephc_builtin_contract::BuiltinKind::Function)
            .flat_map(|contract| crate::builtins::xml::prelude_helpers_for(contract.name))
            .copied()
            .collect();
        let twins: std::collections::BTreeSet<&str> = functions
            .iter()
            .map(String::as_str)
            .filter(|name| name.starts_with("__elephc_"))
            .collect();
        assert_eq!(twins, helpers, "every registry-builtin twin is declared, and nothing else");
    }

    /// `XMLParser` is final and its public constructor exists only to throw; a parser is
    /// minted by the two `xml_parser_create*` functions.
    #[test]
    fn the_parser_class_is_final_with_a_guarded_constructor() {
        let class_decl = xml_declarations()
            .into_iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "XMLParser"))
            .expect("XMLParser must be declared");
        let StmtKind::ClassDecl { methods, is_final, .. } = &class_decl.kind else {
            unreachable!("filtered above");
        };
        assert!(*is_final, "XMLParser is final");
        let ctor = methods
            .iter()
            .find(|method| method.name == "__construct")
            .expect("XMLParser must declare a constructor");
        assert_eq!(ctor.visibility, crate::parser::ast::Visibility::Public);
        assert!(methods.iter().any(|method| method.name == "__destruct"));
    }

    /// Names a top-level declaration for assertion messages and dump files.
    fn declaration_label(stmt: &Stmt) -> String {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. }
            | StmtKind::ClassDecl { name, .. }
            | StmtKind::ExternFunctionDecl { name, .. }
            | StmtKind::ConstDecl { name, .. } => name.clone(),
            other => format!("{other:?}").chars().take(40).collect(),
        }
    }

    /// Removes span payloads so a built node and a parsed node compare on structure alone.
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
}

#[cfg(test)]
mod inventory_tests {
    use super::*;
    use crate::names::php_symbol_key;

    /// `--with-xml` forces the surface through reachability via the prelude inventory:
    /// injection must record every declaration under the "xml" group so `forced_groups`
    /// can root it.
    #[test]
    fn inject_records_the_xml_prelude_group() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject_if_used(Vec::new(), true, &mut inventory);
        assert!(!program.is_empty(), "forced injection must produce the prelude");
        let group = inventory
            .groups
            .get("xml")
            .expect("injection must record the xml prelude group");
        for class in ["XMLParser", "XMLWriter"] {
            assert!(
                group.classes.contains(&php_symbol_key(class)),
                "{class} must be recorded in the xml group"
            );
        }
        for function in ["xml_parser_create", "xmlwriter_open_memory", "__elephc_xml_struct_values"] {
            assert!(
                group.functions.contains(&php_symbol_key(function)),
                "{function} must be recorded in the xml group"
            );
        }
    }

    /// A program that never references the surface is returned untouched.
    #[test]
    fn unrelated_programs_are_not_injected() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let tokens = crate::lexer::tokenize("<?php echo strlen('x');").expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let injected = inject_if_used(program.clone(), false, &mut inventory);
        assert_eq!(injected.len(), program.len());
        assert!(inventory.groups.get("xml").is_none());
    }
}
