//! Purpose:
//! End-to-end regressions for small PHP core builtins required by ext/date PHPTs.
//!
//! Called from:
//! - `cargo test --test php_src_support_builtins_tests` through Rust's test harness.
//!
//! Key details:
//! - Each test compiles a standalone PHP source with the host elephc binary and executes it.
//! - Coverage includes case-insensitive names, namespace fallback, dynamic constants, and scope visibility.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated test directory unique across parallel test processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "{prefix}_{}_{}",
        std::process::id(),
        id,
    ));
    fs::create_dir_all(&dir).expect("failed to create builtin test directory");
    dir
}

/// Resolves the elephc CLI compiled alongside this integration-test binary.
fn elephc_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_elephc")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = std::env::current_exe().expect("failed to resolve test binary");
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path.join("elephc")
        })
}

/// Compiles one PHP source and returns the compiler process output plus executable path.
fn compile_source(dir: &Path, source: &str, stem: &str) -> (Output, PathBuf) {
    let source_path = dir.join(format!("{stem}.php"));
    fs::write(&source_path, source).expect("failed to write PHP fixture");
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .arg(&source_path)
        .output()
        .expect("failed to execute elephc");
    (output, dir.join(stem))
}

/// Compiles and runs a successful PHP fixture, returning its stdout.
fn compile_and_run(source: &str, stem: &str) -> String {
    let dir = make_test_dir(stem);
    let (compile, binary) = compile_source(&dir, source, stem);
    assert!(
        compile.status.success(),
        "elephc compilation failed:\n{}",
        String::from_utf8_lossy(&compile.stderr),
    );
    let run = Command::new(binary)
        .output()
        .expect("failed to run compiled fixture");
    assert!(
        run.status.success(),
        "compiled fixture failed:\n{}",
        String::from_utf8_lossy(&run.stderr),
    );
    String::from_utf8(run.stdout).expect("fixture stdout must be UTF-8")
}

/// Verifies all five builtins resolve case-insensitively from a namespace and produce PHP values.
#[test]
fn support_builtins_resolve_through_global_namespace_fallback() {
    let output = compile_and_run(
        r#"<?php
namespace Demo;
class Sample {
    public int $visible = 7;
    private int $hidden = 9;
}
\GC_ENABLE();
echo \GETRANDMAX(), "\n";
echo \SiZeOf([1, 2, 3]), "\n";
$constantName = "DATE_ATOM";
echo \CoNsTaNt($constantName), "\n";
$vars = \GET_OBJECT_VARS(new Sample());
echo $vars["visible"], "\n";
var_dump(isset($vars["hidden"]));
"#,
        "support_builtin_namespace",
    );
    assert_eq!(
        output,
        "2147483647\n3\nY-m-d\\TH:i:sP\n7\nbool(false)\n",
    );
}

/// Verifies `get_object_vars` includes private/protected members only inside an allowed scope.
#[test]
fn get_object_vars_honors_current_scope_visibility() {
    let output = compile_and_run(
        r#"<?php
class VisibilitySample {
    public int $publicValue = 1;
    protected int $protectedValue = 2;
    private int $privateValue = 3;

    public function inside(): array {
        return get_object_vars($this);
    }
}
$sample = new VisibilitySample();
$outside = get_object_vars($sample);
$inside = $sample->inside();
echo $outside["publicValue"], "/", sizeof($outside), "\n";
echo $inside["publicValue"], "/", $inside["protectedValue"], "/", $inside["privateValue"], "\n";
"#,
        "get_object_vars_visibility",
    );
    assert_eq!(output, "1/1\n1/2/3\n");
}

