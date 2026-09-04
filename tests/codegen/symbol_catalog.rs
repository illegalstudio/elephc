//! Purpose:
//! End-to-end joins between the shared symbol catalog (`elephc-builtin-contract`) and what a
//! compiled program actually exposes, natively and inside `eval()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests symbol_catalog` through Rust's test harness.
//!
//! Key details:
//! - One compiled program probes every checker-provided class-like with the PHP predicate of
//!   its kind, so the catalog's AOT and eval routes are checked against real behavior rather
//!   than against another list.

use elephc_builtin_contract::{
    classes, eval_class_support, BackendSupport, ClassKind, ClassRoute,
};

use crate::support::*;

/// Verifies every checker-provided catalogued class-like exists natively, and exists inside
/// `eval()` exactly when the catalog's eval route says so.
#[test]
fn test_checker_provided_classes_exist_natively_and_in_eval_per_catalog() {
    let probed: Vec<_> = classes()
        .iter()
        .filter(|class| {
            !class.internal
                && matches!(
                    class.aot,
                    ClassRoute::CheckerInjected | ClassRoute::LanguageIntrinsic
                )
        })
        .collect();
    let mut source = String::from("<?php\nget_declared_classes();\n");
    for class in &probed {
        let predicate = match class.kind {
            ClassKind::Class => "class_exists",
            ClassKind::Interface => "interface_exists",
            ClassKind::Enum => "enum_exists",
            ClassKind::Trait => "trait_exists",
        };
        let name = class.name.replace('\\', "\\\\");
        source.push_str(&format!(
            "echo {predicate}(\"{name}\") ? \"1\" : \"0\";\n\
             echo eval('return {predicate}(\"{name}\") ? \"1\" : \"0\";');\n\
             echo \":\";\n"
        ));
    }
    let out = compile_and_run(&source);
    let results: Vec<&str> = out.trim_end_matches(':').split(':').collect();
    assert_eq!(results.len(), probed.len(), "one probe per class; output was {out:?}");

    let mut mismatches = Vec::new();
    for (class, result) in probed.iter().zip(results) {
        let native = result.starts_with('1');
        let in_eval = result.ends_with('1');
        let eval_expected = matches!(eval_class_support(class), BackendSupport::Implemented(_));
        if !native {
            mismatches.push(format!("{}: not found natively", class.name));
        }
        if in_eval != eval_expected {
            mismatches.push(format!(
                "{}: eval() says {in_eval}, catalog eval route says {eval_expected}",
                class.name
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "class catalog and compiled behavior disagree:\n{}",
        mismatches.join("\n")
    );
}
