//! Purpose:
//! Integration coverage for call-result/argument alias summaries with EIR
//! optimization enabled and disabled.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::call_result_alias`.
//!
//! Key details:
//! - Fresh method and inlined function results release unrelated inline arrays.
//! - True parameter passthrough remains valid, COW-safe, and heap-clean.

use super::*;

/// Compiles and runs the alias-summary fixture in one EIR optimizer mode.
fn run_call_alias_fixture(ir_opt: bool) -> (String, String) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_call_result_alias_runtime_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("failed to create call-result alias fixture directory");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
final class OptimizedAliasScanner {
    public function scan(array $delimiters): array {
        $tokens = [];
        $count = count($delimiters);
        for ($i = 0; $i < $count; $i++) {
            $tokens[] = $delimiters[$i];
        }
        return $tokens;
    }
}
final class OptimizedAliasChooser {
    public function choose(array $discard, array $keep): array {
        return $keep;
    }
}
function optimizedAliasCopy(array $source): array {
    $copy = [];
    $copy[] = $source[0];
    return $copy;
}
$scanner = new OptimizedAliasScanner();
$chooser = new OptimizedAliasChooser();
for ($i = 0; $i < 40; $i++) {
    $tokens = $scanner->scan(['//', '#']);
    $chosen = $chooser->choose(['drop'], ['keep']);
    $functionResult = optimizedAliasCopy(['function']);
}
$tokenCopy = $tokens;
$tokenCopy[] = 'tail';
$chosenCopy = $chosen;
$chosenCopy[] = 'tail';
echo $tokens[0] . ':' . count($tokens) . ':' . count($tokenCopy);
echo '|' . $chosen[0] . ':' . count($chosen) . ':' . count($chosenCopy);
echo '|' . $functionResult[0];
"#,
    )
    .expect("failed to write call-result alias PHP fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile call-result alias fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run call-result alias fixture");
    assert!(
        output.status.success(),
        "fixture execution failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    (stdout, stderr)
}

/// Verifies precise alias cleanup is behaviorally identical in both optimizer modes.
#[test]
fn test_call_result_alias_cleanup_with_optimizer_on_and_off() {
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_call_alias_fixture(ir_opt);
        assert_eq!(stdout, "//:2:3|keep:1:2|function");
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}

/// Compiles and runs `source` with the heap debugger in one EIR optimizer mode.
fn run_passthrough_fixture(source: &str, ir_opt: bool) -> (String, String) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_call_result_passthrough_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("failed to create passthrough fixture directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write passthrough PHP fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile passthrough fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run passthrough fixture");
    assert!(
        output.status.success(),
        "fixture execution failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    (stdout, stderr)
}

/// A wrapper that returns another function's result (`return id($s)`) hands back the storage of
/// the string it was given, exactly as `id` does. Its summary used to say "unknown", so its caller
/// treated the result as an owner and released it -- freeing the argument the caller still held,
/// which the next allocation then overwrote (`$x` read as `zzzzzz`). The summary now traces the
/// call through `id`. The loop checks the traced answer neither leaks nor double-frees, including
/// a temporary argument, a second-parameter pick, a cast argument, and an untyped wrapper whose
/// result is NOT its parameter's storage (it unboxes a copy).
#[test]
fn test_call_result_passthrough_wrapper_keeps_the_callers_string() {
    let source = r#"<?php
function id(string $s) { return $s; }
function pass(string $s) { return id($s); }
function pass2(string $s) { return pass($s); }
function cast(string $s) { return id((string)$s); }
function pick(string $a, string $b) { return $b; }
function second(string $x, string $y) { return pick($x, $y); }
function untyped($s) { return id($s); }
function fresh(string $s) { return strtoupper(id($s)); }
$x = str_repeat("ab", 3);
$r = pass($x);
$junk = str_repeat("z", 6);
echo $x, " ", $r, " ", $junk, "\n";
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $k = str_repeat("x", 20);
    $a = pass($k);
    $b = pass2($k);
    $c = cast($k);
    $d = second($k, $k . "y");
    $e = untyped($k);
    $f = pass(strtoupper($k));
    $g = fresh($k);
    $t += strlen($a) + strlen($b) + strlen($c) + strlen($d) + strlen($e) + strlen($f) + strlen($g);
}
$junk2 = str_repeat("q", 20);
echo $t, " ", $k, " ", $a, " ", $d, " ", $f, " ", $junk2, "\n";
"#;
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_passthrough_fixture(source, ir_opt);
        assert_eq!(
            stdout,
            "ababab ababab zzzzzz\n42300 xxxxxxxxxxxxxxxxxxxx xxxxxxxxxxxxxxxxxxxx xxxxxxxxxxxxxxxxxxxxy XXXXXXXXXXXXXXXXXXXX qqqqqqqqqqqqqqqqqqqq\n",
            "ir_opt={ir_opt}"
        );
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}

/// The smallest form of the wrapper defect: one call, then an allocation that reuses a freed
/// block. `pass($x)` returns `$x`'s own string, so releasing the result freed `$x`.
#[test]
fn test_call_result_passthrough_wrapper_single_call() {
    let source = r#"<?php
function id(string $s) { return $s; }
function pass(string $s) { return id($s); }
$x = str_repeat("ab", 3);
$r = pass($x);
$junk = str_repeat("z", 6);
echo $x, " ", $r, " ", $junk, "\n";
"#;
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_passthrough_fixture(source, ir_opt);
        assert_eq!(stdout, "ababab ababab zzzzzz\n", "ir_opt={ir_opt}");
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}

