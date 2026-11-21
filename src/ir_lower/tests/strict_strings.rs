//! Purpose:
//! Verifies string concatenation and explicit output formatting contracts.
//!
//! Called from:
//! - Focused staged strict-string compiler tests.
//!
//! Key details:
//! - PHP and LFC source share semantics while interpolation retains formatting intent.
//! - Native, eval and public admission tests complement these focused compiler controls.

use crate::source::SourceMode;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Prepared producer operations and their physical output consumer retain independent policies.
#[test]
fn strict_string_prepared_producer_and_output_policies() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true);
    for (source, physical, producer, rejected) in [
        ("<?php echo 1 . 'x';", SourceMode::Internal, SourceMode::Php, true),
        ("<?php echo 1 . 'x';", SourceMode::Php, SourceMode::Internal, false),
        ("<?php echo [1];", SourceMode::Php, SourceMode::Internal, true),
        ("<?php echo [1];", SourceMode::Internal, SourceMode::Php, false),
    ] {
        let mut program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        program[0].source_mode = physical;
        let crate::parser::ast::StmtKind::Echo(value) = &mut program[0].kind else { panic!("echo"); };
        value.producer_binding = Some(crate::parser::ast::ProducerBinding::Identity);
        value.source_profile.mode = producer;
        assert_eq!(crate::semantics::validate_source(&program, profile).is_err(), rejected,
            "{source}, physical={physical:?}, producer={producer:?}");
    }
}

/// PHP source serialization retains formatting syntax for ordinary and rewritten operands.
#[test]
fn strict_string_formatting_source_preserves_interpolation_context() {
    struct FormattingCount(usize);
    impl crate::ast_visit::AstVisitor for FormattingCount {
        /// Counts semantic formatting conversions after source reconstruction.
        fn expression(&mut self, expr: &crate::parser::ast::Expr) {
            if matches!(expr.kind, crate::parser::ast::ExprKind::Cast {
                target: crate::parser::ast::CastType::StringFormat, .. }) { self.0 += 1; }
        }
    }
    for expression in ["$value", "($value + 1)", "[1]", "2.5"] {
        let formatted = crate::semantics::strings::formatting_source(expression);
        let source = format!("<?php echo {formatted};");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let mut count = FormattingCount(0);
        crate::ast_visit::walk_program(&program, &mut count);
        assert_eq!(count.0, 1, "{source}");
        elephc_magician::parser::parse_fragment(format!("echo {formatted};").as_bytes())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Dynamic guards use original kinds on every target and remain independent of Coercions.
#[test]
fn strict_string_all_target_guards_and_internal_controls() {
    let source = r#"<?php
function joinText(mixed $left, mixed $right): string { return $left . $right; }
function outputText(mixed $value): void { echo $value; print $value; echo "[$value]"; }
function joinMissingText(int $index): string { $parts = ['a']; return $parts[$index] . 'x'; }
"#;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true);
    for mode in [SourceMode::Php, SourceMode::Internal] {
        let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
            let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            for optimized in [false, true] {
                let mut module = module.clone();
                if optimized { crate::ir_passes::optimize_module(&mut module); }
                let guards = module.data.strings.iter().any(|message| message.contains("--strict-strings:"));
                assert_eq!(guards, mode != SourceMode::Internal, "{name}, {mode:?}, opt={optimized}");
                let missing = module.functions.iter().find(|function| function.name == "joinMissingText").unwrap();
                assert_eq!(missing.instructions.iter().any(|inst| inst.op == crate::ir::Op::TypePredicate
                    && inst.immediate == Some(crate::ir::Immediate::TypePredicate(crate::ir::PhpTypePredicate::String))),
                    mode != SourceMode::Internal, "{name}, {mode:?}, opt={optimized}: logical string guard");
                crate::ir::validate_module(&module).unwrap();
                crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            }
        }
    }
}

/// Literal, compound and declared nonstring operands cannot silently enter concatenation or output.
#[test]
fn strict_string_source_rejects_implicit_concat_and_array_formatting() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true);
    let mut failures = Vec::new();
    for source in [
        "<?php if (false) { echo 1 . 'x'; }",
        "<?php echo 'x' . true;", "<?php $value = 'x'; $value .= 1;",
        "<?php echo [1];", "<?php print [1];",
        "<?php function label(int $value): string { return 'row:' . $value; }",
        "<?php class TextValue { function __toString(): string { return 'x'; } } echo 'v:' . new TextValue();",
    ] {
        for mode in [SourceMode::Php, SourceMode::Lfc] {
            let program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
            let result = crate::types::checker::check_staged_semantics(&program,
                crate::codegen_support::platform::Target::detect_host(), profile);
            match result {
                Err(error) if error.message.contains("strict-strings") => {}
                Err(error) => failures.push(format!("{mode:?}: {source}: {}", error.message)),
                Ok(_) => failures.push(format!("{mode:?}: unexpectedly accepted {source}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Explicit casts and output/interpolation remain intentional formatting contexts.
#[test]
fn strict_string_source_allows_explicit_formatting_and_interpolation() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Strings, true);
    for source in [
        "<?php echo 'a' . 'b';",
        "<?php $value = 'a'; $value .= 'b'; echo $value;",
        "<?php echo 1, 2.5, true, null; print false;",
        "<?php $row = 2; echo \"Row: $row\";",
        "<?php echo 'Row: ' . (string) 2;",
        "<?php echo (string) [1];",
        "<?php class TextValue { function __toString(): string { return 'x'; } } echo new TextValue();",
    ] {
        let program = crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap();
        crate::types::checker::check_staged_semantics(&program,
            crate::codegen_support::platform::Target::detect_host(), profile)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}
