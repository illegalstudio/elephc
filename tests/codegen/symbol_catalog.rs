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
    // One literal native probe per class (AOT folds `class_exists()` from a string literal
    // only), and ONE `eval()` that loops over the same list: an `eval()` call per class made
    // the compiled program too slow to build inside nextest's per-test budget on macOS.
    let mut source = String::from("<?php\nget_declared_classes();\n$native = \"\";\n");
    // The eval side carries its own copy of the probe list inside the eval'd source (no
    // host-scope capture): names sit in double-quoted PHP strings inside a single-quoted
    // eval string, so a namespace separator needs four backslashes here.
    let mut eval_source = String::from("$probes = [");
    for class in &probed {
        let (kind, predicate) = match class.kind {
            ClassKind::Class => ("c", "class_exists"),
            ClassKind::Interface => ("i", "interface_exists"),
            ClassKind::Enum => ("e", "enum_exists"),
            ClassKind::Trait => ("t", "trait_exists"),
        };
        let native_name = class.name.replace('\\', "\\\\");
        source.push_str(&format!(
            "$native .= {predicate}(\"{native_name}\") ? \"1\" : \"0\";\n"
        ));
        let eval_name = class.name.replace('\\', "\\\\\\\\");
        eval_source.push_str(&format!("[\"{kind}\", \"{eval_name}\"], "));
    }
    eval_source.push_str(
        "];\n$r = \"\";\nforeach ($probes as $probe) {\n\
             $kind = $probe[0]; $name = $probe[1];\n\
             if ($kind === \"c\") { $ok = class_exists($name); }\n\
             elseif ($kind === \"i\") { $ok = interface_exists($name); }\n\
             elseif ($kind === \"e\") { $ok = enum_exists($name); }\n\
             else { $ok = trait_exists($name); }\n\
             $r .= $ok ? \"1\" : \"0\";\n\
         }\nreturn $r;",
    );
    source.push_str(&format!("$in_eval = eval('{eval_source}');\necho $native, \":\", $in_eval;\n"));
    let out = compile_and_run(&source);
    let (native_bits, eval_bits) = out
        .trim()
        .split_once(':')
        .unwrap_or_else(|| panic!("native and eval probe strings; output was {out:?}"));
    assert_eq!(native_bits.len(), probed.len(), "one native probe per class; output was {out:?}");
    assert_eq!(eval_bits.len(), probed.len(), "one eval probe per class; output was {out:?}");

    let mut mismatches = Vec::new();
    for (index, class) in probed.iter().enumerate() {
        let native = native_bits.as_bytes()[index] == b'1';
        let in_eval = eval_bits.as_bytes()[index] == b'1';
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