/// A by-value builtin call on a string parameter (`strlen($s)`) cannot rebind it, but the summary
/// treated every direct variable argument as possibly by-reference and forgot that `$s` is still
/// the parameter. `return $s` then read as "unknown", the caller released the result as its own,
/// and that freed the argument it still held (`$x` read as the next allocation's bytes). A fresh
/// result on another path is still released, which the heap summary pins; the summary unit tests
/// pin that a by-reference builtin argument still invalidates.
#[test]
fn test_call_result_passthrough_after_a_by_value_builtin_keeps_the_callers_string() {
    let source = r#"<?php
function noisy(string $s): string { $t = str_repeat("x", 3) . strlen($s); return $s; }
function either(string $s): string { if (strlen($s) > 3) { return $s . "x"; } return $s; }
function plain(): string {
    $x = str_repeat("a", 5) . "bcdef";
    $y = noisy($x);
    $j1 = str_repeat("Z", 10);
    $j2 = "W" . str_repeat("V", 9);
    return $x . "|" . $y . "|" . $j1 . $j2;
}
echo plain(), "\n";
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $k = str_repeat("k", 20) . $i;
    $t += strlen(noisy($k)) + strlen(either($k));
}
$junk = str_repeat("q", 23);
echo $t, " ", $k, " ", $junk, "\n";
"#;
    let expected = "aaaaabcdef|aaaaabcdef|ZZZZZZZZZZWVVVVVVVVV\n\
                    13880 kkkkkkkkkkkkkkkkkkkk299 qqqqqqqqqqqqqqqqqqqqqqq\n";
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_passthrough_fixture(source, ir_opt);
        assert_eq!(stdout, expected, "ir_opt={ir_opt}");
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}

/// A function that returns a copy on one path and its string parameter itself on another stays
/// summarized as a passthrough (the summary unit tests pin that), so the caller never takes the
/// borrowed path's result for its own. These outputs pin both paths, through a parameter and a
/// widened parameter, with the results consumed at once and kept past a reusing allocation; the
/// copied path still keeps its copy, as it does on main.
#[test]
fn test_call_result_copy_on_one_path_keeps_the_borrow_on_the_other() {
    let source = r#"<?php
function choose(string $s, bool $copy): string {
    if ($copy) {
        $t = $s;
        return $t;
    }
    return $s;
}
function widen(string $s, bool $copy): string {
    if ($copy) {
        $s = strlen($s) > 4 ? $s : "x";
    }
    return $s;
}
function show(string $x, bool $copy): void {
    $n = strlen(choose($x, $copy)) + strlen(widen($x, $copy));
    $r = choose($x, $copy);
    $w = widen($x, $copy);
    $junk = str_repeat("z", 8);
    echo $n, "|", $x, "|", $r, "|", $w, "|", $junk, "\n";
}
show(str_repeat("a", 8), $argc > 1);
show(str_repeat("b", 8), $argc < 2);
"#;
    let expected = "16|aaaaaaaa|aaaaaaaa|aaaaaaaa|zzzzzzzz\n16|bbbbbbbb|bbbbbbbb|bbbbbbbb|zzzzzzzz\n";
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_passthrough_fixture(source, ir_opt);
        assert_eq!(stdout, expected, "ir_opt={ir_opt}");
        assert!(
            !stderr.contains("bad refcount"),
            "ir_opt={ir_opt}: the caller freed its own argument: {stderr}"
        );
    }
}

/// A named by-value builtin argument (`strlen(string: $s)`) is matched to its declared parameter
/// like a positional one, so `return $s` stays a passthrough: read as "unknown", the caller
/// released the result and freed the argument it still held. A local the parameter was copied
/// into holds a box with its own copy, so returning it is NOT the parameter's storage, and the
/// caller must release it: taken for a passthrough, it leaked one copy per call.
#[test]
fn test_call_result_named_builtin_argument_and_copied_local_are_told_apart() {
    let source = r#"<?php
function named(string $s): string { $n = strlen(string: $s); return $s; }
function copied(string $s): string { $n = strlen($s); $copy = $s; return $copy; }
function plain(): string {
    $x = str_repeat("a", 5) . "bcdef";
    $y = named($x);
    $j1 = str_repeat("Z", 10);
    $j2 = "W" . str_repeat("V", 9);
    return $x . "|" . $y . "|" . $j1 . $j2;
}
echo plain(), "\n";
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $k = str_repeat("k", 20) . $i;
    $t += strlen(named($k)) + strlen(copied($k));
}
$junk = str_repeat("q", 23);
echo $t, " ", $k, " ", $junk, "\n";
"#;
    let expected = "aaaaabcdef|aaaaabcdef|ZZZZZZZZZZWVVVVVVVVV\n\
                    13580 kkkkkkkkkkkkkkkkkkkk299 qqqqqqqqqqqqqqqqqqqqqqq\n";
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_passthrough_fixture(source, ir_opt);
        assert_eq!(stdout, expected, "ir_opt={ir_opt}");
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}
