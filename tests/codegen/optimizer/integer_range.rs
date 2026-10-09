//! Purpose:
//! End-to-end coverage for EIR integer range and induction-variable analysis.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::integer_range`.
//!
//! Key details:
//! - Textual EIR proves bounded checked operations become scalar while unproven overflow stays.
//! - Native execution compares optimizer-on and optimizer-off PHP behavior.
//! - Assembly emission exercises both modes on every supported target.

use super::*;

#[path = "integer_range/configuration.rs"]
mod configuration;

/// Emits only the main function's textual EIR for one PHP program.
fn main_ir(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_integer_range_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write integer range EIR fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let output = command.arg(&php_path).output().expect("emit integer range EIR");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("  function main()")
        .nth(1)
        .expect("main function")
        .to_string();
    fs::remove_dir_all(dir).expect("remove integer range EIR fixture");
    main
}

/// Compiles and executes a fixture with the selected optimizer mode.
fn run_variant(source: &str, optimized: bool) -> (String, String) {
    run_variant_with_options(source, optimized, &[])
}

/// Compiles and executes a fixture with explicit configuration options.
fn run_variant_with_options(source: &str, optimized: bool, options: &[&str]) -> (String, String) {
    let dir = make_cli_test_dir("elephc_integer_range_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write integer range runtime fixture");
    let mut command = elephc_cli_command(&dir);
    command.args(options);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compile = command.arg(&php_path).output().expect("compile integer range fixture");
    assert!(
        compile.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = run_binary(&dir.join("main"), &dir);
    assert!(
        run.status.success(),
        "integer range fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8(run.stderr).expect("stderr is UTF-8");
    fs::remove_dir_all(dir).expect("remove integer range runtime fixture");
    (stdout, stderr)
}

/// Emits target-specific assembly with the selected optimizer mode.
fn target_assembly(source: &str, target: &str, optimized: bool) -> String {
    target_assembly_with_options(source, target, optimized, &[])
}

/// Emits target assembly while applying configuration-specific representation options.
fn target_assembly_with_options(source: &str, target: &str, optimized: bool, options: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_integer_range_target");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write integer range target fixture");
    let mut command = elephc_cli_command(&dir);
    command.args(options);
    command.arg("--emit-asm").arg("--target").arg(target);
    if target.starts_with("ios-") {
        command.arg("--emit").arg("staticlib");
    }
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let output = command.arg(&php_path).output().expect("emit target assembly");
    assert!(
        output.status.success(),
        "emit-asm for {target} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = fs::read_to_string(php_path.with_extension("s"))
        .expect("read integer range target assembly");
    fs::remove_dir_all(dir).expect("remove integer range target fixture");
    assembly
}

/// The example's bounded induction operations become unchecked scalar EIR.
#[test]
fn test_integer_range_rewrites_bounded_example() {
    let source = include_str!("../../../examples/integer-range/main.php");
    let plain = main_ir(source, false);
    let optimized = main_ir(source, true);
    let body = optimized
        .split("for.body:")
        .nth(1)
        .expect("optimized loop body")
        .split("for.update:")
        .next()
        .expect("body before update");
    let update = optimized
        .split("for.update:")
        .nth(1)
        .expect("optimized loop update")
        .split("for.exit:")
        .next()
        .expect("update before exit");

    assert!(plain.contains("= ichecked_mul "), "{plain}");
    assert!(plain.contains("= ichecked_sub "), "{plain}");
    assert!(body.contains("= imul "), "{optimized}");
    assert!(body.contains("= isub "), "{optimized}");
    assert!(body.contains("= iadd "), "{optimized}");
    assert!(!body.contains("= ichecked_"), "{optimized}");
    assert_eq!(body.matches("= iadd ").count(), 3, "counter update joins the two checksum additions: {optimized}");
    assert!(body.contains("br bb1("), "body returns directly to the header: {optimized}");
    assert!(update.trim().starts_with("unreachable"), "{optimized}");
    assert!(!update.contains("= ichecked_add_to_int "), "{optimized}");
    assert!(optimized.contains("origin: integer_range"), "{optimized}");

    let plain_run = run_variant(source, false);
    let optimized_run = run_variant(source, true);
    assert_eq!(plain_run, optimized_run);
    assert_eq!(optimized_run.0, "153");
}

/// A bounded expression observed directly as Mixed can still narrow to scalar output.
#[test]
fn test_integer_range_narrows_boxed_checked_output() {
    let source = "<?php echo ($argc & 255) + 1;";
    let plain = main_ir(source, false);
    let optimized = main_ir(source, true);
    assert!(plain.contains("= ichecked_add "), "{plain}");
    assert!(!optimized.contains("= ichecked_add "), "{optimized}");
    assert!(optimized.contains("= iadd "), "{optimized}");
    assert!(optimized.contains("origin: integer_range"), "{optimized}");
    assert_eq!(run_variant(source, false), run_variant(source, true));
}

/// Typed static stores retain the conversion performed on the original Mixed input.
#[test]
fn test_integer_range_preserves_typed_static_store_conversions() {
    let source = r#"<?php
class RangeStore {
    public static float $number = 0.0;
    public static bool $flag = false;
    public static string $text = "";
}
RangeStore::$number = ($argc & 255) + 2;
RangeStore::$flag = ($argc & 255) + 2;
RangeStore::$text = ($argc & 255) + 2;
var_dump(RangeStore::$number, RangeStore::$flag, RangeStore::$text);
$property = new ReflectionProperty(RangeStore::class, "number");
$property->setValue(null, ($argc & 255) + 3);
var_dump(RangeStore::$number);
"#;
    let plain = run_variant(source, false);
    assert_eq!(plain.0, "float(3)\nbool(true)\nstring(1) \"3\"\nfloat(4)\n");
    assert_eq!(plain, run_variant(source, true));
}

/// A PHP array cast requires a Mixed cell even when its integer payload is proven bounded.
#[test]
fn test_integer_range_preserves_array_cast_box() {
    for mask in [255, 0] {
        let source = format!("<?php var_dump((array)(($argc & {mask}) + 2));");
        let plain = run_variant(&source, false);
        assert_eq!(plain.0, format!("array(1) {{\n  [0]=>\n  int({})\n}}\n", if mask == 0 { 2 } else { 3 }));
        assert_eq!(plain, run_variant(&source, true));
    }
}

/// Narrowing must not change loose equality by avoiding a lossy boxed-to-float comparison.
#[test]
fn test_integer_range_large_integer_equality_is_exact_in_both_modes() {
    let mut source = String::from("<?php\n");
    let mut expected = String::new();
    for base in [9_007_199_254_740_992i64, i64::MAX - 256, i64::MIN] {
        for rhs in [base, base + 1, base + 2] {
            for op in ["==", "!="] {
                let result = (base + 1 == rhs) == (op == "==");
                for reversed in [false, true] {
                    let base_text = if base == i64::MIN { "PHP_INT_MIN".to_string() }
                        else { base.to_string() };
                    let rhs_text = if rhs == i64::MIN { "PHP_INT_MIN".to_string() }
                        else { rhs.to_string() };
                    let value = format!("(($argc & 255) + ({base_text}))");
                    let comparison = if reversed { format!("({rhs_text}) {op} {value}") }
                        else { format!("{value} {op} ({rhs_text})") };
                    source.push_str(&format!("var_dump({comparison});\n"));
                    expected.push_str(if result { "bool(true)\n" } else { "bool(false)\n" });
                }
            }
        }
    }
    for optimized in [false, true] {
        let output = run_variant(&source, optimized);
        assert_eq!(output.0, expected, "optimized={optimized}");
        assert!(output.1.is_empty(), "{}", output.1);
    }
}

/// Narrowing keeps resource-to-integer loose equality on the full PHP comparison table.
#[test]
fn test_integer_range_resource_equality_matches_in_both_modes() {
    let source = r#"<?php
function hide_resource(mixed $value): mixed { return $value; }
$resource = fopen("php://memory", "r");
$id = (int)$resource;
$float_id = (float)$id;
$mixed = hide_resource($resource);
var_dump((($id & 254) + ($id & 1)) == $mixed);
var_dump($mixed == (($id & 254) + ($id & 1)));
var_dump((($id & 254) + ($id & 1)) != $mixed);
var_dump($mixed != (($id & 254) + ($id & 1)));
var_dump($float_id == $mixed);
var_dump($mixed == $float_id);
var_dump($float_id != $mixed);
var_dump($mixed != $float_id);
"#;
    let expected = concat!(
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n",
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n",
    );
    for optimized in [false, true] {
        let output = run_variant(source, optimized);
        assert_eq!(output.0, expected, "optimized={optimized}");
        assert!(output.1.is_empty(), "{}", output.1);
    }
}

/// Runtime ordering keeps PHP bool/null coercions after range analysis and constant folding.
#[test]
fn test_integer_range_boxed_ordering_preserves_php_coercions() {
    for mask in [255, 0] {
        let source = format!("<?php
var_dump((($argc & {mask}) + 2) <=> true);
var_dump((($argc & {mask}) - 2) <=> null);
var_dump(true <=> (($argc & {mask}) + 2));
var_dump(null <=> (($argc & {mask}) - 2));
var_dump((($argc & {mask}) + 2) < true);
var_dump((($argc & {mask}) - 2) < null);
var_dump((($argc & {mask}) - 2) > null);
var_dump((($argc & {mask}) + 2) >= true);
");
        for optimized in [false, true] {
            assert_eq!(run_variant(&source, optimized).0,
                "int(0)\nint(1)\nint(0)\nint(-1)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n");
        }
    }
}

/// Scalar and boxed observations agree for zero, negative, large and overflowing results.
#[test]
fn test_integer_range_observer_matrix_matches_in_both_modes() {
    let mut source = String::from("<?php for ($i = -1; $i <= 1; $i++) {\n");
    for expression in ["(($argc & 255) + $i)",
        "(($argc & 255) + 9007199254740992)", "(PHP_INT_MAX + ($argc & 255))"] {
        for cast in ["int", "float", "string", "bool"] {
            source.push_str(&format!("var_dump(({cast}){expression});\n"));
        }
        for predicate in ["is_int", "is_float", "is_bool", "is_null", "empty"] {
            source.push_str(&format!("var_dump({predicate}({expression}));\n"));
        }
        for rhs in ["0", "1", "1.0", "true", "false", "null", "\"0\"", "\"1\"", "[]"] {
            for op in ["===", "!==", "==", "!=", "<", "<=", ">", ">=", "<=>"] {
                if matches!(rhs, "\"0\"" | "\"1\"" | "[]")
                    && !matches!(op, "===" | "!==" | "==" | "!=") { continue; }
                source.push_str(&format!("var_dump({expression} {op} {rhs});\n"));
            }
        }
        source.push_str(&format!("echo {expression}; print_r({expression});\n"));
    }
    source.push_str("}\n");
    let plain = run_variant(&source, false);
    assert!(plain.1.is_empty(), "{}", plain.1);
    assert_eq!(plain, run_variant(&source, true));
}

/// Boolean normalization must precede proofs about arithmetic using the cast result.
#[test]
fn test_integer_range_boolean_cast_preserves_overflow() {
    let source = r#"<?php
for ($i = 1; $i < 3; $i++) {
    var_dump(is_float(PHP_INT_MIN + (int)((int)(bool)($i << 4) - 2)));
}
"#;
    let plain = run_variant(source, false);
    let optimized = run_variant(source, true);
    assert_eq!(plain.0, "bool(true)\nbool(true)\n");
    assert_eq!(plain, optimized);
}

/// Composed numeric expressions exercise overflow, casts, masks and shifts under loop bounds.
#[test]
fn test_integer_range_generated_expression_matrix() {
    let inputs = ["($argc & 255)", "($argc << 62)", "(int)(bool)($argc << 4)"];
    let operands = ["$i", "($i << 60)", "PHP_INT_MAX", "PHP_INT_MIN"];
    let mut source = String::from("<?php for ($i = -2; $i < 3; $i++) {\n");
    for input in inputs {
        for operand in operands {
            for op in ["+", "-", "*"] {
                source.push_str(&format!("var_dump({input} {op} {operand});\n"));
            }
        }
    }
    source.push_str("}\n");
    let plain = run_variant(&source, false);
    assert_eq!(plain.0.lines().count(), 180);
    assert!(plain.1.is_empty(), "{}", plain.1);
    assert_eq!(plain, run_variant(&source, true));
}

/// The pass before IntegerRange must preserve truthiness and boolean result metadata too.
#[test]
fn test_integer_range_checked_boolean_cast_keeps_type_and_truthiness() {
    let source = r#"<?php
var_dump((bool)($argc + 1));
var_dump((bool)($argc - 1));
var_dump((bool)($argc * -3));
var_dump((bool)(PHP_INT_MAX + $argc));
var_dump((bool)(($argc + 1) * 2));
"#;
    let plain = run_variant(source, false);
    assert_eq!(plain.0, "bool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n");
    assert_eq!(plain, run_variant(source, true));
}

/// Unbounded add, subtract, and multiply retain PHP overflow-to-float behavior.
#[test]
fn test_integer_range_keeps_unproven_overflow_checked() {
    let source = r#"<?php
$one = $argc;
echo PHP_INT_MAX + $one, "|";
echo PHP_INT_MIN - $one, "|";
echo PHP_INT_MAX * $one;
"#;
    let optimized = main_ir(source, true);
    assert!(optimized.contains("= ichecked_add "), "{optimized}");
    assert!(optimized.contains("= ichecked_sub "), "{optimized}");
    assert!(optimized.contains("= ichecked_mul "), "{optimized}");
    assert_eq!(run_variant(source, false), run_variant(source, true));
}

/// A widening shift count must not leave a stale no-overflow proof in its successor.
#[test]
fn test_integer_range_widening_shift_preserves_float_overflow() {
    let source = r#"<?php
$shift = $argc & 0;
while ($shift < 63) {
    $value = 1 << $shift;
    if ($argc > 0) {
        $product = $value * 4;
        if ($shift >= 60) { echo is_float($product) ? "float|" : "int|"; }
    }
    $shift = ($shift + 1) & 63;
}
"#;
    let plain = run_variant(source, false);
    let optimized = run_variant(source, true);
    assert_eq!(plain, optimized);
    assert_eq!(optimized.0, "int|float|float|");
    assert!(optimized.1.is_empty(), "{}", optimized.1);
}

/// Every supported target emits both modes and removes only the bounded multiply.
#[test]
fn test_integer_range_all_supported_targets_compile_both_modes() {
    let source = r#"<?php
#[Export]
function bounded_checksum(): int {
    $limit = 9;
    $checksum = 0;
    for ($i = 0; $i < $limit; $i++) {
        $checksum = ($checksum + (($i * 3) & 65535)) & 65535;
    }
    return $checksum;
}
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let plain = target_assembly(source, target, false);
        let optimized = target_assembly(source, target, true);
        assert!(
            plain.contains("op=ichecked_mul"),
            "unoptimized {target} assembly did not retain checked multiply"
        );
        assert!(
            optimized.contains("op=imul"),
            "optimized {target} assembly did not contain scalar multiply"
        );
        assert!(
            !optimized.contains("op=ichecked_mul"),
            "optimized {target} assembly retained bounded checked multiply"
        );
    }
}

/// Both modes retain an unproven shifted multiply on all supported emitters.
#[test]
fn test_integer_range_all_supported_targets_keep_shift_overflow_checked() {
    let source = r#"<?php
#[Export]
function range_overflow_probe(int $input): bool {
    return is_float((1 << ($input & 63)) * 4);
}
"#;
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let assembly = target_assembly(source, target, optimized);
            assert!(assembly.contains("op=ichecked_mul"), "{target}, optimized={optimized}");
        }
    }
}

