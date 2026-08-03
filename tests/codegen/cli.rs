//! Purpose:
//! Integration coverage for top-level compile/native dispatch and compiler output modes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Native help and managed-PCRE2 recovery diagnostics are exercised through subprocesses.
//! - Non-link modes must remain independent of installed native artifacts.
//! - Inline PHP fixtures are compiled to native binaries or wasm32-wasi modules,
//!   and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies compiler-version output is exact, successful, and independent of a source file.
#[test]
fn test_cli_version_reports_cargo_package_version() {
    let dir = make_cli_test_dir("elephc_cli_version");

    for flag in ["--version", "-V"] {
        let output = elephc_cli_command(&dir)
            .arg(flag)
            .output()
            .expect("failed to run elephc version command");
        assert!(output.status.success(), "{flag} should succeed");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("elephc {}\n", env!("CARGO_PKG_VERSION")),
            "unexpected {flag} stdout"
        );
        assert!(output.stderr.is_empty(), "unexpected {flag} stderr");
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies native help is handled before project discovery and bare native is a usage error.
#[test]
fn test_cli_native_help_and_bare_usage() {
    let dir = make_cli_test_dir("elephc_cli_native_help");

    let help = elephc_cli_command(&dir)
        .args(["native", "--help"])
        .output()
        .expect("failed to run elephc native --help");
    assert!(help.status.success(), "native help should succeed");
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("elephc native add"),
        "native help should print the command synopsis"
    );
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("elephc native prune"),
        "native help should include explicit cache pruning"
    );

    let bare = elephc_cli_command(&dir)
        .arg("native")
        .output()
        .expect("failed to run bare elephc native");
    assert!(!bare.status.success(), "bare native should be a usage error");
    let stderr = String::from_utf8_lossy(&bare.stderr);
    assert!(stderr.contains("missing native command"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("elephc native install"), "missing synopsis: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies read-only native commands preserve their captured stdout and health exit status.
#[test]
fn test_cli_native_read_only_commands_map_output_and_status() {
    let dir = make_cli_test_dir("elephc_cli_native_read_only");
    let cache = dir.join("native-cache-must-not-exist");

    let list = elephc_cli_command(&dir)
        .args(["native", "list"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run elephc native list");
    assert!(list.status.success(), "empty native list should succeed");
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("no native dependencies"),
        "unexpected list output: {}",
        String::from_utf8_lossy(&list.stdout)
    );

    let doctor = elephc_cli_command(&dir)
        .args(["native", "doctor"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run elephc native doctor");
    assert!(!doctor.status.success(), "doctor without a project should be unhealthy");
    assert!(
        String::from_utf8_lossy(&doctor.stdout).contains("summary: unhealthy")
            && String::from_utf8_lossy(&doctor.stdout).contains("cache size:")
            && String::from_utf8_lossy(&doctor.stdout).contains("stale staging summary:"),
        "unexpected doctor output: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );
    assert!(!cache.exists(), "read-only commands must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies explicit pruning is a successful no-op when no global native cache exists.
#[test]
fn test_cli_native_prune_empty_cache_is_noop() {
    let dir = make_cli_test_dir("elephc_cli_native_prune_empty");
    let cache = dir.join("native-cache-must-not-exist");
    let prune = elephc_cli_command(&dir)
        .args(["native", "prune"])
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run elephc native prune");
    assert!(prune.status.success(), "empty-cache prune should succeed");
    assert!(
        String::from_utf8_lossy(&prune.stdout).contains("removed stale artifacts: 0"),
        "unexpected prune output: {}",
        String::from_utf8_lossy(&prune.stdout)
    );
    assert!(!cache.exists(), "empty-cache prune must not create cache state");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies non-link output modes never require or create a managed native cache.
#[test]
fn test_cli_regex_non_link_modes_skip_native_resolution() {
    for mode in ["--check", "--emit-ir", "--emit-asm"] {
        let dir = make_cli_test_dir("elephc_cli_regex_non_link");
        let cache = dir.join("native-cache-must-not-exist");
        let php_path = dir.join("main.php");
        fs::write(&php_path, "<?php echo preg_match('/a/', 'a');").unwrap();

        let output = elephc_cli_command(&dir)
            .arg(mode)
            .arg(&php_path)
            .env("ELEPHC_NATIVE_CACHE", &cache)
            .output()
            .unwrap_or_else(|error| panic!("failed to run elephc {mode}: {error}"));
        assert!(
            output.status.success(),
            "elephc {mode} unexpectedly required native PCRE2: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !cache.exists(),
            "elephc {mode} must not create the managed native cache"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

/// Verifies a final regex link without a project fails with the frozen recovery command.
#[test]
fn test_cli_regex_final_link_requires_managed_pcre2_project() {
    let dir = make_cli_test_dir("elephc_cli_regex_requires_native");
    let cache = dir.join("native-cache-must-not-exist");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo preg_match('/a/', 'a');").unwrap();

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run final-link regex compilation");
    assert!(!output.status.success(), "regex link without a project must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("regex support requires managed native package pcre2"),
        "unexpected missing-project diagnostic: {stderr}"
    );
    assert!(stderr.contains("project: not found"), "missing project context: {stderr}");
    assert!(
        stderr.contains("recovery: cd --") && stderr.contains("elephc native add pcre2"),
        "missing copy-paste recovery command: {stderr}"
    );
    assert!(!cache.exists(), "failed compilation must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--check` stops after type-checking and produces "Checked" output
/// without emitting any assembly (.s), object (.o), or binary files.
#[test]
fn test_cli_check_stops_after_typecheck() {
    let dir = make_cli_test_dir("elephc_cli_check");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo "ok";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--check")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --check");

    assert!(
        output.status.success(),
        "elephc --check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Checked"),
        "expected --check success output, got stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !dir.join("main.s").exists(),
        "--check should not emit assembly files"
    );
    assert!(
        !dir.join("main.o").exists(),
        "--check should not emit object files"
    );
    assert!(
        !dir.join("main").exists(),
        "--check should not emit binaries"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--emit-asm` writes a .s assembly file containing the `_main` label
/// but does NOT produce object or binary files.
#[test]
fn test_cli_emit_asm_writes_assembly_only() {
    let dir = make_cli_test_dir("elephc_cli_emit_asm");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo "ok";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-asm");

    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Emitted assembly"),
        "expected --emit-asm success output, got stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );

    let asm_path = dir.join("main.s");
    assert!(asm_path.exists(), "--emit-asm should write the .s file");
    let asm = fs::read_to_string(&asm_path).expect("failed to read emitted assembly");
    assert!(
        asm.contains("_main"),
        "expected emitted assembly to contain the program entry label"
    );
    assert!(
        !dir.join("main.o").exists(),
        "--emit-asm should not emit object files"
    );
    assert!(
        !dir.join("main").exists(),
        "--emit-asm should not emit binaries"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies plain `--web` assembly keeps the compact auto-start core while
/// pruning public session APIs and callable-handler machinery that user code
/// does not reference.
#[test]
fn test_cli_web_prunes_unused_session_surface_from_assembly() {
    let dir = make_cli_test_dir("elephc_cli_web_pruned_prelude");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--web")
        .arg(&php_path)
        .output()
        .expect("failed to compile pruned web program");
    assert!(
        output.status.success(),
        "elephc --web failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read web assembly");
    assert!(
        asm.contains("_fn__u__u_elephc_u_session_u_start_u_core"),
        "plain web assembly must retain the auto-start session core"
    );
    assert!(
        !asm.contains(".globl _fn_session_u_start\n"),
        "plain web assembly must not emit the public option-heavy session_start wrapper"
    );
    assert!(
        !asm.contains("_fn_session_u_set_u_save_u_handler"),
        "plain web assembly must not emit session_set_save_handler"
    );
    assert!(
        !asm.contains("__ElephcCallableSessionHandler"),
        "plain web assembly must not emit legacy callable-handler dispatch"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies repeated boxed-Mixed callable sites reuse module-wide descriptor
/// wrappers instead of regenerating the full candidate set in every function.
#[test]
fn test_cli_runtime_callable_descriptors_are_shared_across_call_sites() {
    let dir = make_cli_test_dir("elephc_cli_callable_descriptor_dedup");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class InvokableTarget { public function __invoke(int $value): int { return $value + 1; } }
function first(mixed $callback): mixed { return call_user_func($callback, 1); }
function second(mixed $callback): mixed { return call_user_func($callback, 2); }
function plus_one(int $value): int { return $value + 1; }
echo first('plus_one');
echo second('plus_one');
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile callable dedup fixture");
    assert!(
        output.status.success(),
        "callable dedup fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read callable assembly");
    assert!(
        asm.contains("_eir_first_callable_invoker"),
        "the first dynamic call site must emit shared invokers"
    );
    assert!(
        !asm.contains("_eir_second_callable_invoker"),
        "the second equivalent call site must reuse the first site's invokers"
    );

    let run = run_binary(&dir.join("main"), &dir);
    assert!(
        run.status.success(),
        "callable dedup fixture failed at runtime: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "23");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--emit-ir` prints textual EIR and stops before assembly, object,
/// or binary emission.
#[test]
fn test_cli_emit_ir_prints_eir_only() {
    let dir = make_cli_test_dir("elephc_cli_emit_ir");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function greet(): int {
    return 7;
}
echo greet();
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-ir");

    assert!(
        output.status.success(),
        "elephc --emit-ir failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("module target="), "missing module header: {stdout}");
    assert!(stdout.contains("function greet"), "missing lowered function: {stdout}");
    assert!(stdout.contains("const_i64 7"), "missing lowered return literal: {stdout}");
    assert!(stdout.contains("function main"), "missing lowered main function: {stdout}");
    assert!(
        !dir.join("main.s").exists(),
        "--emit-ir should not emit assembly files"
    );
    assert!(
        !dir.join("main.o").exists(),
        "--emit-ir should not emit object files"
    );
    assert!(
        !dir.join("main").exists(),
        "--emit-ir should not emit binaries"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that passing `--emit-asm` and `--check` together fails with a
/// "mutually exclusive" error message.
#[test]
fn test_cli_rejects_emit_asm_and_check_together() {
    let dir = make_cli_test_dir("elephc_cli_flag_conflict");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--check")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with conflicting flags");

    assert!(
        !output.status.success(),
        "expected conflicting flags to fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("mutually exclusive"),
        "expected conflict message, got stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--emit-ir` participates in the same exclusive output-mode group
/// as `--emit-asm` and `--check`.
#[test]
fn test_cli_rejects_emit_ir_output_mode_conflicts() {
    let dir = make_cli_test_dir("elephc_cli_emit_ir_flag_conflict");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    for conflicting_flag in ["--emit-asm", "--check"] {
        let output = elephc_cli_command(&dir)
            .arg("--emit-ir")
            .arg(conflicting_flag)
            .arg(&php_path)
            .output()
            .expect("failed to run elephc CLI with conflicting --emit-ir flag");

        assert!(
            !output.status.success(),
            "expected --emit-ir {conflicting_flag} to fail"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("mutually exclusive"),
            "expected conflict message, got stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--check --timings` renders the frontend phase table without
/// reporting code generation, assembly, or linking phases.
#[test]
fn test_cli_timings_reports_check_phases() {
    let dir = make_cli_test_dir("elephc_cli_timings_check");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--check")
        .arg("--timings")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --timings --check");

    assert!(
        output.status.success(),
        "elephc --timings --check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Compiler timings"), "missing timings header: {stderr}");
    assert!(stderr.contains("Tokenizing source"), "missing tokenize timing: {stderr}");
    assert!(stderr.contains("Parsing program"), "missing parse timing: {stderr}");
    assert!(stderr.contains("Checking types"), "missing typecheck timing: {stderr}");
    assert!(stderr.contains("Total"), "missing total timing: {stderr}");
    assert!(
        !stderr.contains("Generating native code"),
        "unexpected codegen timing in --check output: {stderr}"
    );
    assert!(
        !stderr.contains("Assembling object file"),
        "unexpected assemble timing in --check output: {stderr}"
    );
    assert!(
        !stderr.contains("Linking native output"),
        "unexpected link timing in --check output: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--timings` renders the native build phases and total duration
/// when compiling a full binary, and that the binary is emitted.
#[test]
fn test_cli_timings_reports_assemble_and_link() {
    let dir = make_cli_test_dir("elephc_cli_timings_build");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--timings")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --timings");

    assert!(
        output.status.success(),
        "elephc --timings failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Generating native code"),
        "missing codegen timing: {stderr}"
    );
    assert!(
        stderr.contains("Assembling object file"),
        "missing assemble timing: {stderr}"
    );
    assert!(
        stderr.contains("Linking native output"),
        "missing link timing: {stderr}"
    );
    assert!(stderr.contains("Total"), "missing total timing: {stderr}");
    assert!(dir.join("main").exists(), "expected compiled binary to exist");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the timing report records `Runtime cache: miss` for the first
/// compile and `Runtime cache: hit` for the second without rebuilding it.
#[test]
fn test_cli_runtime_cache_reuses_runtime_object() {
    let dir = make_cli_test_dir("elephc_cli_runtime_cache");
    let cache_root = dir.join("cache-root");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let first = Command::new(elephc_cli_bin())
        .arg("--timings")
        .arg(&php_path)
        .env("XDG_CACHE_HOME", &cache_root)
        .current_dir(&dir)
        .output()
        .expect("failed to run first elephc CLI compile with runtime cache");
    assert!(
        first.status.success(),
        "first cached compile failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stderr = String::from_utf8_lossy(&first.stderr);
    assert!(
        first_stderr.contains("Notes"),
        "expected timing notes after first compile, got stderr={first_stderr}"
    );
    assert!(
        first_stderr.contains("Runtime cache: miss"),
        "expected first compile to miss runtime cache, got stderr={first_stderr}"
    );

    let cache_dir = cache_root.join("elephc");
    let cached_objects: Vec<_> = fs::read_dir(&cache_dir)
        .expect("expected runtime cache directory to exist")
        .map(|entry| entry.expect("cache entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("o"))
        .collect();
    assert_eq!(
        cached_objects.len(),
        1,
        "expected exactly one cached runtime object, got {:?}",
        cached_objects
    );

    let second = Command::new(elephc_cli_bin())
        .arg("--timings")
        .arg(&php_path)
        .env("XDG_CACHE_HOME", &cache_root)
        .current_dir(&dir)
        .output()
        .expect("failed to run second elephc CLI compile with runtime cache");
    assert!(
        second.status.success(),
        "second cached compile failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        second_stderr.contains("Notes"),
        "expected timing notes after second compile, got stderr={second_stderr}"
    );
    assert!(
        second_stderr.contains("Runtime cache: hit"),
        "expected second compile to hit runtime cache, got stderr={second_stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--source-map` emits a sidecar .map file in the v2 schema:
/// versioned envelope, function ranges (user function + main), labels, and
/// opcode-tagged line mappings.
#[test]
fn test_cli_source_map_writes_sidecar_file() {
    let dir = make_cli_test_dir("elephc_cli_source_map");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function foo(int $x): int {
    return $x + 1;
}
echo foo(1);
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--source-map")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --source-map");

    assert!(
        output.status.success(),
        "elephc --source-map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let map_path = dir.join("main.map");
    assert!(map_path.exists(), "expected source map sidecar to exist");
    let map = fs::read_to_string(&map_path).expect("failed to read source map");
    assert!(
        map.contains("\"format\": \"elephc-source-map\""),
        "missing source map format header: {map}"
    );
    assert!(
        map.contains("\"version\": 2"),
        "missing source map schema version: {map}"
    );
    assert!(
        map.contains("\"asm\":"),
        "expected source map to record the asm path: {map}"
    );
    assert!(
        map.contains("\"name\": \"foo\""),
        "expected a function entry for foo: {map}"
    );
    assert!(
        map.contains("\"name\": \"main\""),
        "expected a function entry for main: {map}"
    );
    assert!(
        map.contains("\"php_line\": 3"),
        "expected a mapping for the return on PHP line 3: {map}"
    );
    assert!(
        map.contains("\"op\": \""),
        "expected opcode-tagged mappings: {map}"
    );
    assert!(
        map.contains("\"labels\": ["),
        "expected a labels section: {map}"
    );
    assert!(
        map.contains("\"source_sha256\": \""),
        "expected a source checksum: {map}"
    );
    assert!(
        map.contains("\"synthetic\": true") && map.contains("\"synthetic\": false"),
        "expected both user and synthetic function entries: {map}"
    );
    assert!(
        map.contains("\"block\": \"entry\""),
        "expected an entry-block label annotation: {map}"
    );
    assert!(
        map.contains("\"php_end_col\":"),
        "expected expression end positions in mappings: {map}"
    );
    assert!(
        map.contains("\"lines\": ["),
        "expected the PHP-line inverse index: {map}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies native-only environment defaults do not make a WASM build fail as
/// though the user had explicitly passed `--null-repr` or `--regalloc`.
#[test]
fn test_cli_wasm_ignores_native_codegen_environment_defaults() {
    let dir = make_cli_test_dir("elephc_cli_wasm_native_env_defaults");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;\n").unwrap();

    let output = elephc_cli_command(&dir)
        .env("ELEPHC_NULL_REPR", "tagged")
        .env("ELEPHC_REGALLOC", "stack")
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with native-only environment defaults");

    assert!(
        output.status.success(),
        "native-only environment defaults must not reject WASM: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        dir.join("main.wat").exists(),
        "WASM --emit-asm should publish main.wat"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles integer and boolean concatenation through PHP -> EIR -> WASM for
/// every supported PHP profile and verifies `IToStr`, including both signed
/// i64 edges, matches PHP output.
#[test]
fn test_cli_wasm_integer_and_boolean_string_coercion_matches_php_profiles() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_int_bool_string_coercion");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function integer_value(int $value): int {
    return $value;
}
function boolean_value(bool $value): bool {
    return $value;
}
$integer = integer_value(-42);
$other_integer = integer_value(123);
$minimum = integer_value(PHP_INT_MIN);
$maximum = integer_value(PHP_INT_MAX);
$false = boolean_value(false);
$true = boolean_value(true);
echo $integer . $other_integer . "|" . $true . $false . "|" . $minimum . ":" . $maximum;
"#,
    )
    .unwrap();

    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile integer/boolean string coercion to WASM");
        assert!(
            output.status.success(),
            "PHP {version} integer/boolean coercion compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let run = Command::new("wasmer")
            .arg("run")
            .arg(dir.join("main.wasm"))
            .current_dir(&dir)
            .output()
            .expect("failed to run integer/boolean string coercion under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} integer/boolean coercion trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "-42123|1|-9223372036854775808:9223372036854775807"
        );
        assert!(
            run.stderr.is_empty(),
            "PHP {version}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles exact strict scalar/string/object equality through the public PHP
/// frontend for every supported compiler profile and verifies raw execution.
///
/// This is target execution coverage; the pinned php-src differential oracle is
/// a separate W1 gate and must not be inferred from this hand-authored matrix.
#[test]
fn test_cli_wasm_strict_equality_executes_supported_profiles() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_strict_equality");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function integer_value(int $value): int {
    return $value;
}
function boolean_value(bool $value): bool {
    return $value;
}
function float_value(float $value): float {
    return $value;
}
function string_value(string $value): string {
    return $value;
}
class ChildValue {}

$one = integer_value(1);
$two = integer_value(2);
$true = boolean_value(true);
$false = boolean_value(false);
$nan = float_value(NAN);
$positiveZero = float_value(0.0);
$negativeZero = float_value(-0.0);
$empty = string_value("");
$binary = string_value("a\0b\xFF");
$sameBinary = string_value("a\0b\xFF");
$prefix = string_value("a\0b");
$differentBinary = string_value("a\0c\xFF");
$object = new ChildValue();
$otherObject = new ChildValue();
$null = null;

echo $one === integer_value(1); echo ",";
echo $one !== $two; echo ",";
echo $true === boolean_value(true); echo ",";
echo $true !== $false; echo ",";
echo $one !== string_value("1"); echo ",";
echo $nan !== $nan; echo ",";
echo $positiveZero === $negativeZero; echo ",";
echo $empty === string_value(""); echo ",";
echo $binary === $sameBinary; echo ",";
echo $binary !== $prefix; echo ",";
echo $binary !== $differentBinary; echo ",";
echo $object !== $null; echo ",";
echo $object !== $otherObject; echo ",";
echo $object === $object; echo ",";
echo $null === null; echo ",";
echo $false !== $null; echo ",";
echo $one === $two; echo ",";
echo $one !== integer_value(1); echo ",";
echo $nan === $nan; echo ",";
echo $positiveZero !== $negativeZero; echo ",";
echo $binary !== $sameBinary; echo ",";
echo $object !== $object; echo ",";
echo $one === string_value("1"); echo ",";
echo match ("need" . string_value("le")) {
    "needle" . string_value("") => true,
    default => false,
}; echo ",";
echo match ("need" . string_value("le")) {
    "other" . string_value("") => false,
    default => true,
}; echo ",";
echo match (new ChildValue()) {
    new ChildValue() => false,
    default => true,
}; echo ",";
"#,
    )
    .unwrap();

    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile strict equality to WASM");
        assert!(
            output.status.success(),
            "PHP {version} strict equality compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let run = Command::new("wasmer")
            .arg("run")
            .arg(dir.join("main.wasm"))
            .current_dir(&dir)
            .output()
            .expect("failed to run strict equality under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} strict equality trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let expected = format!("{}{}{}", "1,".repeat(16), ",".repeat(7), "1,1,1,");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
        assert!(
            run.stderr.is_empty(),
            "PHP {version}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles the php-src saturated-array append edge through PHP -> EIR -> WASM
/// and verifies the command runtime reports the exact failure instead of wrapping
/// `PHP_INT_MAX` to a negative key or surfacing an unclassified Wasm trap.
#[test]
fn test_cli_wasm_append_at_occupied_php_int_max_fails_like_php() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_append_php_int_max");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$a = [PHP_INT_MAX => 1];
$a[] = 2;
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile saturated hash append to WASM");
    assert!(
        output.status.success(),
        "saturated hash append compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wasm_path = dir.join("main.wasm");
    assert!(wasm_path.exists(), "WASM compilation must publish main.wasm");
    let run = Command::new("wasmer")
        .arg("run")
        .arg(&wasm_path)
        .current_dir(&dir)
        .output()
        .expect("failed to run saturated hash append under Wasmer");
    assert_eq!(run.status.code(), Some(255));
    assert!(run.stdout.is_empty(), "fatal append must not write stdout");
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "PHP Fatal error: Uncaught Error: Cannot add element to the array as the next element is already occupied\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles the exact php-src next-free origin split through PHP -> EIR -> WASM
/// for every supported compatibility profile. PHP 8.2 promotes immutable `[]`
/// with next=0, while a direct mutable `[-3 => 1]` starts at LONG_MIN; PHP
/// 8.3-8.5 start both empty-literal and mutable paths at LONG_MIN.
#[test]
fn test_cli_wasm_empty_promotion_and_direct_hash_match_php_profiles() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_hash_next_origin");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$a = [];
$a[-3] = 1;
$a[] = 2;
echo "empty:";
foreach ($a as $key => $value) {
    echo $key, ",";
}

$b = [-3 => 1];
$b[] = 2;
echo "|literal:";
foreach ($b as $key => $value) {
    echo $key, ",";
}
"#,
    )
    .unwrap();

    for (version, expected) in [
        ("8.2", "empty:-3,0,|literal:-3,-2,"),
        ("8.3", "empty:-3,-2,|literal:-3,-2,"),
        ("8.4", "empty:-3,-2,|literal:-3,-2,"),
        ("8.5", "empty:-3,-2,|literal:-3,-2,"),
    ] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile hash next-free profile fixture to WASM");
        assert!(
            output.status.success(),
            "PHP {version} hash-origin compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let wasm_path = dir.join("main.wasm");
        assert!(wasm_path.exists(), "WASM compilation must publish main.wasm");
        let run = Command::new("wasmer")
            .arg("run")
            .arg(&wasm_path)
            .current_dir(&dir)
            .output()
            .expect("failed to run hash-origin fixture under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} hash-origin fixture trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            expected,
            "PHP {version}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles a PHP-source closure stored in a local, invokes it with one argument
/// through the non-empty Mixed argument-buffer path, and checks exact output.
#[test]
fn test_cli_wasm_dynamic_closure_argument_prints_42() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_dynamic_closure_42");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$f = function(int $x): int { return $x; };
echo $f(42);
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile dynamic closure call to WASM");
    assert!(
        output.status.success(),
        "dynamic closure compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wasm_path = dir.join("main.wasm");
    assert!(wasm_path.exists(), "WASM compilation must publish main.wasm");
    let run = Command::new("wasmer")
        .arg("run")
        .arg(&wasm_path)
        .current_dir(&dir)
        .output()
        .expect("failed to run dynamic closure call under Wasmer");
    assert!(
        run.status.success(),
        "dynamic closure call trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42");
    assert!(run.stderr.is_empty(), "unexpected stderr: {:?}", run.stderr);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a Mixed receiver dispatches directly to the selected covariant
/// override instead of imposing another implementation's WASM return ABI.
#[test]
fn test_cli_wasm_mixed_virtual_covariant_return_uses_exact_implementation_abi() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_covariant_return");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class A {
    public function run(): mixed { return 1; }
}
class B extends A {
    public function run(): string { return "x"; }
}
function invoke_mixed(mixed $value): mixed {
    return $value->run();
}
echo invoke_mixed(new B());
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile covariant Mixed dispatch to WASM");
    assert!(
        output.status.success(),
        "covariant Mixed dispatch compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wasm_path = dir.join("main.wasm");
    assert!(wasm_path.exists(), "WASM compilation must publish main.wasm");
    let run = Command::new("wasmer")
        .arg("run")
        .arg(&wasm_path)
        .current_dir(&dir)
        .output()
        .expect("failed to run covariant Mixed dispatch under Wasmer");
    assert!(
        run.status.success(),
        "covariant Mixed dispatch failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "x");
    assert!(run.stderr.is_empty(), "unexpected stderr: {:?}", run.stderr);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies heterogeneous dynamic method returns box PHP void as null and
/// transfer callable ownership into the result cell without leaking the
/// callee-owned descriptor.
#[test]
fn test_cli_wasm_mixed_method_void_and_callable_returns_are_balanced() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_void_callable_return");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class VoidResult {
    public function run(): void {}
}
class CallableResult {
    public function run(): callable {
        return function(): int { return 42; };
    }
}
function invoke_mixed(mixed $value): mixed {
    return $value->run();
}
echo is_null(invoke_mixed(new VoidResult())), ";";
$callable = invoke_mixed(new CallableResult());
echo is_null($callable), ";";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile void/callable Mixed dispatch to WASM");
    assert!(
        output.status.success(),
        "void/callable Mixed dispatch compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wasm_path = dir.join("main.wasm");
    let run = Command::new("wasmer")
        .arg("run")
        .arg(&wasm_path)
        .current_dir(&dir)
        .output()
        .expect("failed to run void/callable Mixed dispatch under Wasmer");
    assert!(
        run.status.success(),
        "void/callable Mixed dispatch failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1;;");
    assert!(run.stderr.is_empty(), "unexpected stderr: {:?}", run.stderr);

    let emit = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to emit void/callable Mixed dispatch WAT");
    assert!(
        emit.status.success(),
        "void/callable Mixed WAT emission failed: {}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let wat = fs::read_to_string(dir.join("main.wat")).expect("read emitted WAT");
    assert!(
        wat.contains("box null (void callee, mixed result)"),
        "void return did not materialize Mixed(null): {wat}"
    );
    let callable_source = wat
        .find("callee-owned callable descriptor")
        .expect("callable source ownership marker");
    let callable_release = wat[callable_source..]
        .find("call $__rt_decref_any")
        .map(|offset| callable_source + offset)
        .expect("callable source release");
    assert!(
        callable_release > callable_source,
        "callable source must be released after result-cell boxing: {wat}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles an escaping by-ref closure from PHP source to wasm32-wasi and runs it
/// twice under Wasmer. The creator's frame is gone before either call, so two
/// successful writes and reads prove the closure owns the ref cell.
#[test]
fn test_cli_wasm_escaping_by_ref_closure_survives_creator_return() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_escaping_ref_closure");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function make() {
    $x = 0;
    return function() use (&$x) {
        $x = $x ? 3 : 2;
        return $x;
    };
}

$f = make();
echo $f(), $f();
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile escaping by-ref closure to WASM");
    assert!(
        output.status.success(),
        "escaping by-ref closure compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wasm_path = dir.join("main.wasm");
    assert!(wasm_path.exists(), "WASM compilation must publish main.wasm");
    let run = Command::new("wasmer")
        .arg("run")
        .arg(&wasm_path)
        .current_dir(&dir)
        .output()
        .expect("failed to run escaping by-ref closure under Wasmer");
    assert!(
        run.status.success(),
        "escaping by-ref closure trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "23");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies null coalescing lowers indexed int/bool/string reads through the
/// silent opcode with an explicit null-capable Tagged/Mixed result.
///
/// Full Wasmer execution of `??` remains blocked by the separate unsupported
/// `UnsetLocal` capability; this test proves the EIR boundary does not erase null.
#[test]
fn test_cli_wasm_null_coalesce_array_reads_keep_nullable_eir() {
    let dir = make_cli_test_dir("elephc_cli_wasm_array_coalesce_eir");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo [10][$argc] ?? 77;
echo [true][$argc] ?? 77;
echo ["x"][$argc] ?? 77;
$hash = ["x" => 10];
echo $hash["missing"] ?? 77;
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--emit-ir")
        .arg(&php_path)
        .output()
        .expect("failed to emit WASM-target EIR for null coalescing");
    assert!(
        output.status.success(),
        "WASM-target EIR emission failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let eir = String::from_utf8_lossy(&output.stdout);
    assert!(
        eir.contains("TaggedScalar php=int|null = array_get_silent"),
        "int coalesce read lost nullable TaggedScalar metadata: {eir}"
    );
    assert_eq!(
        eir.matches("Heap(Mixed) php=mixed own=owned = array_get_silent")
            .count(),
        2,
        "bool/string coalesce reads must remain boxed nullable values: {eir}"
    );
    assert!(
        eir.contains("TaggedScalar php=int|null = hash_get_silent"),
        "associative int coalesce read lost nullable TaggedScalar metadata: {eir}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles typed int/bool/string indexed reads from PHP through EIR to WASM and
/// executes them under Wasmer. Negative/OOB reads emit one PHP warning per
/// ordinary access and remain null through `is_null` and `echo`; the former
/// integer sentinel remains a valid in-range value.
#[test]
fn test_cli_wasm_indexed_array_oob_preserves_php_null() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_oob_null");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo is_null([10][-1]), ":", [10][-1], ";";
echo is_null([10][1]), ":", [10][1], ";";
echo is_null([9223372036854775806][0]), ":", [9223372036854775806][0], ";";
echo is_null([true][-1]), ":", [true][-1], ";";
echo is_null([true][1]), ":", [true][1], ";";
echo is_null([""][0]), ":", [""][0], ";";
echo is_null(["x"][-1]), ":", ["x"][-1], ";";
echo is_null(["x"][1]), ":", ["x"][1], ";";
echo (int)[10][-1], ",", (bool)[10][-1], ",", (float)[10][-1], ";";
echo (int)[true][-1], ",", (bool)[true][-1], ";";
echo (int)["x"][-1], ",", (bool)["x"][-1], ",", (string)["x"][-1], ";";
"#,
    )
    .unwrap();

    let warning_keys = [
        -1, -1, 1, 1, -1, -1, 1, 1, -1, -1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
        -1,
    ];
    let expected_stderr = warning_keys
        .iter()
        .map(|key| format!("Warning: Undefined array key {key}"))
        .collect::<Vec<_>>();
    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile indexed-array OOB fixture to WASM");
        assert!(
            output.status.success(),
            "PHP {version} indexed-array OOB compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let wasm_path = dir.join("main.wasm");
        let run = Command::new("wasmer")
            .arg("run")
            .arg(&wasm_path)
            .current_dir(&dir)
            .output()
            .expect("failed to run indexed-array OOB fixture under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} indexed-array OOB fixture trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "1:;1:;:9223372036854775806;1:;1:;:;1:;1:;\
0,,0;0,;0,,;",
            "PHP {version}"
        );
        let actual_stderr = String::from_utf8_lossy(&run.stderr)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            actual_stderr, expected_stderr,
            "PHP {version} ordinary indexed misses must warn exactly once in source order"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Compiles typed associative reads through PHP -> EIR -> WASM. Missing string
/// and integer keys remain PHP null, emit the key-class-specific warning once,
/// and cannot collide with a valid integer equal to the former sentinel.
#[test]
fn test_cli_wasm_hash_reads_preserve_null_and_warn_like_php() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_hash_oob_null");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$ints = ["hit" => 10];
$bools = ["hit" => true];
$floats = ["hit" => 1.5];
$strings = ["hit" => ""];
$sentinel = ["hit" => 9223372036854775806];
$integerKeys = [7 => 10];
echo is_null($ints["missing"]), ":", $ints["hit"], ";";
echo is_null($bools["missing"]), ":", $bools["hit"], ";";
echo is_null($floats["missing"]), ":", $floats["hit"], ";";
echo is_null($strings["missing"]), ":", $strings["hit"], ";";
echo is_null($sentinel["hit"]), ":", $sentinel["hit"], ";";
echo is_null($integerKeys[9]), ":", $integerKeys[7], ";";
"#,
    )
    .unwrap();

    let expected_stderr = [
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key 9",
    ];
    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile associative-read fixture to WASM");
        assert!(
            output.status.success(),
            "PHP {version} associative-read compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let wasm_path = dir.join("main.wasm");
        let run = Command::new("wasmer")
            .arg("run")
            .arg(&wasm_path)
            .current_dir(&dir)
            .output()
            .expect("failed to run associative-read fixture under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} associative-read fixture trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "1:10;1:1;1:1.5;1:;:9223372036854775806;1:10;",
            "PHP {version}"
        );
        let actual_stderr = String::from_utf8_lossy(&run.stderr)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            actual_stderr, expected_stderr,
            "PHP {version} associative misses must warn exactly once in source order"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies PHP source cannot reach WASM float-to-int or float-key lowering
/// while their versioned warning and deprecation diagnostics remain incomplete.
#[test]
fn test_cli_wasm_rejects_diagnostic_sensitive_float_to_int_paths() {
    let dir = make_cli_test_dir("elephc_cli_wasm_float_to_int_diagnostics");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$key = (float) $argv[1];
echo (int) $key;
$discard_cast = (int) $key;
$discard_array = [$key => 1];
$discard_bool = !$key;
if ($key) {}
$discard_nan_not = !NAN;
$discard_nan_ternary = NAN ? 1 : 2;
$discard_nan_short = NAN ?: 2;
if (NAN) {}
$mixed = $argc > 1 ? $key : "1";
echo (int) $mixed;
function wasm_source(bool $flag): mixed { return $flag ? 1.5 : 1; }
function wasm_sink(int $value): void {}
wasm_sink(wasm_source($argc > 1));
$checked = function(int $value): int { return $value + 1; };
$discard_checked = $checked($argc);
function wasm_checked_ref(): callable {
    $value = 1;
    return function() use (&$value) { return ++$value; };
}
$checked_ref = wasm_checked_ref();
$discard_checked_ref = $checked_ref();
$values = ["seed" => 1];
$values[$key] = 2;
echo $values[$key];
unset($values[$key]);
"#,
    )
    .unwrap();

    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile float-diagnostics fixture");
        assert!(
            !output.status.success(),
            "PHP {version} must reject diagnostic-sensitive float conversions"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        // The explicit `(int)` cast now carries its exact PHP 8.5 diagnostic and is
        // admitted, so this fixture no longer produces a float-to-int issue. The implicit
        // Mixed-to-scalar TRANSFER is lowered too — through `__rt_mixed_narrow_int`, which
        // narrows silently the way the native backend does rather than borrowing the explicit
        // cast's warning — so its refusal message is gone as well. What still refuses here is
        // NAN truthiness and the explicit Mixed-to-scalar CAST.
        assert!(
            stderr.contains(
                "float or Mixed truthiness requires exact profile-specific NAN diagnostics"
            ),
            "PHP {version}: {stderr}"
        );
        assert!(
            stderr
                .matches(
                    "float or Mixed truthiness requires exact profile-specific NAN diagnostics"
                )
                .count()
                >= 6,
            "PHP {version}: constant NAN truthiness was optimized away: {stderr}"
        );
        assert!(
            stderr.contains(
                "Mixed-to-scalar casts require exact per-tag PHP values and diagnostics"
            ),
            "PHP {version}: {stderr}"
        );
        // Checked arithmetic through an escaping ref cell no longer produces a shape
        // rejection: the capture widens to `Mixed`, so the store and the loads agree and
        // the overflow promotion survives. `test_by_ref_capture_preserves_integer_overflow_promotion`
        // owns that behavior now.
        assert!(
            stderr.contains(
                "float associative keys require exact profile-specific implicit-conversion diagnostics"
            ),
            "PHP {version}: {stderr}"
        );
        assert!(
            !dir.join("main.wat").exists() && !dir.join("main.wasm").exists(),
            "PHP {version} rejection must publish no artifact"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies WASM arithmetic over boxed Mixed operands matches php-src.
///
/// `MixedNumericBinop` carries PHP's numeric semantics for values whose type is only
/// known at runtime: integer-overflow promotion, `bool` and `null` as integers, and the
/// numeric-string rules, where the *form* decides the result type — `"7" + 5` is an
/// integer while `"7.0" + 5` is a double. A string with only a numeric prefix warns and
/// contributes that prefix; one with none is a PHP `TypeError`.
#[test]
fn test_cli_wasm_mixed_numeric_arithmetic_matches_php() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_numeric");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function box(mixed $v): mixed { return $v; }
echo box(2) + 5, "\n";
echo box(1.5) + 5, "\n";
echo box(true) + 5, "\n";
echo box(null) + 5, "\n";
echo box("7") + 5, "\n";
echo box("7.0") + 5, "\n";
echo box("7e2") + 5, "\n";
echo box(" 7") + 5, "\n";
echo box("7 ") + 5, "\n";
echo box("007") + 5, "\n";
echo box("+7") + 5, "\n";
echo box("-7") + 5, "\n";
echo box(".5") + 5, "\n";
echo box(9223372036854775807) + 5, "\n";
echo box("9223372036854775808") + 5, "\n";
echo box("7") * 3, "\n";
echo box(10) - box(3), "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile mixed numeric arithmetic to WASM");
    assert!(
        output.status.success(),
        "mixed numeric compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let run = Command::new("wasmer")
        .arg("run")
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run mixed numeric arithmetic under Wasmer");
    assert!(
        run.status.success(),
        "mixed numeric arithmetic trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // Every line is php-src 8.5's own output for the same program.
    let expected = concat!(
        "7\n", "6.5\n", "6\n", "5\n", "12\n", "12\n", "705\n", "12\n", "12\n", "12\n",
        "12\n", "-2\n", "5.5\n", "9.2233720368548E+18\n", "9.2233720368548E+18\n",
        "21\n", "7\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    assert!(
        run.stderr.is_empty(),
        "well-formed operands must not diagnose: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a string carrying only a numeric prefix warns, and a non-numeric one is fatal.
#[test]
fn test_cli_wasm_mixed_numeric_string_diagnostics() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_numeric_diag");

    let leading = dir.join("leading.php");
    fs::write(
        &leading,
        "<?php\nfunction box(mixed $v): mixed { return $v; }\necho box(\"7abc\") + 5, \"\\n\";\n",
    )
    .unwrap();
    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&leading)
        .output()
        .expect("failed to compile the leading-numeric fixture");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let run = Command::new("wasmer")
        .arg("run")
        .arg(dir.join("leading.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the leading-numeric fixture");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "12\n");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("A non-numeric value encountered"),
        "a numeric prefix must warn: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let fatal = dir.join("fatal.php");
    fs::write(
        &fatal,
        "<?php\nfunction box(mixed $v): mixed { return $v; }\necho box(\"abc\") + 5, \"\\n\";\n",
    )
    .unwrap();
    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&fatal)
        .output()
        .expect("failed to compile the non-numeric fixture");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let run = Command::new("wasmer")
        .arg("run")
        .arg(dir.join("fatal.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the non-numeric fixture");
    assert_eq!(
        run.status.code(),
        Some(255),
        "a non-numeric operand is an uncaught TypeError: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("Unsupported operand types"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `declare(strict_types=1)` suppresses PHP's scalar argument coercions.
///
/// PHP performs no scalar coercion at a typed parameter under strict typing, with one
/// documented exception: an `int` argument still widens to a `float` parameter. Without
/// the directive the same calls are legal coercive conversions. Verified against the
/// pinned php-src CLIs, which raise `TypeError` for exactly the rejected pairs.
#[test]
fn test_cli_strict_types_refuses_scalar_argument_coercion() {
    let dir = make_cli_test_dir("elephc_cli_strict_types_coercion");

    // (parameter type, argument, admitted under strict typing)
    let cases = [
        ("int", "true", false),
        ("float", "true", false),
        ("bool", "1", false),
        ("float", "1", true),
        ("int", "1", true),
    ];

    for (param_ty, argument, strict_admits) in cases {
        for strict in [false, true] {
            let declare = if strict { "declare(strict_types=1);" } else { "" };
            let php_path = dir.join("main.php");
            fs::write(
                &php_path,
                format!(
                    "<?php\n{declare}\nfunction sink({param_ty} $x): {param_ty} {{ return $x; }}\necho sink({argument});\n"
                ),
            )
            .unwrap();

            let output = elephc_cli_command(&dir)
                .arg("--check")
                .arg(&php_path)
                .output()
                .expect("failed to type-check the coercion fixture");

            // Coercive mode admits every pair; strict mode admits only widening.
            let expected = !strict || strict_admits;
            assert_eq!(
                output.status.success(),
                expected,
                "strict={strict} {param_ty} <- {argument}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if !expected {
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("strict_types=1 performs no coercion"),
                    "strict={strict} {param_ty} <- {argument} must name the cause: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }

    // The same gate gates every typed write, not just arguments: PHP applies strict
    // typing to a typed property assignment and to a declared return type as well.
    let sites = [
        ("class C { public int $v = 0; } $o = new C(); $o->v = true; echo 1;", "property"),
        ("function f(): int { return true; } echo f();", "return type"),
    ];
    for (source, site) in sites {
        for strict in [false, true] {
            let declare = if strict { "declare(strict_types=1);" } else { "" };
            let php_path = dir.join("main.php");
            fs::write(&php_path, format!("<?php\n{declare}\n{source}\n")).unwrap();

            let output = elephc_cli_command(&dir)
                .arg("--check")
                .arg(&php_path)
                .output()
                .expect("failed to type-check the typed-write fixture");
            assert_eq!(
                output.status.success(),
                !strict,
                "strict={strict} bool at an int {site}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the explicit `(int)` float cast matches each pinned php-src profile.
///
/// Requirement H (`PHP-WASM-NUM-004`). The value is PHP's modulo-2^64 result on every
/// profile, so `(int) 1.0e20` is the mandatory regression. The diagnostic is version
/// dependent: PHP 8.5 alone warns, and only for values no integer can represent. That
/// predicate is about range and finiteness, never integrality, so `1.9` stays silent
/// on every profile while `NAN` and `INF` warn on 8.5.
#[test]
fn test_cli_wasm_explicit_float_to_int_cast_matches_php_profiles() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_float_to_int_cast");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function float_value(float $value): float {
    return $value;
}
echo (int) float_value(1.0e20); echo "\n";
echo (int) float_value(-1.0e20); echo "\n";
echo (int) float_value(1.9); echo "\n";
echo (int) float_value(-1.9); echo "\n";
echo (int) float_value(NAN); echo "\n";
echo (int) float_value(INF); echo "\n";
"#,
    )
    .unwrap();

    // Values are identical on every profile; only the diagnostics differ.
    let expected_stdout =
        "7766279631452241920\n-7766279631452241920\n1\n-1\n0\n0\n";
    let warning = "is not representable as an int, cast occurred";

    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile the float cast to WASM");
        assert!(
            output.status.success(),
            "PHP {version} float cast compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let run = Command::new("wasmer")
            .arg("run")
            .arg(dir.join("main.wasm"))
            .current_dir(&dir)
            .output()
            .expect("failed to run the float cast under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} float cast trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            expected_stdout,
            "PHP {version} float cast values must match php-src"
        );

        let stderr = String::from_utf8_lossy(&run.stderr);
        if version == "8.5" {
            // 1.0e20, -1.0e20, NAN and INF are unrepresentable; 1.9 and -1.9 are not.
            assert_eq!(
                stderr.matches(warning).count(),
                4,
                "PHP {version} must warn once per unrepresentable value: {stderr}"
            );
            assert!(
                stderr.contains("Warning: The float 1.0E+20 "),
                "PHP {version} must render the float exactly as PHP prints it: {stderr}"
            );
        } else {
            assert!(
                stderr.is_empty(),
                "PHP {version} must not diagnose the cast: {stderr}"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies public PHP shapes with incomplete WASM runtime contracts fail closed.
#[test]
fn test_cli_wasm_rejects_unproven_object_iterator_and_global_shapes() {
    let dir = make_cli_test_dir("elephc_cli_wasm_unproven_shapes");
    let php_path = dir.join("main.php");
    let cases = [
        (
            r#"<?php class C { public int $value; } $c = new C(); echo $c->value;"#,
            "may be uninitialized and requires an exact PHP fatal check",
        ),
        (
            r#"<?php #[AllowDynamicProperties] class C {} $c = new C(); echo $c->missing;"#,
            "reads require the exact PHP undefined-property warning",
        ),
        (
            r#"<?php class A { public int $x = 1; } class B { public int $x = 2; } $o = $argc > 1 ? new A() : new B(); echo $o?->x;"#,
            "Nullsafe property access requires a single nullable object type",
        ),
        (
            r#"<?php $h = ["a" => 1]; foreach ($h as &$v) { $v = 2; }"#,
            "by-reference foreach over associative arrays",
        ),
        (
            r#"<?php $a = [1, 2]; foreach ($a as $v) { echo $v; $a[] = 3; }"#,
            "may mutate the iterated container without PHP snapshot/COW semantics",
        ),
        (
            r#"<?php function cmp(int $a, int $b): int { return $a - $b; } $a = [2, 1]; foreach ($a as $v) { echo $v; usort($a, 'cmp'); }"#,
            "usort may mutate the iterated container without PHP snapshot/COW semantics",
        ),
        (
            r#"<?php function read_custom(): mixed { global $custom; return $custom; } echo read_custom();"#,
            "global $custom is not implemented by the WASI runtime",
        ),
    ];

    for (index, (source, expected)) in cases.iter().enumerate() {
        fs::write(&php_path, source).unwrap();
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg("8.5")
            .arg(&php_path)
            .output()
            .unwrap_or_else(|error| panic!("case #{index} failed to invoke elephc: {error}"));
        assert!(
            !output.status.success(),
            "case #{index} unexpectedly compiled: {source}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "case #{index} missing {expected:?}: {stderr}"
        );
        assert!(
            !dir.join("main.wat").exists() && !dir.join("main.wasm").exists(),
            "case #{index} rejection published a WASM artifact"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies indexed boolean arrays preserve tag 3 when promoted to hashes.
#[test]
fn test_cli_wasm_bool_array_promotion_preserves_boolean_tags() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_bool_array_promotion");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$a = [false];
$a["k"] = true;
echo "[", $a[0], "]";
$b = [];
$b[] = false;
$b["k"] = true;
echo "[", $b[0], "]";
$c = [];
$c[0] = false;
$c["k"] = true;
echo "[", $c[0], "]";
class Flag { public bool $value = false; }
$flag = new Flag();
echo "[", $flag->value, "]";
$flag->value = false;
echo "[", $flag?->value, "]";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--php-version")
        .arg("8.5")
        .arg(&php_path)
        .output()
        .expect("failed to compile boolean-array promotion fixture");
    assert!(
        output.status.success(),
        "boolean-array promotion compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let run = Command::new("wasmer")
        .arg("run")
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run boolean-array promotion fixture");
    assert!(
        run.status.success(),
        "boolean-array promotion fixture trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "[][][][][]");
    assert!(
        run.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a declared mixed property retains a borrowed cell independently of its source.
#[test]
fn test_cli_wasm_mixed_property_retains_borrowed_source() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_property_borrow");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class Holder { public mixed $value = 1; }
function make_value(): mixed { return "hello"; }
function fill(Holder $holder): int {
    $value = make_value();
    $holder->value = $value;
    return 0;
}
$holder = new Holder();
fill($holder);
echo $holder->value;
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--php-version")
        .arg("8.5")
        .arg(&php_path)
        .output()
        .expect("failed to compile borrowed mixed-property fixture");
    assert!(
        output.status.success(),
        "borrowed mixed-property compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let run = Command::new("wasmer")
        .arg("run")
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run borrowed mixed-property fixture");
    assert!(
        run.status.success(),
        "borrowed mixed-property fixture trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello");
    assert!(
        run.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies associative reads preserve precise nullable container pointers:
/// misses remain PHP null, hits remain non-null, and hit containers still feed
/// typed chained reads.
#[test]
fn test_cli_wasm_hash_container_reads_preserve_nullable_php_values() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_hash_container_null");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class Item {
    public function value(): int {
        return 3;
    }
}
$arrays = ["hit" => [1]];
$hashes = ["hit" => ["x" => 2]];
$objects = ["hit" => new Item()];
echo is_null($arrays["missing"]), ":", is_null($arrays["hit"]), ";";
echo is_null($hashes["missing"]), ":", is_null($hashes["hit"]), ";";
echo is_null($objects["missing"]), ":", is_null($objects["hit"]), ";";
echo $arrays["hit"][0], ":", $hashes["hit"]["x"], ";";
echo $objects["hit"]->value(), ";";
echo is_null($arrays["hit"][99]), ":", is_null($hashes["hit"]["missing"]), ";";
"#,
    )
    .unwrap();

    let expected_stderr = [
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key \"missing\"",
        "Warning: Undefined array key 99",
        "Warning: Undefined array key \"missing\"",
    ];
    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile associative container reads to WASM");
        assert!(
            output.status.success(),
            "PHP {version} associative container compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let wasm_path = dir.join("main.wasm");
        let run = Command::new("wasmer")
            .arg("run")
            .arg(&wasm_path)
            .current_dir(&dir)
            .output()
            .expect("failed to run associative container reads under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} associative container reads trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "1:;1:;1:;1:2;3;1:1;",
            "PHP {version}"
        );
        let actual_stderr = String::from_utf8_lossy(&run.stderr)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(actual_stderr, expected_stderr, "PHP {version}");
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies nullable chained reads evaluate their index exactly once before
/// normal offset-on-null warnings while coalescing reads remain silent.
#[test]
fn test_cli_wasm_nullable_chained_reads_preserve_php_index_order() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_nullable_chain_order");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function int_key_side_effect(string $label): int {
    echo $label;
    return 0;
}
function string_key_side_effect(string $label): string {
    echo $label;
    return "inner";
}
$arrays = ["hit" => [1]];
$hashes = ["hit" => ["inner" => 2]];
echo "A", $arrays["missing-array"][int_key_side_effect("a")], "Z;";
echo "H", $hashes["missing-hash"][string_key_side_effect("h")], "Z;";
echo "S", ($arrays["missing-array"][int_key_side_effect("s")] ?? 9), ";";
echo "T", ($hashes["missing-hash"][string_key_side_effect("t")] ?? 8), ";";
echo "I", $arrays["hit"][int_key_side_effect("i")], ":";
echo $hashes["hit"][string_key_side_effect("j")], ";";
"#,
    )
    .unwrap();

    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let offset_warning = if version == "8.2" {
            "Warning: Trying to access array offset on value of type null"
        } else {
            "Warning: Trying to access array offset on null"
        };
        let expected_stderr = [
            "Warning: Undefined array key \"missing-array\"",
            offset_warning,
            "Warning: Undefined array key \"missing-hash\"",
            offset_warning,
        ];
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile nullable chained reads to WASM");
        assert!(
            output.status.success(),
            "PHP {version} nullable chain compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let wasm_path = dir.join("main.wasm");
        let run = Command::new("wasmer")
            .arg("run")
            .arg(&wasm_path)
            .current_dir(&dir)
            .output()
            .expect("failed to run nullable chained reads under Wasmer");
        assert!(
            run.status.success(),
            "PHP {version} nullable chained reads trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "AaZ;HhZ;Ss9;Tt8;Ii1:j2;",
            "PHP {version}"
        );
        let actual_stderr = String::from_utf8_lossy(&run.stderr)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(actual_stderr, expected_stderr, "PHP {version}");
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a missing object-valued associative entry raises PHP's method-on-null
/// warning/fatal pair before evaluating method arguments.
#[test]
fn test_cli_wasm_missing_hash_object_method_call_is_php_fatal() {
    if Command::new("wasmer").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_hash_object_null_fatal");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function side_effect(): int {
    echo "BAD";
    return 1;
}
class Item {
    public function value(int $value): int {
        return $value;
    }
}
$objects = ["hit" => new Item()];
$objects["missing"]->value(side_effect());
"#,
    )
    .unwrap();

    for version in ["8.2", "8.3", "8.4", "8.5"] {
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("wasm32-wasi")
            .arg("--php-version")
            .arg(version)
            .arg(&php_path)
            .output()
            .expect("failed to compile missing object method call to WASM");
        assert!(
            output.status.success(),
            "PHP {version} missing object method-call compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let wasm_path = dir.join("main.wasm");
        let run = Command::new("wasmer")
            .arg("run")
            .arg(&wasm_path)
            .current_dir(&dir)
            .output()
            .expect("failed to run missing object method call under Wasmer");
        assert_eq!(run.status.code(), Some(255), "PHP {version}");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "",
            "PHP {version}: argument side effects must not run after a null receiver"
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stderr),
            "Warning: Undefined array key \"missing\"\nPHP Fatal error: Uncaught Error: Call to a member function value() on null\n",
            "PHP {version}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--debug-info` injects DWARF line-table directives into the emitted
/// assembly: one `.file 1` header and a `.loc 1 <line> <col>` per source marker.
#[test]
fn test_cli_debug_info_injects_dwarf_line_directives() {
    let dir = make_cli_test_dir("elephc_cli_debug_info");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
echo 1 + 2;
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--emit-asm")
        .arg("--debug-info")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --debug-info");

    assert!(
        output.status.success(),
        "elephc --debug-info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm_path = dir.join("main.s");
    let asm = fs::read_to_string(&asm_path).expect("failed to read assembly");
    assert!(
        asm.starts_with(".file 1 \""),
        "expected .file header at top of assembly, got: {}",
        &asm[..asm.len().min(120)]
    );
    assert!(
        asm.contains(".loc 1 2 "),
        "expected a .loc directive for PHP line 2: {asm}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies PHP `try`/`catch`/`throw` lowers to the Core WebAssembly exception forms.
///
/// The shapes asserted here are the whole of the design: one module-level `tag` carrying the
/// exception object pointer, a `try_table` wrapping the dispatch loop, a `throw` at the raise
/// site, and a landing pad that turns the catch into an ordinary dispatch-state transition.
/// Asserting on the emitted WAT rather than on program output keeps this test meaningful on a
/// machine with no exceptions-capable host installed.
#[test]
fn test_cli_wasm_try_catch_lowers_to_core_exception_forms() {
    let dir = make_cli_test_dir("elephc_cli_wasm_try_catch_forms");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class Boom extends Exception {
}

function risky(int $n): void {
    if ($n < 0) {
        throw new Boom();
    }
    echo "ok\n";
}

try {
    risky(1);
    risky(-1);
} catch (Boom $e) {
    echo "caught\n";
}
echo "done\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile try/catch to WASM");
    assert!(
        output.status.success(),
        "try/catch compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let wat = fs::read_to_string(dir.join("main.wat")).expect("missing emitted WAT");
    assert!(
        wat.contains("(tag $__php_exc (param i32))"),
        "expected the PHP exception tag: {wat}"
    );
    assert!(
        wat.contains("(try_table (catch $__php_exc $__caught)"),
        "expected the dispatch loop to be guarded: {wat}"
    );
    assert!(
        wat.contains("throw $__php_exc"),
        "expected the raise site to throw the tag: {wat}"
    );
    assert!(
        wat.contains("global.set $__exc_value"),
        "expected the landing pad to publish the caught exception: {wat}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies thrown exceptions select the matching `catch` clause and reach the right frame.
///
/// Runs the compiled module under Node's WASI, which implements the Core WebAssembly exception
/// proposal; the expected output is php-src's own for the same program. Skipped when no Node is
/// installed, so `test_cli_wasm_try_catch_lowers_to_core_exception_forms` remains the assertion
/// that always runs.
#[test]
fn test_cli_wasm_try_catch_dispatch_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_try_catch_dispatch");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class AlphaError extends Exception {
}

class BetaError extends Exception {
}

function pick(int $n): void {
    if ($n === 1) {
        throw new AlphaError();
    }
    if ($n === 2) {
        throw new BetaError();
    }
    echo "none\n";
}

foreach ([0, 1, 2] as $n) {
    try {
        pick($n);
        echo "no throw\n";
    } catch (AlphaError $e) {
        echo "alpha\n";
    } catch (BetaError $e) {
        echo "beta\n";
    }
}

try {
    try {
        throw new AlphaError();
    } catch (BetaError $e) {
        echo "inner-wrong\n";
    }
} catch (AlphaError $e) {
    echo "outer-right\n";
}

echo "end\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile catch dispatch to WASM");
    assert!(
        output.status.success(),
        "catch dispatch compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run catch dispatch under Node");
    // Node without exception support fails to compile the module rather than misbehaving;
    // treat that as "no capable host" rather than a lowering failure.
    if !run.status.success()
        && String::from_utf8_lossy(&run.stderr).contains("CompileError")
    {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "catch dispatch trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5's own output for the same program.
    let expected = concat!(
        "none\n",
        "no throw\n",
        "alpha\n",
        "beta\n",
        "outer-right\n",
        "end\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an exception nobody catches is PHP's fatal, not an escape into the host.
///
/// A WebAssembly exception that unwinds out of `_start` would surface as a host-level crash with
/// no PHP diagnostic at all, so `main` is guarded even when it contains no `catch`. The exit
/// status is php-src's 255. The message text is deliberately not compared: reproducing PHP's
/// `Uncaught Exception: <message> in <file>:<line>` needs the built-in Throwable accessors,
/// which this target does not lower yet.
#[test]
fn test_cli_wasm_uncaught_exception_is_a_php_fatal() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_uncaught_exception");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\necho \"before\\n\";\nthrow new Exception();\n",
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the uncaught throw to WASM");
    assert!(
        output.status.success(),
        "uncaught throw compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the uncaught throw under Node");
    if String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "before\n",
        "output before the throw must still be flushed"
    );
    assert_eq!(
        run.status.code(),
        Some(255),
        "an uncaught PHP exception exits 255: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies PHP's arithmetic RAISES on the native backend, on both targets alike.
///
/// Five operators answered a machine result where reference PHP raises: `%` by zero returned
/// zero, either shift by a negative count returned the hardware's masked result, and BOTH float
/// and integer `/` by zero returned an infinity. All five were SILENT wrong answers — a program
/// that expected an exception continued with a plausible-looking number.
///
/// Integer `/` is a separate opcode from float `/` because PHP promotes its operands, so the
/// guard has to run on the INTEGER divisor before the promotion: a promoted zero has a sign, and
/// testing it after the fact would need the sign masked off.
///
/// The shift also masked its count to six bits, so `1 << 64` answered 1 and `-8 >> 64` answered
/// -8 where PHP answers 0 and -1. That is fixed here too: PHP saturates rather than wrapping.
///
/// Both backends are checked against the same expected output, because this is where they
/// disagreed: WASM already raised four of the five, so the native one was mostly the outlier —
/// but integer `/` was wrong on BOTH, which is why one expected output covers them together.
#[test]
fn test_cli_arithmetic_raises_match_php_on_both_backends() {
    let dir = make_cli_test_dir("elephc_cli_arithmetic_raises");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function m(int $a, int $b): int { return $a % $b; }
function sl(int $a, int $b): int { return $a << $b; }
function sr(int $a, int $b): int { return $a >> $b; }
function fd(float $a, float $b): float { return $a / $b; }
function q(int $a, int $b): float { return $a / $b; }
echo m(7,3), "|", m(-7,3), "|", m(7,-3), "\n";
echo sl(1,0), "|", sl(1,63), "|", sl(1,64), "|", sl(1,100), "|", sl(-8,1), "\n";
echo sr(1,63), "|", sr(1,64), "|", sr(-8,1), "|", sr(-8,64), "|", sr(-8,100), "\n";
echo fd(1.5,0.5), "|", fd(-6.0,3.0), "|", q(6,3), "|", q(7,2), "\n";
try { echo m(1,0), "\n"; } catch (\DivisionByZeroError $a) { echo "mod0|", $a->getMessage(), "\n"; }
try { echo sl(1,-1), "\n"; } catch (\ArithmeticError $b) { echo "shl|", $b->getMessage(), "\n"; }
try { echo sr(1,-1), "\n"; } catch (\ArithmeticError $c) { echo "shr|", $c->getMessage(), "\n"; }
try { echo fd(1.0,0.0), "\n"; } catch (\DivisionByZeroError $d) { echo "fdiv|", $d->getMessage(), "\n"; }
try { echo fd(1.0,-0.0), "\n"; } catch (\DivisionByZeroError $e) { echo "fdiv-neg0|", $e->getMessage(), "\n"; }
try { echo q(1,0), "\n"; } catch (\DivisionByZeroError $f) { echo "intdiv0|", $f->getMessage(), "\n"; }
try { echo q(0,0), "\n"; } catch (\DivisionByZeroError $g) { echo "int00|", $g->getMessage(), "\n"; }
echo "end\n";
"#,
    )
    .unwrap();

    // php-src 8.5.6's own output for the same program.
    let expected = concat!(
        "1|-1|1\n",
        "1|-9223372036854775808|0|0|-16\n",
        "0|0|-4|-1|-1\n",
        "3|-2|2|3.5\n",
        "mod0|Modulo by zero\n",
        "shl|Bit shift by negative number\n",
        "shr|Bit shift by negative number\n",
        "fdiv|Division by zero\n",
        "fdiv-neg0|Division by zero\n",
        "intdiv0|Division by zero\n",
        "int00|Division by zero\n",
        "end\n",
    );

    let native = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile the arithmetic raises natively");
    assert!(
        native.status.success(),
        "native compilation failed: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let native_run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the arithmetic raises natively");
    assert!(
        native_run.status.success(),
        "a caught arithmetic error still killed the native program: {}",
        String::from_utf8_lossy(&native_run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&native_run.stdout), expected);

    if Command::new("node").arg("--version").output().is_err() {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let wasm = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the arithmetic raises to WASM");
    assert!(
        wasm.status.success(),
        "WASM compilation failed: {}",
        String::from_utf8_lossy(&wasm.stderr)
    );
    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();
    let wasm_run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the arithmetic raises under Node");
    if !wasm_run.status.success()
        && String::from_utf8_lossy(&wasm_run.stderr).contains("CompileError")
    {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        wasm_run.status.success(),
        "a caught arithmetic error still killed the WASM program: {}",
        String::from_utf8_lossy(&wasm_run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&wasm_run.stdout), expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies PHP's arithmetic runtime errors are CATCHABLE, not process-killing fatals.
///
/// Reference PHP raises `DivisionByZeroError` / `ArithmeticError` for these five guards, so a
/// `catch` receives them and execution continues past the `try`. Emitting them as a direct
/// `__rt_fail` exit — which is what this backend did before — silently skipped every handler
/// and killed the program, so the assertion that matters is that `end` is reached at all. Each
/// clause binds its own variable because re-binding one name across clauses of DIFFERENT classes
/// still corrupts the caught object on this target, which is an unrelated open defect.
#[test]
fn test_cli_wasm_runtime_errors_are_catchable() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_catchable_runtime_errors");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function idiv(int $x, int $y): int { return intdiv($x, $y); }
function imod(int $x, int $y): int { return $x % $y; }
function ishift(int $x, int $y): int { return $x << $y; }
function fdiv2(float $x, float $y): float { return $x / $y; }

try { echo idiv(1, 0), "\n"; } catch (\DivisionByZeroError $a) { echo "A|", $a->getMessage(), "\n"; }
try { echo imod(1, 0), "\n"; } catch (\DivisionByZeroError $b) { echo "B|", $b->getMessage(), "\n"; }
try { echo ishift(1, -1), "\n"; } catch (\ArithmeticError $c) { echo "C|", $c->getMessage(), "\n"; }
try { echo idiv(PHP_INT_MIN, -1), "\n"; } catch (\ArithmeticError $d) { echo "D|", $d->getMessage(), "\n"; }
try { echo fdiv2(1.0, 0.0), "\n"; } catch (\DivisionByZeroError $f) { echo "E|", $f->getMessage(), "\n"; }
echo "end\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile catchable runtime errors to WASM");
    assert!(
        output.status.success(),
        "catchable runtime error compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run catchable runtime errors under Node");
    if !run.status.success() && String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "a caught runtime error still killed the program: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    let expected = concat!(
        "A|Division by zero\n",
        "B|Modulo by zero\n",
        "C|Bit shift by negative number\n",
        "D|Division of PHP_INT_MIN by -1 is not an integer\n",
        "E|Division by zero\n",
        "end\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an uncaught runtime error reports identically whether or not it was raised.
///
/// A module with no `try` cannot catch, so its guards stay on the direct `__rt_fail` path that
/// keeps the program runnable on a host without the exceptions proposal. A module that does
/// catch raises instead, and the diagnostic then has to travel with the exception for `main`'s
/// landing pad to print it — otherwise the uncaught case regresses to the class-agnostic
/// "Uncaught exception". Both variants of each failure are compiled here precisely because the
/// two paths are different code: the point is that they are indistinguishable from outside.
/// php-src also prints the file, line and stack trace, which this target does not reproduce
/// yet; the class, the message and the 255 exit status are what is compared.
#[test]
fn test_cli_wasm_uncaught_runtime_error_keeps_its_php_diagnostic() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let runner_source = r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#;

    // A `try` for a class the failure never raises: it does not catch anything here, it only
    // makes the module declare the exception tag, which is what puts the guards on the raise
    // path. Without it the same program stays on the direct fatal path.
    let arming_try = concat!(
        "try {\n",
        "    echo \"armed\\n\";\n",
        "} catch (\\RuntimeException $unused) {\n",
        "    echo \"never\\n\";\n",
        "}\n",
    );

    for (label, body, expected) in [
        (
            "div",
            "function f(int $x, int $y): int { return intdiv($x, $y); }\necho f(1, 0), \"\\n\";\n",
            "PHP Fatal error: Uncaught DivisionByZeroError: Division by zero\n",
        ),
        (
            "mod",
            "function f(int $x, int $y): int { return $x % $y; }\necho f(1, 0), \"\\n\";\n",
            "PHP Fatal error: Uncaught DivisionByZeroError: Modulo by zero\n",
        ),
        (
            "shift",
            "function f(int $x, int $y): int { return $x << $y; }\necho f(1, -1), \"\\n\";\n",
            "PHP Fatal error: Uncaught ArithmeticError: Bit shift by negative number\n",
        ),
        (
            "overflow",
            "function f(int $x, int $y): int { return intdiv($x, $y); }\necho f(PHP_INT_MIN, -1), \"\\n\";\n",
            "PHP Fatal error: Uncaught ArithmeticError: Division of PHP_INT_MIN by -1 is not an integer\n",
        ),
    ] {
        for (path_label, prologue, expected_stdout) in
            [("direct", "", ""), ("raised", arming_try, "armed\n")]
        {
            let dir = make_cli_test_dir(&format!(
                "elephc_cli_wasm_uncaught_runtime_error_{label}_{path_label}"
            ));
            let php_path = dir.join("main.php");
            fs::write(&php_path, format!("<?php\n{prologue}{body}")).unwrap();

            let output = elephc_cli_command(&dir)
                .arg("--target")
                .arg("wasm32-wasi")
                .arg(&php_path)
                .output()
                .expect("failed to compile an uncaught runtime error to WASM");
            assert!(
                output.status.success(),
                "uncaught runtime error compilation failed for {label}/{path_label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );

            let runner = dir.join("run.mjs");
            fs::write(&runner, runner_source).unwrap();

            // `--no-warnings` keeps Node's `ExperimentalWarning: WASI` off the stream the PHP
            // diagnostic is compared on.
            let run = Command::new("node")
                .arg("--no-warnings")
                .arg(&runner)
                .arg(dir.join("main.wasm"))
                .current_dir(&dir)
                .output()
                .expect("failed to run an uncaught runtime error under Node");
            let stderr = String::from_utf8_lossy(&run.stderr).to_string();
            if stderr.contains("CompileError") {
                let _ = fs::remove_dir_all(&dir);
                continue;
            }
            assert_eq!(
                stderr, expected,
                "uncaught {label} lost the PHP class and message it names on the {path_label} path"
            );
            assert_eq!(
                String::from_utf8_lossy(&run.stdout),
                expected_stdout,
                "output before the {label} failure must still be flushed on the {path_label} path"
            );
            assert_eq!(
                run.status.code(),
                Some(255),
                "an uncaught PHP runtime error exits 255 for {label}/{path_label}"
            );

            let _ = fs::remove_dir_all(&dir);
        }
    }
}

/// Verifies a class property STRING default is materialized, raw and boxed.
///
/// Object construction writes defaults inline rather than through the class's
/// `_class_propinit_*` function, so a string default has no `DataId` to address at the
/// construction site and needs its own content-keyed data segment. A `mixed` slot exercises the
/// boxed arm, where the string becomes a Mixed cell rather than a raw (ptr, len) pair.
#[test]
fn test_cli_wasm_string_property_defaults_are_materialized() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_string_property_defaults");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class C {
    public string $name = "x";
    public mixed $tag = "boxed";
}

$c = new C();
echo $c->name, "|", $c->tag, "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile string property defaults to WASM");
    assert!(
        output.status.success(),
        "string property default compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run string property defaults under Node");
    assert!(
        run.status.success(),
        "string property defaults trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    // php-src 8.5's own output for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), "x|boxed\n");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the built-in `Throwable` accessors answer what the NATIVE backend answers.
///
/// These methods carry a signature but no EIR body on either backend, so both open-code them.
/// The comparison that matters is therefore native-vs-WASM, not WASM-vs-php-src: elephc records
/// no per-throw file, line or backtrace, so `getFile()` is empty, `getLine()` zero,
/// `getTraceAsString()` empty and `__toString()` the message alone — php-src reports all four
/// differently, and a program that changed behavior once compiled for WebAssembly would be the
/// real defect. `getPrevious()` returns `?Throwable`, so the chained call exercises the
/// Mixed-receiver dispatch ladder rather than the direct path.
#[test]
fn test_cli_wasm_throwable_accessors_match_the_native_backend() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_throwable_accessors");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class Wrapped extends Exception {
}

$first = new Wrapped("inner", 3);
try {
    throw new Exception("outer", 9, $first);
} catch (Exception $e) {
    echo $e->getMessage(), "|", $e->getCode(), "\n";
    echo "[", $e->getFile(), "]", $e->getLine(), "\n";
    echo "[", $e->getTraceAsString(), "]\n";
    echo $e->__toString(), "\n";
    $p = $e->getPrevious();
    echo $p->getMessage(), "|", $p->getCode(), "\n";
}
echo "end\n";
"#,
    )
    .unwrap();

    let native = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to compile Throwable accessors natively");
    assert!(
        native.status.success(),
        "native compilation failed: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let native_run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the native Throwable accessors");
    assert!(
        native_run.status.success(),
        "native run failed: {}",
        String::from_utf8_lossy(&native_run.stderr)
    );

    let wasm = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile Throwable accessors to WASM");
    assert!(
        wasm.status.success(),
        "WASM compilation failed: {}",
        String::from_utf8_lossy(&wasm.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let wasm_run = Command::new("node")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the WASM Throwable accessors under Node");
    if String::from_utf8_lossy(&wasm_run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        wasm_run.status.success(),
        "WASM run failed: {}",
        String::from_utf8_lossy(&wasm_run.stderr)
    );

    assert_eq!(
        String::from_utf8_lossy(&wasm_run.stdout),
        String::from_utf8_lossy(&native_run.stdout),
        "the two backends must answer the Throwable accessors identically"
    );
    // Pinned so a change to elephc's synthetic answers has to be deliberate on both backends.
    assert_eq!(
        String::from_utf8_lossy(&native_run.stdout),
        "outer|9\n[]0\n[]\nouter\ninner|3\nend\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the explicit `(int)` / `(float)` cast of a runtime-typed value matches php-src.
///
/// The interesting cases are the ones a naive implementation gets wrong: PHP yields 1 for ANY
/// non-empty array rather than its length, wraps a finite out-of-range float modulo 2^64 instead
/// of saturating, maps NaN and both infinities to zero, and diagnoses an object while still
/// producing 1. Every expected line here is php-src 8.5's own output for the same program.
#[test]
fn test_cli_wasm_explicit_mixed_scalar_casts_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_scalar_casts");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
class C {}
function box(mixed $v): mixed { return $v; }
echo (int) box(42), "\n";
echo (int) box(true), "\n";
echo (int) box(null), "\n";
echo (int) box(3.7), "\n";
echo (int) box(-3.7), "\n";
echo (int) box("  12abc"), "\n";
echo (int) box("abc"), "\n";
echo (int) box([]), "\n";
echo (int) box([1, 2, 3]), "\n";
echo (int) box(1.0e19), "\n";
echo (int) box(NAN), "\n";
echo (int) box(INF), "\n";
echo (int) box(new C()), "\n";
echo (float) box("3.5"), "\n";
echo (float) box([1]), "\n";
echo (float) box(new C()), "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile mixed scalar casts to WASM");
    assert!(
        output.status.success(),
        "mixed scalar cast compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    // `--no-warnings` keeps Node's own ExperimentalWarning about `node:wasi` out of the stderr
    // this test compares against php-src's diagnostics.
    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run mixed scalar casts under Node");
    assert!(
        run.status.success(),
        "mixed scalar casts trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let expected = concat!(
        "42\n", "1\n", "0\n", "3\n", "-3\n", "12\n", "0\n", "0\n", "1\n",
        "-8446744073709551616\n", "0\n", "0\n", "1\n", "3.5\n", "1\n", "1\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);

    // php-src reports the same four diagnostics, in this order. The project's WASM convention
    // drops php-src's `PHP ` prefix and its ` in <file> on line <n>` tail.
    let stderr = String::from_utf8_lossy(&run.stderr);
    let diagnostics: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        diagnostics,
        vec![
            "Warning: The float 1.0E+19 is not representable as an int, cast occurred",
            "Warning: The float NAN is not representable as an int, cast occurred",
            "Warning: The float INF is not representable as an int, cast occurred",
            "Warning: Object of class C could not be converted to int",
            "Warning: Object of class C could not be converted to float",
        ],
        "diagnostics must match php-src's set and order"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the builtins lowered to a single WebAssembly instruction match php-src exactly.
///
/// `floor`, `ceil` and `sqrt` are bit-for-bit identities with their WebAssembly counterparts,
/// which the negative-zero case pins: `ceil(-0.5)` is `-0`, not `0`. `count` reads the container
/// header. `abs` is the one with an argument-dependent shape — integral in, integral out.
#[test]
fn test_cli_wasm_direct_builtins_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_direct_builtins");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function ints(int $n): void { echo abs($n), "\n"; }
function floats(float $x): void { echo abs($x), "|", floor($x), "|", ceil($x), "|", sqrt(abs($x)), "\n"; }
ints(-3); ints(3); ints(0);
floats(3.7); floats(-3.2); floats(-0.5); floats(0.0);
$a = [1, 2, 3];
echo count($a), "\n";
$h = ['x' => 1, 'y' => 2];
echo count($h), "\n";
$e = [];
echo array_is_list($a) ? "list" : "not", "|", array_is_list($e) ? "list" : "not", "\n";
function arrays(array $xs, int $n): void {
    $v = array_values($xs);
    $k = array_keys($xs);
    $v[0] = 99;
    echo in_array($n, $xs, true) ? "y" : "n", "|", count($k), "|", $k[1], "|", $xs[0], ",", $v[0], "\n";
}
arrays([7, 8, 9], 8);
arrays([7, 8, 9], 5);
function folds(array $xs): void {
    $r = array_reverse($xs);
    echo array_sum($xs), "|", array_product($xs), "|", $r[0], ",", $r[2], "\n";
}
folds([1, 2, 3]);
echo array_sum($e), "|", array_product($e), "|", count(array_reverse($e)), "\n";
function pairs(int $p, int $q): void {
    echo max($p, $q), "|", min($p, $q), "|", intdiv($p, $q), "\n";
}
pairs(7, 2);
pairs(-7, 2);
$filled = array_fill(0, 3, 7);
echo count($filled), "|", $filled[2], "|", count(array_fill(0, 0, 9)), "\n";
function needles(string $h, string $n): void {
    echo str_contains($h, $n) ? 1 : 0, str_starts_with($h, $n) ? 1 : 0, str_ends_with($h, $n) ? 1 : 0, "\n";
}
needles("hello", "ell");
needles("hello", "");
needles("he", "hello");
needles("", "");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the direct builtins to WASM");
    assert!(
        output.status.success(),
        "direct builtin compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the direct builtins under Node");
    assert!(
        run.status.success(),
        "direct builtins trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5's own output for the same program.
    let expected = concat!(
        "3\n",
        "3\n",
        "0\n",
        "3.7|3|4|1.9235384061671\n",
        "3.2|-4|-3|1.7888543819998\n",
        "0.5|-1|-0|0.70710678118655\n",
        "0|0|0|0\n",
        "3\n",
        "2\n",
        "list|list\n",
        "y|3|1|7,99\n",
        "n|3|1|7,99\n",
        "6|6|3,1\n",
        "0|1|0\n",
        "7|2|3\n",
        "2|-7|-3\n",
        "3|7|0\n",
        "100\n",
        "111\n",
        "000\n",
        "111\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    assert!(
        run.stderr.is_empty(),
        "these builtins diagnose nothing: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the byte-mapping string transforms match php-src.
///
/// Since PHP 8.2 `strtoupper` and `strtolower` are locale-independent and touch `A-Z` / `a-z`
/// only, so a byte outside that range comes back unchanged; `strrev` reverses BYTES. Every
/// expected line is php-src 8.5's own output for the same program.
#[test]
fn test_cli_wasm_unary_string_transforms_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_unary_strings");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function t(string $s): void {
    echo strtoupper($s), "|", strtolower($s), "|", strrev($s), "\n";
}
t("aBc1-z");
t("");
t("a");
t("Hello, World!");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the string transforms to WASM");
    assert!(
        output.status.success(),
        "string transform compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the string transforms under Node");
    assert!(
        run.status.success(),
        "string transforms trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "ABC1-Z|abc1-z|z-1cBa\n",
            "||\n",
            "A|a|a\n",
            "HELLO, WORLD!|hello, world!|!dlroW ,olleH\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the LENGTH-CHANGING string transforms reproduce php-src byte for byte.
///
/// Each result is printed through `bin2hex` so a wrong byte cannot hide behind a terminal's
/// rendering, and the samples pin the edges that separate these from a naive implementation:
/// `addslashes` escapes NUL to the two characters `\0`; `stripslashes` turns `\0` into a NUL
/// byte but `\n` into the letter n, and drops a trailing lone backslash; `nl2br` keeps the break
/// it tags and treats `\r\n` as one. Raw high bytes are included because they are exactly what a
/// data segment written from Rust's UTF-8 rather than the PHP bytes would corrupt.
#[test]
fn test_cli_wasm_re_encoding_string_transforms_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_re_encoding_strings");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function t(string $s): void {
    echo bin2hex($s), "|", bin2hex(addslashes($s)), "|", bin2hex(stripslashes($s)), "|", bin2hex(nl2br($s)), "\n";
}
t("");
t("abc");
t("a'b\"c\\d");
t("x\ny\r\nz\rw");
t("\n\r");
t("\x00\x01\xff");
t("\\0");
t("a\\");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the re-encoding transforms to WASM");
    assert!(
        output.status.success(),
        "re-encoding transform compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the re-encoding transforms under Node");
    assert!(
        run.status.success(),
        "re-encoding transforms trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "|||\n",
            "616263|616263|616263|616263\n",
            "61276222635c64|615c27625c22635c5c64|612762226364|61276222635c64\n",
            "780a790d0a7a0d77|780a790d0a7a0d77|780a790d0a7a0d77|783c6272202f3e0a793c6272202f3e0d0a7a3c6272202f3e0d77\n",
            "0a0d|0a0d|0a0d|3c6272202f3e0a0d\n",
            "0001ff|5c3001ff|0001ff|0001ff\n",
            "5c30|5c5c30|00|5c30\n",
            "615c|615c5c|61|615c\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the string-shaping builtins reproduce php-src byte for byte.
///
/// The samples pin what a naive implementation gets wrong. `ucwords` treats VERTICAL TAB as a
/// word delimiter but not `-`, `_` or `.`; `trim`'s default set includes NUL and vertical tab,
/// while an explicitly EMPTY charlist strips nothing. `strcmp` reports the raw UNSIGNED byte
/// distance at the first mismatch — -32 for `ABC` against `abc`, 254 for `\xff` against `\x01` —
/// but normalizes a pure length difference to +/-1. `substr` answers the empty string rather than
/// false for every out-of-range case, a negative offset counts from the end and saturates, and a
/// negative length names an end offset from the right.
#[test]
fn test_cli_wasm_string_shaping_builtins_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_string_shaping");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function t(string $s): void {
    echo bin2hex($s), "|", bin2hex(ucfirst($s)), "|", bin2hex(lcfirst($s)), "|", bin2hex(ucwords($s)),
         "|", bin2hex(trim($s)), "|", bin2hex(ltrim($s)), "|", bin2hex(rtrim($s)), "\n";
}
t(""); t("a"); t("abc"); t("ABC"); t("hello world  foo"); t(" \t x \n "); t("\x00\x0bz\x0b\x00"); t("h\xc3\xa9llo"); t("123abc");
t("a\tb\nc\rd\x0ce\x0bf g"); t("a-b_c.d");
function c(string $a, string $b): void { echo strcmp($a, $b), "|", strcasecmp($a, $b), "\n"; }
c("a","a"); c("a","b"); c("b","a"); c("","a"); c("a",""); c("",""); c("abc","abd"); c("ABC","abc");
c("a","A"); c("abc","ab"); c("\xff","\x01"); c("abcd","a"); c("ab","abcdefgh"); c("Z","a"); c("_","a");
function s2(string $x, int $o): void { echo bin2hex(substr($x, $o)), "\n"; }
function s3(string $x, int $o, int $n): void { echo bin2hex(substr($x, $o, $n)), "\n"; }
s2("hello",0); s2("hello",2); s2("hello",-2); s2("hello",5); s2("hello",6); s2("hello",-9); s2("",0); s2("",3);
s3("hello",1,3); s3("hello",1,-1); s3("hello",-3,2); s3("hello",0,-5); s3("hello",0,-9); s3("hello",2,0); s3("hello",2,99); s3("hello",-2,-1);
function tc(string $x, string $cl): void { echo bin2hex(trim($x,$cl)), "|", bin2hex(ltrim($x,$cl)), "|", bin2hex(rtrim($x,$cl)), "\n"; }
tc("xxhelloxx","x"); tc("abcHELLOcba","abc"); tc("hello",""); tc("aaa","a");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the shaping builtins to WASM");
    assert!(
        output.status.success(),
        "shaping builtin compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the shaping builtins under Node");
    assert!(
        run.status.success(),
        "shaping builtins trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "||||||\n",
            "61|41|61|41|61|61|61\n",
            "616263|416263|616263|416263|616263|616263|616263\n",
            "414243|414243|614243|414243|414243|414243|414243\n",
            "68656c6c6f20776f726c642020666f6f|48656c6c6f20776f726c642020666f6f|68656c6c6f20776f726c642020666f6f|48656c6c6f20576f726c642020466f6f|68656c6c6f20776f726c642020666f6f|68656c6c6f20776f726c642020666f6f|68656c6c6f20776f726c642020666f6f\n",
            "20092078200a20|20092078200a20|20092078200a20|20092058200a20|78|78200a20|20092078\n",
            "000b7a0b00|000b7a0b00|000b7a0b00|000b5a0b00|7a|7a0b00|000b7a\n",
            "68c3a96c6c6f|48c3a96c6c6f|68c3a96c6c6f|48c3a96c6c6f|68c3a96c6c6f|68c3a96c6c6f|68c3a96c6c6f\n",
            "313233616263|313233616263|313233616263|313233616263|313233616263|313233616263|313233616263\n",
            "6109620a630d640c650b662067|4109620a630d640c650b662067|6109620a630d640c650b662067|4109420a430d440c450b462047|6109620a630d640c650b662067|6109620a630d640c650b662067|6109620a630d640c650b662067\n",
            "612d625f632e64|412d625f632e64|612d625f632e64|412d625f632e64|612d625f632e64|612d625f632e64|612d625f632e64\n",
            "0|0\n",
            "-1|-1\n",
            "1|1\n",
            "-1|-1\n",
            "1|1\n",
            "0|0\n",
            "-1|-1\n",
            "-32|0\n",
            "32|0\n",
            "1|1\n",
            "254|254\n",
            "1|1\n",
            "-1|-1\n",
            "-7|25\n",
            "-2|-2\n",
            "68656c6c6f\n",
            "6c6c6f\n",
            "6c6f\n",
            "\n",
            "\n",
            "68656c6c6f\n",
            "\n",
            "\n",
            "656c6c\n",
            "656c6c\n",
            "6c6c\n",
            "\n",
            "\n",
            "\n",
            "6c6c6f\n",
            "6c\n",
            "68656c6c6f|68656c6c6f7878|787868656c6c6f\n",
            "48454c4c4f|48454c4c4f636261|61626348454c4c4f\n",
            "68656c6c6f|68656c6c6f|68656c6c6f\n",
            "||\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `htmlspecialchars` under PHP 8.1+ defaults, invalid UTF-8 included.
///
/// The defaults are `ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML401`, so BOTH quote styles are
/// escaped — `'` becomes `&#039;` rather than passing through as it did before 8.1 — and invalid
/// UTF-8 is replaced with U+FFFD instead of making the call return the empty string. NUL and the
/// control bytes are valid UTF-8 and pass through untouched.
///
/// The substitution span is the subtle part and is WIDER than the usual "maximal subpart": a
/// valid lead absorbs following bytes up to what it announced, stopping only at a byte that could
/// START a sequence. So `"\xc2\xc0"` is ONE replacement while `"\xc2\xc2"` is two, and a byte
/// that can never lead stands alone, making `"\xc0\x80"` two and `"\xf5\x80\x80\x80"` four.
/// A plain continuation-byte test gets 102 byte pairs wrong and passes this sample set anyway,
/// which is why the rule was settled by sweeping every pair rather than by these cases.
#[test]
fn test_cli_wasm_htmlspecialchars_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_htmlspecialchars");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function h(string $s): void { echo bin2hex(htmlspecialchars($s)), "\n"; }
h(""); h("abc"); h("<a href=\"x\">"); h("a&b"); h("it's"); h("<>&\"'");
h("h\xc3\xa9llo"); h("\xff\xfe"); h("a\xffb"); h("\x00\x01"); h("&amp;");
h("\xc3"); h("\xc3\x28"); h("\xe2\x82"); h("\xe2\x82\x28"); h("\xf0\x9f"); h("\xf0\x9f\x92"); h("\xf0\x9f\x92\xa9");
h("\xc0\x80"); h("\xed\xa0\x80"); h("\xf5\x80\x80\x80"); h("\xc2\x80"); h("\xe0\x80\x80"); h("\xf4\x90\x80\x80");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile htmlspecialchars to WASM");
    assert!(
        output.status.success(),
        "htmlspecialchars compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run htmlspecialchars under Node");
    assert!(
        run.status.success(),
        "htmlspecialchars trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "\n",
            "616263\n",
            "266c743b6120687265663d2671756f743b782671756f743b2667743b\n",
            "6126616d703b62\n",
            "697426233033393b73\n",
            "266c743b2667743b26616d703b2671756f743b26233033393b\n",
            "68c3a96c6c6f\n",
            "efbfbdefbfbd\n",
            "61efbfbd62\n",
            "0001\n",
            "26616d703b616d703b\n",
            "efbfbd\n",
            "efbfbd28\n",
            "efbfbd\n",
            "efbfbd28\n",
            "efbfbd\n",
            "efbfbd\n",
            "f09f92a9\n",
            "efbfbdefbfbd\n",
            "efbfbd\n",
            "efbfbdefbfbdefbfbdefbfbd\n",
            "c280\n",
            "efbfbd\n",
            "efbfbd\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `md5` reproduces php-src, block boundaries included.
///
/// MD5 shares SHA-1's padding SHAPE but reads and writes every word LITTLE-endian, which is the
/// single biggest difference between them and the usual way a port of one into the other goes
/// wrong: a digest that is byte-reversed per word still looks like a plausible hash. The digest
/// bytes come out low-first within each word for the same reason. Lengths either side of every
/// boundary the padding rule turns on are covered, as they are for sha1.
#[test]
fn test_cli_wasm_md5_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_md5");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function h(string $s): void { echo md5($s), "\n"; }
h("");
h("a");
h("abc");
h("message digest");
h("The quick brown fox jumps over the lazy dog");
h("\x00\x01\xff");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile md5 to WASM");
    assert!(
        output.status.success(),
        "md5 compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run md5 under Node");
    assert!(
        run.status.success(),
        "md5 trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own digests, which are the published RFC 1321 test vectors.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "d41d8cd98f00b204e9800998ecf8427e\n",
            "0cc175b9c0f1b6a831c399e269772661\n",
            "900150983cd24fb0d6963f7d28e17f72\n",
            "f96b697d7cb7938d525a2f31aaf161d0\n",
            "9e107d9d372bb6826bd81d3542a419d6\n",
            "ffbb8cd5a232b7d906904533e9609f48\n",
            "eced9e0b81ef2bba605cbc5e2e76a1d0\n",
            "ef1772b6dff9a122358552954ad0df65\n",
            "3b0c8ac703f828b04c6c197006d17218\n",
            "652b906d60af96844ebd21b674f35e93\n",
            "b06521f39153d618550606be297466d5\n",
            "014842d480b571495a4a0363793f7367\n",
            "c743a45e0d2e6a95cb859adae0248435\n",
            "8a7bd0732ed6a28ce75f6dabc90e1613\n",
            "5f61c0ccad4cac44c75ff505e1f1e537\n",
            "020406e1d05cdc2aa287641f7ae2cc39\n",
            "e510683b3f5ffe4093d021808bc6ff70\n",
            "887f30b43b2867f4a9accceee7d16e6c\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `sha1` reproduces php-src, block boundaries included.
///
/// Every SHA-1 word is BIG-endian, which is where an implementation usually diverges, and the
/// padding rule is the other: one `0x80` byte, zeros up to 56 bytes past a 64-byte boundary, then
/// the BIT length as a big-endian 64-bit word. The sample lengths sit either side of every
/// boundary that rule turns on — 55/56/57, 63/64/65, 119/120, 127/128 — because a digest that is
/// right for short inputs and wrong for those is the usual failure.
#[test]
fn test_cli_wasm_sha1_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_sha1");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function h(string $s): void { echo sha1($s), "\n"; }
h("");
h("a");
h("abc");
h("message digest");
h("The quick brown fox jumps over the lazy dog");
h("\x00\x01\xff");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
h("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile sha1 to WASM");
    assert!(
        output.status.success(),
        "sha1 compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run sha1 under Node");
    assert!(
        run.status.success(),
        "sha1 trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own digests, which are the published SHA-1 test vectors.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "da39a3ee5e6b4b0d3255bfef95601890afd80709\n",
            "86f7e437faa5a7fce15d1ddcb9eaeaea377667b8\n",
            "a9993e364706816aba3e25717850c26c9cd0d89d\n",
            "c12252ceda8be8994d5fa0290a47231c1d16aae3\n",
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12\n",
            "c63e8274458bc7501e7c981f6394ced6d4490fda\n",
            "b05d71c64979cb95fa74a33cdb31a40d258ae02e\n",
            "c1c8bbdc22796e28c0e15163d20899b65621d65a\n",
            "c2db330f6083854c99d4b5bfb6e8f29f201be699\n",
            "f08f24908d682555111be7ff6f004e78283d989a\n",
            "03f09f5b158a7a8cdad920bddc29b81c18a551f5\n",
            "0098ba824b5c16427bd7a1122a5a442a25ec644d\n",
            "11655326c708d70319be2610e8a57d9a5b959d3b\n",
            "ee971065aaa017e0632a8ca6c77bb3bf8b1dfc56\n",
            "f34c1488385346a55709ba056ddd08280dd4c6d6\n",
            "89d95fa32ed44a7c610b7ee38517ddf57e0bb975\n",
            "ad5b3fdbcb526778c2839d2f151ea753995e26a0\n",
            "e61cfffe0d9195a525fc6cf06ca2d77119c24a40\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `str_replace` and `crc32` reproduce php-src.
///
/// `str_replace` scans left to right, NON-overlapping, and never rescans what it wrote — which is
/// what makes `str_replace("a", "ab", "a")` answer `"ab"` instead of looping, and
/// `str_replace("ab", "ba", "abab")` answer `"baba"`. An EMPTY search matches nothing and returns
/// the subject, php-src's own guard against that loop. `crc32` is checked against the standard
/// IEEE 802.3 vectors, including the quick-brown-fox one, and answers PHP's UNSIGNED 32-bit
/// value rather than a sign-extended one.
#[test]
fn test_cli_wasm_str_replace_and_crc32_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_str_replace_crc32");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function r(string $se, string $rp, string $su): void { echo "[", str_replace($se, $rp, $su), "]|"; }
r("a","b","aaa"); r("aa","b","aaaa"); r("aa","b","aaa"); r("","x","abc"); r("a","","aaa"); r("abc","x","abcabc"); echo "\n";
r("a","aa","aaa"); r("x","y","abc"); r("a","b",""); r("ab","ba","abab"); r("a","ab","a"); r("\x00","X","a\x00b"); echo "\n";
r("\xc3\xa9","E","h\xc3\xa9llo"); r("ll","LL","hello"); r("o","0","foo bar boo"); echo "\n";
function c(string $s): void { echo crc32($s), "|"; }
c(""); c("a"); c("abc"); c("hello world"); c("\x00\x01\xff"); c("The quick brown fox jumps over the lazy dog"); echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile str_replace/crc32 to WASM");
    assert!(
        output.status.success(),
        "str_replace/crc32 compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run str_replace/crc32 under Node");
    assert!(
        run.status.success(),
        "str_replace/crc32 trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"[bbb]|[bb]|[ba]|[abc]|[]|[xx]|\n".as_slice(),
        b"[aaaaaa]|[abc]|[]|[baba]|[ab]|[aXb]|\n".as_slice(),
        b"[hEllo]|[heLLo]|[f00 bar b00]|\n".as_slice(),
        b"0|3904355907|891568578|222957957|3411544030|1095738169|\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `str_pad` reproduces php-src, including where it does NOT raise.
///
/// The empty-pad `ValueError` fires only when padding is actually needed: `str_pad("abc", 2, "")`
/// answers `"abc"` rather than raising, so the guard tests the target length as well as the pad.
/// A target at or below the current length — including a negative one — returns the subject
/// untouched. The default pad is a single space, synthesized rather than interned, so a module
/// that never calls the two-argument form carries no segment for it.
#[test]
fn test_cli_wasm_str_pad_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_str_pad");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function p2(string $s, int $n): void { echo "[", str_pad($s, $n), "]|"; }
function p3(string $s, int $n, string $p): void { echo "[", str_pad($s, $n, $p), "]|"; }
p2("ab", 5); p2("ab", 1); p2("ab", 2); p2("ab", 0); p2("ab", -3); p2("", 4); echo "\n";
p3("ab", 7, "xy"); p3("ab", 8, "xyz"); p3("a", 6, "12"); p3("abc", 4, "xy"); p3("abc", 3, ""); p3("abc", 2, ""); echo "\n";
p3("", 3, "ab"); p3("abc", 5, " "); p3("h\xc3\xa9", 6, "\x00\x01"); echo "\n";
function guard(string $s, int $n, string $p): void {
    try { echo "[", str_pad($s, $n, $p), "]"; } catch (\ValueError $e) { echo "V:", $e->getMessage(); }
    echo "|";
}
guard("abc", 5, ""); guard("abc", 2, ""); echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile str_pad to WASM");
    assert!(
        output.status.success(),
        "str_pad compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run str_pad under Node");
    if !run.status.success() && String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "the caught ValueError still killed the program: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"[ab   ]|[ab]|[ab]|[ab]|[ab]|[    ]|\n".as_slice(),
        b"[abxyxyx]|[abxyzxyz]|[a12121]|[abcx]|[abc]|[abc]|\n".as_slice(),
        b"[aba]|[abc  ]|[h\xc3\xa9\x00\x01\x00]|\n".as_slice(),
        b"[V:str_pad(): Argument #3 ($pad_string) must not be empty|[abc]|\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `printf` writes the formatted bytes and answers their COUNT.
///
/// It is `sprintf` plus one write, and shares the same builder, so the interesting part is the
/// return value: PHP answers the number of BYTES, not characters, which `printf("h\xc3\xa9")`
/// pins at 3 rather than 2.
#[test]
fn test_cli_wasm_printf_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_printf");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function p(int $n, string $s, float $x): void {
    $a = printf("%d-%s|%.2f\n", $n, $s, $x);
    echo "ret=", $a, "\n";
}
p(42, "ab", 1.5); p(-7, "", 2.675);
$b = printf("literal\n"); echo "ret=", $b, "\n";
$c = printf(""); echo "ret=", $c, "\n";
$d = printf("h\xc3\xa9"); echo "|ret=", $d, "\n";
$e = printf("%05d|%-5s|%+.1f\n", 7, "x", -2.25); echo "ret=", $e, "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile printf to WASM");
    assert!(
        output.status.success(),
        "printf compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run printf under Node");
    assert!(
        run.status.success(),
        "printf trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "42-ab|1.50\n",
            "ret=11\n",
            "-7-|2.67\n",
            "ret=9\n",
            "literal\n",
            "ret=8\n",
            "ret=0\n",
            "hé|ret=3\n",
            "00007|x    |-2.2\n",
            "ret=17\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `sprintf`'s `%f` rounds the EXACT binary value with ties-to-even.
///
/// This is C's rule and NOT `number_format`'s, which is the distinction worth pinning:
/// `sprintf("%.2f", 2.675)` is 2.67 because the double is really 2.67499…, while
/// `number_format(2.675, 2)` is 2.68 because it rounds the shortest decimal that round-trips.
/// Ties go to even, so `%.0f` gives 0 for 0.5, 2 for 1.5 AND 2 for 2.5.
///
/// Non-finite values IGNORE the field entirely — `%08.2f` of INF is `INF`, not `00000INF` — and
/// PHP spells NaN with that capitalisation. A true zero drops its sign, so `-0.0` prints `0.00`,
/// while a negative value that merely rounds to zero keeps it.
#[test]
fn test_cli_wasm_sprintf_float_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_sprintf_float");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function f(float $v): void {
    echo "[", sprintf("%f", $v), "][", sprintf("%.0f", $v), "][", sprintf("%.2f", $v), "][", sprintf("%10.2f", $v), "][", sprintf("%-10.2f", $v), "][", sprintf("%010.2f", $v), "][", sprintf("%+.2f", $v), "]\n";
}
f(0.0); f(1.5); f(-1.5); f(2.5); f(0.125); f(-0.125); f(2.675); f(1234.5678); f(9.99); f(-0.4);
echo sprintf("%08.2f", INF), "|", sprintf("%-8.2f", -INF), "|", sprintf("%+.2f", NAN), "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile sprintf %f to WASM");
    assert!(
        output.status.success(),
        "sprintf %f compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run sprintf %f under Node");
    assert!(
        run.status.success(),
        "sprintf %f trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "[0.000000][0][0.00][      0.00][0.00      ][0000000.00][+0.00]\n",
            "[1.500000][2][1.50][      1.50][1.50      ][0000001.50][+1.50]\n",
            "[-1.500000][-2][-1.50][     -1.50][-1.50     ][-000001.50][-1.50]\n",
            "[2.500000][2][2.50][      2.50][2.50      ][0000002.50][+2.50]\n",
            "[0.125000][0][0.12][      0.12][0.12      ][0000000.12][+0.12]\n",
            "[-0.125000][-0][-0.12][     -0.12][-0.12     ][-000000.12][-0.12]\n",
            "[2.675000][3][2.67][      2.67][2.67      ][0000002.67][+2.67]\n",
            "[1234.567800][1235][1234.57][   1234.57][1234.57   ][0001234.57][+1234.57]\n",
            "[9.990000][10][9.99][      9.99][9.99      ][0000009.99][+9.99]\n",
            "[-0.400000][-0][-0.40][     -0.40][-0.40     ][-000000.40][-0.40]\n",
            "INF|-INF|NaN\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `sprintf` reproduces php-src's formatting, which is NOT C's.
///
/// The format is required to be a LITERAL, so it is parsed once at compile time and the module
/// carries a fixed sequence of appends rather than a format interpreter — which is what an AOT
/// compiler should do with a format it already knows. A computed format is refused by the audit.
///
/// Three rules here are php-src's and not C's, each measured before the parser was written:
/// the LAST padding flag wins, so `%'x03d` pads with zeros while `%0'x3d` pads with `x`; `-`
/// cancels a ZERO pad on `%d` but NOT on `%s`, so `%-08d` is space-padded while `%-03s` is
/// zero-padded; and zeros go AFTER the sign while spaces go before it, making `%05d` of -7
/// come out `-0007`.
#[test]
fn test_cli_wasm_sprintf_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_sprintf");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function d(int $n): void { echo "[", sprintf("%d", $n), "][", sprintf("%5d", $n), "][", sprintf("%-5d", $n), "][", sprintf("%05d", $n), "][", sprintf("%+d", $n), "]\n"; }
function s(string $v): void { echo "[", sprintf("%s", $v), "][", sprintf("%5s", $v), "][", sprintf("%-5s", $v), "][", sprintf("%.2s", $v), "][", sprintf("%05s", $v), "]\n"; }
d(0); d(7); d(-7); d(12345);
s(""); s("ab"); s("abcdef");
echo sprintf("a%%b"), "|", sprintf("%s-%d", "x", 5), "|", sprintf("%2\$s %1\$s", "world", "hello"), "|", sprintf("%1\$s%1\$s", "ab"), "\n";
echo sprintf("literal"), "|", sprintf("%d%%", 50), "|", sprintf("[%5s|%-5d]", "ab", 7), "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile sprintf to WASM");
    assert!(
        output.status.success(),
        "sprintf compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run sprintf under Node");
    assert!(
        run.status.success(),
        "sprintf trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "[0][    0][0    ][00000][+0]\n",
            "[7][    7][7    ][00007][+7]\n",
            "[-7][   -7][-7   ][-0007][-7]\n",
            "[12345][12345][12345][12345][+12345]\n",
            "[][     ][     ][][00000]\n",
            "[ab][   ab][ab   ][ab][000ab]\n",
            "[abcdef][abcdef][abcdef][ab][abcdef]\n",
            "a%b|x-5|hello world|abab\n",
            "literal|50%|[   ab|7    ]\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `wordwrap` reproduces php-src's in-place line breaking.
///
/// The transform REPLACES a space with the break rather than inserting one, so the result has the
/// same length as the subject. That is why `wordwrap("a ", 1)` is `"a\n"` — the trailing space
/// becomes the break — and why a word longer than the width is left whole: with no space to
/// consume, there is nowhere to break without growing the string.
///
/// Consecutive spaces are where a plausible implementation diverges. `wordwrap("a  b", 1)` is
/// `"a\n b"` but `wordwrap("a  b", 2)` is `"a \nb"`: the break lands on whichever space first
/// reaches the width, and the other survives as content. A width of zero or less is not an error.
///
/// The algorithm was derived from php-src's fast path and checked against 400 random subjects
/// over the alphabet `{a, b, space, newline}` before any of it was written in WAT.
#[test]
fn test_cli_wasm_wordwrap_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_wordwrap");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function w(string $s, int $n): void { echo "[", wordwrap($s, $n), "]|[", wordwrap($s, $n), "]\n"; }
function w1(string $s): void { echo "[", wordwrap($s), "]\n"; }
w("The quick brown fox", 10); w("The quick brown fox", 1); w("abcdefghij", 3);
w("a b c d e", 3); w("", 5); w("short", 99); w("aa bb cc", 5); w("  lead", 3);
w("a  b", 1); w("a  b", 2); w("a ", 1); w("  ", 1); w("x  ", 2);
w("one two\nthree four", 5); w("a b c", 0);
w1("a b c"); w1("");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile wordwrap to WASM");
    assert!(
        output.status.success(),
        "wordwrap compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run wordwrap under Node");
    assert!(
        run.status.success(),
        "wordwrap trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"[The quick\n".as_slice(),
        b"brown fox]|[The quick\n".as_slice(),
        b"brown fox]\n".as_slice(),
        b"[The\n".as_slice(),
        b"quick\n".as_slice(),
        b"brown\n".as_slice(),
        b"fox]|[The\n".as_slice(),
        b"quick\n".as_slice(),
        b"brown\n".as_slice(),
        b"fox]\n".as_slice(),
        b"[abcdefghij]|[abcdefghij]\n".as_slice(),
        b"[a b\n".as_slice(),
        b"c d\n".as_slice(),
        b"e]|[a b\n".as_slice(),
        b"c d\n".as_slice(),
        b"e]\n".as_slice(),
        b"[]|[]\n".as_slice(),
        b"[short]|[short]\n".as_slice(),
        b"[aa bb\n".as_slice(),
        b"cc]|[aa bb\n".as_slice(),
        b"cc]\n".as_slice(),
        b"[ \n".as_slice(),
        b"lead]|[ \n".as_slice(),
        b"lead]\n".as_slice(),
        b"[a\n".as_slice(),
        b" b]|[a\n".as_slice(),
        b" b]\n".as_slice(),
        b"[a \n".as_slice(),
        b"b]|[a \n".as_slice(),
        b"b]\n".as_slice(),
        b"[a\n".as_slice(),
        b"]|[a\n".as_slice(),
        b"]\n".as_slice(),
        b"[ \n".as_slice(),
        b"]|[ \n".as_slice(),
        b"]\n".as_slice(),
        b"[x \n".as_slice(),
        b"]|[x \n".as_slice(),
        b"]\n".as_slice(),
        b"[one\n".as_slice(),
        b"two\n".as_slice(),
        b"three\n".as_slice(),
        b"four]|[one\n".as_slice(),
        b"two\n".as_slice(),
        b"three\n".as_slice(),
        b"four]\n".as_slice(),
        b"[a\n".as_slice(),
        b"b\n".as_slice(),
        b"c]|[a\n".as_slice(),
        b"b\n".as_slice(),
        b"c]\n".as_slice(),
        b"[a b c]\n".as_slice(),
        b"[]\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `str_split` cuts into chunks the way php-src does, empty subject included.
///
/// The final chunk is SHORT when the length does not divide evenly, and an EMPTY subject yields
/// the EMPTY array — PHP 8.2's behaviour, and the opposite of `explode`, whose tail is always
/// pushed so `explode(",", "")` is `[""]`. A chunk length below one raises php-src's ValueError
/// rather than being clamped.
///
/// Each helper calls `str_split` TWICE per invocation, once for the count and once for the
/// contents, so a lowering whose scratch locals collide on a second call fails to assemble here.
#[test]
fn test_cli_wasm_str_split_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_str_split");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function s2(string $x, int $n): void { echo count(str_split($x, $n)), ":", implode("|", str_split($x, $n)), " "; }
function s1(string $x): void { echo count(str_split($x)), ":", implode("|", str_split($x)), " "; }
s2("abcdef",1); s2("abcdef",2); s2("abcdef",3); s2("abcdef",4); s2("abcdef",6); s2("abcdef",99); echo "\n";
s2("",1); s2("",5); s2("a",1); s2("ab",1); s2("abc",2); s2("h\xc3\xa9llo",2); echo "\n";
s1("abc"); s1(""); s1("\x00\x01"); echo "\n";
function guard(string $x, int $n): void {
    try { echo implode("|", str_split($x, $n)); } catch (\ValueError $e) { echo "V:", $e->getMessage(); }
    echo " ";
}
guard("abc", 0); guard("abc", -1); echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile str_split to WASM");
    assert!(
        output.status.success(),
        "str_split compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run str_split under Node");
    if !run.status.success() && String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "the caught ValueError still killed the program: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"6:a|b|c|d|e|f 3:ab|cd|ef 2:abc|def 2:abcd|ef 1:abcdef 1:abcdef \n".as_slice(),
        b"0: 0: 1:a 2:a|b 2:ab|c 3:h\xc3|\xa9l|lo \n".as_slice(),
        b"3:a|b|c 0: 2:\x00|\x01 \n".as_slice(),
        b"V:str_split(): Argument #2 ($length) must be greater than 0 V:str_split(): Argument #2 ($length) must be greater than 0 \n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `explode` builds php-src's array, empty pieces included.
///
/// Every separator is a boundary, so a leading or trailing one yields an EMPTY element rather
/// than being trimmed, and the tail after the last separator is always pushed — which is why
/// `explode(",", "")` is `[""]`, one empty element, and never the empty array. The results are
/// read back through `implode`, so a wrong element COUNT shows up as well as wrong contents.
///
/// An empty separator raises php-src's ValueError outright, unlike `str_pad`'s empty pad which
/// only raises when it would be used: there is no split it could mean, and the scan would not
/// advance. The `$limit` form is refused — a positive limit caps the count with the remainder in
/// the last element, a negative one drops from the END, and zero behaves as one.
#[test]
fn test_cli_wasm_explode_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_explode");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function e(string $sep, string $s): void { echo "[", implode("|", explode($sep, $s)), "]"; }
e(",", "a,b,c"); e(",", "a"); e(",", ""); e(",", ",a"); e(",", "a,"); echo "\n";
e(",", ",,"); e("--", "a--b"); e(",", "a,,b"); e("ab", "1ab2ab3"); e("\x00", "a\x00b"); echo "\n";
function guard(string $sep, string $s): void {
    try { echo "[", implode("|", explode($sep, $s)), "]"; } catch (\ValueError $x) { echo "V:", $x->getMessage(); }
}
guard("", "abc"); echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("compilation failed to run");
    assert!(
        output.status.success(),
        "compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run under Node");
    if !run.status.success() && String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"[a|b|c][a][][|a][a|]\n".as_slice(),
        b"[||][a|b][a||b][1|2|3][a|b]\n".as_slice(),
        b"[V:explode(): Argument #1 ($separator) must not be empty\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a builtin that needs scratch locals can be called TWICE in one function.
///
/// Each of these lowerings spills operands it has to read more than once. Naming those locals
/// made two calls in the same function declare the same local twice, which WebAssembly rejects —
/// so the module failed to assemble rather than answering wrongly. Every earlier test happened to
/// call each builtin once per function and missed it entirely; this one calls each of them twice.
#[test]
fn test_cli_wasm_scratch_using_builtins_survive_repeated_calls() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_builtin_twice");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function all(string $a, string $b): void {
    echo str_repeat($a, 2), "|", str_repeat($b, 3), "|";
    echo str_pad($a, 5, "-"), "|", str_pad($b, 6, "."), "|";
    $p = strpos($a, "x"); $q = strpos($b, "y");
    echo $p === false ? "F" : "@", $q === false ? "F" : "@", "|";
    $r = strrpos($a, "x"); $t = strrpos($b, "y");
    echo $r === false ? "F" : "@", $t === false ? "F" : "@", "|";
    $u = strstr($a, "x"); $v = strstr($b, "y", true);
    echo $u === false ? "F" : "S", $v === false ? "F" : "S", "|";
    echo implode(",", explode("-", $a)), "|", implode(":", explode("-", $b)), "\n";
}
all("xa-xb", "cy-dy");
all("no", "ne");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("compilation failed to run");
    assert!(
        output.status.success(),
        "compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run under Node");
    if !run.status.success() && String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"xa-xbxa-xb|cy-dycy-dycy-dy|xa-xb|cy-dy.|@@|@@|SS|xa,xb|cy:dy\n".as_slice(),
        b"nono|nenene|no---|ne....|FF|FF|FF|no|ne\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `implode` joins an indexed string array the way php-src does.
///
/// This is the first builtin on this target that READS an array, so the shape contract is
/// deliberately narrow: the elements must be exactly `string`, because `__rt_array_get_str` reads
/// a slot as a (pointer, length) pair and a slot holding an int or a boxed Mixed is a different
/// layout. An array of `Never` — the type of a literal `[]` — is admitted alongside it, since the
/// element read never happens and the answer is the empty string.
///
/// The glue goes BETWEEN elements, so there is one fewer glue than elements: an empty array joins
/// to nothing, a single element to itself, and three empty strings joined by `,` give `,,`.
#[test]
fn test_cli_wasm_implode_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_implode");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function j(string $g, array $a): void { echo "[", implode($g, $a), "]|"; }
j(",", ["a","b","c"]); j(",", ["a"]); j("", ["a","b"]); j("--", ["x","y","z"]); echo "\n";
j(",", ["","",""]); j("\x00", ["a","b"]); j("::", ["one"]); j(" ", ["a","b","c","d","e"]); echo "\n";
j(",", ["h\xc3\xa9","llo"]); j("\xff", ["\x00","\x01"]); echo "\n";
$e = [];
echo "[", implode(",", $e), "]\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile implode to WASM");
    assert!(
        output.status.success(),
        "implode compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run implode under Node");
    assert!(
        run.status.success(),
        "implode trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"[a,b,c]|[a]|[ab]|[x--y--z]|\n".as_slice(),
        b"[,,]|[a\x00b]|[one]|[a b c d e]|\n".as_slice(),
        b"[h\xc3\xa9,llo]|[\x00\xff\x01]|\n".as_slice(),
        b"[]\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a HETEROGENEOUS array literal — the thing that makes `array<mixed>` reachable at all.
///
/// EIR pushes RAW scalars into an `array<mixed>`; there is no boxing instruction, so the backend
/// boxes at the push site the way the native one does. Each scalar gets its exact cell tag (int 0,
/// string 1, float 2, bool 3, and `PhpType::Void` — EIR's `const_null` — 8), and `implode` then
/// converts each cell with the same rule as an explicit `(string)` cast, which is what php-src
/// does element by element.
///
/// The reads matter as much as the writes: a Mixed-cell array has 16-byte slots with the cell
/// pointer at slot+0, NOT the 8-byte stride the int accessor walks — reading it wrong silently
/// yields every other element interleaved with nulls rather than trapping. `count` is in here to
/// prove the array's `value_type` survives the build.
#[test]
fn test_cli_wasm_heterogeneous_array_literal_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_literal");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function j(string $g, array $a): void { echo "[", implode($g, $a), "]|"; }
function c(array $a): void { echo count($a), ":", implode(",", $a), "|"; }
j(",", [1, "a", 2.5, true, null]);
j("", [0, "", false, -0.0]);
j("::", [PHP_INT_MAX, "s", PHP_INT_MIN, 0.1, 1e100, -1e-7]);
j("+", ["\x00\xff", 7, "\n", 1.5]);
echo "\n";
c([1, true]); c([true, false, 0]); c([null, true, "x"]); c([false, null]);
j(";", [1, "b", 2.0, null]); j(";", [1, "b", 2.0, null]);
echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile a heterogeneous literal to WASM");
    assert!(
        output.status.success(),
        "heterogeneous literal compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the heterogeneous literal under Node");
    assert!(
        run.status.success(),
        "heterogeneous literal trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"[1,a,2.5,1,]|[0-0]|".as_slice(),
        b"[9223372036854775807::s::-9223372036854775808::0.1::1.0E+100::-1.0E-7]|".as_slice(),
        b"[\x00\xff+7+\n+1.5]|\n".as_slice(),
        b"2:1,1|3:1,,0|3:,1,x|2:,|[1;b;2;]|[1;b;2;]|\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Proves a Mixed-cell array RELEASES its cells, by watching the module's memory not grow.
///
/// This is a negative control, not a smoke test: the boxed elements were leaking entirely and the
/// program still printed the right answer, because nothing on the output path reads the array's
/// `value_type`. Only `__rt_array_free_deep` does — and pushing a bool used to restamp that field
/// to 3 (scalar), which made the deep free skip its child loop and drop every cell on the floor.
///
/// The measurement subtracts the module's DECLARED initial memory, so it isolates runtime growth
/// from the constant data a longer program carries. A bool is in the literal on purpose: it is the
/// element that triggered the restamp.
#[test]
fn test_cli_wasm_mixed_cell_array_releases_its_cells() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_leak");
    let php_path = dir.join("main.php");
    let mut src = String::from(
        "<?php\nfunction j(array $a): void { if (count($a) === 99) { echo \"x\"; } }\n",
    );
    // Unrolled: a counting `for` loop does not compile on this target yet.
    for i in 0..2000 {
        src.push_str(&format!(
            "j([{i}, \"abcdefghij\", 2.5, true, null]);\n"
        ));
    }
    src.push_str("echo \"ok\\n\";\n");
    fs::write(&php_path, src).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the mixed-cell leak probe");
    assert!(
        output.status.success(),
        "leak probe compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(readFileSync(process.argv[2])),
  wasi.getImportObject(),
);
const code = wasi.start(instance);
console.error(`pages=${instance.exports.memory.buffer.byteLength / 65536}`);
process.exitCode = code;
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the leak probe under Node");
    assert!(
        run.status.success(),
        "leak probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, b"ok\n");

    let stderr = String::from_utf8_lossy(&run.stderr);
    let final_pages: usize = stderr
        .split("pages=")
        .nth(1)
        .and_then(|rest| rest.trim().parse().ok())
        .expect("the runner reported the final page count");

    // The declared initial size is the static baseline; anything above it is runtime growth.
    let wat_output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to emit the leak probe's WAT");
    assert!(wat_output.status.success());
    let wat = fs::read_to_string(dir.join("main.wat")).expect("the WAT was written");
    let initial_pages: usize = wat
        .split("(memory (export \"memory\") ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .expect("the module declares its initial memory");

    assert_eq!(
        final_pages, initial_pages,
        "2000 boxed 5-element arrays grew memory from {initial_pages} to {final_pages} pages: \
         the cells are not being released"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Proves a `mixed` ARGUMENT's boxed cell is freed, and that freeing it does not break aliasing.
///
/// This target represents a `mixed` parameter as a heap cell, so passing a concrete scalar has to
/// box one. EIR never asked for that box and so emits no matching release: the cell has exactly
/// one owner, the call site, and every such call used to leak 32 bytes — invisibly, since the
/// program still printed the right answer.
///
/// The release is withheld from callees whose declared return is itself a Mixed cell, because
/// `Terminator::Return` MOVES a value out without increfing: such a callee can hand the very cell
/// back. The other escape routes are safe and are exercised here — copying into a callee local and
/// forwarding to a further call both borrow, and a container push increfs.
#[test]
fn test_cli_wasm_boxed_mixed_argument_is_released() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_boxed_arg");

    // Aliasing first: a callee that hands its parameter back must still answer correctly.
    let alias_path = dir.join("alias.php");
    fs::write(
        &alias_path,
        r#"<?php
function id(mixed $x): mixed { return $x; }
function pick(mixed $a, mixed $b): mixed { return $b; }
function copy_local(mixed $x): void { $y = $x; if ($y === "zz") { echo "q"; } }
function forward(mixed $x): void { copy_local($x); }
echo (id("hello") === "hello") ? "y" : "n";
echo (id(42) === 42) ? "y" : "n";
echo (id(2.5) === 2.5) ? "y" : "n";
echo (id(null) === null) ? "y" : "n";
echo (id(true) === true) ? "y" : "n";
echo (pick(1, "b") === "b") ? "y" : "n";
copy_local("hello"); forward("hello"); forward(7);
echo "\n";
"#,
    )
    .unwrap();

    let runner_src = r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(readFileSync(process.argv[2])),
  wasi.getImportObject(),
);
const code = wasi.start(instance);
console.error(`pages=${instance.exports.memory.buffer.byteLength / 65536}`);
process.exitCode = code;
"#;
    let runner = dir.join("run.mjs");
    fs::write(&runner, runner_src).unwrap();

    let compile = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&alias_path)
        .output()
        .expect("failed to compile the aliasing probe");
    assert!(
        compile.status.success(),
        "aliasing probe compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let alias_run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("alias.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the aliasing probe under Node");
    assert!(
        alias_run.status.success(),
        "aliasing probe trapped: {}",
        String::from_utf8_lossy(&alias_run.stderr)
    );
    // php-src 8.5.6's own bytes.
    assert_eq!(alias_run.stdout, b"yyyyyy\n");

    // Then the release itself, watched as runtime memory growth.
    let leak_path = dir.join("leak.php");
    let mut src = String::from(
        "<?php\nfunction m(mixed $x): void { if ($x === \"zz\") { echo \"y\"; } }\n",
    );
    // Unrolled: a counting `for` loop does not compile on this target yet.
    for i in 0..1000 {
        src.push_str(&format!("m({i}); m(\"abcdefghij\"); m(2.5); m(null); m(true);\n"));
    }
    src.push_str("echo \"ok\\n\";\n");
    fs::write(&leak_path, src).unwrap();

    let compile = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&leak_path)
        .output()
        .expect("failed to compile the boxed-argument leak probe");
    assert!(
        compile.status.success(),
        "leak probe compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let wat_output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg("--emit-asm")
        .arg(&leak_path)
        .output()
        .expect("failed to emit the leak probe's WAT");
    assert!(wat_output.status.success());

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("leak.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the leak probe under Node");
    assert!(
        run.status.success(),
        "leak probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, b"ok\n");

    let stderr = String::from_utf8_lossy(&run.stderr);
    let final_pages: usize = stderr
        .split("pages=")
        .nth(1)
        .and_then(|rest| rest.trim().parse().ok())
        .expect("the runner reported the final page count");
    let wat = fs::read_to_string(dir.join("leak.wat")).expect("the WAT was written");
    let initial_pages: usize = wat
        .split("(memory (export \"memory\") ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .expect("the module declares its initial memory");

    assert_eq!(
        final_pages, initial_pages,
        "5000 boxed `mixed` arguments grew memory from {initial_pages} to {final_pages} pages: \
         the boxed cells are not being released"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an `array<T>` handed to an `array<mixed>` parameter is CONVERTED, not reinterpreted.
///
/// This target specializes array storage per element type — int and bool arrays use 8-byte slots,
/// string arrays 16-byte (pointer, length) pairs, and `mixed` a `value_type`-7 array of boxed
/// cells. So passing one where another is expected is a real element-wise conversion; treating it
/// as a pointer copy would read the wrong slot layout without trapping. An empty literal's
/// `array<never>` widens too: there is nothing to convert.
///
/// The conversion allocates, and the callee only borrows, so the call site frees the copy
/// afterwards — withheld when the callee's declared return is itself an array, since a returned
/// value moves out without an incref.
#[test]
fn test_cli_wasm_array_widens_to_mixed_parameter() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_widen");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function j(string $g, array $a): void { echo count($a), "[", implode($g, $a), "]|"; }
j(",", [1, "a", 2.5, true, null]);
j(",", ["x", "y", "z"]);
j("-", [1, 2, 3]);
j("", []);
j(",", [true, false, true]);
j("|", ["only"]);
j(",", [7]);
j("::", ["a", "", "b"]);
j(",", [1, "mix", 2]);
echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the array-widening probe");
    assert!(
        output.status.success(),
        "array-widening compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the array-widening probe under Node");
    assert!(
        run.status.success(),
        "array-widening probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(
        run.stdout,
        b"5[1,a,2.5,1,]|3[x,y,z]|3[1-2-3]|0[]|3[1,,1]|1[only]|1[7]|3[a::::b]|3[1,mix,2]|\n"
            .to_vec()
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the smallest magnitudes render exactly, where the digit buffer used to overflow.
///
/// `__rt_f64_digits` writes 9-byte chunks LEFTWARDS from the end of its buffer, so the buffer has
/// to cover `ceil(digits/9)*9`, not the digit count. The worst case is `p == 1074`, where `J` has
/// up to 767 digits — 86 chunks, 774 bytes — and the buffer was sized 768.
///
/// Undersizing did not trap: the cursor went negative, the chunks landed BEFORE the buffer, and
/// the leading-zero strip compared a negative start with `i32.ge_u`, read it as a huge unsigned
/// value and exited at once. `1e-308` printed as `0.0000001E-301` — right value, unnormalized —
/// and the leading zeros then ate the 14 significant digits.
#[test]
fn test_cli_wasm_smallest_floats_render_like_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_tiny_floats");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function s(mixed $m): void { echo implode("", [$m]), "|"; }
s(1e-307); s(1.5e-307); s(1e-308); s(1.5e-308); s(2.2e-308); s(5e-308);
s(1e-309); s(1.5e-309); s(2.2e-309); s(5e-309); s(9.99e-309);
s(1e-310); s(1e-320); s(5e-324); s(2.2250738585072014e-308);
s(PHP_FLOAT_MIN); s(PHP_FLOAT_MAX); s(PHP_FLOAT_EPSILON);
echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the tiny-float probe");
    assert!(
        output.status.success(),
        "tiny-float compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the tiny-float probe under Node");
    assert!(
        run.status.success(),
        "tiny-float probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected = concat!(
        "1.0E-307|1.5E-307|1.0E-308|1.5E-308|2.2E-308|5.0E-308|",
        "1.0E-309|1.5E-309|2.2E-309|5.0E-309|9.99E-309|",
        "1.0E-310|9.9998886718268E-321|4.9406564584125E-324|2.2250738585072E-308|",
        "2.2250738585072E-308|1.7976931348623E+308|2.2204460492503E-16|\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an array of FLOAT elements, which had no lowered storage at all.
///
/// A float shares the int slot width — the payload is the f64's bits — so this is
/// `__rt_array_push_int` plus the `value_type` 2 stamp that records which it is, matching the
/// native layout this is byte-identical to. `implode` renders each element with the same rule as
/// an explicit `(string)` cast, which for a float only `__rt_mixed_cast_string` knows, so a float
/// slot is boxed into a throwaway tag-2 cell and cast through it.
#[test]
fn test_cli_wasm_float_element_arrays_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_float_array");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function j(string $g, array $a): void { echo count($a), "[", implode($g, $a), "]|"; }
j(",", [1.0, 100.0, 0.5, 1e15, 1e16, 1e-5]);
j("-", [2.5]);
j(",", [0.1, -0.0, 1e100, -1e-7, 3.14159265358979]);
j("", [1.5, 2.5]);
j(",", [INF, -INF, NAN]);
j(",", [PHP_FLOAT_EPSILON, PHP_FLOAT_MAX, PHP_FLOAT_MIN]);
j(",", [1.5, 2.5]);
echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the float-array probe");
    assert!(
        output.status.success(),
        "float-array compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the float-array probe under Node");
    assert!(
        run.status.success(),
        "float-array probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected = concat!(
        "6[1,100,0.5,1.0E+15,1.0E+16,1.0E-5]|1[2.5]|",
        "5[0.1,-0,1.0E+100,-1.0E-7,3.1415926535898]|2[1.52.5]|",
        "3[INF,-INF,NAN]|",
        "3[2.2204460492503E-16,1.7976931348623E+308,2.2250738585072E-308]|2[1.5,2.5]|\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_slice` over a list, whose offset/length rules are `substr`'s exactly.
///
/// Validated as a MODEL first: a transcription of `substr`'s clamping was checked against php-src
/// on 52 offset/length pairs before any WAT was written, and matched all 52. A negative offset
/// counts from the end and floors at 0, an offset at or past the end gives an empty result, a
/// negative length drops that many from the end, and a length is clamped so the window never runs
/// past the end or backwards.
///
/// `PHP_INT_MIN` is in here because both bounds have to be clamped into `[-n, n]` BEFORE any
/// arithmetic — negating `PHP_INT_MIN` wraps an i64, and the clamp is what makes the rest safe.
#[test]
fn test_cli_wasm_array_slice_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_slice");
    let php_path = dir.join("main.php");
    fs::write(&php_path, PHP_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the array_slice probe");
    assert!(
        output.status.success(),
        "array_slice compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the array_slice probe under Node");
    assert!(
        run.status.success(),
        "array_slice probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), PHP_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The `array_slice` probe program: every boundary of the offset/length rules.
const PHP_SOURCE: &str = r##"<?php
$s = [10,20,30,40,50];
$v0 = array_slice($s, -9); echo count($v0), ":", implode(",", $v0), "|";
$v1 = array_slice($s, -5); echo count($v1), ":", implode(",", $v1), "|";
$v2 = array_slice($s, -1); echo count($v2), ":", implode(",", $v2), "|";
$v3 = array_slice($s, 0); echo count($v3), ":", implode(",", $v3), "|";
$v4 = array_slice($s, 1); echo count($v4), ":", implode(",", $v4), "|";
$v5 = array_slice($s, 4); echo count($v5), ":", implode(",", $v5), "|";
$v6 = array_slice($s, 5); echo count($v6), ":", implode(",", $v6), "|";
$v7 = array_slice($s, 9); echo count($v7), ":", implode(",", $v7), "|";
$v8 = array_slice($s, -6, -6); echo count($v8), ":", implode(",", $v8), "|";
$v9 = array_slice($s, -6, -1); echo count($v9), ":", implode(",", $v9), "|";
$v10 = array_slice($s, -6, 0); echo count($v10), ":", implode(",", $v10), "|";
$v11 = array_slice($s, -6, 1); echo count($v11), ":", implode(",", $v11), "|";
$v12 = array_slice($s, -6, 3); echo count($v12), ":", implode(",", $v12), "|";
$v13 = array_slice($s, -6, 7); echo count($v13), ":", implode(",", $v13), "|";
$v14 = array_slice($s, -1, -6); echo count($v14), ":", implode(",", $v14), "|";
$v15 = array_slice($s, -1, -1); echo count($v15), ":", implode(",", $v15), "|";
$v16 = array_slice($s, -1, 0); echo count($v16), ":", implode(",", $v16), "|";
$v17 = array_slice($s, -1, 1); echo count($v17), ":", implode(",", $v17), "|";
$v18 = array_slice($s, -1, 3); echo count($v18), ":", implode(",", $v18), "|";
$v19 = array_slice($s, -1, 7); echo count($v19), ":", implode(",", $v19), "|";
$v20 = array_slice($s, 0, -6); echo count($v20), ":", implode(",", $v20), "|";
$v21 = array_slice($s, 0, -1); echo count($v21), ":", implode(",", $v21), "|";
$v22 = array_slice($s, 0, 0); echo count($v22), ":", implode(",", $v22), "|";
$v23 = array_slice($s, 0, 1); echo count($v23), ":", implode(",", $v23), "|";
$v24 = array_slice($s, 0, 3); echo count($v24), ":", implode(",", $v24), "|";
$v25 = array_slice($s, 0, 7); echo count($v25), ":", implode(",", $v25), "|";
$v26 = array_slice($s, 2, -6); echo count($v26), ":", implode(",", $v26), "|";
$v27 = array_slice($s, 2, -1); echo count($v27), ":", implode(",", $v27), "|";
$v28 = array_slice($s, 2, 0); echo count($v28), ":", implode(",", $v28), "|";
$v29 = array_slice($s, 2, 1); echo count($v29), ":", implode(",", $v29), "|";
$v30 = array_slice($s, 2, 3); echo count($v30), ":", implode(",", $v30), "|";
$v31 = array_slice($s, 2, 7); echo count($v31), ":", implode(",", $v31), "|";
$v32 = array_slice($s, PHP_INT_MIN); echo count($v32), ":", implode(",", $v32), "|";
$v33 = array_slice($s, PHP_INT_MAX); echo count($v33), ":", implode(",", $v33), "|";
$v34 = array_slice($s, 0, PHP_INT_MIN); echo count($v34), ":", implode(",", $v34), "|";
$v35 = array_slice($s, 0, PHP_INT_MAX); echo count($v35), ":", implode(",", $v35), "|";
$v36 = array_slice($s, PHP_INT_MIN, PHP_INT_MAX); echo count($v36), ":", implode(",", $v36), "|";
echo "\n";
"##;

/// php-src 8.5.6's own output for `PHP_SOURCE`.
const PHP_EXPECTED: &str = r##"5:10,20,30,40,50|5:10,20,30,40,50|1:50|5:10,20,30,40,50|4:20,30,40,50|1:50|0:|0:|0:|4:10,20,30,40|0:|1:10|3:10,20,30|5:10,20,30,40,50|0:|0:|0:|1:50|1:50|1:50|0:|4:10,20,30,40|0:|1:10|3:10,20,30|5:10,20,30,40,50|0:|2:30,40|0:|1:30|3:30,40,50|3:30,40,50|5:10,20,30,40,50|0:|0:|5:10,20,30,40,50|5:10,20,30,40,50|
"##;

/// Verifies `array_merge` over lists, and that both operands survive it intact.
///
/// Unlike `+`, which keeps the left's keys and takes only the right's surplus tail, `array_merge`
/// APPENDS every element of the right and reindexes. The two share their element-copy walk, and
/// that walk is where ownership can go wrong in the direction that double-frees rather than leaks:
/// a string element is re-persisted so the result owns its own copy, and a refcounted child is
/// increfed.
///
/// Mixed elements are in here because they live in 16-BYTE slots: reading them at the scalar
/// stride and appending them as scalars wrote into the middle of the previous slot, which showed
/// up as `[1, "x", 2.5]` merging to `1,x,,,` — a corrupted element the source arrays still held
/// correctly.
#[test]
fn test_cli_wasm_array_merge_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_merge");
    let php_path = dir.join("main.php");
    fs::write(&php_path, MERGE_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the array_merge probe");
    assert!(
        output.status.success(),
        "array_merge compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the array_merge probe under Node");
    assert!(
        run.status.success(),
        "array_merge probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), MERGE_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The `array_merge` probe program: every lowered element type, plus both operands re-read after.
const MERGE_SOURCE: &str = r##"<?php
$a = [1,2]; $b = [3,4,5]; $e = [];
$s1 = ["xx","y"]; $s2 = ["z"];
$f1 = [1.5, 2.5]; $f2 = [3.5];
$m1 = [1, "x", 2.5]; $m2 = [true, null];
$r1 = array_merge($a, $b);   echo count($r1), ":", implode(",", $r1), "|";
$r2 = array_merge($a, $e);   echo count($r2), ":", implode(",", $r2), "|";
$r3 = array_merge($e, $b);   echo count($r3), ":", implode(",", $r3), "|";
$r4 = array_merge($e, $e);   echo count($r4), ":", implode(",", $r4), "|";
$r5 = array_merge($s1, $s2); echo count($r5), ":", implode(",", $r5), "|";
$r6 = array_merge($f1, $f2); echo count($r6), ":", implode(",", $r6), "|";
$r7 = array_merge($m1, $m2); echo count($r7), ":", implode(",", $r7), "|";
$r8 = array_merge($a, $a);   echo count($r8), ":", implode(",", $r8), "|";
echo "\n";
echo count($a), count($b), count($s1), count($s2), count($m1), count($m2), "\n";
echo implode(",", $s1), ";", implode(",", $m1), "\n";
"##;

/// php-src 8.5.6's own output for `MERGE_SOURCE`.
const MERGE_EXPECTED: &str = r##"5:1,2,3,4,5|2:1,2|3:3,4,5|0:|3:xx,y,z|3:1.5,2.5,3.5|5:1,x,2.5,1,|4:1,2,1,2|
232132
xx,y;1,x,2.5
"##;

/// Verifies `range` over integers, in both directions and at the i64 boundaries.
///
/// Only the two-bound form exists — the front-end rejects every other arity — so the step is
/// always 1 and the DIRECTION comes from the operands: `range(5, 1)` counts down. A single-element
/// range is `range(n, n)`, which is why the count is the span plus one.
///
/// `PHP_INT_MIN`/`PHP_INT_MAX` bounds are here because the span is computed with wrapping
/// arithmetic: a range spanning more than `i64::MAX` elements cannot have its count represented,
/// and asks for a layout the allocator is guaranteed to reject rather than looping forever.
#[test]
fn test_cli_wasm_range_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_range");
    let php_path = dir.join("main.php");
    fs::write(&php_path, RANGE_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the range probe");
    assert!(
        output.status.success(),
        "range compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the range probe under Node");
    assert!(
        run.status.success(),
        "range probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), RANGE_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The `range` probe program: both directions, single-element, and the i64 boundaries.
const RANGE_SOURCE: &str = r##"<?php
function p(array $r): void { echo count($r), ":", implode(",", $r), "|"; }
p(range(1, 5)); p(range(5, 1)); p(range(0, 0)); p(range(-3, 2)); p(range(2, -3));
p(range(-1, -1)); p(range(100, 97)); p(range(PHP_INT_MAX - 2, PHP_INT_MAX));
p(range(PHP_INT_MIN, PHP_INT_MIN + 2)); p(range(PHP_INT_MAX, PHP_INT_MAX - 3));
foreach (range(1, 4) as $n) { echo $n, "."; }
echo "|"; p(range(1, 5)); echo "\n";
"##;

/// php-src 8.5.6's own output for `RANGE_SOURCE`.
const RANGE_EXPECTED: &str = r##"5:1,2,3,4,5|5:5,4,3,2,1|1:0|6:-3,-2,-1,0,1,2|6:2,1,0,-1,-2,-3|1:-1|4:100,99,98,97|3:9223372036854775805,9223372036854775806,9223372036854775807|3:-9223372036854775808,-9223372036854775807,-9223372036854775806|4:9223372036854775807,9223372036854775806,9223372036854775805,9223372036854775804|1.2.3.4.|5:1,2,3,4,5|
"##;

/// Verifies `==` and `!=` — PHP's LOOSE comparison — over the pairs whose rule was measured.
///
/// The string rule is php-src's `zendi_smart_strcmp`, transcribed and validated on 3000 pairs
/// against 8.5.6: 1600 from this systematic matrix and 1400 randomly generated. The naive reading
/// — "both numeric, so compare the numbers" — passes a 625-pair sample and is STILL WRONG, which
/// is why the sweep was widened; php-src additionally tracks `oflow`, set only for an
/// INTEGRAL-form string whose magnitude escapes i64, and uses it to settle the comparison without
/// converting.
///
/// That is what separates the two rules this test pins side by side:
///   "9223372036854775807" == "9223372036854775808"   is FALSE (integral form, oflow)
///   "9223372036854775807" == "9.2233720368547758e18" is TRUE  (float form, no oflow)
///   PHP_INT_MAX          == 9.2233720368547758e18    is TRUE  (values, plain widening)
///
/// KNOWN GAP, deliberately kept out of the matrix: `__rt_digits_to_f64` documents that it flushes
/// magnitudes below 1e-308 to zero, so a SUBNORMAL numeric string parses to 0.0 and
/// `"9.22e-312" == "0"` answers true where php-src answers false. That is the parser's deferral,
/// not the comparison's — the random sweep found exactly that one case out of 1400.
#[test]
fn test_cli_wasm_loose_equality_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_loose_eq");
    let php_path = dir.join("main.php");
    fs::write(&php_path, LOOSE_EQ_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the loose-equality probe");
    assert!(
        output.status.success(),
        "loose-equality compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the loose-equality probe under Node");
    assert!(
        run.status.success(),
        "loose-equality probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), LOOSE_EQ_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The loose-equality probe: a 40-string matrix (1600 pairs) then the scalar pairs.
const LOOSE_EQ_SOURCE: &str = r##"<?php
function s(string $a, string $b): void { echo ($a == $b) ? "1" : "0"; }
function f(float $a, float $b): void { echo ($a == $b) ? "1" : "0"; }
function b(bool $a, bool $b): void { echo ($a == $b) ? "1" : "0"; }
function m(int $a, float $b): void { echo ($a == $b) ? "1" : "0"; }
function n(float $a, int $b): void { echo ($a == $b) ? "1" : "0"; }
s("", "");
s("", "0");
s("", "00");
s("", "0.0");
s("", "abc");
s("", "10");
s("", "1e1");
s("", "10.0");
s("", " 10");
s("", "10 ");
s("", "+10");
s("", "-0");
s("", "0x1A");
s("", "1e400");
s("", "1e500");
s("", "-1e400");
s("", "9223372036854775807");
s("", "9223372036854775808");
s("", "9223372036854775809");
s("", "9.2233720368547758e18");
s("", "9223372036854775808.0");
s("", "-9223372036854775808");
s("", "-9223372036854775809");
s("", "-9.2233720368547758e18");
s("", "18446744073709551616");
s("", "10abc");
s("", " ");
s("", ".5");
s("", "5.");
s("", "1E1");
s("", "0.1");
s("", "1e-1");
s("", "NAN");
s("", "INF");
s("", "007");
s("", "7");
s("", "+0");
s("", "-0.0");
s("", "0e0");
s("", "1e0");
s("0", "");
s("0", "0");
s("0", "00");
s("0", "0.0");
s("0", "abc");
s("0", "10");
s("0", "1e1");
s("0", "10.0");
s("0", " 10");
s("0", "10 ");
s("0", "+10");
s("0", "-0");
s("0", "0x1A");
s("0", "1e400");
s("0", "1e500");
s("0", "-1e400");
s("0", "9223372036854775807");
s("0", "9223372036854775808");
s("0", "9223372036854775809");
s("0", "9.2233720368547758e18");
s("0", "9223372036854775808.0");
s("0", "-9223372036854775808");
s("0", "-9223372036854775809");
s("0", "-9.2233720368547758e18");
s("0", "18446744073709551616");
s("0", "10abc");
s("0", " ");
s("0", ".5");
s("0", "5.");
s("0", "1E1");
s("0", "0.1");
s("0", "1e-1");
s("0", "NAN");
s("0", "INF");
s("0", "007");
s("0", "7");
s("0", "+0");
s("0", "-0.0");
s("0", "0e0");
s("0", "1e0");
s("00", "");
s("00", "0");
s("00", "00");
s("00", "0.0");
s("00", "abc");
s("00", "10");
s("00", "1e1");
s("00", "10.0");
s("00", " 10");
s("00", "10 ");
s("00", "+10");
s("00", "-0");
s("00", "0x1A");
s("00", "1e400");
s("00", "1e500");
s("00", "-1e400");
s("00", "9223372036854775807");
s("00", "9223372036854775808");
s("00", "9223372036854775809");
s("00", "9.2233720368547758e18");
s("00", "9223372036854775808.0");
s("00", "-9223372036854775808");
s("00", "-9223372036854775809");
s("00", "-9.2233720368547758e18");
s("00", "18446744073709551616");
s("00", "10abc");
s("00", " ");
s("00", ".5");
s("00", "5.");
s("00", "1E1");
s("00", "0.1");
s("00", "1e-1");
s("00", "NAN");
s("00", "INF");
s("00", "007");
s("00", "7");
s("00", "+0");
s("00", "-0.0");
s("00", "0e0");
s("00", "1e0");
s("0.0", "");
s("0.0", "0");
s("0.0", "00");
s("0.0", "0.0");
s("0.0", "abc");
s("0.0", "10");
s("0.0", "1e1");
s("0.0", "10.0");
s("0.0", " 10");
s("0.0", "10 ");
s("0.0", "+10");
s("0.0", "-0");
s("0.0", "0x1A");
s("0.0", "1e400");
s("0.0", "1e500");
s("0.0", "-1e400");
s("0.0", "9223372036854775807");
s("0.0", "9223372036854775808");
s("0.0", "9223372036854775809");
s("0.0", "9.2233720368547758e18");
s("0.0", "9223372036854775808.0");
s("0.0", "-9223372036854775808");
s("0.0", "-9223372036854775809");
s("0.0", "-9.2233720368547758e18");
s("0.0", "18446744073709551616");
s("0.0", "10abc");
s("0.0", " ");
s("0.0", ".5");
s("0.0", "5.");
s("0.0", "1E1");
s("0.0", "0.1");
s("0.0", "1e-1");
s("0.0", "NAN");
s("0.0", "INF");
s("0.0", "007");
s("0.0", "7");
s("0.0", "+0");
s("0.0", "-0.0");
s("0.0", "0e0");
s("0.0", "1e0");
s("abc", "");
s("abc", "0");
s("abc", "00");
s("abc", "0.0");
s("abc", "abc");
s("abc", "10");
s("abc", "1e1");
s("abc", "10.0");
s("abc", " 10");
s("abc", "10 ");
s("abc", "+10");
s("abc", "-0");
s("abc", "0x1A");
s("abc", "1e400");
s("abc", "1e500");
s("abc", "-1e400");
s("abc", "9223372036854775807");
s("abc", "9223372036854775808");
s("abc", "9223372036854775809");
s("abc", "9.2233720368547758e18");
s("abc", "9223372036854775808.0");
s("abc", "-9223372036854775808");
s("abc", "-9223372036854775809");
s("abc", "-9.2233720368547758e18");
s("abc", "18446744073709551616");
s("abc", "10abc");
s("abc", " ");
s("abc", ".5");
s("abc", "5.");
s("abc", "1E1");
s("abc", "0.1");
s("abc", "1e-1");
s("abc", "NAN");
s("abc", "INF");
s("abc", "007");
s("abc", "7");
s("abc", "+0");
s("abc", "-0.0");
s("abc", "0e0");
s("abc", "1e0");
s("10", "");
s("10", "0");
s("10", "00");
s("10", "0.0");
s("10", "abc");
s("10", "10");
s("10", "1e1");
s("10", "10.0");
s("10", " 10");
s("10", "10 ");
s("10", "+10");
s("10", "-0");
s("10", "0x1A");
s("10", "1e400");
s("10", "1e500");
s("10", "-1e400");
s("10", "9223372036854775807");
s("10", "9223372036854775808");
s("10", "9223372036854775809");
s("10", "9.2233720368547758e18");
s("10", "9223372036854775808.0");
s("10", "-9223372036854775808");
s("10", "-9223372036854775809");
s("10", "-9.2233720368547758e18");
s("10", "18446744073709551616");
s("10", "10abc");
s("10", " ");
s("10", ".5");
s("10", "5.");
s("10", "1E1");
s("10", "0.1");
s("10", "1e-1");
s("10", "NAN");
s("10", "INF");
s("10", "007");
s("10", "7");
s("10", "+0");
s("10", "-0.0");
s("10", "0e0");
s("10", "1e0");
s("1e1", "");
s("1e1", "0");
s("1e1", "00");
s("1e1", "0.0");
s("1e1", "abc");
s("1e1", "10");
s("1e1", "1e1");
s("1e1", "10.0");
s("1e1", " 10");
s("1e1", "10 ");
s("1e1", "+10");
s("1e1", "-0");
s("1e1", "0x1A");
s("1e1", "1e400");
s("1e1", "1e500");
s("1e1", "-1e400");
s("1e1", "9223372036854775807");
s("1e1", "9223372036854775808");
s("1e1", "9223372036854775809");
s("1e1", "9.2233720368547758e18");
s("1e1", "9223372036854775808.0");
s("1e1", "-9223372036854775808");
s("1e1", "-9223372036854775809");
s("1e1", "-9.2233720368547758e18");
s("1e1", "18446744073709551616");
s("1e1", "10abc");
s("1e1", " ");
s("1e1", ".5");
s("1e1", "5.");
s("1e1", "1E1");
s("1e1", "0.1");
s("1e1", "1e-1");
s("1e1", "NAN");
s("1e1", "INF");
s("1e1", "007");
s("1e1", "7");
s("1e1", "+0");
s("1e1", "-0.0");
s("1e1", "0e0");
s("1e1", "1e0");
s("10.0", "");
s("10.0", "0");
s("10.0", "00");
s("10.0", "0.0");
s("10.0", "abc");
s("10.0", "10");
s("10.0", "1e1");
s("10.0", "10.0");
s("10.0", " 10");
s("10.0", "10 ");
s("10.0", "+10");
s("10.0", "-0");
s("10.0", "0x1A");
s("10.0", "1e400");
s("10.0", "1e500");
s("10.0", "-1e400");
s("10.0", "9223372036854775807");
s("10.0", "9223372036854775808");
s("10.0", "9223372036854775809");
s("10.0", "9.2233720368547758e18");
s("10.0", "9223372036854775808.0");
s("10.0", "-9223372036854775808");
s("10.0", "-9223372036854775809");
s("10.0", "-9.2233720368547758e18");
s("10.0", "18446744073709551616");
s("10.0", "10abc");
s("10.0", " ");
s("10.0", ".5");
s("10.0", "5.");
s("10.0", "1E1");
s("10.0", "0.1");
s("10.0", "1e-1");
s("10.0", "NAN");
s("10.0", "INF");
s("10.0", "007");
s("10.0", "7");
s("10.0", "+0");
s("10.0", "-0.0");
s("10.0", "0e0");
s("10.0", "1e0");
s(" 10", "");
s(" 10", "0");
s(" 10", "00");
s(" 10", "0.0");
s(" 10", "abc");
s(" 10", "10");
s(" 10", "1e1");
s(" 10", "10.0");
s(" 10", " 10");
s(" 10", "10 ");
s(" 10", "+10");
s(" 10", "-0");
s(" 10", "0x1A");
s(" 10", "1e400");
s(" 10", "1e500");
s(" 10", "-1e400");
s(" 10", "9223372036854775807");
s(" 10", "9223372036854775808");
s(" 10", "9223372036854775809");
s(" 10", "9.2233720368547758e18");
s(" 10", "9223372036854775808.0");
s(" 10", "-9223372036854775808");
s(" 10", "-9223372036854775809");
s(" 10", "-9.2233720368547758e18");
s(" 10", "18446744073709551616");
s(" 10", "10abc");
s(" 10", " ");
s(" 10", ".5");
s(" 10", "5.");
s(" 10", "1E1");
s(" 10", "0.1");
s(" 10", "1e-1");
s(" 10", "NAN");
s(" 10", "INF");
s(" 10", "007");
s(" 10", "7");
s(" 10", "+0");
s(" 10", "-0.0");
s(" 10", "0e0");
s(" 10", "1e0");
s("10 ", "");
s("10 ", "0");
s("10 ", "00");
s("10 ", "0.0");
s("10 ", "abc");
s("10 ", "10");
s("10 ", "1e1");
s("10 ", "10.0");
s("10 ", " 10");
s("10 ", "10 ");
s("10 ", "+10");
s("10 ", "-0");
s("10 ", "0x1A");
s("10 ", "1e400");
s("10 ", "1e500");
s("10 ", "-1e400");
s("10 ", "9223372036854775807");
s("10 ", "9223372036854775808");
s("10 ", "9223372036854775809");
s("10 ", "9.2233720368547758e18");
s("10 ", "9223372036854775808.0");
s("10 ", "-9223372036854775808");
s("10 ", "-9223372036854775809");
s("10 ", "-9.2233720368547758e18");
s("10 ", "18446744073709551616");
s("10 ", "10abc");
s("10 ", " ");
s("10 ", ".5");
s("10 ", "5.");
s("10 ", "1E1");
s("10 ", "0.1");
s("10 ", "1e-1");
s("10 ", "NAN");
s("10 ", "INF");
s("10 ", "007");
s("10 ", "7");
s("10 ", "+0");
s("10 ", "-0.0");
s("10 ", "0e0");
s("10 ", "1e0");
s("+10", "");
s("+10", "0");
s("+10", "00");
s("+10", "0.0");
s("+10", "abc");
s("+10", "10");
s("+10", "1e1");
s("+10", "10.0");
s("+10", " 10");
s("+10", "10 ");
s("+10", "+10");
s("+10", "-0");
s("+10", "0x1A");
s("+10", "1e400");
s("+10", "1e500");
s("+10", "-1e400");
s("+10", "9223372036854775807");
s("+10", "9223372036854775808");
s("+10", "9223372036854775809");
s("+10", "9.2233720368547758e18");
s("+10", "9223372036854775808.0");
s("+10", "-9223372036854775808");
s("+10", "-9223372036854775809");
s("+10", "-9.2233720368547758e18");
s("+10", "18446744073709551616");
s("+10", "10abc");
s("+10", " ");
s("+10", ".5");
s("+10", "5.");
s("+10", "1E1");
s("+10", "0.1");
s("+10", "1e-1");
s("+10", "NAN");
s("+10", "INF");
s("+10", "007");
s("+10", "7");
s("+10", "+0");
s("+10", "-0.0");
s("+10", "0e0");
s("+10", "1e0");
s("-0", "");
s("-0", "0");
s("-0", "00");
s("-0", "0.0");
s("-0", "abc");
s("-0", "10");
s("-0", "1e1");
s("-0", "10.0");
s("-0", " 10");
s("-0", "10 ");
s("-0", "+10");
s("-0", "-0");
s("-0", "0x1A");
s("-0", "1e400");
s("-0", "1e500");
s("-0", "-1e400");
s("-0", "9223372036854775807");
s("-0", "9223372036854775808");
s("-0", "9223372036854775809");
s("-0", "9.2233720368547758e18");
s("-0", "9223372036854775808.0");
s("-0", "-9223372036854775808");
s("-0", "-9223372036854775809");
s("-0", "-9.2233720368547758e18");
s("-0", "18446744073709551616");
s("-0", "10abc");
s("-0", " ");
s("-0", ".5");
s("-0", "5.");
s("-0", "1E1");
s("-0", "0.1");
s("-0", "1e-1");
s("-0", "NAN");
s("-0", "INF");
s("-0", "007");
s("-0", "7");
s("-0", "+0");
s("-0", "-0.0");
s("-0", "0e0");
s("-0", "1e0");
s("0x1A", "");
s("0x1A", "0");
s("0x1A", "00");
s("0x1A", "0.0");
s("0x1A", "abc");
s("0x1A", "10");
s("0x1A", "1e1");
s("0x1A", "10.0");
s("0x1A", " 10");
s("0x1A", "10 ");
s("0x1A", "+10");
s("0x1A", "-0");
s("0x1A", "0x1A");
s("0x1A", "1e400");
s("0x1A", "1e500");
s("0x1A", "-1e400");
s("0x1A", "9223372036854775807");
s("0x1A", "9223372036854775808");
s("0x1A", "9223372036854775809");
s("0x1A", "9.2233720368547758e18");
s("0x1A", "9223372036854775808.0");
s("0x1A", "-9223372036854775808");
s("0x1A", "-9223372036854775809");
s("0x1A", "-9.2233720368547758e18");
s("0x1A", "18446744073709551616");
s("0x1A", "10abc");
s("0x1A", " ");
s("0x1A", ".5");
s("0x1A", "5.");
s("0x1A", "1E1");
s("0x1A", "0.1");
s("0x1A", "1e-1");
s("0x1A", "NAN");
s("0x1A", "INF");
s("0x1A", "007");
s("0x1A", "7");
s("0x1A", "+0");
s("0x1A", "-0.0");
s("0x1A", "0e0");
s("0x1A", "1e0");
s("1e400", "");
s("1e400", "0");
s("1e400", "00");
s("1e400", "0.0");
s("1e400", "abc");
s("1e400", "10");
s("1e400", "1e1");
s("1e400", "10.0");
s("1e400", " 10");
s("1e400", "10 ");
s("1e400", "+10");
s("1e400", "-0");
s("1e400", "0x1A");
s("1e400", "1e400");
s("1e400", "1e500");
s("1e400", "-1e400");
s("1e400", "9223372036854775807");
s("1e400", "9223372036854775808");
s("1e400", "9223372036854775809");
s("1e400", "9.2233720368547758e18");
s("1e400", "9223372036854775808.0");
s("1e400", "-9223372036854775808");
s("1e400", "-9223372036854775809");
s("1e400", "-9.2233720368547758e18");
s("1e400", "18446744073709551616");
s("1e400", "10abc");
s("1e400", " ");
s("1e400", ".5");
s("1e400", "5.");
s("1e400", "1E1");
s("1e400", "0.1");
s("1e400", "1e-1");
s("1e400", "NAN");
s("1e400", "INF");
s("1e400", "007");
s("1e400", "7");
s("1e400", "+0");
s("1e400", "-0.0");
s("1e400", "0e0");
s("1e400", "1e0");
s("1e500", "");
s("1e500", "0");
s("1e500", "00");
s("1e500", "0.0");
s("1e500", "abc");
s("1e500", "10");
s("1e500", "1e1");
s("1e500", "10.0");
s("1e500", " 10");
s("1e500", "10 ");
s("1e500", "+10");
s("1e500", "-0");
s("1e500", "0x1A");
s("1e500", "1e400");
s("1e500", "1e500");
s("1e500", "-1e400");
s("1e500", "9223372036854775807");
s("1e500", "9223372036854775808");
s("1e500", "9223372036854775809");
s("1e500", "9.2233720368547758e18");
s("1e500", "9223372036854775808.0");
s("1e500", "-9223372036854775808");
s("1e500", "-9223372036854775809");
s("1e500", "-9.2233720368547758e18");
s("1e500", "18446744073709551616");
s("1e500", "10abc");
s("1e500", " ");
s("1e500", ".5");
s("1e500", "5.");
s("1e500", "1E1");
s("1e500", "0.1");
s("1e500", "1e-1");
s("1e500", "NAN");
s("1e500", "INF");
s("1e500", "007");
s("1e500", "7");
s("1e500", "+0");
s("1e500", "-0.0");
s("1e500", "0e0");
s("1e500", "1e0");
s("-1e400", "");
s("-1e400", "0");
s("-1e400", "00");
s("-1e400", "0.0");
s("-1e400", "abc");
s("-1e400", "10");
s("-1e400", "1e1");
s("-1e400", "10.0");
s("-1e400", " 10");
s("-1e400", "10 ");
s("-1e400", "+10");
s("-1e400", "-0");
s("-1e400", "0x1A");
s("-1e400", "1e400");
s("-1e400", "1e500");
s("-1e400", "-1e400");
s("-1e400", "9223372036854775807");
s("-1e400", "9223372036854775808");
s("-1e400", "9223372036854775809");
s("-1e400", "9.2233720368547758e18");
s("-1e400", "9223372036854775808.0");
s("-1e400", "-9223372036854775808");
s("-1e400", "-9223372036854775809");
s("-1e400", "-9.2233720368547758e18");
s("-1e400", "18446744073709551616");
s("-1e400", "10abc");
s("-1e400", " ");
s("-1e400", ".5");
s("-1e400", "5.");
s("-1e400", "1E1");
s("-1e400", "0.1");
s("-1e400", "1e-1");
s("-1e400", "NAN");
s("-1e400", "INF");
s("-1e400", "007");
s("-1e400", "7");
s("-1e400", "+0");
s("-1e400", "-0.0");
s("-1e400", "0e0");
s("-1e400", "1e0");
s("9223372036854775807", "");
s("9223372036854775807", "0");
s("9223372036854775807", "00");
s("9223372036854775807", "0.0");
s("9223372036854775807", "abc");
s("9223372036854775807", "10");
s("9223372036854775807", "1e1");
s("9223372036854775807", "10.0");
s("9223372036854775807", " 10");
s("9223372036854775807", "10 ");
s("9223372036854775807", "+10");
s("9223372036854775807", "-0");
s("9223372036854775807", "0x1A");
s("9223372036854775807", "1e400");
s("9223372036854775807", "1e500");
s("9223372036854775807", "-1e400");
s("9223372036854775807", "9223372036854775807");
s("9223372036854775807", "9223372036854775808");
s("9223372036854775807", "9223372036854775809");
s("9223372036854775807", "9.2233720368547758e18");
s("9223372036854775807", "9223372036854775808.0");
s("9223372036854775807", "-9223372036854775808");
s("9223372036854775807", "-9223372036854775809");
s("9223372036854775807", "-9.2233720368547758e18");
s("9223372036854775807", "18446744073709551616");
s("9223372036854775807", "10abc");
s("9223372036854775807", " ");
s("9223372036854775807", ".5");
s("9223372036854775807", "5.");
s("9223372036854775807", "1E1");
s("9223372036854775807", "0.1");
s("9223372036854775807", "1e-1");
s("9223372036854775807", "NAN");
s("9223372036854775807", "INF");
s("9223372036854775807", "007");
s("9223372036854775807", "7");
s("9223372036854775807", "+0");
s("9223372036854775807", "-0.0");
s("9223372036854775807", "0e0");
s("9223372036854775807", "1e0");
s("9223372036854775808", "");
s("9223372036854775808", "0");
s("9223372036854775808", "00");
s("9223372036854775808", "0.0");
s("9223372036854775808", "abc");
s("9223372036854775808", "10");
s("9223372036854775808", "1e1");
s("9223372036854775808", "10.0");
s("9223372036854775808", " 10");
s("9223372036854775808", "10 ");
s("9223372036854775808", "+10");
s("9223372036854775808", "-0");
s("9223372036854775808", "0x1A");
s("9223372036854775808", "1e400");
s("9223372036854775808", "1e500");
s("9223372036854775808", "-1e400");
s("9223372036854775808", "9223372036854775807");
s("9223372036854775808", "9223372036854775808");
s("9223372036854775808", "9223372036854775809");
s("9223372036854775808", "9.2233720368547758e18");
s("9223372036854775808", "9223372036854775808.0");
s("9223372036854775808", "-9223372036854775808");
s("9223372036854775808", "-9223372036854775809");
s("9223372036854775808", "-9.2233720368547758e18");
s("9223372036854775808", "18446744073709551616");
s("9223372036854775808", "10abc");
s("9223372036854775808", " ");
s("9223372036854775808", ".5");
s("9223372036854775808", "5.");
s("9223372036854775808", "1E1");
s("9223372036854775808", "0.1");
s("9223372036854775808", "1e-1");
s("9223372036854775808", "NAN");
s("9223372036854775808", "INF");
s("9223372036854775808", "007");
s("9223372036854775808", "7");
s("9223372036854775808", "+0");
s("9223372036854775808", "-0.0");
s("9223372036854775808", "0e0");
s("9223372036854775808", "1e0");
s("9223372036854775809", "");
s("9223372036854775809", "0");
s("9223372036854775809", "00");
s("9223372036854775809", "0.0");
s("9223372036854775809", "abc");
s("9223372036854775809", "10");
s("9223372036854775809", "1e1");
s("9223372036854775809", "10.0");
s("9223372036854775809", " 10");
s("9223372036854775809", "10 ");
s("9223372036854775809", "+10");
s("9223372036854775809", "-0");
s("9223372036854775809", "0x1A");
s("9223372036854775809", "1e400");
s("9223372036854775809", "1e500");
s("9223372036854775809", "-1e400");
s("9223372036854775809", "9223372036854775807");
s("9223372036854775809", "9223372036854775808");
s("9223372036854775809", "9223372036854775809");
s("9223372036854775809", "9.2233720368547758e18");
s("9223372036854775809", "9223372036854775808.0");
s("9223372036854775809", "-9223372036854775808");
s("9223372036854775809", "-9223372036854775809");
s("9223372036854775809", "-9.2233720368547758e18");
s("9223372036854775809", "18446744073709551616");
s("9223372036854775809", "10abc");
s("9223372036854775809", " ");
s("9223372036854775809", ".5");
s("9223372036854775809", "5.");
s("9223372036854775809", "1E1");
s("9223372036854775809", "0.1");
s("9223372036854775809", "1e-1");
s("9223372036854775809", "NAN");
s("9223372036854775809", "INF");
s("9223372036854775809", "007");
s("9223372036854775809", "7");
s("9223372036854775809", "+0");
s("9223372036854775809", "-0.0");
s("9223372036854775809", "0e0");
s("9223372036854775809", "1e0");
s("9.2233720368547758e18", "");
s("9.2233720368547758e18", "0");
s("9.2233720368547758e18", "00");
s("9.2233720368547758e18", "0.0");
s("9.2233720368547758e18", "abc");
s("9.2233720368547758e18", "10");
s("9.2233720368547758e18", "1e1");
s("9.2233720368547758e18", "10.0");
s("9.2233720368547758e18", " 10");
s("9.2233720368547758e18", "10 ");
s("9.2233720368547758e18", "+10");
s("9.2233720368547758e18", "-0");
s("9.2233720368547758e18", "0x1A");
s("9.2233720368547758e18", "1e400");
s("9.2233720368547758e18", "1e500");
s("9.2233720368547758e18", "-1e400");
s("9.2233720368547758e18", "9223372036854775807");
s("9.2233720368547758e18", "9223372036854775808");
s("9.2233720368547758e18", "9223372036854775809");
s("9.2233720368547758e18", "9.2233720368547758e18");
s("9.2233720368547758e18", "9223372036854775808.0");
s("9.2233720368547758e18", "-9223372036854775808");
s("9.2233720368547758e18", "-9223372036854775809");
s("9.2233720368547758e18", "-9.2233720368547758e18");
s("9.2233720368547758e18", "18446744073709551616");
s("9.2233720368547758e18", "10abc");
s("9.2233720368547758e18", " ");
s("9.2233720368547758e18", ".5");
s("9.2233720368547758e18", "5.");
s("9.2233720368547758e18", "1E1");
s("9.2233720368547758e18", "0.1");
s("9.2233720368547758e18", "1e-1");
s("9.2233720368547758e18", "NAN");
s("9.2233720368547758e18", "INF");
s("9.2233720368547758e18", "007");
s("9.2233720368547758e18", "7");
s("9.2233720368547758e18", "+0");
s("9.2233720368547758e18", "-0.0");
s("9.2233720368547758e18", "0e0");
s("9.2233720368547758e18", "1e0");
s("9223372036854775808.0", "");
s("9223372036854775808.0", "0");
s("9223372036854775808.0", "00");
s("9223372036854775808.0", "0.0");
s("9223372036854775808.0", "abc");
s("9223372036854775808.0", "10");
s("9223372036854775808.0", "1e1");
s("9223372036854775808.0", "10.0");
s("9223372036854775808.0", " 10");
s("9223372036854775808.0", "10 ");
s("9223372036854775808.0", "+10");
s("9223372036854775808.0", "-0");
s("9223372036854775808.0", "0x1A");
s("9223372036854775808.0", "1e400");
s("9223372036854775808.0", "1e500");
s("9223372036854775808.0", "-1e400");
s("9223372036854775808.0", "9223372036854775807");
s("9223372036854775808.0", "9223372036854775808");
s("9223372036854775808.0", "9223372036854775809");
s("9223372036854775808.0", "9.2233720368547758e18");
s("9223372036854775808.0", "9223372036854775808.0");
s("9223372036854775808.0", "-9223372036854775808");
s("9223372036854775808.0", "-9223372036854775809");
s("9223372036854775808.0", "-9.2233720368547758e18");
s("9223372036854775808.0", "18446744073709551616");
s("9223372036854775808.0", "10abc");
s("9223372036854775808.0", " ");
s("9223372036854775808.0", ".5");
s("9223372036854775808.0", "5.");
s("9223372036854775808.0", "1E1");
s("9223372036854775808.0", "0.1");
s("9223372036854775808.0", "1e-1");
s("9223372036854775808.0", "NAN");
s("9223372036854775808.0", "INF");
s("9223372036854775808.0", "007");
s("9223372036854775808.0", "7");
s("9223372036854775808.0", "+0");
s("9223372036854775808.0", "-0.0");
s("9223372036854775808.0", "0e0");
s("9223372036854775808.0", "1e0");
s("-9223372036854775808", "");
s("-9223372036854775808", "0");
s("-9223372036854775808", "00");
s("-9223372036854775808", "0.0");
s("-9223372036854775808", "abc");
s("-9223372036854775808", "10");
s("-9223372036854775808", "1e1");
s("-9223372036854775808", "10.0");
s("-9223372036854775808", " 10");
s("-9223372036854775808", "10 ");
s("-9223372036854775808", "+10");
s("-9223372036854775808", "-0");
s("-9223372036854775808", "0x1A");
s("-9223372036854775808", "1e400");
s("-9223372036854775808", "1e500");
s("-9223372036854775808", "-1e400");
s("-9223372036854775808", "9223372036854775807");
s("-9223372036854775808", "9223372036854775808");
s("-9223372036854775808", "9223372036854775809");
s("-9223372036854775808", "9.2233720368547758e18");
s("-9223372036854775808", "9223372036854775808.0");
s("-9223372036854775808", "-9223372036854775808");
s("-9223372036854775808", "-9223372036854775809");
s("-9223372036854775808", "-9.2233720368547758e18");
s("-9223372036854775808", "18446744073709551616");
s("-9223372036854775808", "10abc");
s("-9223372036854775808", " ");
s("-9223372036854775808", ".5");
s("-9223372036854775808", "5.");
s("-9223372036854775808", "1E1");
s("-9223372036854775808", "0.1");
s("-9223372036854775808", "1e-1");
s("-9223372036854775808", "NAN");
s("-9223372036854775808", "INF");
s("-9223372036854775808", "007");
s("-9223372036854775808", "7");
s("-9223372036854775808", "+0");
s("-9223372036854775808", "-0.0");
s("-9223372036854775808", "0e0");
s("-9223372036854775808", "1e0");
s("-9223372036854775809", "");
s("-9223372036854775809", "0");
s("-9223372036854775809", "00");
s("-9223372036854775809", "0.0");
s("-9223372036854775809", "abc");
s("-9223372036854775809", "10");
s("-9223372036854775809", "1e1");
s("-9223372036854775809", "10.0");
s("-9223372036854775809", " 10");
s("-9223372036854775809", "10 ");
s("-9223372036854775809", "+10");
s("-9223372036854775809", "-0");
s("-9223372036854775809", "0x1A");
s("-9223372036854775809", "1e400");
s("-9223372036854775809", "1e500");
s("-9223372036854775809", "-1e400");
s("-9223372036854775809", "9223372036854775807");
s("-9223372036854775809", "9223372036854775808");
s("-9223372036854775809", "9223372036854775809");
s("-9223372036854775809", "9.2233720368547758e18");
s("-9223372036854775809", "9223372036854775808.0");
s("-9223372036854775809", "-9223372036854775808");
s("-9223372036854775809", "-9223372036854775809");
s("-9223372036854775809", "-9.2233720368547758e18");
s("-9223372036854775809", "18446744073709551616");
s("-9223372036854775809", "10abc");
s("-9223372036854775809", " ");
s("-9223372036854775809", ".5");
s("-9223372036854775809", "5.");
s("-9223372036854775809", "1E1");
s("-9223372036854775809", "0.1");
s("-9223372036854775809", "1e-1");
s("-9223372036854775809", "NAN");
s("-9223372036854775809", "INF");
s("-9223372036854775809", "007");
s("-9223372036854775809", "7");
s("-9223372036854775809", "+0");
s("-9223372036854775809", "-0.0");
s("-9223372036854775809", "0e0");
s("-9223372036854775809", "1e0");
s("-9.2233720368547758e18", "");
s("-9.2233720368547758e18", "0");
s("-9.2233720368547758e18", "00");
s("-9.2233720368547758e18", "0.0");
s("-9.2233720368547758e18", "abc");
s("-9.2233720368547758e18", "10");
s("-9.2233720368547758e18", "1e1");
s("-9.2233720368547758e18", "10.0");
s("-9.2233720368547758e18", " 10");
s("-9.2233720368547758e18", "10 ");
s("-9.2233720368547758e18", "+10");
s("-9.2233720368547758e18", "-0");
s("-9.2233720368547758e18", "0x1A");
s("-9.2233720368547758e18", "1e400");
s("-9.2233720368547758e18", "1e500");
s("-9.2233720368547758e18", "-1e400");
s("-9.2233720368547758e18", "9223372036854775807");
s("-9.2233720368547758e18", "9223372036854775808");
s("-9.2233720368547758e18", "9223372036854775809");
s("-9.2233720368547758e18", "9.2233720368547758e18");
s("-9.2233720368547758e18", "9223372036854775808.0");
s("-9.2233720368547758e18", "-9223372036854775808");
s("-9.2233720368547758e18", "-9223372036854775809");
s("-9.2233720368547758e18", "-9.2233720368547758e18");
s("-9.2233720368547758e18", "18446744073709551616");
s("-9.2233720368547758e18", "10abc");
s("-9.2233720368547758e18", " ");
s("-9.2233720368547758e18", ".5");
s("-9.2233720368547758e18", "5.");
s("-9.2233720368547758e18", "1E1");
s("-9.2233720368547758e18", "0.1");
s("-9.2233720368547758e18", "1e-1");
s("-9.2233720368547758e18", "NAN");
s("-9.2233720368547758e18", "INF");
s("-9.2233720368547758e18", "007");
s("-9.2233720368547758e18", "7");
s("-9.2233720368547758e18", "+0");
s("-9.2233720368547758e18", "-0.0");
s("-9.2233720368547758e18", "0e0");
s("-9.2233720368547758e18", "1e0");
s("18446744073709551616", "");
s("18446744073709551616", "0");
s("18446744073709551616", "00");
s("18446744073709551616", "0.0");
s("18446744073709551616", "abc");
s("18446744073709551616", "10");
s("18446744073709551616", "1e1");
s("18446744073709551616", "10.0");
s("18446744073709551616", " 10");
s("18446744073709551616", "10 ");
s("18446744073709551616", "+10");
s("18446744073709551616", "-0");
s("18446744073709551616", "0x1A");
s("18446744073709551616", "1e400");
s("18446744073709551616", "1e500");
s("18446744073709551616", "-1e400");
s("18446744073709551616", "9223372036854775807");
s("18446744073709551616", "9223372036854775808");
s("18446744073709551616", "9223372036854775809");
s("18446744073709551616", "9.2233720368547758e18");
s("18446744073709551616", "9223372036854775808.0");
s("18446744073709551616", "-9223372036854775808");
s("18446744073709551616", "-9223372036854775809");
s("18446744073709551616", "-9.2233720368547758e18");
s("18446744073709551616", "18446744073709551616");
s("18446744073709551616", "10abc");
s("18446744073709551616", " ");
s("18446744073709551616", ".5");
s("18446744073709551616", "5.");
s("18446744073709551616", "1E1");
s("18446744073709551616", "0.1");
s("18446744073709551616", "1e-1");
s("18446744073709551616", "NAN");
s("18446744073709551616", "INF");
s("18446744073709551616", "007");
s("18446744073709551616", "7");
s("18446744073709551616", "+0");
s("18446744073709551616", "-0.0");
s("18446744073709551616", "0e0");
s("18446744073709551616", "1e0");
s("10abc", "");
s("10abc", "0");
s("10abc", "00");
s("10abc", "0.0");
s("10abc", "abc");
s("10abc", "10");
s("10abc", "1e1");
s("10abc", "10.0");
s("10abc", " 10");
s("10abc", "10 ");
s("10abc", "+10");
s("10abc", "-0");
s("10abc", "0x1A");
s("10abc", "1e400");
s("10abc", "1e500");
s("10abc", "-1e400");
s("10abc", "9223372036854775807");
s("10abc", "9223372036854775808");
s("10abc", "9223372036854775809");
s("10abc", "9.2233720368547758e18");
s("10abc", "9223372036854775808.0");
s("10abc", "-9223372036854775808");
s("10abc", "-9223372036854775809");
s("10abc", "-9.2233720368547758e18");
s("10abc", "18446744073709551616");
s("10abc", "10abc");
s("10abc", " ");
s("10abc", ".5");
s("10abc", "5.");
s("10abc", "1E1");
s("10abc", "0.1");
s("10abc", "1e-1");
s("10abc", "NAN");
s("10abc", "INF");
s("10abc", "007");
s("10abc", "7");
s("10abc", "+0");
s("10abc", "-0.0");
s("10abc", "0e0");
s("10abc", "1e0");
s(" ", "");
s(" ", "0");
s(" ", "00");
s(" ", "0.0");
s(" ", "abc");
s(" ", "10");
s(" ", "1e1");
s(" ", "10.0");
s(" ", " 10");
s(" ", "10 ");
s(" ", "+10");
s(" ", "-0");
s(" ", "0x1A");
s(" ", "1e400");
s(" ", "1e500");
s(" ", "-1e400");
s(" ", "9223372036854775807");
s(" ", "9223372036854775808");
s(" ", "9223372036854775809");
s(" ", "9.2233720368547758e18");
s(" ", "9223372036854775808.0");
s(" ", "-9223372036854775808");
s(" ", "-9223372036854775809");
s(" ", "-9.2233720368547758e18");
s(" ", "18446744073709551616");
s(" ", "10abc");
s(" ", " ");
s(" ", ".5");
s(" ", "5.");
s(" ", "1E1");
s(" ", "0.1");
s(" ", "1e-1");
s(" ", "NAN");
s(" ", "INF");
s(" ", "007");
s(" ", "7");
s(" ", "+0");
s(" ", "-0.0");
s(" ", "0e0");
s(" ", "1e0");
s(".5", "");
s(".5", "0");
s(".5", "00");
s(".5", "0.0");
s(".5", "abc");
s(".5", "10");
s(".5", "1e1");
s(".5", "10.0");
s(".5", " 10");
s(".5", "10 ");
s(".5", "+10");
s(".5", "-0");
s(".5", "0x1A");
s(".5", "1e400");
s(".5", "1e500");
s(".5", "-1e400");
s(".5", "9223372036854775807");
s(".5", "9223372036854775808");
s(".5", "9223372036854775809");
s(".5", "9.2233720368547758e18");
s(".5", "9223372036854775808.0");
s(".5", "-9223372036854775808");
s(".5", "-9223372036854775809");
s(".5", "-9.2233720368547758e18");
s(".5", "18446744073709551616");
s(".5", "10abc");
s(".5", " ");
s(".5", ".5");
s(".5", "5.");
s(".5", "1E1");
s(".5", "0.1");
s(".5", "1e-1");
s(".5", "NAN");
s(".5", "INF");
s(".5", "007");
s(".5", "7");
s(".5", "+0");
s(".5", "-0.0");
s(".5", "0e0");
s(".5", "1e0");
s("5.", "");
s("5.", "0");
s("5.", "00");
s("5.", "0.0");
s("5.", "abc");
s("5.", "10");
s("5.", "1e1");
s("5.", "10.0");
s("5.", " 10");
s("5.", "10 ");
s("5.", "+10");
s("5.", "-0");
s("5.", "0x1A");
s("5.", "1e400");
s("5.", "1e500");
s("5.", "-1e400");
s("5.", "9223372036854775807");
s("5.", "9223372036854775808");
s("5.", "9223372036854775809");
s("5.", "9.2233720368547758e18");
s("5.", "9223372036854775808.0");
s("5.", "-9223372036854775808");
s("5.", "-9223372036854775809");
s("5.", "-9.2233720368547758e18");
s("5.", "18446744073709551616");
s("5.", "10abc");
s("5.", " ");
s("5.", ".5");
s("5.", "5.");
s("5.", "1E1");
s("5.", "0.1");
s("5.", "1e-1");
s("5.", "NAN");
s("5.", "INF");
s("5.", "007");
s("5.", "7");
s("5.", "+0");
s("5.", "-0.0");
s("5.", "0e0");
s("5.", "1e0");
s("1E1", "");
s("1E1", "0");
s("1E1", "00");
s("1E1", "0.0");
s("1E1", "abc");
s("1E1", "10");
s("1E1", "1e1");
s("1E1", "10.0");
s("1E1", " 10");
s("1E1", "10 ");
s("1E1", "+10");
s("1E1", "-0");
s("1E1", "0x1A");
s("1E1", "1e400");
s("1E1", "1e500");
s("1E1", "-1e400");
s("1E1", "9223372036854775807");
s("1E1", "9223372036854775808");
s("1E1", "9223372036854775809");
s("1E1", "9.2233720368547758e18");
s("1E1", "9223372036854775808.0");
s("1E1", "-9223372036854775808");
s("1E1", "-9223372036854775809");
s("1E1", "-9.2233720368547758e18");
s("1E1", "18446744073709551616");
s("1E1", "10abc");
s("1E1", " ");
s("1E1", ".5");
s("1E1", "5.");
s("1E1", "1E1");
s("1E1", "0.1");
s("1E1", "1e-1");
s("1E1", "NAN");
s("1E1", "INF");
s("1E1", "007");
s("1E1", "7");
s("1E1", "+0");
s("1E1", "-0.0");
s("1E1", "0e0");
s("1E1", "1e0");
s("0.1", "");
s("0.1", "0");
s("0.1", "00");
s("0.1", "0.0");
s("0.1", "abc");
s("0.1", "10");
s("0.1", "1e1");
s("0.1", "10.0");
s("0.1", " 10");
s("0.1", "10 ");
s("0.1", "+10");
s("0.1", "-0");
s("0.1", "0x1A");
s("0.1", "1e400");
s("0.1", "1e500");
s("0.1", "-1e400");
s("0.1", "9223372036854775807");
s("0.1", "9223372036854775808");
s("0.1", "9223372036854775809");
s("0.1", "9.2233720368547758e18");
s("0.1", "9223372036854775808.0");
s("0.1", "-9223372036854775808");
s("0.1", "-9223372036854775809");
s("0.1", "-9.2233720368547758e18");
s("0.1", "18446744073709551616");
s("0.1", "10abc");
s("0.1", " ");
s("0.1", ".5");
s("0.1", "5.");
s("0.1", "1E1");
s("0.1", "0.1");
s("0.1", "1e-1");
s("0.1", "NAN");
s("0.1", "INF");
s("0.1", "007");
s("0.1", "7");
s("0.1", "+0");
s("0.1", "-0.0");
s("0.1", "0e0");
s("0.1", "1e0");
s("1e-1", "");
s("1e-1", "0");
s("1e-1", "00");
s("1e-1", "0.0");
s("1e-1", "abc");
s("1e-1", "10");
s("1e-1", "1e1");
s("1e-1", "10.0");
s("1e-1", " 10");
s("1e-1", "10 ");
s("1e-1", "+10");
s("1e-1", "-0");
s("1e-1", "0x1A");
s("1e-1", "1e400");
s("1e-1", "1e500");
s("1e-1", "-1e400");
s("1e-1", "9223372036854775807");
s("1e-1", "9223372036854775808");
s("1e-1", "9223372036854775809");
s("1e-1", "9.2233720368547758e18");
s("1e-1", "9223372036854775808.0");
s("1e-1", "-9223372036854775808");
s("1e-1", "-9223372036854775809");
s("1e-1", "-9.2233720368547758e18");
s("1e-1", "18446744073709551616");
s("1e-1", "10abc");
s("1e-1", " ");
s("1e-1", ".5");
s("1e-1", "5.");
s("1e-1", "1E1");
s("1e-1", "0.1");
s("1e-1", "1e-1");
s("1e-1", "NAN");
s("1e-1", "INF");
s("1e-1", "007");
s("1e-1", "7");
s("1e-1", "+0");
s("1e-1", "-0.0");
s("1e-1", "0e0");
s("1e-1", "1e0");
s("NAN", "");
s("NAN", "0");
s("NAN", "00");
s("NAN", "0.0");
s("NAN", "abc");
s("NAN", "10");
s("NAN", "1e1");
s("NAN", "10.0");
s("NAN", " 10");
s("NAN", "10 ");
s("NAN", "+10");
s("NAN", "-0");
s("NAN", "0x1A");
s("NAN", "1e400");
s("NAN", "1e500");
s("NAN", "-1e400");
s("NAN", "9223372036854775807");
s("NAN", "9223372036854775808");
s("NAN", "9223372036854775809");
s("NAN", "9.2233720368547758e18");
s("NAN", "9223372036854775808.0");
s("NAN", "-9223372036854775808");
s("NAN", "-9223372036854775809");
s("NAN", "-9.2233720368547758e18");
s("NAN", "18446744073709551616");
s("NAN", "10abc");
s("NAN", " ");
s("NAN", ".5");
s("NAN", "5.");
s("NAN", "1E1");
s("NAN", "0.1");
s("NAN", "1e-1");
s("NAN", "NAN");
s("NAN", "INF");
s("NAN", "007");
s("NAN", "7");
s("NAN", "+0");
s("NAN", "-0.0");
s("NAN", "0e0");
s("NAN", "1e0");
s("INF", "");
s("INF", "0");
s("INF", "00");
s("INF", "0.0");
s("INF", "abc");
s("INF", "10");
s("INF", "1e1");
s("INF", "10.0");
s("INF", " 10");
s("INF", "10 ");
s("INF", "+10");
s("INF", "-0");
s("INF", "0x1A");
s("INF", "1e400");
s("INF", "1e500");
s("INF", "-1e400");
s("INF", "9223372036854775807");
s("INF", "9223372036854775808");
s("INF", "9223372036854775809");
s("INF", "9.2233720368547758e18");
s("INF", "9223372036854775808.0");
s("INF", "-9223372036854775808");
s("INF", "-9223372036854775809");
s("INF", "-9.2233720368547758e18");
s("INF", "18446744073709551616");
s("INF", "10abc");
s("INF", " ");
s("INF", ".5");
s("INF", "5.");
s("INF", "1E1");
s("INF", "0.1");
s("INF", "1e-1");
s("INF", "NAN");
s("INF", "INF");
s("INF", "007");
s("INF", "7");
s("INF", "+0");
s("INF", "-0.0");
s("INF", "0e0");
s("INF", "1e0");
s("007", "");
s("007", "0");
s("007", "00");
s("007", "0.0");
s("007", "abc");
s("007", "10");
s("007", "1e1");
s("007", "10.0");
s("007", " 10");
s("007", "10 ");
s("007", "+10");
s("007", "-0");
s("007", "0x1A");
s("007", "1e400");
s("007", "1e500");
s("007", "-1e400");
s("007", "9223372036854775807");
s("007", "9223372036854775808");
s("007", "9223372036854775809");
s("007", "9.2233720368547758e18");
s("007", "9223372036854775808.0");
s("007", "-9223372036854775808");
s("007", "-9223372036854775809");
s("007", "-9.2233720368547758e18");
s("007", "18446744073709551616");
s("007", "10abc");
s("007", " ");
s("007", ".5");
s("007", "5.");
s("007", "1E1");
s("007", "0.1");
s("007", "1e-1");
s("007", "NAN");
s("007", "INF");
s("007", "007");
s("007", "7");
s("007", "+0");
s("007", "-0.0");
s("007", "0e0");
s("007", "1e0");
s("7", "");
s("7", "0");
s("7", "00");
s("7", "0.0");
s("7", "abc");
s("7", "10");
s("7", "1e1");
s("7", "10.0");
s("7", " 10");
s("7", "10 ");
s("7", "+10");
s("7", "-0");
s("7", "0x1A");
s("7", "1e400");
s("7", "1e500");
s("7", "-1e400");
s("7", "9223372036854775807");
s("7", "9223372036854775808");
s("7", "9223372036854775809");
s("7", "9.2233720368547758e18");
s("7", "9223372036854775808.0");
s("7", "-9223372036854775808");
s("7", "-9223372036854775809");
s("7", "-9.2233720368547758e18");
s("7", "18446744073709551616");
s("7", "10abc");
s("7", " ");
s("7", ".5");
s("7", "5.");
s("7", "1E1");
s("7", "0.1");
s("7", "1e-1");
s("7", "NAN");
s("7", "INF");
s("7", "007");
s("7", "7");
s("7", "+0");
s("7", "-0.0");
s("7", "0e0");
s("7", "1e0");
s("+0", "");
s("+0", "0");
s("+0", "00");
s("+0", "0.0");
s("+0", "abc");
s("+0", "10");
s("+0", "1e1");
s("+0", "10.0");
s("+0", " 10");
s("+0", "10 ");
s("+0", "+10");
s("+0", "-0");
s("+0", "0x1A");
s("+0", "1e400");
s("+0", "1e500");
s("+0", "-1e400");
s("+0", "9223372036854775807");
s("+0", "9223372036854775808");
s("+0", "9223372036854775809");
s("+0", "9.2233720368547758e18");
s("+0", "9223372036854775808.0");
s("+0", "-9223372036854775808");
s("+0", "-9223372036854775809");
s("+0", "-9.2233720368547758e18");
s("+0", "18446744073709551616");
s("+0", "10abc");
s("+0", " ");
s("+0", ".5");
s("+0", "5.");
s("+0", "1E1");
s("+0", "0.1");
s("+0", "1e-1");
s("+0", "NAN");
s("+0", "INF");
s("+0", "007");
s("+0", "7");
s("+0", "+0");
s("+0", "-0.0");
s("+0", "0e0");
s("+0", "1e0");
s("-0.0", "");
s("-0.0", "0");
s("-0.0", "00");
s("-0.0", "0.0");
s("-0.0", "abc");
s("-0.0", "10");
s("-0.0", "1e1");
s("-0.0", "10.0");
s("-0.0", " 10");
s("-0.0", "10 ");
s("-0.0", "+10");
s("-0.0", "-0");
s("-0.0", "0x1A");
s("-0.0", "1e400");
s("-0.0", "1e500");
s("-0.0", "-1e400");
s("-0.0", "9223372036854775807");
s("-0.0", "9223372036854775808");
s("-0.0", "9223372036854775809");
s("-0.0", "9.2233720368547758e18");
s("-0.0", "9223372036854775808.0");
s("-0.0", "-9223372036854775808");
s("-0.0", "-9223372036854775809");
s("-0.0", "-9.2233720368547758e18");
s("-0.0", "18446744073709551616");
s("-0.0", "10abc");
s("-0.0", " ");
s("-0.0", ".5");
s("-0.0", "5.");
s("-0.0", "1E1");
s("-0.0", "0.1");
s("-0.0", "1e-1");
s("-0.0", "NAN");
s("-0.0", "INF");
s("-0.0", "007");
s("-0.0", "7");
s("-0.0", "+0");
s("-0.0", "-0.0");
s("-0.0", "0e0");
s("-0.0", "1e0");
s("0e0", "");
s("0e0", "0");
s("0e0", "00");
s("0e0", "0.0");
s("0e0", "abc");
s("0e0", "10");
s("0e0", "1e1");
s("0e0", "10.0");
s("0e0", " 10");
s("0e0", "10 ");
s("0e0", "+10");
s("0e0", "-0");
s("0e0", "0x1A");
s("0e0", "1e400");
s("0e0", "1e500");
s("0e0", "-1e400");
s("0e0", "9223372036854775807");
s("0e0", "9223372036854775808");
s("0e0", "9223372036854775809");
s("0e0", "9.2233720368547758e18");
s("0e0", "9223372036854775808.0");
s("0e0", "-9223372036854775808");
s("0e0", "-9223372036854775809");
s("0e0", "-9.2233720368547758e18");
s("0e0", "18446744073709551616");
s("0e0", "10abc");
s("0e0", " ");
s("0e0", ".5");
s("0e0", "5.");
s("0e0", "1E1");
s("0e0", "0.1");
s("0e0", "1e-1");
s("0e0", "NAN");
s("0e0", "INF");
s("0e0", "007");
s("0e0", "7");
s("0e0", "+0");
s("0e0", "-0.0");
s("0e0", "0e0");
s("0e0", "1e0");
s("1e0", "");
s("1e0", "0");
s("1e0", "00");
s("1e0", "0.0");
s("1e0", "abc");
s("1e0", "10");
s("1e0", "1e1");
s("1e0", "10.0");
s("1e0", " 10");
s("1e0", "10 ");
s("1e0", "+10");
s("1e0", "-0");
s("1e0", "0x1A");
s("1e0", "1e400");
s("1e0", "1e500");
s("1e0", "-1e400");
s("1e0", "9223372036854775807");
s("1e0", "9223372036854775808");
s("1e0", "9223372036854775809");
s("1e0", "9.2233720368547758e18");
s("1e0", "9223372036854775808.0");
s("1e0", "-9223372036854775808");
s("1e0", "-9223372036854775809");
s("1e0", "-9.2233720368547758e18");
s("1e0", "18446744073709551616");
s("1e0", "10abc");
s("1e0", " ");
s("1e0", ".5");
s("1e0", "5.");
s("1e0", "1E1");
s("1e0", "0.1");
s("1e0", "1e-1");
s("1e0", "NAN");
s("1e0", "INF");
s("1e0", "007");
s("1e0", "7");
s("1e0", "+0");
s("1e0", "-0.0");
s("1e0", "0e0");
s("1e0", "1e0");
echo "\n";function i(int $a, int $b): void { echo ($a == $b) ? "1" : "0"; echo ($a != $b) ? "1" : "0"; }
i(1,1); i(1,2); i(0,-0); i(PHP_INT_MAX,PHP_INT_MAX); i(PHP_INT_MIN,PHP_INT_MAX);
echo "|";
f(1.5,1.5); f(1.5,2.5); f(0.0,-0.0); f(NAN,NAN); f(INF,INF); f(INF,-INF); f(NAN,1.0);
echo "|";
b(true,true); b(true,false); b(false,false);
echo "|";
m(1,1.0); m(1,1.5); m(PHP_INT_MAX,9.2233720368547758e18); m(0,-0.0); m(1,NAN); m(2,INF);
echo "|";
n(1.0,1); n(1.5,1); n(-0.0,0); n(NAN,0); n(9.2233720368547758e18,PHP_INT_MAX);
echo "\n";
"##;

/// php-src 8.5.6's own output for `LOOSE_EQ_SOURCE`.
const LOOSE_EQ_EXPECTED: &str = r##"1000000000000000000000000000000000000000011100000001000000000000000000000000111001110000000100000000000000000000000011100111000000010000000000000000000000001110000010000000000000000000000000000000000000000111111000000000000000000100000000000000011111100000000000000000010000000000000001111110000000000000000001000000000000000111111000000000000000000100000000000000011111100000000000000000010000000000000001111110000000000000000001000000000001110000000100000000000000000000000011100000000000001000000000000000000000000000000000000000010000000000000000000000000000000000000000100000000000000000000000000000000000000001000000000000000000000000000000000000000010011000000000000000000000000000000000000101100000000000000000000000000000000000001110000000000000000000000000000000000011111000000000000000000000000000000000001111100000000000000000000000000000000000000001010000000000000000000000000000000000000011000000000000000000000000000000000000011100000000000000000000000000000000000000001000000000000000000000000000000000000000010000000000000000000000000000000000000000100000000000000000000000000000000000000001000000000000000000000000000000000000000010000000000000000111111000000000000000000100000000000000000000000000000000000000001100000000000000000000000000000000000000110000000000000000000000000000000000000000100000000000000000000000000000000000000001000000000000000000000000000000000000000011000000000000000000000000000000000000001100000111000000010000000000000000000000001110011100000001000000000000000000000000111001110000000100000000000000000000000011100000000000000000000000000000000000000001
1001101001|1010100|101|101100|10101
"##;

/// Verifies `in_array` in BOTH forms, over the (needle, element) pairs whose rule was measured.
///
/// It used to be lowered only as a strict identity scan over int slots, because the loose form
/// needs PHP's juggling. It now reuses the very comparison `==` lowers, so the loose form answers
/// the numeric-string rule: `in_array("1e1", ["a","10","b"])` is TRUE loosely and FALSE strictly,
/// and so is `in_array(" 10", ...)` — leading whitespace and all.
///
/// A needle and elements of DIFFERENT types short-circuit under `===`: PHP compares types first,
/// so `in_array(1, [1.0, 2.0], true)` is false without looking at a single element, while the
/// loose form widens and finds it.
///
/// The empty-haystack cases are here because they still have to TYPE-CHECK: the scan takes the
/// needle by value, so its shape follows the needle even when there is nothing to compare against.
#[test]
fn test_cli_wasm_in_array_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_in_array");
    let php_path = dir.join("main.php");
    fs::write(&php_path, IN_ARRAY_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the in_array probe");
    assert!(
        output.status.success(),
        "in_array compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the in_array probe under Node");
    assert!(
        run.status.success(),
        "in_array probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), IN_ARRAY_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The `in_array` probe: each lowered pair, in both forms, plus the empty haystack.
const IN_ARRAY_SOURCE: &str = r##"<?php
$i = [10, 20, 30];      $e = [];
$s = ["a", "10", "b"];  $f = [1.5, 2.5];
echo in_array(20, $i)?"1":"0", in_array(99, $i)?"1":"0", in_array(20, $i, true)?"1":"0", in_array(20, $e)?"1":"0", "|";
echo in_array("a", $s)?"1":"0", in_array("10", $s)?"1":"0", in_array("1e1", $s)?"1":"0", in_array("1e1", $s, true)?"1":"0", in_array("z", $s)?"1":"0", "|";
echo in_array(" 10", $s)?"1":"0", in_array(" 10", $s, true)?"1":"0", in_array("10.0", $s)?"1":"0", "|";
echo in_array(1.5, $f)?"1":"0", in_array(3.5, $f)?"1":"0", in_array(1.5, $f, true)?"1":"0", in_array(1.5, $e)?"1":"0", "|";
echo in_array(1, [1.0, 2.0])?"1":"0", in_array(1, [1.0, 2.0], true)?"1":"0", in_array(1.0, [1, 2])?"1":"0", in_array(1.0, [1, 2], true)?"1":"0", "|";
echo in_array(3, [1.0, 2.0])?"1":"0", in_array(3.0, [1, 2])?"1":"0", "|";
echo "\n";
"##;

/// php-src 8.5.6's own output for `IN_ARRAY_SOURCE`.
const IN_ARRAY_EXPECTED: &str = r##"1010|11100|101|1010|1010|00|
"##;

/// Verifies `array_search`, which shares its scan with `in_array` and boxes the result.
///
/// One scan serves both: it answers the first matching INDEX, which `in_array` reduces to a bool
/// and this boxes. `int|false` travels as a Mixed cell — tag 0 carrying the key, tag 3 carrying
/// false — the same convention `strpos` uses for the same result type, which is why a miss prints
/// as the empty string here.
///
/// Only the LOOSE form exists: the front-end rejects a third operand with "array_search() takes
/// exactly 2 arguments". So the numeric-string rule applies throughout —
/// `array_search("1e1", ["a","10","b"])` is 1, and `array_search(" 10", ...)` matches through the
/// leading whitespace.
#[test]
fn test_cli_wasm_array_search_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_search");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ARRAY_SEARCH_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the array_search probe");
    assert!(
        output.status.success(),
        "array_search compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the array_search probe under Node");
    assert!(
        run.status.success(),
        "array_search probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ARRAY_SEARCH_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The `array_search` probe: hits, misses, the numeric-string rule, and the empty haystack.
const ARRAY_SEARCH_SOURCE: &str = r##"<?php
function p(mixed $m): void { echo implode("", [$m]), "|"; }
$i = [10, 20, 30]; $s = ["a", "10", "b"]; $f = [1.5, 2.5]; $e = [];
p(array_search(20, $i)); p(array_search(99, $i)); p(array_search(10, $i)); p(array_search(30, $i));
p(array_search("1e1", $s)); p(array_search("a", $s)); p(array_search("z", $s)); p(array_search(" 10", $s));
p(array_search(2.5, $f)); p(array_search(9.5, $f));
p(array_search(1, $e));
p(array_search(1, [1.0, 2.0])); p(array_search(2.0, [1, 2]));
echo "\n";
"##;

/// php-src 8.5.6's own output for `ARRAY_SEARCH_SOURCE`.
const ARRAY_SEARCH_EXPECTED: &str = r##"1||0|2|1|0||1|1|||0|1|
"##;

/// Verifies the EMPTY-ARRAY ACCUMULATOR — `$out = []; foreach (...) { $out[] = ...; }`.
///
/// The slot is typed from the empty literal (`array<never>`) and the value from whatever gets
/// pushed, and the two meet at the loop's phi in BOTH directions. This target specializes slot
/// width and value_type per element type, so those transfers looked like a widening and were
/// refused — which turned away one of the most common shapes in PHP.
///
/// They are not a widening: an array whose element type is `never` has no elements and no decided
/// layout, because `__rt_array_push_*` shapes slot width and value_type on the FIRST push. So the
/// pointer is interchangeable with any element type's, and the transfer is a plain copy.
///
/// The float accumulator is here because `foreach` over float elements needed its own load
/// contract, the counterpart of the float array storage.
#[test]
fn test_cli_wasm_empty_array_accumulator_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_accumulator");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ACCUMULATOR_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the accumulator probe");
    assert!(
        output.status.success(),
        "accumulator compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the accumulator probe under Node");
    assert!(
        run.status.success(),
        "accumulator probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ACCUMULATOR_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The accumulator probe: every element type, a filtered build, one that stays empty, one
/// returned, and a realistic slugifier that combines several of the lowered builtins.
const ACCUMULATOR_SOURCE: &str = r##"<?php
function slugify(string $title): string {
    $lower = strtolower(trim($title));
    $parts = explode(" ", $lower);
    $kept = [];
    foreach ($parts as $p) {
        if ($p !== "" && !in_array($p, ["the", "a", "of"])) { $kept[] = $p; }
    }
    return implode("-", $kept);
}
function strs(array $xs): string { $o = []; foreach ($xs as $x) { $o[] = strtoupper($x); } return implode(",", $o); }
function ints(array $xs): string { $o = []; foreach ($xs as $x) { $o[] = $x; } return implode(",", $o); }
function flts(array $xs): string { $o = []; foreach ($xs as $x) { $o[] = $x; } return implode(",", $o); }
function filt(array $xs): string { $o = []; foreach ($xs as $x) { if ($x !== "b") { $o[] = $x; } } return implode(",", $o); }
function empt(array $xs): string { $o = []; foreach ($xs as $x) { if ($x === "zz") { $o[] = $x; } } return count($o) . ":" . implode(",", $o); }
function ret(array $xs): array { $o = []; foreach ($xs as $x) { $o[] = $x; } return $o; }
echo strs(["a","b"]), "|", ints([1,2,3]), "|", flts([1.5,2.5]), "|";
echo filt(["a","b","c"]), "|", empt(["a","b"]), "|";
$r = ret(["p","q"]); echo count($r), ":", implode(",", $r), "|";
$n = []; echo count($n), ":", implode(",", $n), "|";
echo slugify("  The Rise of  Machines "), "\n";
"##;

/// php-src 8.5.6's own output for `ACCUMULATOR_SOURCE`.
const ACCUMULATOR_EXPECTED: &str = r##"A,B|1,2,3|1.5,2.5|a,c|0:|2:p,q|0:|rise-machines
"##;

/// Verifies arrays of OBJECTS end to end: building one, walking it, and reading through it.
///
/// An object is a refcounted container, so its slot holds a pointer under `value_type` 4 — the
/// stamp that makes `__rt_array_free_deep` release each element instead of dropping it. The array
/// takes a SHARE at the push, because the EIR emits `array_push` then `release` of the operand.
///
/// `foreach` binds an OWNED element, so the read increfs. Deciding that from the result's
/// REPRESENTATION alone was wrong: an object pointer is a `Ptr` just like a Mixed cell, so the
/// binding was boxed into a cell and the property read then found an empty slot — right shape,
/// wrong object, and the loop printed nothing.
///
/// A promoted constructor property is admitted although it is typed with no default: the
/// promotion assigns it from the constructor's signature, before the body runs, so no read can
/// precede it. Non-promoted typed properties still need the initialization check and stay refused.
///
/// The Mixed-element loop is here because an untyped `array` parameter widens to cells, which is
/// the shape any function boundary produces.
#[test]
fn test_cli_wasm_object_arrays_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_object_array");
    let php_path = dir.join("main.php");
    fs::write(&php_path, OBJECT_ARRAY_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the object-array probe");
    assert!(
        output.status.success(),
        "object-array compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the object-array probe under Node");
    assert!(
        run.status.success(),
        "object-array probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), OBJECT_ARRAY_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The object-array probe: a method call per element, a string and an int property, an empty
/// array, and a Mixed-element loop.
const OBJECT_ARRAY_SOURCE: &str = r##"<?php
class Item {
    public function __construct(public string $name, public int $qty) {}
    public function label(): string { return $this->name . " x" . $this->qty; }
}
$items = [new Item("bolt", 3), new Item("nut", 7), new Item("pin", 1)];
$out = [];
foreach ($items as $it) { $out[] = $it->label(); }
echo implode("; ", $out), "\n";
$up = [];
foreach ($items as $it) { $up[] = strtoupper($it->name); }
echo implode(", ", $up), "\n";
echo count($items), "\n";
$empty = [];
foreach ($empty as $it) { echo "never"; }
foreach ($items as $it) { echo $it->qty, "."; }
echo "\n";
$m = [1, "hi", 2.5];
foreach ($m as $v) { echo $v, "|"; }
echo "done\n";
"##;

/// php-src 8.5.6's own output for `OBJECT_ARRAY_SOURCE`.
const OBJECT_ARRAY_EXPECTED: &str = r##"bolt x3; nut x7; pin x1
BOLT, NUT, PIN
3
3.7.1.
1|hi|2.5|done
"##;

/// A list of records — `[["name" => ..., "qty" => ...], ...]` — built, iterated, read by key,
/// accumulated from, and walked key-by-key one level down.
#[test]
fn test_cli_wasm_array_of_records_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_records");
    let php_path = dir.join("main.php");
    fs::write(&php_path, RECORD_LIST_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the record-list probe");
    assert!(
        output.status.success(),
        "record-list compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the record-list probe under Node");
    assert!(
        run.status.success(),
        "record-list probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), RECORD_LIST_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The record-list probe. The last loop reads each row through `$k => $v`, which is what
/// proves the row binds as a HASH rather than as a boxed cell.
const RECORD_LIST_SOURCE: &str = r##"<?php
$rows = [["name" => "bolt", "qty" => 3], ["name" => "nut", "qty" => 7], ["name" => "pin", "qty" => 1]];
$total = 0;
foreach ($rows as $r) { $total = $total + $r["qty"]; echo $r["name"], "=", $r["qty"], ";"; }
echo "|", $total, "|", count($rows), "|";
$acc = [];
foreach ($rows as $r2) { $acc[] = $r2["name"]; }
echo implode(",", $acc), "|";
foreach ($rows as $r3) { foreach ($r3 as $k => $v) { echo $k, ":", $v, " "; } }
echo "\n";
"##;

/// php-src 8.5.6's own output for `RECORD_LIST_SOURCE`.
const RECORD_LIST_EXPECTED: &str = "bolt=3;nut=7;pin=1;|11|3|bolt,nut,pin|name:bolt qty:3 name:nut qty:7 name:pin qty:1 \n";

/// A class holding an array collection: `$this->items[] = $v`, `$this->items = []`, and a
/// `void` method whose call expression is used.
///
/// PHP gives a `void` call the value null even though the callee returns nothing, so the
/// emitter supplies it; and clearing to `[]` writes an `array<never>` into an `array<mixed>`
/// slot, which is exact because no element layout is decided until the first push. The last
/// loop rebuilds the object forty times so a stale slot pointer would surface as a dispatch
/// failure rather than a wrong count.
#[test]
fn test_cli_wasm_array_property_collection_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_property");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ARRAY_PROPERTY_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the array-property probe");
    assert!(
        output.status.success(),
        "array-property compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the array-property probe under Node");
    assert!(
        run.status.success(),
        "array-property probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ARRAY_PROPERTY_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The array-property probe. The collection is introduced by CONSTRUCTOR PROMOTION, not by a
/// `= []` property default — that form is still refused, see the note on
/// `object_new_shape_issue`.
const ARRAY_PROPERTY_SOURCE: &str = r##"<?php
class Bag {
    public function __construct(private array $items = []) {}
    public function add(int $v): void { $this->items[] = $v; }
    public function clear(): void { $this->items = []; }
    public function size(): int { return count($this->items); }
}
$b = new Bag();
$r = $b->add(1);
$b->add(2);
echo $b->size(), ",", $r === null ? "null" : "notnull", ";";
$b->clear();
echo $b->size(), ";";
foreach (range(1, 40) as $i) { $t = new Bag(); $t->add(1); $t->add(2); $t->clear(); $t->add(3); echo $t->size(); }
echo "\n";
"##;

/// php-src 8.5.6's own output for `ARRAY_PROPERTY_SOURCE`.
const ARRAY_PROPERTY_EXPECTED: &str = "2,null;0;1111111111111111111111111111111111111111\n";

/// A class holding a `= []` array property, rebuilt sixty times so a stale release surfaces.
///
/// The sixty-iteration loop is the point: the defect this covers was a release walk that read
/// its property count from the HEAP BLOCK, and an object served an oversized free block then
/// walked phantom slots and freed live memory. It answered correctly for the first handful of
/// iterations, so a short loop proves nothing.
#[test]
fn test_cli_wasm_array_property_default_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_array_default");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ARRAY_DEFAULT_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the array-default probe");
    assert!(
        output.status.success(),
        "array-default compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the array-default probe under Node");
    assert!(
        run.status.success(),
        "array-default probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ARRAY_DEFAULT_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The array-default probe. Three properties of different types make the phantom-slot walk
/// reachable, and `clear()` exercises assigning a fresh `[]` over a populated slot.
const ARRAY_DEFAULT_SOURCE: &str = r##"<?php
class Stack {
    private array $items = [];
    private int $pushes = 0;
    private string $label = "st";
    public function push(int $v): void { $this->items[] = $v; $this->pushes = $this->pushes + 1; }
    public function size(): int { return count($this->items); }
    public function all(): array { return $this->items; }
    public function stats(): string { return $this->label . ":" . $this->pushes; }
    public function clear(): void { $this->items = []; }
}
$s = new Stack();
foreach ([3, 1, 4, 1, 5] as $v) { $s->push($v); }
echo $s->size(), ",", implode("-", $s->all()), ",", $s->stats(), ";";
$s->clear();
echo $s->size(), ";";
foreach (range(1, 60) as $i) {
    $t = new Stack();
    $t->push($i);
    $t->push(7);
    echo $t->size();
}
echo "\n";
"##;

/// php-src 8.5.6's own output for `ARRAY_DEFAULT_SOURCE`.
const ARRAY_DEFAULT_EXPECTED: &str = "5,3-1-4-1-5,st:5;0;222222222222222222222222222222222222222222222222222222222222\n";

/// `match` over an ENUM and over `true`, and the fatal an unmatched `match` raises.
#[test]
fn test_cli_wasm_match_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_match");
    let php_path = dir.join("main.php");
    fs::write(&php_path, MATCH_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the match probe");
    assert!(
        output.status.success(),
        "match compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the match probe under Node");
    assert!(
        run.status.success(),
        "match probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), MATCH_EXPECTED);

    // A `match` with no arm taken terminates. PHP names the value and the file; the EIR
    // interns the shorter text the NATIVE backend also prints, so the two targets agree.
    let unmatched = dir.join("unmatched.php");
    fs::write(
        &unmatched,
        "<?php\nfunction f(int $n): string { return match ($n) { 1 => \"one\" }; }\necho f(1);\necho f(9);\n",
    )
    .unwrap();
    let compiled = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&unmatched)
        .output()
        .expect("failed to compile the unmatched probe");
    assert!(compiled.status.success());
    let fell_through = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("unmatched.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the unmatched probe under Node");
    assert_eq!(
        fell_through.status.code(),
        Some(255),
        "an unmatched match must exit with PHP's fatal status"
    );
    assert_eq!(
        String::from_utf8_lossy(&fell_through.stdout),
        "one",
        "output before the fatal must still be flushed"
    );
    assert!(
        String::from_utf8_lossy(&fell_through.stderr).contains("unhandled match case"),
        "the interned fatal text must reach stderr: {}",
        String::from_utf8_lossy(&fell_through.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The match probe. Matching on an ENUM compares singleton identity, and `match (true)` is
/// PHP's idiom for a guard ladder.
const MATCH_SOURCE: &str = r##"<?php
enum Suit: string { case H = "h"; case S = "s"; case C = "c"; }
function name(Suit $s): string { return match ($s) { Suit::H => "hearts", Suit::S => "spades", Suit::C => "clubs" }; }
function grade(int $n): string { return match (true) { $n >= 90 => "A", $n >= 80 => "B", default => "C" }; }
echo name(Suit::H), ",", name(Suit::S), ",", name(Suit::C), ";";
echo grade(95), grade(85), grade(10), "\n";
"##;

/// php-src 8.5.6's own output for `MATCH_SOURCE`.
const MATCH_EXPECTED: &str = "hearts,spades,clubs;ABC\n";

/// Closures whose visible parameters are `mixed`, called with several tags and with a capture.
#[test]
fn test_cli_wasm_mixed_closure_parameters_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_closure");
    let php_path = dir.join("main.php");
    fs::write(&php_path, MIXED_CLOSURE_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the Mixed-closure probe");
    assert!(
        output.status.success(),
        "Mixed-closure compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the Mixed-closure probe under Node");
    assert!(
        run.status.success(),
        "Mixed-closure probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), MIXED_CLOSURE_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The Mixed-closure probe. `$double(21)` answering 42 and `$double(1.5)` answering 3 is
/// what proves the cell reaches the body carrying its own tag rather than being narrowed.
const MIXED_CLOSURE_SOURCE: &str = r##"<?php
$double = function (mixed $x): mixed { return $x * 2; };
$label  = function (mixed $x): string { return "[" . $x . "]"; };
$pick   = function (mixed $a, mixed $b): mixed { return $a; };
echo $double(21), ",", $double(1.5), ";";
echo $label(7), $label("s"), $label(2.5), ";";
echo $pick(3, 9), ";";
$n = 10;
$add = function (mixed $x) use ($n): mixed { return $x + $n; };
echo $add(5), "\n";
"##;

/// php-src 8.5.6's own output for `MIXED_CLOSURE_SOURCE`.
const MIXED_CLOSURE_EXPECTED: &str = "42,3;[7][s][2.5];3;15\n";

/// A Mixed rendered in a STRING CONTEXT — concatenation and interpolation — over every tag.
#[test]
fn test_cli_wasm_mixed_string_context_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_string_context");
    let php_path = dir.join("main.php");
    fs::write(&php_path, MIXED_STRING_CONTEXT_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the string-context probe");
    assert!(
        output.status.success(),
        "string-context compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the string-context probe under Node");
    assert!(
        run.status.success(),
        "string-context probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        MIXED_STRING_CONTEXT_EXPECTED
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The string-context probe. These casts are IMPLICIT — nothing in the source says
/// `(string)` — and PHP's conversion here is the same one the explicit cast performs,
/// which is why the array row reads `[Array]` rather than raising.
const MIXED_STRING_CONTEXT_SOURCE: &str = r##"<?php
function show(mixed $v): string { return "[" . $v . "]"; }
function interp(mixed $v): string { return "<$v>"; }
echo show(42), show("x"), show(2.5), show(true), show(false), show(null), show([1,2]), ";";
echo interp(42), interp("x"), interp(2.5), ";";
$out = "";
foreach ([1, "a", 2.5, null] as $v) { $out = $out . $v . ";"; }
echo $out, "\n";
"##;

/// php-src 8.5.6's own output for `MIXED_STRING_CONTEXT_SOURCE`.
const MIXED_STRING_CONTEXT_EXPECTED: &str = "[42][x][2.5][1][][][Array];<42><x><2.5>;1;a;2.5;;\n";

/// Every scalar cast of a Mixed, over every runtime tag, plus `echo` of a container.
#[test]
fn test_cli_wasm_mixed_scalar_casts_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_mixed_casts");
    let php_path = dir.join("main.php");
    fs::write(&php_path, MIXED_CAST_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the Mixed-cast probe");
    assert!(
        output.status.success(),
        "Mixed-cast compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the Mixed-cast probe under Node");
    assert!(
        run.status.success(),
        "Mixed-cast probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program, with diagnostics silenced: the
    // "Array to string conversion" warning goes to stderr and is not compared here.
    assert_eq!(String::from_utf8_lossy(&run.stdout), MIXED_CAST_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The Mixed-cast probe: fifteen values covering every runtime tag a cell can carry, each
/// through `(int)`, `(float)`, `(bool)` and `(string)`. The two array rows are the ones that
/// used to answer the empty string where PHP prints "Array".
const MIXED_CAST_SOURCE: &str = r##"<?php
function show(mixed $v): void {
    echo (int)$v, "|", (float)$v, "|", ((bool)$v) ? "T" : "F", "|", (string)$v, ";";
}
show(1); show(-5); show(0); show(1.5); show(-2.7);
show(true); show(false); show(null);
show("42"); show("3.9"); show("abc"); show(""); show("0");
show([1,2]); show([]);
echo "\n";
$mixedish = [1, "x", [7, 8], 2.5];
foreach ($mixedish as $v) { echo $v, ";"; }
echo "\n";
$rows = [[1, 2], [3, 4]];
foreach ($rows as $r) { echo $r, ";"; }
echo "\n";
"##;

/// php-src 8.5.6's own output for `MIXED_CAST_SOURCE`.
const MIXED_CAST_EXPECTED: &str = "1|1|T|1;-5|-5|T|-5;0|0|F|0;1|1.5|T|1.5;-2|-2.7|T|-2.7;1|1|T|1;0|0|F|;0|0|F|;42|42|T|42;3|3.9|T|3.9;0|0|T|abc;0|0|F|;0|0|F|0;1|1|T|Array;0|0|F|Array;\n1;x;Array;2.5;\nArray;Array;\n";

/// Enums: string-backed, int-backed and pure, read through `->value` and `->name`, compared
/// by identity, and passed to a typed parameter.
#[test]
fn test_cli_wasm_enums_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_enums");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ENUM_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the enum probe");
    assert!(
        output.status.success(),
        "enum compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the enum probe under Node");
    assert!(
        run.status.success(),
        "enum probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ENUM_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The enum probe. `$s === Suit::Spades` reading `same` while `$s === Suit::Hearts` reads
/// `diff` is what proves each case is ONE singleton — two allocations per read would make
/// every identity comparison false.
const ENUM_SOURCE: &str = r##"<?php
enum Suit: string {
    case Hearts = "H";
    case Spades = "S";
    case Clubs = "C";
}
enum Level: int {
    case Low = 1;
    case High = 10;
}
enum Flag {
    case On;
    case Off;
}
echo Suit::Hearts->value, Suit::Spades->value, Suit::Clubs->value, ";";
echo Suit::Hearts->name, ",", Suit::Spades->name, ";";
echo Level::Low->value + Level::High->value, ";";
echo Level::Low->name, ",", Flag::On->name, ",", Flag::Off->name, ";";
$s = Suit::Spades;
echo $s->value, ",", $s === Suit::Spades ? "same" : "diff", ",", $s === Suit::Hearts ? "same" : "diff", ";";
function describe(Suit $s): string { return $s->name . "=" . $s->value; }
echo describe(Suit::Clubs), ";";
echo Suit::Hearts === Suit::Hearts ? "id" : "no", "\n";
"##;

/// php-src 8.5.6's own output for `ENUM_SOURCE`.
const ENUM_EXPECTED: &str = "HSC;Hearts,Spades;11;Low,On,Off;S,same,diff;Clubs=C;id\n";

/// Variadic parameters: free functions, an instance method and a static one, with and
/// without leading fixed parameters, over int and string element types.
#[test]
fn test_cli_wasm_variadic_calls_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_variadic");
    let php_path = dir.join("main.php");
    fs::write(&php_path, VARIADIC_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the variadic probe");
    assert!(
        output.status.success(),
        "variadic compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the variadic probe under Node");
    assert!(
        run.status.success(),
        "variadic probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), VARIADIC_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The variadic probe. `sum()` with NO arguments is what proves the empty packed array is
/// built and passed rather than the call being reshaped.
const VARIADIC_SOURCE: &str = r##"<?php
function sum(int ...$xs): int { $t = 0; foreach ($xs as $x) { $t = $t + $x; } return $t; }
function label(string $prefix, string ...$parts): string { return $prefix . ":" . implode("|", $parts); }
function counted(int $base, int ...$rest): int { return $base + count($rest); }
class Adder {
    public function all(int ...$xs): int { $t = 0; foreach ($xs as $x) { $t = $t + $x; } return $t; }
    public static function stat(int ...$xs): int { return count($xs); }
}
echo sum(1,2,3), ",", sum(), ",", sum(7), ";";
echo label("a"), ",", label("a","b"), ",", label("a","b","c"), ";";
echo counted(10), ",", counted(10,1,2), ";";
$a = new Adder();
echo $a->all(4,5,6), ",", Adder::stat(1,2), "\n";
"##;

/// php-src 8.5.6's own output for `VARIADIC_SOURCE`.
const VARIADIC_EXPECTED: &str = "6,0,7;a:,a:b,a:b|c;10,12;15,2\n";

/// Static properties: defaults of every slottable type, reads, writes, a string reassigned
/// and concatenated, and the shared storage an inherited static has.
#[test]
fn test_cli_wasm_static_properties_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_statics");
    let php_path = dir.join("main.php");
    fs::write(&php_path, STATIC_PROPERTY_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the static-property probe");
    assert!(
        output.status.success(),
        "static-property compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the static-property probe under Node");
    assert!(
        run.status.success(),
        "static-property probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), STATIC_PROPERTY_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The static-property probe. `Child::$shared` and `Base::$shared` must both read 106 after
/// two separate increments — one through each name — which is what proves an inherited static
/// is ONE slot and not a per-class copy.
const STATIC_PROPERTY_SOURCE: &str = r##"<?php
class Base {
    public static int $shared = 100;
    public static string $tag = "base";
    public static float $ratio = 1.5;
    public static bool $on = false;
}
class Child extends Base {}
class Counter {
    public static int $n = 0;
    public static function tick(): int { Counter::$n = Counter::$n + 1; return Counter::$n; }
    public static function reset(): void { Counter::$n = 0; }
}
echo Base::$shared, ",", Base::$tag, ",", Base::$ratio, ",", Base::$on ? "y" : "n", ";";
Base::$shared = Base::$shared + 5;
Child::$shared = Child::$shared + 1;
echo Base::$shared, ",", Child::$shared, ";";
Base::$tag = "changed";
echo Base::$tag, ",", Child::$tag, ";";
Base::$tag = Base::$tag . "!";
echo Base::$tag, ";";
Counter::tick(); Counter::tick(); Counter::tick();
echo Counter::$n, ";";
Counter::reset();
echo Counter::$n, ";";
Base::$ratio = Base::$ratio * 2;
Base::$on = true;
echo Base::$ratio, ",", Base::$on ? "y" : "n", "\n";
"##;

/// php-src 8.5.6's own output for `STATIC_PROPERTY_SOURCE`.
const STATIC_PROPERTY_EXPECTED: &str = "100,base,1.5,n;106,106;changed,changed;changed!;3;0;3,y\n";

/// The word-counter — `$c[$k] = $c[$k] + 1` — and a hash carrying one value of every tag.
///
/// This is the shape that read back WRONG before the store flattened its Mixed value: the
/// counter printed `a=;b=;c=1;`, because a re-read key had been stored as a cell holding a
/// cell and nothing follows that indirection. The `else` branch's plain `= 1` was the only
/// entry that printed, which is what made the bug look like a counting error rather than a
/// storage one.
#[test]
fn test_cli_wasm_heterogeneous_hash_values_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_het_hash");
    let php_path = dir.join("main.php");
    fs::write(&php_path, HETEROGENEOUS_HASH_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the heterogeneous-hash probe");
    assert!(
        output.status.success(),
        "heterogeneous-hash compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the heterogeneous-hash probe under Node");
    assert!(
        run.status.success(),
        "heterogeneous-hash probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        HETEROGENEOUS_HASH_EXPECTED
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The heterogeneous-hash probe: one value of every runtime tag, read back through both
/// `foreach` and `$h[k]`, plus a re-read-and-store and a copy between two keys.
const HETEROGENEOUS_HASH_SOURCE: &str = r##"<?php
$counts = [];
foreach (["a", "b", "a", "c", "b", "a"] as $ch) {
    if (isset($counts[$ch])) { $counts[$ch] = $counts[$ch] + 1; } else { $counts[$ch] = 1; }
}
foreach ($counts as $k => $n) { echo $k, "=", $n, ";"; }
echo "|";
$h = [];
$h["i"] = 1;
$h["s"] = "text";
$h["f"] = 2.5;
$h["b"] = true;
$h["n"] = null;
$h["i"] = $h["i"] + 10;
$h["copy"] = $h["s"];
foreach ($h as $k => $v) { echo $k, "=", $v, ";"; }
echo "|", count($h), "|", isset($h["n"]) ? "y" : "n", array_key_exists("n", $h) ? "y" : "n";
echo "|", $h["s"], ",", $h["f"], ",", $h["i"], ",", $h["copy"], "\n";
"##;

/// php-src 8.5.6's own output for `HETEROGENEOUS_HASH_SOURCE`.
const HETEROGENEOUS_HASH_EXPECTED: &str = "a=3;b=2;c=1;|i=11;s=text;f=2.5;b=1;n=;copy=text;|6|ny|text,2.5,11,text\n";

/// Nested indexed arrays: `[[1,2],[3,4]]` built, iterated, accumulated into a fresh `[]`,
/// and nested one level deeper.
#[test]
fn test_cli_wasm_nested_arrays_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_nested_arrays");
    let php_path = dir.join("main.php");
    fs::write(&php_path, NESTED_ARRAY_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the nested-array probe");
    assert!(
        output.status.success(),
        "nested-array compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the nested-array probe under Node");
    assert!(
        run.status.success(),
        "nested-array probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), NESTED_ARRAY_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The nested-array probe. The last group is nested twice, which is what proves the element
/// layout is chosen from the element's own type rather than assumed one level deep.
const NESTED_ARRAY_SOURCE: &str = r##"<?php
$m = [[1, 2], [3, 4], [5, 6]];
foreach ($m as $row) { echo implode("-", $row), ";"; }
echo "|", count($m), "|";
$g = [["a", "b"], ["c", "d"]];
foreach ($g as $words) { echo "[", implode("", $words), "]"; }
echo "|";
$t = 0;
foreach ($m as $pair) { foreach ($pair as $n) { $t = $t + $n; } }
echo $t, "|";
$sizes = [];
foreach ($m as $r2) { $sizes[] = count($r2); }
echo implode(",", $sizes), "|";
$built = [];
foreach ($m as $r3) { $built[] = $r3; }
foreach ($built as $r4) { echo count($r4); }
echo "|";
$deep = [[[1, 2]], [[3]]];
foreach ($deep as $outer) { foreach ($outer as $inner) { echo implode("+", $inner), "."; } }
echo "\n";
"##;

/// php-src 8.5.6's own output for `NESTED_ARRAY_SOURCE`.
const NESTED_ARRAY_EXPECTED: &str = "1-2;3-4;5-6;|3|[ab][cd]|21|2,2,2|222|1+2.3.\n";

/// Proves the cycle collector actually reclaims a cycle, by watching memory not grow.
///
/// Two objects pointing at each other keep each other's refcount above zero forever, so
/// refcounting alone can never free them — this is the one shape on this target that needs
/// `__rt_gc_collect_cycles`, which `unset(...)` reaches. Measured: with the collector
/// neutralized the loop grows 50 pages over its declared memory; with it, 2 — exactly what
/// the same loop grows when the cycle is not formed at all. The program prints the right
/// answer either way, which is why this watches memory rather than output.
#[test]
fn test_cli_wasm_unset_collects_reference_cycles() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_gc_cycle");
    let runner_src = r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(readFileSync(process.argv[2])),
  wasi.getImportObject(),
);
const code = wasi.start(instance);
console.error(`pages=${instance.exports.memory.buffer.byteLength / 65536}`);
process.exitCode = code;
"#;
    let runner = dir.join("run.mjs");
    fs::write(&runner, runner_src).unwrap();

    // The control forms no cycle, so refcounting alone frees both nodes. Anything the cycle
    // case grows BEYOND it is a cycle the collector failed to reclaim.
    let bodies = [
        ("no cycle", ""),
        ("cycle", "$b->next = $a;"),
    ];
    for (label, link_back) in bodies {
        let php_path = dir.join("main.php");
        fs::write(
            &php_path,
            format!(
                "<?php\nclass Node {{ public ?Node $next = null; }}\n\
                 $sum = 0;\n\
                 foreach (range(1, 20000) as $i) {{\n\
                 \x20   $a = new Node();\n\
                 \x20   $b = new Node();\n\
                 \x20   $a->next = $b;\n\
                 \x20   {link_back}\n\
                 \x20   $sum = $sum + 1;\n\
                 \x20   unset($a);\n\
                 \x20   unset($b);\n\
                 }}\n\
                 if ($sum === 20000) {{ echo \"ok\\n\"; }}\n"
            ),
        )
        .unwrap();

        for extra in [vec!["--emit-asm"], vec![]] {
            let mut command = elephc_cli_command(&dir);
            command.arg("--target").arg("wasm32-wasi");
            for flag in extra {
                command.arg(flag);
            }
            let output = command
                .arg(&php_path)
                .output()
                .unwrap_or_else(|error| panic!("{label}: failed to invoke elephc: {error}"));
            assert!(
                output.status.success(),
                "{label} failed to compile: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let run = Command::new("node")
            .arg("--no-warnings")
            .arg(&runner)
            .arg(dir.join("main.wasm"))
            .current_dir(&dir)
            .output()
            .unwrap_or_else(|error| panic!("{label}: failed to run under Node: {error}"));
        assert!(
            run.status.success(),
            "{label} trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(run.stdout, b"ok\n", "{label} printed the wrong thing");

        let stderr = String::from_utf8_lossy(&run.stderr);
        let final_pages: usize = stderr
            .split("pages=")
            .nth(1)
            .and_then(|rest| rest.trim().parse().ok())
            .unwrap_or_else(|| panic!("{label}: the runner reported no page count"));
        let wat = fs::read_to_string(dir.join("main.wat")).expect("the WAT was written");
        let initial_pages: usize = wat
            .split("(memory (export \"memory\") ")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or_else(|| panic!("{label}: the module declares no initial memory"));

        // 2 pages is the `range` array itself (20000 * 8 bytes), which both cases allocate.
        assert_eq!(
            final_pages - initial_pages,
            2,
            "{label}: 20000 iterations grew memory by {} pages over the declared \
             {initial_pages}, where only the range array (2 pages) should — the cycle \
             was not reclaimed",
            final_pages - initial_pages
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Proves a property read leaves no reference behind, by watching memory not grow.
///
/// The object is rebuilt each iteration so its property's backing array has to die with it. A
/// read that retains twice leaves the array alive forever: measured at 98 pages against the bare
/// loop's 3 before the fix, and 43 for a string property. Both are invisible in the output — the
/// program prints the right answer either way, which is why this watches memory instead.
#[test]
fn test_cli_wasm_property_read_leaves_no_reference() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_prop_leak");
    let runner_src = r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(readFileSync(process.argv[2])),
  wasi.getImportObject(),
);
const code = wasi.start(instance);
console.error(`pages=${instance.exports.memory.buffer.byteLength / 65536}`);
process.exitCode = code;
"#;
    let runner = dir.join("run.mjs");
    fs::write(&runner, runner_src).unwrap();

    // `foreach (range(...))` rather than an unrolled program: thousands of statements do not
    // compile fast enough to reach the 64 KiB page granularity a per-read leak needs.
    let bodies = [
        ("baseline", r#"if ($n === 999999) { echo "x"; }"#),
        ("array property", r#"if (count($x->a) === 99) { echo "x"; }"#),
        ("string property", r#"if ($x->s === "zz") { echo "x"; }"#),
    ];
    for (label, body) in bodies {
        let php_path = dir.join("main.php");
        fs::write(
            &php_path,
            format!(
                "<?php\nclass Box {{ public function __construct(public string $s, public array $a) {{}} }}\n                 foreach (range(1, 30000) as $n) {{\n    $x = new Box(\"bolt\", [1,2,3]);\n    {body}\n}}\n                 echo \"ok\\n\";\n"
            ),
        )
        .unwrap();

        for extra in [vec!["--emit-asm"], vec![]] {
            let mut command = elephc_cli_command(&dir);
            command.arg("--target").arg("wasm32-wasi");
            for flag in extra {
                command.arg(flag);
            }
            let output = command
                .arg(&php_path)
                .output()
                .unwrap_or_else(|error| panic!("{label}: failed to invoke elephc: {error}"));
            assert!(
                output.status.success(),
                "{label} failed to compile: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let run = Command::new("node")
            .arg("--no-warnings")
            .arg(&runner)
            .arg(dir.join("main.wasm"))
            .current_dir(&dir)
            .output()
            .unwrap_or_else(|error| panic!("{label}: failed to run under Node: {error}"));
        assert!(
            run.status.success(),
            "{label} trapped: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(run.stdout, b"ok\n", "{label} printed the wrong thing");

        let stderr = String::from_utf8_lossy(&run.stderr);
        let final_pages: usize = stderr
            .split("pages=")
            .nth(1)
            .and_then(|rest| rest.trim().parse().ok())
            .unwrap_or_else(|| panic!("{label}: the runner reported no page count"));
        let wat = fs::read_to_string(dir.join("main.wat")).expect("the WAT was written");
        let initial_pages: usize = wat
            .split("(memory (export \"memory\") ")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or_else(|| panic!("{label}: the module declares no initial memory"));

        // The `range` array itself grows, so every case is compared against the bare loop.
        assert_eq!(
            final_pages - initial_pages,
            3,
            "{label}: 30000 reads grew memory by {} pages over the declared {initial_pages}, \
             where the bare loop grows 3 — a reference is being left behind",
            final_pages - initial_pages
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a property read RETAINS exactly once, and that copy-on-write follows from it.
///
/// `Op::PropGet` is always followed by `Op::Acquire` — checked across every use shape: store,
/// echo, argument, concat, builtin argument, return, strict compare. The acquire persists a string
/// and increfs a refcounted child, so the READ must only view them. Retaining in both places left
/// one extra reference per read: measured at ~207 bytes per read of an array property, whose
/// backing array was then never freed, and ~87 for a string.
///
/// The Throwable accessors share the same slot reader and need the OPPOSITE: no acquire follows
/// them, and their result outlives the object it came from, so they own their copy. Reading
/// `getMessage()` here is what catches getting that backwards — it answers dead bytes otherwise.
///
/// With the reference count finally right, `$c = $src; $c[] = "z";` gets PHP's value semantics:
/// the push sees two owners and splits. Before, the push had no copy-on-write at all and simply
/// grew the shared array in place, freeing the block the other reference still pointed at — which
/// the extra retain had been hiding.
#[test]
fn test_cli_wasm_property_read_retains_once_and_arrays_copy_on_write() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_prop_retain");
    let php_path = dir.join("main.php");
    fs::write(&php_path, PROP_RETAIN_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the property-retain probe");
    assert!(
        output.status.success(),
        "property-retain compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the property-retain probe under Node");
    assert!(
        run.status.success(),
        "property-retain probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), PROP_RETAIN_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The probe: every property-read shape, the Throwable accessors, and copy-on-write on a copy.
const PROP_RETAIN_SOURCE: &str = r##"<?php
class C {
    public function __construct(public string $s, public array $a, public mixed $m, public int $i) {}
}
function take(string $v): int { return strlen($v); }
function give(C $c): string { return $c->s; }
$x = new C("abc", [1,2,3], "boxed", 7);
$a = $x->s;              echo "[", $a, "]";
echo "[", $x->s, "]";
echo "[", take($x->s), "]";
echo "[", $x->s . "y", "]";
echo "[", strtoupper($x->s), "]";
echo "[", give($x), "]";
echo "[", ($x->s === "abc") ? "y" : "n", "]";
echo "[", count($x->a), "]";
echo "[", implode(",", $x->a), "]";
echo "[", $x->i, "]";
echo "[", $x->m, "]";
$b = $x->a; $b[] = 9; echo "[", count($b), ":", count($x->a), "]";
echo "\n";
try { throw new RuntimeException("boom", 42); }
catch (RuntimeException $e) { echo $e->getMessage(), "|", $e->getCode(), "|", get_class($e), "\n"; }
$src = ["a", "b"];
$c = $src;
$c[] = "z";
echo count($c), ":", count($src), "|", implode(",", $c), ":", implode(",", $src), "|";
$i = [1, 2];
$j = $i; $j[] = 3;
echo count($j), ":", count($i), "|", implode(",", $j), ":", implode(",", $i), "|";
$k = $i; $k[] = 4; $k[] = 5;
echo implode(",", $k), ":", implode(",", $i), "|";
echo "\n";
"##;

/// php-src 8.5.6's own output for `PROP_RETAIN_SOURCE`.
const PROP_RETAIN_EXPECTED: &str = r##"[abc][abc][3][abcy][ABC][abc][y][3][1,2,3][7][boxed][4:3]
boom|42|RuntimeException
3:2|a,b,z:a,b|3:2|1,2,3:1,2|1,2,4,5:1,2|
"##;

/// Verifies `round` and `sprintf`'s radix conversions against php-src.
///
/// PHP's `round` is half away from ZERO, where WebAssembly's `f64.nearest` is half to EVEN — it
/// answers 2 for `round(2.5)` where PHP answers 3. The naive repair `floor(|x| + 0.5)` is worse:
/// the addition is inexact, so `round(0.49999999999999994)` answers 1 instead of 0, and above
/// 2^52 it perturbs values that are already integers. Comparing against `trunc(x)` is exact, and
/// `f64.trunc` carries the sign of zero, which PHP prints — `round(-0.4)` is `-0`.
///
/// `%x`, `%X`, `%b` and `%o` read the argument as UNSIGNED, so `-1` prints as `ffffffffffffffff`
/// and no sign is ever emitted whatever the flags say.
#[test]
fn test_cli_wasm_round_and_radix_conversions_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_round");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ROUND_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the round probe");
    assert!(
        output.status.success(),
        "round compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the round probe under Node");
    assert!(
        run.status.success(),
        "round probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ROUND_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The probe: both halfway directions, the 0.49999999999999994 trap, the 2^52/2^53 boundaries,
/// infinities and NaN, then every radix conversion including negatives and both i64 extremes.
const ROUND_SOURCE: &str = r##"<?php
foreach ([0.0, -0.0, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5, 2.4, -2.4, 2.6] as $v) { echo round($v), "|"; }
echo "\n";
foreach ([0.49999999999999994, -0.49999999999999994, 4503599627370495.5, 9007199254740993.0] as $v) { echo round($v), "|"; }
echo "\n";
foreach ([1e15, 1e16, -1e16, 1e300, -1e300, INF, -INF, NAN, 1e-300] as $v) { echo round($v), "|"; }
echo "\n";
echo sprintf("%x|%X|%b|%o", 255, 255, 5, 8), "\n";
echo sprintf("%x|%X|%b|%o", -1, -255, -1, -1), "\n";
echo sprintf("[%08x][%-8x][%8b][%08b]", 255, 255, 5, 5), "\n";
echo sprintf("%x|%b", PHP_INT_MAX, PHP_INT_MIN), "\n";
"##;

/// php-src 8.5.6's own output for `ROUND_SOURCE`.
const ROUND_EXPECTED: &str = r##"0|-0|1|-1|2|-2|3|-3|2|-2|3|
0|-0|4.5035996273705E+15|9.007199254741E+15|
1.0E+15|1.0E+16|-1.0E+16|1.0E+300|-1.0E+300|INF|-INF|NAN|0|
ff|FF|101|10
ffffffffffffffff|FFFFFFFFFFFFFF01|1111111111111111111111111111111111111111111111111111111111111111|1777777777777777777777
[000000ff][ff      ][     101][00000101]
7fffffffffffffff|1000000000000000000000000000000000000000000000000000000000000000
"##;

/// Verifies COUNTING LOOPS, which no target-side gap explains but which did not compile.
///
/// `$i = $i + 1` lowers to a checked add, whose result must be Mixed because PHP promotes an
/// overflowing integer to a float. So the local widens to Mixed, and every later read of it is an
/// implicit Mixed-to-scalar transfer — which was refused, turning away the most ordinary loop in
/// the language along with anything that accumulates.
///
/// The transfer unboxes through the same helpers the NATIVE backend uses for the identical
/// coercion, except that a float narrows SILENTLY: PHP performs no cast here, so borrowing the
/// explicit `(int)` cast's out-of-range warning would print a diagnostic for a program PHP runs
/// quietly, and the two backends would disagree.
///
/// The gap this inherits belongs to the EIR, not to either lowering: a read is typed `int` from
/// the slot's type BEFORE the loop's widening store, so once an add really does overflow, both
/// targets answer a saturated `9223372036854775807` where PHP answers `9.2233720368548E+18`.
/// They agree with each other, which is what this pins.
#[test]
fn test_cli_wasm_counting_loops_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_counting");
    let php_path = dir.join("main.php");
    fs::write(&php_path, COUNTING_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the counting probe");
    assert!(
        output.status.success(),
        "counting-loop compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the counting probe under Node");
    assert!(
        run.status.success(),
        "counting probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), COUNTING_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The probe: a `while` counter, a `foreach` sum, a decreasing counter, and a running product.
const COUNTING_SOURCE: &str = r##"<?php
$i = 0;
while ($i < 3) { echo $i; $i = $i + 1; }
echo "|";
$t = 0;
foreach ([5, 7, 9] as $v) { $t = $t + $v; }
echo $t, "|";
$n = 10;
while ($n > 0) { $n = $n - 3; }
echo $n, "|";
$p = 1;
foreach (range(1, 5) as $k) { $p = $p * $k; }
echo $p, "\n";
"##;

/// php-src 8.5.6's own output for `COUNTING_SOURCE`.
const COUNTING_EXPECTED: &str = r##"012|21|-2|120
"##;

/// Verifies a property COUNTER, and `wordwrap`'s break-string and cut forms.
///
/// `$this->n = $this->n + 1` widens the value to Mixed through the checked add while the slot
/// stays an `int`, so the store narrows the same way a local load does — refusing it turned away
/// every counter held in an object.
///
/// `wordwrap`'s four-argument form BUILDS its result, because a multi-byte break and a cut both
/// lengthen the text; only the one-byte no-cut form can rewrite in place, where a space BECOMES
/// the break. Transcribed from php-src and validated on 314 cases, mostly generated over an
/// alphabet of `a`, `b`, `c` and space — which is where the awkward shapes are: `"a  b"` at width
/// 2 cutting is `a -b`, the first space becoming the break and the second surviving, and
/// `"  lead"` is ` -lea-d`.
#[test]
fn test_cli_wasm_property_counter_and_wordwrap_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_wordwrap");
    let php_path = dir.join("main.php");
    fs::write(&php_path, WORDWRAP_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the wordwrap probe");
    assert!(
        output.status.success(),
        "wordwrap compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the wordwrap probe under Node");
    assert!(
        run.status.success(),
        "wordwrap probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), WORDWRAP_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The probe: an int and a float property counter, then every wordwrap form over the shapes that
/// separate them.
const WORDWRAP_SOURCE: &str = r##"<?php
class Counter {
    private int $n = 0;
    private float $f = 0.5;
    public function bump(): int { $this->n = $this->n + 1; return $this->n; }
    public function grow(): float { $this->f = $this->f + 1.25; return $this->f; }
}
$c = new Counter();
echo $c->bump(), $c->bump(), $c->bump(), "|", $c->grow(), "|", $c->grow(), "\n";
$w = ["aaa bbb ccc", "abcdefghij", "a b c d e", "the quick brown fox", "", "one", "aaaa bb", "a  b", "  lead"];
foreach ($w as $s) { echo "[", str_replace("\n", "N", wordwrap($s, 4, "-", true)), "]"; }
echo "\n";
foreach ($w as $s) { echo "[", str_replace("\n", "N", wordwrap($s, 4, "-", false)), "]"; }
echo "\n";
foreach ($w as $s) { echo "[", str_replace("\n", "N", wordwrap($s, 7, "<>", true)), "]"; }
echo "\n";
echo "[", str_replace("\n", "N", wordwrap("aaa bbb ccc", 7)), "]\n";
"##;

/// php-src 8.5.6's own output for `WORDWRAP_SOURCE`.
const WORDWRAP_EXPECTED: &str = r##"123|1.75|3
[aaa-bbb-ccc][abcd-efgh-ij][a b-c d-e][the-quic-k-brow-n-fox][][one][aaaa-bb][a  b][ -lead]
[aaa-bbb-ccc][abcdefghij][a b-c d-e][the-quick-brown-fox][][one][aaaa-bb][a  b][ -lead]
[aaa bbb<>ccc][abcdefg<>hij][a b c d<>e][the<>quick<>brown<>fox][][one][aaaa bb][a  b][  lead]
[aaa bbbNccc]
"##;

/// Verifies an array passed BY VALUE, which PHP copies on write and this target used to corrupt.
///
/// The argument was borrowed rather than counted, so a push inside the callee saw one owner and
/// grew the array in place — and `__rt_array_grow` freed the block the CALLER still pointed at.
/// The caller then read a dead pointer: `mutate($src)` left `count($src)` answering 0.
///
/// The callee OWNS its array parameter now. The caller lends a counted reference and never takes
/// it back; the callee releases at every exit. Both branches balance: when it mutates,
/// `__rt_array_ensure_unique` hands it a clone and drops the original back to the caller's single
/// reference, and the epilogue frees the clone; when it does not, the epilogue simply undoes the
/// lend. A returned parameter moves out instead.
///
/// Every call-site kind is here because a missed lend is an over-release, not a leak: a plain
/// call, a two-level pass-through, a mutation, two mutations of the same source, a returned
/// parameter, five levels of recursion, a constructor argument and a method argument.
#[test]
fn test_cli_wasm_array_arguments_are_passed_by_value() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_by_value");
    let php_path = dir.join("main.php");
    fs::write(&php_path, BY_VALUE_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the by-value probe");
    assert!(
        output.status.success(),
        "by-value compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the by-value probe under Node");
    assert!(
        run.status.success(),
        "by-value probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), BY_VALUE_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The by-value probe: one of every call-site kind that can carry an array.
const BY_VALUE_SOURCE: &str = r##"<?php
class Bag {
    public function __construct(public array $items) {}
    public function size(): int { return count($this->items); }
    public function grow(array $a): int { $a[] = 9; return count($a); }
}
function pass(array $a): int { return inner($a); }
function inner(array $a): int { return count($a); }
function mutate(array $a): int { $a[] = 9; return count($a); }
function twice(array $a): int { $x = mutate($a); $y = mutate($a); if ($x === $y) { return $x; } return 0; }
function give_back(array $a): array { return $a; }
function deep(array $a, int $n): int { if ($n <= 0) { return count($a); } return deep($a, $n - 1); }
function strs(array $a): int { $a[] = "z"; return count($a); }

$src = [1, 2, 3];
echo pass($src), "|", count($src), "|";
echo mutate($src), "|", count($src), "|";
echo twice($src), "|", count($src), "|";
$b = give_back($src); echo count($b), ":", count($src), "|";
echo deep($src, 5), "|", count($src), "|";
$bag = new Bag($src);
echo $bag->size(), "|", $bag->grow($src), "|", count($src), "|", $bag->size(), "|";
$w = ["a", "bb"];
echo strs($w), "|", count($w), "|", implode(",", $w), "|";
echo implode(",", $src), "\n";
"##;

/// php-src 8.5.6's own output for `BY_VALUE_SOURCE`.
const BY_VALUE_EXPECTED: &str = r##"3|3|4|3|4|3|3:3|3|3|3|4|3|3|3|2|a,bb|1,2,3
"##;

/// Verifies arithmetic inside TYPED functions, and an array of interface implementors.
///
/// `return $this->s * $this->s;` from an `: int` method could not compile. The multiplication is
/// checked, so its result is typed Mixed — an overflow would promote it to a float — and narrowing
/// that back for the declared return was refused as an implicit coercion. It is not one: PHP
/// performs no conversion there, `square(7)` is just 49. The narrowing is admitted when the value
/// is TRANSITIVELY integer arithmetic, which `$a + $b + $c` also is: the chain runs through
/// `MixedNumericBinop`, whose left operand is the previous Mixed.
///
/// The transitivity is what keeps the refusal where PHP really does coerce — `f(mixed $m): int {
/// return $m + 1; }` emits the same opcode over a genuine `mixed`, and still refuses.
///
/// The shapes array is `array<mixed>` because the classes differ, so each object BOXES into a
/// cell under tag 6. The EIR emits no release after that push, unlike a concrete `array<Object>`,
/// so the operand's single reference is handed over rather than shared.
#[test]
fn test_cli_wasm_typed_arithmetic_and_polymorphic_arrays_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_polymorphic");
    let php_path = dir.join("main.php");
    fs::write(&php_path, POLYMORPHIC_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the polymorphic probe");
    assert!(
        output.status.success(),
        "polymorphic compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the polymorphic probe under Node");
    assert!(
        run.status.success(),
        "polymorphic probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), POLYMORPHIC_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The probe: two implementors dispatched through one array, and typed arithmetic including a
/// chained sum, zero and a negative.
const POLYMORPHIC_SOURCE: &str = r##"<?php
interface Shape { public function area(): int; }
class Sq implements Shape {
    public function __construct(private int $s) {}
    public function area(): int { return $this->s * $this->s; }
}
class Re implements Shape {
    public function __construct(private int $w, private int $h) {}
    public function area(): int { return $this->w * $this->h; }
}
class Math {
    public static function square(int $x): int { return $x * $x; }
    public static function sum3(int $a, int $b, int $c): int { return $a + $b + $c; }
}
$shapes = [new Sq(3), new Re(2, 5), new Sq(4)];
foreach ($shapes as $s) { echo $s->area(), ";"; }
echo "|", count($shapes), "|";
echo Math::square(7), "|", Math::sum3(1, 2, 3), "|";
echo Math::square(0), "|", Math::square(-4), "\n";
"##;

/// php-src 8.5.6's own output for `POLYMORPHIC_SOURCE`.
const POLYMORPHIC_EXPECTED: &str = r##"9;10;16;|3|49|6|0|16
"##;

/// Verifies `isset`, which is exactly "not null" for a variable the checker proved defined.
///
/// It reuses `Op::IsNull`'s per-representation tag rules rather than growing a second copy — a
/// Mixed cell tests tag 8, a tagged scalar its tag word, a nullable container its pointer, and a
/// statically non-null value folds to true.
///
/// The audit confined EVERY language construct to `main`, because `exit`/`die` cannot unwind a
/// caller's WASM frames. `isset` only reads a tag, so it is exempt — and
/// `test_cli_wasm_rejects_exit_outside_main` still pins that `exit` is not.
#[test]
fn test_cli_wasm_isset_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_isset");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ISSET_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the isset probe");
    assert!(
        output.status.success(),
        "isset compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the isset probe under Node");
    assert!(
        run.status.success(),
        "isset probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ISSET_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `exit` outside `main` is still refused, which the `isset` exemption must not relax.
#[test]
fn test_cli_wasm_rejects_exit_outside_main() {
    let dir = make_cli_test_dir("elephc_cli_wasm_exit_nested");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\nfunction boom(): void { exit(1); }\nboom();\n",
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to invoke elephc");
    assert!(
        !output.status.success(),
        "exit outside main cannot unwind caller-owned frames and must be refused"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("exit/die outside main cannot unwind caller-owned WASM frames"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The `isset` probe: every representation it can reach, inside and outside a function.
const ISSET_SOURCE: &str = r##"<?php
function mm(mixed $v): string { return isset($v) ? "y" : "n"; }
class Box { public function __construct(public int $n) {} }
$a = 5; $b = null; $s = "x"; $f = 1.5; $arr = [1,2]; $e = []; $o = new Box(1);
echo isset($a) ? "y" : "n", isset($b) ? "y" : "n", isset($s) ? "y" : "n";
echo isset($f) ? "y" : "n", isset($arr) ? "y" : "n", isset($e) ? "y" : "n";
echo isset($o) ? "y" : "n", "|";
echo mm(3), mm("z"), mm(0), mm(1.5), mm(""), "|";
echo $a ?? 9, "|";
$t = 0;
foreach ([1, 2, 3] as $v) { if (isset($v)) { $t = $t + $v; } }
echo $t, "\n";
"##;

/// php-src 8.5.6's own output for `ISSET_SOURCE`.
const ISSET_EXPECTED: &str = r##"ynyyyyy|yyyyy|5|6
"##;

/// Verifies `sort` and `rsort` over scalar arrays, on 64 orderings against php-src.
///
/// The sort is STABLE — PHP's have been since 8.0 — so the swap test is strict and equal
/// elements keep their order. It copy-on-write-uniques first and answers the array pointer, which
/// the call site writes back: `sort($a)` rebinds `$a`.
///
/// String and Mixed elements stay refused. PHP orders strings with its standard comparison, where
/// two numeric strings compare NUMERICALLY — `sort(["10", "9"])` answers `9, 10` — and that rule
/// is not this helper's.
#[test]
fn test_cli_wasm_scalar_sorts_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_sort");
    let php_path = dir.join("main.php");
    fs::write(&php_path, SORT_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the sort probe");
    assert!(
        output.status.success(),
        "sort compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the sort probe under Node");
    assert!(
        run.status.success(),
        "sort probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), SORT_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// `foreach ($h as $k => $v)` over Mixed hash values, plus `isset($h[$k])` and
/// `array_key_exists($k, $h)` — the pair PHP answers DIFFERENTLY for a stored null.
#[test]
fn test_cli_wasm_assoc_foreach_and_key_tests_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_assoc_keys");
    let php_path = dir.join("main.php");
    fs::write(&php_path, ASSOC_KEYS_SOURCE).unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the associative-key probe");
    assert!(
        output.status.success(),
        "associative-key compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the associative-key probe under Node");
    assert!(
        run.status.success(),
        "associative-key probe trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    assert_eq!(String::from_utf8_lossy(&run.stdout), ASSOC_KEYS_EXPECTED);

    let _ = fs::remove_dir_all(&dir);
}

/// The associative-key probe. `"b" => null` is the case that separates the two questions:
/// `isset` answers false there, `array_key_exists` answers true. `"0"`/`"7"` cover PHP's
/// numeric-string key normalization, and the last group reads Mixed and float hash values.
const ASSOC_KEYS_SOURCE: &str = r##"<?php
$conf = ["host" => "local", "port" => 8080, "debug" => true];
foreach ($conf as $k => $v) { echo $k, "=", $v, ";"; }
echo "|";
$h = ["a" => 1, "b" => null, "c" => 0, "d" => "", "e" => false, "0" => "zero", "7" => "seven"];
foreach (["a","b","c","d","e","zz","0","7",""] as $k) {
  echo $k, isset($h[$k]) ? "1" : "0", array_key_exists($k, $h) ? "1" : "0", ",";
}
echo "|", isset($h[0]) ? "1" : "0", array_key_exists(0, $h) ? "1" : "0";
echo "|", isset($h[7]) ? "1" : "0", array_key_exists(7, $h) ? "1" : "0";
echo "|";
$n = [1 => "a", 2 => null, 3 => "c"];
foreach ([0,1,2,3,4] as $i) { echo $i, isset($n[$i]) ? "1" : "0", array_key_exists($i, $n) ? "1" : "0", ","; }
echo "|";
foreach ($n as $key => $val) { echo $key, "=>", $val, " "; }
echo "|";
$f = ["p" => 1.5, "q" => -0.25];
foreach ($f as $key => $val) { echo $key, ":", $val, " "; }
echo "|", count($conf), count($h), count($n), count($f), "\n";
"##;

/// php-src 8.5.6's own output for `ASSOC_KEYS_SOURCE`.
const ASSOC_KEYS_EXPECTED: &str = "host=local;port=8080;debug=1;|a11,b01,c11,d11,e11,zz00,011,711,00,|11|11|000,111,201,311,400,|1=>a 2=> 3=>c |p:1.5 q:-0.25 |3732\n";

/// The sort probe: empty, single, already-ordered, reversed, duplicates, negatives, both i64
/// extremes, floats including -0.0, twenty generated orderings, and the STRING cases — where
/// two numeric strings order NUMERICALLY, equal-as-doubles i64-overflowing texts fall back to
/// their bytes, and equal infinities do too.
const SORT_SOURCE: &str = r##"<?php
$a0 = [5,3,9,1,3]; sort($a0); echo implode(",", $a0), ";";
$b0 = [5,3,9,1,3]; rsort($b0); echo implode(",", $b0), ";";
$a1 = []; sort($a1); echo implode(",", $a1), ";";
$b1 = []; rsort($b1); echo implode(",", $b1), ";";
$a2 = [7]; sort($a2); echo implode(",", $a2), ";";
$b2 = [7]; rsort($b2); echo implode(",", $b2), ";";
$a3 = [2,1]; sort($a3); echo implode(",", $a3), ";";
$b3 = [2,1]; rsort($b3); echo implode(",", $b3), ";";
$a4 = [1,2,3]; sort($a4); echo implode(",", $a4), ";";
$b4 = [1,2,3]; rsort($b4); echo implode(",", $b4), ";";
$a5 = [3,2,1]; sort($a5); echo implode(",", $a5), ";";
$b5 = [3,2,1]; rsort($b5); echo implode(",", $b5), ";";
$a6 = [-5,0,5,-1]; sort($a6); echo implode(",", $a6), ";";
$b6 = [-5,0,5,-1]; rsort($b6); echo implode(",", $b6), ";";
$a7 = [0,0,0]; sort($a7); echo implode(",", $a7), ";";
$b7 = [0,0,0]; rsort($b7); echo implode(",", $b7), ";";
$a8 = [PHP_INT_MAX, PHP_INT_MIN, 0]; sort($a8); echo implode(",", $a8), ";";
$b8 = [PHP_INT_MAX, PHP_INT_MIN, 0]; rsort($b8); echo implode(",", $b8), ";";
$a9 = [1.5,-2.5,0.0,1.5]; sort($a9); echo implode(",", $a9), ";";
$b9 = [1.5,-2.5,0.0,1.5]; rsort($b9); echo implode(",", $b9), ";";
$a10 = [3.0,1.0,2.0]; sort($a10); echo implode(",", $a10), ";";
$b10 = [3.0,1.0,2.0]; rsort($b10); echo implode(",", $b10), ";";
$a11 = [-0.0,0.0]; sort($a11); echo implode(",", $a11), ";";
$b11 = [-0.0,0.0]; rsort($b11); echo implode(",", $b11), ";";
$a12 = [25,19,-34]; sort($a12); echo implode(",", $a12), ";";
$b12 = [25,19,-34]; rsort($b12); echo implode(",", $b12), ";";
$a13 = [27,10,30,24,-42]; sort($a13); echo implode(",", $a13), ";";
$b13 = [27,10,30,24,-42]; rsort($b13); echo implode(",", $b13), ";";
$a14 = []; sort($a14); echo implode(",", $a14), ";";
$b14 = []; rsort($b14); echo implode(",", $b14), ";";
$a15 = [-17,20,-21,-26,41,10,19]; sort($a15); echo implode(",", $a15), ";";
$b15 = [-17,20,-21,-26,41,10,19]; rsort($b15); echo implode(",", $b15), ";";
$a16 = [10,0,31,-31,-21,31,-31,16]; sort($a16); echo implode(",", $a16), ";";
$b16 = [10,0,31,-31,-21,31,-31,16]; rsort($b16); echo implode(",", $b16), ";";
$a17 = [44,-49,35,49,-42,-30]; sort($a17); echo implode(",", $a17), ";";
$b17 = [44,-49,35,49,-42,-30]; rsort($b17); echo implode(",", $b17), ";";
$a18 = []; sort($a18); echo implode(",", $a18), ";";
$b18 = []; rsort($b18); echo implode(",", $b18), ";";
$a19 = [49,-47,-16,10]; sort($a19); echo implode(",", $a19), ";";
$b19 = [49,-47,-16,10]; rsort($b19); echo implode(",", $b19), ";";
$a20 = [41,4,0,43,23,6]; sort($a20); echo implode(",", $a20), ";";
$b20 = [41,4,0,43,23,6]; rsort($b20); echo implode(",", $b20), ";";
$a21 = [-4,-38]; sort($a21); echo implode(",", $a21), ";";
$b21 = [-4,-38]; rsort($b21); echo implode(",", $b21), ";";
$a22 = []; sort($a22); echo implode(",", $a22), ";";
$b22 = []; rsort($b22); echo implode(",", $b22), ";";
$a23 = [13,-23]; sort($a23); echo implode(",", $a23), ";";
$b23 = [13,-23]; rsort($b23); echo implode(",", $b23), ";";
$a24 = [36,5,49,30]; sort($a24); echo implode(",", $a24), ";";
$b24 = [36,5,49,30]; rsort($b24); echo implode(",", $b24), ";";
$a25 = [3,14,-1,23]; sort($a25); echo implode(",", $a25), ";";
$b25 = [3,14,-1,23]; rsort($b25); echo implode(",", $b25), ";";
$a26 = [18,24,2,24,-21]; sort($a26); echo implode(",", $a26), ";";
$b26 = [18,24,2,24,-21]; rsort($b26); echo implode(",", $b26), ";";
$a27 = [37,-47,-15,27,35]; sort($a27); echo implode(",", $a27), ";";
$b27 = [37,-47,-15,27,35]; rsort($b27); echo implode(",", $b27), ";";
$a28 = [39,-9]; sort($a28); echo implode(",", $a28), ";";
$b28 = [39,-9]; rsort($b28); echo implode(",", $b28), ";";
$a29 = [23,22,-37,41,33,-23,31,23]; sort($a29); echo implode(",", $a29), ";";
$b29 = [23,22,-37,41,33,-23,31,23]; rsort($b29); echo implode(",", $b29), ";";
$a30 = [-14,-35,-42,11]; sort($a30); echo implode(",", $a30), ";";
$b30 = [-14,-35,-42,11]; rsort($b30); echo implode(",", $b30), ";";
$a31 = [-39,-6,-42,2,-31,-48,-13]; sort($a31); echo implode(",", $a31), ";";
$b31 = [-39,-6,-42,2,-31,-48,-13]; rsort($b31); echo implode(",", $b31), ";";
echo "
";$s0 = ["pear","apple","fig"]; sort($s0); echo implode("|", $s0), ";";
$t0 = ["pear","apple","fig"]; rsort($t0); echo implode("|", $t0), ";";
$s1 = ["10","9","1e1","10.0"]; sort($s1); echo implode("|", $s1), ";";
$t1 = ["10","9","1e1","10.0"]; rsort($t1); echo implode("|", $t1), ";";
$s2 = ["abc","ABC","zz","a"]; sort($s2); echo implode("|", $s2), ";";
$s3 = ["9223372036854775808","9223372036854775807","9223372036854775809"]; sort($s3); echo implode("|", $s3), ";";
$s4 = ["1e400","1e401","inf"]; sort($s4); echo implode("|", $s4), ";";
$s5 = ["007","7","7.0"]; sort($s5); echo implode("|", $s5), ";";
$s6 = [" 1","1 ","1"]; sort($s6); echo implode("|", $s6), ";";
$s7 = ["only"]; sort($s7); echo implode("|", $s7), ";";
$s8 = []; sort($s8); echo implode("|", $s8), ";";
"##;

/// php-src 8.5.6's own output for `SORT_SOURCE`.
const SORT_EXPECTED: &str = r##"1,3,3,5,9;9,5,3,3,1;;;7;7;1,2;2,1;1,2,3;3,2,1;1,2,3;3,2,1;-5,-1,0,5;5,0,-1,-5;0,0,0;0,0,0;-9223372036854775808,0,9223372036854775807;9223372036854775807,0,-9223372036854775808;-2.5,0,1.5,1.5;1.5,1.5,0,-2.5;1,2,3;3,2,1;-0,0;-0,0;-34,19,25;25,19,-34;-42,10,24,27,30;30,27,24,10,-42;;;-26,-21,-17,10,19,20,41;41,20,19,10,-17,-21,-26;-31,-31,-21,0,10,16,31,31;31,31,16,10,0,-21,-31,-31;-49,-42,-30,35,44,49;49,44,35,-30,-42,-49;;;-47,-16,10,49;49,10,-16,-47;0,4,6,23,41,43;43,41,23,6,4,0;-38,-4;-4,-38;;;-23,13;13,-23;5,30,36,49;49,36,30,5;-1,3,14,23;23,14,3,-1;-21,2,18,24,24;24,24,18,2,-21;-47,-15,27,35,37;37,35,27,-15,-47;-9,39;39,-9;-37,-23,22,23,23,31,33,41;41,33,31,23,23,22,-23,-37;-42,-35,-14,11;11,-14,-35,-42;-48,-42,-39,-31,-13,-6,2;2,-6,-13,-31,-39,-42,-48;
apple|fig|pear;pear|fig|apple;9|10|1e1|10.0;10|1e1|10.0|9;ABC|a|abc|zz;9223372036854775807|9223372036854775808|9223372036854775809;1e400|1e401|inf;007|7|7.0; 1|1 |1;only;;"##;

/// Verifies `strrpos` finds the RIGHTMOST match and answers php-src's `int|false`.
///
/// Scanning right to left is what makes overlapping matches resolve to the last one —
/// `strrpos("aaa", "aa")` is 1, not 0 — and an empty needle answers the position just past the
/// end rather than zero, so `strrpos("abcabc", "")` is 6. Only the two-argument form is lowered:
/// the offset form's rule is NOT the mirror of `strpos`'s, since a negative offset there bounds
/// where the match may START counted from the end.
#[test]
fn test_cli_wasm_strrpos_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_strrpos");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function f(string $h, string $n): void {
    $p = strrpos($h, $n);
    if ($p === false) { echo "F"; } else { echo "@"; echo $p; }
    echo "|";
}
f("abcabc","b"); f("abcabc","z"); f("abcabc",""); f("","a"); f("",""); echo "\n";
f("abcabc","bc"); f("abcabc","abcabc"); f("aaa","aa"); f("abc","c"); f("abc","a"); echo "\n";
f("abcabc","abcabcd"); f("h\xc3\xa9llo","\xc3\xa9"); f("\x00\x01\x00","\x00"); f("aXbXc","X"); echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile strrpos to WASM");
    assert!(
        output.status.success(),
        "strrpos compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run strrpos under Node");
    assert!(
        run.status.success(),
        "strrpos trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program.
    let expected: Vec<u8> = [
        b"@4|F|@6|F|@0|\n".as_slice(),
        b"@4|@0|@1|@2|@0|\n".as_slice(),
        b"F|@1|@2|@3|\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `strstr` reproduces php-src in both arities, including its `string|false`.
///
/// The result is a REGION of the haystack — from the match to the end, or from the start up to
/// the match when `$before_needle` is true — so the two arities return different halves of the
/// same scan rather than one being a default of the other. An empty needle matches at offset 0,
/// which makes `strstr($h, "")` the whole string and its `before` form empty; a needle that is
/// absent gives false in BOTH arities. Binary samples are included because boxing under the
/// string tag persists a copy of the region rather than aliasing the source.
#[test]
fn test_cli_wasm_strstr_matches_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_strstr");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function f(string $h, string $n): void {
    $r = strstr($h, $n);
    if ($r === false) { echo "F"; } else { echo "[", $r, "]"; }
    echo "|";
}
function b(string $h, string $n): void {
    $r = strstr($h, $n, true);
    if ($r === false) { echo "F"; } else { echo "[", $r, "]"; }
    echo "|";
}
f("abcdef","cd"); f("abcdef","z"); f("abcdef",""); f("","a"); f("abcdef","a"); f("abcdef","f"); f("abcabc","bc"); echo "\n";
b("abcdef","cd"); b("abcdef","z"); b("abcdef",""); b("","a"); b("abcdef","a"); b("abcdef","f"); b("abcabc","bc"); echo "\n";
f("h\xc3\xa9llo","\xc3\xa9"); b("h\xc3\xa9llo","\xc3\xa9"); f("\x00\x01\x02","\x01"); b("\x00\x01\x02","\x01"); echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile strstr to WASM");
    assert!(
        output.status.success(),
        "strstr compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run strstr under Node");
    assert!(
        run.status.success(),
        "strstr trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own bytes for the same program. A byte literal rather than a `str`,
    // because the samples carry bytes no Rust string literal can hold.
    let expected: Vec<u8> = [
        b"[cdef]|F|[abcdef]|F|[abcdef]|[f]|[bcabc]|\n".as_slice(),
        b"[ab]|F|[]|F|[]|[abcde]|[a]|\n".as_slice(),
        b"[\xc3\xa9llo]|[h]|[\x01\x02]|[\x00]|\n".as_slice(),
    ]
    .concat();
    assert_eq!(run.stdout, expected);

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `strpos` and PHP's `===` against a runtime-tagged value.
///
/// These belong in one test because neither is usable without the other: `strpos` answers
/// `int|false`, which EIR carries as a tagged `Mixed` cell, and the whole point of the idiom
/// `strpos($h, $n) === false` is that it separates a match at OFFSET ZERO from a miss. Boxing the
/// miss as an int zero, or comparing the cell by storage rather than by tag, gets that backwards.
///
/// The tagged comparison is then exercised against every concrete type it admits. The float cases
/// are the ones a bit-for-bit payload comparison fails: `NAN === NAN` is false and
/// `0.0 === -0.0` is true. `null` is the other, because an unboxed null literal carries a
/// sentinel while an absent cell reads as zero, so only the tag can decide it.
#[test]
fn test_cli_wasm_strpos_and_tagged_strict_equality_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_strpos_strict");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function f(string $h, string $n): void {
    $p = strpos($h, $n);
    if ($p === false) { echo "F"; } else { echo "@"; echo $p; }
    echo "|";
}
f("abcabc","b"); f("abcabc","z"); f("abcabc",""); f("","a"); f("",""); echo "\n";
f("abcabc","B"); f("aXbXc","X"); f("abcabc","bc"); f("abcabc","abcabc"); f("abcabc","abcabcd"); echo "\n";
f("abc","a"); f("abc","c"); f("aaa","aa"); f("\x00\x01\x02","\x01"); f("h\xc3\xa9llo","\xc3\xa9"); echo "\n";
function g(string $h, string $n): void { echo strpos($h, $n) !== false ? "Y" : "N"; }
g("abc","a"); g("abc","z"); g("abc",""); echo "\n";
function probe(mixed $m): void {
    echo $m === 1 ? "i1" : "-";
    echo $m === "a" ? "sa" : "-";
    echo $m === true ? "T" : "-";
    echo $m === null ? "N" : "-";
    echo $m === 1.5 ? "f" : "-";
    echo $m === 0 ? "i0" : "-";
    echo $m === false ? "F" : "-";
    echo $m === "" ? "se" : "-";
    echo $m !== 1 ? "!i1" : "==";
    echo "|";
}
probe(1); probe("a"); probe(true); probe(null); probe(1.5); probe(0); probe(false); probe(""); probe(1.0); probe("A");
echo "\n";
function edge(mixed $m): void {
    echo $m === 0.0 ? "z" : "-";
    echo $m === -0.0 ? "nz" : "-";
    echo $m === NAN ? "nan" : "-";
    echo $m === INF ? "inf" : "-";
    echo $m === PHP_INT_MAX ? "max" : "-";
    echo "|";
}
edge(0.0); edge(-0.0); edge(NAN); edge(INF); edge(PHP_INT_MAX); edge(0);
echo "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile strpos and tagged equality to WASM");
    assert!(
        output.status.success(),
        "strpos/tagged equality compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run strpos and tagged equality under Node");
    assert!(
        run.status.success(),
        "strpos/tagged equality trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "@1|F|@0|F|@0|\n",
            "F|@1|@1|@0|F|\n",
            "@0|@2|@0|@1|@1|\n",
            "YNY\n",
            "i1-------==|-sa------!i1|--T-----!i1|---N----!i1|----f---!i1|-----i0--!i1|------F-!i1|-------se!i1|--------!i1|--------!i1|\n",
            "znz---|znz---|-----|---inf-|----max|-----|\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `str_repeat` answers like php-src and RAISES php-src's ValueError.
///
/// PHP does not clamp a negative `$times` to zero, it raises a `ValueError` an ordinary `catch`
/// receives — so this is the first builtin on this target whose failure is a PHP exception rather
/// than a machine guard, and it reuses the raise path the arithmetic errors already take. A count
/// of zero is NOT a failure: it answers the empty string.
#[test]
fn test_cli_wasm_str_repeat_matches_php_and_raises_its_value_error() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_str_repeat");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function r(string $s, int $n): string { return str_repeat($s, $n); }
echo bin2hex(r("ab", 0)), "|", bin2hex(r("ab", 1)), "|", bin2hex(r("ab", 3)), "|", bin2hex(r("", 5)), "|", bin2hex(r("a", 7)), "\n";
try { echo bin2hex(r("a", -1)), "\n"; } catch (\ValueError $e) { echo "caught|", get_class($e), "|", $e->getMessage(), "\n"; }
echo "end\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile str_repeat to WASM");
    assert!(
        output.status.success(),
        "str_repeat compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run str_repeat under Node");
    if !run.status.success() && String::from_utf8_lossy(&run.stderr).contains("CompileError") {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert!(
        run.status.success(),
        "the caught ValueError still killed the program: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "|6162|616261626162||61616161616161\n",
            "caught|ValueError|str_repeat(): Argument #2 ($times) must be greater than or equal to 0\n",
            "end\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `chr` and `ord` reproduce php-src, including the values PHP does not reject.
///
/// PHP does not refuse an out-of-range `chr`: it constrains the argument with `% 256`, bringing a
/// negative remainder back up, so `chr(-1)` is `\xff` and `chr(1000000)` is `\x40`. `ord` answers
/// 0 for the empty string and the FIRST byte of a longer one. Since PHP 8.5 both cases are
/// deprecated, but they still answer, and the value is what this compares.
///
/// Each helper is reached through a user function returning a `string`, which is what makes this
/// also the coverage for `Op::StrPersist`: without it the returned bytes would not outlive the
/// callee's frame.
#[test]
fn test_cli_wasm_chr_and_ord_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_chr_ord");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function c(int $n): string { return chr($n); }
function o(string $s): int { return ord($s); }
echo bin2hex(c(65)), "|", bin2hex(c(0)), "|", bin2hex(c(255)), "|", bin2hex(c(10)), "\n";
echo bin2hex(c(-1)), "|", bin2hex(c(-256)), "|", bin2hex(c(-257)), "|", bin2hex(c(256)), "|", bin2hex(c(257)), "|", bin2hex(c(1000000)), "\n";
echo o("A"), "|", o("\xff"), "|", o("0"), "|", o(""), "|", o("AB"), "\n";
echo bin2hex(c(o("Z"))), "|", o(c(200)), "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile chr/ord to WASM");
    assert!(
        output.status.success(),
        "chr/ord compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run chr/ord under Node");
    assert!(
        run.status.success(),
        "chr/ord trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "41|00|ff|0a\n",
            "ff|00|ff|00|01|40\n",
            "65|255|48|0|65\n",
            "5a|200\n",
        )
    );

    // php-src 8.5 diagnoses the six out-of-range `chr` arguments and the two `ord` arguments
    // that are not one byte, once each — counted rather than matched whole, because php-src also
    // prints a file, line and stack trace this target does not reproduce.
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(stderr.matches("chr():").count(), 6, "stderr was: {stderr}");
    assert_eq!(stderr.matches("ord():").count(), 2, "stderr was: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies base64 and url coding reproduce php-src, tolerant decoding included.
///
/// The samples pin what separates these from a textbook implementation. `urlencode` folds a
/// space to `+` and percent-encodes `~`, while `rawurlencode` does the opposite on both;
/// percent-encoding is UPPERCASE hex. Decoding never fails: `"a%2"` and `"a%zz"` keep a literal
/// `%`. One-argument `base64_decode` is non-strict, so `"YWJj="`, `"YW Jj"` and `"YWJj\n"` all
/// give `abc`, `"YWJ"` gives `ab`, and `"!!!!"` gives the empty string. Every result goes through
/// `bin2hex` where its bytes are not already printable ASCII.
#[test]
fn test_cli_wasm_base64_and_url_coding_match_php() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_codecs");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function t(string $s): void {
    echo bin2hex($s), "|", urlencode($s), "|", rawurlencode($s), "|",
         bin2hex(urldecode($s)), "|", bin2hex(rawurldecode($s)), "|",
         base64_encode($s), "|", bin2hex(base64_decode($s)), "\n";
}
t("");
t("a");
t("ab");
t("abc");
t("abcd");
t("a b+c~d.e_f-g");
t("h\xc3\xa9llo");
t("\x00\x01\xff");
t("a%2");
t("a%zz");
t("%C3%A9");
t("YWJj");
t("YWJj=");
t("YW Jj");
t("YWJ");
t("!!!!");
t("Hello, World!");
t("\n\r\t");
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the codecs to WASM");
    assert!(
        output.status.success(),
        "codec compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the codecs under Node");
    assert!(
        run.status.success(),
        "codecs trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "||||||\n",
            "61|a|a|61|61|YQ==|\n",
            "6162|ab|ab|6162|6162|YWI=|69\n",
            "616263|abc|abc|616263|616263|YWJj|69b7\n",
            "61626364|abcd|abcd|61626364|61626364|YWJjZA==|69b71d\n",
            "6120622b637e642e655f662d67|a+b%2Bc%7Ed.e_f-g|a%20b%2Bc~d.e_f-g|61206220637e642e655f662d67|6120622b637e642e655f662d67|YSBiK2N+ZC5lX2YtZw==|69bf9c75e7e0\n",
            "68c3a96c6c6f|h%C3%A9llo|h%C3%A9llo|68c3a96c6c6f|68c3a96c6c6f|aMOpbGxv|865968\n",
            "0001ff|%00%01%FF|%00%01%FF|0001ff|0001ff|AAH/|\n",
            "612532|a%252|a%252|612532|612532|YSUy|6b\n",
            "61257a7a|a%25zz|a%25zz|61257a7a|61257a7a|YSV6eg==|6b3c\n",
            "254333254139|%25C3%25A9|%25C3%25A9|c3a9|c3a9|JUMzJUE5|0b703d\n",
            "59574a6a|YWJj|YWJj|59574a6a|59574a6a|WVdKag==|616263\n",
            "59574a6a3d|YWJj%3D|YWJj%3D|59574a6a3d|59574a6a3d|WVdKaj0=|616263\n",
            "5957204a6a|YW+Jj|YW%20Jj|5957204a6a|5957204a6a|WVcgSmo=|616263\n",
            "59574a|YWJ|YWJ|59574a|59574a|WVdK|6162\n",
            "21212121|%21%21%21%21|%21%21%21%21|21212121|21212121|ISEhIQ==|\n",
            "48656c6c6f2c20576f726c6421|Hello%2C+World%21|Hello%2C%20World%21|48656c6c6f2c20576f726c6421|48656c6c6f2c20576f726c6421|SGVsbG8sIFdvcmxkIQ==|1de965a16a2b95\n",
            "0a0d09|%0A%0D%09|%0A%0D%09|0a0d09|0a0d09|Cg0J|\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a string literal reaches the module as PHP BYTES, not as Rust's UTF-8.
///
/// A PHP string is a byte string while a Rust `String` must be valid UTF-8, so the lexer carries
/// every escaped non-ASCII byte as a private-use marker char. A data segment written straight
/// from those Rust bytes turns `"\xff"` into the three UTF-8 bytes of U+E0FF, which `strlen`
/// then reports as 3. The native backend decodes through `string_bytes::literal_bytes`; this
/// pins that the WASM segments do too.
#[test]
fn test_cli_wasm_string_literals_carry_raw_php_bytes() {
    if Command::new("node").arg("--version").output().is_err() {
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_wasm_raw_literal_bytes");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
$high = "\xff";
$mixed = "\x00\x01\xfe\xff";
$octal = "\101\377";
$utf8 = "h\xc3\xa9llo";
echo strlen($high), "|", bin2hex($high), "\n";
echo strlen($mixed), "|", bin2hex($mixed), "\n";
echo strlen($octal), "|", bin2hex($octal), "\n";
echo strlen($utf8), "|", bin2hex($utf8), "\n";
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("wasm32-wasi")
        .arg(&php_path)
        .output()
        .expect("failed to compile the raw byte literals to WASM");
    assert!(
        output.status.success(),
        "raw byte literal compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let runner = dir.join("run.mjs");
    fs::write(
        &runner,
        r#"import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";
const wasi = new WASI({ version: "preview1", args: ["m"], env: {}, returnOnExit: true });
const bytes = readFileSync(process.argv[2]);
const instance = await WebAssembly.instantiate(
  await WebAssembly.compile(bytes),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
"#,
    )
    .unwrap();

    let run = Command::new("node")
        .arg("--no-warnings")
        .arg(&runner)
        .arg(dir.join("main.wasm"))
        .current_dir(&dir)
        .output()
        .expect("failed to run the raw byte literals under Node");
    assert!(
        run.status.success(),
        "raw byte literals trapped: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // php-src 8.5.6's own output for the same program.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        concat!(
            "1|ff\n",
            "4|0001feff\n",
            "2|41ff\n",
            "6|68c3a96c6c6f\n",
        )
    );

    let _ = fs::remove_dir_all(&dir);
}
