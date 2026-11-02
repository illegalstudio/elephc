//! Purpose:
//! Covers concrete enum variadic transport and weak integer argument boundaries.
//!
//! Called from:
//! - The codegen types integration suite.
//!
//! Key details:
//! - Runtime unpacks exercise the selected binder without statically expanding arguments.
//! - Both EIR optimization modes preserve nullsafe laziness and PHP numeric diagnostics.

use crate::support::*;

/// Concrete nullsafe enum calls use the same collector layout as their compiled bodies.
#[test]
fn test_enum_argument_boundaries_concrete_nullsafe_collectors() {
    let out = compile_and_run(r#"<?php
enum BoundaryValues {
    case A;
    public function strings(string ...$values): string { return implode('|', $values); }
}
function string_call(?BoundaryValues $value, array $args): ?string {
    return $value?->strings(...$args);
}
function argument_side_effect(): array { echo 'bad'; return ['unused']; }
function lazy_call(?BoundaryValues $value): ?string {
    return $value?->strings(...argument_side_effect());
}
echo string_call(BoundaryValues::A, ['one', 'two']), '|';
echo string_call(BoundaryValues::A, ['named' => 'three']), '|';
echo BoundaryValues::A->strings('direct'), '|';
echo string_call(null, ['unused']) === null ? 'N' : 'bad';
echo lazy_call(null) === null ? 'N' : 'bad';
"#);
    assert_eq!(out, "one|two|three|direct|NN");
}

/// Collector transport cannot depend on EIR optimization recovering an incorrect layout.
#[test]
fn test_enum_argument_boundaries_concrete_nullsafe_collectors_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_concrete_nullsafe_collectors);
}

/// Weak integer unpacking rejects non-finite and upper-overflow values without losing exact ints.
#[test]
fn test_enum_argument_boundaries_weak_integer_range() {
    let out = compile_and_run(r#"<?php
interface BoundaryInteger { public function value(int $value): int; }
enum BoundaryIntegerValue implements BoundaryInteger {
    case A;
    public function value(int $value): int { return $value; }
}
function integer_boundary(BoundaryInteger $target, array $args): string {
    try { return 'V:' . $target->value(...$args); }
    catch (TypeError $error) { return 'T'; }
}
foreach (['9223372036854775808', '1e400', INF, -INF, NAN,
    9223372036854775808.0, PHP_INT_MAX, '9223372036854775807',
    '-9223372036854775808', '-9223372036854775809', -9223372036854775808.0,
    '1e3', true] as $value) {
    echo integer_boundary(BoundaryIntegerValue::A, [$value]), '|';
}
"#);
    assert_eq!(out, "T|T|T|T|T|T|V:9223372036854775807|V:9223372036854775807|V:-9223372036854775808|V:-9223372036854775808|V:-9223372036854775808|V:1000|V:1|");
}

/// Integer acceptance retains the PHP boundary rules with EIR optimization disabled.
#[test]
fn test_enum_argument_boundaries_weak_integer_range_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_weak_integer_range);
}

/// Lossy weak integer binding reports the original float or quoted numeric-string value.
#[test]
fn test_enum_argument_boundaries_weak_integer_precision() {
    let out = compile_and_run_capture(r#"<?php
error_reporting(32767);
interface BoundaryPrecision { public function value(int $value): int; }
enum BoundaryPrecisionValue implements BoundaryPrecision {
    case A;
    public function value(int $value): int { return $value; }
}
function precision_boundary(BoundaryPrecision $target, array $args): int {
    return $target->value(...$args);
}
echo precision_boundary(BoundaryPrecisionValue::A, [1.5]), '|';
echo precision_boundary(BoundaryPrecisionValue::A, ['2.75']);
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|2");
    assert!(out.stderr.contains("Implicit conversion from float 1.5 to int loses precision"), "{}", out.stderr);
    assert!(out.stderr.contains("Implicit conversion from float-string \"2.75\" to int loses precision"), "{}", out.stderr);
}

/// Precision diagnostics remain observable without EIR optimization.
#[test]
fn test_enum_argument_boundaries_weak_integer_precision_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_weak_integer_precision);
}

