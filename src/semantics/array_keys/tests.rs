//! Purpose:
//! Verifies original key-domain checks before optimization and storage normalization.
//!
//! Called from:
//! - Focused semantic profile and selected-checker tests.
//!
//! Key details:
//! - Array keys have an independent owner; string offsets and object dimensions are controls.

use super::*;
use crate::{
    codegen_support::platform::Target, parser::ast::Program, php_version::PhpVersion,
    source::SourceMode,
};

/// Restores the thread-local compile profile after a version-sensitive source-validation test.
struct CompileProfileGuard {
    version: PhpVersion,
    web: bool,
}

impl Drop for CompileProfileGuard {
    /// Restores the profile that was active before the test selected an older PHP version.
    fn drop(&mut self) {
        crate::codegen_support::set_compile_profile(self.version, self.web);
    }
}

/// Selects one PHP version for source validation and returns its restoration guard.
fn select_php_version(version: PhpVersion) -> CompileProfileGuard {
    let guard = CompileProfileGuard {
        version: crate::codegen_support::compile_php_version(),
        web: crate::codegen_support::compile_is_web_sapi(),
    };
    crate::codegen_support::set_compile_profile(version, false);
    guard
}

/// Selects only the key guarantee, leaving all other strict families disabled.
fn profile() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true)
}

/// Parses source while retaining its physical mode for pre-fold validation.
fn parse(source: &str, mode: SourceMode) -> Program {
    crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap()
}

/// Key membership keeps unions and unknowns distinct from coercion and indexing policy.
#[test]
fn strict_array_key_domain_plans_are_independent() {
    let profile = profile();
    for ty in [PhpType::Int, PhpType::Str, PhpType::Never,
        PhpType::Union(vec![PhpType::Int, PhpType::Str])] {
        assert_eq!(plan(profile, &ty), ConversionPlan::Identity);
    }
    for ty in [PhpType::Bool, PhpType::False, PhpType::Float, PhpType::Void,
        PhpType::Object("stdClass".into()), PhpType::Array(Box::new(PhpType::Int)),
        PhpType::Union(vec![PhpType::Bool, PhpType::Float])] {
        assert_eq!(plan(profile, &ty), ConversionPlan::Reject(StrictFeature::ArrayKeys));
        assert!(validate(profile, &ty, Span::dummy()).unwrap_err().message.contains("strict-array-keys"));
    }
    for ty in [PhpType::Mixed, PhpType::Union(vec![PhpType::Int, PhpType::Float]),
        PhpType::Union(vec![PhpType::Str, PhpType::Void])] {
        let ConversionPlan::Guard { feature, allowed } = plan(profile, &ty) else { panic!("dynamic guard"); };
        assert_eq!(feature, StrictFeature::ArrayKeys);
        assert!(allowed.allows(ValueKind::Integer) && allowed.allows(ValueKind::String));
        assert!(!allowed.allows(ValueKind::Boolean) && !allowed.allows(ValueKind::Float)
            && !allowed.allows(ValueKind::Null) && !allowed.allows(ValueKind::NonScalar));
    }
    for feature in [StrictFeature::Coercions, StrictFeature::Indexing, StrictFeature::ArrayTypes,
        StrictFeature::Nullability, StrictFeature::Locals] {
        let other = SemanticProfile::PERMISSIVE.with(feature, true);
        assert_eq!(plan(other, &PhpType::Float), ConversionPlan::ExistingBehavior);
    }
    let key = PhpType::Float;
    let array = PhpType::php_array();
    assert!(validate_receiver(profile, &array, &key, Span::dummy()).is_err());
    for receiver in [PhpType::Str, PhpType::Object("ArrayAccess".into()), PhpType::Mixed,
        PhpType::Union(vec![array.clone(), PhpType::Str])] {
        assert!(validate_receiver(profile, &receiver, &key, Span::dummy()).is_ok());
    }
    assert!(validate_receiver(profile, &PhpType::Void, &key, Span::dummy()).is_ok());
    assert!(validate_write_receiver(profile, &PhpType::Void, &key, Span::dummy()).is_err());
}

/// Literal keys and literal array receivers are audited before dead-code and access folding.
#[test]
fn strict_array_key_source_validation_keeps_original_domains() {
    for mode in [SourceMode::Php, SourceMode::Internal] {
        for source in [
            "<?php if (false) { $a = [true => 1]; }",
            "<?php $a = [1.0 => 1];",
            "<?php $a = [null => 1];",
            "<?php $a = [[] => 1];",
            "<?php $a = [new stdClass() => 1];",
            "<?php $a = [...[], (float)1 => 1];",
            "<?php echo [10, 20][1.0];",
            "<?php echo [10, 20][!false];",
            "<?php echo [(1 === 1) => 1][1];",
            "<?php echo [(1 / 2) => 1][0];",
            "<?php function defaults($a = [false => 1]) {}",
            "<?php class KeyDefault { public $items = [null => 1]; }",
            "<?php eval('$a = [true => 1];');",
            "<?php declare(strict_types=0); $a = [false => 1];",
        ] {
            let program = parse(source, mode);
            let error = crate::semantics::validate_source(&program, profile()).unwrap_err();
            assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
            crate::semantics::validate_source(&program, SemanticProfile::PERMISSIVE).unwrap();
        }
        for source in [
            "<?php $a = ['1' => 1, 1 => 2, '01' => 3];",
            "<?php $a = [(int)true => 1, (string)1.0 => 2];",
            "<?php echo 'abc'[1.0];",
            "<?php echo (new KeyAccess())[false];",
            "<?php if (1) { echo 1 + '2'; }",
            "<?php eval('echo 1;');",
            "<?php eval($source);",
        ] {
            crate::semantics::validate_source(&parse(source, mode), profile()).unwrap();
        }
    }
}

/// Audits explicit eval assignments while deferring the caller's process argument bindings.
#[test]
fn strict_array_key_source_validation_literal_eval_inherits_process_arguments() {
    let inherited = r#"<?php function writeArgv(string $argv) {
        eval('$argv[false] = "x";');
    }"#;
    crate::semantics::validate_source(&parse(inherited, SourceMode::Php), profile()).unwrap();

    let assigned = r#"<?php eval('$argv = []; $argv[false] = "x";');"#;
    let error =
        crate::semantics::validate_source(&parse(assigned, SourceMode::Php), profile()).unwrap_err();
    assert!(error.message.contains("strict-array-keys"));
}

