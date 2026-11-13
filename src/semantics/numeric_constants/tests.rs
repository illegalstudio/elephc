//! Purpose:
//! Verifies exact source numeric evidence before optimization erases invalid operations.
//!
//! Called from:
//! - Focused semantic validation unit tests.
//!
//! Key details:
//! - Named/spread arguments and declaration cycles use the same proof engine as compilation.
//! - Lexical source policies and PHP parameter binding prevent false overflow diagnoses.

use super::*;

/// Associative reductions collapse final values while retaining first-insertion order and key kind.
#[test]
fn strict_literal_array_plan_reduction_evidence_preserves_overwrites_and_exact_keys() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for source in [
        "<?php if (false) { array_sum(['max' => PHP_INT_MAX, 'one' => 1]); }",
        "<?php array_product([false => PHP_INT_MAX, true => 2]);",
        "<?php array_sum(['max' => PHP_INT_MAX, 1]);",
        "<?php const VALUES = ['max' => PHP_INT_MAX, 'two' => 2]; array_product(VALUES);",
    ] {
        let error = super::super::validate_source(&parse(source), profile).expect_err(source);
        assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
    }
    for source in [
        "<?php array_sum(['key' => PHP_INT_MAX, 'key' => 0, 1]);",
        "<?php array_sum([0 => PHP_INT_MAX, 1, 0 => 1]);",
        "<?php array_sum([null => PHP_INT_MAX, '' => 0, 1]);",
        "<?php array_sum([1.5 => PHP_INT_MAX, 1 => 0, 1]);",
        "<?php array_sum([18446744073709551616.0 => PHP_INT_MAX, 0 => 0, 1]);",
        "<?php array_sum(['first' => 0, 'max' => PHP_INT_MAX, 'one' => 1, 'first' => 1.0]);",
        "<?php array_sum(['max' => PHP_INT_MAX, ...$unknown]);",
    ] { assert!(super::super::validate_source(&parse(source), profile).is_ok(), "{source}"); }
    for source in [
        "<?php array_sum(['1' => PHP_INT_MAX, 1 => 0, 'last' => 1]);",
        "<?php array_sum(['9223372036854775807' => PHP_INT_MAX, 1]);",
    ] {
        let program = parse(source);
        assert!(validate(&program, profile).is_ok(), "{source}");
        assert!(validate(&program, profile.with(StrictFeature::ArrayKeys, true)).is_err(), "{source}");
    }
}

/// Dense reductions preserve ordered intermediate overflow and avoid uncertain array key plans.
#[test]
fn strict_numeric_builtins_prove_reductions_and_intdiv_parameter_domains() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for source in [
        "<?php if (false) { array_sum([PHP_INT_MAX, 1]); }",
        "<?php array_product(array: [PHP_INT_MAX, 2]);",
        "<?php array_sum(...[[PHP_INT_MAX, 1]]);",
        "<?php array_sum([PHP_INT_MAX, 1, 1.0]);",
        "<?php declare(strict_types=1); array_product(['9223372036854775807', 2]);",
        "<?php const ITEMS = VALUES; const VALUES = [PHP_INT_MAX, 1]; array_sum(ITEMS);",
        "<?php class Values { const ITEMS = [PHP_INT_MAX, 2]; } array_product(Values::ITEMS);",
        "<?php INTDIV(num2: -1, num1: PHP_INT_MIN);",
        "<?php intdiv(...[PHP_INT_MIN, -1]);",
        "<?php intdiv('-9223372036854775808', -1);",
        "<?php eval('array_sum([PHP_INT_MAX, 1]);');",
    ] {
        let error = super::super::validate_source(&parse(source), profile).expect_err(source);
        assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
    }
    for source in [
        "<?php array_sum([1.0, PHP_INT_MAX, PHP_INT_MAX]); array_product([0, PHP_INT_MAX, 2]);",
        "<?php array_sum(['9223372036854775808', 1]); array_product([-2, 4611686018427387904]);",
        "<?php array_sum(['key' => PHP_INT_MAX, 'key' => 0, 1]);",
        "<?php const ITEMS = ITEMS; array_sum(ITEMS);",
        "<?php intdiv(1, 0); intdiv(PHP_INT_MAX, -1); intdiv('1e50', -1);",
        "<?php declare(strict_types=1); intdiv('-9223372036854775808', -1);",
        "<?php declare(strict_types=1); intdiv(-9223372036854775808.0, -1);",
        "<?php namespace Local; function array_sum($items) { return 0; } array_sum([PHP_INT_MAX, 1]);",
    ] { assert!(super::super::validate_source(&parse(source), profile).is_ok(), "{source}"); }
    let division = parse("<?php echo (4 / 2) + PHP_INT_MAX;");
    assert!(validate(&division, profile).is_err());
    assert!(validate(&division, profile.with(StrictFeature::Arithmetic, true)).is_ok());
    let mut internal = parse("<?php const ITEMS = [PHP_INT_MAX + 1, 1]; array_sum(ITEMS);");
    internal[0].source_mode = SourceMode::Internal;
    assert!(validate(&internal, profile).is_ok());
}