/// Scalar collectors store coerced values and failed int attempts try other union members.
#[test]
fn test_enum_argument_boundaries_scalar_collectors_and_union_fallbacks() {
    let out = compile_and_run(r#"<?php
error_reporting(0);
interface BoundaryScalars {
    public function integers(int ...$values): string;
    public function floats(float ...$values): string;
    public function texts(string ...$values): string;
}
enum ScalarValues implements BoundaryScalars {
    case A;
    public function integers(int ...$values): string {
        $out = ''; foreach ($values as $key => $value) {
            $out .= gettype($key) . ':' . $key . '=' . gettype($value) . ':' . $value . ';';
        } return $out;
    }
    public function floats(float ...$values): string {
        $out = ''; foreach ($values as $value) { $out .= gettype($value) . ':' . $value . ';'; } return $out;
    }
    public function texts(string ...$values): string {
        $out = ''; foreach ($values as $value) { $out .= gettype($value) . ':' . $value . ';'; } return $out;
    }
}
function integer_values(BoundaryScalars $target, array $args): string { return $target->integers(...$args); }
function float_values(BoundaryScalars $target, array $args): string { return $target->floats(...$args); }
function text_values(BoundaryScalars $target, array $args): string { return $target->texts(...$args); }
interface BoundaryText { public function value(int|string $value): mixed; }
enum TextValue implements BoundaryText { case A; public function value(int|string $value): mixed { return $value; } }
interface BoundaryBool { public function value(int|bool $value): mixed; }
enum BoolValue implements BoundaryBool { case A; public function value(int|bool $value): mixed { return $value; } }
interface BoundaryNumber { public function value(int|float $value): mixed; }
enum NumberValue implements BoundaryNumber { case A; public function value(int|float $value): mixed { return $value; } }
function text_union(BoundaryText $target, array $args): mixed { return $target->value(...$args); }
function bool_union(BoundaryBool $target, array $args): mixed { return $target->value(...$args); }
function number_union(BoundaryNumber $target, array $args): mixed { return $target->value(...$args); }
function describe_boundary(mixed $value): string { return gettype($value) . ':' . (string)$value . ';'; }
echo integer_values(ScalarValues::A, [1.5, '2', true, 'named' => '2.75']), '|';
echo float_values(ScalarValues::A, [true, '2.5', 3]), '|';
echo text_values(ScalarValues::A, [7, true, false]), '|';
foreach ([1.5, INF, '1.5', true] as $value) { echo describe_boundary(text_union(TextValue::A, [$value])); }
echo '|';
foreach ([true, 'foo', '', '9223372036854775808', INF, 1.5, '0.0'] as $value) {
    echo describe_boundary(bool_union(BoolValue::A, [$value]));
}
echo '|';
foreach (['2', '2.75', 1.5, true] as $value) { echo describe_boundary(number_union(NumberValue::A, [$value])); }
echo is_float(number_union(NumberValue::A, ['9223372036854775808'])) ? 'float' : 'bad';
"#);
    assert_eq!(out, "integer:0=integer:1;integer:1=integer:2;integer:2=integer:1;string:named=integer:2;|double:1;double:2.5;double:3;|string:7;string:1;string:;|integer:1;string:INF;string:1.5;integer:1;|boolean:1;boolean:1;boolean:;boolean:1;boolean:1;integer:1;integer:0;|integer:2;double:2.75;double:1.5;integer:1;float");
}

/// Scalar transport and union preference do not depend on EIR optimization.
#[test]
fn test_enum_argument_boundaries_scalar_collectors_and_union_fallbacks_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_scalar_collectors_and_union_fallbacks);
}