/// Audits variadic array keys while deferring the type of a slot to its call.
#[test]
fn strict_array_key_source_validation_variadic_bindings() {
    for declaration in [
        "function rewrite(string ...$items) { WRITE }",
        "class VariadicWriter { public function rewrite(string ...$items) { WRITE } }",
        "$rewrite = function (string ...$items) { WRITE };",
    ] {
        for write in ["$items[0][false] = 'x';", "$items['label'][false] = 'x';"] {
            let source = format!("<?php {}", declaration.replace("WRITE", write));
            crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
                .unwrap_or_else(|error| panic!("{source}: {error:?}"));
        }
        let source = format!(
            "<?php {}",
            declaration.replace("WRITE", "if (false) { $items[false] = 'x'; }"),
        );
        let error =
            crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile()).unwrap_err();
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
}

/// Source-order local updates select the current array slot in both statement and expression walks.
#[test]
fn strict_array_key_source_validation_tracks_local_inc_dec() {
    for (initial, update) in [(0, "$key++"), (0, "++$key"), (2, "$key--"), (2, "--$key")] {
        for statement in [
            format!("{update}; $values[$key][false] = 'x';"),
            format!("$results = [{update}, $values[$key][false] = 'x'];"),
        ] {
            let accepted = format!("<?php $key = {initial}; $values = [[], 'ab', []]; {statement}");
            crate::semantics::validate_source(&parse(&accepted, SourceMode::Php), profile())
                .unwrap_or_else(|error| panic!("{accepted}: {error:?}"));

            let rejected = format!("<?php $key = {initial}; $values = ['ab', [], 'cd']; {statement}");
            let error = crate::semantics::validate_source(&parse(&rejected, SourceMode::Php), profile())
                .expect_err("the updated key selects the array, even though its old slot was a string");
            assert!(error.message.contains("strict-array-keys"), "{rejected}: {error:?}");
        }
    }

    for (update, values) in [("$key++", "[[], 'ab']"), ("++$key", "['ab', []]")] {
        let rejected = format!(
            "<?php $key = 0; $index = {update}; $values = {values}; $values[$index][false] = 'x';"
        );
        let error = crate::semantics::validate_source(&parse(&rejected, SourceMode::Php), profile())
            .expect_err("the expression result selects the array before or after the update");
        assert!(error.message.contains("strict-array-keys"), "{rejected}: {error:?}");
    }
}

