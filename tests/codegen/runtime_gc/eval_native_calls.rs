//! Purpose:
//! Checks argument staging and returned-value ownership when eval invokes native functions.
//!
//! Called from:
//! - The focused runtime-GC codegen test module.
//!
//! Key details:
//! - Repetition distinguishes per-call leaks from allocations retained for an eval entry.
//! - Native functions and eval callables share descriptor-compatible argument arrays.

use crate::support::*;

/// Releases main-scope process argument boxes when eval reuses the local scope as globals.
#[test]
fn test_eval_noop_scope_sync_releases_main_process_arguments() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$source = $argc > 0 ? '' : 'echo "unreachable";';
eval($source);
echo $argc, ":", count($argv);
"#,
    );
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "1:1");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// Releases the previous Mixed owner when eval replaces and then unsets a caller local.
#[test]
fn test_eval_scope_reassignment_unset_heap_discriminant() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$value = "before";
$source = $argc > 0 ? '$value = "after"; unset($value);' : '';
eval($source);
echo isset($value) ? $value : "unset";
"#,
    );
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "unset");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// Balances unchanged, replaced, and aliased Mixed eval reloads.
#[test]
fn test_eval_scope_reload_balances_mixed_owner_transitions() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$value = "before";
$replace = $argc > 0 ? '$value = "after";' : '';
eval($replace);
echo $value, ":";