/// Throwing deprecation handlers retire staged arguments before the selected method runs.
#[test]
fn test_enum_argument_boundaries_precision_handler_unwind_owners() {
    let mut baseline = None;
    for iterations in [1, 16] {
        let out = compile_and_run_with_heap_debug(&format!(r#"<?php
error_reporting(32767);
interface BoundaryUnwind {{ public function value(int $value): int; }}
enum UnwindValue implements BoundaryUnwind {{
    case A; public function value(int $value): int {{ return $value; }}
}}
function unwind_value(BoundaryUnwind $target, array $args): int {{ return $target->value(...$args); }}
set_error_handler(function(int $severity, string $message): bool {{
    echo $severity, ':'; throw new RuntimeException('precision');
}});
for ($i = 0; $i < {iterations}; $i++) {{
    foreach ([1.5, '2.75'] as $value) {{
        try {{ unwind_value(UnwindValue::A, [$value]); echo 'bad'; }}
        catch (RuntimeException $error) {{ echo 'C|'; }}
        unset($error);
    }}
}}
restore_error_handler();
"#));
        assert!(out.success, "{}\n{}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "8192:C|8192:C|".repeat(iterations));
        let summary = out.stderr.lines().find(|line| line.contains("live_blocks=")).unwrap();
        let retained = ["live_blocks=", "live_bytes="].map(|metric| {
            summary.split_whitespace().find_map(|field| field.strip_prefix(metric))
                .unwrap().parse::<usize>().unwrap()
        });
        if let Some(baseline) = baseline {
            assert_eq!(retained, baseline, "warning unwind retains extra owners: {}", out.stderr);
        } else {
            baseline = Some(retained);
        }
    }
}

/// Warning-handler unwinding remains balanced with EIR optimization disabled.
#[test]
fn test_enum_argument_boundaries_precision_handler_unwind_owners_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_precision_handler_unwind_owners);
}

/// Nullable and union argument errors share PHP's canonical type spelling with property guards.
#[test]
fn test_enum_argument_boundaries_canonical_type_errors() {
    let out = compile_and_run(r#"<?php
interface NullableBoundary { public function value(?int $value): int; }
enum NullableBoundaryValue implements NullableBoundary {
    case A; public function value(?int $value): int { return $value ?? 0; }
}
interface TextBoundary { public function value(int|string $value): mixed; }
enum TextBoundaryValue implements TextBoundary {
    case A; public function value(int|string $value): mixed { return $value; }
}
function nullable_error(NullableBoundary $target, array $args): string {
    try { $target->value(...$args); return 'bad'; }
    catch (TypeError $error) { return $error->getMessage(); }
}
function text_error(TextBoundary $target, array $args): string {
    try { $target->value(...$args); return 'bad'; }
    catch (TypeError $error) { return $error->getMessage(); }
}
echo nullable_error(NullableBoundaryValue::A, ['9223372036854775808']), '|';
echo text_error(TextBoundaryValue::A, [[1]]);
"#);
    assert_eq!(out, "NullableBoundaryValue::value(): Argument #1 ($value) must be of type ?int, string given|TextBoundaryValue::value(): Argument #1 ($value) must be of type string|int, array given");
}

/// Canonical errors and the throwing unused call survive EIR optimization being disabled.
#[test]
fn test_enum_argument_boundaries_canonical_type_errors_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_canonical_type_errors);
}

/// Typed staged arguments preserve coercion diagnostics before a missing-argument exception.
#[test]
fn test_enum_argument_boundaries_typed_missing_arguments() {
    let source = r#"<?php
interface MissingBoundary { public function value(int $first, int $second): int; }
enum MissingBoundaryValue implements MissingBoundary {
    case A;
    public function value(int $first, int $second): int { return $first + $second; }
}
function good_int(): int { return 1; }
function good_float(): float { return 1.5; }
function good_string(): string { return str_repeat('2.75', 1); }
function good_bool(): bool { return true; }
function boundary_handler(int $code, string $message): bool { echo 'D'; return true; }
set_error_handler('boundary_handler');
function baseline_boundary(MissingBoundary $target): void { $target->value(1, 2); }
function missing_boundary(MissingBoundary $target): void {
    try { $target->value(good_int()); } catch (ArgumentCountError $error) { echo 'I|'; unset($error); }
    try { $target->value(good_float()); } catch (ArgumentCountError $error) { echo 'F|'; unset($error); }
    try { $target->value(good_string()); } catch (ArgumentCountError $error) { echo 'S|'; unset($error); }
    try { $target->value(good_bool()); } catch (ArgumentCountError $error) { echo 'B'; unset($error); }
}
missing_boundary(MissingBoundaryValue::A);
restore_error_handler();
"#;
    let mut baseline = None;
    for iterations in [1, 16] {
        let source = source.replace("missing_boundary(MissingBoundaryValue::A);", &format!(
            "for ($i = 0; $i < {iterations}; $i++) {{ missing_boundary(MissingBoundaryValue::A); }}",
        ));
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{}\n{}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "I|DF|DS|B".repeat(iterations), "{}", out.stderr);
        // Ordinary successful enum-interface calls retain a singleton wrapper too. Compare
        // its complete inventory at the same call count, without hiding extra argument leases.
        let control = compile_and_run_with_heap_debug(&source.replace(
            "missing_boundary(MissingBoundaryValue::A);", "baseline_boundary(MissingBoundaryValue::A);",
        ));
        assert!(control.success, "{}", control.stderr);
        assert_eq!(control.stdout, "", "{}", control.stderr);
        let inventory = |stderr: &str| {
            let summary = stderr.lines().find(|line| line.contains("live_blocks=")).unwrap();
            let retained = ["live_blocks=", "live_bytes="].map(|metric| summary.split_whitespace()
                .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap());
            assert!(retained[0] <= 1, "{stderr}");
            let blocks: Vec<_> = stderr.lines().filter(|line| line.starts_with("HEAP DEBUG: live offset="))
                .map(|line| ["kind=", "refs="].map(|metric| line.split_whitespace()
                    .find_map(|field| field.strip_prefix(metric)).unwrap().parse::<usize>().unwrap()))
                .collect();
            assert_eq!(blocks.len(), retained[0], "{stderr}");
            assert!(blocks.iter().all(|block| block[0] == 4), "{stderr}");
            (retained, blocks)
        };
        let (retained, blocks) = inventory(&out.stderr);
        assert_eq!((retained, blocks), inventory(&control.stderr), "{}\n{}", out.stderr, control.stderr);
        if let Some(baseline) = baseline { assert_eq!(retained, baseline, "{}", out.stderr); }
        else { baseline = Some(retained); }
    }
}

