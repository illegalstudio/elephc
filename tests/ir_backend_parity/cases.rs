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
