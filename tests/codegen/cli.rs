//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of cli, including check stops after typecheck, emit asm writes assembly only, and rejects emit asm and check together.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

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

/// Verifies cross-target `--emit-asm` stops before preparing a host-incompatible runtime object.
#[test]
fn test_cli_emit_asm_does_not_require_target_assembler() {
    let dir = make_cli_test_dir("elephc_cli_emit_cross_target_asm");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'cross-target';").unwrap();

    let target = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    };
    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg(target)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run cross-target elephc CLI with --emit-asm");

    assert!(
        output.status.success(),
        "cross-target elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.join("main.s").exists(), "expected target assembly output");
    assert!(
        !dir.join("main.o").exists() && !dir.join("main").exists(),
        "cross-target --emit-asm must not assemble or link"
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

/// Verifies `--check --timings` reports per-phase timings for tokenize, parse,
/// typecheck, and total — without running codegen/assemble/link phases.
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
    assert!(stderr.contains("Compiler timings:"), "missing timings header: {stderr}");
    assert!(stderr.contains("tokenize"), "missing tokenize timing: {stderr}");
    assert!(stderr.contains("parse"), "missing parse timing: {stderr}");
    assert!(stderr.contains("typecheck"), "missing typecheck timing: {stderr}");
    assert!(stderr.contains("total"), "missing total timing: {stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--timings` reports codegen, assemble, link, and total durations
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
    assert!(stderr.contains("codegen"), "missing codegen timing: {stderr}");
    assert!(stderr.contains("assemble"), "missing assemble timing: {stderr}");
    assert!(stderr.contains("link"), "missing link timing: {stderr}");
    assert!(stderr.contains("total"), "missing total timing: {stderr}");
    assert!(dir.join("main").exists(), "expected compiled binary to exist");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the runtime cache: the first compile produces a "runtime-cache miss"
/// and caches a runtime .o object; the second compile with the same input hits
/// the cache ("runtime-cache hit") without recompiling the runtime.
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
        first_stderr.contains("runtime-cache miss"),
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
        second_stderr.contains("runtime-cache hit"),
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