/// Unreachable conversion blocks must still be well-typed with EIR optimization disabled.
#[test]
fn test_enum_argument_boundaries_typed_missing_arguments_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_typed_missing_arguments);
}

/// Named holes reject before coercion, after evaluating every supplied source argument once.
#[test]
fn test_enum_argument_boundaries_named_hole_diagnostics() {
    let out = compile_and_run(r#"<?php
interface NamedHole { public function value(int $a, int $b, int $c): void; }
enum NamedHoleValue implements NamedHole {
    case A; public function value(int $a, int $b, int $c): void {}
}
function fractional(): float { echo 'F'; return 1.5; }
function fractional_text(): string { echo 'S'; return str_repeat('2.75', 1); }
function later(): int { echo 'L'; return 2; }
function hole_handler(int $code, string $message): bool { echo 'D'; return true; }
set_error_handler('hole_handler');
function run_holes(NamedHole $target): void {
    try { $target->value(fractional(), c: later()); } catch (ArgumentCountError $e) { echo 'A|'; unset($e); }
    try { $target->value(a: fractional(), c: later()); } catch (ArgumentCountError $e) { echo 'A|'; unset($e); }
    try { $target->value(fractional_text(), c: later()); } catch (ArgumentCountError $e) { echo 'A|'; unset($e); }
    try { $target->value(fractional()); } catch (ArgumentCountError $e) { echo 'A|'; unset($e); }
    try { $target->value(a: fractional()); } catch (ArgumentCountError $e) { echo 'A|'; unset($e); }
    try { $target->value('invalid', c: later()); } catch (ArgumentCountError $e) { echo 'A|'; unset($e); }
    try { $target->value(...['a' => fractional(), 'c' => later()]); }
    catch (ArgumentCountError $e) { echo $e->getMessage(); unset($e); }
    echo '|';
    try { $target->value(...[fractional()], b: later()); }
    catch (ArgumentCountError $e) { echo 'E:', $e->getMessage(); unset($e); }
    echo '|';
    try { $target->value(...['invalid'], b: later()); }
    catch (TypeError $e) { echo 'E:', $e->getMessage(); unset($e); }
    echo '|';
    try { $target->value(...[], a: fractional()); }
    catch (ArgumentCountError $e) { echo 'E:', $e->getMessage(); unset($e); }
}
run_holes(NamedHoleValue::A);
restore_error_handler();
"#);
    assert_eq!(out, concat!(
        "FLA|FLA|SLA|FDA|FDA|LA|FLNamedHoleValue::value(): Argument #2 ($b) not passed",
        "|FLDE:Too few arguments to function NamedHoleValue::value(), 2 passed and exactly 3 expected",
        "|LE:NamedHoleValue::value(): Argument #1 ($a) must be of type int, string given",
        "|FDE:Too few arguments to function NamedHoleValue::value(), 1 passed and exactly 3 expected",
    ));
}

/// Named-hole and missing-tail diagnostic ordering is identical without EIR optimization.
#[test]
fn test_enum_argument_boundaries_named_hole_diagnostics_no_ir_opt() {
    without_ir_opt(test_enum_argument_boundaries_named_hole_diagnostics);
}
