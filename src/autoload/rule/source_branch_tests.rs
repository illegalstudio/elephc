//! Purpose:
//! Verifies source preservation across compile-time autoload rule discovery.
//!
//! Called from:
//! - Focused library tests for strict local source diagnostics.
//!
//! Key details:
//! - The selected collection view and the checker source have distinct responsibilities.

use super::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Parses a physical PHP program with ordinary source identities.
fn parse(source: &str) -> Program {
    crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap()
}

/// Retaining every ordinary arm prevents autoload discovery from hiding an invalid source store.
#[test]
fn strict_autoload_source_keeps_constant_branch_diagnostics() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for tail in [
        "if(false) {$value='bad';}",
        "if(true) {} else {$value='bad';}",
        "if(false) {} elseif(false) {$value='bad';} else {}",
        "if(true) {if(false) {$value='bad';}}",
        "if(true) {spl_autoload_register(function($name) {require $name;});} else {$value='bad';}",
        "if(false) {spl_autoload_register(function($name) {require $name;}); $value='bad';}",
    ] {
        let (program, _, warnings) = collect_register_calls_with_semantics(
            parse(&format!("<?php $value=1; {tail}")), profile);
        assert!(warnings.is_empty(), "{tail}");
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let error = crate::types::checker::check_staged_semantics(&program,
                crate::codegen::platform::Target::parse(name).unwrap(), profile).unwrap_err();
            assert!(error.message.contains("--strict-locals"), "{tail}: {error:?}");
        }
    }
}

/// Inactive callback bodies are consumed without contributing a rule or dropping ordinary statements.
#[test]
fn strict_autoload_source_collects_only_selected_loader_rules() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    for source in [
        "<?php if(true) {spl_autoload_register(function($name) {require 'live.php';}); echo 1;}",
        "<?php if(false) {spl_autoload_register(function($name) {require 'dead.php';}); echo 0;} else {spl_autoload_register(function($name) {require 'live.php';}); echo 1;}",
        "<?php if(false) {echo 0;} elseif(true) {if(true) {spl_autoload_register(function($name) {require 'live.php';}); echo 1;}} else {spl_autoload_register(function($name) {require 'dead.php';});}",
        "<?php if(false) {if($argc > 1) {spl_autoload_register(function($name) {require 'dead.php';}); echo 0;}} else {spl_autoload_register(function($name) {require 'live.php';}); echo 1;}",
        "<?php if(false) {$loader=function($name) {require 'dead.php';}; spl_autoload_register($loader);} else {$loader=function($name) {require 'live.php';}; spl_autoload_register($loader); echo 1;}",
    ] {
        let original = parse(source);
        let (program, rules, warnings) = collect_register_calls_with_semantics(original.clone(), profile);
        assert!(warnings.is_empty(), "{source}");
        assert_eq!(rules.len(), 1, "{source}");
        assert!(format!("{:?}", rules[0]).contains("live.php"), "{source}");
        assert_eq!(program[0].span, original[0].span);
        assert!(matches!(program[0].kind, StmtKind::If { .. }));
        crate::types::checker::check_staged_semantics(&program,
            crate::codegen::platform::Target::detect_host(), profile).unwrap();
        let (legacy, legacy_rules, _) = collect_register_calls(original);
        assert_eq!(legacy_rules, rules);
        assert!(!matches!(legacy[0].kind, StmtKind::If { .. }));
    }
}

/// Inactive uses cannot remove a live callback source, and namespaced shadowing is unchanged.
#[test]
fn strict_autoload_source_preserves_live_sources_and_shadowing() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
    let source = "<?php $loader=function($name) {echo $name;}; if(false) {spl_autoload_register($loader);} $loader('live');";
    let (program, rules, warnings) = collect_register_calls_with_semantics(parse(source), profile);
    assert!(rules.is_empty() && warnings.is_empty());
    assert!(matches!(program[0].kind, StmtKind::Assign { .. }));
    let StmtKind::If { then_body, .. } = &program[1].kind else { panic!("source condition lost") };
    assert!(then_body.is_empty());
    for source in [
        "<?php namespace Demo; function spl_autoload_register($callback): int {return 1;} if(true) {spl_autoload_register(function($name) {echo $name;});}",
        "<?php if($argc > 1) {spl_autoload_register(function($name) {echo $name;});}",
        "<?php if(false) {} elseif($argc > 1) {spl_autoload_register(function($name) {echo $name;});}",
    ] {
        let original = parse(source);
        let (program, rules, warnings) = collect_register_calls_with_semantics(original.clone(), profile);
        assert!(rules.is_empty() && warnings.is_empty(), "{source}");
        assert_eq!(program, original, "{source}");
    }
}
