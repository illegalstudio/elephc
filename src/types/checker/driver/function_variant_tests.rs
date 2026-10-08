//! Purpose:
//! Regression tests for checker signature agreement across include-loaded function variants.
//!
//! Called from:
//! - The `driver::functions` unit test module.
//!
//! Key details:
//! - Synthetic group statements reproduce the resolver's input without filesystem fixtures.
//! - Diagnostics must identify the conflicting declaration before a recursive body uses the group.

use crate::codegen_support::platform::Target;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

/// Parses two declarations and appends the group metadata normally emitted by the resolver.
fn grouped_program(source: &str) -> Program {
    let tokens = crate::lexer::tokenize(source).expect("tokenize fixture");
    let mut program = crate::parser::parse(&tokens).expect("parse fixture");
    program.push(Stmt::new(
        StmtKind::FunctionVariantGroup {
            name: "selected".to_string(),
            variants: vec!["left_variant".to_string(), "right_variant".to_string()],
        },
        Span::dummy(),
    ));
    program
}

/// Declared recursive returns disagree before the first group's placeholder can mask the cause.
#[test]
fn variant_signature_mismatch_precedes_recursive_return_error() {
    let program = grouped_program(
        r#"<?php
function left_variant(int $n): string { if ($n == 0) { return "left"; } return selected($n - 1); }
function right_variant(int $n): int { if ($n == 0) { return 1; } return selected($n - 1); }
echo selected(1);
"#,
    );
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        let result = crate::types::check_with_target(&program, Target::parse(target).unwrap());
        let error = match result {
            Ok(_) => panic!("conflicting variant returns must fail on {target}"),
            Err(error) => error,
        };
        let mismatches: Vec<_> = error.flatten().into_iter().filter(|error| {
            error.message.contains("Function variants for 'selected' must have identical signatures")
        }).collect();
        assert_eq!(mismatches.len(), 1, "{target}: report the declaration mismatch once");
        assert_eq!(mismatches[0].span.line, 3, "{target}: point at the conflicting declaration");
    }
}

/// Unhinted parameters still specialize to the caller's type after contract preflight.
#[test]
fn variant_signature_matching_inferred_parameters_are_preserved() {
    let program = grouped_program(
        "<?php\nfunction left_variant($value) { return $value; }\nfunction right_variant($value) { return $value; }\necho selected('value');",
    );
    let checked = crate::types::check(&program).expect("matching inferred parameters remain valid");
    assert_eq!(checked.functions["selected"].params[0].1, crate::types::PhpType::Str);
    assert_eq!(checked.functions["selected"].return_type, crate::types::PhpType::Str);
}

/// Inferred return mismatches also point at the later declaration instead of a dummy group span.
#[test]
fn variant_signature_inferred_mismatch_uses_conflicting_declaration_span() {
    let program = grouped_program(
        "<?php\nfunction left_variant() { return 1; }\nfunction right_variant() { return 'right'; }\necho selected();",
    );
    let error = match crate::types::check(&program) {
        Ok(_) => panic!("different inferred returns must fail"),
        Err(error) => error,
    };
    let mismatch = error.flatten().into_iter().find(|error| {
        error.message.contains("Function variants for 'selected' must have identical signatures")
    }).expect("report inferred signature disagreement");
    assert_eq!(mismatch.span.line, 3);
}

/// Identical unhinted string returns are inferred normally rather than compared to Int placeholders.
#[test]
fn variant_signature_matching_inferred_returns_are_preserved() {
    let program = grouped_program(
        "<?php\nfunction left_variant() { return 'left'; }\nfunction right_variant() { return 'right'; }\necho selected();",
    );
    let checked = crate::types::check(&program).expect("matching inferred variants remain valid");
    assert_eq!(checked.functions["selected"].return_type, crate::types::PhpType::Str);
}

