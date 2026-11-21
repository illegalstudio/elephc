//! Purpose:
//! Verifies original key-domain checks before optimization and storage normalization.
//!
//! Called from:
//! - Focused semantic profile and selected-checker tests.
//!
//! Key details:
//! - Array keys have an independent owner; string offsets and object dimensions are controls.

use super::*;
use crate::{codegen_support::platform::Target, parser::ast::Program, source::SourceMode};

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
        "<?php $inner = 'ab'; $values = [$inner, ($inner = [])]; $values[1][true] = 'x';",
        "<?php function mixed_dynamic_parent(int $key): void { $values = [null, false]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php function array_or_null_dynamic_parent(int $key): void { $values = [[1], null]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php function deep_dynamic_parent(int $key): void { $values = [[[null]], [[false]]]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][0][0][false] = 2; } }",
        "<?php function mixed_typed_parents(int $key, array<int> $left, array<string> $right): void { $values = [$left, $right]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][false] = 2; } }",
        "<?php function ordered_write(): int { $values = [1]; if (PHP_OS_FAMILY === 'Windows') { return ($values = [[1]]) ? ($values[0][false] = 2) : 0; } return 0; }",
        "<?php $values = [1]; if (PHP_OS_FAMILY === 'Windows') { $writer = ($values = [[1]]) ? function () use ($values): void { $values[0][false] = 2; } : null; }",
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
        "<?php $values = [[1]]; if (PHP_OS_FAMILY === 'Windows') { $values[0][($values = [1]) ? false : 0] = 2; }",
        "<?php $values = [[1]]; $writer = function() use (&$values): void { if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; } };",
        "<?php function guarded_dynamic_mixed_depth(int $key): void { $values = [[[1]], 2]; if (PHP_OS_FAMILY === 'Windows') { $values[$key][0][false] = 2; } }",
        "<?php $key = 0; $ignored = false && ($key = true); $values = []; $values[$key] = 1;",
        "<?php $key = 0; $ignored = true || ($key = true); $values = []; $values[$key] = 1;",
        "<?php $key = 0; $ignored = [] && ($key = true); $values = []; $values[$key] = 1;",
        "<?php $values = [[1]]; $ignored = match (1) { 1 => ($values = [1]), default => ($values = [[1]]) }; if (PHP_OS_FAMILY === 'Windows') { $values[0][false] = 2; }",
        "<?php $inner = 'ab'; $values = [$inner, ($inner = [])]; $values[0][true] = 'x';",
        "<?php $inner = 'ab'; $values = ['first' => $inner, 'second' => ($inner = [])]; $values['first'][true] = 'x';",
        "<?php $inner = 'ab'; $values = [0 => $inner, ($inner = [])]; $values[0][true] = 'x';",
        "<?php $inner = 'ab'; $values = [...[$inner, ($inner = [])]]; $values[0][true] = 'x';",
        "<?php $values = [1]; $ignored = false ? ($values = [[1]]) : 0; $values[0][false] = 2;",
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