/// Parses checked integer spellings without exposing the unfinished CLI feature.
fn parse(source: &str) -> Program {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    crate::parser::parse(&crate::lexer::tokenize_with_semantics(source, SourceMode::Php, 0, profile).unwrap()).unwrap()
}

/// Power evidence uses Mixed operand rules, planned arguments and lexical source contracts.
#[test]
fn strict_pow_source_constants_share_operator_evidence_and_argument_planning() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for source in [
        "<?php if (false) { pow(2, 63); }",
        "<?php POW(exponent: 2, num: PHP_INT_MAX);",
        "<?php pow(...[PHP_INT_MAX, 2]);",
        "<?php declare(strict_types=1); pow('9223372036854775807', 2);",
        "<?php const BASE = '9223372036854775807'; pow(BASE, 2);",
        "<?php pow('9223372036854775807x', true + 1);",
        "<?php eval('pow(2, 63);');",
    ] {
        let error = super::super::validate_source(&parse(source), profile).expect_err(source);
        assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
    }
    for source in [
        "<?php pow(-2, 63); pow(2, -1); pow('9223372036854775808', 2);",
        "<?php const LOOP = pow(LOOP, 2); pow(LOOP, 2);",
        "<?php function pow($n, $e) { return 0; } pow(PHP_INT_MAX, 2);",
        "<?php pow(num: 0, 2);",
    ] { assert!(super::super::validate_source(&parse(source), profile).is_ok(), "{source}"); }
    let mut program = parse("<?php const BASE = pow(PHP_INT_MAX, 2); echo BASE + 1;");
    program[0].source_mode = SourceMode::Internal;
    assert!(validate(&program, profile).is_ok());
}

/// Provable overflow is rejected in dead syntax, exact-width literals and scalar coercions.
#[test]
fn strict_overflow_constants_validate_unpruned_numeric_trees() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for expression in ["9223372036854775807 + 1", "-9223372036854775808 - 1", "9223372036854775807 * 2",
        "2 ** 63", "-(-9223372036854775808)", "(9223372036854775806 + 1) + 1",
        "'9223372036854775807' + true", "('9223372036854775807' + null) + 1"] {
        let program = parse(&format!("<?php if (false) {{ echo {expression}; }}"));
        let error = validate(&program, profile).unwrap_err();
        assert!(error.message.contains("strict-overflow"), "{expression}: {error:?}");
        assert!(validate(&program, SemanticProfile::PERMISSIVE).is_ok());
    }
    for expression in ["9223372036854775807 + 0", "-9223372036854775808 - 0", "(-2) ** 63", "2 ** -1",
        "1.0e308 * 1.0e308", "'9223372036854775808' + 1", "-9223372036854775808 % -1", "1 / 0"] {
        assert!(validate(&parse(&format!("<?php echo {expression};")), profile).is_ok(), "{expression}");
    }
}

