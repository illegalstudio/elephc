//! Purpose:
//! Focused parity cases that compare legacy AST backend output with EIR backend output.
//!
//! Called from:
//! - `tests/ir_backend_parity.rs` as an integration-test module.
//!
//! Key details:
//! - Each case compiles the same PHP snippet twice at the binary level, once
//!   through `--ast-backend` and once through the default EIR backend.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
enum Backend {
    Legacy,
    Ir,
}

impl Backend {
    /// Returns the short backend name used in diagnostics and temporary paths.
    fn name(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Ir => "ir",
        }
    }

    /// Adds the backend-specific CLI flags to the compile command.
    fn add_compile_flags(self, command: &mut Command) {
        if matches!(self, Self::Legacy) {
            command.arg("--ast-backend");
        }
    }
}

/// Verifies scalar locals and control flow stay equivalent across the two backends.
#[test]
fn parity_scalar_control_and_builtin_baseline() {
    assert_backend_parity(
        "scalar_control",
        "<?php $x = 40; if ($argc === 1) { echo $x + 2; } else { echo 0; }",
        &[],
    );
    assert_backend_parity(
        "scalar_builtins",
        "<?php echo strlen('abc'); echo ':'; echo strtoupper('ir');",
        &[],
    );
}

/// Verifies non-local `??=` expression lowering snapshots RHS containers before writes.
#[test]
fn parity_null_coalesce_assignment_snapshots_array_rhs() {
    assert_backend_parity(
        "null_coalesce_assignment_array_rhs_snapshot",
        r#"<?php
$items = [];
$result = ($items[0] ??= $items);
echo count($result) . ":" . count($items[0]);
"#,
        &[],
    );
}

/// Verifies direct and boxed object payloads retain their runtime class name in `var_dump()`.
#[test]
fn parity_var_dump_mixed_object_prints_class_name() {
    assert_backend_parity(
        "var_dump_mixed_object_class_name",
        r#"<?php
class Box {}
var_dump(new Box());
$map = ["i" => 1, "o" => new Box()];
var_dump($map["o"]);
"#,
        &[],
    );
}

/// Verifies discarded boxed `strpos()` comparison results are released by both backends.
#[test]
fn parity_strpos_strict_compare_releases_mixed_result() {
    assert_backend_gc_clean(
        "strpos_strict_compare_releases_mixed_result",
        r#"<?php
for ($i = 0; $i < 10; $i++) {
    if (strpos("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n", "\r\n\r\n") === false) {
        echo "bad";
    }
}
echo "done";
"#,
    );
}

/// Verifies chained concat temporaries are released after `echo` consumes them.
#[test]
fn parity_echo_concat_chain_releases_intermediates() {
    assert_backend_gc_clean(
        "echo_concat_chain_releases_intermediates",
        r#"<?php
for ($i = 0; $i < 8; $i++) {
    echo "a" . $i . "b" . $i . "c" . $i . "\n";
}
echo "done";
"#,
    );
}

/// Verifies returned concat strings survive later calls before the caller consumes them.
#[test]
fn parity_returned_concat_survives_caller_concat() {
    assert_backend_parity(
        "returned_concat_survives_caller_concat",
        r#"<?php
function eir_label($name) {
    return "[" . $name . "]";
}
echo eir_label("title") . "|" . eir_label("slug");
"#,
        &[],
    );
}

/// Verifies missing indexed-array reads return null and emit PHP's warning.
#[test]
fn parity_indexed_array_missing_key_warns() {
    let source = r#"<?php
$a = [10, 20, 30];
$v = $a[5];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#;
    let legacy = compile_and_run_backend_capture(
        "indexed_array_missing_key_warns",
        source,
        &[],
        Backend::Legacy,
        &[],
    );
    let ir = compile_and_run_backend_capture(
        "indexed_array_missing_key_warns",
        source,
        &[],
        Backend::Ir,
        &[],
    );
    assert_eq!(
        ir.0, legacy.0,
        "IR backend stdout differed from legacy for indexed_array_missing_key_warns"
    );
    assert!(
        legacy.1.contains("Warning: Undefined array key 5"),
        "legacy backend did not emit undefined-key warning: {}",
        legacy.1
    );
    assert!(
        ir.1.contains("Warning: Undefined array key 5"),
        "IR backend did not emit undefined-key warning: {}",
        ir.1
    );
}

/// Verifies float array keys truncate to integer keys in indexed and hash-backed arrays.
#[test]
fn parity_float_array_keys_truncate_to_int() {
    assert_backend_parity(
        "float_array_keys_truncate_to_int",
        r#"<?php
$a = [0, 10, 20];
$a[1.9] = 3;
echo $a[1] . "|" . $a[1.2];
echo "|";
$b = [];
$b[1.9] = 10;
$b[1] = 20;
echo $b[1.2];
"#,
        &[],
    );
}

/// Verifies nullable integer array literals preserve null tags when boxed as mixed values.
///
/// Elements are dumped individually rather than `var_dump($items)`: the EIR backend now renders the
/// full Mixed-array body, while the frozen legacy backend only emits the header (its Mixed-array
/// walker was never implemented and is being removed in v0.26). Per-element `var_dump` exercises the
/// same null-tag preservation and stays identical across both backends; the full-body Mixed `var_dump`
/// output is covered by `codegen::io::printing::test_var_dump_mixed_indexed_array`.
#[test]
fn parity_nullable_int_array_literal_preserves_nulls() {
    assert_backend_parity(
        "nullable_int_array_literal_preserves_nulls",
        r#"<?php
$items = [1, null, 3];
var_dump($items[0]);
var_dump($items[1]);
var_dump($items[2]);
echo json_encode($items);
"#,
        &[],
    );
}

/// Verifies by-reference indexed-array append stores unboxed Mixed payloads and writes back growth.
#[test]
fn parity_ref_array_push_unboxes_mixed_value() {
    assert_backend_parity(
        "ref_array_push_unboxes_mixed_value",
        r#"<?php
function eir_append_ref(&$arr, $val) {
    $arr[] = $val;
}
$x = [10, 20];
eir_append_ref($x, 30);
echo count($x) . "|" . $x[2];
"#,
        &[],
    );
}

/// Verifies `array_push()` can mutate an array carried through a boxed Mixed parameter.
#[test]
fn parity_array_push_mixed_receiver_growth() {
    assert_backend_parity(
        "array_push_mixed_receiver_growth",
        r#"<?php
function eir_grow($arr) {
    for ($i = 0; $i < 8; $i++) {
        array_push($arr, $i);
    }
    return $arr;
}
$arr = [100];
for ($j = 0; $j < 4; $j++) {
    $arr = eir_grow($arr);
}
echo count($arr) . "|" . $arr[32];
"#,
        &[],
    );
}

