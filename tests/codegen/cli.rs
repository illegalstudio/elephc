//! Purpose:
//! Integration coverage for top-level compile/native dispatch and compiler output modes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Native help and managed-PCRE2 recovery diagnostics are exercised through subprocesses.
//! - Non-link modes must remain independent of installed native artifacts.

use crate::support::*;

/// End-to-end `elephc monitor`: compiles a busy fixture, samples it with
/// /usr/bin/sample, and writes a two-view Speedscope document whose frames are
/// PHP names — not EIR block labels, not runtime helpers in the folded view.
///
/// The export matters as much as the table. `--out` and `--pprof` were once
/// wired only to the sampled capture, so when the exact profile became the
/// default they wrote nothing at all — silently, including for the CI
/// regression gate, which is documented as `--out` a baseline and `--baseline`
/// it back. A test that only read the table would not have noticed.
#[test]
fn test_cli_monitor_writes_php_level_speedscope_profile() {
    let dir = make_cli_test_dir("elephc_cli_monitor");
    // The hot function is RECURSIVE on purpose: a self-recursive body cannot be
    // fully inlined away, so its frame is guaranteed in the samples — the test
    // must not depend on the best-effort inlined-frame recovery, whose address
    // bucketing varies run to run.
    fs::write(
        dir.join("busy.php"),
        "<?php\nfunction burn(int $depth) { $n = 0; for ($i = 0; $i < 2000000; $i = $i + 1) { $n = ($n + $i) % 1000003; } if ($depth > 0) { $n = ($n + burn($depth - 1)) % 1000003; } return $n; }\necho burn(4);\n",
    )
    .expect("failed to write the monitor fixture");

    let output = elephc_cli_command(&dir)
        .args(["monitor", "busy.php", "--out", "busy.prof.json"])
        .output()
        .expect("failed to run elephc monitor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("exact profile") && stdout.contains("burn"),
        "the table should be the exact profile and name the PHP function: {stdout}"
    );

    let raw = fs::read_to_string(dir.join("busy.prof.json"))
        .expect("monitor should write the speedscope file");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let profiles = doc["profiles"].as_array().expect("profiles array");
    assert_eq!(profiles.len(), 2, "one folded view and one why view");
    for profile in profiles {
        let weights: u64 = profile["weights"]
            .as_array()
            .expect("weights")
            .iter()
            .map(|w| w.as_u64().expect("integer weight"))
            .sum();
        assert_eq!(
            weights,
            profile["endValue"].as_u64().expect("endValue"),
            "weights must partition the profile total"
        );
    }
    let frames = doc["shared"]["frames"].as_array().expect("frames");
    // `burn` may still appear as `burn (inlined)` for partially inlined
    // shallow calls; either spelling proves the PHP-level attribution worked.
    assert!(
        frames
            .iter()
            .any(|f| f["name"].as_str().is_some_and(|n| n.starts_with("burn"))),
        "frames should carry the demangled PHP name: {raw}"
    );
    assert!(
        !frames
            .iter()
            .any(|f| f["name"].as_str().is_some_and(|n| n.contains("eir_"))),
        "no EIR block label may leak into the profile: {raw}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies both compiler version flags print the Cargo package version and exit successfully.
#[test]
fn test_cli_version_flags_report_package_version() {
    let dir = make_cli_test_dir("elephc_cli_version");
    let expected = format!("elephc {}\n", env!("CARGO_PKG_VERSION"));

    for flag in ["--version", "-V"] {
        let output = elephc_cli_command(&dir)
            .arg(flag)
            .output()
            .unwrap_or_else(|error| panic!("failed to run elephc {flag}: {error}"));
        assert!(output.status.success(), "elephc {flag} should succeed");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert!(output.stderr.is_empty(), "elephc {flag} should not write stderr");
    }

    let help = elephc_cli_command(&dir)
        .arg("--help")
        .output()
        .expect("failed to run elephc --help");
    assert!(help.status.success(), "elephc --help should succeed");
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains(&format!("Version: {}", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains("-V, --version"));

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

/// The teardown calls in main's epilogue must run on an aligned stack (x86_64).
///
/// System V AMD64 wants `rsp` 16-byte aligned AT the call. After `leave`, `rsp`
/// holds its entry value — already 8 past alignment — so every call emitted
/// after the frame restore is off by 8. The hand-written runtime helpers
/// tolerate that; compiled Rust does not, because an aligned SSE store to a
/// stack temporary faults.
///
/// It cost a CI shard on linux-x86_64 alone: `main` ran, printed its output, and
/// died in the profiler's exit dump — the last call before the exit syscall and
/// the first one made of Rust. AArch64 keeps `sp` aligned by construction, so
/// the same commit was green there and the failure looked like a profiler bug
/// for an afternoon.
///
/// Read from the assembly because that is where the property lives, and because
/// this host cannot execute the architecture that has it.
#[test]
fn test_cli_x86_64_epilogue_aligns_before_its_teardown_calls() {
    let dir = make_cli_test_dir("elephc_cli_x86_epilogue_align");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php\nfunction f(int $n): int { return $n + 1; }\necho f(1);\n").unwrap();

    let output = elephc_cli_command(&dir)
        .args(["--with-monitoring", "--target", "linux-x86_64", "--emit-asm"])
        .arg(&php_path)
        .output()
        .expect("failed to emit x86_64 assembly");
    assert!(
        output.status.success(),
        "cross-target --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("expected target assembly output");
    let lines: Vec<&str> = asm.lines().map(str::trim).collect();

    // Anchor on the teardown calls themselves, walk BACK to the frame restore
    // that precedes them, and require the realignment in between. Scanning
    // forward from `leave` and stopping at the first alignment was the version
    // that asserted nothing at all — it never reached a call.
    let teardown = lines
        .iter()
        .position(|line| line.starts_with("call elephc_probe_dump")
            || line.starts_with("call elephc_instr_dump"))
        .expect("a monitored build must emit its exit dump");
    let restore = lines[..teardown]
        .iter()
        .rposition(|line| *line == "leave")
        .expect("the exit dump follows main's frame restore");
    let between = &lines[restore + 1..teardown];
    assert!(
        between.iter().any(|line| line.starts_with("and rsp, -16")),
        "`{}` is called after `leave` with no realignment between them, so it \
         runs 8 bytes off the alignment the ABI promises it. Between:\n{}",
        lines[teardown],
        between.join("\n")
    );
    assert!(
        !between.iter().any(|line| line.starts_with("call ")),
        "nothing may be called between the frame restore and the realignment"
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

/// Verifies compile-time web isolation selects one bridge symbol and leaves the default
/// assembly byte-identical to an explicit `worker` selection.
#[test]
fn test_cli_web_isolation_selects_entry_symbol_at_compile_time() {
    let dir = make_cli_test_dir("elephc_cli_web_isolation_symbols");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let compile = |flags: &[&str]| {
        let output = elephc_cli_command(&dir)
            .args(flags)
            .arg(&php_path)
            .output()
            .expect("failed to compile web-isolation fixture");
        assert!(
            output.status.success(),
            "web-isolation compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(dir.join("main.s")).expect("failed to read web-isolation assembly")
    };

    let default_worker = compile(&["--web"]);
    let explicit_worker = compile(&["--web", "--web-isolation=worker"]);
    assert_eq!(
        default_worker, explicit_worker,
        "plain --web must emit exactly the explicit worker entry path"
    );
    assert!(default_worker.contains("elephc_web_run"));
    assert!(!default_worker.contains("elephc_web_run_pool"));
    assert!(!default_worker.contains("elephc_web_run_request"));

    let pool = compile(&["--web", "--web-isolation=pool"]);
    assert!(pool.contains("elephc_web_run_pool"));
    assert!(!pool.contains("elephc_web_run_request"));

    let request = compile(&["--web", "--web-isolation=request"]);
    assert!(request.contains("elephc_web_run_request"));
    assert!(!request.contains("elephc_web_run_pool"));

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies web-isolation is rejected without web mode and reports invalid model names.
#[test]
fn test_cli_web_isolation_validation_errors_are_focused() {
    let dir = make_cli_test_dir("elephc_cli_web_isolation_errors");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let without_web = elephc_cli_command(&dir)
        .arg("--web-isolation=pool")
        .arg(&php_path)
        .output()
        .expect("failed to run web-isolation validation fixture");
    assert!(!without_web.status.success());
    assert!(
        String::from_utf8_lossy(&without_web.stderr).contains("--web-isolation requires --web"),
        "unexpected missing-web diagnostic: {}",
        String::from_utf8_lossy(&without_web.stderr)
    );

    let invalid = elephc_cli_command(&dir)
        .args(["--web", "--web-isolation=banana"])
        .arg(&php_path)
        .output()
        .expect("failed to run invalid web-isolation fixture");
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("expected worker|pool|request"),
        "unexpected invalid-mode diagnostic: {}",
        String::from_utf8_lossy(&invalid.stderr)
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

/// Verifies `--debug-info` survives a source path that carries assembler string
/// metacharacters. A `\` used to be spliced into `.file`/`.asciz` unescaped, so
/// the assembler rejected the module outright; combined with `"` it terminated
/// the directive string early and let the rest of the path be assembled as
/// directives. The full compile must now succeed and the program must run.
#[test]
fn test_cli_debug_info_escapes_metacharacters_in_source_path() {
    let dir = make_cli_test_dir("elephc_cli_debug_info_escapes");
    // A backslash alone broke the assembler; `\"` was the directive-injection
    // vector. Both are legal filename bytes on every supported target.
    let php_path = dir.join("bs\\la\"sh.php");
    fs::write(
        &php_path,
        r#"<?php
function greet(): void { echo "escaped\n"; }
greet();
"#,
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--debug-info")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --debug-info");

    assert!(
        output.status.success(),
        "elephc --debug-info failed for a path with `\\` and `\"`: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("bs\\la\"sh.s")).expect("failed to read assembly");
    let file_line = asm.lines().next().expect("assembly is empty");
    assert!(
        file_line.contains("bs\\\\la\\\"sh.php\""),
        "source path must be escaped inside the .file string: {file_line}"
    );
    assert!(
        asm.contains(".asciz \"") && asm.contains("bs\\\\la\\\"sh.php\""),
        "source path must be escaped inside the compile-unit DW_AT_name too"
    );
    for line in asm.lines() {
        assert!(
            !line.starts_with(".globl bs") && !line.trim_start().starts_with("sh.php"),
            "path bytes leaked out of their directive: {line}"
        );
    }

    let run = std::process::Command::new(dir.join("bs\\la\"sh"))
        .output()
        .expect("failed to run the compiled binary");
    assert!(run.status.success(), "compiled binary did not run");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "escaped\n");

    let _ = fs::remove_dir_all(&dir);
}


/// End-to-end `--with-monitoring`: a binary that carries the tooling is silent
/// until asked, and reports fully when `monitor` asks.
///
/// Both halves matter. The silence is the property that makes the capability
/// safe to ship — a program that starts emitting profiler output on its own
/// stderr would be a surprise its author cannot explain. And the reporting is
/// what the capability is for. Asserting only one of them would let the other
/// break unnoticed.
///
/// macOS-only: the fixture is CPU-bound so SIGPROF samples are guaranteed.
#[cfg(target_os = "macos")]
#[test]
fn test_cli_probe_embeds_in_process_sampler() {
    let dir = make_cli_test_dir("elephc_cli_probe");
    fs::write(
        dir.join("burn.php"),
        "<?php\nfunction burn(int $depth): int { $n = 0; for ($i = 0; $i < 6000000; $i = $i + 1) { $n = ($n + $i) % 1000003; } if ($depth > 0) { $n = ($n + burn($depth - 1)) % 1000003; } return $n; }\necho burn(4);\n",
    )
    .expect("failed to write the probe fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "burn.php"])
        .output()
        .expect("failed to run elephc --probe");
    assert!(
        compile.status.success(),
        "probe compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    // Run on its own: capable, but nobody asked.
    let alone = std::process::Command::new(dir.join("burn"))
        .output()
        .expect("failed to run the monitored binary");
    assert!(alone.status.success(), "monitored binary did not run");
    assert_eq!(String::from_utf8_lossy(&alone.stdout), "855");
    let quiet = String::from_utf8_lossy(&alone.stderr);
    assert!(
        !quiet.contains("elephc-probe") && !quiet.contains("elephc-instr"),
        "a binary nobody asked must not announce a profiler: {quiet}"
    );

    // Run through `monitor`, which asks over the control channel.
    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./burn"])
        .output()
        .expect("failed to run elephc monitor");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&watched.stdout),
        String::from_utf8_lossy(&watched.stderr)
    );
    assert!(
        report.contains("burn"),
        "the profile should name the PHP function, symbolized from the embedded table: {report}"
    );

    let _ = fs::remove_dir_all(&dir);
}