/// Exact integer-tag comparison remains available on both architectures before narrowing.
#[test]
fn test_integer_range_all_supported_targets_keep_exact_integer_equality() {
    let source = r#"<?php
#[Export]
function exact_equality_probe(int $input): bool {
    return (($input & 255) + 9007199254740992) == 9007199254740992;
}
#[Export]
function boxed_relational_probe(int $input): bool {
    return (($input & 255) + 2) < true;
}
#[Export]
function boxed_spaceship_probe(int $input): int {
    return (($input & 255) + 2) <=> true;
}
"#;
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let assembly = target_assembly(source, target, optimized);
            assert_eq!(assembly.contains("mixed_numeric_integer"), !optimized,
                "{target}, optimized={optimized}");
            if !optimized {
                let compare = if target == "linux-x86_64" { "cmp r10, rax" } else { "cmp x10, x0" };
                assert!(assembly.contains(compare), "{target}");
            }
            assert!(assembly.contains("op=php_rel_cmp"), "{target}");
            assert!(assembly.contains("op=spaceship"), "{target}");
            assert!(assembly.contains("op=ichecked_add"), "{target}");
        }
    }
}

/// Resource-to-integer loose equality uses the display-id path on every target.
#[test]
fn test_integer_range_all_supported_targets_keep_resource_equality() {
    let source = r#"<?php
function resource_equality_probe(int $id, mixed $resource): bool {
    return (($id & 254) + ($id & 1)) == $resource;
}
function resource_float_equality_probe(float $id, mixed $resource): bool {
    return $id == $resource;
}
$resource = fopen("php://memory", "r");
$id = (int)$resource;
var_dump(resource_equality_probe($id, $resource));
var_dump(resource_float_equality_probe((float)$id, $resource));
"#;
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let assembly = target_assembly(source, target, optimized);
            assert!(assembly.contains("mixed_numeric_resource"),
                "{target}, optimized={optimized}");
            assert_eq!(assembly.contains("mixed_numeric_integer"), optimized,
                "{target}, optimized={optimized}");
        }
    }
}