/// Verifies `explode()` arrays and copied string elements are released at function exit.
#[test]
fn parity_explode_parser_releases_arrays_and_elements() {
    assert_backend_gc_clean(
        "explode_parser_releases_arrays_and_elements",
        r#"<?php
function parse_once(string $raw): void {
    $lines = explode("\r\n", $raw);
    $parts = explode(" ", $lines[0]);
    $method = $parts[0];
    $path = $parts[1];
}

for ($i = 0; $i < 3; $i++) {
    parse_once("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}
echo "done";
"#,
    );
}

/// Verifies array elements returned from local containers survive function cleanup.
#[test]
fn parity_explode_returned_element_survives_cleanup() {
    assert_backend_gc_clean(
        "explode_returned_element_survives_cleanup",
        r#"<?php
function parse($data): string {
    $parts = explode(",", $data);
    return $parts[0];
}

for ($i = 0; $i < 3; $i++) {
    $r = parse("a,b,c");
}
echo $r;
"#,
    );
}

/// Verifies later indexed-array foreach states do not clobber source pointers at large frame offsets.
#[test]
fn parity_repeated_indexed_foreach() {
    assert_backend_parity(
        "repeated_indexed_foreach",
        r#"<?php
$a = [1, 2];
foreach ($a as $value) { echo $value; }
echo "|";
$b = [3, 4];
foreach ($b as $value) { echo $value; }
echo "|";
$c = [5, 6];
foreach ($c as $value) { echo $value; }
"#,
        &[],
    );
}

/// Verifies untyped numeric ordering and unary negation match legacy codegen.
///
/// The former `in_array_direct_echo_false` parity case was removed: the EIR backend now
/// types `in_array()` as `bool` (PHP-correct), so a false result echoes as "" while the
/// frozen legacy backend still echoes "0". This is a deliberate EIR improvement, not a
/// regression — the bool behavior is covered by `array_basics::test_in_array_returns_bool`
/// and the `in_array_*_missing` cases in `ir_backend_smoke_test`.
#[test]
fn parity_untyped_numeric_ordering_negation() {
    assert_backend_parity(
        "untyped_numeric_ordering_negation",
        r#"<?php
function abs_val($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}
echo abs_val(-5) . " " . abs_val(3);
"#,
        &[],
    );
}

// NOTE: there is intentionally no generator parity test.
//
// Generators were reimplemented on the EIR backend as stackful fiber coroutines
// (issue #329). That reimplementation replaced the shared `__rt_gen_*` runtime
// helpers and the `Generator` object layout with the fiber-coroutine versions.
// The frozen legacy `--ast-backend` still emits the old `GeneratorFrame`-based
// generator, which is no longer runtime-compatible with those shared helpers, so
// a legacy-compiled generator can no longer iterate or be freed correctly. The
// two backends therefore cannot be compared for generators, and the legacy path
// is slated for removal in v0.26.0. EIR generator behavior is covered directly by
// `tests/codegen/generators/`.

/// Verifies recursive regex child iterators keep their boxed child object alive.
#[test]
fn parity_recursive_regex_iterator_children() {
    assert_backend_parity(
        "recursive_regex_iterator_children",
        r#"<?php
$filter = new RecursiveRegexIterator(
    new RecursiveArrayIterator([
        "keep" => ["apple" => 1, "skip" => 2],
        "drop" => ["banana" => 3],
        "tail" => "apple",
    ]),
    "/keep|apple|tail/",
    RecursiveRegexIterator::MATCH,
    RecursiveRegexIterator::USE_KEY
);
$tree = new RecursiveIteratorIterator($filter, RecursiveIteratorIterator::SELF_FIRST);
foreach ($tree as $key => $value) {
    echo $tree->getDepth();
    echo ":";
    echo $key;
    echo "=";
    echo gettype($value) === "array" ? "array" : $value;
    echo ";";
}
"#,
        &[],
    );
}

/// Verifies RegexIterator GET_MATCH keeps capture slots after a by-ref array capture.
#[test]
fn parity_regex_iterator_get_match_many_captures() {
    assert_backend_parity(
        "regex_iterator_get_match_many_captures",
        r#"<?php
$it = new RegexIterator(
    new ArrayIterator(["abcdefghijkl"]),
    "/(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)/",
    RegexIterator::GET_MATCH
);
foreach ($it as $match) {
    echo count($match);
    echo ":";
    echo $match[11];
    echo $match[12];
}
"#,
        &[],
    );
}

/// Verifies RegexIterator offset-capture nested arrays survive Mixed array access.
#[test]
fn parity_regex_iterator_get_match_offset_capture() {
    assert_backend_parity(
        "regex_iterator_get_match_offset_capture",
        r#"<?php
$it = new RegexIterator(
    new ArrayIterator(["a12"]),
    "/([a-z])([0-9]+)/",
    RegexIterator::GET_MATCH,
    0,
    PREG_OFFSET_CAPTURE
);
foreach ($it as $match) {
    echo count($match);
    echo ":";
    echo $match[0][0];
    echo "@";
    echo $match[0][1];
    echo "/";
    echo $match[1][0];
    echo "@";
    echo $match[1][1];
    echo "/";
    echo $match[2][0];
    echo "@";
    echo $match[2][1];
}
"#,
        &[],
    );
}

/// Verifies Fiber descriptor-backed callable construction matches legacy backend behavior.
#[test]
fn parity_fiber_descriptor_backed_callables() {
    assert_backend_parity(
        "fiber_first_class_function",
        r#"<?php
function fiber_job(int $x): int {
    echo "job:" . $x;
    return $x + 1;
}
$f = new Fiber(fiber_job(...));
$v = $f->start(7);
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_string_builtin",
        r#"<?php
$f = new Fiber("STRLEN");
echo $f->start("abcd");
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_static_callable_array",
        r#"<?php
class FiberStaticJob {
    public static function run(string $value): string {
        echo "static:" . $value;
        return "static:done";
    }
}
$f = new Fiber([FiberStaticJob::class, "run"]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_instance_callable_array",
        r#"<?php
class FiberArrayJob {
    public function __construct(private string $prefix) {}

    public function run(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}
$job = new FiberArrayJob("array:");
$f = new Fiber([$job, "run"]);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_invokable_object",
        r#"<?php
class FiberInvokerJob {
    public function __construct(private string $prefix) {}

    public function __invoke(string $value): string {
        echo $this->prefix . $value;
        return $this->prefix . "done";
    }
}
$job = new FiberInvokerJob("invoke:");
$f = new Fiber($job);
$v = $f->start("go");
echo "/";
echo is_null($v) ? "null" : $v;
echo "/";
echo $f->getReturn();
"#,
        &[],
    );
}

/// Verifies core Fiber lifecycle and error behavior match the legacy backend.
#[test]
fn parity_fiber_lifecycle_and_errors() {
    assert_backend_parity(
        "fiber_get_current_inside",
        r#"<?php
$f = new Fiber(function(): void {
    $cur = Fiber::getCurrent();
    echo ($cur instanceof Fiber) ? "fiber" : "not-fiber";
    echo "/";
    echo $cur->isRunning() ? "running" : "not-running";
});
$f->start();
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_full_suspend_resume_cycle",
        r#"<?php
$f = new Fiber(function(): void {
    $a = Fiber::suspend("yield-1");
    echo "[got " . $a . "]";
    $b = Fiber::suspend("yield-2");
    echo "[got " . $b . "]";
    Fiber::suspend("yield-3");
});
echo $f->start();
echo "|";
echo $f->resume("resume-A");
echo "|";
echo $f->resume("resume-B");
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_capture_string_survives_suspend",
        r#"<?php
$ctx = "stable";
$f = new Fiber(function() use ($ctx): void {
    Fiber::suspend(0);
    echo "after=" . $ctx;
});
$f->start();
$f->resume(0);
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_error_start_twice",
        r#"<?php
$f = new Fiber(function(): void {});
$f->start();
try { $f->start(); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
        &[],
    );
    assert_backend_parity(
        "fiber_error_get_return_before_terminated",
        r#"<?php
$f = new Fiber(function(): void { Fiber::suspend(0); });
$f->start();
try { $f->getReturn(); echo "no-throw"; }
catch (FiberError $e) { echo $e->getMessage(); }
"#,
        &[],
    );
}

/// Verifies recent static callable fixes match the legacy backend behavior.
#[test]
fn parity_static_callable_checks() {
    assert_backend_parity(
        "direct_fcc_is_callable",
        r#"<?php
function eir_callable_probe(): int { return 1; }
class EirCallableProbe {
    public static function hit(): int { return 1; }
}
echo is_callable(strlen(...)) ? "yes" : "no";
echo ":";
echo is_callable(eir_callable_probe(...)) ? "yes" : "no";
echo ":";
echo is_callable(EirCallableProbe::hit(...)) ? "yes" : "no";
"#,
        &[],
    );
    assert_backend_parity(
        "static_method_string_is_callable",
        r#"<?php
class EirCallableBox {
    public static function hit(): int { return 1; }
    private static function hidden(): int { return 2; }
}
echo is_callable("EirCallableBox::hit") ? "yes" : "no";
echo ":";
echo is_callable("eircallablebox::HIT") ? "yes" : "no";
echo ":";
echo is_callable("EirCallableBox::missing") ? "yes" : "no";
echo ":";
echo is_callable("MissingCallableBox::hit") ? "yes" : "no";
echo ":";
echo is_callable("EirCallableBox::hidden") ? "yes" : "no";
"#,
        &[],
    );
    assert_backend_parity(
        "static_method_array_is_callable",
        r#"<?php
class EirCallableArrayBox {
    public static function hit(): int { return 1; }
    private static function hidden(): int { return 2; }
    public function instance(): int { return 3; }
}
echo is_callable(["EirCallableArrayBox", "hit"]) ? "yes" : "no";
echo ":";
echo is_callable(["eircallablearraybox", "HIT"]) ? "yes" : "no";
echo ":";
echo is_callable(["EirCallableArrayBox", "missing"]) ? "yes" : "no";
echo ":";
echo is_callable(["MissingCallableArrayBox", "hit"]) ? "yes" : "no";
echo ":";
echo is_callable(["EirCallableArrayBox", "hidden"]) ? "yes" : "no";
echo ":";
echo is_callable(["EirCallableArrayBox", "instance"]) ? "yes" : "no";
"#,
        &[],
    );
    assert_backend_parity(
        "static_callable_local_is_callable",
        r#"<?php
function eir_local_callable_probe(): int { return 1; }
class EirLocalCallableProbe {
    public static function hit(): int { return 1; }
}
$fn = eir_local_callable_probe(...);
echo is_callable($fn) ? "yes" : "no";
echo ":";
$name = "eir_local_callable_probe";
echo is_callable($name) ? "yes" : "no";
echo ":";
$stat = EirLocalCallableProbe::hit(...);
echo is_callable($stat) ? "yes" : "no";
"#,
        &[],
    );
}

/// Verifies recent callable dispatch fixes preserve legacy backend behavior.
#[test]
fn parity_function_first_class_callable_dispatch() {
    assert_backend_parity(
        "function_fcc_call_user_func",
        r#"<?php
function fcc_sum(int $left, int $right): int {
    return $left + $right;
}
function fcc_join(string $left, string $right): string {
    return $left . ":" . $right;
}
echo call_user_func(fcc_sum(...), 2, 5);
echo "|";
echo call_user_func_array(fcc_join(...), ["go", "now"]);
"#,
        &[],
    );
    assert_backend_parity(
        "preg_replace_callback_function_fcc",
        r#"<?php
function eir_regex_replace_fcc(array $matches): string {
    return "F" . count($matches);
}
echo preg_replace_callback("/[A-Z]/", eir_regex_replace_fcc(...), "AB");
"#,
        &[],
    );
    assert_backend_parity(
        "preg_replace_callback_stored_function_fcc",
        r#"<?php
function eir_regex_replace_stored_fcc(array $matches): string {
    return "S" . count($matches);
}
$cb = eir_regex_replace_stored_fcc(...);
echo preg_replace_callback("/[A-Z]/", $cb, "AB");
"#,
        &[],
    );
    assert_backend_parity(
        "direct_first_class_callable_expr_call",
        r#"<?php
function eir_direct_fcc_add(int $left, int $right): int {
    return $left + $right;
}
class EirDirectFcc {
    public static function hit(int $value): int {
        return $value + 2;
    }
}
echo (eir_direct_fcc_add(...))(2, 5);
echo "|";
echo (strlen(...))("abcd");
echo "|";
echo (EirDirectFcc::hit(...))(3);
"#,
        &[],
    );
    assert_backend_parity(
        "direct_string_callable_expr_call",
        r#"<?php
function eir_direct_string_add(int $value): int {
    return $value + 1;
}
echo ("eir_direct_string_add")(4);
"#,
        &[],
    );
    assert_backend_parity(
        "assignment_expression_callable_call",
        r#"<?php
function eir_assign_call_add(int $value): int {
    return $value + 1;
}
echo ($fn = eir_assign_call_add(...))(4);
echo "|";
echo ($name = "eir_assign_call_add")(5);
"#,
        &[],
    );
    assert_backend_parity(
        "stored_first_class_callable_variable_call",
        r#"<?php
function eir_stored_fcc_add(int $value): int {
    return $value + 1;
}
class EirStoredFcc {
    public static function hit(int $value): int {
        return $value + 2;
    }
}
$fn = eir_stored_fcc_add(...);
echo $fn(4);
echo "|";
$len = strlen(...);
echo $len("abcd");
echo "|";
$stat = EirStoredFcc::hit(...);
echo $stat(5);
"#,
        &[],
    );
    assert_backend_parity(
        "stored_first_class_callable_call_user_func",
        r#"<?php
function eir_stored_cuf_add(int $value): int {
    return $value + 1;
}
function eir_stored_cuf_join(string $left, string $right): string {
    return $left . ":" . $right;
}
class EirStoredCuf {
    public static function hit(int $value): int {
        return $value + 2;
    }
}
$fn = eir_stored_cuf_add(...);
echo call_user_func($fn, 4);
echo "|";
echo call_user_func_array($fn, [6]);
echo "|";
$join = eir_stored_cuf_join(...);
echo call_user_func_array($join, ["right" => "R", "left" => "L"]);
echo "|";
$len = strlen(...);
echo call_user_func($len, "abcd");
echo "|";
$stat = EirStoredCuf::hit(...);
echo call_user_func($stat, 5);
"#,
        &[],
    );
    assert_backend_parity(
        "stored_string_callable_dispatch",
        r#"<?php
function eir_string_callable_add(int $value): int {
    return $value + 1;
}
function eir_string_callable_join(string $left, string $right): string {
    return $left . ":" . $right;
}
function eir_string_callable_regex(array $matches): string {
    return "C" . count($matches);
}
$fn = "eir_string_callable_add";
echo $fn(4);
echo "|";
echo call_user_func($fn, 5);
echo "|";
$join = "eir_string_callable_join";
echo call_user_func_array($join, ["right" => "R", "left" => "L"]);
echo "|";
$regex = "eir_string_callable_regex";
echo preg_replace_callback("/[A-Z]/", $regex, "AB");
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_string_callable_direct_call_first",
        r#"<?php
function eir_runtime_pick_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pick_right(int $value): int {
    return $value + 2;
}
$fn = $argc === 1 ? "eir_runtime_pick_left" : "eir_runtime_pick_right";
echo $fn(4);
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_string_callable_direct_call_second",
        r#"<?php
function eir_runtime_pick_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pick_right(int $value): int {
    return $value + 2;
}
$fn = $argc === 1 ? "eir_runtime_pick_left" : "eir_runtime_pick_right";
echo $fn(4);
"#,
        &["extra"],
    );
    assert_backend_parity(
        "runtime_string_callable_expr_call_first",
        r#"<?php
function eir_runtime_expr_left(int $value): int {
    return $value + 1;
}
function eir_runtime_expr_right(int $value): int {
    return $value + 2;
}
echo ($argc === 1 ? "eir_runtime_expr_left" : "eir_runtime_expr_right")(4);
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_string_callable_expr_call_second",
        r#"<?php
function eir_runtime_expr_left(int $value): int {
    return $value + 1;
}
function eir_runtime_expr_right(int $value): int {
    return $value + 2;
}
echo ($argc === 1 ? "eir_runtime_expr_left" : "eir_runtime_expr_right")(4);
"#,
        &["extra"],
    );
    assert_backend_parity(
        "runtime_function_pipe_call_first",
        r#"<?php
function eir_runtime_pipe_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pipe_right(int $value): int {
    return $value + 2;
}
$cb = $argc === 1 ? eir_runtime_pipe_left(...) : eir_runtime_pipe_right(...);
echo 4 |> $cb;
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_function_pipe_call_second",
        r#"<?php
function eir_runtime_pipe_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pipe_right(int $value): int {
    return $value + 2;
}
$cb = $argc === 1 ? eir_runtime_pipe_left(...) : eir_runtime_pipe_right(...);
echo 4 |> $cb;
"#,
        &["extra"],
    );
}

/// Verifies static method callback forms lower to the same direct calls as legacy codegen.
#[test]
fn parity_static_method_callable_dispatch() {
    assert_backend_parity(
        "static_method_call_user_func",
        r#"<?php
class EirStaticCallback {
    public static function hit(int $value): int {
        return $value + 1;
    }
    public static function join(string $left, string $right): string {
        return $left . ":" . $right;
    }
}
echo call_user_func(["EirStaticCallback", "hit"], 4);
echo "|";
echo call_user_func(EirStaticCallback::hit(...), 8);
echo "|";
echo call_user_func_array(["eirstaticcallback", "JOIN"], ["right" => "R", "left" => "L"]);
"#,
        &[],
    );
    assert_backend_parity(
        "static_method_array_expr_call",
        r#"<?php
class EirArrayExprCallback {
    public static function hit(int $value): int {
        return $value + 1;
    }
    public static function join(string $left, string $right): string {
        return $left . ":" . $right;
    }
}
echo (["EirArrayExprCallback", "hit"])(4);
echo "|";
echo ([EirArrayExprCallback::class, "join"])(left: "L", right: "R");
"#,
        &[],
    );
}

/// Verifies static `array_map()` callback forms over indexed literals match legacy output.
#[test]
fn parity_static_array_map_callbacks() {
    assert_backend_parity(
        "static_array_map_callbacks",
        r#"<?php
function eir_map_inc(int $value): int {
    return $value + 1;
}
class EirMapStatic {
    public static function bump(int $value): int {
        return $value + 2;
    }
}
$ints = array_map(eir_map_inc(...), [1, 2]);
echo $ints[0]; echo ":"; echo $ints[1];
echo "|";
$fn = eir_map_inc(...);
$more = array_map($fn, [3, 4]);
echo $more[0]; echo ":"; echo $more[1];
echo "|";
$len = strlen(...);
$sizes = array_map($len, ["a", "abcd"]);
echo $sizes[0]; echo ":"; echo $sizes[1];
echo "|";
$stat = array_map(EirMapStatic::bump(...), [5, 6]);
echo $stat[0]; echo ":"; echo $stat[1];
"#,
        &[],
    );
}

/// Verifies static `array_filter()` string callbacks use the runtime helper like legacy codegen.
#[test]
fn parity_static_array_filter_callbacks() {
    assert_backend_parity(
        "static_array_filter_callbacks",
        r#"<?php
function eir_filter_odd(int $value): bool {
    return ($value % 2) === 1;
}
function eir_filter_key(int $key): bool {
    return $key === 1;
}
function eir_filter_both(int $value, int $key): bool {
    return $value > 2 && $key < 3;
}
class EirFilterStatic {
    public static function odd(int $value): bool {
        return ($value % 2) === 1;
    }
    public static function key_one(int $key): bool {
        return $key === 1;
    }
    public static function both(int $value, int $key): bool {
        return $value >= 4 && $key < 2;
    }
}
$odd = array_filter([1, 2, 3, 4], "eir_filter_odd");
echo count($odd); echo ":"; echo $odd[0]; echo ":"; echo $odd[1];
echo "|";
$keyed = array_filter([7, 8, 9], "eir_filter_key", ARRAY_FILTER_USE_KEY);
echo count($keyed); echo ":"; echo $keyed[0];
echo "|";
$both = array_filter([1, 3, 4, 2], "eir_filter_both", ARRAY_FILTER_USE_BOTH);
echo count($both); echo ":"; echo $both[0]; echo ":"; echo $both[1];
echo "|";
$mode = ARRAY_FILTER_USE_KEY;
$dynamic = array_filter([10, 20, 30], "eir_filter_key", $mode);
echo count($dynamic); echo ":"; echo $dynamic[0];
echo "|";
$fcc = array_filter([1, 2, 3], eir_filter_odd(...));
echo count($fcc); echo ":"; echo $fcc[0]; echo ":"; echo $fcc[1];
echo "|";
$static = array_filter([5, 6, 7], EirFilterStatic::odd(...));
echo count($static); echo ":"; echo $static[0]; echo ":"; echo $static[1];
echo "|";
$staticKey = array_filter([7, 8, 9], EirFilterStatic::key_one(...), ARRAY_FILTER_USE_KEY);
echo count($staticKey); echo ":"; echo $staticKey[0];
echo "|";
$staticBoth = array_filter([1, 4, 5], EirFilterStatic::both(...), ARRAY_FILTER_USE_BOTH);
echo count($staticBoth); echo ":"; echo $staticBoth[0];
"#,
        &[],
    );
}

/// Verifies static `array_reduce()` callback forms over immediate indexed literals match legacy output.
#[test]
fn parity_static_array_reduce_callbacks() {
    assert_backend_parity(
        "static_array_reduce_callbacks",
        r#"<?php
function eir_reduce_add(int $carry, int $item): int {
    return $carry + $item;
}
class EirReduceStatic {
    public static function mul(int $carry, int $item): int {
        return $carry * $item;
    }
}
echo array_reduce([1, 2, 3], "eir_reduce_add", 0);
echo "|";
$fn = eir_reduce_add(...);
echo array_reduce([4, 5], $fn, 1);
echo "|";
$stat = EirReduceStatic::mul(...);
echo array_reduce([2, 3, 4], $stat, 1);
"#,
        &[],
    );
}

/// Verifies static `array_walk()` callback forms over immediate indexed literals match legacy output.
#[test]
fn parity_static_array_walk_callbacks() {
    assert_backend_parity(
        "static_array_walk_callbacks",
        r#"<?php
function eir_walk_show(int $value): void {
    echo $value;
}
class EirWalkStatic {
    public static function show(int $value): void {
        echo $value + 1;
    }
}
array_walk([1, 2], "eir_walk_show");
echo "|";
$fn = eir_walk_show(...);
array_walk([3, 4], $fn);
echo "|";
$stat = EirWalkStatic::show(...);
array_walk([5, 6], $stat);
"#,
        &[],
    );
}

/// Verifies static user-sort callback forms route through the same runtime helper as legacy.
#[test]
fn parity_static_user_sort_callbacks() {
    assert_backend_parity(
        "static_usort_callback",
        r#"<?php
function eir_sort_asc(int $left, int $right): int {
    return $left - $right;
}
$usorted = [5, 3, 1, 4, 2];
usort($usorted, "eir_sort_asc");
foreach ($usorted as $value) { echo $value; }
"#,
        &[],
    );
    assert_backend_parity(
        "static_uksort_callback",
        r#"<?php
function eir_sort_desc(int $left, int $right): int {
    return $right - $left;
}
$uksorted = [1, 3, 2];
uksort($uksorted, "eir_sort_desc");
foreach ($uksorted as $value) { echo $value; }
"#,
        &[],
    );
    assert_backend_parity(
        "static_uasort_callback",
        r#"<?php
function eir_sort_asc(int $left, int $right): int {
    return $left - $right;
}
$uasorted = [30, 10, 20];
uasort($uasorted, "eir_sort_asc");
foreach ($uasorted as $value) { echo $value . ":"; }
"#,
        &[],
    );
    assert_backend_parity(
        "static_usort_first_class_callback",
        r#"<?php
function eir_sort_asc(int $left, int $right): int {
    return $left - $right;
}
class EirSortStatic {
    public static function desc(int $left, int $right): int {
        return $right - $left;
    }
}
$usorted = [3, 1, 2];
usort($usorted, eir_sort_asc(...));
foreach ($usorted as $value) { echo $value; }
echo "|";
$static = [1, 3, 2];
usort($static, EirSortStatic::desc(...));
foreach ($static as $value) { echo $value; }
"#,
        &[],
    );
}

/// Verifies pure static late-bound first-class callbacks keep vtable metadata like legacy codegen.
#[test]
fn parity_late_bound_static_first_class_callbacks_without_object_metadata() {
    assert_backend_parity(
        "late_bound_static_fcc_without_object_metadata",
        r#"<?php
class BaseLateMap {
    public static function offset(int $value): int {
        return $value + 10;
    }

    public static function add(int $carry, int $value): int {
        return $carry + $value + 10;
    }

    public static function map(): string {
        $values = array_map(static::offset(...), [1, 2]);
        return $values[0] . ":" . $values[1];
    }

    public static function reduce(): int {
        return array_reduce([1, 2], static::add(...), 0);
    }
}

class ChildLateMap extends BaseLateMap {
    public static function offset(int $value): int {
        return $value + 20;
    }

    public static function add(int $carry, int $value): int {
        return $carry + $value + 20;
    }
}

echo ChildLateMap::map();
echo "|";
echo ChildLateMap::reduce();
"#,
        &[],
    );
}

/// Verifies late-bound `static::` sort callbacks match legacy runtime dispatch.
#[test]
fn parity_late_bound_static_sort_callbacks() {
    assert_backend_parity(
        "late_bound_static_sort_callbacks",
        r#"<?php
class BaseCallbacks {
    public static function add(int $carry, int $value): int {
        return $carry + $value + 10;
    }

    public static function show(int $value): void {
        echo $value + 10;
        echo ",";
    }

    public static function compare(int $left, int $right): int {
        return $right - $left;
    }

    public static function run(): void {
        echo array_reduce([1, 2], static::add(...), 0);
        echo ":";
        array_walk([1, 2], static::show(...));
        echo ":";
        $usorted = [1, 2, 3];
        usort($usorted, static::compare(...));
        foreach ($usorted as $value) { echo $value; }
        echo ":";
        $uksorted = [1, 2, 3];
        uksort($uksorted, static::compare(...));
        foreach ($uksorted as $value) { echo $value; }
        echo ":";
        $uasorted = [1, 2, 3];
        uasort($uasorted, static::compare(...));
        foreach ($uasorted as $value) { echo $value; }
    }
}

class ChildCallbacks extends BaseCallbacks {
    public static function add(int $carry, int $value): int {
        return $carry + $value + 20;
    }

    public static function show(int $value): void {
        echo $value + 20;
        echo ",";
    }

    public static function compare(int $left, int $right): int {
        return $left - $right;
    }
}

BaseCallbacks::run();
echo "|";
ChildCallbacks::run();
"#,
        &[],
    );
}

/// Verifies first-class instance-method sort callbacks match legacy receiver dispatch.
#[test]
fn parity_instance_method_sort_callbacks() {
    assert_backend_parity(
        "instance_method_sort_callbacks",
        r#"<?php
class Sorter {
    public function desc(int $left, int $right): int {
        return $right - $left;
    }

    public function asc(int $left, int $right): int {
        return $left - $right;
    }
}

$sorter = new Sorter();

$usorted = [1, 3, 2];
usort($usorted, $sorter->desc(...));
foreach ($usorted as $value) { echo $value; }
echo ":";

$uksorted = [1, 3, 2];
uksort($uksorted, $sorter->desc(...));
foreach ($uksorted as $value) { echo $value; }
echo ":";

$uasorted = [3, 1, 2];
uasort($uasorted, $sorter->asc(...));
foreach ($uasorted as $value) { echo $value; }
"#,
        &[],
    );
}

/// Verifies instance-method reduce and walk callbacks match legacy receiver dispatch.
#[test]
fn parity_instance_method_reduce_and_walk_callbacks() {
    assert_backend_parity(
        "instance_method_reduce_and_walk_callbacks",
        r#"<?php
class CallbackBox {
    public function add_offset(int $carry, int $item): int {
        return $carry + $item + 10;
    }

    public function show(int $item): void {
        echo $item + 5;
        echo ":";
    }
}

$box = new CallbackBox();
echo array_reduce([1, 2], $box->add_offset(...), 0);
echo "|";
array_walk([1, 2], $box->show(...));
"#,
        &[],
    );
}

/// Verifies instance-method `array_map()` callbacks match legacy receiver dispatch.
#[test]
fn parity_instance_method_array_map_callbacks() {
    assert_backend_parity(
        "instance_method_array_map_callbacks",
        r#"<?php
class MapperBox {
    public function add_offset(int $item): int {
        return $item + 10;
    }
}

$box = new MapperBox();
$mapped = array_map($box->add_offset(...), [1, 2]);
echo $mapped[0];
echo ":";
echo $mapped[1];
"#,
        &[],
    );
}

/// Verifies instance-method `array_map()` callbacks over string arrays match legacy output.
#[test]
fn parity_instance_method_array_map_string_callbacks() {
    assert_backend_parity(
        "instance_method_array_map_string_callbacks",
        r#"<?php
class StringMapperBox {
    public function bracket(string $item): string {
        return "[" . $item . "]";
    }
}

$box = new StringMapperBox();
$mapped = array_map($box->bracket(...), ["a", "b"]);
echo $mapped[0];
echo ":";
echo $mapped[1];
"#,
        &[],
    );
}

/// Verifies stored instance-method `array_map()` callbacks keep legacy receiver capture.
#[test]
fn parity_stored_instance_method_array_map_callbacks() {
    assert_backend_parity(
        "stored_instance_method_array_map_callbacks",
        r#"<?php
class StoredMapperBox {
    public int $base = 0;

    public function add(int $item): int {
        return $this->base + $item;
    }
}

$box = new StoredMapperBox();
$box->base = 10;
$fn = $box->add(...);
$box = new StoredMapperBox();
$box->base = 100;
$mapped = array_map($fn, [1, 2]);
echo $mapped[0];
echo ":";
echo $mapped[1];
"#,
        &[],
    );
}

/// Verifies callable-parameter `array_map()` callbacks keep legacy descriptor receivers.
#[test]
fn parity_instance_method_array_map_callable_parameter() {
    assert_backend_parity(
        "instance_method_array_map_callable_parameter",
        r#"<?php
class ParamMapperBox {
    public function __construct(private int $base) {}

    public function add(int $item): int {
        return $this->base + $item;
    }
}

function run_map(callable $cb): void {
    $mapped = array_map($cb, [1, 2]);
    echo $mapped[0];
    echo ":";
    echo $mapped[1];
}

$box = new ParamMapperBox(10);
$fn = $box->add(...);
$box = new ParamMapperBox(100);
run_map($fn);
"#,
        &[],
    );
}

/// Verifies stored instance-method reduce and walk callbacks keep legacy receiver capture.
#[test]
fn parity_stored_instance_method_reduce_and_walk_callbacks() {
    assert_backend_parity(
        "stored_instance_method_reduce_and_walk_callbacks",
        r#"<?php
class StoredReduceWalkBox {
    public int $base = 0;

    public function add(int $carry, int $item): int {
        return $carry + $this->base + $item;
    }

    public function show(int $item): void {
        echo $this->base + $item;
        echo ":";
    }
}

$box = new StoredReduceWalkBox();
$box->base = 10;
$reduce = $box->add(...);
$walk = $box->show(...);
$box = new StoredReduceWalkBox();
$box->base = 100;
echo array_reduce([1, 2], $reduce, 0);
echo "|";
array_walk([1, 2], $walk);
"#,
        &[],
    );
}

/// Verifies stored instance-method first-class callable expression calls match legacy output.
#[test]
fn parity_stored_instance_method_expr_call() {
    assert_backend_parity(
        "stored_instance_method_expr_call",
        r#"<?php
class StoredExprCallBox {
    public function add(int $value): int {
        return $value + 7;
    }
}

$box = new StoredExprCallBox();
$fn = $box->add(...);
echo ($fn)(5);
"#,
        &[],
    );
}

/// Verifies stored instance-method variable calls match legacy output.
#[test]
fn parity_stored_instance_method_variable_call() {
    assert_backend_parity(
        "stored_instance_method_variable_call",
        r#"<?php
class StoredVariableCallBox {
    public function __construct(private string $name) {}

    public function read(): string {
        return $this->name;
    }
}

$box = new StoredVariableCallBox("old");
$fn = $box->read(...);
$box = new StoredVariableCallBox("new");
echo $fn();
"#,
        &[],
    );
}

/// Verifies stored instance-method named args/defaults match legacy output.
#[test]
fn parity_stored_instance_method_named_args() {
    assert_backend_parity(
        "stored_instance_method_named_args",
        r#"<?php
class StoredNamedArgBox {
    public function __construct(private string $prefix) {}

    public function format(string $value, string $suffix = "!"): string {
        return $this->prefix . $value . $suffix;
    }
}

$box = new StoredNamedArgBox("old:");
$fn = $box->format(...);
$box = new StoredNamedArgBox("new:");
echo $fn(value: "Ada");
"#,
        &[],
    );
}

/// Verifies stored instance-method by-reference params match legacy output.
#[test]
fn parity_stored_instance_method_by_ref_params() {
    assert_backend_parity(
        "stored_instance_method_by_ref_params",
        r#"<?php
class StoredByRefBox {
    public function bump(&$value): void {
        $value = $value + 2;
    }
}

$box = new StoredByRefBox();
$fn = $box->bump(...);
$value = 5;
$fn($value);
echo $value;
"#,
        &[],
    );
}

/// Verifies first-class function callable by-reference aliases match legacy output.
#[test]
fn parity_first_class_callable_alias_by_ref_params() {
    assert_backend_parity(
        "first_class_callable_alias_by_ref_params",
        r#"<?php
function bump(&$value): void {
    $value = $value + 1;
}

$fn = bump(...);
$alias = $fn;
$value = 7;
$alias($value);
echo $value;
"#,
        &[],
    );
}

/// Verifies closure by-reference aliases with Mixed params match legacy output.
#[test]
fn parity_closure_alias_by_ref_mixed_params() {
    assert_backend_parity(
        "closure_alias_by_ref_mixed_params",
        r#"<?php
$fn = function (&$value): void {
    $value = $value + 1;
};

$alias = $fn;
$value = 7;
$alias($value);
echo $value;
"#,
        &[],
    );
}

/// Verifies instance-method first-class callable `call_user_func*` output matches legacy.
#[test]
fn parity_instance_method_call_user_func_callbacks() {
    assert_backend_parity(
        "instance_method_call_user_func_callbacks",
        r#"<?php
class StoredCallUserFuncBox {
    public int $base = 0;

    public function add(int $value): int {
        return $this->base + $value;
    }

    public function combine(int $left, int $right): int {
        return $this->base + $left * 10 + $right;
    }
}

class InlineCallUserFuncGreeter {
    public function greet(string $name): string {
        return "Hi " . $name;
    }
}

$box = new StoredCallUserFuncBox();
$box->base = 7;
$add = $box->add(...);
$combine = $box->combine(...);
echo call_user_func($add, 5);
echo ":";
echo call_user_func_array($combine, [3, 4]);
echo ":";
$greeter = new InlineCallUserFuncGreeter();
echo call_user_func($greeter->greet(...), "Ada");
"#,
        &[],
    );
}

/// Verifies stored instance-method `array_filter()` callbacks keep legacy receiver capture.
#[test]
fn parity_stored_instance_method_array_filter_callbacks() {
    assert_backend_parity(
        "stored_instance_method_array_filter_callbacks",
        r#"<?php
class StoredFilterBox {
    public int $base = 0;

    public function keep(int $item): bool {
        return $this->base + $item > 12;
    }
}

$box = new StoredFilterBox();
$box->base = 10;
$filter = $box->keep(...);
$box = new StoredFilterBox();
$box->base = 100;
$values = array_filter([1, 2, 3], $filter);
echo count($values);
foreach ($values as $value) {
    echo ":";
    echo $value;
}
"#,
        &[],
    );
}

/// Verifies local `array_filter()` mode values keep legacy instance callback ABI shape.
#[test]
fn parity_stored_instance_method_array_filter_local_mode_callbacks() {
    assert_backend_parity(
        "stored_instance_method_array_filter_local_mode_callbacks",
        r#"<?php
class StoredKeyFilterBox {
    public int $offset = 0;

    public function keep(int $key): bool {
        return $key + $this->offset === 1;
    }
}

$box = new StoredKeyFilterBox();
$box->offset = 0;
$filter = $box->keep(...);
$mode = ARRAY_FILTER_USE_KEY;
$values = array_filter([7, 8, 9], $filter, $mode);
echo count($values);
foreach ($values as $value) {
    echo ":";
    echo $value;
}
"#,
        &[],
    );
}

/// Verifies reflection attribute owner metadata matches the legacy backend.
#[test]
fn parity_reflection_owner_attributes() {
    assert_backend_parity(
        "reflection_owner_get_attributes",
        r#"<?php
class EirRoute {
    public function __construct(string $name) {
        echo "ctor:" . $name . ":";
    }
}
#[EirRoute("class")]
class EirReflectedController {
    #[EirRoute("method")]
    public function handle(): void {}

    #[EirRoute("property")]
    public int $id = 0;
}
$class = new ReflectionClass(EirReflectedController::class);
$classAttrs = $class->getAttributes();
$methodAttrs = (new ReflectionMethod(EirReflectedController::class, "handle"))->getAttributes();
$propertyAttrs = (new ReflectionProperty(EirReflectedController::class, "id"))->getAttributes();
echo $class->getName();
echo ":";
echo $classAttrs[0]->getName();
echo ":";
echo $classAttrs[0]->getArguments()[0];
echo ":";
$classAttrs[0]->newInstance();
$methodAttrs[0]->newInstance();
$propertyAttrs[0]->newInstance();
"#,
        &[],
    );
}

/// Verifies the supported `SplFileInfo` EIR slice matches the legacy backend.
#[test]
fn parity_spl_file_info_basics() {
    assert_backend_parity(
        "spl_file_info_basics",
        r#"<?php
$info = new SplFileInfo(".");
echo $info->getPathname();
echo ":";
echo $info->__toString();
echo ":";
echo ($info instanceof SplFileInfo) ? "C" : "x";
echo ($info instanceof Stringable) ? "I" : "x";
"#,
        &[],
    );
}

/// Verifies `SplFileInfo` path/stat helper methods match the legacy backend.
#[test]
fn parity_spl_file_info_path_stat_helpers() {
    assert_backend_parity(
        "spl_file_info_path_stat_helpers",
        r#"<?php
mkdir("docs");
file_put_contents("docs/a.txt", "one\ntwo\n");

$info = new SplFileInfo("docs/a.txt");
echo $info->getFilename();
echo "|";
echo $info->getExtension();
echo "|";
echo $info->getBasename(".txt");
echo "|";
echo $info->getPath();
echo "|";
echo $info->isFile() ? "file" : "no";
echo "|";
echo $info->getSize();

unlink("docs/a.txt");
rmdir("docs");
"#,
        &[],
    );
}

/// Verifies extended `SplFileInfo` stat/access/link helper methods match the legacy backend.
#[test]
fn parity_spl_file_info_extended_stat_helpers() {
    assert_backend_parity(
        "spl_file_info_extended_stat_helpers",
        r##"<?php
mkdir("docs");
file_put_contents("docs/a.txt", "one\n");
file_put_contents("docs/run.sh", "#!/bin/sh\n");
chmod("docs/run.sh", 0755);
symlink("a.txt", "docs/link.txt");

$file = new SplFileInfo("docs/a.txt");
$dir = new SplFileInfo("docs");
$exec = new SplFileInfo("docs/run.sh");
$link = new SplFileInfo("docs/link.txt");

echo ($file->getPerms() !== false) ? "P" : "x";
echo ($file->getInode() !== false) ? "I" : "x";
echo ($file->getOwner() !== false) ? "O" : "x";
echo ($file->getGroup() !== false) ? "G" : "x";
echo ($file->getATime() !== false) ? "A" : "x";
echo ($file->getMTime() > 0) ? "M" : "x";
echo ($file->getCTime() !== false) ? "C" : "x";
echo $file->getType();
echo ":";
echo $file->isWritable() ? "W" : "x";
echo $file->isWriteable() ? "w" : "x";
echo $file->isReadable() ? "R" : "x";
echo $exec->isExecutable() ? "X" : "x";
echo $dir->isDir() ? "D" : "x";
echo $link->isLink() ? "L" : "x";
echo ":";
echo $link->getLinkTarget();
echo ":";
echo ($file->getRealPath() === false) ? "x" : "P";

unlink("docs/link.txt");
unlink("docs/run.sh");
unlink("docs/a.txt");
rmdir("docs");
"##,
        &[],
    );
}

/// Verifies dynamic `SplFileInfo` factories match the legacy backend.
#[test]
fn parity_spl_file_info_dynamic_factories() {
    assert_backend_parity(
        "spl_file_info_dynamic_factories",
        r#"<?php
class EirInfo extends SplFileInfo {}

$info = new SplFileInfo(".");
$file = $info->getFileInfo();
$path = $info->getPathInfo();
$customFile = $info->getFileInfo("EirInfo");
$customPath = $info->getPathInfo("EirInfo");

echo ($file instanceof SplFileInfo) ? "F" : "x";
echo ":";
echo $file->getPathname();
echo ":";
echo ($path instanceof SplFileInfo) ? "P" : "x";
echo ":";
echo $path->getPathname();
echo ":";
echo ($customFile instanceof EirInfo) ? "E" : "x";
echo ":";
echo $customFile->getPathname();
echo ":";
echo ($customPath instanceof EirInfo) ? "Q" : "x";
echo ":";
echo $customPath->getPathname();
"#,
        &[],
    );
}

/// Verifies `setInfoClass()` stored factory overrides match the legacy backend.
#[test]
fn parity_spl_file_info_stored_info_class() {
    assert_backend_parity(
        "spl_file_info_stored_info_class",
        r#"<?php
class EirInfo extends SplFileInfo {}

$info = new SplFileInfo(".");
$info->setInfoClass(EirInfo::class);
$file = $info->getFileInfo();
$path = $info->getPathInfo();
echo ($file instanceof EirInfo) ? "F" : "x";
echo ":";
echo ($path instanceof EirInfo) ? "P" : "x";
"#,
        &[],
    );
}

/// Verifies `SplFileInfo::openFile()` and `setFileClass()` match the legacy backend.
#[test]
fn parity_spl_file_info_open_file() {
    assert_backend_parity(
        "spl_file_info_open_file",
        r#"<?php
class EirFile extends SplFileObject {}

file_put_contents("a.txt", "one\ntwo\n");

$info = new SplFileInfo("a.txt");
$file = $info->openFile();
echo ($file instanceof SplFileObject) ? "F" : "x";
echo ":";
echo $file->fgets();
echo ":";
echo $file->key();

$direct = new SplFileObject("a.txt");
echo ":";
echo $direct->fgets();

$info->setFileClass(EirFile::class);
$custom = $info->openFile("r");
echo ":";
echo ($custom instanceof EirFile) ? "C" : "x";
echo ":";
echo $custom->fgets();

unlink("a.txt");
"#,
        &[],
    );
}

/// Verifies direct `SplFileObject` construction and method calls match the legacy backend.
#[test]
fn parity_direct_spl_file_object_methods() {
    assert_backend_parity(
        "direct_spl_file_object_methods",
        r#"<?php
file_put_contents("a.txt", "one\ntwo\n");

$file = new SplFileObject("a.txt");
echo ($file instanceof SplFileObject) ? "F" : "x";
echo ":";
$file->seek(1);
echo $file->current();
echo ":";
$file->rewind();
echo $file->fgets();
echo ":";
echo $file->key();

unlink("a.txt");
"#,
        &[],
    );
}

/// Verifies `foreach` over `SplFileObject` matches the legacy backend.
#[test]
fn parity_spl_file_object_foreach() {
    assert_backend_parity(
        "spl_file_object_foreach",
        r#"<?php
file_put_contents("a.txt", "one\ntwo\n");

$info = new SplFileInfo("a.txt");
foreach ($info->openFile() as $line => $text) {
    echo $line;
    echo ":";
    echo $text;
    echo ";";
}

unlink("a.txt");
"#,
        &[],
    );
}

/// Verifies simple `SplFileObject` CSV current-row behavior matches the legacy backend.
#[test]
fn parity_spl_file_object_csv_current() {
    assert_backend_parity(
        "spl_file_object_csv_current",
        r#"<?php
file_put_contents("a.txt", "one\ntwo\n");

$csv = new SplFileObject("a.txt");
$csv->setFlags(SplFileObject::READ_CSV);
$csv->setCsvControl("n");
$row = $csv->current();
echo $row[0];
echo ":";
echo $row[1];

unlink("a.txt");
"#,
        &[],
    );
}

/// Verifies `SplFileObject` stream-position methods match the legacy backend.
#[test]
fn parity_spl_file_object_stream_position_methods() {
    assert_backend_parity(
        "spl_file_object_stream_position_methods",
        r#"<?php
file_put_contents("stream.txt", "abcdef\nsecond\n");

$file = new SplFileObject("stream.txt", "r+");
echo $file->fread(3);
echo "|";
echo $file->ftell();
$file->fseek(4);
echo "|";
echo $file->fread(2);
$file->fseek(0);
$file->fwrite("XY");
$file->fseek(0);
echo "|";
echo $file->fread(6);
$file->ftruncate(4);
$file->fseek(0);
echo "|";
echo $file->fread(10);

unlink("stream.txt");
"#,
        &[],
    );
}

/// Verifies `SplFileObject` lightweight state helpers match the legacy backend.
#[test]
fn parity_spl_file_object_state_helpers() {
    assert_backend_parity(
        "spl_file_object_state_helpers",
        r#"<?php
file_put_contents("meta.txt", "aa\nbb\n");

$file = new SplFileObject("meta.txt");
echo $file->getCurrentLine();
echo "|";
echo $file->fgetc();
echo $file->fgetc();
echo "|";
$file->fseek(0, 2);
echo ($file->fgetc() === false) ? "F" : "x";
echo $file->eof() ? "E" : "N";
echo "|";
$file->setFlags(SplFileObject::READ_CSV);
echo $file->getFlags();
echo "|";
$file->setMaxLineLen(7);
echo $file->getMaxLineLen();
echo "|";

unlink("meta.txt");
"#,
        &[],
    );
}

/// Verifies `SplFileObject` CSV read/write methods match the legacy backend.
#[test]
fn parity_spl_file_object_csv_methods() {
    assert_backend_parity(
        "spl_file_object_csv_methods",
        r#"<?php
$file = new SplFileObject("csv.txt", "w+");
echo $file->fputcsv(["hello", "world"]);
$file->rewind();
$row = $file->fgetcsv();
echo ":";
echo $row[0];
echo ":";
echo $row[1];
echo ":";
echo $file->key();

unlink("csv.txt");
"#,
        &[],
    );
}

/// Verifies `SplTempFileObject` memory-mode read/write methods match the legacy backend.
#[test]
fn parity_spl_temp_file_object_memory_stream() {
    assert_backend_parity(
        "spl_temp_file_object_memory_stream",
        r#"<?php
$tmp = new SplTempFileObject(-1);
echo $tmp->getPathname();
$tmp->fwrite("first\nsecond\n");
$tmp->rewind();
echo "|";
echo $tmp->fgets();
echo "|";
echo $tmp->fgets();
echo "|";
echo $tmp->eof() ? "eof" : "more";
"#,
        &[],
    );
}

/// Verifies `SplTempFileObject` memory cursor/stat helpers match the legacy backend.
#[test]
fn parity_spl_temp_file_object_memory_cursor_and_stat() {
    assert_backend_parity(
        "spl_temp_file_object_memory_cursor_and_stat",
        r#"<?php
$tmp = new SplTempFileObject(10);
echo $tmp->getPathname();
echo "|";
echo $tmp->ftell();
echo "|";
echo $tmp->fwrite("abc");
echo "|";
echo $tmp->ftell();
$tmp->fseek(1);
$tmp->fwrite("Z");
$tmp->rewind();
echo "|";
echo $tmp->fread(3);
$stat = $tmp->fstat();
echo "|";
echo $stat["size"];
"#,
        &[],
    );
}

/// Verifies `SplTempFileObject` memory byte/truncate helpers match the legacy backend.
#[test]
fn parity_spl_temp_file_object_memory_byte_and_truncate() {
    assert_backend_parity(
        "spl_temp_file_object_memory_byte_and_truncate",
        r#"<?php
$tmp = new SplTempFileObject(-1);
$tmp->fwrite("abcd");
$tmp->fseek(1);
echo $tmp->fgetc();
echo "|";
echo $tmp->fflush() ? "T" : "F";
echo "|";
$tmp->ftruncate(2);
$tmp->rewind();
echo $tmp->fread(10);
"#,
        &[],
    );
}

/// Verifies `SplTempFileObject` spill stream behavior matches the legacy backend.
#[test]
fn parity_spl_temp_file_object_spill_stream() {
    assert_backend_parity(
        "spl_temp_file_object_spill_stream",
        r#"<?php
$tmp = new SplTempFileObject(3);
$tmp->fwrite("abc");
echo $tmp->ftell();
echo "|";
$tmp->fwrite("d");
echo $tmp->ftell();
$tmp->fseek(1);
$tmp->fwrite("YY");
$tmp->rewind();
echo "|";
echo $tmp->fread(4);
$tmp->ftruncate(2);
$tmp->rewind();
echo "|";
echo $tmp->fread(10);
"#,
        &[],
    );
}

/// Compiles and runs a PHP snippet through both backends and compares stdout.
fn assert_backend_parity(name: &str, source: &str, args: &[&str]) {
    let legacy = compile_and_run_backend(name, source, args, Backend::Legacy);
    let ir = compile_and_run_backend(name, source, args, Backend::Ir);
    assert_eq!(ir, legacy, "IR backend stdout differed from legacy for {name}");
}

/// Compiles/runs both backends with GC stats and requires each to leave no live heap blocks.
fn assert_backend_gc_clean(name: &str, source: &str) {
    let legacy = compile_and_run_backend_capture(name, source, &[], Backend::Legacy, &["--gc-stats"]);
    let ir = compile_and_run_backend_capture(name, source, &[], Backend::Ir, &["--gc-stats"]);
    assert_eq!(
        ir.0, legacy.0,
        "IR backend stdout differed from legacy for {name}"
    );
    let legacy_stats = parse_gc_stats(&legacy.1);
    let ir_stats = parse_gc_stats(&ir.1);
    assert_eq!(
        legacy_stats.0, legacy_stats.1,
        "legacy backend leaked heap blocks for {name}: {}",
        legacy.1
    );
    assert_eq!(
        ir_stats.0, ir_stats.1,
        "IR backend leaked heap blocks for {name}: {}",
        ir.1
    );
}

/// Compiles a PHP snippet with one backend, runs the produced binary, and returns stdout.
fn compile_and_run_backend(name: &str, source: &str, args: &[&str], backend: Backend) -> String {
    compile_and_run_backend_capture(name, source, args, backend, &[]).0
}

/// Compiles a PHP snippet with one backend, runs it, and returns stdout/stderr.
fn compile_and_run_backend_capture(
    name: &str,
    source: &str,
    args: &[&str],
    backend: Backend,
    compiler_args: &[&str],
) -> (String, String) {
    let dir = temp_case_dir(name, backend);
    fs::create_dir_all(&dir).expect("failed to create parity test directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write parity PHP fixture");

    let mut compile_command = Command::new(elephc_cli_bin());
    compile_command
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir);
    backend.add_compile_flags(&mut compile_command);
    compile_command.args(compiler_args);
    let compile = compile_command
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI for parity fixture");
    assert!(
        compile.status.success(),
        "elephc {} backend failed for {name}: stderr={}",
        backend.name(),
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .args(args)
        .output()
        .expect("failed to run parity binary");
    assert!(
        run.status.success(),
        "{} backend binary failed for {name}: stderr={}",
        backend.name(),
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8(run.stdout).expect("parity binary stdout should be utf8");
    let stderr = String::from_utf8(run.stderr).expect("parity binary stderr should be utf8");
    let _ = fs::remove_dir_all(&dir);
    (stdout, stderr)
}

/// Parses one `GC: allocs=N frees=N` line from a parity binary's stderr.
fn parse_gc_stats(stderr: &str) -> (u64, u64) {
    let line = stderr
        .lines()
        .find(|line| line.starts_with("GC: allocs="))
        .unwrap_or_else(|| panic!("missing gc stats line: {stderr}"));
    let allocs = line
        .split("allocs=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing alloc count: {stderr}"));
    let frees = line
        .split("frees=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing free count: {stderr}"));
    (allocs, frees)
}

/// Builds a unique temporary directory path for one backend run.
fn temp_case_dir(name: &str, backend: Backend) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "elephc_ir_parity_{}_{}_{}_{}",
        name,
        backend.name(),
        std::process::id(),
        id
    ))
}

/// Returns the path to the cargo-built `elephc` binary.
fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}
