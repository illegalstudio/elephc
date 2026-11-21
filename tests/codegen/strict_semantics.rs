//! Purpose:
//! Exercises independently selected strict guarantees through the real CLI pipeline.
//!
//! Called from:
//! - The focused codegen integration test binary.
//!
//! Key details:
//! - Each fixture uses the existing isolated project and cleanup helpers.

use crate::support::*;

/// Checked helper entries throw for boxed and scalar integer overflow with balanced native owners.
#[test]
fn test_strict_boxed_overflow_staged_native_arithmetic_and_powers() {
    let source = r#"<?php
function strict_add(mixed $a, mixed $b): mixed { return $a + $b; }
function strict_sub(mixed $a, mixed $b): mixed { return $a - $b; }
function strict_mul(mixed $a, mixed $b): mixed { return $a * $b; }
function strict_pow(mixed $a, mixed $b): mixed { return $a ** $b; }
function strict_int_pow(int $a, int $b): mixed { return $a ** $b; }
function strict_scalar_rhs(mixed $a, int $b): mixed { return $a + $b; }
function strict_scalar_lhs(int $a, mixed $b): mixed { return $a * $b; }
function strict_discard(int $a, int $b): void {
    try { $a + $b; } catch (ArithmeticError $e) { echo 'D'; }
}
function strict_discard_negate(int $a): void {
    try { (-$a); } catch (ArithmeticError $e) { echo 'N'; }
}
try { strict_add(9223372036854775807, 1); } catch (ArithmeticError $e) { echo 'A'; }
try { strict_sub(-9223372036854775808, 1); } catch (ArithmeticError $e) { echo 'S'; }
try { strict_mul(9223372036854775807, 2); } catch (ArithmeticError $e) { echo 'M'; }
try { strict_pow(2, 63); } catch (ArithmeticError $e) { echo 'P'; }
try { strict_int_pow(2, 63); } catch (ArithmeticError $e) { echo 'I'; }
try { strict_scalar_rhs(strict_add(9223372036854775807, 0), 1); } catch (ArithmeticError $e) { echo 'R'; }
try { strict_scalar_lhs(2, strict_add(9223372036854775807, 0)); } catch (ArithmeticError $e) { echo 'L'; }
try { (strict_add(9223372036854775807, 0) + strict_add(1, 0)) * 2; } catch (ArithmeticError $e) { echo 'T'; }
strict_discard(9223372036854775807, 1); strict_discard_negate(-9223372036854775808);
echo ':', strict_add(20, 22), ':', strict_sub(-20, 22), ':', strict_mul(-6, 7);
echo ':', strict_pow(-2, 63), ':', strict_int_pow(-2, 63);
echo ':', strict_pow(2, -1), ':', strict_int_pow(2, -1), ':', strict_pow(2.0, 3);
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "ASMPIRLTDN:42:-42:-42:-9223372036854775808:-9223372036854775808:0.5:0.5:8", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Failed increments leave locals, references, properties, array elements and buffers unchanged.
#[test]
fn test_strict_overflow_staged_updates_preserve_storage_and_owners() {
    let source = r#"<?php
function strict_post(mixed &$value): mixed { return $value++; }
function strict_pre(mixed &$value): mixed { return --$value; }
function strict_int_post(int &$value): mixed { return $value++; }
function strict_buffer_post(buffer<int> $values, int $index): mixed { return $values[$index]++; }
class StrictUpdateCell { public int $value = 9223372036854775807; }
$value = 9223372036854775807;
try { strict_post($value); } catch (ArithmeticError $e) { echo 'M'; }
echo $value, ':';
$value = -9223372036854775808;
try { strict_pre($value); } catch (ArithmeticError $e) { echo 'D'; }
echo $value, ':';
$integer = 9223372036854775807;
try { strict_int_post($integer); } catch (ArithmeticError $e) { echo 'R'; }
echo $integer, ':';
$text = '9223372036854775807';
try { strict_post($text); } catch (ArithmeticError $e) { echo 'S'; }
echo $text, ':';
$text = '-9223372036854775808';
try { strict_pre($text); } catch (ArithmeticError $e) { echo 'N'; }
echo $text, ':';
$cell = new StrictUpdateCell();
try { $cell->value++; } catch (ArithmeticError $e) { echo 'P'; }
echo $cell->value, ':';
$array = [9223372036854775807];
try { $array[0]++; } catch (ArithmeticError $e) { echo 'A'; }
echo $array[0], ':';
try { $array[0] += 1; } catch (ArithmeticError $e) { echo 'C'; }
echo $array[0], ':';
$buffer = buffer_new<int>(1); $buffer[0] = 9223372036854775807;
try { strict_buffer_post($buffer, 0); } catch (ArithmeticError $e) { echo 'B'; }
echo $buffer[0], ':'; buffer_free($buffer);
$small = 4; echo strict_post($small), ':', $small, ':';
$letters = 'az'; echo strict_post($letters), ':', $letters;
$wide = '9223372036854775808'; strict_post($wide); echo ':', gettype($wide);
$prefix = '9223372036854775807foo'; strict_post($prefix); echo ':', $prefix;
$decimal = 1.5; strict_post($decimal); echo ':', $decimal;
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "M9223372036854775807:D-9223372036854775808:R9223372036854775807:S9223372036854775807:N-9223372036854775808:P9223372036854775807:A9223372036854775807:C9223372036854775807:B9223372036854775807:4:5:az:ba:double:9223372036854775807fop:2.5", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Keeps verified literal eval native with either EIR optimization setting.
#[test]
fn test_strict_dynamic_code_literal_eval_optimization_parity() {
    for optimization in ["--ir-opt=on", "--ir-opt=off"] {
        assert_eq!(StrictFixture::new("<?php eval('echo 42;');").run(&["--strict-dynamic-code", optimization]), "42");
    }
}

/// Refuses opaque source before folding and refuses unsupported interpreter fallbacks.
#[test]
fn test_strict_dynamic_code_refuses_runtime_source_and_literal_bridge_fallback() {
    for source in [
        "<?php $code = $argc > 0 ? 'echo 42;' : ''; eval($code);",
        "<?php if (false) { eval($code); }",
        "<?php eval('class StrictLiteralBridgeClass {}');",
    ] {
        StrictFixture::new(source).assert_error(&["--strict-dynamic-code"], "strict-dynamic-code");
    }
}

/// A separate strict feature can install its native eval profile without enabling dynamic-code restrictions.
#[test]
fn test_strict_dynamic_code_override_keeps_independent_locals_eval_context() {
    assert_eq!(StrictFixture::new("<?php echo eval('class StrictProfileContextClass {} return 17;');")
        .run(&["--strict-locals", "--no-strict-dynamic-code"]), "17");
}

/// Included physical source is checked before frontend simplification can discard its branch.
#[test]
fn test_strict_dynamic_code_validates_included_physical_source() {
    let fixture = StrictFixture::new("<?php require __DIR__ . '/included.php';");
    fixture.file("included.php", "<?php if (false) { eval($source); }");
    fixture.assert_error(&["--strict-dynamic-code"], "strict-dynamic-code");
}

/// Composer-loaded declarations retain the same source restrictions as the entry file.
#[test]
fn test_strict_dynamic_code_validates_autoloaded_physical_source() {
    let fixture = StrictFixture::new("<?php $service = new StrictAutoloadService();");
    fixture.file("composer.json", r#"{"autoload":{"classmap":["StrictAutoloadService.php"]}}"#);
    fixture.file("StrictAutoloadService.php",
        "<?php class StrictAutoloadService { public function run(): void { if (false) { eval($source); } } }");
    fixture.assert_error(&["--strict-dynamic-code"], "strict-dynamic-code");
}

/// Rejects invalid source conditions even when folding could remove their use.
#[test]
fn test_strict_bools_rejects_nonboolean_conditions_before_folding() {
    for source in [
        "<?php if (1) { echo 'bad'; }",
        "<?php if (strlen('x')) { echo 'bad'; }",
        "<?php $x = 1; if ($x) { echo 'bad'; }",
        "<?php while (0) {}",
        "<?php do {} while ('');",
        "<?php for (; 0;) {}",
        "<?php echo !1;",
        "<?php echo true && 1;",
        "<?php echo true xor 1;",
        "<?php echo 1 ? 'bad' : 'bad';",
        "<?php echo 1 ?: 'bad';",
        "<?php echo array_any([1], fn($x): int => 1);",
        "<?php echo array_filter([1], fn($x): int => 1);",
        "<?php eval('if (1) { echo 42; }');",
    ] {
        StrictFixture::new(source).assert_error(&["--strict-bools"], "strict-bools");
    }
}

/// Guards mixed conditions, preserves lazy evaluation, and keeps explicit formatting/probes usable.
#[test]
fn test_strict_bools_runtime_guards_and_optimization_parity() {
    let source = r#"<?php
function strict_bool_if(mixed $x): void { if ($x) { echo 'T'; } else { echo 'F'; } }
function strict_bool_not(mixed $x): void { echo !$x ? 'N' : 'Y'; }
function strict_bool_short(mixed $x): void { echo $x ?: false; }
function strict_bool_side(): bool { echo 'S'; return true; }
function strict_bool_xor(mixed $x): void { $r = $x xor strict_bool_side(); }
function strict_bool_and(mixed $x): void { $r = $x && strict_bool_side(); }
strict_bool_if(true); strict_bool_if(false); strict_bool_not(false);
foreach ([1, 'x', null, []] as $value) {
    try { strict_bool_if($value); } catch (TypeError $error) { echo 'E'; }
}
try { strict_bool_not(1); } catch (TypeError $error) { echo 'N'; }
try { strict_bool_short(1); } catch (TypeError $error) { echo 'Q'; }
try { strict_bool_xor(1); } catch (TypeError $error) { echo 'X'; }
strict_bool_and(false);
if ((bool) $argc) { echo 'B'; }
$a = []; echo isset($a['absent']) ? 'bad' : 'I'; echo empty($a['absent']) ? 'P' : 'bad';
echo $a['absent'] ?? 'C';
"#;
    for optimization in ["--ir-opt=on", "--ir-opt=off"] {
        assert_eq!(StrictFixture::new(source).run(&["--strict-bools", optimization]), "TFNEEEENQSXBIPC");
    }
}

/// Predicate callbacks require boolean results while callback-free filtering keeps its data contract.
#[test]
fn test_strict_bools_array_predicate_callbacks() {
    let source = r#"<?php
function strict_bool_predicate(mixed $value): mixed { return $value; }
function strict_bool_owned_predicate(mixed $value): mixed { return str_repeat('x', $value); }
try { array_filter([1], strict_bool_predicate(...)); } catch (TypeError $error) { echo 'E'; }
try { array_any([1], strict_bool_predicate(...)); } catch (TypeError $error) { echo 'E'; }
try { array_all([1], strict_bool_predicate(...)); } catch (TypeError $error) { echo 'E'; }
try { array_find([1], strict_bool_predicate(...)); } catch (TypeError $error) { echo 'E'; }
$filter = array_filter(...);
try { $filter([1], strict_bool_owned_predicate(...)); } catch (TypeError $error) { echo 'E'; }
echo count(array_filter([0, 1, 2]));
echo count(array_filter([1, 2], fn($value): bool => $value > 1));
"#;
    let output = StrictFixture::new(source).capture(&["--strict-bools", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "EEEEE21");
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
}

/// Iterator callback results use the same consumed-cell guard as direct conditions.
#[test]
fn test_strict_bools_iterator_apply_callback_results() {
    let source = r#"<?php
class StrictBoolIterator implements Iterator {
    private int $position = 0;
    public function rewind(): void { $this->position = 0; }
    public function valid(): bool { return $this->position < 1; }
    public function current(): int { return $this->position; }
    public function key(): int { return $this->position; }
    public function next(): void { $this->position++; }
}
function strict_iterator_result(int $count): mixed { return str_repeat('x', $count); }
try { iterator_apply(new StrictBoolIterator(), strict_iterator_result(...), [$argc]); }
catch (TypeError $error) { echo 'E'; }
echo iterator_apply(new StrictBoolIterator(), fn(): bool => false);
"#;
    let output = StrictFixture::new(source).capture(&["--strict-bools", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "E1");
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
}

/// Opaque eval retains independent boolean rules when dynamic source itself is permitted.
#[test]
fn test_strict_bools_native_eval_context_preserves_guarantees() {
    let source = r#"<?php
$code = $argc > 0 ? 'try { if (1) { echo "bad"; } } catch (TypeError $error) { echo "E"; } if ((bool) 1) { echo "B"; }' : '';
eval($code);
"#;
    assert_eq!(StrictFixture::new(source).run(&["--strict-bools", "--no-strict-dynamic-code"]), "EB");
}

/// Failure is established before releasing an invalid callback result with a throwing destructor.
#[test]
fn test_strict_bools_callback_destructor_keeps_failure_as_previous() {
    let source = r#"<?php
class StrictBoolThrowingResult {
    public function __destruct() { throw new RuntimeException('destructor'); }
}
class StrictBoolSingleIterator implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return true; }
    public function current(): int { return 0; }
    public function key(): int { return 0; }
    public function next(): void {}
}
function strict_bool_object_result(): mixed { return new StrictBoolThrowingResult(); }
try { iterator_apply(new StrictBoolSingleIterator(), strict_bool_object_result(...)); }
catch (RuntimeException $error) { echo get_class($error->getPrevious()); }
"#;
    let output = StrictFixture::new(source).capture(&["--strict-bools", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "TypeError");
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("HEAP DEBUG: leak summary: clean"), "{errors}");
}

/// Numeric-looking simple interpolation keys retain their original bytes before key policy.
#[test]
fn test_strict_integer_literal_interpolation_text_keys_remain_strings() {
    let source = r#"<?php
$items = [0 => 'zero', '1' => 'one', '01' => 'leading', '-0' => 'negative', '9223372036854775808' => 'large'];
echo "$items[01]|$items[-0]|$items[9223372036854775808]";
class StrictTextKeyProbe implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return true; }
    public function offsetGet(mixed $key): mixed { return (is_int($key) ? 'i:' : 's:') . (string) $key; }
    public function offsetSet(mixed $key, mixed $value): void {}
    public function offsetUnset(mixed $key): void {}
}
$probe = new StrictTextKeyProbe();
echo "|$probe[1]|$probe[01]|$probe[-0]|$probe[9223372036854775808]";
"#;
    assert_eq!(StrictFixture::new(source).run(&[]), "leading|negative|large|i:1|s:01|s:-0|s:9223372036854775808");
}

/// Native and eval OPcache calls reuse a runtime-created script under the same explicit profile.
#[test]
fn test_strict_cache_native_and_eval_profile_parity() {
    let fixture = StrictFixture::new(r#"<?php
$path = __DIR__ . '/runtime.php';
file_put_contents($path, '<?php return true;');
$code = $argc > 0 ? 'echo opcache_is_script_cached($path) ? "E" : "x"; $result = include $path; if ($result) { echo "R"; }' : '';
echo opcache_compile_file($path) ? 'N' : 'x';
echo opcache_is_script_cached($path) ? 'M' : 'x';
eval($code);
"#);
    assert_eq!(fixture.run(&["--strict-bools", "--ini", "opcache.enable_cli=1", "--ini", "opcache.file_update_protection=0"]), "NMER");
}

/// The real native runtime refuses a conflicting host profile before pinning its context.
#[test]
fn test_strict_runtime_semantic_profile_handshake() {
    let fixture = StrictFixture::new(r#"<?php
extern "elephc_magician" {
    function __elephc_eval_context_new(): int;
    function __elephc_eval_set_semantic_profile(int $context, int $version, int $bits): int;
    function __elephc_eval_context_free(int $context): int;
}
$context = __elephc_eval_context_new();
echo __elephc_eval_set_semantic_profile($context, 1, 0), ':';
echo __elephc_eval_set_semantic_profile($context, 1, 32), ':';
__elephc_eval_context_free($context);
$code = $argc > 0 ? 'echo "E";' : '';
eval($code);
"#);
    assert_eq!(fixture.run(&["--strict-bools"]), "5:0:E");
}


/// Checked abs throws before returning an invalid magnitude and releases every temporary argument.
#[test]
fn test_strict_abs_staged_native_calls_and_numeric_kinds() {
    let source = r#"<?php
function strict_abs_value(mixed $value): mixed { return abs($value); }
function strict_abs_integer(int $value): mixed { return abs($value); }
$minimum = $argc > 0 ? PHP_INT_MIN : 0;
try { strict_abs_value($minimum); } catch (ArithmeticError $error) { echo 'M'; }
try { strict_abs_integer($minimum); } catch (ArithmeticError $error) { echo 'I'; }
$callback = abs(...);
try { $callback($minimum); } catch (ArithmeticError $error) { echo 'F'; }
try { call_user_func('abs', $minimum); } catch (ArithmeticError $error) { echo 'C'; }
try { call_user_func_array('abs', [$minimum]); } catch (ArithmeticError $error) { echo 'A'; }
$name = $argc > 0 ? 'abs' : 'floor';
try { $name($minimum); } catch (ArithmeticError $error) { echo 'D'; }
try { strict_abs_value('-922337203685477580' . (string) (7 + $argc)); } catch (ArithmeticError $error) { echo 'S'; } finally { echo 'X'; }
try { strict_abs_value('12x'); } catch (TypeError $error) { echo 'T'; }
echo is_int(strict_abs_value(-42)) ? 'N' : 'x';
echo is_float(strict_abs_value(-1.25)) ? 'F' : 'x';
echo is_float(strict_abs_value('-9223372036854775809')) ? 'W' : 'x';
echo is_float(strict_abs_value('-1.25')) ? 'S' : 'x';
echo is_float(strict_abs_value(-0.0)) ? 'Z' : 'x';
echo strict_abs_value(true), strict_abs_value(null);
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "MIFCADSXTNFWSZ10", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}


/// Direct and descriptor powers retain integer overflow checks and independent numeric owners.
#[test]
fn test_strict_pow_staged_builtin_calls_preserve_integer_kinds() {
    let source = r#"<?php
function strict_builtin_power(mixed $base, mixed $exponent): mixed { return pow($base, $exponent); }
function strict_typed_power(int $base, int $exponent): mixed { return pow($base, $exponent); }
$maximum = $argc > 0 ? PHP_INT_MAX : 0;
try { strict_builtin_power($maximum, 2); } catch (ArithmeticError $error) { echo 'M'; }
try { strict_typed_power($maximum, 2); } catch (ArithmeticError $error) { echo 'I'; }
$callback = pow(...);
try { $callback($maximum, 2); } catch (ArithmeticError $error) { echo 'F'; }
try { call_user_func('pow', $maximum, 2); } catch (ArithmeticError $error) { echo 'C'; }
try { call_user_func_array('pow', [$maximum, 2]); } catch (ArithmeticError $error) { echo 'A'; }
$name = $argc > 0 ? 'pow' : 'fdiv';
try { $name($maximum, 2); } catch (ArithmeticError $error) { echo 'D'; }
echo ':', strict_builtin_power(-2, 63), ':', strict_typed_power(-2, 63);
echo ':', strict_builtin_power(2, -1), ':', strict_typed_power(2, -1);
echo ':', is_int($callback(2, 3)), ':', is_int(strict_builtin_power(2, 3));
echo ':', is_float(strict_builtin_power(2.0, 3));
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "MIFCAD:-9223372036854775808:-9223372036854775808:0.5:0.5:1:1:1", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Numeric strings retain exact integer powers, callback snapshots and exceptional cleanup.
#[test]
fn test_strict_pow_staged_numeric_strings_and_warning_ownership() {
    let source = r#"<?php
function strict_string_power(mixed $base, mixed $exponent): mixed { return pow($base, $exponent); }
function strict_string_operator(mixed $base, mixed $exponent): mixed { return $base ** $exponent; }
function strict_string_float_power(string $base, float $exponent): float { return pow($base, $exponent); }
function strict_discard_prefix_power(mixed $base): void {
    try { $base ** 2.0; } catch (RuntimeException $error) { echo 'Q'; }
}
$maximum = $argc > 0 ? '9223372036854775807' : '0';
try { strict_string_power($maximum, 2); } catch (ArithmeticError $error) { echo 'M'; }
try { strict_string_operator($maximum, 2); } catch (ArithmeticError $error) { echo 'S'; }
$callback = pow(...);
try { $callback($maximum, 2); } catch (ArithmeticError $error) { echo 'F'; }
try { call_user_func('pow', $maximum, 2); } catch (ArithmeticError $error) { echo 'C'; }
try { call_user_func_array('pow', [$maximum, 2]); } catch (ArithmeticError $error) { echo 'A'; }
$name = $argc > 0 ? 'pow' : 'fdiv';
try { $name($maximum, 2); } catch (ArithmeticError $error) { echo 'D'; }
echo ':', is_int(strict_string_power('2', '3'));
echo ':', is_int(strict_string_power(true, 2)), ':', is_int(strict_string_power(null, 2));
echo ':', is_int(strict_string_power(false, 0));
echo ':', is_float(strict_string_power('2.0', 3)), ':', is_float(strict_string_power('9223372036854775808', 2));
echo ':', strict_string_power(2, '-1'), ':', is_float(strict_string_float_power('2', 3.0));
$prefix = $argc > 0 ? '2x' : '0'; $exponent = 3;
set_error_handler(function (int $severity, string $message) use (&$exponent): bool { $exponent = 63; echo 'W'; return true; });
echo ':', strict_string_power($prefix, $exponent), ':', $exponent;
restore_error_handler();
set_error_handler(function (int $severity, string $message): bool { throw new RuntimeException('numeric prefix'); });
try { strict_string_power($prefix . '', 3); } catch (RuntimeException $error) { echo ':E'; } finally { echo 'Z'; }
strict_discard_prefix_power($prefix);
restore_error_handler();
try { strict_string_power('invalid', 2); } catch (TypeError $error) { echo 'T'; }
try { strict_string_operator('invalid', 2.0); } catch (TypeError $error) { echo 'O'; }
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for optimize in [false, true] {
        let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
        assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
        assert_eq!(output.stdout, "MSFCAD:1:1:1:1:1:1:0.5:1:W8:63:EZQTO", "opt={optimize}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
    }
}

/// Integer division keeps catchable overflow and zero guards on all callable routes.
#[test]
fn test_strict_numeric_builtins_staged_intdiv_boundaries() {
    let source = r#"<?php
function strict_integer_division(int $left, int $right): int { return intdiv($left, $right); }
$minimum = $argc > 0 ? PHP_INT_MIN : 0;
try { strict_integer_division($minimum, -1); } catch (ArithmeticError $error) { echo 'I'; }
$divide = intdiv(...);
try { $divide($minimum, -1); } catch (ArithmeticError $error) { echo 'F'; }
try { call_user_func('intdiv', $minimum, -1); } catch (ArithmeticError $error) { echo 'C'; }
try { call_user_func_array('intdiv', [$minimum, -1]); } catch (ArithmeticError $error) { echo 'A'; }
$name = $argc > 0 ? 'intdiv' : 'fdiv';
try { $name($minimum, -1); } catch (ArithmeticError $error) { echo 'D'; }
try { strict_integer_division(1, 0); } catch (DivisionByZeroError $error) { echo 'Z'; }
echo ':', strict_integer_division(-7, 2), ':', strict_integer_division($minimum, 1), ':', strict_integer_division(0, -1);
"#;
    let profile = elephc_semantics::SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
    for profile in [elephc_semantics::SemanticProfile::PERMISSIVE, profile] {
        for optimize in [false, true] {
            let output = StrictFixture::new(source).capture_staged_lowering(profile, optimize);
            assert!(output.success, "opt={optimize}: {} {} {}", output.status, output.stdout, output.stderr);
            assert_eq!(output.stdout, "IFCADZ:-3:-9223372036854775808:0", "opt={optimize}");
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "opt={optimize}: {}", output.stderr);
        }
    }
}