$alias = "alias";
$bind = $argc > 0 ? '$value =& $alias;' : '';
eval($bind);
$noop = $argc > 0 ? '' : 'echo "unreachable";';
eval($noop);
echo $value, ":", $alias;
"#,
    );
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "after:alias:alias");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// Gives eval-owned replacements to by-value and by-reference Mixed parameter storage.
#[test]
fn test_eval_scope_reload_balances_mixed_parameter_transitions() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function update_eval_parameters(mixed $parameter, mixed &$reference, string $code): string {
    eval($code);
    return $parameter . ":" . $reference;
}
$caller = "caller";
$reference = $argc > 0 ? "reference" : 0;
echo update_eval_parameters(
    $caller,
    $reference,
    '$parameter = "local"; $reference = "reference-local";'
), ":", $caller, ":", $reference;
"#,
    );
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(
        output.stdout,
        "local:reference-local:caller:reference-local"
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// Borrows native method arguments through normal returns and throws without retaining lookup cells.
#[test]
fn test_mbstring_eval_native_method_argument_ownership() {
    let calls = r#"
echo $owner->identity($text, 11), ":", NativeArgumentOwner::identityStatic($text, 12), "\n";
try { $owner->fail($text, 13); } catch (Throwable $error) { echo $error->getMessage(), "\n"; }
"#.repeat(8);
    let source = format!(r#"<?php
class NativeArgumentOwner {{
    public function identity(string $text, int $phase): string {{ return $text; }}
    public static function identityStatic(string $text, int $phase): string {{ return $text; }}
    public function fail(string $text, int $phase): void {{ throw new Exception($text); }}
}}
$source = $argc > 0 ? '
$text = "retained"; $owner = new NativeArgumentOwner();
{calls}
echo $text, "\n";
' : '';
eval($source);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}retained\n", "retained:retained\nretained\n".repeat(8)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Retains returned strings while consuming literal arguments across ordinary, dynamic, and nullsafe methods.
#[test]
fn test_mbstring_eval_native_method_literal_argument_ownership() {
    let calls = r#"
echo $owner->identity("literal", 11), ":", NativeLiteralOwner::identityStatic("literal", 12), ":";
echo $owner?->identity("literal", 13), ":", $owner->$method("literal", 14), ":";
echo $owner?->$method("literal", 15), ":", $class::$static("literal", 16), "\n";
"#.repeat(8);
    let source = format!(r#"<?php
class NativeLiteralOwner {{
    public function identity(string $text, int $phase): string {{ return $text; }}
    public static function identityStatic(string $text, int $phase): string {{ return $text; }}
}}
$source = $argc > 0 ? '
$owner = new NativeLiteralOwner(); $method = "identity";
$class = "NativeLiteralOwner"; $static = "identityStatic";
{calls}
' : '';
eval($source);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "literal:literal:literal:literal:literal:literal\n".repeat(8));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Keeps caller and staging owners distinct when native methods keep, replace, or throw after replacing Mixed refs.
#[test]
fn test_mbstring_eval_native_method_mixed_reference_ownership() {
    let calls = r#"
$owner->keep($value); NativeMixedOwner::replace($value);
try { $owner->fail($value); } catch (Throwable $error) { echo $error->getMessage(), ":", $value, "\n"; }
"#.repeat(8);
    let source = format!(r#"<?php
class NativeMixedOwner {{
    public function keep(mixed &$value): void {{}}
    public static function replace(mixed &$value): void {{ $value = 41; }}
    public function fail(mixed &$value): void {{ $value = 42; throw new Exception("updated"); }}
}}
$source = $argc > 0 ? '
$value = 30; $owner = new NativeMixedOwner();
{calls}
echo $value, "\n";
' : '';
eval($source);
"#);
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}42\n", "updated:42\n".repeat(8)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Releases native argument conversions, including ledger growth, while preserving returned aliases.
#[test]
fn test_mbstring_eval_native_argument_conversion_ownership() {
    let source = r#"<?php
function native_argument_select(string $a, string $b, string $c, string $d, string $e, string $f): string {
    if (strlen($a) + strlen($b) + strlen($c) + strlen($d) + strlen($e) + strlen($f) != 9) {
        throw new Exception("argument-corruption");
    }
    return $a;
}
function native_argument_identity(mixed $value): mixed { return $value; }
function exercise_native_arguments(string $code): void { eval($code); }
for ($i = 0; $i < 8; $i++) {
    exercise_native_arguments('$value = "held";
        echo native_argument_select($value, "1", "2", "3", "4", "5"), ":";
        $copy = native_argument_identity($value);
        echo $copy, ":", $value, "\n";
        unset($copy); unset($value);');
}
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "held:held:held\n".repeat(8));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Unwinds native string conversion owners before eval catches a thrown native exception.
#[test]
fn test_mbstring_eval_native_argument_conversion_throw_ownership() {
    let source = r#"<?php
function native_argument_throw(string $message): void { throw new Exception($message); }
function exercise_native_arguments(string $code): void { eval($code); }
for ($i = 0; $i < 8; $i++) {
    exercise_native_arguments('try { native_argument_throw("argument-failure"); }
        catch (Exception $e) { echo $e->getMessage(), "\n"; }');
}
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "argument-failure\n".repeat(8));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Releases temporary argument-array indices for direct and first-class native eval calls.
#[test]
fn test_mbstring_eval_native_argument_array_key_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "native_key_result($value); $callback($value);\n".repeat(count);
        let source = format!(r#"<?php
function native_key_result(int $value): int {{ return $value + 1; }}
$source = $argc > 0 ? '
$value = 11; $callback = native_key_result(...);
{calls}
echo $value; unset($callback);
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "11");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "native eval argument staging retained per-call cells");
}

/// Balances native object results, borrowed identity returns, and echo temporaries across eval entries.
#[test]
fn test_mbstring_eval_native_object_return_ownership() {
    let source = r#"<?php
class NativeReturnObject {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destroy:", $this->name, "\n"; }
}
function native_return_fresh(): NativeReturnObject {
    return new NativeReturnObject("native");
}
function native_return_identity(NativeReturnObject $value): NativeReturnObject {
    return $value;
}
function exercise_native_returns(string $code): void { eval($code); }
for ($i = 0; $i < 8; $i++) {
    exercise_native_returns('$original = native_return_fresh();
        $copy = native_return_identity($original);
        echo "copy:", $copy->name, "\n"; unset($copy);
        $callback = native_return_identity(...);
        $copy = $callback($original);
        echo "callback:", $copy->name, "\n"; unset($copy); unset($callback);
        echo "alive:", $original->name, "\n"; unset($original);');
}
echo "done\n";
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}done\n",
        "copy:native\ncallback:native\nalive:native\ndestroy:native\n".repeat(8)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Releases output literals and converted Stringable temporaries without consuming a borrowed caller object.
#[test]
fn test_mbstring_eval_echo_print_temporary_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = r#"
echo "echo:", $text, "\n";
print "print:"; print $held; print "\n";
echo new OutputObject("temporary"), "\n";
"#.repeat(count);
        let source = format!(r#"<?php
class OutputObject {{
    public function __construct(public string $name) {{}}
    public function __toString(): string {{ return $this->name; }}
    public function __destruct() {{ echo "destroy:", $this->name, "\n"; }}
}}
$source = $argc > 0 ? '
$text = "text"; $held = new OutputObject("held");
{calls}
echo "alive:", $held->name, "\n";
unset($held);
' : '';
eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}\n{}", output.stdout, output.stderr);
        assert_eq!(output.stdout, format!("{}alive:held\ndestroy:held\n",
            "echo:text\nprint:held\ntemporarydestroy:temporary\n\n".repeat(count)));
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "eval output retained per-expression cells");
}
