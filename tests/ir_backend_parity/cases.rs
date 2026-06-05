//! Purpose:
//! Focused parity cases that compare legacy AST backend output with EIR backend output.
//!
//! Called from:
//! - `tests/ir_backend_parity.rs` as an integration-test module.
//!
//! Key details:
//! - Each case compiles the same PHP snippet twice at the binary level, once
//!   through the default backend and once through `--ir-backend`.

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
        if matches!(self, Self::Ir) {
            command.arg("--ir-backend");
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

/// Compiles and runs a PHP snippet through both backends and compares stdout.
fn assert_backend_parity(name: &str, source: &str, args: &[&str]) {
    let legacy = compile_and_run_backend(name, source, args, Backend::Legacy);
    let ir = compile_and_run_backend(name, source, args, Backend::Ir);
    assert_eq!(ir, legacy, "IR backend stdout differed from legacy for {name}");
}

/// Compiles a PHP snippet with one backend, runs the produced binary, and returns stdout.
fn compile_and_run_backend(name: &str, source: &str, args: &[&str], backend: Backend) -> String {
    let dir = temp_case_dir(name, backend);
    fs::create_dir_all(&dir).expect("failed to create parity test directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write parity PHP fixture");

    let mut compile_command = Command::new(elephc_cli_bin());
    compile_command
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir);
    backend.add_compile_flags(&mut compile_command);
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
    let _ = fs::remove_dir_all(&dir);
    stdout
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