/// Boxed array casts and typed property conversions compile unchanged on all targets.
#[test]
fn test_integer_range_all_supported_targets_preserve_boxed_consumers() {
    let source = r#"<?php
class RangeTargetStore {
    public static float $number = 0.0;
    public static bool $flag = false;
    public static string $text = "";
}
#[Export]
function boxed_consumer_probe(int $input): void {
    var_dump((array)(($input & 255) + 2));
    var_dump((array)(($input & 0) + 2));
    RangeTargetStore::$number = ($input & 255) + 2;
    RangeTargetStore::$flag = ($input & 255) + 2;
    RangeTargetStore::$text = ($input & 255) + 2;
}
"#;
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let assembly = target_assembly(source, target, optimized);
            assert!(assembly.contains("op=ichecked_add"), "{target}, optimized={optimized}");
            assert!(assembly.contains("op=cast"), "{target}, optimized={optimized}");
            assert!(assembly.contains("op=store_static_property"), "{target}, optimized={optimized}");
        }
    }
}

/// Cast normalization and its remaining overflow check survive every supported emitter.
#[test]
fn test_integer_range_all_supported_targets_preserve_boolean_casts() {
    let source = r#"<?php
#[Export]
function range_boolean_overflow(): bool {
    $overflow = false;
    for ($i = 1; $i < 3; $i++) {
        $overflow = is_float(PHP_INT_MIN + (int)((int)(bool)($i << 4) - 2));
    }
    return $overflow;
}
"#;
    let sinks = r#"<?php
#[Export]
function range_boolean_sink(int $input): bool {
    return (bool)($input + 1);
}
#[Export]
function range_boolean_chain(int $input): bool {
    return (bool)(($input + 1) * 2);
}
"#;
    for target in [
        "macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let assembly = target_assembly(source, target, optimized);
            assert!(assembly.contains("op=ichecked_add"), "{target}, optimized={optimized}");
            assert!(assembly.contains("op=cast"), "{target}, optimized={optimized}");
            let assembly = target_assembly(sinks, target, optimized);
            assert!(assembly.matches("op=cast").count() >= 2, "{target}, optimized={optimized}");
            assert!(!assembly.contains("op=ichecked_numeric_chain_to_int"), "{target}");
        }
    }
}