/// Verifies virtual ext/date properties are materialized through their synthetic getters.
#[test]
fn get_object_vars_reads_datetime_virtual_properties() {
    let output = compile_and_run(
        r#"<?php
$left = new DateTimeImmutable("2020-01-01 00:00:00.100000", new DateTimeZone("UTC"));
$right = new DateTimeImmutable("2020-01-02 00:00:00.350000", new DateTimeZone("UTC"));
$interval = $left->diff($right);
$vars = get_object_vars($interval);
var_dump($vars["f"] === $interval->f);
echo $vars["d"], "/", $vars["invert"], "\n";
"#,
        "get_object_vars_datetime",
    );
    assert_eq!(output, "bool(true)\n1/0\n");
}

/// Verifies a runtime name selects different prescanned date constants inside a loop.
#[test]
fn constant_selects_dynamic_date_constant_names() {
    let output = compile_and_run(
        r#"<?php
foreach (["DATE_ATOM", "DATE_RFC7231", "DATE_RSS"] as $name) {
    echo constant($name), "\n";
}
"#,
        "dynamic_date_constants",
    );
    assert_eq!(
        output,
        "Y-m-d\\TH:i:sP\nD, d M Y H:i:s \\G\\M\\T\nD, d M Y H:i:s O\n",
    );
}

/// Verifies the checker rejects a non-object receiver with the builtin-specific diagnostic.
#[test]
fn get_object_vars_rejects_non_object_receiver() {
    let dir = make_test_dir("get_object_vars_error");
    let (compile, _) = compile_source(&dir, "<?php get_object_vars(42);", "app");
    assert!(!compile.status.success(), "invalid receiver unexpectedly compiled");
    let stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(
        stderr.contains("get_object_vars() argument must be an object"),
        "unexpected diagnostic:\n{stderr}",
    );
}

/// Verifies explicit cycle collection returns the number of blocks reclaimed by that pass.
#[test]
fn gc_collect_cycles_returns_a_native_collection_count() {
    let output = compile_and_run(
        r#"<?php
gc_enable();
echo gc_collect_cycles(), "\n";
"#,
        "gc_collect_cycles_count",
    );
    assert_eq!(output, "0\n");
}

/// Verifies `end` reads indexed and associative final values and returns false for emptiness.
#[test]
fn end_selects_the_final_array_value() {
    let output = compile_and_run(
        r#"<?php
$indexed = [10, 20, 30];
$assoc = ["first" => 1, "last" => 2];
$empty = [];
var_dump(\EnD($indexed));
var_dump(end($assoc));
var_dump(end($empty));
"#,
        "end_array_values",
    );
    assert_eq!(output, "int(30)\nint(2)\nbool(false)\n");
}

/// Verifies DatePeriod's object-to-array handler exposes its public and virtual properties.
#[test]
fn date_period_array_cast_materializes_virtual_properties() {
    let output = compile_and_run(
        r#"<?php
class CastPeriod extends DatePeriod {
    public int $prop = 3;
}
$period = CastPeriod::createFromISO8601String("R2/2000-01-01T00:00:00Z/P1D");
$vars = (array) $period;
echo $vars["prop"], "/", $vars["recurrences"], "/", sizeof($vars), "\n";
"#,
        "date_period_array_cast",
    );
    assert_eq!(output, "3/3/8\n");
}

/// Verifies DatePeriod JSON and var-export consumers use the same virtual-property projection.
#[test]
fn date_period_json_and_var_export_use_virtual_properties() {
    let output = compile_and_run(
        r#"<?php
class JsonPeriod extends DatePeriod {
    public int $prop = 3;
}
$period = JsonPeriod::createFromISO8601String("R2/2000-01-01T00:00:00Z/P1D");
echo json_encode($period), "\n";
$export = var_export($period, true);
var_dump(str_contains($export, "'prop' => 3,"));
"#,
        "date_period_json_var_export",
    );
    assert_eq!(
        output,
        "{\"prop\":3,\"start\":{\"date\":\"2000-01-01 00:00:00.000000\",\"timezone_type\":1,\"timezone\":\"+00:00\"},\"current\":null,\"end\":null,\"interval\":{\"y\":0,\"m\":0,\"d\":1,\"h\":0,\"i\":0,\"s\":0,\"f\":0,\"invert\":0,\"days\":false,\"from_string\":false},\"recurrences\":3,\"include_start_date\":true,\"include_end_date\":false}\nbool(true)\n",
    );
}