/// A captured index increments before the assignment evaluates its right-hand side.
#[test]
fn strict_array_key_source_validation_keeps_inc_dec_capture_order() {
    let setup = "<?php $key = 0; $cells = [0, 0]; $result = ($cells[$key++] = $key);";
    for index in ["$key", "$result"] {
        let accepted = format!("{setup} $values = [[], 'ab']; $values[{index}][false] = 'x';");
        crate::semantics::validate_source(&parse(&accepted, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{accepted}: {error:?}"));

        let rejected = format!("{setup} $values = ['ab', []]; $values[{index}][false] = 'x';");
        let error = crate::semantics::validate_source(&parse(&rejected, SourceMode::Php), profile())
            .expect_err("the capture and the assignment result both select the updated slot");
        assert!(error.message.contains("strict-array-keys"), "{rejected}: {error:?}");
    }
}

/// Negative maximum keys advance implicit appends by one instead of resetting to zero.
#[test]
fn strict_array_key_source_validation_tracks_negative_append_keys() {
    let source = "<?php $values = [-2 => []]; $values[] = 'ab'; $values[-1][false] = 'x';";
    crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
}

/// Spread-free literals use the selected PHP version's negative implicit-key rule before folding.
#[test]
fn strict_array_key_source_validation_uses_versioned_literal_append_keys() {
    let _profile = select_php_version(PhpVersion::Php82);
    for key in [
        "-2",
        "true ? -2 : 9",
        "true ? -2 : intval(9)",
        "true ? (match (1) { 1 => -2, default => $unused }) : 0",
        "false ?: -2",
        "null ?? -2",
    ] {
        let accepted = format!(
            "<?php $values = [({key}) => [], 'ab']; $values[0][false] = 'x';"
        );
        crate::semantics::validate_source(&parse(&accepted, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{accepted}: {error:?}"));

        let rejected = format!(
            "<?php $values = [({key}) => [], 'ab']; $values[-1][false] = 'x';"
        );
        let error = crate::semantics::validate_source(&parse(&rejected, SourceMode::Php), profile())
            .expect_err("PHP 8.2 leaves the missing -1 slot as an array write parent");
        assert!(error.message.contains("strict-array-keys"), "{rejected}: {error:?}");
    }

    let runtime =
        "<?php $values = [(($key = -2)) => [], 'ab']; $values[-1][false] = 'x';";
    crate::semantics::validate_source(&parse(runtime, SourceMode::Php), profile())
        .unwrap_or_else(|error| panic!("{runtime}: {error:?}"));

    for key in [
        "true ? -2 : $unused",
        "true ? -2 : intval($argc)",
        "true ? -2 : foo()",
        "match (1) { 1 => (true ? -2 : $unused), default => 0 }",
    ] {
        let accepted = format!(
            "<?php $values = [({key}) => [], 'ab']; $values[-1][false] = 'x';"
        );
        crate::semantics::validate_source(&parse(&accepted, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{accepted}: {error:?}"));

        let rejected = format!(
            "<?php $values = [({key}) => [], 'ab']; $values[0][false] = 'x';"
        );
        let error = crate::semantics::validate_source(&parse(&rejected, SourceMode::Php), profile())
            .expect_err("unfolded lazy keys must retain the emitted runtime append cursor");
        assert!(error.message.contains("strict-array-keys"), "{rejected}: {error:?}");
    }

    let shadowed = "<?php namespace App; function intval($value) { return 9; } $values = [(true ? -2 : intval(9)) => [], 'ab']; $values[-1][false] = 'x';";
    crate::semantics::validate_source(&parse(shadowed, SourceMode::Php), profile())
        .unwrap_or_else(|error| panic!("{shadowed}: {error:?}"));

    let global = "<?php namespace App; function intval($value) { return 9; } $values = [(true ? -2 : \\intval(9)) => [], 'ab']; $values[0][false] = 'x';";
    crate::semantics::validate_source(&parse(global, SourceMode::Php), profile())
        .unwrap_or_else(|error| panic!("{global}: {error:?}"));

    let strict = "<?php $values = [+\"-2\" => [], 'ab']; $values[0][false] = 'x';";
    let error = crate::semantics::validate_source(
        &parse(strict, SourceMode::Php),
        SemanticProfile::all(),
    )
    .expect_err("strict arithmetic must reject the unary string conversion after key planning");
    assert!(error.message.contains("strict-arithmetic"), "{strict}: {error:?}");
}

/// A selected match arm fixes the key before any later unreachable pattern is inspected.
#[test]
fn strict_array_key_source_validation_stops_after_selected_match_arm() {
    for (version, accepted_index, rejected_index) in [
        (PhpVersion::Php82, 0, -1),
        (PhpVersion::Php85, -1, 0),
    ] {
        let _profile = select_php_version(version);
        let accepted = format!(
            "<?php $values = [(match (1) {{ 1 => -2, $unused => 9, default => 0 }}) => [], 'ab']; $values[{accepted_index}][false] = 'x';"
        );
        crate::semantics::validate_source(&parse(&accepted, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{accepted}: {error:?}"));

        let rejected = format!(
            "<?php $values = [(match (1) {{ 1 => -2, $unused => 9, default => 0 }}) => [], 'ab']; $values[{rejected_index}][false] = 'x';"
        );
        let error = crate::semantics::validate_source(
            &parse(&rejected, SourceMode::Php),
            profile(),
        )
        .expect_err("the missing slot must retain strict nested-key validation");
        assert!(
            error.message.contains("strict-array-keys"),
            "{rejected}: {error:?}"
        );
    }
}

/// A decided short ternary folds its selected key before PHP 8.2 advances the cursor.
#[test]
fn strict_array_key_source_validation_folds_selected_short_ternary_key() {
    let _profile = select_php_version(PhpVersion::Php82);
    for key in [
        "(-2) ?: $unused",
        "match (1) { 1 => ((-2) ?: $unused), default => 0 }",
    ] {
        let accepted = format!(
            "<?php $values = [({key}) => [], 'ab']; $values[0][false] = 'x';"
        );
        crate::semantics::validate_source(&parse(&accepted, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{accepted}: {error:?}"));

        let rejected = format!(
            "<?php $values = [({key}) => [], 'ab']; $values[-1][false] = 'x';"
        );
        let error = crate::semantics::validate_source(
            &parse(&rejected, SourceMode::Php),
            profile(),
        )
        .expect_err("PHP 8.2 leaves the missing -1 slot as an array write parent");
        assert!(
            error.message.contains("strict-array-keys"),
            "{rejected}: {error:?}"
        );
    }

    let runtime = "<?php $values = [((false ?: [-2])[0]) => [], 'ab']; $values[-1][false] = 'x';";
    crate::semantics::validate_source(&parse(runtime, SourceMode::Php), profile())
        .expect("a non-scalar short-ternary fallback must not create a literal-key proof");
}

/// Pre-resolution source facts leave magic constants and imported class names unresolved.
#[test]
fn strict_array_key_source_validation_defers_unresolved_literal_keys() {
    for source in [
        "<?php eval('$values = [__LINE__ => 1, 2];');",
        "<?php use Vendor\\Thing as Alias; $values = [Alias::class => 'ab', 0]; $values['Vendor\\Thing'][false] = 'x';",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Literal eval lowers magic constants before proving nested strict array-key writes.
#[test]
fn strict_array_key_source_validation_lowers_eval_magic_keys() {
    let source = r#"<?php eval('$values = [__LINE__ => []]; $values[__LINE__][false] = "x";');"#;
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("equal eval line keys must expose the invalid nested boolean key");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");
}

/// Safe scalar folds retain pre-prune receiver proofs in a PHP 8.2 target guard.
#[test]
fn strict_array_key_source_validation_folds_safe_target_guard_keys() {
    let _profile = select_php_version(PhpVersion::Php82);
    for (key, expected) in [
        (r#"\Vendor\Thing::class"#, r#"'Vendor\Thing'"#),
        ("match (1) { 1 => -2 }", "-2"),
        ("match (1) { 1 => -2, default => $unused }", "-2"),
        ("[-2][0]", "-2"),
        ("[-2 => -2, 9][-2]", "-2"),
    ] {
        let source = format!(
            "<?php if (PHP_OS_FAMILY === 'Windows') {{ $values = [{expected} => 'ab', ({key}) => [], 'tail']; $values[{expected}][false] = 'x'; }}"
        );
        let error = match crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile()) {
            Ok(()) => panic!("safe source folds must retain the invalid guarded receiver: {source}"),
            Err(error) => error,
        };
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
}

/// Target pruning cannot hide invalid keys written through proven local or property arrays.
#[test]
fn strict_array_key_source_validation_audits_target_guarded_writes() {
    for source in [
        "<?php $values = ['ok' => 1]; if (PHP_OS_FAMILY === 'Windows') { $values[true] = 2; }",
        "<?php $values = []; $key = true; if (PHP_OS_FAMILY === 'Windows') { $values[$key] = 2; }",
        "<?php class KeyWrites { public array $items = []; } $value = new KeyWrites(); if (PHP_OS_FAMILY === 'Windows') { $value->items[1.5] = 2; }",
        "<?php class StaticKeyWrites { public static array $items = []; } if (PHP_OS_FAMILY === 'Windows') { StaticKeyWrites::$items[null] = 2; }",
        "<?php function guarded_keys(): void { $values = []; if (PHP_OS_FAMILY === 'Windows') { $values[false] = 2; } }",
        "<?php function guarded_typed_key(array $values, bool $key): void { if (PHP_OS_FAMILY === 'Windows') { $values[$key] = 2; } }",
        "<?php accept_key_writer(function(array $values): void { if (PHP_OS_FAMILY === 'Windows') { $values[false] = 2; } });",
        "<?php function returned_key_write(array $values): int { if (PHP_OS_FAMILY === 'Windows') { return $values[false] = 2; } return 1; }",
        "<?php function chained_key_write(array $values): void { if (PHP_OS_FAMILY === 'Windows') { $ignored = $values[false] = 2; } }",
        "<?php accept_key_writer(function(array $values): int { if (PHP_OS === 'Linux') { return $values[false] = 2; } return 1; });",
        "<?php function nested_statement_key_write(array $values): void { if (PHP_OS_FAMILY === 'Windows') { $values[false][0] = 2; } }",
        "<?php function nested_expression_key_write(array $values): int { if (PHP_OS_FAMILY === 'Windows') { return $values[false][1] = 3; } return 1; }",
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { return $values[0][false] = 2; }",
        "<?php $values = []; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][0][false] = 2; }",
        "<?php $values = null; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $empty = []; $slot = $empty[0]; if (PHP_OS_FAMILY === 'Windows') { $slot[false] = 2; }",
        "<?php $values = ['0' => [1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = ['0' => [1]]; if (PHP_OS_FAMILY === 'Windows') { $values['0'][false] = 2; }",
        "<?php $values = ['a' => 1]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { $values[1][false] = 2; }",
        "<?php $values = [[1], ['a' => 2]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[], 1]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php function guarded_dynamic_nested_key(int $key): void { $values = [[1], [2]]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php $values = [...[[1]]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = []; $values[0] = []; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[]]; $values[0][0] = []; if (PHP_OS_FAMILY === 'Windows') { $values[0][0][false] = 2; }",
        "<?php $values = [-1 => [1], [2]]; if (PHP_OS_FAMILY === 'Windows') { $values[-1][false] = 2; }",
        "<?php $values = [-1 => [1]]; $values[] = []; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [-1 => [1], ...[[2]]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [9223372036854775807 => [1]]; $values[] = []; if (PHP_OS_FAMILY === 'Windows') { $values[9223372036854775807][false] = 2; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = ($values = [[1]]); }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { $values[0][($values = [[1]]) ? false : 0] = 2; }",
        "<?php $values = [1]; do { $values = [[1]]; } while ($values[0][false] = false);",
        "<?php $inner = [1]; $values = [$inner]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $inner = [1]; $values = ['a' => $inner]; if (PHP_OS_FAMILY === 'Windows') { $values['a'][false] = 2; }",
        "<?php $inner = [[1]]; $values = [...$inner]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $inner = [[1]]; $values = [...[$inner, ($inner = 'ab')]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; $writer = function() use ($values): void { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } };",
        "<?php function guarded_dynamic_deep_key(int $key): void { $values = [[[1]], [[2]]]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][0][false] = 2; } }",
        "<?php $key = 0; $ignored = true && ($key = true); $values = []; $values[$key] = 1;",
        "<?php $x = 1; $key = 0; $values = []; if (PHP_OS_FAMILY === 'Windows') { $ignored = [$x] && ($key = true); $values[$key] = 1; }",
        "<?php $values = [[1]]; $ignored = match (1) { 1 => 1, default => ($values = [1]) }; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows' && ($values = [[1]])) { $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (true || ($values = [1])) { $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (true || ($values = [1])) {} else { $values[0][false] = 2; }",
        "<?php $values = [1]; if (false || ($values = [[1]])) { $values[0][false] = 2; }",
        "<?php $values = [1]; while (PHP_OS_FAMILY === 'Windows' && ($values = [[1]])) { $values[0][false] = 2; break; }",
        "<?php $a = []; while ($argc > 1) { break; } if (PHP_OS_FAMILY === 'Windows') { $a[false] = 1; }",
        "<?php $values = [[1]]; while ($argc > 1) { $ignore = function () { $values = 1; }; break; } if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; while ($argc > 1) { $values = [[1]]; break; } if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; while ($argc > 1) { $values = [[1]]; continue; } if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; while ($argc > 1) { $ignore = function () { $values = 1; }; continue; } if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; while ($argc > 1) { try { $values = [[1]]; break; } catch (Exception $error) {} } if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [1]; while (true) { try { $values = [[1]]; break; } finally {} } $values[0][false] = 2;",
        "<?php $values = ['ab']; do { try { $values = [['a']]; continue; } finally {} } while (false); $values[0][false] = 'x';",
        "<?php $values = [1]; switch ($argc) { case 1: $values = [[1]]; case 2: $values[0][false] = 2; break; }",
        "<?php $values = [1]; foreach ([10] as $item) { $values = [[1]]; continue; } $values[0][false] = 2;",
        "<?php $values = [1]; while (true) { $values = [[1]]; } $values[0][false] = 2;",
        "<?php $values = [[1]]; while ($argc > 1) { while ($argc > 2) { $values = [[1]]; break 2; } break; } if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [1]; while (true) { $values = [[1]]; while ($argc > 2) { break 2; } break; } $values[0][false] = 2;",
        "<?php $values = [[1]]; while ($argc > 1) { if (false) { $values[0][false] = 2; } break; }",
        "<?php $values = [[1]]; while ($argc > 1) { break; $values[0][false] = 2; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { if (true) { $values = [[1]]; } $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { if (false && ($values = [1])) {} $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { $values[false ?: 0][false] = 2; }",
        "<?php $values = []; if (PHP_OS_FAMILY === 'Windows') { $values[null ?? false] = 1; }",
        "<?php $values = [(true ? 0 : 1) => [1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; $ignored = false && ($values = [1]) && ($values = [1]); if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = []; if (PHP_OS_FAMILY === 'Windows') { $values[(null ?? false) ? 1 : false] = 1; }",
        "<?php $values = [((null ?? null) ?? 0) => [1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [['a']]; while ($argc > 1) { $values = [['a']]; } $values[0][false] = 'x';",
        "<?php $values = [1]; for (; PHP_OS_FAMILY === 'Windows' && ($values = [[1]]);) { $values[0][false] = 2; break; }",
        "<?php $values = [1]; do { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } } while ($values = [[1]]);",
        "<?php $inner = 'ab'; $values = [$inner, ($inner = [])]; $values[1][true] = 'x';",
        "<?php function mixed_dynamic_parent(int $key): void { $values = [null, false]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php function array_or_null_dynamic_parent(int $key): void { $values = [[1], null]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php function deep_dynamic_parent(int $key): void { $values = [[[null]], [[false]]]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][0][0][false] = 2; } }",
        "<?php function mixed_typed_parents(int $key, array<int> $left, array<string> $right): void { $values = [$left, $right]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php function ordered_write(): int { $values = [1]; if (PHP_OS_FAMILY === 'Windows') { return ($values = [[1]]) ? ($values[0][false] = 2) : 0; } return 0; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { $writer = ($values = [[1]]) ? function () use ($values): void { $values[0][false] = 2; } : null; }",
        "<?php count($values = [[]]); if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; foreach ([] as $item) { $values = [1]; } $values[0][false] = 2;",
        "<?php function return_branch_keeps_fallthrough_shape(int $argc): void { $values = [[1]]; if ($argc > 1) { $values = [1]; return; } $values[0][false] = 2; }",
        "<?php function return_try_keeps_fallthrough_shape(int $argc): void { $values = [[1]]; try { if ($argc > 1) { $values = [1]; return; } } finally {} $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (($flag = 1) ?? ($values = [1])) { $values[0][false] = 2; }",
        "<?php $values = [1]; if (($flag = null) ?? ($values = [[1]])) { $values[0][false] = 2; }",
        "<?php $values = [[1]]; try { throw new Exception(); $values = [1]; } catch (Exception $error) { $values[0][false] = 2; }",
        "<?php $values = [1]; foreach ($values as &$item) { $item = [1]; } $values[0][false] = 2;",
        "<?php $values = [1]; foreach ($values as &$item) {} $item = [1]; $values[0][false] = 2;",
        "<?php $values = [1]; try { $values = [[1]]; return; } finally { $values[0][false] = 2; }",
        "<?php $values = [1]; try { $values = [[1]]; throw new Exception(); } finally { $values[0][false] = 2; }",
        "<?php $values = []; $key = 0; switch (1) { case 1: $key = 1.5; break; } $values[$key] = 1;",
        "<?php if ($argc) { $values = [[1]]; } else { $values = null; } $values[false] = 1;",
        "<?php if ($argc) { $values = [[1]]; } else { $values = null; } $values[0][false] = 2;",
        "<?php $values = [[1]]; $empty = []; foreach ($empty as $item) { $values = [1]; } $values[0][false] = 2;",
        "<?php $values = [1]; $items = [10]; foreach ($items as $item) { $values = [[1]]; } $values[0][false] = 2;",
        "<?php $values = [[1]]; if ($argc) { foreach ($values as &$item) {} } $item = [1]; $values[0][false] = 2;",
        "<?php $values = [1]; foreach ($values as &$item) {} foreach ([[1]] as $item) {} $values[0][false] = 2;",
        "<?php $values = [1]; foreach ($values as &$item) {} $values = [[1]]; $item = 'ab'; $values[0][false] = 2;",
    ] {
        let error = match crate::semantics::validate_source(
            &parse(source, SourceMode::Php),
            profile(),
        ) {
            Err(error) => error,
            Ok(()) => panic!("{source}: expected strict-array-keys rejection"),
        };
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
}

/// Stabilized expression keys retain their source facts without treating hidden temps as null.
#[test]
fn strict_array_key_source_validation_tracks_expression_preludes() {
    for source in [
        "<?php function key_is_int(): int { return 1; } function write_call_key(array $values): int { return $values[key_is_int()] = 2; }",
        "<?php function write_ternary_key(array $values, bool $flag): bool { return $values[$flag ? 1 : 2] = ($flag = true); }",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }

    let source = "<?php function write_stabilized_bool_key(array $values): int { return $values[!false] = 2; }";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .unwrap_err();
    assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
}

/// Source auditing leaves string offsets and ArrayAccess dimensions to their own contracts.
#[test]
fn strict_array_key_source_validation_keeps_non_array_target_writes() {
    for source in [
        "<?php $text = 'ab'; if (PHP_OS_FAMILY === 'Windows') { $text[true] = 'x'; }",
        "<?php class StringKeyWrites { public string $items = 'ab'; public static string $shared = 'ab'; } $value = new StringKeyWrites(); if (PHP_OS_FAMILY === 'Windows') { $value->items[1.5] = 'x'; StringKeyWrites::$shared[false] = 'y'; }",
        "<?php class KeyAccessWrite implements ArrayAccess { public function offsetExists(mixed $key): bool { return true; } public function offsetGet(mixed $key): mixed { return 1; } public function offsetSet(mixed $key, mixed $value): void {} public function offsetUnset(mixed $key): void {} } $value = new KeyAccessWrite(); if (PHP_OS_FAMILY === 'Windows') { $value[true] = 2; }",
        "<?php class NestedKeyAccessWrite implements ArrayAccess { public function offsetExists(mixed $key): bool { return true; } public function offsetGet(mixed $key): mixed { return 1; } public function offsetSet(mixed $key, mixed $value): void {} public function offsetUnset(mixed $key): void {} } $values = [new NestedKeyAccessWrite()]; if (PHP_OS_FAMILY === 'Windows') { $values[0][true] = 2; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = []; $values[0] = 1; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[]]; $values[0][0] = 1; if (PHP_OS_FAMILY === 'Windows') { $values[0][0][false] = 2; }",
        "<?php class SpreadKeyAccessWrite implements ArrayAccess { public function offsetExists(mixed $key): bool { return true; } public function offsetGet(mixed $key): mixed { return 1; } public function offsetSet(mixed $key, mixed $value): void {} public function offsetUnset(mixed $key): void {} } $values = [...[new SpreadKeyAccessWrite()]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][true] = 2; }",
        "<?php $values = []; foreach ([10] as $key => $value) { $values[$key] = $value; }",
        "<?php $values = []; foreach ([10] as $value) { $values[$value] = 1; }",
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = ($values[0] = 1); }",
        "<?php $values = [[1]]; do { $values[0] = 1; } while ($values[0][false] = false);",
        "<?php $values = [['a']]; do { $values = ['ab']; continue; } while (false); $values[0][false] = 'x';",
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][($values = [1]) ? false : 0] = 2; }",
        "<?php $values = [[1]]; $writer = function() use (&$values): void { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } };",
        "<?php function guarded_dynamic_mixed_depth(int $key): void { $values = [[[1]], 2]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][0][false] = 2; } }",
        "<?php $key = 0; $ignored = false && ($key = true); $values = []; $values[$key] = 1;",
        "<?php $key = 0; $ignored = true || ($key = true); $values = []; $values[$key] = 1;",
        "<?php $key = 0; $ignored = [] && ($key = true); $values = []; $values[$key] = 1;",
        "<?php $key = 0; $ignored = [...[]] && ($key = true); $values = []; $values[$key] = 1;",
        "<?php $values = [[1]]; $ignored = match (1) { 1 => ($values = [1]), default => ($values = [[1]]) }; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $values = [[1]]; if ($values = [1]) { $values[0][false] = 2; }",
        "<?php $values = [[1]]; if (true && ($values = [1])) { $values[0][false] = 2; }",
        "<?php $values = [1]; if (false && ($values = [[1]]) && ($values[0][false] = 2)) {}",
        "<?php function fail_after_local_write(): bool { throw new Exception(); } $values = [[1]]; while ($argc > 1) { try { while ($argc > 2) { $values = (($values = [1]) && fail_after_local_write()) ? [[1]] : [[1]]; break 2; } } catch (Exception $error) { break; } break; } $values[0][false] = 'x';",
        "<?php $inner = 'ab'; $values = [$inner, ($inner = [])]; $values[0][true] = 'x';",
        "<?php $inner = 'ab'; $values = ['first' => $inner, 'second' => ($inner = [])]; $values['first'][true] = 'x';",
        "<?php $inner = 'ab'; $values = [0 => $inner, ($inner = [])]; $values[0][true] = 'x';",
        "<?php $inner = 'ab'; $values = [...[$inner, ($inner = [])]]; $values[0][true] = 'x';",
        "<?php $values = [1]; $ignored = false ? ($values = [[1]]) : 0; $values[0][false] = 2;",
        "<?php $values = [['a']]; while ($values = [['a']]) { $values = ['ab']; break; } $values[0][false] = 'x';",
        "<?php $values = [[1]]; while (true) { try { $values = [1]; break; } finally {} } $values[0][false] = 2;",
        "<?php $values = [1]; switch (1) { case 1: break; $values = [[1]]; } $values[0][false] = 2;",
        "<?php $values = [[1]]; while (true) { $values = [1]; } $values[0][false] = 2;",
        "<?php function replace_key_target(&$value): void { $value = ['ab']; } $values = [[]]; replace_key_target($values); $values[0][false] = 'x';",
        "<?php $values = [[]]; try { $values = ['ab']; throw new Exception(); } catch (Exception $error) { $values[0][false] = 'x'; }",
        "<?php $values = [[]]; foreach (($values = ['ab']) as $item) {} $values[0][false] = 'x';",
        "<?php count($values = ['ab']); $values[0][false] = 'x';",
        "<?php $values = [1]; foreach ([] as $item) { $values = [[1]]; } $values[0][false] = 2;",
        "<?php try { $values = [1]; throw new Exception(); } catch (Exception $error) { $values[0][false] = 2; }",
        "<?php $values = [1]; while (true) { try { break; $values = [[1]]; } finally { $values[0][false] = 2; } }",
        "<?php $values = [[1]]; foreach ($values as &$item) { $item = 1; } $values[0][false] = 2;",
        "<?php $values = [[]]; foreach ($values as &$item) {} $item = 'ab'; $values[0][false] = 'x';",
        "<?php function replace_unset_key_target(&$value): void { $value = 'ab'; } replace_unset_key_target($values); $values[false] = 'x';",
        "<?php $values = [[1]]; try { $values = [1]; return; } finally { $values[0][false] = 2; }",
        "<?php $values = [[1]]; try { $values = [1]; throw new Exception(); } finally { $values[0][false] = 2; }",
        "<?php $values = [1]; if ($argc) { foreach ($values as &$item) {} } $item = 'ab'; $values[0][false] = 'x';",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
    crate::semantics::validate_source(
        &parse(
            "<?php function internal_write(array $values): void { $values[false] = 1; }",
            SourceMode::Internal,
        ),
        profile(),
    )
    .expect("compiler-owned helper writes retain internal semantics");
}

/// Ordinary PHP references do not regain independent receiver facts after assignment.
#[test]
fn strict_array_key_source_validation_forgets_ordinary_reference_facts() {
    for source in [
        "<?php $a = [[]]; $b =& $a; $a = [[]]; $b = ['ab']; $a[0][false] = 'x';",
        "<?php $arr = [[[]]]; $alias =& $arr; foreach ($arr as &$elem) { $other =& $elem; $alias = 'ab'; $arr[false] = 1; }",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Assignments after a reference foreach update the last source element through the alias.
#[test]
fn strict_array_key_source_validation_tracks_lingering_foreach_reference_writes() {
    let source = "<?php $values = [1]; foreach ($values as &$item) { $item = []; } $item = 'ab'; $values[0][false] = 'x';";
    crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
}

/// Active foreach references update the source before later body and terminal-path writes.
#[test]
fn strict_array_key_source_validation_tracks_active_foreach_reference_writes() {
    let source = "<?php $values = [[1]]; foreach ($values as &$item) { $item = 1; $values[0][false] = 2; }";
    crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));

    for terminal in ["return;", "throw new Exception();"] {
        let source = format!(
            "<?php $values = [1]; try {{ foreach ($values as &$item) {{ $item = [[1]]; {terminal} }} }} finally {{ $values[0][false] = 2; }}"
        );
        let error = crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
            .expect_err("terminal foreach reference writes must reach finally");
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
}

/// Long left-associated boolean chains keep source validation linear.
#[test]
fn strict_array_key_source_validation_handles_long_boolean_chains() {
    let condition = (0..24)
        .map(|_| "PHP_OS_FAMILY === 'Windows'")
        .collect::<Vec<_>>()
        .join(" && ");
    let source = format!("<?php $values = [1]; if ({condition}) {{ $values[0] = 2; }}");
    crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
        .expect("boolean-chain validation remains bounded");
}

/// Deeply nested loops keep strict source validation linear in the AST depth.
#[test]
fn strict_array_key_source_validation_handles_deep_loops() {
    let mut source = String::from("<?php ");
    for depth in 0..24 {
        source.push_str(&format!("foreach ([1] as $value{depth}) {{ "));
    }
    source.push_str("echo 1;");
    for _ in 0..24 {
        source.push_str(" }");
    }
    crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
        .expect("nested-loop validation remains bounded");

    let mut source = String::from("<?php $shape = [1]; ");
    for depth in 0..24 {
        source.push_str(&format!(
            "while ($argc > {depth}) {{ $shape = [[{depth}]]; "
        ));
    }
    source.push_str("echo 1;");
    for _ in 0..24 {
        source.push_str(" }");
    }
    crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
        .expect("fact-changing nested-loop validation remains bounded");
}

/// Nested loop refinement coalesces identical terminal snapshots at every depth.
#[test]
fn strict_array_key_source_validation_coalesces_nested_loop_terminals() {
    for (open, close) in [
        ("while ($argc > 1) { ", " }"),
        ("do { ", " } while ($argc > 1);"),
        ("for (; $argc > 1;) { ", " }"),
    ] {
        let mut source = String::from("<?php ");
        for _ in 0..24 {
            source.push_str(open);
        }
        source.push_str("if ($argc > 2) { return; }");
        for _ in 0..24 {
            source.push_str(close);
        }
        crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
            .expect("nested terminal snapshots remain bounded");
    }
}

/// Deeply nested finally blocks reuse each control-flow traversal instead of replaying the AST.
#[test]
fn strict_array_key_source_validation_handles_deeply_nested_finally() {
    let mut source = String::from("<?php $values = [1]; ");
    for _ in 0..24 {
        source.push_str("try { ");
    }
    source.push_str("$values = [[1]];");
    for _ in 0..24 {
        source.push_str(" } finally { }");
    }
    source.push_str(" $values[0][false] = 2;");
    let error = crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
        .expect_err("the nested finally fixture still reaches the invalid key write");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");
}

/// Return and throw actions with equal facts share each nested finally traversal.
#[test]
fn strict_array_key_source_validation_reuses_equal_finally_entries() {
    let mut source = String::from("<?php ");
    for _ in 0..20 {
        source.push_str(
            "try { if ($argc > 1) { return; } throw new Exception(); } finally { ",
        );
    }
    source.push_str("echo 1;");
    for _ in 0..20 {
        source.push_str(" }");
    }
    crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
        .expect("equal finally entries remain bounded");
}

/// Finally receives terminal facts produced inside every nested breakable statement.
#[test]
fn strict_array_key_source_validation_replays_finally_after_nested_terminals() {
    for source in [
        "<?php $values = [1]; try { while (true) { $values = [[1]]; return; } } finally { $values[0][false] = 2; }",
        "<?php $values = [1]; try { foreach ([1] as $item) { $values = [[1]]; return; } } finally { $values[0][false] = 2; }",
        "<?php $values = [1]; try { do { $values = [[1]]; return; } while (false); } finally { $values[0][false] = 2; }",
        "<?php $values = [1]; try { for (;;) { $values = [[1]]; return; } } finally { $values[0][false] = 2; }",
        "<?php $values = [1]; try { switch (1) { case 1: $values = [[1]]; return; } } finally { $values[0][false] = 2; }",
    ] {
        let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .expect_err("nested terminal facts must reach finally");
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
    for source in [
        "<?php $values = [[1]]; try { while (true) { $values = 'ab'; return; } } finally { $values[0][false] = 'x'; }",
        "<?php $values = [[1]]; try { foreach ([1] as $item) { $values = 'ab'; return; } } finally { $values[0][false] = 'x'; }",
        "<?php $values = [[1]]; try { do { $values = 'ab'; return; } while (false); } finally { $values[0][false] = 'x'; }",
        "<?php $values = [[1]]; try { for (;;) { $values = 'ab'; return; } } finally { $values[0][false] = 'x'; }",
        "<?php $values = [[1]]; try { switch (1) { case 1: $values = 'ab'; return; } } finally { $values[0][false] = 'x'; }",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Matching catches consume throw facts before their fallthrough reaches finally.
#[test]
fn strict_array_key_source_validation_routes_caught_throws_before_finally() {
    for source in [
        "<?php $values = [1]; try { $values = [[1]]; throw new Exception(); } catch (Exception $error) { $values = [1]; } finally { $values[0][false] = 2; }",
        "<?php class ChildFailure extends RuntimeException {} $values = [1]; try { $values = [[1]]; throw new ChildFailure(); } catch (Exception $error) { $values = [1]; } finally { $values[0][false] = 2; }",
        "<?php function failure(): Throwable { return new Exception(); } $values = [1]; try { $values = [[1]]; throw failure(); } catch (Throwable $error) { $values = [1]; } finally { $values[0][false] = 2; }",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }

    let source = "<?php $values = [1]; try { $values = [[1]]; throw new RuntimeException(); } catch (InvalidArgumentException $error) { $values = [1]; } finally { $values[0][false] = 2; }";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("an unmatched throw must retain its terminal facts");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");

    let source = "<?php $values = [1]; try { $values = [[1]]; throw new Exception(); } catch (Exception $error) {} finally { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } }";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("a caught throw must retain its exact facts through finally");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");

    let source = "<?php $values = [1]; try { try { $values = [[1]]; throw new Exception(); } catch (Exception $error) {} } finally { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } }";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("an enclosing finally must retain caught throw facts");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");

    let source = "<?php $values = [1]; try { $values = [[1]]; throw new Exception(); } catch (Exception $error) {} $values[0][false] = 2;";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("later statements must retain caught throw facts");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");

    let source = "<?php $values = [1]; try { try { $values = [[1]]; throw new Exception(); } catch (Exception $error) { throw new RuntimeException(); } } finally { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } }";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("a rethrow must retain caught throw facts through outer finally");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");
}

/// Exact catch replays stay bounded while retaining outer array facts.
#[test]
fn strict_array_key_source_validation_bounds_nested_catch_replays() {
    let mut source = String::from("<?php ");
    for depth in 0..20 {
        source.push_str(&format!(
            "try {{ $values{depth} = [[1]]; throw new Exception(); }} catch (Exception $error{depth}) {{ "
        ));
    }
    source.push_str("$values0[0][false] = 2;");
    for _ in 0..20 {
        source.push_str(" }");
    }
    let error = crate::semantics::validate_source(&parse(&source, SourceMode::Php), profile())
        .expect_err("nested exact catches retain the outer invalid receiver");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");
}

/// Implicit and equal caught paths keep conservative facts in the published continuation.
#[test]
fn strict_array_key_source_validation_preserves_conservative_catch_paths() {
    for source in [
        "<?php function fail_before_array_change(): void { throw new Exception(); } $values = ['ab']; try { fail_before_array_change(); $values = [[]]; throw new Exception(); } catch (Exception $error) {} $values[0][false] = 'x';",
        "<?php function dynamic_payload(): mixed { return 1; } $values = [[1]]; try { if ($argc > 0) { throw new Exception(); } $values = dynamic_payload(); throw new RuntimeException(); } catch (Exception $error) {} $values[0][false] = 2;",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Catch bodies merge exact and conservative entries that can reach the same handler.
#[test]
fn strict_array_key_source_validation_merges_reachable_catch_entries() {
    for source in [
        "<?php function dynamic_catch_payload(): mixed { return 1; } $values = [[1]]; try { if ($argc > 0) { throw new Exception(); } $values = dynamic_catch_payload(); throw new RuntimeException(); } catch (Exception $error) { $values[0][false] = 2; }",
        "<?php function dynamic_finally_payload(): mixed { return 1; } $values = [[1]]; try { if ($argc > 0) { throw new Exception(); } $values = dynamic_finally_payload(); throw new RuntimeException(); } catch (Exception $error) {} finally { $values[0][false] = 2; }",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }

    for source in [
        "<?php function nested_caught_failure(): void { throw new Exception(); } $values = [1]; try { try { nested_caught_failure(); } catch (Throwable $ignored) {} $values = [[1]]; throw new Exception(); } catch (Exception $error) {} $values[0][false] = 2;",
        "<?php function nested_exception_failure(): void { throw new Exception(); } $values = [1]; try { try { nested_exception_failure(); } catch (Exception $ignored) {} $values = [[1]]; throw new Exception(); } catch (Exception $error) {} $values[0][false] = 2;",
        "<?php $values = [1]; try { switch (1) { case 1: break; } $values = [[1]]; throw new Exception(); } catch (Exception $error) {} $values[0][false] = 2;",
        "<?php $values = [[1]]; try { switch ((array) 1) { case 1: break; } $values = [1]; throw new Exception(); } catch (Throwable $error) {} $values[0][false] = 2;",
    ] {
        let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .expect_err("every reachable caught path retains an invalid array receiver");
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
}

/// Unresolved value-position includes remain conservative throw points before resolution.
#[test]
fn strict_array_key_source_validation_handles_value_includes_in_try_blocks() {
    for source in [
        "<?php try { $value = require 'config.php'; } catch (Throwable $error) {}",
        "<?php function load_config(): mixed { try { return require 'config.php'; } catch (Throwable $error) { return null; } }",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// A nested loop audits one changed repeated entry before the refinement bound applies.
#[test]
fn strict_array_key_source_validation_audits_bounded_nested_replay() {
    let source = "<?php $values = []; $key = 0; $source = 1; while ($outer) { while ($inner) { $values[$key] = 1; $key = $source; } $source = 1.5; $key = 1; }";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("nested repeated float key must be rejected");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");
}

/// Multi-level loop actions carry their exact assignment facts through each target level.
#[test]
fn strict_array_key_source_validation_tracks_multilevel_loop_jumps() {
    for source in [
        "<?php $values = [[1]]; do { while (true) { $values = [1]; continue 2; } } while (false); $values[0][false] = 2;",
        "<?php while (true) { while (true) { $values = [1]; break 2; } } $values[0][false] = 2;",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }

    for source in [
        "<?php $values = [1]; do { while (true) { $values = [[1]]; continue 2; } } while (false); $values[0][false] = 2;",
        "<?php while (true) { while (true) { $values = [[1]]; break 2; } } $values[0][false] = 2;",
        "<?php $values = []; $key = 0; while (true) { switch (1) { case 1: $key = 1.5; break 2; } } $values[$key] = 1;",
        "<?php while (true) { while (true) { try { $values = [1]; break 2; } finally { $values = [[1]]; } } } $values[0][false] = 2;",
    ] {
        let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_err();
        assert!(
            error.message.contains("strict-array-keys"),
            "{source}: {error:?}"
        );
    }
}

/// Exact switch selection still audits source bodies that cannot contribute runtime exits.
#[test]
fn strict_array_key_source_validation_audits_unselected_switch_bodies() {
    for source in [
        "<?php $values = []; switch (1) { case 2: $values[false] = 1; case 1: break; }",
        "<?php $values = []; switch (1) { default: $values[false] = 1; case 1: break; }",
    ] {
        let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .expect_err("strict source validation must audit unselected switch bodies");
        assert!(error.message.contains("strict-array-keys"), "{source}: {error:?}");
    }
}

/// Switch label effects reach matching and no-match exits in PHP evaluation order.
#[test]
fn strict_array_key_source_validation_applies_switch_label_effects() {
    for source in [
        "<?php $values = [[]]; switch (0) { case count($values = ['ab']): break; } $values[0][false] = 'x';",
        "<?php $values = ['ab']; switch (0) { case 0: break; case count($values = [[]]): break; } $values[0][false] = 'x';",
    ] {
        crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }

    let source = "<?php $values = ['ab']; switch (0) { case count($values = [[]]): break; } $values[0][false] = 'x';";
    let error = crate::semantics::validate_source(&parse(source, SourceMode::Php), profile())
        .expect_err("an evaluated case label must update the no-match exit");
    assert!(error.message.contains("strict-array-keys"), "{error:?}");
}

/// Typed selectors are checked on array places without normalizing away their original kinds.
#[test]
fn strict_array_key_checker_validates_original_array_places() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for source in [
            "<?php $key = true; $a = [1]; echo $a[$key];",
            "<?php $key = true; $a = [1]; echo isset($a[$key]);",
            "<?php $key = 1.0; $a = [1]; echo empty($a[$key]);",
            "<?php $key = false; $a = [1]; unset($a[$key]);",
            "<?php $key = 1.0; $a = [1]; echo $a[$key] ?? 9;",
            "<?php $key = 1.0; $a = []; $a[$key] = 1;",
            "<?php $key = null; $fresh[$key] = 1;",
            "<?php $key = 1.0; $a = null; $a[$key] = 1;",
            "<?php $key = false; $a = [[1]]; $a[0][$key] = 1;",
            "<?php $key = 1.0; $a = [1, 'x']; $ref =& $a[$key];",
            "<?php class KeyPlace { public array $items = []; } $o = new KeyPlace(); $key = true; $o->items[$key] = 1;",
            "<?php class StaticKeyPlace { public static array $items = []; } $key = false; StaticKeyPlace::$items[$key] = 1;",
            "<?php function selectKey(array $a, float $key): mixed { return $a[$key]; } echo selectKey([1], 0.0);",
        ] {
            let program = parse(source, SourceMode::Php);
            let error = crate::types::checker::check_staged_semantics(&program, target, profile()).err().unwrap();
            assert!(error.message.contains("strict-array-keys"), "{name}, {source}: {error:?}");
        }
        for source in [
            "<?php $key = 1.0; $text = 'abc'; echo $text[$key];",
            "<?php echo isset($missing[1.0]), empty($missing[1.0]); unset($missing[1.0]);",
            "<?php $a = ['1' => 1, 1 => 2]; $a['01'] = 3; echo count($a);",
            "<?php function dynamicKey(array $a, mixed $key): mixed { return $a[$key]; } echo dynamicKey([1], 0);",
            "<?php class KeyAccess implements ArrayAccess { public function offsetExists(mixed $key): bool { return true; } public function offsetGet(mixed $key): mixed { return 1; } public function offsetSet(mixed $key, mixed $value): void {} public function offsetUnset(mixed $key): void {} } $o = new KeyAccess(); echo $o[true]; $o[false] = 1;",
        ] {
            crate::types::checker::check_staged_semantics(&parse(source, SourceMode::Php), target, profile())
                .unwrap_or_else(|error| panic!("{name}, {source}: {error:?}"));
        }
    }
}