/// A yielding body does not change the declared contract compared during preflight.
#[test]
fn variant_signature_same_annotations_ignore_generator_placeholder() {
    let program = grouped_program("<?php\nfunction left_variant(): int { return 1; }\nfunction right_variant(): int { yield 1; }\necho selected();");
    let errors = crate::types::check(&program).err().expect("invalid generator hint must fail").flatten();
    assert!(errors.iter().any(|error| error.message.contains("Generator")), "{errors:?}");
    assert!(!errors.iter().any(|error| error.message.contains("must have identical signatures")), "{errors:?}");
}

/// Two Generator placeholders cannot hide different explicit return annotations.
#[test]
fn variant_signature_different_generator_annotations_report_contract_mismatch() {
    let program = grouped_program("<?php\nfunction left_variant(): string { yield 'left'; }\nfunction right_variant(): int { yield 1; }\necho selected();");
    let errors = crate::types::check(&program).err().expect("different annotations must fail").flatten();
    let mismatches: Vec<_> = errors.iter().filter(|error| error.message.contains("must have identical signatures")).collect();
    assert_eq!(mismatches.len(), 1, "{errors:?}");
    assert_eq!(mismatches[0].span.line, 3);
}

/// Call-site specialization disagreements retain the conflicting declaration location.
#[test]
fn variant_signature_specialized_mismatch_uses_conflicting_declaration_span() {
    let program = grouped_program("<?php\nfunction left_variant($value) { return $value; }\nfunction right_variant($value) { return strlen($value); }\necho selected('value');");
    let errors = crate::types::check(&program).err().expect("specialized returns disagree").flatten();
    let mismatches: Vec<_> = errors.iter().filter(|error| error.message.contains("must have identical signatures")).collect();
    assert_eq!(mismatches.len(), 1, "{errors:?}");
    assert_eq!(mismatches[0].span.line, 3);
}

/// A temporary inferred mismatch can still recover after argument-driven specialization.
#[test]
fn variant_signature_inferred_placeholder_mismatch_can_recover() {
    let program = grouped_program("<?php\nfunction left_variant($value) { return $value; }\nfunction right_variant($value) { return 'right'; }\necho selected('value');");
    let checked = crate::types::check(&program).expect("only declared contract failures are cached");
    assert_eq!(checked.functions["selected"].return_type, crate::types::PhpType::Str);
}

/// A group first visited from a variant body is compared after that body finishes.
#[test]
fn variant_signature_recursive_inferred_mismatch_without_top_level_call() {
    let program = grouped_program("<?php\nfunction left_variant() { selected(); return 'left'; }\nfunction right_variant() { return 1; }");
    for _ in 0..8 {
        let errors = crate::types::check(&program).err().expect("completed variant returns disagree").flatten();
        let mismatches: Vec<_> = errors.iter().filter(|error| error.message.contains("must have identical signatures")).collect();
        assert_eq!(mismatches.len(), 1, "{errors:?}");
        assert_eq!(mismatches[0].span.line, 3);
    }
}

/// Matching variants replace a recursive placeholder even when no outer call visits the group.
#[test]
fn variant_signature_recursive_matching_returns_without_top_level_call() {
    let program = grouped_program("<?php\nfunction left_variant() { selected(); return 'left'; }\nfunction right_variant() { return 'right'; }");
    for _ in 0..8 {
        let checked = crate::types::check(&program).expect("matching inferred variants remain valid");
        assert_eq!(checked.functions["selected"].return_type, crate::types::PhpType::Str);
    }
}

/// Contract preflight and unchecked resolution report each invalid declaration once.
#[test]
fn variant_signature_unknown_return_annotation_is_not_duplicated() {
    let program = grouped_program("<?php\nfunction left_variant(): NotAClass { return null; }\nfunction right_variant(): NotAClass { return null; }\necho selected();");
    let errors = crate::types::check(&program).err().expect("unknown return annotations must fail").flatten();
    for line in [2, 3] {
        let declarations: Vec<_> = errors.iter().filter(|error| error.span.line == line && error.message.contains("NotAClass")).collect();
        assert_eq!(declarations.len(), 1, "{errors:?}");
    }
}