/// Verifies hidden var_dump runtime calls cannot clobber later variadic scalar operands.
#[test]
fn var_dump_preserves_late_operands_after_object_and_float_arguments() {
    let output = compile_and_run(
        r#"<?php
$interval = new DateInterval("P1D");
var_dump($interval, 0, 0, 0, 0, 0, 0, 0.5, 7, false);
"#,
        "var_dump_many_mixed_arguments",
    );
    assert!(
        output.ends_with("float(0.5)\nint(7)\nbool(false)\n"),
        "unexpected var_dump tail:\n{output}",
    );
}

/// Verifies user `__debugInfo()` overrides inherited ext/date debug renderers.
#[test]
fn var_dump_calls_debug_info_on_date_subclasses() {
    let output = compile_and_run(
        r#"<?php
class UDateTime extends DateTime { public function __construct() {} public function __debugInfo(): array { return ['value' => 'zzz']; } }
class UDateTimeImmutable extends DateTimeImmutable { public function __construct() {} public function __debugInfo(): array { return ['value' => 'zzz']; } }
class UDateTimeZone extends DateTimeZone { public function __construct() {} public function __debugInfo(): array { return ['value' => 'zzz']; } }
class UDateInterval extends DateInterval { public function __construct() {} public function __debugInfo(): array { return ['value' => 'zzz']; } }
class UDatePeriod extends DatePeriod { public function __construct() {} public function __debugInfo(): array { return ['value' => 'zzz']; } }
var_dump(new UDateTime());
var_dump(new UDateTimeImmutable());
var_dump(new UDateTimeZone('UTC'));
var_dump(new UDateInterval('P1D'));
var_dump(UDatePeriod::createFromISO8601String('R2/2000-01-01T00:00:00Z/P1D'));
"#,
        "var_dump_date_subclass_debug_info",
    );
    assert_eq!(
        output.matches("[\"value\"]=>").count(),
        5,
        "every date subclass must expose its user debug projection:\n{output}",
    );
    assert_eq!(
        output.matches("string(3) \"zzz\"").count(),
        5,
        "every date subclass must render the debug value:\n{output}",
    );
}

/// Replays php-src bug #80047: DatePeriod yields base-class snapshots for custom date starts.
#[test]
fn date_period_iterator_canonicalizes_custom_date_classes() {
    let output = compile_and_run(
        r#"<?php
class CustomDateTime extends DateTime {}
class CustomDateTimeImmutable extends DateTimeImmutable {}

$dt = new DateTime('2022-06-24');
$dti = new DateTimeImmutable('2022-06-24');
$cdt = new CustomDateTime('2022-06-25');
$cdti = new CustomDateTimeImmutable('2022-06-25');
$i = new DateInterval('P1D');
$tests = [
    [$dt, $i, $cdt],
    [$cdt, $i, $dt],
    [$cdt, $i, $cdt],
    [$dti, $i, $cdti],
    [$cdti, $i, $dti],
    [$cdti, $i, $cdti],
    [$cdt, $i, $cdti],
];
foreach ($tests as $test) {
    $period = new DatePeriod(...$test);
    foreach ($period as $date) {}
    echo get_class($date), "\n";
}
"#,
        "date_period_custom_date_classes",
    );
    assert_eq!(
        output,
        "DateTime\nDateTime\nDateTime\nDateTimeImmutable\nDateTimeImmutable\nDateTimeImmutable\nDateTimeImmutable\n",
    );
}

/// Verifies `@` suppresses both DatePeriod string-overload deprecations for a null argument.
#[test]
fn date_period_null_string_deprecations_are_suppressible() {
    let output = compile_and_run(
        r#"<?php
try {
    @new DatePeriod(null);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
"#,
        "date_period_null_deprecation_suppression",
    );
    assert_eq!(output, "Unknown or bad format ()\n");
}