/// Forward aliases and lexical class constants share exact evidence and never recurse through cycles.
#[test]
fn strict_overflow_constants_resolve_aliases_without_guessing_cycles() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for source in [
        "<?php const LAST = LIMIT; const LIMIT = PHP_INT_MAX; if (false) { echo LAST + 1; }",
        "<?php class Limits { const LAST = self::LIMIT; const LIMIT = PHP_INT_MAX; } echo lImItS::LAST + 1;",
        "<?php class Limits { const LAST = PHP_INT_MAX; function check() { echo self::LAST + 1; } }",
    ] { assert!(validate(&parse(source), profile).is_err(), "{source}"); }
    for source in ["<?php const A = B; const B = A; echo A + 1;",
        "<?php if ($argc) { const A = PHP_INT_MAX; } echo A + 1;",
        "<?php const A = PHP_INT_MAX; const A = 0; echo A + 1;",
        "<?php namespace One { const LIMIT = PHP_INT_MAX; } namespace Two { const LIMIT = 0; echo LIMIT + 1; }",
    ] { assert!(validate(&parse(source), profile).is_ok(), "{source}"); }
    for (source, invalid) in [
        ("<?php namespace One { const LIMIT = PHP_INT_MAX; } namespace Two { const LIMIT = 0; echo LIMIT + 1; }", false),
        ("<?php namespace One { const LIMIT = PHP_INT_MAX; } namespace Two { use const One\\LIMIT as MAX; echo MAX + 1; }", true),
    ] {
        let resolved = crate::name_resolver::resolve(parse(source)).unwrap();
        assert_eq!(validate(&resolved, profile).is_err(), invalid, "{source}");
    }
}

/// Trusted implementation constants keep their PHP kind at a checked user consumption site.
#[test]
fn strict_overflow_constants_keep_internal_constant_result_kinds() {
    let mut program = parse("<?php const LIMIT = PHP_INT_MAX + 1; echo -LIMIT;");
    program[0].source_mode = SourceMode::Internal;
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    assert!(validate(&program, profile).is_ok());
    program[0].source_mode = SourceMode::Php;
    assert!(validate(&program, profile).is_err());
}

/// The shared source boundary checks literal eval and all declaration expression positions.
#[test]
fn strict_overflow_constants_validate_literal_eval_and_declaration_defaults() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for source in [
        "<?php if (false) { eval('echo PHP_INT_MAX + 1;'); }",
        "<?php eval('echo 1;'); eval('echo 2 ** 63;');",
        "<?php function example(int $value = PHP_INT_MAX + 1) {}",
        "<?php class Example { public int $value = PHP_INT_MAX + 1; }",
        "<?php #[Example(PHP_INT_MAX + 1)] class Annotated {}",
        "<?php $closure = fn() => PHP_INT_MAX + 1;",
    ] {
        let error = super::super::validate_source(&parse(source), profile).unwrap_err();
        assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
    }
    let program = parse("<?php const OUTER_MAX = PHP_INT_MAX; eval('echo OUTER_MAX + 1;'); eval('echo 1 + 2;');");
    assert!(super::super::validate_source(&program, profile).is_ok());
}

/// Abs uses full numeric-string parameters, shared call planning and declaration-safe proofs.
#[test]
fn strict_abs_source_constants_validate_known_magnitudes_without_guessing_names() {
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
    for source in [
        "<?php abs(PHP_INT_MIN);",
        "<?php if (false) { ABS(num: PHP_INT_MIN); }",
        "<?php abs(...[PHP_INT_MIN]);",
        "<?php const MINIMUM = PHP_INT_MIN; abs(MINIMUM);",
        "<?php const MINIMUM = '-9223372036854775808'; abs(MINIMUM);",
        "<?php class Limits { const MINIMUM = PHP_INT_MIN; } abs(Limits::MINIMUM);",
        "<?php eval('abs(PHP_INT_MIN);');",
    ] {
        let error = super::super::validate_source(&parse(source), profile)
            .expect_err(source);
        assert!(error.message.contains("strict-overflow"), "{source}: {error:?}");
    }
    for source in [
        "<?php abs(PHP_INT_MAX); abs(-1.25); abs('-9223372036854775809');",
        "<?php abs('-9223372036854775808x');",
        "<?php declare(strict_types=1); abs('-9223372036854775808');",
        "<?php declare(strict_types=1); echo PHP_INT_MAX + abs(true);",
        "<?php const PREFIX = '-9223372036854775808x'; abs(PREFIX);",
        "<?php const LOOP = abs(LOOP); abs(LOOP);",
        "<?php namespace Local; function abs($n) { return 0; } abs(PHP_INT_MIN);",
        "<?php function abs($n) { return 0; } abs(PHP_INT_MIN);",
        "<?php extern function abs(int $n): int; abs(PHP_INT_MIN);",
    ] {
        assert!(super::super::validate_source(&parse(source), profile).is_ok(), "{source}");
    }
    let mut internal = parse("<?php abs(PHP_INT_MIN);");
    internal[0].source_mode = SourceMode::Internal;
    assert!(validate(&internal, profile).is_ok());
}
