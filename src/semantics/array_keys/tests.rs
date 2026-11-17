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
