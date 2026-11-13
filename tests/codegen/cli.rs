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

/// A top-level-only program still produces one exact `{main}` frame when run
/// through the real compile/control-channel/monitor pipeline.
#[test]
fn test_cli_monitor_profiles_a_top_level_only_program() {
    let dir = make_cli_test_dir("elephc_cli_monitor_main_only");
    fs::write(
        dir.join("top.php"),
        "<?php\n$sum = 0; for ($i = 0; $i < 200000; $i = $i + 1) { $sum = ($sum + $i) % 100003; } echo $sum;\n",
    )
    .expect("failed to write the top-level monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "top.php",
            "--out",
            "top.prof.json",
            "--save",
            "top.exact.json",
        ])
        .output()
        .expect("failed to run exact top-level monitor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "top-level monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("exact profile"), "{stdout}");
    assert!(stdout.contains("{main}"), "the exact root is missing: {stdout}");

    let raw = fs::read_to_string(dir.join("top.prof.json"))
        .expect("top-level monitor should write its Speedscope export");
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert!(
        doc["shared"]["frames"]
            .as_array()
            .expect("frames")
            .iter()
            .any(|frame| frame["name"] == "{main}"),
        "the export must preserve the exact root: {raw}"
    );
    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("top.exact.json")).expect("saved exact graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    assert_eq!(nodes.len(), 1, "a top-level-only run has one frame: {exact}");
    assert_eq!(nodes[0]["name"], "{main}");
    assert_eq!(nodes[0]["call_count"], 1, "root must enter and exit once");
    assert!(
        exact["edges"].as_array().expect("exact edges").is_empty(),
        "the root must not call itself: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// End-to-end `elephc monitor`: compiles a busy fixture, activates exact
/// instrumentation through the control channel, and writes a two-view
/// Speedscope document whose frames are PHP names — not EIR block labels or
/// runtime helpers in the folded view.
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
        .args([
            "monitor",
            "busy.php",
            "--out",
            "busy.prof.json",
            "--save",
            "busy.exact.json",
        ])
        .output()
        .expect("failed to run elephc monitor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("exact profile") && stdout.contains("{main}") && stdout.contains("burn"),
        "the exact table should contain its root and the PHP function: {stdout}"
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

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("busy.exact.json")).expect("saved exact graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let burn = nodes.iter().position(|node| node["name"] == "burn").unwrap();
    assert_eq!(nodes[main]["call_count"], 1, "root must enter exactly once");
    assert!(
        exact["edges"].as_array().expect("exact edges").iter().any(|edge| {
            edge["from"] == main && edge["to"] == burn && edge["count"] == 1
        }),
        "the ordinary call must be attributed from {{main}} to burn: {exact}"
    );
    assert!(
        !exact["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["to"] == main),
        "no function may be misattributed as calling the root: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Shutdown output handlers and object destructors remain children of the
/// exact `{main}` frame instead of becoming disconnected graph roots.
#[test]
fn test_cli_monitor_keeps_shutdown_callbacks_under_main() {
    let dir = make_cli_test_dir("elephc_cli_monitor_shutdown_callbacks");
    fs::write(
        dir.join("shutdown.php"),
        r#"<?php
function shutdown_handler(string $contents, int $phase): string {
    $n = 0;
    for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    if ($phase < 0) { echo $n; }
    return $contents;
}
class CleanupProbe {
    public function __destruct() {
        $n = 0;
        for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    }
}
$probe = new CleanupProbe();
ob_start(shutdown_handler(...));
echo "profiled\n";
"#,
    )
    .expect("failed to write the shutdown monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "shutdown.php",
            "--out",
            "shutdown.prof.json",
            "--save",
            "shutdown.exact.json",
        ])
        .output()
        .expect("failed to monitor shutdown callbacks");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "shutdown monitor should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("shutdown.exact.json")).expect("saved shutdown graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let edges = exact["edges"].as_array().expect("exact edges");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let handler = nodes
        .iter()
        .position(|node| node["name"] == "shutdown_handler")
        .unwrap();
    let destructor = nodes
        .iter()
        .position(|node| node["name"] == "CleanupProbe::__destruct")
        .unwrap();
    for child in [handler, destructor] {
        assert!(
            edges.iter().any(|edge| {
                edge["from"] == main && edge["to"] == child && edge["count"] == 1
            }),
            "shutdown PHP work must remain below {{main}}: {exact}"
        );
    }
    assert!(
        nodes[main]["inclusive"].as_u64().unwrap()
            > nodes[main]["exclusive"].as_u64().unwrap(),
        "the root must subtract shutdown callees from self time: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Runs one clean language-level termination fixture and verifies that monitor
/// receives the complete exact graph despite bypassed generated epilogues.
fn assert_clean_language_exit_profile(tag: &str, source: &str) {
    let dir = make_cli_test_dir(tag);
    fs::write(
        dir.join("exit.php"),
        source,
    )
    .expect("failed to write the clean-exit monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "exit.php",
            "--out",
            "exit.prof.json",
            "--save",
            "exit.exact.json",
        ])
        .output()
        .expect("failed to monitor a clean language exit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a clean language exit should publish a profile\nstdout: {stdout}\nstderr: {stderr}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("exit.exact.json")).expect("saved exit graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let child = nodes
        .iter()
        .position(|node| node["name"] == "before_exit")
        .unwrap();
    assert_eq!(nodes[main]["call_count"], 1);
    assert!(
        exact["edges"].as_array().expect("exact edges").iter().any(|edge| {
            edge["from"] == main && edge["to"] == child && edge["count"] == 1
        }),
        "the clean exit must publish the complete rooted graph: {exact}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `exit(0)` closes and publishes the active exact stack even though normal
/// generated function and main epilogues are bypassed.
#[test]
fn test_cli_monitor_profiles_a_clean_language_exit() {
    assert_clean_language_exit_profile(
        "elephc_cli_monitor_exit_zero",
        r#"<?php
function before_exit(): int {
    $n = 0;
    for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    return $n;
}
$value = before_exit();
if ($value < 0) { echo $value; }
exit(0);
"#,
    );
}

/// The zero-argument `die()` lowering publishes the same rooted exact graph as
/// the status-bearing `exit(0)` path.
#[test]
fn test_cli_monitor_profiles_clean_die_without_status() {
    assert_clean_language_exit_profile(
        "elephc_cli_monitor_die_no_status",
        r#"<?php
function before_exit(): int {
    $n = 0;
    for ($i = 0; $i < 10000; $i = $i + 1) { $n = $n + $i; }
    return $n;
}
$value = before_exit();
if ($value < 0) { echo $value; }
die();
"#,
    );
}

/// An uncaught generated PHP error bypasses every ordinary epilogue just like
/// `exit()`, so it must still close and publish the exact root and live callee.
#[test]
fn test_cli_monitor_profiles_an_uncaught_codegen_error() {
    let dir = make_cli_test_dir("elephc_cli_monitor_uncaught_codegen_error");
    // The negative-value recursive branch keeps the failing function out of the
    // inliner while `$argc` still takes the direct uncaught path at runtime.
    fs::write(
        dir.join("uncaught.php"),
        r#"<?php
function fail_uncaught(int $value): int {
    if ($value < 0) { return fail_uncaught(-$value); }
    return intdiv($value, $value - $value);
}
fail_uncaught($argc);
"#,
    )
    .expect("failed to write the uncaught-error monitoring fixture");

    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "uncaught.php",
            "--out",
            "uncaught.prof.json",
            "--save",
            "uncaught.exact.json",
        ])
        .output()
        .expect("failed to monitor an uncaught generated error");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "an uncaught generated error should still publish a profile\nstdout: {stdout}\nstderr: {stderr}"
    );

    let exact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("uncaught.exact.json"))
            .expect("saved uncaught-error graph"),
    )
    .expect("valid exact graph");
    let nodes = exact["nodes"].as_array().expect("exact nodes");
    let main = nodes.iter().position(|node| node["name"] == "{main}").unwrap();
    let failing = nodes
        .iter()
        .position(|node| node["name"] == "fail_uncaught")
        .unwrap();
    assert_eq!(nodes[main]["call_count"], 1);
    assert!(
        exact["edges"].as_array().expect("exact edges").iter().any(|edge| {
            edge["from"] == main && edge["to"] == failing && edge["count"] == 1
        }),
        "the uncaught error must publish the complete rooted graph: {exact}"
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
    assert!(stdout.contains("name `{main}` for the top-level root"));

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

/// Verifies a final `--with-curl` link without a project fails with the same recovery
/// style as PCRE2. The compiled program itself never calls a `curl_*` function — the
/// explicit `--with-curl` flag is what forces `elephc_curl` into the plan here, exercising
/// that override path directly rather than source-based detection — but planning it must
/// still emit the `curl` native requirement and never fall back to a system `-lcurl`.
#[test]
fn test_cli_with_curl_final_link_requires_managed_curl_project() {
    let dir = make_cli_test_dir("elephc_cli_curl_requires_native");
    let cache = dir.join("native-cache-must-not-exist");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-curl")
        .arg(&php_path)
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run --with-curl compilation");
    assert!(!output.status.success(), "--with-curl link without a project must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("curl support requires managed native package curl"),
        "unexpected missing-project diagnostic: {stderr}"
    );
    assert!(stderr.contains("project: not found"), "missing project context: {stderr}");
    assert!(
        stderr.contains("recovery: cd --") && stderr.contains("elephc native add curl"),
        "missing copy-paste recovery command: {stderr}"
    );
    assert!(!cache.exists(), "failed compilation must not create the native cache");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a final `--with-xml` link without a project fails with the same managed-package
/// recovery style as curl and PCRE2. The compiled program itself never names the xml
/// surface — the explicit `--with-xml` flag is what forces `elephc_xml` into the plan here —
/// but planning it must still emit the `libxml2` native requirement (the bridge's parser IS
/// the managed libxml2, reached through the catalog's shim) and never fall back to a system
/// `-lxml2`. The assertion matches the package half of the diagnostic only, so it holds for
/// whichever feature label the resolver prints in front of it.
#[test]
fn test_cli_with_xml_final_link_requires_managed_libxml2_project() {
    let dir = make_cli_test_dir("elephc_cli_xml_requires_native");
    let cache = dir.join("native-cache-must-not-exist");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1;").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-xml")
        .arg(&php_path)
        .env("ELEPHC_NATIVE_CACHE", &cache)
        .output()
        .expect("failed to run --with-xml compilation");
    assert!(!output.status.success(), "--with-xml link without a project must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("support requires managed native package libxml2"),
        "unexpected missing-project diagnostic: {stderr}"
    );
    assert!(stderr.contains("project: not found"), "missing project context: {stderr}");
    assert!(
        stderr.contains("recovery: cd --") && stderr.contains("elephc native add libxml2"),
        "missing copy-paste recovery command: {stderr}"
    );
    assert!(
        !stderr.contains("-lxml2") && !stderr.contains("required Elephc bridge"),
        "the managed-package diagnostic must fire before any link attempt: {stderr}"
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
        .arg("--timings")
        .arg(&php_path)
        .output()
        .expect("failed to run cross-target elephc CLI with --emit-asm");

    assert!(
        output.status.success(),
        "cross-target elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Writing assembly"),
        "emit-asm should report its final output phase: {stderr}"
    );
    for skipped_phase in [
        "Preparing runtime object",
        "Assembling object file",
        "Linking native output",
        "Runtime cache:",
    ] {
        assert!(
            !stderr.contains(skipped_phase),
            "emit-asm unexpectedly reported `{skipped_phase}`: {stderr}"
        );
    }
    assert!(dir.join("main.s").exists(), "expected target assembly output");
    assert!(
        !dir.join("main.o").exists() && !dir.join("main").exists(),
        "cross-target --emit-asm must not assemble or link"
    );
    let runtime_cache = dir.join("cache-root").join("elephc");
    assert!(
        !runtime_cache.exists(),
        "cross-target --emit-asm must not prepare a runtime object cache at {}",
        runtime_cache.display()
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

/// Verifies the x86_64 web bridge reaches the handler and the application callbacks through
/// entries that preserve rbx and r12-r15: an optimized bridge keeps its worker loop state in
/// them across the handler call, and the worker died after its first request.
#[test]
fn test_cli_web_x86_64_bridge_calls_preserve_callee_saved_registers() {
    let dir = make_cli_test_dir("elephc_cli_web_x86_rust_entries");
    let php_path = dir.join("main.php");
    // `--web` refuses `--emit-asm`, so this is a full build. `main.s` is written before the
    // assembler runs, which is as far as a host without an x86_64 toolchain gets.
    let compile = |source: &str, flags: &[&str]| {
        fs::write(&php_path, source).unwrap();
        let _ = fs::remove_file(dir.join("main.s"));
        let output = elephc_cli_command(&dir)
            .args(["--target", "linux-x86_64"])
            .args(flags)
            .arg(&php_path)
            .output()
            .expect("failed to compile web entry fixture");
        fs::read_to_string(dir.join("main.s")).unwrap_or_else(|_| {
            panic!("web entry compile wrote no assembly: {}", String::from_utf8_lossy(&output.stderr))
        })
    };
    let preserving_entry = |asm: &str, body: &str| {
        let entry = format!(
            "{body}_rust_entry:\n    push rbx\n    push r12\n    push r13\n    push r14\n    push r15\n    call {body}\n    pop r15\n    pop r14\n    pop r13\n    pop r12\n    pop rbx\n    ret\n"
        );
        assert!(asm.contains(&entry), "missing the preserving entry for {body}");
    };

    let worker = compile("<?php echo 'ok';", &["--web"]);
    preserving_entry(&worker, "_elephc_web_handler");
    assert!(worker.contains("lea rdx, [rip + _elephc_web_handler_rust_entry]"), "the worker bridge must get the entry");

    let application = compile(
        r#"<?php
final class App implements Elephc\Web\WorkerApplication {
    public function handle(Elephc\Web\Request $request): void { echo 'ok'; }
    public function shutdown(): void {}
}
Elephc\Web\Worker::register(new App());
"#,
        &["--web", "--web-lifecycle=application"],
    );
    for body in [
        "_elephc_web_handler",
        "_fn__u__u_elephc_u_web_u_application_u_ready",
        "_fn__u__u_elephc_u_web_u_application_u_request",
        "_fn__u__u_elephc_u_web_u_application_u_shutdown",
    ] {
        preserving_entry(&application, body);
        assert!(application.contains(&format!(".quad {body}_rust_entry")), "the callback table must hold the entry for {body}");
    }

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

/// Verifies `--with-pdo` roots the complete injected PDO group even without source-level PDO use.
#[test]
fn test_with_pdo_keeps_unreferenced_pdo_function() {
    let dir = make_cli_test_dir("elephc_cli_with_pdo_reachability");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-pdo")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile forced PDO assembly");
    assert!(
        output.status.success(),
        "elephc --with-pdo --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read PDO assembly");
    let symbol = elephc::names::function_symbol("pdo_drivers");
    assert!(
        asm.contains(&format!(".globl {symbol}\n")),
        "--with-pdo must keep unreferenced PDO declarations"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--with-crypto` force-links the bridge without force-injecting the hash prelude.
#[test]
fn test_with_crypto_does_not_force_hash_prelude() {
    let dir = make_cli_test_dir("elephc_cli_with_crypto_reachability");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 'ok';").unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-crypto")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile forced crypto assembly");
    assert!(
        output.status.success(),
        "elephc --with-crypto --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read crypto assembly");
    let hash_init = elephc::names::function_symbol("hash_init");
    assert!(
        !asm.contains(&format!(".globl {hash_init}\n")),
        "--with-crypto must not inject the source-level hash prelude"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--with-eval` keeps user declarations available to opaque runtime source.
#[test]
fn test_with_eval_keeps_unreferenced_user_declaration() {
    let dir = make_cli_test_dir("elephc_cli_with_eval_reachability");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php function runtime_only(): string { return 'eval'; } echo 'ok';",
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--with-eval")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to compile forced eval assembly");
    assert!(
        output.status.success(),
        "elephc --with-eval --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read eval assembly");
    let symbol = elephc::names::function_symbol("runtime_only");
    assert!(
        asm.contains(&format!(".globl {symbol}\n")),
        "--with-eval must keep unreferenced user declarations"
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
    assert_eq!(asm.matches("_eir_callable_argument_normalizer:\n").count(), 1,
        "unrestricted call sites must share one candidate resolver");
    assert!(asm.contains("_callable_invoker_"), "the shared resolver emits native invokers");
    for name in ["first", "second"] {
        let body = asm.split_once(&format!("@fn name={name} symbol=")).unwrap().1
            .split_once(&format!("@endfn name={name}")).unwrap().0;
        assert_eq!(body.matches("_eir_callable_argument_normalizer").count(), 1,
            "{name}: each call site reuses the module resolver");
        assert!(!asm.contains(&format!("_eir_{name}_callable_invoker")),
            "{name}: candidate invokers must not be attributed to a call site");
    }

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

/// Verifies `--source-map` emits a sidecar .map file in the v3 schema:
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
        map.contains("\"version\": 3"),
        "missing source map schema version: {map}"
    );
    assert!(
        map.contains("\"sources\": ["),
        "missing source map sources array: {map}"
    );
    assert!(
        map.contains("\"source_id\":"),
        "missing source ids in source map mappings: {map}"
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

/// Verifies the LINKED binary's debug info resolves through `addr2line`, not just the assembly.
///
/// Follow-up for #943. The two fixtures around this one assert assembler DIRECTIVES — a
/// `.file` header, a `.loc`, an escaped `DW_AT_name`. What they cannot say is whether the
/// assembler and linker turned those into debug info a debugger can actually use: a compile
/// unit whose `DW_TAG_subprogram` anchors carry `DW_AT_low_pc`, and a line program covering
/// those addresses. The author measured that separately; CI did not.
///
/// The probe is `addr2line -f` over the bytes of a known function. The exact prologue length
/// is not part of the contract, so the fixture sweeps the first 256 bytes of `greet` rather
/// than naming one offset, and requires at least one of them to answer both the function name
/// and the PHP file and line it came from.
///
/// Linux only, and skipped when binutils is absent: this is about the ELF toolchain's own
/// consumption of what elephc emitted.
#[cfg(target_os = "linux")]
#[test]
fn test_cli_debug_info_resolves_compile_unit_anchors_with_addr2line() {
    /// Reports whether an external tool cannot be invoked for its version probe.
    fn tool_missing(tool: &str) -> bool {
        std::process::Command::new(tool)
            .arg("--version")
            .output()
            .is_err()
    }
    if tool_missing("addr2line") || tool_missing("nm") {
        eprintln!("skipping: binutils addr2line/nm not available");
        return;
    }

    let dir = make_cli_test_dir("elephc_cli_debug_info_addr2line");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
function greet(): void {
    echo "hi\n";
}
greet();
"#,
    )
    .unwrap();

    // This probes a standalone subprogram anchor, so retain greet's call boundary even
    // when the optimizer can inline a discarded void call and the linker can remove it.
    let output = elephc_cli_command(&dir)
        .arg("--no-ir-opt")
        .arg("--debug-info")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --debug-info");
    assert!(
        output.status.success(),
        "elephc --debug-info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let binary = dir.join("main");

    // The symbol prefix differs by target, so both spellings are accepted.
    let symbols = std::process::Command::new("nm")
        .arg(&binary)
        .output()
        .expect("failed to run nm");
    let listing = String::from_utf8_lossy(&symbols.stdout);
    let greet_address = listing
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let address = fields.next()?;
            let _kind = fields.next()?;
            let name = fields.next()?;
            (name == "_fn_greet" || name == "fn_greet")
                .then(|| u64::from_str_radix(address, 16).ok())
                .flatten()
        })
        .unwrap_or_else(|| panic!("no greet symbol in the linked binary:\n{listing}"));

    let probes: Vec<String> = (0..64)
        .map(|step| format!("0x{:x}", greet_address + step * 4))
        .collect();
    let resolved = std::process::Command::new("addr2line")
        .arg("-f")
        .arg("-e")
        .arg(&binary)
        .args(&probes)
        .output()
        .expect("failed to run addr2line");
    let answers = String::from_utf8_lossy(&resolved.stdout);

    // `addr2line -f` prints the function name and then the source location, per address.
    let expected_location = format!("{}:3", php_path.display());
    let anchored = answers
        .lines()
        .collect::<Vec<_>>()
        .chunks(2)
        .any(|pair| pair.len() == 2 && pair[0] == "greet" && pair[1] == expected_location);
    assert!(
        anchored,
        "no address inside greet resolved to {expected_location}; addr2line said:\n{answers}"
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

/// Debug builds assemble and run methods and generator bodies with separate exceptional cleanup entries.
#[test]
fn test_cli_debug_info_delimits_method_and_generator_cleanup_sections() {
    let dir = make_cli_test_dir("elephc_debug_cleanup_ranges");
    let php_path = dir.join("main.php");
    fs::write(&php_path, r#"<?php
function debugItems(int $value): Generator { yield $value; }
class DebugCleanupRange {
    public function read(int $value): int { return $value + 1; }
    public function items(int $value): Generator { yield $value + 2; }
}
$object = new DebugCleanupRange();
echo $object->read($argc), ":";
foreach (debugItems($argc) as $value) { echo $value, ":"; }
foreach ($object->items($argc) as $value) { echo $value; }
"#).unwrap();
    let output = elephc_cli_command(&dir).arg("--debug-info").arg(&php_path).output().unwrap();
    assert!(output.status.success(), "debug compilation failed: {}", String::from_utf8_lossy(&output.stderr));
    let run = Command::new(dir.join("main")).output().unwrap();
    assert!(run.status.success(), "debug binary failed: {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2:1:3");
    let _ = fs::remove_dir_all(&dir);
}

/// Opaque eval registers native invokers without moving the enclosing PHP debug range to another section.
#[test]
fn test_cli_debug_info_preserves_sections_around_eval_invokers() {
    let dir = make_cli_test_dir("elephc_debug_eval_invoker_ranges");
    let php_path = dir.join("main.php");
    fs::write(&php_path, r#"<?php
function debugEvalValue(int $value): int { return $value + 1; }
function debugEvalDispatch(string $source): void { eval($source); }
class DebugEvalConsumer {
    public function apply(callable $callback): int { return $callback(4); }
}
$source = 'echo debugEvalValue(2), ":", (new DebugEvalConsumer())->apply("debugEvalValue"); // ' . $argc;
debugEvalDispatch($source);
"#).unwrap();
    let output = elephc_cli_command(&dir).arg("--debug-info").arg(&php_path).output().unwrap();
    assert!(output.status.success(), "debug eval compilation failed: {}", String::from_utf8_lossy(&output.stderr));
    let run = Command::new(dir.join("main")).output().unwrap();
    assert!(run.status.success(), "debug eval binary failed: {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3:5");
    let _ = fs::remove_dir_all(&dir);
}

/// Lists the symbol names `nm` reports from a linked binary's symbol table.
/// A fully stripped executable yields an empty list: `nm` either prints
/// nothing, reports "no symbols", or exits non-zero depending on the platform,
/// and all three shapes collapse to "no names" here.
fn symbol_table_names(binary: &Path) -> Vec<String> {
    let output = Command::new("nm")
        .arg(binary)
        .output()
        .expect("failed to run nm on the compiled binary");
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.contains("no symbols"))
        .filter_map(|line| line.split_whitespace().last().map(str::to_string))
        .collect()
}

/// Verifies linked executables are stripped of their symbol table by default
/// and that `--keep-symbols` retains it (runtime helper names become visible).
#[test]
fn test_cli_executables_strip_symbols_by_default_and_keep_symbols_retains_them() {
    let dir = make_cli_test_dir("elephc_cli_strip");
    let php_path = dir.join("main.php");
    fs::write(&php_path, "<?php echo 1 + 2;").unwrap();

    let stripped_build = elephc_cli_command(&dir)
        .arg(&php_path)
        .output()
        .expect("failed to run elephc for the default (stripped) build");
    assert!(
        stripped_build.status.success(),
        "default build failed: {}",
        String::from_utf8_lossy(&stripped_build.stderr)
    );
    let stripped_names = symbol_table_names(&dir.join("main"));
    assert!(
        !stripped_names.iter().any(|name| name.contains("__rt_")),
        "default build must not keep runtime helper names in its symbol table: {:?}",
        stripped_names
    );

    let kept_build = elephc_cli_command(&dir)
        .arg("--keep-symbols")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc --keep-symbols");
    assert!(
        kept_build.status.success(),
        "--keep-symbols build failed: {}",
        String::from_utf8_lossy(&kept_build.stderr)
    );
    let kept_names = symbol_table_names(&dir.join("main"));
    assert!(
        kept_names.iter().any(|name| name.contains("__rt_")),
        "--keep-symbols must retain runtime helper names in the symbol table"
    );
    assert!(
        kept_names.len() > stripped_names.len(),
        "--keep-symbols must keep strictly more symbols ({}) than the stripped default ({})",
        kept_names.len(),
        stripped_names.len()
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Returns the `incl` percentage the exact profile reports for `name`.
///
/// The table prints one row per function as `<name> <bar> incl <n>%  self …`.
/// Parsing the number rather than matching a literal keeps the assertion about
/// the attribution instead of about the formatting.
#[cfg(test)]
fn exact_incl_percent(report: &str, name: &str) -> f64 {
    for line in report.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() != Some(name) {
            continue;
        }
        let Some(index) = line.find("incl") else {
            continue;
        };
        let after = &line[index + 4..];
        let Some(percent) = after.split('%').next() else {
            continue;
        };
        if let Ok(value) = percent.trim().parse::<f64>() {
            return value;
        }
    }
    panic!("the profile has no row for `{name}`:\n{report}");
}

/// A suspended coroutine is not charged for what its consumer does.
///
/// The only test on this branch that runs with the profiler ACTIVE. Every other
/// check of the attribution is either a unit test driving `State` directly — which
/// cannot see the emitted hooks — or the dormant-output test above, which asserts
/// the profiler stays quiet and so is blind to what it says when it speaks.
///
/// `yield` and `Fiber::suspend` switch stacks rather than returning, so the body's
/// frame used to stay open across everything the consumer did next: it became the
/// caller of that work and the owner of its cost. Measured before the fix, a
/// generator body that ran 23 µs reported **99.8%** of the program's inclusive
/// time, and the call graph carried an edge from it to a function the loop called
/// and the generator never did. A delegating generator reported 52.6% for the same
/// reason on the `yield from` half.
///
/// The thresholds are deliberately loose. The gap being asserted is three orders
/// of magnitude — a coroutine that runs microseconds against a consumer that runs
/// milliseconds — so anything under 10% passes and every shape of the defect
/// lands far above it.
///
/// `heavy` runs 1,200,000 rounds per call so its work keeps dominating the fixed
/// drain and dispatch overhead: at 120,000 rounds a faster loop left it at about
/// 4 ms in total, and the consumer's own share fell to 85-89.5% on loaded CI
/// runners, failing the >90% control while the coroutines stayed far below 10%.
#[test]
fn test_cli_monitor_does_not_charge_a_coroutine_for_its_consumer() {
    let dir = make_cli_test_dir("elephc_cli_monitor_coroutine");
    fs::write(
        dir.join("coro.php"),
        "<?php\n\
         function heavy(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n += $i; } return $n; }\n\
         function inner(): iterable { yield 1; yield 2; }\n\
         function outer(): iterable { yield 0; yield from inner(); }\n\
         function body(): int { Fiber::suspend(1); return 7; }\n\
         function drain(): int {\n\
         $t = 0;\n\
         foreach (outer() as $v) { $t += heavy(1200000); }\n\
         $f = new Fiber('body');\n\
         $f->start();\n\
         $t += heavy(1200000);\n\
         $f->resume(0);\n\
         return $t + $f->getReturn();\n\
         }\n\
         echo drain();\n",
    )
    .expect("failed to write the coroutine fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "coro.php"])
        .output()
        .expect("failed to compile the coroutine fixture");
    assert!(
        compile.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./coro", "--dot", "coro.dot"])
        .output()
        .expect("failed to run elephc monitor");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&watched.stdout),
        String::from_utf8_lossy(&watched.stderr)
    );

    assert!(
        exact_incl_percent(&report, "heavy") > 90.0,
        "the consumer's own work should dominate:\n{report}"
    );
    for coroutine in ["outer", "inner", "body"] {
        let share = exact_incl_percent(&report, coroutine);
        assert!(
            share < 10.0,
            "`{coroutine}` was charged {share}% of the program while suspended:\n{report}"
        );
    }

    // The edge is the other half: a suspended coroutine that keeps its frame is
    // read as the CALLER of whatever the consumer does next, so the graph gains
    // an edge that does not exist. `drain` is what calls `heavy`.
    let graph = fs::read_to_string(dir.join("coro.dot")).expect("monitor wrote no graph");
    let node_of = |name: &str| -> Option<String> {
        graph.lines().find_map(|line| {
            let (id, rest) = line.trim().split_once(" [label=\"")?;
            rest.starts_with(name).then(|| id.to_string())
        })
    };
    let heavy = node_of("heavy").expect("the graph has no `heavy` node");
    for coroutine in ["outer", "inner", "body"] {
        if let Some(node) = node_of(coroutine) {
            assert!(
                !graph.contains(&format!("{node} -> {heavy}")),
                "the graph says `{coroutine}` calls `heavy`, which it never does:\n{graph}"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

/// A coroutine resumed with a pending exception gets its frame back before its
/// own handler runs.
///
/// `__rt_fiber_suspend` does not always return to the suspension site. Three
/// paths leave without returning — `Fiber::suspend()` outside a fiber, a live
/// `unserialize()`, and a `Fiber::throw()`/`Generator::throw()` delivered on
/// resume — and all three reach PHP handlers. The second half of the bracket at
/// the suspension site is skipped, so the activation stayed parked while its own
/// `catch` ran.
///
/// The edge is what says it: `body`'s handler calls `heavy`, so `body → heavy`
/// is the true edge. With the activation still parked, `heavy` entered on a
/// stack whose top was the CONSUMER, and the graph read `drive → heavy` — a call
/// `drive` never makes. Asserting an edge that must EXIST, where the sibling
/// test asserts ones that must not.
///
/// The third path is the one real async code hits; the first two are error
/// paths that reach the same helper and are covered by the same hook.
#[test]
fn test_cli_monitor_restores_a_coroutine_resumed_into_a_throw() {
    let dir = make_cli_test_dir("elephc_cli_monitor_fiber_throw");
    fs::write(
        dir.join("ft.php"),
        "<?php\n\
         function heavy(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n += $i; } return $n; }\n\
         function body(): int {\n\
         $t = 0;\n\
         try { Fiber::suspend(1); } catch (RuntimeException $e) { $t += heavy(300000); }\n\
         return $t;\n\
         }\n\
         function drive(): int {\n\
         $f = new Fiber('body');\n\
         $f->start();\n\
         $f->throw(new RuntimeException('x'));\n\
         return $f->getReturn();\n\
         }\n\
         echo drive();\n",
    )
    .expect("failed to write the fiber-throw fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "ft.php"])
        .output()
        .expect("failed to compile the fiber-throw fixture");
    assert!(
        compile.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./ft", "--dot", "ft.dot"])
        .output()
        .expect("failed to run elephc monitor");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&watched.stdout),
        String::from_utf8_lossy(&watched.stderr)
    );
    assert!(
        report.contains("44999850000"),
        "the program's own answer changed, so the hooks are not transparent:\n{report}"
    );

    let graph = fs::read_to_string(dir.join("ft.dot")).expect("monitor wrote no graph");
    let node_of = |name: &str| -> String {
        graph
            .lines()
            .find_map(|line| {
                let (id, rest) = line.trim().split_once(" [label=\"")?;
                rest.starts_with(name).then(|| id.to_string())
            })
            .unwrap_or_else(|| panic!("the graph has no `{name}` node:\n{graph}"))
    };
    let (body, drive, heavy) = (node_of("body"), node_of("drive"), node_of("heavy"));
    assert!(
        graph.contains(&format!("{body} -> {heavy}")),
        "the handler's work was not attributed to the coroutine that ran it:\n{graph}"
    );
    assert!(
        !graph.contains(&format!("{drive} -> {heavy}")),
        "the graph says `drive` calls `heavy`, which only holds while `body` is \
         still parked:\n{graph}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--live` works on every platform, because it asks the target it launched.
///
/// Deliberately NOT gated on macOS, which is the whole point of the test. The
/// live view used to read its own child from the outside, through a tool that
/// ships on macOS alone, and was refused everywhere else — for a program this
/// process had started ITSELF and could simply have asked. It now hands the
/// child a socketpair, exactly as the exact path always has, and the probe
/// answers snapshots on it.
///
/// The fixture runs to a wall-clock DEADLINE rather than an iteration count. A
/// count is a bet on the machine: calibrated here it gave five windows, and on a
/// faster CI runner the program finished inside the second one, so the loop saw
/// the child exit before it could redraw and the test failed for a reason that
/// had nothing to do with what it measures. Six seconds against a one-second
/// window leaves room on any machine, in both directions.
///
/// Three things are asserted, and the third is the one a reader would not think
/// of. Windows advance, so the loop is really looping. A PHP function is named,
/// so the answer carries symbolised frames rather than an empty table anyone
/// could produce. And the program's own output survives while the profiler's
/// does not: asking wakes the EXACT profiler too, which writes its table to
/// stderr at exit, and forwarding that would print a raw dump under the live
/// view as if the program had written it.
#[test]
fn test_cli_monitor_live_needs_no_external_sampler() {
    let dir = make_cli_test_dir("elephc_cli_monitor_live");
    fs::write(
        dir.join("hot.php"),
        "<?php\n\
         function spin(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n = ($n + $i) % 1000003; } return $n; }\n\
         function descend(int $depth, int $rounds): int {\n\
         if ($depth <= 0) { return spin($rounds); }\n\
         return descend($depth - 1, $rounds) % 1000003;\n\
         }\n\
         $t = 0;\n\
         $deadline = microtime(true) + 6.0;\n\
         while (microtime(true) < $deadline) { $t = ($t + descend(6, 400000)) % 1000003; }\n\
         echo 'done', $t;\n",
    )
    .expect("failed to write the live fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--with-monitoring", "hot.php"])
        .output()
        .expect("failed to compile the live fixture");
    assert!(
        compile.status.success(),
        "compile failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    let watched = elephc_cli_command(&dir)
        .args(["monitor", "./hot", "--live", "--duration", "1"])
        .output()
        .expect("failed to run elephc monitor --live");
    let stdout = String::from_utf8_lossy(&watched.stdout).to_string();
    let stderr = String::from_utf8_lossy(&watched.stderr).to_string();
    let report = format!("{stdout}{stderr}");

    assert!(
        report.contains("window 2"),
        "the live loop produced no second window:\n{report}"
    );
    assert!(
        report.contains("descend"),
        "the live table named no PHP function, so the frames were not symbolised:\n{report}"
    );
    assert!(
        report.contains("done"),
        "the program's own output was swallowed:\n{report}"
    );
    for leaked in ["elephc-probe:", "elephc-instr:"] {
        assert!(
            !report.contains(leaked),
            "raw profiler output reached the operator under the live view ({leaked}):\n{report}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

/// `--live` on a `.php` SOURCE, which is the way a user actually reaches it.
///
/// The live view asks the program it launched, so the program has to carry the
/// probe — and the live path compiled the source with `--debug-info` instead of
/// `--with-monitoring`, producing a binary with nothing listening. `monitor`
/// then opened a channel, waited out every window for an ACK that could not
/// come, and reported an empty table for a program running perfectly well.
///
/// Nothing caught it because the test above compiles the fixture by hand first
/// and monitors the BINARY. That is a real path, but it is not the one the
/// documentation puts first, and between them they left the common case
/// uncovered. This runs `monitor hot.php --live` and nothing else.
#[test]
fn test_cli_monitor_live_compiles_the_source_with_the_probe() {
    let dir = make_cli_test_dir("elephc_cli_monitor_live_source");
    fs::write(
        dir.join("hot.php"),
        "<?php\n\
         function spin(int $rounds): int { $n = 0; for ($i = 0; $i < $rounds; $i++) { $n = ($n + $i) % 1000003; } return $n; }\n\
         function descend(int $depth, int $rounds): int {\n\
         if ($depth <= 0) { return spin($rounds); }\n\
         return descend($depth - 1, $rounds) % 1000003;\n\
         }\n\
         $t = 0;\n\
         $deadline = microtime(true) + 6.0;\n\
         while (microtime(true) < $deadline) { $t = ($t + descend(6, 400000)) % 1000003; }\n\
         echo 'done', $t;\n",
    )
    .expect("failed to write the live source fixture");

    // No `--with-monitoring` step. That is the point of the test.
    let watched = elephc_cli_command(&dir)
        .args(["monitor", "hot.php", "--live", "--duration", "1"])
        .output()
        .expect("failed to run elephc monitor hot.php --live");
    let stdout = String::from_utf8_lossy(&watched.stdout).to_string();
    let stderr = String::from_utf8_lossy(&watched.stderr).to_string();
    let report = format!("{stdout}{stderr}");

    assert!(
        report.contains("descend"),
        "the live table named no PHP function, so the source was compiled without the probe \
         and the child had nothing to answer with:\n{report}"
    );
    assert!(
        !report.contains("did not answer within the window"),
        "the child never answered, which is what a binary compiled without the probe does:\n{report}"
    );
    assert!(
        report.contains("done"),
        "the program's own output was swallowed:\n{report}"
    );

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


/// Verifies a bridge-archive override that points somewhere without the archive is reported as
/// an override, through the real `archive_path` routing rather than the validator alone.
///
/// `ELEPHC_<NAME>_LIB_DIR` REPLACES the search instead of joining it: resolution returns on a
/// non-empty override before a single fallback is read. The failure used to render the generic
/// message anyway, which listed four directories it had not consulted and then recommended
/// keeping the archives next to the elephc binary — a location this run will not look at while
/// the variable stays set, so someone who has already done that follows the advice and stays
/// broken (issue #517).
///
/// This goes through the CLI on purpose. A unit test that calls the validator directly would
/// still pass if `archive_path` went back to the generic error, because the routing is the part
/// that decides which message a user sees. The override reaches the compiler as a CHILD process
/// environment variable, so it cannot race other tests in the same process.
#[test]
fn test_cli_bridge_override_miss_reports_the_override_not_the_fallbacks() {
    let dir = make_cli_test_dir("elephc_cli_bridge_override");
    let empty = dir.join("no-archives-here");
    fs::create_dir_all(&empty).expect("create the empty override directory");
    fs::write(dir.join("main.php"), "<?php\necho \"hi\\n\";\n")
        .expect("write the web fixture");

    let output = elephc_cli_command(&dir)
        .env("ELEPHC_WEB_LIB_DIR", &empty)
        .args(["-q", "--web", "main.php"])
        .output()
        .expect("failed to run elephc --web");

    assert!(
        !output.status.success(),
        "an override without the archive must fail the link"
    );
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let override_dir = empty.display().to_string();

    assert!(
        report.contains("needs: libelephc_web.a"),
        "the message must name the archive:\n{report}"
    );
    assert!(
        report.contains(&override_dir),
        "the message must name the directory it actually read:\n{report}"
    );
    assert!(
        report.contains("takes priority over every other location"),
        "the message must say the override is what stopped the search:\n{report}"
    );
    assert!(
        report.contains("unset ELEPHC_WEB_LIB_DIR"),
        "the message must offer unsetting the override as a way out:\n{report}"
    );
    assert!(
        !report.contains("keep the bridge archives next to the elephc binary"),
        "advice the override would ignore must not be offered:\n{report}"
    );
    assert!(
        !report.contains("target/debug"),
        "a fallback that was never consulted must not be reported as searched:\n{report}"
    );

    // The same build links once the override is gone, which is what makes that last sentence
    // actionable rather than decorative.
    let recovered = elephc_cli_command(&dir)
        .args(["-q", "--web", "main.php"])
        .output()
        .expect("failed to re-run elephc --web");
    assert!(
        recovered.status.success(),
        "the same build must succeed without the override:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&recovered.stdout),
        String::from_utf8_lossy(&recovered.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the `--debug` build preset compiles and runs a normal executable with
/// unchanged output, and that a library emit is refused before codegen.
#[test]
fn test_cli_debug_preset_compiles_and_runs() {
    let dir = make_cli_test_dir("elephc_cli_debug_preset");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\nfunction combine(int $a, int $b): int {\n    $sum = $a + $b;\n    if ($a > $b) { return $a - $b; }\n    return $sum;\n}\necho combine(2, 3), \"\\n\";\n",
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--debug")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc --debug");
    assert!(
        output.status.success(),
        "elephc --debug failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let run = run_binary(&dir.join("main"), &dir);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "5\n");

    let site_run = std::process::Command::new(dir.join("main"))
        .env("ELEPHC_DEBUG_SITES", "1")
        .output()
        .expect("failed to run debug site capture");
    assert!(
        site_run.status.success(),
        "debug site capture failed: {}",
        String::from_utf8_lossy(&site_run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&site_run.stdout), "5\n");
    let site_stderr = String::from_utf8_lossy(&site_run.stderr);
    assert!(
        site_stderr.contains("elephc-debug-sites: total="),
        "missing site summary: {site_stderr}"
    );
    assert!(
        site_stderr.lines().any(|line| line.starts_with("elephc-debug-site: ")),
        "missing per-site counters: {site_stderr}"
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("debug assembly");
    assert!(
        asm.contains("elephc_debug_metadata:"),
        "embedded metadata symbol missing from assembly"
    );
    let source_markers = asm.lines().filter(|line| line.contains("@src ")).count();
    let guard_markers = asm
        .lines()
        .filter(|line| line.contains("@src ") && line.contains(" site="))
        .count();
    assert!(
        guard_markers < source_markers,
        "guards were not deduplicated: {guard_markers} guards for {source_markers} instruction markers"
    );
    let metadata_path = dir.join("main.edbg.json");
    let metadata = fs::read_to_string(&metadata_path).expect("debug metadata sidecar");
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata).expect("valid debug metadata JSON");
    assert_eq!(metadata["format"], "elephc-debug-metadata");
    assert_eq!(metadata["version"], 1);
    assert!(
        metadata["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| {
                source["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("main.php"))
            }),
        "missing entry source: {metadata}"
    );
    assert!(
        metadata["functions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|function| function["name"] == "combine"),
        "missing combine function: {metadata}"
    );
    assert_eq!(
        metadata["sites"].as_array().unwrap().len(),
        guard_markers,
        "metadata sites must match the deduplicated guard markers"
    );
    let mut metadata_site_ids: std::collections::HashSet<u64> = metadata["sites"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|site| site["id"].as_u64())
        .collect();
    // Branch-edge guards share the site counter with source sites but are
    // described by the `branches` table instead of `sites`.
    if let Some(branches) = metadata["branches"].as_array() {
        for branch in branches {
            if let Some(id) = branch["then_site"].as_u64() {
                metadata_site_ids.insert(id);
            }
            if let Some(id) = branch["else_site"].as_u64() {
                metadata_site_ids.insert(id);
            }
        }
    }
    for line in site_stderr
        .lines()
        .filter_map(|line| line.strip_prefix("elephc-debug-site: "))
        .filter_map(|tail| tail.split_once(' '))
        .filter_map(|(id, _)| id.parse::<u64>().ok())
    {
        assert!(
            metadata_site_ids.contains(&line),
            "debug site {line} missing from metadata: {metadata_site_ids:?}"
        );
    }
    let local_names: Vec<&str> = metadata["locals"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|local| local["name"].as_str())
        .collect();
    for expected in ["a", "b", "sum"] {
        assert!(
            local_names.contains(&expected),
            "missing local {expected}: {local_names:?}"
        );
    }

    let library = elephc_cli_command(&dir)
        .args(["--debug", "--emit", "cdylib"])
        .arg(&php_path)
        .output()
        .expect("failed to run elephc --debug --emit cdylib");
    assert!(
        !library.status.success(),
        "--debug must refuse a library emit"
    );
    assert!(
        String::from_utf8_lossy(&library.stderr).contains("executable builds only"),
        "missing library refusal: {}",
        String::from_utf8_lossy(&library.stderr)
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `elephc debug` builds a source and exposes the minimal interactive
/// session: a breakpoint stop, current-frame locals, continue, unchanged output.
#[test]
fn test_cli_debug_command_pauses_inspects_and_continues() {
    let dir = make_cli_test_dir("elephc_cli_debug_command");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\nfunction combine(int $a, int $b): int {\n    $sum = $a + $b;\n    if ($a > $b) { return $a - $b; }\n    return $sum;\n}\necho combine(2, 3), \"\\n\";\n",
    )
    .unwrap();

    use std::io::Write as _;

    let mut child = elephc_cli_command(&dir)
        .args(["debug", "main.php", "--break", "main.php:4"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn elephc debug");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"bt\nframe 1\nframe 0\nlocals\nn\nbt\nc\n")
        .expect("write debug commands");
    let output = child.wait_with_output().expect("wait for elephc debug");
    assert!(
        output.status.success(),
        "elephc debug failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.ends_with("5\n"), "missing program output: {stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Stopped at main.php:4:"), "{stderr}");
    assert!(stderr.contains("#0 combine at main.php:4"), "{stderr}");
    assert!(stderr.contains("#1 main at main.php:7"), "{stderr}");
    assert!(stderr.contains("Selected frame #1"), "{stderr}");
    assert!(stderr.contains("a int(2)"), "{stderr}");
    assert!(stderr.contains("b int(3)"), "{stderr}");
    assert!(stderr.contains("Stopped at main.php:5:"), "{stderr}");
    assert!(stderr.contains("#0 combine at main.php:5"), "{stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `step` enters a callee and `finish` returns to the caller, with the
/// activation stack tracking both frames.
#[test]
fn test_cli_debug_step_and_finish_follow_calls() {
    let dir = make_cli_test_dir("elephc_cli_debug_stepping");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        "<?php\nfunction add(int $a, int $b): int {\n    $sum = $a + $b;\n    return $sum;\n}\nfunction compute(): int {\n    $x = add(2, 3);\n    $y = $x * 2;\n    return $y;\n}\necho compute(), \"\\n\";\n",
    )
    .unwrap();

    use std::io::Write as _;
    let mut child = elephc_cli_command(&dir)
        .args(["debug", "main.php", "--break", "main.php:7"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn elephc debug");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"s\nbt\nf\nbt\nc\n")
        .expect("write stepping commands");
    let output = child.wait_with_output().expect("wait for elephc debug");
    assert!(
        output.status.success(),
        "elephc debug failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).ends_with("10\n"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Stopped at main.php:7:"), "{stderr}");
    assert!(stderr.contains("Stopped at main.php:3:"), "{stderr}");
    assert!(stderr.contains("#0 add at main.php:3"), "{stderr}");
    assert!(stderr.contains("#1 compute at main.php:7"), "{stderr}");
    assert!(stderr.contains("#2 main at main.php:11"), "{stderr}");
    assert!(stderr.contains("Stopped at main.php:8:"), "{stderr}");
    assert!(stderr.contains("#0 compute at main.php:8"), "{stderr}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--debug-info --source-map` keeps an included file as its own source:
/// the assembly gets a `.file` directive for it, and the map's `sources`/mappings
/// carry its source id instead of the entry file's.
#[test]
fn test_cli_debug_info_and_source_map_keep_included_source_identity() {
    let dir = make_cli_test_dir("elephc_cli_included_source_identity");
    fs::write(
        dir.join("main.php"),
        "<?php\nrequire __DIR__ . '/lib.php';\necho add(2, 3), \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("lib.php"),
        "<?php\nfunction add(int $a, int $b): int {\n    return $a + $b;\n}\n",
    )
    .unwrap();

    let output = elephc_cli_command(&dir)
        .arg("--debug-info")
        .arg("--emit-asm")
        .arg("--source-map")
        .arg(dir.join("main.php"))
        .output()
        .expect("failed to run elephc CLI with --debug-info --source-map");
    assert!(
        output.status.success(),
        "elephc --debug-info --source-map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read assembly");
    let lib_file = asm
        .lines()
        .find(|line| line.contains(".file ") && line.contains("lib.php"))
        .unwrap_or_else(|| panic!("no .file directive for lib.php: {asm}"));
    let lib_file_number = lib_file
        .split_whitespace()
        .nth(1)
        .expect("file number in .file directive");
    assert!(
        asm.contains(&format!(".loc {lib_file_number} 3 ")),
        "no .loc for lib.php line 3: {asm}"
    );

    let raw = fs::read_to_string(dir.join("main.map")).expect("failed to read source map");
    let map: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    let sources = map["sources"].as_array().expect("sources array");
    let lib = sources
        .iter()
        .find(|source| source["path"].as_str().is_some_and(|path| path.ends_with("lib.php")))
        .expect("included lib.php source entry");
    let lib_id = lib["id"].as_u64().expect("included source id");
    assert_ne!(lib_id, 0, "included source must not reuse the entry id");
    assert!(
        map["mappings"].as_array().unwrap().iter().any(|mapping| {
            mapping["source_id"].as_u64() == Some(lib_id)
                && mapping["php_line"].as_u64() == Some(3)
        }),
        "no mapping attributed to lib.php: {raw}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// A line past column 65,535 in an INCLUDED file compiles, and `--source-map` gives its tokens the
/// same exact positions as the same line in the root file.
///
/// The included file's end columns are packed into 16 bits: past that the compile aborted
/// ("included-source column exceeds packed span range", #1292), and a saturated end made every
/// token starting past column 65535 lose its end in the map (`php_end_col: null`, #1306).
#[test]
fn test_cli_source_map_keeps_long_included_line_ends() {
    // `strlen($x)` sits after column 70,000 on line 2.
    let body = format!("<?php\n$x = '{}'; echo strlen($x), \"\\n\";\n", "a".repeat(70_000));
    let map_of = |label: &str, files: &[(&str, &str)]| -> Vec<serde_json::Value> {
        let dir = make_cli_test_dir(label);
        for (name, source) in files {
            fs::write(dir.join(name), source).unwrap();
        }
        let output = elephc_cli_command(&dir)
            .arg("--emit-asm")
            .arg("--source-map")
            .arg(dir.join("main.php"))
            .output()
            .expect("failed to run elephc CLI with --source-map");
        assert!(
            output.status.success(),
            "{label}: elephc --source-map failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = fs::read_to_string(dir.join("main.map")).expect("failed to read source map");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        let mut wide: Vec<serde_json::Value> = Vec::new();
        let mut collect = |value: &serde_json::Value| {
            if value.get("php_line").and_then(|v| v.as_u64()) == Some(2)
                && value.get("php_col").and_then(|v| v.as_u64()).is_some_and(|col| col > 65_535)
            {
                let mut entry = value.clone();
                if let Some(object) = entry.as_object_mut() {
                    object.remove("asm_line");
                    object.remove("source_id");
                }
                wide.push(entry);
            }
        };
        /// Visits `value`, then every array item and object field nested inside it, depth first.
        fn walk(value: &serde_json::Value, visit: &mut dyn FnMut(&serde_json::Value)) {
            visit(value);
            match value {
                serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, visit)),
                serde_json::Value::Object(fields) => fields.values().for_each(|item| walk(item, visit)),
                _ => {}
            }
        }
        walk(&doc, &mut collect);
        let _ = fs::remove_dir_all(&dir);
        wide
    };

    let included = map_of(
        "elephc_cli_source_map_long_include",
        &[("long.php", body.as_str()), ("main.php", "<?php\ninclude __DIR__ . '/long.php';\n")],
    );
    let root = map_of("elephc_cli_source_map_long_root", &[("main.php", body.as_str())]);

    assert!(!root.is_empty(), "the root map has no entry past column 65,535");
    // `strlen($x)` starts at column 70,015: with a saturated end it mapped as a point span.
    // (The `"\n"` literal maps as a point span in BOTH files, so it is not the oracle.)
    let strlen = included
        .iter()
        .find(|entry| entry["op"] == "str_len")
        .unwrap_or_else(|| panic!("no str_len entry past column 65,535: {included:#?}"));
    assert_eq!(strlen["php_col"], 70_015);
    assert_eq!(strlen["php_end_col"], 70_025, "the included strlen() lost its end: {strlen}");
    assert_eq!(included, root, "the included line maps differently from the same root line");
}

/// Verifies a conditional breakpoint is armed after syntax validation but only
/// stops when its expression becomes true in the hit frame; false loop iterations
/// continue, and a malformed condition is refused.
#[test]
fn test_cli_debug_dap_conditional_breakpoint_stops_only_when_true() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_condition");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction sum(): int {\n    $total = 0;\n    for ($i = 0; $i < 5; $i++) {\n        $total += $i;\n    }\n    return $total;\n}\necho sum(), \"\\n\";\n",
    )
    .expect("write conditional breakpoint fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);

    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 5, "condition": "$i ==="}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], false, "{response}");
    assert!(
        response["body"]["breakpoints"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("expected")),
        "{response}"
    );

    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 5, "condition": "$i === 3"}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], true, "{response}");
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:5")),
        "{stopped}"
    );

    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("conditional stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["sum", "main"], "{response}");
    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("conditional frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("conditional variables");
    let find = |name: &str| {
        variables
            .iter()
            .find(|variable| variable["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };
    assert_eq!(find("$i")["value"], "int(3)");
    assert_eq!(find("$total")["value"], "int(3)");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        assert_ne!(
            message["event"], "stopped",
            "condition stopped again after becoming false: {message}"
        );
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for conditional adapter");
    assert!(status.success(), "conditional adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("10"),
        "conditional program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies exception filters stop on a caught throw and on the later uncaught
/// throw, with the throwable class surfaced in the stopped event.
#[test]
fn test_cli_debug_dap_exception_filters_stop_caught_and_uncaught() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_exception_filters");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction caught_exception(): void {\n    try {\n        throw new RuntimeException(\"caught\");\n    } catch (RuntimeException $e) {\n        echo \"caught\\n\";\n    }\n}\ncaught_exception();\nthrow new RuntimeException(\"uncaught\");\n",
    )
    .expect("write exception filter fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["success"], true);
    let filters = response["body"]["exceptionBreakpointFilters"]
        .as_array()
        .expect("exceptionBreakpointFilters");
    assert!(
        filters.iter().any(|filter| filter["filter"] == "caught"),
        "{response}"
    );
    assert!(
        filters.iter().any(|filter| filter["filter"] == "uncaught"),
        "{response}"
    );
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setExceptionBreakpoints",
        Some(serde_json::json!({"filters": ["caught", "uncaught"]})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);

    let caught = client.event("stopped");
    assert_eq!(caught["body"]["reason"], "exception", "{caught}");
    assert_eq!(caught["body"]["text"], "RuntimeException", "{caught}");
    assert!(
        caught["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("caught")),
        "{caught}"
    );
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);

    let uncaught = client.event("stopped");
    assert_eq!(uncaught["body"]["reason"], "exception", "{uncaught}");
    assert_eq!(uncaught["body"]["text"], "RuntimeException", "{uncaught}");
    assert!(
        uncaught["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("uncaught")),
        "{uncaught}"
    );
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);

    // The uncaught report exits with PHP's 255 status; the DAP transport closes
    // before a `terminated` event can be emitted. Drain any output events that
    // were queued before the close.
    client.drain_until_closed();
    let status = client.child.wait().expect("wait for exception adapter");
    assert_eq!(status.code(), Some(255), "uncaught exception exit status");
    let output = client.program_output();
    assert!(output.contains("caught"), "caught handler output missing: {output}");
    let mut report = String::new();
    {
        use std::io::Read as _;
        client
            .stderr
            .read_to_string(&mut report)
            .expect("read exception adapter stderr");
    }
    assert!(
        report.contains("RuntimeException"),
        "uncaught Throwable report missing: {report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a throw selected by the `throw` exception filter stops inside a
/// destructor with its class and throwing frame, before the uncaught report.
#[test]
fn test_cli_debug_dap_exception_stops_in_destructor() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_destructor_exception");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass Boom {\n    public function __destruct() {\n        throw new RuntimeException(\"dtor\", 9);\n    }\n}\nfunction run(): void {\n    $boom = new Boom();\n    unset($boom);\n    echo \"after\\n\";\n}\nrun();\n",
    )
    .expect("write destructor exception fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setExceptionBreakpoints",
        Some(serde_json::json!({"filters": ["uncaught"]})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);

    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "exception", "{stopped}");
    assert_eq!(stopped["body"]["text"], "RuntimeException", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("uncaught")),
        "{stopped}"
    );

    let seq = client.send(
        "stackTrace",
        Some(serde_json::json!({"threadId": stopped["body"]["threadId"]})),
    );
    let stack = client.response(seq);
    let names: Vec<&str> = stack["body"]["stackFrames"]
        .as_array()
        .expect("destructor stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert!(
        names.iter().any(|name| name.contains("__destruct")),
        "destructor frame missing: {stack}"
    );

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    client.drain_until_closed();
    let status = client.child.wait().expect("wait for destructor adapter");
    assert_eq!(status.code(), Some(255), "uncaught destructor exit status");
    let output = client.program_output();
    let mut report = String::new();
    {
        use std::io::Read as _;
        client
            .stderr
            .read_to_string(&mut report)
            .expect("read destructor adapter stderr");
    }
    assert!(
        report.contains("RuntimeException") && report.contains("dtor"),
        "uncaught destructor report missing: {report}"
    );
    assert!(
        !output.contains("after") && !report.contains("after"),
        "statements after the uncaught destructor must not run: {output}{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `dataBreakpointInfo` + `setDataBreakpoints` watch a frame local and
/// stop at the next site after its value changes, with malformed dataIds refused.
#[test]
fn test_cli_debug_dap_data_breakpoint_stops_on_write() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_data_breakpoint");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction countdown(): int {\n    $x = 0;\n    $x = 1;\n    $x = 2;\n    $x = 3;\n    return $x;\n}\necho countdown(), \"\\n\";\n",
    )
    .expect("write data breakpoint fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["success"], true);
    assert_eq!(response["body"]["supportsDataBreakpoints"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 4}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("data frame reference");
    let seq = client.send(
        "dataBreakpointInfo",
        Some(serde_json::json!({"variablesReference": frame_ref, "name": "$x"})),
    );
    let response = client.response(seq);
    let data_id = response["body"]["dataId"]
        .as_str()
        .unwrap_or_else(|| panic!("no dataId: {response}"))
        .to_string();
    assert!(
        response["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("x")),
        "{response}"
    );
    assert_eq!(response["body"]["accessTypes"][0], "write", "{response}");
    assert_eq!(response["body"]["canPersist"], false);

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({"breakpoints": [{"dataId": "bogus"}]})),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], false, "{response}");

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({
            "breakpoints": [{"dataId": data_id, "accessType": "write"}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], true, "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "data breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:5")),
        "{stopped}"
    );
    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("data hit frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("data hit variables");
    let x = variables
        .iter()
        .find(|variable| variable["name"] == "$x")
        .unwrap_or_else(|| panic!("missing $x: {response}"));
    assert_eq!(x["value"], "int(1)");

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({"breakpoints": []})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for data adapter");
    assert!(status.success(), "data adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("3"),
        "data program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a virtual property is shown as a read-only marker, a backed getter
/// from its raw backing slot with a caveat, and a virtual property is refused as
/// a data-breakpoint target instead of executing its getter.
#[test]
fn test_cli_debug_dap_property_hooks_and_virtual_properties() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_property_hooks");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass Temperature {\n    public float $celsius = 0.0;\n    public float $fahrenheit {\n        get => $this->celsius * 9 / 5 + 32;\n        set { $this->celsius = ($value - 32) * 5 / 9; }\n    }\n    public int $doubled {\n        get => $this->doubled * 2;\n        set => intdiv($value, 2);\n    }\n    public int $writeOnly {\n        set { $this->celsius = $value; }\n    }\n}\nfunction inspect(): void {\n    $t = new Temperature();\n    $t->celsius = 10.0;\n    $t->doubled = 8;\n    echo $t->fahrenheit, \" \", $t->doubled, \"\\n\";\n}\ninspect();\n",
    )
    .expect("write property hooks fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 20}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("property hook frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("property hook frame variables");
    let t = variables
        .iter()
        .find(|variable| variable["name"] == "$t")
        .unwrap_or_else(|| panic!("missing $t: {response}"));
    assert_eq!(t["value"], "object(Temperature) (4)", "{t}");
    let object_ref = t["variablesReference"]
        .as_i64()
        .expect("temperature variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": object_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("temperature properties");
    let find = |name: &str| {
        properties
            .iter()
            .find(|property| property["name"] == name)
            .unwrap_or_else(|| panic!("missing property {name}: {response}"))
    };

    let celsius = find("celsius");
    assert_eq!(celsius["value"], "float(10)", "{celsius}");
    assert_eq!(celsius["type"], "float", "{celsius}");

    let fahrenheit = find("fahrenheit");
    assert_eq!(
        fahrenheit["value"], "virtual [get/set hooks not executed]",
        "{fahrenheit}"
    );
    assert_eq!(fahrenheit["type"], "float", "{fahrenheit}");
    assert_eq!(fahrenheit["variablesReference"], 0, "{fahrenheit}");

    let doubled = find("doubled");
    assert_eq!(
        doubled["value"], "int(4) [backing value; get hook not executed]",
        "{doubled}"
    );
    assert_eq!(doubled["type"], "int", "{doubled}");
    assert_eq!(doubled["variablesReference"], 0, "{doubled}");

    let write_only = find("writeOnly");
    assert_eq!(
        write_only["value"], "virtual [write-only; set hook]",
        "{write_only}"
    );
    assert_eq!(write_only["type"], "int", "{write_only}");

    let seq = client.send(
        "dataBreakpointInfo",
        Some(serde_json::json!({
            "variablesReference": object_ref,
            "name": "fahrenheit",
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["dataId"], serde_json::Value::Null, "{response}");
    assert!(
        response["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("virtual property")
                && description.contains("No watchable backing slot")),
        "{response}"
    );
    let seq = client.send(
        "dataBreakpointInfo",
        Some(serde_json::json!({
            "variablesReference": object_ref,
            "name": "doubled",
        })),
    );
    let response = client.response(seq);
    assert!(
        response["body"]["dataId"].is_string(),
        "a backed hooked property must stay watchable: {response}"
    );

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client
        .child
        .wait()
        .expect("wait for property hook adapter");
    assert!(status.success(), "property hook adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("50 8"),
        "property hook program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies DAP object variables expose declared properties, dynamic properties
/// and a statically folded `__debugInfo()` projection, while a runtime-only
/// `__debugInfo()` is surfaced without executing user code.
#[test]
fn test_cli_debug_dap_dynamic_properties_and_debug_info() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_dynamic_properties");
    fs::write(
        dir.join("main.php"),
        "<?php\n#[\\AllowDynamicProperties]\nclass Bag {\n    public function __construct(public int $id) {}\n}\nclass Projected {\n    public function __construct(public int $x, public int $y) {}\n    public function __debugInfo() {\n        return ['identifier' => $this->x];\n    }\n}\nclass Computed {\n    public function __construct(public int $x) {}\n    public function __debugInfo() {\n        return ['n' => $this->x + 1];\n    }\n}\nfunction inspect(): void {\n    $bag = new Bag(7);\n    $bag->extra = \"dyn\";\n    $projected = new Projected(4, 9);\n    $computed = new Computed(5);\n    echo $bag->id, $projected->x, $computed->x, \"\\n\";\n}\ninspect();\n",
    )
    .expect("write dynamic property fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 23}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("dynamic frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("dynamic frame variables");
    let find = |name: &str| {
        variables
            .iter()
            .find(|variable| variable["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };

    let bag = find("$bag");
    assert_eq!(bag["value"], "object(Bag) (2)", "{bag}");
    let bag_ref = bag["variablesReference"]
        .as_i64()
        .expect("bag variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": bag_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("bag properties");
    let find_bag = |name: &str| {
        properties
            .iter()
            .find(|property| property["name"] == name)
            .unwrap_or_else(|| panic!("missing bag property {name}: {response}"))
    };
    assert_eq!(find_bag("id")["value"], "int(7)");
    assert_eq!(find_bag("extra")["value"], "string(3) \"dyn\"");

    let projected = find("$projected");
    assert_eq!(projected["value"], "object(Projected) (1)", "{projected}");
    let projected_ref = projected["variablesReference"]
        .as_i64()
        .expect("projected variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": projected_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("projected properties");
    let names: Vec<&str> = properties
        .iter()
        .filter_map(|property| property["name"].as_str())
        .collect();
    assert_eq!(names, ["identifier"], "{response}");
    assert_eq!(properties[0]["value"], "int(4)");
    // A projected child key still resolves to the declared property slot.
    let seq = client.send(
        "dataBreakpointInfo",
        Some(serde_json::json!({
            "variablesReference": projected_ref,
            "name": "identifier",
        })),
    );
    let response = client.response(seq);
    assert!(
        response["body"]["dataId"].is_string(),
        "projected property must be watchable: {response}"
    );

    let computed = find("$computed");
    assert_eq!(
        computed["value"], "object(Computed) (1) [__debugInfo() runtime]",
        "{computed}"
    );
    let computed_ref = computed["variablesReference"]
        .as_i64()
        .expect("computed variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": computed_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("computed properties");
    assert_eq!(properties[0]["name"], "x", "{response}");
    assert_eq!(properties[0]["value"], "int(5)");

    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for dynamic adapter");
    assert!(status.success(), "dynamic adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("745"),
        "dynamic program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the loopback TCP attach path: a wrong key is rejected without
/// stopping the listener, and the correct key then drives a complete DAP session.
#[test]
fn test_cli_debug_tcp_attach_rejects_wrong_key_then_accepts_correct_key() {
    let dir = make_cli_test_dir("elephc_cli_debug_tcp_attach");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction add(int $a, int $b): int {\n    return $a + $b;\n}\necho add(2, 3), \"\\n\";\n",
    )
    .expect("write TCP attach fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile TCP attach fixture");
    assert!(
        compile.status.success(),
        "TCP attach fixture failed to compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let key = dir.join("attach.key");
    let address_file = dir.join("attach.key.addr");
    let mut debuggee = DebuggeeGuard {
        child: std::process::Command::new(dir.join("main"))
            .env("ELEPHC_DEBUG_DAP", "1")
            .env("ELEPHC_DEBUG_ATTACH_ADDR", "127.0.0.1:0")
            .env("ELEPHC_DEBUG_ATTACH_KEY", &key)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn TCP attach debuggee"),
    };
    for _ in 0..400 {
        if address_file.exists() {
            break;
        }
        if let Some(status) = debuggee.child.try_wait().expect("poll TCP attach debuggee") {
            panic!("TCP attach debuggee exited before listening: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        address_file.exists(),
        "TCP attach address file was not created"
    );
    let address = fs::read_to_string(&address_file)
        .expect("read TCP attach address")
        .trim()
        .to_string();
    use std::os::unix::fs::PermissionsExt as _;
    let key_mode = fs::metadata(&key)
        .expect("attach key metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(key_mode, 0o600, "attach key must be owner-only");

    fs::write(dir.join("wrong.key"), "0".repeat(64)).expect("write wrong attach key");
    let wrong = elephc_cli_command(&dir)
        .args([
            "debug",
            "--attach-tcp",
            &address,
            "--key",
            "wrong.key",
        ])
        .output()
        .expect("run wrong-key TCP attach");
    assert!(!wrong.status.success(), "a wrong attach key must be rejected");
    let wrong_stderr = String::from_utf8_lossy(&wrong.stderr);
    assert!(
        wrong_stderr.contains("attach key rejected"),
        "wrong-key diagnostic missing: {wrong_stderr}"
    );
    eprintln!("tcp-attach: wrong key rejected: {}", wrong_stderr.trim());
    assert!(
        address_file.exists() && key.exists(),
        "a rejected client must not stop the TCP attach listener"
    );
    eprintln!("tcp-attach: listener still owns the key/address files");

    let mut client = DapClient::spawn(elephc_cli_command(&dir).args([
        "debug",
        "--attach-tcp",
        &address,
        "--key",
        "attach.key",
    ]));
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main", "stopOnEntry": true})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let entry = client.event("stopped");
    assert_eq!(entry["body"]["reason"], "entry", "{entry}");
    eprintln!(
        "tcp-attach: correct key stopped at {:?}",
        entry["body"]["description"]
    );
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    client.event("exited");
    client.event("terminated");
    let status = client.child.wait().expect("wait TCP attach adapter");
    assert!(status.success(), "TCP attach adapter failed");

    let output = client.program_output();
    assert!(
        output.contains("5"),
        "TCP attach program output missing: {output}"
    );
    eprintln!("tcp-attach: program output {:?}", output.trim());
    let status = debuggee.child.wait().expect("wait TCP attach debuggee");
    assert!(status.success(), "TCP attach debuggee failed");
    assert!(!key.exists(), "attach key must be removed after the session");
    assert!(
        !address_file.exists(),
        "attach address must be removed after the session"
    );
    eprintln!("tcp-attach: key/address/pid files removed after the session");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `elephc_break()` stops at the caller's source line in a DAP session
/// without a line breakpoint, and that the helper frame itself is transparent.
#[test]
fn test_cli_debug_dap_elephc_break_stops_at_call_site() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_elephc_break");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction hit(): void {\n    elephc_break();\n    echo \"after\\n\";\n}\nhit();\n",
    )
    .expect("write elephc_break fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:3:5 (hit)")),
        "{stopped}"
    );

    let seq = client.send(
        "stackTrace",
        Some(serde_json::json!({"threadId": stopped["body"]["threadId"]})),
    );
    let response = client.response(seq);
    let frames = response["body"]["stackFrames"]
        .as_array()
        .expect("elephc_break stack frames");
    assert_eq!(frames[0]["name"], "hit", "{response}");
    assert_eq!(frames[0]["line"], 3, "{response}");
    assert_eq!(frames[1]["name"], "main", "{response}");
    assert!(
        frames
            .iter()
            .all(|frame| frame["name"] != "elephc_break"),
        "the source helper frame must be transparent: {response}"
    );

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for elephc_break adapter");
    assert!(status.success(), "elephc_break adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("after"),
        "elephc_break program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `elephc_break()` is available as a no-op outside `--debug`, and that
/// `--strict-php` still hides the extension helper.
#[test]
fn test_cli_elephc_break_is_noop_without_debug_and_hidden_by_strict_php() {
    let dir = make_cli_test_dir("elephc_cli_elephc_break_noop");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction hit(): void {\n    elephc_break();\n    echo \"after\\n\";\n}\nhit();\n",
    )
    .expect("write elephc_break noop fixture");
    let compile = elephc_cli_command(&dir)
        .args(["main.php"])
        .output()
        .expect("compile elephc_break noop fixture");
    assert!(
        compile.status.success(),
        "normal build rejected elephc_break: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run elephc_break noop binary");
    assert!(run.status.success(), "elephc_break noop binary failed");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim_end(),
        "after",
        "elephc_break must be a no-op without a debugger"
    );

    let strict = elephc_cli_command(&dir)
        .args(["--strict-php", "main.php"])
        .output()
        .expect("strict-php compile of elephc_break fixture");
    assert!(!strict.status.success(), "strict PHP must hide elephc_break");
    assert!(
        String::from_utf8_lossy(&strict.stderr).contains("Undefined function: elephc_break"),
        "strict PHP diagnostic missing: {}",
        String::from_utf8_lossy(&strict.stderr)
    );

    fs::write(dir.join("bad.php"), "<?php\nelephc_break(1);\n")
        .expect("write elephc_break arity fixture");
    let bad = elephc_cli_command(&dir)
        .args(["bad.php"])
        .output()
        .expect("compile elephc_break arity fixture");
    assert!(!bad.status.success(), "elephc_break accepted an argument");
    assert!(
        String::from_utf8_lossy(&bad.stderr)
            .contains("Function 'elephc_break' expects 0 arguments, got 1"),
        "arity diagnostic missing: {}",
        String::from_utf8_lossy(&bad.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `elephc_dump()` rejects an invalid destination with ValueError before
/// the native recorder sees it.
#[test]
fn test_cli_debug_elephc_dump_rejects_invalid_destination() {
    let dir = make_cli_test_dir("elephc_cli_debug_dump_invalid_destination");
    fs::write(
        dir.join("main.php"),
        "<?php\nelephc_dump(\"x\", \"label\", \"bogus\");\n",
    )
    .expect("write elephc_dump destination fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile elephc_dump destination fixture");
    assert!(
        compile.status.success(),
        "elephc_dump destination fixture failed to compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run elephc_dump destination fixture");
    assert!(!run.status.success(), "invalid destination must throw");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(output.contains("ValueError"), "{output}");
    assert!(
        output.contains("Argument #3 ($to) must be 'auto', 'stderr', 'terminal' or 'html'"),
        "{output}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a safe `__debugInfo()` projection renders property reads and
/// compile-time literal scalars/arrays without executing the method, while a
/// computed body stays on the runtime marker plus declared properties.
#[test]
fn test_cli_debug_dap_safe_debug_info_projection() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_safe_debug_info");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass StaticInfo {\n    public function __construct(public int $id, public string $name) {}\n    public function __debugInfo(): array {\n        return [\n            'id' => $this->id,\n            'name' => $this->name,\n            'version' => '1.0.0',\n            'flags' => [1, 2, 3],\n            'meta' => ['stable' => true, 'label' => 'ok'],\n        ];\n    }\n}\nclass ComputedInfo {\n    public function __construct(public int $id) {}\n    public function __debugInfo(): array {\n        echo \"COMPUTED_CALLED\\n\";\n        return ['id' => $this->id, 'double' => $this->id * 2];\n    }\n}\nfunction inspect(): void {\n    $static = new StaticInfo(7, \"safe\");\n    $computed = new ComputedInfo(5);\n    echo $static->id, $computed->id, \"\\n\";\n}\ninspect();\n",
    )
    .expect("write safe debug info fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 24}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("main.edbg.json")).expect("debug info metadata"),
    )
    .expect("valid debug info metadata");
    let class = |name: &str| {
        metadata["classes"]
            .as_array()
            .expect("classes")
            .iter()
            .find(|class| class["name"] == name)
            .unwrap_or_else(|| panic!("missing class {name}: {metadata}"))
    };
    let static_info = class("StaticInfo");
    assert_eq!(static_info["debug_info"]["kind"], "projection", "{static_info}");
    assert_eq!(
        static_info["debug_info"]["entries"].as_array().map(Vec::len),
        Some(5),
        "{static_info}"
    );
    assert_eq!(
        static_info["debug_info"]["entries"][2]["value"],
        "string(5) \"1.0.0\"",
        "{static_info}"
    );
    let computed_info = class("ComputedInfo");
    assert_eq!(computed_info["debug_info"]["kind"], "runtime", "{computed_info}");
    assert_eq!(
        computed_info["debug_info"]["entries"].as_array().map(Vec::len),
        Some(0),
        "{computed_info}"
    );

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("safe debug info frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("safe debug info frame variables");
    let find = |name: &str| {
        variables
            .iter()
            .find(|variable| variable["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };

    let static_value = find("$static");
    assert_eq!(static_value["value"], "object(StaticInfo) (5)", "{static_value}");
    let static_ref = static_value["variablesReference"]
        .as_i64()
        .expect("static variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": static_ref})),
    );
    let response = client.response(seq);
    let children = response["body"]["variables"]
        .as_array()
        .expect("static projection children");
    let names: Vec<&str> = children
        .iter()
        .filter_map(|child| child["name"].as_str())
        .collect();
    assert_eq!(names, ["id", "name", "version", "flags", "meta"], "{response}");
    assert_eq!(children[0]["value"], "int(7)", "{response}");
    assert_eq!(children[1]["value"], "string(4) \"safe\"", "{response}");
    assert_eq!(
        children[2]["value"], "string(5) \"1.0.0\"",
        "{response}"
    );
    assert_eq!(children[2]["type"], "string", "{response}");
    assert_eq!(
        children[3]["value"],
        "array(3) {[0] => int(1), [1] => int(2), [2] => int(3)}",
        "{response}"
    );
    assert_eq!(children[3]["type"], "array", "{response}");
    assert_eq!(children[3]["variablesReference"], 0, "{response}");
    assert_eq!(
        children[4]["value"],
        "array(2) {[\"stable\"] => bool(true), [\"label\"] => string(2) \"ok\"}",
        "{response}"
    );
    assert_eq!(children[4]["variablesReference"], 0, "{response}");

    let computed_value = find("$computed");
    assert_eq!(
        computed_value["value"], "object(ComputedInfo) (1) [__debugInfo() runtime]",
        "{computed_value}"
    );
    let computed_ref = computed_value["variablesReference"]
        .as_i64()
        .expect("computed variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": computed_ref})),
    );
    let response = client.response(seq);
    let computed_children = response["body"]["variables"]
        .as_array()
        .expect("computed children");
    assert_eq!(computed_children.len(), 1, "{response}");
    assert_eq!(computed_children[0]["name"], "id", "{response}");
    assert_eq!(computed_children[0]["value"], "int(5)", "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for safe debug info adapter");
    assert!(status.success(), "safe debug info adapter failed");
    let stderr = client.program_output();
    assert!(
        !stderr.contains("COMPUTED_CALLED"),
        "the debugger executed a runtime-only __debugInfo(): {stderr}"
    );
    assert!(
        stderr.trim_end().ends_with("75"),
        "safe debug info program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `dataBreakpointInfo` resolves a string-keyed dynamic property and
/// `setDataBreakpoints` stops at the next site after the hash entry changes.
#[test]
fn test_cli_debug_dap_dynamic_property_data_breakpoint_stops_on_write() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_dynamic_data_breakpoint");
    fs::write(
        dir.join("main.php"),
        "<?php\n#[\\AllowDynamicProperties]\nclass Bag {\n    public function __construct(public int $id) {}\n}\nfunction run(): int {\n    $bag = new Bag(7);\n    $bag->extra = \"before\";\n    $bag->extra = \"after\";\n    return $bag->id;\n}\necho run(), \"\\n\";\n",
    )
    .expect("write dynamic data breakpoint fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 9}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("dynamic data frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("dynamic data frame variables");
    let bag = variables
        .iter()
        .find(|variable| variable["name"] == "$bag")
        .unwrap_or_else(|| panic!("missing $bag: {response}"));
    assert_eq!(bag["value"], "object(Bag) (2)", "{bag}");
    let bag_ref = bag["variablesReference"]
        .as_i64()
        .expect("bag variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": bag_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("bag dynamic properties");
    let extra = properties
        .iter()
        .find(|property| property["name"] == "extra")
        .unwrap_or_else(|| panic!("missing dynamic extra property: {response}"));
    assert_eq!(extra["value"], "string(6) \"before\"", "{extra}");

    let seq = client.send(
        "dataBreakpointInfo",
        Some(serde_json::json!({
            "variablesReference": bag_ref,
            "name": "extra",
        })),
    );
    let response = client.response(seq);
    let data_id = response["body"]["dataId"]
        .as_str()
        .unwrap_or_else(|| panic!("no dynamic dataId: {response}"))
        .to_string();
    assert!(data_id.starts_with("dynamic:"), "{response}");
    assert!(
        response["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("dynamic property")
                && description.contains("extra")),
        "{response}"
    );
    assert_eq!(response["body"]["accessTypes"][0], "write", "{response}");

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({
            "breakpoints": [{"dataId": data_id, "accessType": "write"}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], true, "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "data breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:10")),
        "{stopped}"
    );

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("dynamic data hit frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("dynamic data hit variables");
    let bag = variables
        .iter()
        .find(|variable| variable["name"] == "$bag")
        .unwrap_or_else(|| panic!("missing $bag after write: {response}"));
    let bag_ref = bag["variablesReference"]
        .as_i64()
        .expect("bag variablesReference after write");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": bag_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("bag dynamic properties after write");
    let extra = properties
        .iter()
        .find(|property| property["name"] == "extra")
        .unwrap_or_else(|| panic!("missing dynamic extra after write: {response}"));
    assert_eq!(extra["value"], "string(5) \"after\"", "{extra}");

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({"breakpoints": []})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for dynamic data adapter");
    assert!(status.success(), "dynamic data adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.contains("7"),
        "dynamic data program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `dataBreakpointInfo` on an object container returns a property
/// watch, and that writing the property stops at the next source site.
#[test]
fn test_cli_debug_dap_object_property_data_breakpoint_stops_on_write() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_object_data_breakpoint");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass Counter {\n    public function __construct(public int $value) {}\n}\nfunction run(): int {\n    $counter = new Counter(0);\n    $counter->value = 1;\n    $counter->value = 2;\n    return $counter->value;\n}\necho run(), \"\\n\";\n",
    )
    .expect("write object data breakpoint fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 7}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("object data frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("object data frame variables");
    let counter = variables
        .iter()
        .find(|variable| variable["name"] == "$counter")
        .unwrap_or_else(|| panic!("missing $counter: {response}"));
    assert_eq!(counter["value"], "object(Counter) (1)", "{counter}");
    let counter_ref = counter["variablesReference"]
        .as_i64()
        .expect("counter variablesReference");
    assert!(counter_ref > 0, "{counter}");
    // The expanded object starts at int(0).
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": counter_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("counter properties");
    let value = properties
        .iter()
        .find(|property| property["name"] == "value")
        .unwrap_or_else(|| panic!("missing value property: {response}"));
    assert_eq!(value["value"], "int(0)");

    let seq = client.send(
        "dataBreakpointInfo",
        Some(serde_json::json!({
            "variablesReference": counter_ref,
            "name": "value",
        })),
    );
    let response = client.response(seq);
    let data_id = response["body"]["dataId"]
        .as_str()
        .unwrap_or_else(|| panic!("no object dataId: {response}"))
        .to_string();
    assert!(data_id.starts_with("object:"), "{response}");
    assert!(
        response["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("Counter")
                && description.contains("value")),
        "{response}"
    );
    assert_eq!(response["body"]["accessTypes"][0], "write", "{response}");
    assert_eq!(response["body"]["canPersist"], false);

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({
            "breakpoints": [{"dataId": data_id, "accessType": "write"}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], true, "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "data breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:8")),
        "{stopped}"
    );

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("object data hit frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("object data hit variables");
    let counter = variables
        .iter()
        .find(|variable| variable["name"] == "$counter")
        .unwrap_or_else(|| panic!("missing $counter after write: {response}"));
    let counter_ref = counter["variablesReference"]
        .as_i64()
        .expect("counter variablesReference after write");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": counter_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("counter properties after write");
    let value = properties
        .iter()
        .find(|property| property["name"] == "value")
        .unwrap_or_else(|| panic!("missing value property after write: {response}"));
    assert_eq!(value["value"], "int(1)");

    let seq = client.send(
        "setDataBreakpoints",
        Some(serde_json::json!({"breakpoints": []})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for object data adapter");
    assert!(status.success(), "object data adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with('2'),
        "object data program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies DAP function breakpoints stop on entry to a named function, honor
/// function-breakpoint conditions, and report unknown function names.
#[test]
fn test_cli_debug_dap_function_breakpoint_stops_on_entry() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_function_breakpoint");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction target(int $x): int {\n    return $x * 2;\n}\nfunction calls(): int {\n    $a = target(3);\n    $b = target(4);\n    return $a + $b;\n}\necho calls(), \"\\n\";\n",
    )
    .expect("write function breakpoint fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["success"], true);
    assert_eq!(response["body"]["supportsFunctionBreakpoints"], true);
    assert_eq!(response["body"]["supportsConditionalBreakpoints"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);

    let seq = client.send(
        "setFunctionBreakpoints",
        Some(serde_json::json!({"breakpoints": [{"name": "missing_function"}]})),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], false, "{response}");
    assert!(
        response["body"]["breakpoints"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("no function named")),
        "{response}"
    );

    let seq = client.send(
        "setFunctionBreakpoints",
        Some(serde_json::json!({
            "breakpoints": [{"name": "target", "condition": "$x === 4"}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["breakpoints"][0]["verified"], true, "{response}");

    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "function breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("target")),
        "{stopped}"
    );

    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("function stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["target", "calls", "main"], "{response}");
    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("function frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("function variables");
    let x = variables
        .iter()
        .find(|variable| variable["name"] == "$x")
        .unwrap_or_else(|| panic!("missing $x: {response}"));
    assert_eq!(x["value"], "int(4)");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for function adapter");
    assert!(status.success(), "function adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("14"),
        "function program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an object local shows its class name and its declared properties,
/// including their values and a private promoted property's visibility.
#[test]
fn test_cli_debug_dap_inspects_object_class_and_properties() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_object");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass Point {\n    public function __construct(public int $x, public int $y, private string $label) {}\n}\n$point = new Point(2, 3, \"origin\");\necho $point->x, \"\\n\";\n",
    )
    .expect("write DAP object fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 6}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("object stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["main"], "{response}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("object frame reference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("object frame variables");
    let point = variables
        .iter()
        .find(|variable| variable["name"] == "$point")
        .unwrap_or_else(|| panic!("missing $point: {response}"));
    assert_eq!(point["type"], "Point", "{point}");
    assert_eq!(point["value"], "object(Point) (3)", "{point}");
    let point_ref = point["variablesReference"]
        .as_i64()
        .expect("point variablesReference");
    assert!(point_ref > 0, "{point}");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": point_ref})),
    );
    let response = client.response(seq);
    let properties = response["body"]["variables"]
        .as_array()
        .expect("object properties");
    let find = |name: &str| {
        properties
            .iter()
            .find(|property| property["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };
    assert_eq!(find("x")["value"], "int(2)");
    assert_eq!(find("y")["value"], "int(3)");
    assert_eq!(find("label (private)")["value"], "string(6) \"origin\"");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for object DAP adapter");
    assert!(status.success(), "object DAP adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with('2'),
        "object program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a breakpoint in an included file resolves to that source, displays the
/// PHP-level function name, and inspects the included frame's locals.
#[test]
fn test_cli_debug_dap_breakpoint_in_included_file() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_included");
    fs::write(
        dir.join("main.php"),
        "<?php\nrequire __DIR__ . '/lib.php';\necho add_from_lib(2, 3), \"\\n\";\n",
    )
    .expect("write DAP included main fixture");
    fs::write(
        dir.join("lib.php"),
        "<?php\nfunction add_from_lib(int $a, int $b): int {\n    $sum = $a + $b;\n    return $sum;\n}\n",
    )
    .expect("write DAP included library fixture");

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "lib.php"},
            "breakpoints": [{"line": 4}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(
        response["body"]["breakpoints"][0]["verified"], true,
        "{response}"
    );
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("lib.php:4")),
        "{stopped}"
    );
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("included stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["add_from_lib", "main"], "{response}");
    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let reference = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("included variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": reference})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("included variables");
    let find = |name: &str| {
        variables
            .iter()
            .find(|variable| variable["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };
    assert_eq!(find("$a")["value"], "int(2)");
    assert_eq!(find("$b")["value"], "int(3)");
    assert_eq!(find("$sum")["value"], "int(5)");
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for included DAP adapter");
    assert!(status.success(), "included DAP adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with('5'),
        "included program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a breakpoint request against a source changed after compilation is
/// rejected as unverified instead of binding a stale line.
#[test]
fn test_cli_debug_dap_rejects_stale_included_source() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_stale_source");
    fs::write(
        dir.join("main.php"),
        "<?php\nrequire __DIR__ . '/lib.php';\necho add_from_lib(2, 3), \"\\n\";\n",
    )
    .expect("write stale-source main fixture");
    fs::write(
        dir.join("lib.php"),
        "<?php\nfunction add_from_lib(int $a, int $b): int {\n    $sum = $a + $b;\n    return $sum;\n}\n",
    )
    .expect("write stale-source library fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile stale-source fixture");
    assert!(
        compile.status.success(),
        "debug compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    fs::write(
        dir.join("lib.php"),
        "<?php\nfunction add_from_lib(int $a, int $b): int {\n    $sum = $a + $b;\n    return $sum;\n}\n// stale\n",
    )
    .expect("mutate stale-source library fixture");

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "lib.php"},
            "breakpoints": [{"line": 4}],
        })),
    );
    let response = client.response(seq);
    let breakpoint = &response["body"]["breakpoints"][0];
    assert_eq!(breakpoint["verified"], false, "{response}");
    assert!(
        breakpoint["message"]
            .as_str()
            .is_some_and(|message| message.contains("source changed")),
        "{response}"
    );
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        assert_ne!(message["event"], "stopped", "stale source stopped: {message}");
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for stale-source adapter");
    assert!(status.success(), "stale-source adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with('5'),
        "stale-source program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the four `monitor --calls` capture modes render and export the
/// requested amount of value detail, including nested children in `full`.
#[test]
fn test_cli_monitor_calls_modes_filter_values() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_modes");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction inspect(array $items, int $n): array {\n    return [\"total\" => $n, \"items\" => $items];\n}\n$result = inspect([1, 2, 3], 3);\necho $result[\"total\"], \"\\n\";\n",
    )
    .expect("write monitor calls modes fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "--with-monitoring", "main.php"])
        .output()
        .expect("compile monitor calls modes fixture");
    assert!(
        compile.status.success(),
        "monitor calls modes compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = |mode: &str| {
        let output = elephc_cli_command(&dir)
            .args([
                "monitor",
                "main",
                &format!("--calls={mode}"),
                "--calls-out",
                &format!("calls-{mode}.jsonl"),
            ])
            .output()
            .expect("run monitor calls mode");
        assert!(
            output.status.success(),
            "monitor --calls={mode} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let events = |mode: &str| -> Vec<serde_json::Value> {
        fs::read_to_string(dir.join(format!("calls-{mode}.jsonl")))
            .expect("calls JSONL sidecar")
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("valid call event JSONL"))
            .collect()
    };
    let inspect_event = |events: &[serde_json::Value]| {
        events
            .iter()
            .find(|event| event["function"] == "inspect")
            .unwrap_or_else(|| panic!("missing inspect event: {events:#?}"))
            .clone()
    };

    let none_stdout = run("none");
    assert!(
        none_stdout.contains("call values (2) [none]:"),
        "{none_stdout}"
    );
    assert!(none_stdout.contains("inspect()"), "{none_stdout}");
    assert!(!none_stdout.contains("$items"), "{none_stdout}");
    assert!(!none_stdout.contains("->"), "{none_stdout}");
    let none_event = inspect_event(&events("none"));
    assert_eq!(none_event["args"], serde_json::json!([]), "{none_event}");
    assert_eq!(none_event["return"], serde_json::Value::Null, "{none_event}");

    let types_stdout = run("types");
    assert!(
        types_stdout.contains("call values (2) [types]:"),
        "{types_stdout}"
    );
    assert!(
        types_stdout.contains("inspect($items: array, $n: int) -> array"),
        "{types_stdout}"
    );
    assert!(!types_stdout.contains("[0] => int(1)"), "{types_stdout}");
    let types_event = inspect_event(&events("types"));
    assert_eq!(types_event["args"][0]["name"], "items", "{types_event}");
    assert_eq!(types_event["args"][0]["type"], "array", "{types_event}");
    assert!(types_event["args"][0].get("value").is_none(), "{types_event}");
    assert_eq!(types_event["return"]["type"], "array", "{types_event}");
    assert!(types_event["return"].get("value").is_none(), "{types_event}");

    let scalars_stdout = run("scalars");
    assert!(
        scalars_stdout.contains("call values (2) [scalars]:"),
        "{scalars_stdout}"
    );
    assert!(
        scalars_stdout.contains(
            "inspect($items = array(3), $n = int(3)) -> array(2)"
        ),
        "{scalars_stdout}"
    );
    assert!(!scalars_stdout.contains("[0] => int(1)"), "{scalars_stdout}");
    let scalars_event = inspect_event(&events("scalars"));
    assert_eq!(scalars_event["args"][0]["value"], "array(3)", "{scalars_event}");
    assert_eq!(scalars_event["args"][1]["value"], "int(3)", "{scalars_event}");
    assert_eq!(scalars_event["return"]["value"], "array(2)", "{scalars_event}");

    let full_stdout = run("full");
    assert!(
        full_stdout.contains("call values (2) [full]:"),
        "{full_stdout}"
    );
    assert!(
        full_stdout.contains("$items = array(3) {[0] => int(1), [1] => int(2), [2] => int(3)}"),
        "{full_stdout}"
    );
    assert!(
        full_stdout.contains(
            "$n = int(3)) -> array(2) {[\"total\"] => int(3), [\"items\"] => array(3) {[0] => int(1), [1] => int(2), [2] => int(3)}}"
        ),
        "{full_stdout}"
    );
    let full_event = inspect_event(&events("full"));
    assert!(
        full_event["args"][0]["value"]
            .as_str()
            .is_some_and(|value| value.contains("[0] => int(1)")),
        "{full_event}"
    );
    assert!(
        full_event["return"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("[\"total\"] => int(3)")),
        "{full_event}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `monitor --calls --html` writes a self-contained call tree with
/// per-call argument and return values, while keeping the text report and
/// program stdout intact.
#[test]
fn test_cli_monitor_calls_writes_html_panel() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_html");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction greet(string $name, int $times): string {\n    return str_repeat($name, $times);\n}\nfunction add(int $a, int $b): int {\n    return $a + $b;\n}\necho greet(\"world\", 3), add(2, 3), \"\\n\";\n",
    )
    .expect("write monitor calls html fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=full",
            "--html",
            "calls.html",
        ])
        .output()
        .expect("run monitor --calls --html");
    assert!(
        output.status.success(),
        "monitor --calls --html failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("worldworldworld5"),
        "program output missing: {stdout}"
    );
    assert!(
        stdout.contains("call values (3) [full]:"),
        "text call table missing: {stdout}"
    );
    assert!(
        stdout.contains(
            "greet($name = string(5) \"world\", $times = int(3)) -> string(15) \"worldworldworld\""
        ),
        "text call values missing: {stdout}"
    );
    assert!(
        stderr.contains("unified profile + calls panel written to calls.html"),
        "html confirmation missing: {stderr}"
    );

    let html = fs::read_to_string(dir.join("calls.html")).expect("calls.html");
    assert!(html.starts_with("<!doctype html>"), "{html}");
    assert!(
        html.contains("<title>elephc monitor — profile + calls</title>"),
        "{html}"
    );
    assert!(html.contains("data-tab=\"profile\""), "{html}");
    assert!(html.contains("data-tab=\"calls\""), "{html}");
    assert!(html.contains("data-tab=\"dumps\""), "{html}");
    assert!(html.contains("data-tab=\"traces\""), "{html}");
    assert!(html.contains("srcdoc=\""), "{html}");
    assert!(html.contains("call graph"), "profile frame missing: {html}");
    assert!(html.contains("captured calls"), "calls frame missing: {html}");
    assert!(html.contains("worldworldworld"), "{html}");
    assert!(html.contains("greet"), "{html}");
    assert!(html.contains("add"), "{html}");
    assert!(
        !html.contains("src=\"http") && !html.contains("href=\"http"),
        "the unified panel must not need network assets: {html}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `monitor --calls --html` writes one unified page with the exact
/// profile frame, Calls values, Dumps, Trace segments and explicit truncation.
#[test]
fn test_cli_monitor_calls_unified_html_has_profile_calls_dumps_traces() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_unified_html");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction echo_value(string $label, string $value): string {\n    elephc_dump($value, $label);\n    return $value;\n}\nfunction trace_me(string $value): string {\n    elephc_trace_start(\"collect\");\n    $result = echo_value(\"payload\", $value);\n    elephc_trace_end(\"collect\");\n    return $result;\n}\n$long = str_repeat(\"x\", 400);\necho strlen(trace_me($long)), \"\\n\";\n",
    )
    .expect("write unified monitor fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=scalars",
            "--html",
            "report.html",
        ])
        .output()
        .expect("run unified monitor capture");
    assert!(
        output.status.success(),
        "unified monitor capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("400"), "program output missing: {stdout}");
    assert!(
        stdout.contains("exact profile —"),
        "--calls --html must include the exact profile: {stdout}"
    );
    assert!(
        stdout.contains("call values (3) [scalars]:"),
        "call table missing: {stdout}"
    );
    assert!(
        stderr.contains("unified profile + calls panel written to report.html"),
        "{stderr}"
    );

    let html = fs::read_to_string(dir.join("report.html")).expect("report.html");
    assert!(
        html.contains("<title>elephc monitor — profile + calls</title>"),
        "{html}"
    );
    assert!(html.contains("data-tab=\"profile\""), "{html}");
    assert!(html.contains("data-tab=\"calls\""), "{html}");
    assert!(html.contains("data-tab=\"dumps\""), "{html}");
    assert!(html.contains("data-tab=\"traces\""), "{html}");
    assert!(html.contains("call graph"), "profile frame missing: {html}");
    assert!(html.contains("captured calls"), "calls frame missing: {html}");
    assert!(html.contains("echo_value"), "call tree missing: {html}");
    assert!(html.contains("payload"), "dump label missing: {html}");
    assert!(
        html.contains("144 bytes omitted"),
        "truncation marker missing: {html}"
    );
    assert!(html.contains("Trace segments"), "trace panel missing: {html}");
    assert!(html.contains("collect"), "trace segment missing: {html}");
    assert!(html.contains("Dumps"), "dump panel missing: {html}");
    assert!(
        !html.contains("src=\"http") && !html.contains("href=\"http"),
        "the unified panel must not need network assets: {html}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `monitor --exact --calls` builds one `--debug --with-monitoring`
/// program and reports both the exact per-function profile and the captured
/// call tree from that single run.
#[test]
fn test_cli_monitor_calls_exact_profile_in_one_run() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_exact");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction work(int $n): int {\n    $sum = 0;\n    for ($i = 0; $i < $n; $i++) {\n        $sum += $i;\n    }\n    return $sum;\n}\nfunction wrap(int $n): int {\n    return work($n) + 1;\n}\necho wrap(1000), \"\\n\";\n",
    )
    .expect("write exact+calls fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--exact",
            "--calls=scalars",
        ])
        .output()
        .expect("run monitor --exact --calls");
    assert!(
        output.status.success(),
        "monitor --exact --calls failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("499501"), "program output missing: {stdout}");
    assert!(
        stdout.contains("exact profile —"),
        "exact profile missing: {stdout}"
    );
    assert!(stdout.contains("{main}"), "root frame missing: {stdout}");
    assert!(stdout.contains("wrap"), "wrap frame missing: {stdout}");
    assert!(stdout.contains("work"), "work frame missing: {stdout}");
    assert!(stdout.contains("incl"), "inclusive timing missing: {stdout}");
    assert!(stdout.contains("self"), "self timing missing: {stdout}");
    assert!(
        stdout.contains("call values (3) [scalars]:"),
        "call table missing: {stdout}"
    );
    assert!(
        stdout.contains("wrap($n = int(1000)) -> int(499501)"),
        "wrap call values missing: {stdout}"
    );
    assert!(
        stdout.contains("work($n = int(1000)) -> int(499500)"),
        "work call values missing: {stdout}"
    );
    assert!(
        !stderr.contains("elephc-instr:"),
        "raw instrument dump leaked to stderr: {stderr}"
    );
    assert!(
        !stderr.contains("elephc-debug-call:"),
        "raw call event leaked to stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies explicit `elephc_dump` snapshots and named `elephc_trace_start/end`
/// segments are captured as events, exported as JSONL and rendered in text/HTML
/// without becoming pseudo-call frames.
#[test]
fn test_cli_monitor_calls_dump_and_segment_events() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_events");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction inner(int $n): int {\n    return $n * 2;\n}\nfunction parse(int $n): int {\n    elephc_trace_start(\"parse\");\n    $value = inner($n);\n    elephc_dump([\"value\" => $value, \"n\" => $n], \"parsed\");\n    elephc_trace_end(\"parse\");\n    return $value;\n}\nfunction run(): int {\n    return parse(4) + 1;\n}\necho run(), \"\\n\";\n",
    )
    .expect("write dump/segment fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=full",
            "--calls-out",
            "events.jsonl",
            "--html",
            "panel.html",
        ])
        .output()
        .expect("run monitor dump/segment capture");
    assert!(
        output.status.success(),
        "monitor dump/segment capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("9"), "program output missing: {stdout}");
    assert!(
        stdout.contains("call values (4) [full]:"),
        "call table missing: {stdout}"
    );
    assert!(
        stdout.contains("inner($n = int(4)) -> int(8)"),
        "inner call values missing: {stdout}"
    );
    assert!(
        stdout.contains("dumps (1):"),
        "dump section missing: {stdout}"
    );
    assert!(
        stdout.contains("[parse] parsed = array(2) {[\"value\"] => int(8), [\"n\"] => int(4)}"),
        "dump value missing: {stdout}"
    );
    assert!(
        stdout.contains("trace segments (1):"),
        "segment section missing: {stdout}"
    );
    assert!(
        stdout.contains("parse — complete (1 call, 1 dump)"),
        "segment summary missing: {stdout}"
    );
    let call_table = stdout
        .split("call values (4) [full]:")
        .nth(1)
        .unwrap_or_default();
    assert!(
        !call_table.contains("elephc_trace_start"),
        "debug helper leaked into the call table: {stdout}"
    );

    let jsonl = fs::read_to_string(dir.join("events.jsonl")).expect("events.jsonl");
    let events: Vec<serde_json::Value> = jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid capture event JSONL"))
        .collect();
    let start = events
        .iter()
        .find(|event| event["kind"] == "segment_start")
        .unwrap_or_else(|| panic!("missing segment_start: {events:#?}"));
    assert_eq!(start["name"], "parse", "{start}");
    assert_eq!(start["segments"], serde_json::json!(["parse"]), "{start}");
    let end = events
        .iter()
        .find(|event| event["kind"] == "segment_end")
        .unwrap_or_else(|| panic!("missing segment_end: {events:#?}"));
    assert_eq!(end["name"], "parse", "{end}");
    assert_eq!(end["complete"], true, "{end}");
    let dump = events
        .iter()
        .find(|event| event["kind"] == "dump")
        .unwrap_or_else(|| panic!("missing dump event: {events:#?}"));
    assert_eq!(dump["label"], "parsed", "{dump}");
    assert_eq!(dump["type"], "array", "{dump}");
    assert!(
        dump["value"]
            .as_str()
            .is_some_and(|value| value.contains("[\"value\"] => int(8)")),
        "{dump}"
    );
    assert_eq!(dump["segments"], serde_json::json!(["parse"]), "{dump}");
    assert!(
        events
            .iter()
            .filter(|event| event.get("function").is_some())
            .all(|event| !event["function"]
                .as_str()
                .unwrap_or("")
                .starts_with("elephc_")),
        "debug helpers must not be captured as calls: {events:#?}"
    );

    let html = fs::read_to_string(dir.join("panel.html")).expect("panel.html");
    assert!(html.contains("captured calls"), "{html}");
    assert!(html.contains("Dumps"), "{html}");
    assert!(html.contains("dump-label"), "{html}");
    assert!(html.contains("parsed"), "{html}");
    assert!(html.contains("int(8)") && html.contains("int(4)"), "{html}");
    assert!(html.contains("segment-tag"), "{html}");
    assert!(html.contains("[parse]"), "{html}");
    assert!(html.contains("Trace segments"), "{html}");
    assert!(html.contains("complete") && html.contains("1 call") && html.contains("1 dump"), "{html}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies call capture bounds long strings and wide/deep containers, redacts
/// `#[\SensitiveParameter]` arguments, and reports each overflow in text, JSONL
/// and HTML.
#[test]
fn test_cli_monitor_calls_bounds_redacts_and_reports_overflow() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_policy");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction probe(\n    #[\\SensitiveParameter] string $password,\n    string $long,\n    array $wide,\n    array $deep\n): string {\n    return \"ok\";\n}\n$long = str_repeat(\"x\", 400);\n$wide = range(1, 20);\n$deep = [[[[\"bottom\"]]]];\necho probe(\"hunter2\", $long, $wide, $deep), \"\\n\";\n",
    )
    .expect("write capture policy fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=full",
            "--calls-out",
            "events.jsonl",
            "--html",
            "panel.html",
        ])
        .output()
        .expect("run monitor capture policy");
    assert!(
        output.status.success(),
        "monitor capture policy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ok"), "program output missing: {stdout}");
    assert!(
        stdout.contains("$password = <redacted>"),
        "sensitive parameter was not redacted: {stdout}"
    );
    assert!(
        stdout.contains("string(400) \"")
            && stdout.contains("…<144 bytes omitted>"),
        "long string was not bounded: {stdout}"
    );
    assert!(
        stdout.contains("… +4 more"),
        "wide container overflow marker missing: {stdout}"
    );
    assert!(
        stdout.contains("…<depth limit>"),
        "depth overflow marker missing: {stdout}"
    );

    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("main.edbg.json")).expect("main.edbg.json"),
    )
    .expect("valid debug metadata");
    let password = metadata["locals"]
        .as_array()
        .expect("locals")
        .iter()
        .find(|local| local["name"] == "password")
        .unwrap_or_else(|| panic!("missing password local: {metadata}"));
    assert_eq!(password["sensitive"], true, "{password}");

    let jsonl = fs::read_to_string(dir.join("events.jsonl")).expect("events.jsonl");
    let events: Vec<serde_json::Value> = jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid capture event JSONL"))
        .collect();
    let probe = events
        .iter()
        .find(|event| event["function"] == "probe")
        .unwrap_or_else(|| panic!("missing probe event: {events:#?}"));
    let args = probe["args"].as_array().expect("probe args");
    assert_eq!(args[0]["redacted"], true, "{probe}");
    assert_eq!(args[0]["value"], "<redacted>", "{probe}");
    assert_eq!(args[1]["truncated"], true, "{probe}");
    assert_eq!(args[1]["overflow"]["string_bytes"], 144, "{probe}");
    assert_eq!(args[2]["truncated"], true, "{probe}");
    assert_eq!(args[2]["overflow"]["children"], 4, "{probe}");
    assert_eq!(args[3]["truncated"], true, "{probe}");
    assert_eq!(args[3]["overflow"]["depth"], true, "{probe}");

    let html = fs::read_to_string(dir.join("panel.html")).expect("panel.html");
    assert!(html.contains("redacted"), "{html}");
    assert!(html.matches("truncated").count() >= 3, "{html}");
    assert!(html.contains("144 bytes omitted"), "{html}");
    assert!(html.contains("+4 more"), "{html}");
    assert!(html.contains("depth limit"), "{html}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `monitor --calls` compiles a debug build, captures PHP-formatted
/// argument values, and still emits the program's normal stdout.
#[test]
fn test_cli_monitor_calls_captures_argument_values() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction greet(string $name, int $times): string {\n    return str_repeat($name, $times);\n}\nfunction add(int $a, int $b): int {\n    return $a + $b;\n}\nfunction half(float $x): float {\n    return $x / 2;\n}\necho greet(\"world\", 3), add(2, 3), half(5), \"\\n\";\n",
    )
    .expect("write monitor --calls fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=scalars",
            "--calls-out",
            "calls.jsonl",
        ])
        .output()
        .expect("run monitor --calls");
    assert!(
        output.status.success(),
        "monitor --calls failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("worldworldworld52.5"),
        "program output missing: {stdout}"
    );
    assert!(
        stdout.contains(
            "greet($name = string(5) \"world\", $times = int(3)) -> string(15) \"worldworldworld\""
        ),
        "captured greet args/return missing: {stdout}"
    );
    assert!(
        stdout.contains("add($a = int(2), $b = int(3)) -> int(5)"),
        "captured add args/return missing: {stdout}"
    );
    assert!(
        stdout.contains("half($x = float(5)) -> float(2.5)"),
        "captured float args/return missing: {stdout}"
    );
    assert!(stdout.contains("call values ("), "{stdout}");
    let jsonl = fs::read_to_string(dir.join("calls.jsonl")).expect("calls JSONL sidecar");
    let events: Vec<serde_json::Value> = jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid call event JSONL"))
        .collect();
    let greet = events
        .iter()
        .find(|event| event["function"] == "greet")
        .unwrap_or_else(|| panic!("missing greet event: {events:#?}"));
    assert_eq!(greet["args"][0]["value"], "string(5) \"world\"");
    assert_eq!(greet["args"][1]["value"], "int(3)");
    assert_eq!(
        greet["return"]["value"],
        "string(15) \"worldworldworld\""
    );
    let add = events
        .iter()
        .find(|event| event["function"] == "add")
        .unwrap_or_else(|| panic!("missing add event: {events:#?}"));
    assert_eq!(add["args"][0]["value"], "int(2)");
    assert_eq!(add["args"][1]["value"], "int(3)");
    assert_eq!(add["return"]["value"], "int(5)");
    let half = events
        .iter()
        .find(|event| event["function"] == "half")
        .unwrap_or_else(|| panic!("missing half event: {events:#?}"));
    assert_eq!(half["args"][0]["value"], "float(5)");
    assert_eq!(half["return"]["value"], "float(2.5)");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `debug --attach` connects to a pre-started debug binary's one-shot
/// socket, drives the same DAP stack, and leaves the program's stdout independent.
#[test]
fn test_cli_debug_attach_connects_to_listening_binary() {
    let dir = make_cli_test_dir("elephc_cli_debug_attach");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction add(int $a, int $b): int {\n    $sum = $a + $b;\n    return $sum;\n}\necho add(2, 3), \"\\n\";\n",
    )
    .expect("write attach fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile attach fixture");
    assert!(
        compile.status.success(),
        "attach fixture compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let socket = short_attach_socket("attach");
    let mut debuggee = DebuggeeGuard {
        child: std::process::Command::new(dir.join("main"))
            .env("ELEPHC_DEBUG_DAP", "1")
            .env("ELEPHC_DEBUG_DAP_LISTEN", &socket)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn attach debuggee"),
    };
    for _ in 0..200 {
        if socket.exists() {
            break;
        }
        if let Some(status) = debuggee.child.try_wait().expect("poll attach debuggee") {
            panic!("attach debuggee exited before listening: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(socket.exists(), "attach socket was not created");

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--attach", socket.to_str().unwrap()]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 4}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("attach stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["add", "main"], "{response}");
    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let reference = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("attach variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": reference})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("attach variables");
    let find = |name: &str| {
        variables
            .iter()
            .find(|variable| variable["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };
    assert_eq!(find("$a")["value"], "int(2)");
    assert_eq!(find("$b")["value"], "int(3)");
    assert_eq!(find("$sum")["value"], "int(5)");
    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let adapter_status = client.child.wait().expect("wait for attach adapter");
    assert!(adapter_status.success(), "attach adapter failed");
    let stdout = client.program_output();
    let debuggee_status = debuggee.child.wait().expect("wait for attach debuggee");
    assert!(debuggee_status.success(), "attach debuggee failed");
    assert!(stdout.trim_end().ends_with('5'), "attach output: {stdout}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a fiber's parked frames are removed from the consumer stack after
/// suspend and restored above the consumer after resume, so DAP stackTrace (and
/// therefore `bt`/exception stops) shows the real caller chain.
#[test]
fn test_cli_debug_dap_fiber_switch_keeps_call_chain() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_fiber_stack");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction fiberBody(): void {\n    $inside = 1;\n    Fiber::suspend();\n    $after = 2;\n    echo \"fiber\\n\";\n}\nfunction consumer(): void {\n    $fiber = new Fiber('fiberBody');\n    $before = 1;\n    $fiber->start();\n    $afterStart = 3;\n    $fiber->resume();\n    $done = 4;\n}\nconsumer();\n",
    )
    .expect("write fiber stack fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 12}, {"line": 5}],
        })),
    );
    let response = client.response(seq);
    let verified: Vec<bool> = response["body"]["breakpoints"]
        .as_array()
        .expect("fiber breakpoints")
        .iter()
        .map(|breakpoint| breakpoint["verified"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(verified, [true, true], "{response}");
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);

    let consumer_stop = client.event("stopped");
    assert_eq!(consumer_stop["body"]["reason"], "breakpoint", "{consumer_stop}");
    assert!(
        consumer_stop["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:12")),
        "{consumer_stop}"
    );
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("consumer stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["consumer", "main"], "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    let fiber_stop = client.event("stopped");
    assert_eq!(fiber_stop["body"]["reason"], "breakpoint", "{fiber_stop}");
    assert!(
        fiber_stop["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("main.php:5")),
        "{fiber_stop}"
    );
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("fiber stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["fiberBody", "consumer", "main"], "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for fiber adapter");
    assert!(status.success(), "fiber adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("fiber"),
        "fiber program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a suspended fiber appears as a selectable DAP thread with its own
/// stack and locals while the consumer is stopped.
#[test]
fn test_cli_debug_dap_fiber_thread_selection() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_fiber_thread");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction fiberBody(): void {\n    $inside = 1;\n    Fiber::suspend();\n    $after = 2;\n}\nfunction consumer(): void {\n    $fiber = new Fiber('fiberBody');\n    $fiber->start();\n    $marker = 1;\n    $fiber->resume();\n    $done = 2;\n}\nconsumer();\necho \"done\\n\";\n",
    )
    .expect("write fiber thread fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 10}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");
    assert_eq!(stopped["body"]["threadId"], 1, "{stopped}");

    let seq = client.send("threads", None);
    let response = client.response(seq);
    let threads = response["body"]["threads"]
        .as_array()
        .expect("thread list");
    assert_eq!(threads.len(), 2, "{response}");
    let fiber_thread = threads
        .iter()
        .find(|thread| thread["id"].as_i64() != Some(1))
        .and_then(|thread| thread["id"].as_i64())
        .unwrap_or_else(|| panic!("no fiber thread: {response}"));
    assert!(fiber_thread > 1, "{response}");

    let seq = client.send(
        "stackTrace",
        Some(serde_json::json!({"threadId": 1})),
    );
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("main thread stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["consumer", "main"], "{response}");

    let seq = client.send(
        "stackTrace",
        Some(serde_json::json!({"threadId": fiber_thread})),
    );
    let response = client.response(seq);
    let frames = response["body"]["stackFrames"]
        .as_array()
        .expect("fiber thread stackFrames");
    let names: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["fiberBody", "consumer", "main"], "{response}");
    assert_eq!(frames[0]["line"], 4, "{response}");
    let fiber_frame = frames[0]["id"]
        .as_i64()
        .unwrap_or_else(|| panic!("fiber frame id: {response}"));

    let seq = client.send(
        "scopes",
        Some(serde_json::json!({"frameId": fiber_frame})),
    );
    let response = client.response(seq);
    let frame_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("fiber frame variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": frame_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("fiber frame variables");
    let inside = variables
        .iter()
        .find(|variable| variable["name"] == "$inside")
        .unwrap_or_else(|| panic!("missing $inside: {response}"));
    assert_eq!(inside["value"], "int(1)", "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for fiber thread adapter");
    assert!(status.success(), "fiber thread adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("done"),
        "fiber thread program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a generator's parked frames are removed from its consumer's stack
/// after `yield` and restored above the consumer when `next()` resumes it.
#[test]
fn test_cli_debug_dap_generator_switch_keeps_call_chain() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_generator_stack");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction gen(): Generator {\n    yield 1;\n    $after = 2;\n    yield 3;\n}\nfunction consumer(): void {\n    $g = gen();\n    $g->current();\n    $afterStart = 4;\n    $g->next();\n    $done = 5;\n}\nconsumer();\necho \"done\\n\";\n",
    )
    .expect("write generator stack fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 10}, {"line": 4}],
        })),
    );
    let response = client.response(seq);
    let verified: Vec<bool> = response["body"]["breakpoints"]
        .as_array()
        .expect("generator breakpoints")
        .iter()
        .map(|breakpoint| breakpoint["verified"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(verified, [true, true], "{response}");
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);

    let consumer_stop = client.event("stopped");
    assert_eq!(
        consumer_stop["body"]["reason"], "breakpoint", "{consumer_stop}"
    );
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("generator consumer stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["consumer", "main"], "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    let generator_stop = client.event("stopped");
    assert_eq!(
        generator_stop["body"]["reason"], "breakpoint", "{generator_stop}"
    );
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("generator stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["gen", "consumer", "main"], "{response}");

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for generator adapter");
    assert!(status.success(), "generator adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with("done"),
        "generator program output missing: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an uncaught exception thrown inside a resumed fiber stops with the
/// fiber's frames on top, proving the DAP exception hook uses the restored
/// fiber-aware activation chain.
#[test]
fn test_cli_debug_dap_fiber_exception_stop_keeps_call_chain() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_fiber_exception");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction fiberBody(): void {\n    Fiber::suspend();\n    throw new RuntimeException(\"fiber\");\n}\nfunction consumer(): void {\n    $fiber = new Fiber('fiberBody');\n    $fiber->start();\n    $fiber->resume();\n}\nconsumer();\n",
    )
    .expect("write fiber exception fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setExceptionBreakpoints",
        Some(serde_json::json!({"filters": ["caught"]})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);

    let fiber_stop = client.event("stopped");
    assert_eq!(fiber_stop["body"]["reason"], "exception", "{fiber_stop}");
    assert_eq!(fiber_stop["body"]["text"], "RuntimeException", "{fiber_stop}");
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let names: Vec<&str> = response["body"]["stackFrames"]
        .as_array()
        .expect("fiber exception stackFrames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["fiberBody", "consumer", "main"], "{response}");

    // Disconnect while stopped inside the resumed fiber; the adapter exits the
    // debuggee, so the assertion above is the whole exception-stack contract.
    let seq = client.send("disconnect", None);
    loop {
        let message = client
            .recv_with_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|| panic!("fiber exception disconnect did not answer"));
        if message["type"] == "response" && message["request_seq"] == seq {
            break;
        }
    }
    let status = client
        .child
        .wait()
        .expect("wait for fiber exception adapter");
    assert!(status.success(), "fiber exception adapter failed");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies attach-mode pause: the runtime publishes its pid next to the
/// one-shot attach socket and `debug --attach` signals it on DAP `pause`.
#[test]
fn test_cli_debug_attach_pause_stops_outside_started_debuggee() {
    let dir = make_cli_test_dir("elephc_cli_debug_attach_pause");
    fs::write(
        dir.join("main.php"),
        "<?php\n$sum = 0;\nfor ($i = 0; $i < 1000000000; $i++) {\n    $sum = ($sum + 1) % 1000;\n}\necho $sum, \"\\n\";\n",
    )
    .expect("write attach pause fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile attach pause fixture");
    assert!(
        compile.status.success(),
        "attach pause compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let socket = short_attach_socket("attach-pause");
    let pid_path = std::path::PathBuf::from(format!("{}.pid", socket.display()));
    let mut debuggee = DebuggeeGuard {
        child: std::process::Command::new(dir.join("main"))
            .env("ELEPHC_DEBUG_DAP", "1")
            .env("ELEPHC_DEBUG_DAP_LISTEN", &socket)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn attach pause debuggee"),
    };
    for _ in 0..200 {
        if socket.exists() && pid_path.exists() {
            break;
        }
        if let Some(status) = debuggee.child.try_wait().expect("poll attach pause debuggee") {
            panic!("attach pause debuggee exited before listening: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(socket.exists(), "attach pause socket was not created");
    assert!(pid_path.exists(), "attach pause pid file was not created");
    let published_pid: u32 = fs::read_to_string(&pid_path)
        .expect("read attach pid file")
        .trim()
        .parse()
        .expect("parse attach pid");
    assert_eq!(
        published_pid,
        debuggee.child.id(),
        "the pid file must name the debuggee"
    );

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--attach", socket.to_str().unwrap()]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    // Let the debuggee leave the entry stop and enter the loop.
    std::thread::sleep(std::time::Duration::from_millis(100));

    let pause_seq = client.send("pause", None);
    let mut pause_response = false;
    let mut paused = false;
    for _ in 0..200 {
        let message = match client.recv_with_timeout(std::time::Duration::from_secs(5)) {
            Some(message) => message,
            None => panic!("attach pause did not stop within five seconds"),
        };
        if message["type"] == "response" && message["request_seq"] == pause_seq {
            pause_response = true;
        }
        if message["type"] == "event" && message["event"] == "stopped" {
            assert_eq!(message["body"]["reason"], "pause", "{message}");
            paused = true;
            if !pause_response {
                if let Some(message) =
                    client.recv_with_timeout(std::time::Duration::from_secs(5))
                {
                    if message["type"] == "response"
                        && message["request_seq"] == pause_seq
                    {
                        pause_response = true;
                    }
                }
            }
            break;
        }
    }
    assert!(paused, "attach pause never produced a stopped event");
    assert!(pause_response, "attach pause request was not acknowledged");

    let disconnect_seq = client.send("disconnect", None);
    loop {
        let Some(message) = client.recv_with_timeout(std::time::Duration::from_secs(5)) else {
            panic!("attach pause disconnect did not answer");
        };
        if message["type"] == "response" && message["request_seq"] == disconnect_seq {
            break;
        }
    }
    let adapter_status = client.child.wait().expect("wait for attach pause adapter");
    assert!(adapter_status.success(), "attach pause adapter failed");
    let debuggee_status = debuggee
        .child
        .wait()
        .expect("wait for attach pause debuggee");
    assert!(debuggee_status.success(), "attach pause debuggee failed");
    let stderr = client.program_output();
    assert!(
        !stderr.contains("no attach pid file"),
        "attach pause did not use the published pid: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a pause request while the program is running stops at the next
/// site and is acknowledged without waiting for a breakpoint.
#[test]
fn test_cli_debug_dap_pause_while_running() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_pause");
    fs::write(
        dir.join("main.php"),
        "<?php\n$sum = 0;\nfor ($i = 0; $i < 1000000000; $i++) {\n    $sum = ($sum + 1) % 1000;\n}\necho $sum, \"\\n\";\n",
    )
    .expect("write pause fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);

    let pause_seq = client.send("pause", None);
    let mut pause_response = false;
    let mut paused = false;
    for _ in 0..200 {
        let message = match client.recv_with_timeout(std::time::Duration::from_secs(5)) {
            Some(message) => message,
            None => panic!("pause did not stop within five seconds"),
        };
        if message["type"] == "response" && message["request_seq"] == pause_seq {
            pause_response = true;
        }
        if message["type"] == "event" && message["event"] == "stopped" {
            assert_eq!(message["body"]["reason"], "pause", "{message}");
            paused = true;
            if !pause_response {
                if let Some(message) =
                    client.recv_with_timeout(std::time::Duration::from_secs(5))
                {
                    if message["type"] == "response"
                        && message["request_seq"] == pause_seq
                    {
                        pause_response = true;
                    }
                }
            }
            break;
        }
    }
    assert!(paused, "pause never produced a stopped event");
    assert!(pause_response, "pause request was not acknowledged");

    let disconnect_seq = client.send("disconnect", None);
    loop {
        let Some(message) =
            client.recv_with_timeout(std::time::Duration::from_secs(5))
        else {
            panic!("disconnect did not answer");
        };
        if message["type"] == "response" && message["request_seq"] == disconnect_seq {
            break;
        }
    }
    for _ in 0..100 {
        if let Some(status) = client.child.try_wait().expect("poll pause adapter") {
            assert!(status.success(), "pause adapter failed: {status}");
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("pause adapter hung after disconnect");
}

/// Verifies `disconnect` closes the adapter without waiting for the program, so an
/// IDE can end a session while the debuggee is stopped.
#[test]
fn test_cli_debug_dap_disconnect_stops_the_adapter() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_disconnect");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction step_once(int $x): int {\n    $y = $x + 1;\n    return $y;\n}\necho step_once(1), \"\\n\";\n",
    )
    .expect("write DAP disconnect fixture");
    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 3}],
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");

    let seq = client.send("disconnect", None);
    assert_eq!(client.response(seq)["success"], true);
    let mut exited = false;
    for _ in 0..100 {
        if let Some(status) = client.child.try_wait().expect("poll DAP adapter") {
            assert!(status.success(), "DAP adapter exited with {status}");
            exited = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(exited, "DAP adapter hung after disconnect");
    let _ = fs::remove_dir_all(&dir);
}

/// One minimal DAP client over the adapter's stdio, used by the end-to-end test.
struct DapClient {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    stderr: std::process::ChildStderr,
    seq: i64,
    output_stdout: String,
    output_stderr: String,
}

impl DapClient {
    /// Spawns one adapter command with piped DAP stdin/stdout/stderr.
    fn spawn(command: &mut std::process::Command) -> Self {
        use std::process::Stdio;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn DAP adapter");
        let stdin = child.stdin.take().expect("DAP adapter stdin");
        let stdout = std::io::BufReader::new(child.stdout.take().expect("DAP adapter stdout"));
        let stderr = child.stderr.take().expect("DAP adapter stderr");
        Self {
            child,
            stdin,
            stdout,
            stderr,
            seq: 0,
            output_stdout: String::new(),
            output_stderr: String::new(),
        }
    }

    /// Sends one DAP request and returns its sequence number.
    fn send(&mut self, command: &str, arguments: Option<serde_json::Value>) -> i64 {
        use std::io::Write as _;
        self.seq += 1;
        let mut message = serde_json::json!({
            "seq": self.seq,
            "type": "request",
            "command": command,
        });
        if let Some(arguments) = arguments {
            message["arguments"] = arguments;
        }
        let body = serde_json::to_vec(&message).expect("serialize DAP request");
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())
            .expect("write DAP header");
        self.stdin.write_all(&body).expect("write DAP body");
        self.stdin.flush().expect("flush DAP request");
        self.seq
    }

    /// Reads one framed DAP message, collecting output events transparently.
    fn recv(&mut self) -> serde_json::Value {
        loop {
            let message = self.recv_raw();
            if self.collect_output(&message) {
                continue;
            }
            return message;
        }
    }

    /// Drains the DAP stream until the adapter closes it, collecting output.
    fn drain_until_closed(&mut self) {
        while let Some(message) = self.recv_raw_optional() {
            let _ = self.collect_output(&message);
        }
    }

    /// Collects one output event, returning whether it was consumed.
    fn collect_output(&mut self, message: &serde_json::Value) -> bool {
        if message["type"] != "event" || message["event"] != "output" {
            return false;
        }
        let text = message["body"]["output"].as_str().unwrap_or("");
        match message["body"]["category"].as_str().unwrap_or("") {
            "stdout" => self.output_stdout.push_str(text),
            "stderr" => self.output_stderr.push_str(text),
            _ => {}
        }
        true
    }

    /// Returns the program output collected from DAP `output` events.
    fn program_output(&self) -> String {
        let mut output = self.output_stdout.clone();
        output.push_str(&self.output_stderr);
        output
    }

    /// Returns the stdout portion collected from DAP `output` events.
    fn program_stdout(&self) -> &str {
        &self.output_stdout
    }
    /// Sends one `evaluate` request and returns its full response.
    fn evaluate(&mut self, frame_id: i64, expression: &str) -> serde_json::Value {
        let seq = self.send(
            "evaluate",
            Some(serde_json::json!({
                "expression": expression,
                "frameId": frame_id,
            })),
        );
        self.response(seq)
    }

    /// Sends `continue` and drains messages until the terminated event.
    fn continue_to_terminated(&mut self) {
        let seq = self.send("continue", None);
        assert_eq!(self.response(seq)["success"], true);
        loop {
            let message = self.recv();
            if message["type"] == "event" && message["event"] == "terminated" {
                break;
            }
        }
    }


    /// Returns the stderr portion collected from DAP `output` events.
    fn program_stderr(&self) -> &str {
        &self.output_stderr
    }

    /// Reads one framed DAP message without filtering output events.
    fn recv_raw(&mut self) -> serde_json::Value {
        self.recv_raw_optional()
            .expect("DAP adapter closed stdout")
    }

    /// Reads one framed DAP message, returning `None` at EOF.
    fn recv_raw_optional(&mut self) -> Option<serde_json::Value> {
        use std::io::{BufRead as _, Read as _};
        let mut length = None;
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("read DAP header");
            if read == 0 {
                return None;
            }
            let line = line.trim_end_matches(|ch| ch == '\r' || ch == '\n');
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = Some(value.trim().parse::<usize>().expect("DAP body length"));
            }
        }
        let length = length.expect("DAP message has no Content-Length");
        let mut body = vec![0u8; length];
        self.stdout.read_exact(&mut body).expect("read DAP body");
        Some(serde_json::from_slice(&body).expect("parse DAP message"))
    }

    /// Reads one DAP message only if it arrives within `timeout`.
    fn recv_with_timeout(&mut self, timeout: std::time::Duration) -> Option<serde_json::Value> {
        use std::os::unix::io::AsRawFd as _;
        if self.stdout.buffer().is_empty() {
            let mut descriptor = libc::pollfd {
                fd: self.stdout.get_ref().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe {
                libc::poll(
                    &mut descriptor,
                    1,
                    timeout.as_millis().min(i32::MAX as u128) as i32,
                )
            };
            if ready <= 0 {
                return None;
            }
        }
        Some(self.recv())
    }

    /// Reads the response for `request_seq`, failing on an unexpected message.
    fn response(&mut self, request_seq: i64) -> serde_json::Value {
        let message = self.recv();
        assert_eq!(message["type"], "response", "{message}");
        assert_eq!(message["request_seq"], request_seq, "{message}");
        message
    }

    /// Reads one named DAP event.
    fn event(&mut self, name: &str) -> serde_json::Value {
        let message = self.recv();
        assert_eq!(message["type"], "event", "{message}");
        assert_eq!(message["event"], name, "{message}");
        message
    }
}

impl Drop for DapClient {
    /// Stops a test adapter that did not reach its natural exit.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Returns a short Unix-socket path for attach fixtures.
///
/// macOS limits `sun_path` to 104 bytes; the test temp directory is far longer
/// than that on the runners. The runtime removes the socket after accepting.
fn short_attach_socket(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::path::PathBuf::from(format!(
        "/tmp/elephc-{label}-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Kills a separately-spawned debuggee when a test unwinds before waiting.
struct DebuggeeGuard {
    child: std::process::Child,
}

impl Drop for DebuggeeGuard {
    /// Reaps a debuggee that did not reach its natural exit.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Verifies the DAP launch sequence, breakpoint stop, caller stack, scopes,
/// variables (including arrays and strings), evaluate and clean termination.
#[test]
fn test_cli_debug_dap_launch_breakpoints_stack_and_variables() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction add(int &$a, int $b): int {\n    $sum = $a + $b;\n    return $sum;\n}\nfunction compute(): int {\n    $items = [1, \"two\"];\n    $n = 2;\n    $x = add($n, 3);\n    return $x;\n}\necho compute(), \"\\n\";\n",
    )
    .expect("write DAP fixture");

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );

    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");

    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);

    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 4}],
        })),
    );
    let response = client.response(seq);
    assert_eq!(
        response["body"]["breakpoints"][0]["verified"], true,
        "{response}"
    );

    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    let stopped = client.event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint", "{stopped}");
    assert!(
        stopped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("add")),
        "{stopped}"
    );

    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let frames = response["body"]["stackFrames"]
        .as_array()
        .expect("stackFrames");
    let names: Vec<&str> = frames
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["add", "compute", "main"], "{response}");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 0})));
    let response = client.response(seq);
    let add_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("add variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": add_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("add variables");
    let find = |name: &str| {
        variables
            .iter()
            .find(|variable| variable["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"))
    };
    assert_eq!(find("$a")["value"], "&int(2)");
    assert_eq!(find("$b")["value"], "int(3)");
    assert_eq!(find("$sum")["value"], "int(5)");

    let seq = client.send("scopes", Some(serde_json::json!({"frameId": 1})));
    let response = client.response(seq);
    let compute_ref = response["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("compute variablesReference");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": compute_ref})),
    );
    let response = client.response(seq);
    let variables = response["body"]["variables"]
        .as_array()
        .expect("compute variables");
    let items = variables
        .iter()
        .find(|variable| variable["name"] == "$items")
        .unwrap_or_else(|| panic!("missing $items: {response}"));
    assert_eq!(items["value"], "array(2)");
    let items_ref = items["variablesReference"]
        .as_i64()
        .expect("items variablesReference");
    assert!(items_ref > 0, "{items}");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": items_ref})),
    );
    let response = client.response(seq);
    let children = response["body"]["variables"]
        .as_array()
        .expect("items children");
    assert_eq!(children[0]["name"], "[0]");
    assert_eq!(children[0]["value"], "int(1)");
    assert_eq!(children[1]["name"], "[1]");
    assert_eq!(children[1]["value"], "string(3) \"two\"");

    let seq = client.send(
        "evaluate",
        Some(serde_json::json!({"expression": "$items", "frameId": 1})),
    );
    let response = client.response(seq);
    assert_eq!(response["body"]["result"], "array(2)");
    assert!(response["body"]["variablesReference"].as_i64().unwrap() > 0);

    let seq = client.send("next", None);
    assert_eq!(client.response(seq)["success"], true);
    let stepped = client.event("stopped");
    assert_eq!(stepped["body"]["reason"], "step", "{stepped}");
    assert!(
        stepped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("compute")),
        "{stepped}"
    );

    let seq = client.send("continue", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for DAP adapter");
    assert!(status.success(), "DAP adapter failed");
    let stderr = client.program_output();
    assert!(
        stderr.trim_end().ends_with('5'),
        "program output missing from stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies read-only evaluate covers locals, object properties, array/hash
/// entries, scalar builtins and arithmetic while refusing mutation and calls.
#[test]
fn test_cli_debug_dap_evaluate_properties_indices_and_scalars() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_evaluate_expressions");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass EvalPoint {\n    public int $x = 4;\n    public string $label = \"hi\";\n    public array $items = [10, 20];\n}\nfunction evaluateProbe(): int {\n    $point = new EvalPoint();\n    $items = [\"a\" => 1, 1 => 2];\n    $n = 3;\n    $s = \"hello\";\n    return $point->x + $items[1] + $n;\n}\necho evaluateProbe(), \"\\n\";\n",
    )
    .expect("write DAP evaluate fixture");

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send(
        "setBreakpoints",
        Some(serde_json::json!({
            "source": {"path": "main.php"},
            "breakpoints": [{"line": 12}],
        })),
    );
    assert_eq!(client.response(seq)["body"]["breakpoints"][0]["verified"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    client.event("stopped");
    let seq = client.send("stackTrace", None);
    let response = client.response(seq);
    let frame_id = response["body"]["stackFrames"][0]["id"]
        .as_i64()
        .expect("evaluate frame id");

    let response = client.evaluate(frame_id, "$point->x");
    assert_eq!(response["success"], true, "{response}");
    assert_eq!(response["body"]["result"], "int(4)");
    assert_eq!(response["body"]["type"], "int");

    let response = client.evaluate(frame_id, "$point->label");
    assert_eq!(response["body"]["result"], "string(2) \"hi\"");
    assert_eq!(response["body"]["type"], "string");

    let response = client.evaluate(frame_id, "$point->items[1]");
    assert_eq!(response["body"]["result"], "int(20)");

    let response = client.evaluate(frame_id, "$items[\"a\"]");
    assert_eq!(response["body"]["result"], "int(1)");

    let response = client.evaluate(frame_id, "$items[1]");
    assert_eq!(response["body"]["result"], "int(2)");

    let response = client.evaluate(frame_id, "$items");
    assert_eq!(response["body"]["result"], "array(2)");
    let items_ref = response["body"]["variablesReference"]
        .as_i64()
        .expect("evaluate items reference");
    assert!(items_ref > 0, "{response}");
    let seq = client.send(
        "variables",
        Some(serde_json::json!({"variablesReference": items_ref})),
    );
    let response = client.response(seq);
    let children = response["body"]["variables"]
        .as_array()
        .expect("evaluate items children");
    assert!(
        children.iter().any(|child| child["name"] == "[\"a\"]" && child["value"] == "int(1)"),
        "{response}"
    );

    for (expression, expected) in [
        ("count($items)", "int(2)"),
        ("strlen($s)", "int(5)"),
        ("gettype($items)", "string(5) \"array\""),
        ("$n + 4", "int(7)"),
        ("$n * 2", "int(6)"),
        ("$n === 3", "bool(true)"),
        ("$n < 4 && $s === \"hello\"", "bool(true)"),
    ] {
        let response = client.evaluate(frame_id, expression);
        assert_eq!(response["success"], true, "{expression}: {response}");
        assert_eq!(
            response["body"]["result"], expected,
            "{expression}: {response}"
        );
    }

    for expression in [
        "$point->missing",
        "$items[\"missing\"]",
        "$missing",
        "$n = 4",
        "mystery($n)",
    ] {
        let response = client.evaluate(frame_id, expression);
        assert_eq!(
            response["success"], false,
            "{expression} must fail closed: {response}"
        );
    }

    client.continue_to_terminated();
    let status = client.child.wait().expect("wait for evaluate adapter");
    assert!(status.success(), "evaluate adapter failed");
    let output = client.program_output();
    assert!(
        output.trim_end().ends_with('9'),
        "evaluate program output missing: {output}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies stdout and stderr chunks are forwarded as DAP output events while
/// the protocol stream stays free of raw program output.
#[test]
fn test_cli_debug_dap_streams_program_output() {
    let dir = make_cli_test_dir("elephc_cli_debug_dap_output_events");
    fs::write(
        dir.join("main.php"),
        "<?php\necho \"stdout-1\\n\";\nfwrite(STDERR, \"stderr-1\\n\");\necho \"stdout-2\\n\";\n",
    )
    .expect("write DAP output fixture");

    let mut client = DapClient::spawn(
        elephc_cli_command(&dir).args(["debug", "--dap", "main.php"]),
    );
    let seq = client.send(
        "initialize",
        Some(serde_json::json!({
            "clientID": "test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        })),
    );
    assert_eq!(client.response(seq)["success"], true);
    client.event("initialized");
    let seq = client.send(
        "launch",
        Some(serde_json::json!({"program": "main.php", "stopOnEntry": false})),
    );
    assert_eq!(client.response(seq)["success"], true);
    let seq = client.send("configurationDone", None);
    assert_eq!(client.response(seq)["success"], true);
    loop {
        let message = client.recv();
        if message["type"] == "event" && message["event"] == "terminated" {
            break;
        }
    }
    let status = client.child.wait().expect("wait for output adapter");
    assert!(status.success(), "output adapter failed");
    let stdout = client.program_stdout();
    let stderr = client.program_stderr();
    assert!(stdout.contains("stdout-1"), "stdout missing: {stdout:?}");
    assert!(stdout.contains("stdout-2"), "stdout missing: {stdout:?}");
    assert!(stderr.contains("stderr-1"), "stderr missing: {stderr:?}");
    let mut adapter_stderr = String::new();
    {
        use std::io::Read as _;
        client
            .stderr
            .read_to_string(&mut adapter_stderr)
            .expect("read adapter stderr");
    }
    assert!(
        !adapter_stderr.contains("stdout-1")
            && !adapter_stderr.contains("stdout-2")
            && !adapter_stderr.contains("stderr-1"),
        "program output leaked to adapter stderr: {adapter_stderr:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--debug` refuses interactive iOS executables with actionable
/// capture-tool guidance, while compile-only `--emit-asm` builds emit the same
/// debug metadata on both iOS targets.
#[test]
fn test_cli_debug_ios_gate_and_compile_only_metadata() {
    let dir = make_cli_test_dir("elephc_cli_debug_ios_gate");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction traceMe(int $n): int { return $n + 1; }\necho traceMe(1), \"\\n\";\n",
    )
    .expect("write iOS debug fixture");
    for target in ["ios-arm64", "ios-sim-arm64"] {
        let output = elephc_cli_command(&dir)
            .args(["--debug", "--target", target, "main.php"])
            .output()
            .expect("run iOS debug gate");
        assert!(!output.status.success(), "--debug accepted {target}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("interactive debugging is not supported on ios-arm64 or ios-sim-arm64"),
            "{target}: {stderr}"
        );
        assert!(stderr.contains("--emit-asm"), "{target}: {stderr}");
        assert!(stderr.contains("elephc monitor --calls"), "{target}: {stderr}");
        assert!(stderr.contains("elephc test --coverage"), "{target}: {stderr}");

        let compile = elephc_cli_command(&dir)
            .args(["--debug", "--emit-asm", "--target", target, "main.php"])
            .output()
            .expect("compile iOS debug metadata");
        assert!(
            compile.status.success(),
            "{target} compile-only debug failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let assembly = fs::read_to_string(dir.join("main.s")).expect("iOS debug assembly");
        assert!(
            assembly.contains("@fn name=traceMe"),
            "{target}: assembly markers missing"
        );
        assert!(
            assembly.contains("__elephc_debug_site"),
            "{target}: debug hooks missing"
        );
        let metadata: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dir.join("main.edbg.json")).expect("iOS debug metadata"),
        )
        .expect("valid iOS debug metadata");
        assert!(
            metadata["sources"]
                .as_array()
                .is_some_and(|sources| !sources.is_empty()),
            "{target}: {metadata}"
        );
        assert!(
            metadata["functions"]
                .as_array()
                .is_some_and(|functions| functions.iter().any(|function| function["name"] == "traceMe")),
            "{target}: {metadata}"
        );
        assert!(
            metadata["sites"]
                .as_array()
                .is_some_and(|sites| !sites.is_empty()),
            "{target}: {metadata}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an uncaught exception writes a postmortem document with the throw
/// site, PHP stack, recent calls and exception fields, and that `debug --last`
/// renders the same document.
#[test]
fn test_cli_debug_postmortem_flushes_uncaught_exception() {
    let dir = make_cli_test_dir("elephc_cli_debug_postmortem_uncaught");
    fs::write(
        dir.join("main.php"),
        "<?php\nclass Boom extends RuntimeException {}\nfunction inner(int $n): void {\n    if ($n > 0) {\n        throw new Boom(\"kaboom\", 42);\n    }\n}\nfunction outer(): void {\n    inner(1);\n}\nouter();\n",
    )
    .expect("write postmortem exception fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile postmortem exception fixture");
    assert!(
        compile.status.success(),
        "postmortem fixture failed to compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let document = dir.join("postmortem.json");
    let run = std::process::Command::new(dir.join("main"))
        .env("ELEPHC_DEBUG_POSTMORTEM", &document)
        .output()
        .expect("run postmortem exception fixture");
    assert!(!run.status.success(), "uncaught exception must fail");
    let postmortem: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&document).expect("postmortem document"),
    )
    .expect("valid postmortem JSON");
    assert_eq!(postmortem["format"], "elephc-postmortem", "{postmortem}");
    assert_eq!(postmortem["reason"], "uncaught_exception", "{postmortem}");
    assert_eq!(postmortem["exception"]["class"], "Boom", "{postmortem}");
    assert_eq!(postmortem["exception"]["message"], "kaboom", "{postmortem}");
    assert_eq!(postmortem["exception"]["code"], 42, "{postmortem}");
    assert_eq!(postmortem["exception"]["file"], "main.php", "{postmortem}");
    assert_eq!(postmortem["exception"]["line"], 5, "{postmortem}");
    let stack = postmortem["stack"].as_array().expect("postmortem stack");
    let functions: Vec<&str> = stack
        .iter()
        .filter_map(|frame| frame["function"].as_str())
        .collect();
    assert_eq!(functions, ["inner", "outer", "main"], "{postmortem}");
    assert_eq!(stack[0]["line"], 5, "{postmortem}");
    let calls = postmortem["calls"].as_array().expect("postmortem calls");
    let names: Vec<&str> = calls
        .iter()
        .filter_map(|call| call["function"].as_str())
        .collect();
    assert_eq!(names, ["main", "outer", "inner"], "{postmortem}");
    assert_eq!(postmortem["dropped"], 0, "{postmortem}");

    let inspect = elephc_cli_command(&dir)
        .args(["debug", "--last", "postmortem.json"])
        .output()
        .expect("inspect postmortem");
    assert!(
        inspect.status.success(),
        "debug --last failed: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("exception: Boom: kaboom (code 42)"), "{stdout}");
    assert!(stdout.contains("throw site: main.php:5"), "{stdout}");
    assert!(stdout.contains("#0 inner at main.php:5"), "{stdout}");
    assert!(stdout.contains("#1 outer at main.php:9"), "{stdout}");
    assert!(stdout.contains("recent calls: 3 captured, 0 dropped"), "{stdout}");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a normal exit with --postmortem writes no document.
#[test]
fn test_cli_debug_postmortem_normal_exit_writes_nothing() {
    let dir = make_cli_test_dir("elephc_cli_debug_postmortem_normal");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction ok(int $n): int { return $n + 1; }\necho ok(1), \"\\n\";\n",
    )
    .expect("write postmortem normal fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile postmortem normal fixture");
    assert!(compile.status.success());
    let document = dir.join("postmortem.json");
    let run = std::process::Command::new(dir.join("main"))
        .env("ELEPHC_DEBUG_POSTMORTEM", &document)
        .output()
        .expect("run postmortem normal fixture");
    assert!(run.status.success(), "normal program failed");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim_end(), "2");
    assert!(
        !document.exists(),
        "a normal exit must not write a postmortem"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the postmortem ring respects its configured budget and reports the
/// number of dropped events.
#[test]
fn test_cli_debug_postmortem_ring_truncation() {
    let dir = make_cli_test_dir("elephc_cli_debug_postmortem_ring");
    fs::write(
        dir.join("main.php"),
        "<?php\n$calls = 0;\nfunction tick(int $i): int { global $calls; $calls++; return $calls + $i; }\nfunction run(): void {\n    for ($i = 0; $i < 20; $i++) {\n        tick($i);\n    }\n    throw new RuntimeException(\"many\", 7);\n}\nrun();\n",
    )
    .expect("write postmortem ring fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile postmortem ring fixture");
    assert!(compile.status.success());
    let document = dir.join("postmortem.json");
    let run = std::process::Command::new(dir.join("main"))
        .env("ELEPHC_DEBUG_POSTMORTEM", &document)
        .env("ELEPHC_DEBUG_CALLS_MAX", "3")
        .output()
        .expect("run postmortem ring fixture");
    assert!(!run.status.success(), "uncaught exception must fail");
    let postmortem: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&document).expect("postmortem ring document"),
    )
    .expect("valid postmortem ring JSON");
    assert_eq!(postmortem["captured"], 3, "{postmortem}");
    assert!(
        postmortem["dropped"].as_u64().unwrap_or(0) > 0,
        "{postmortem}"
    );
    let names: Vec<&str> = postmortem["calls"]
        .as_array()
        .expect("ring calls")
        .iter()
        .filter_map(|call| call["function"].as_str())
        .collect();
    assert_eq!(names, ["main", "run", "tick"], "{postmortem}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the `elephc debug --postmortem` launcher carries the destination into
/// the runtime rather than only supporting the environment variable directly.
#[test]
fn test_cli_debug_postmortem_flag_launches_and_writes() {
    let dir = make_cli_test_dir("elephc_cli_debug_postmortem_flag");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction boom(): void { throw new RuntimeException(\"flag\", 9); }\nboom();\n",
    )
    .expect("write postmortem flag fixture");
    let mut child = elephc_cli_command(&dir)
        .args(["debug", "main.php", "--postmortem", "pm.json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn debug --postmortem");
    {
        use std::io::Write as _;
        child
            .stdin
            .as_mut()
            .expect("debug stdin")
            .write_all(b"c\n")
            .expect("continue debug --postmortem");
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for debug --postmortem");
    assert!(!output.status.success(), "uncaught exception must fail");
    let postmortem: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("pm.json")).expect("debug --postmortem file"),
    )
    .expect("valid debug --postmortem JSON");
    assert_eq!(postmortem["exception"]["message"], "flag", "{postmortem}");
    assert_eq!(postmortem["exception"]["code"], 9, "{postmortem}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies trace segments and dumps are active in an interactive debug session
/// even without `ELEPHC_DEBUG_CALLS`, and that the dump is scoped to the segment.
#[test]
fn test_cli_debug_traces_and_dumps_are_active_in_interactive_session() {
    let dir = make_cli_test_dir("elephc_cli_debug_interactive_traces");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction run(): void {\n    elephc_trace_start(\"work\");\n    elephc_dump(\"x\", \"snap\");\n    elephc_trace_end(\"work\");\n    echo \"done\\n\";\n}\nrun();\n",
    )
    .expect("write interactive trace fixture");
    let mut child = elephc_cli_command(&dir)
        .args(["debug", "main.php"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn interactive trace session");
    {
        use std::io::Write as _;
        child
            .stdin
            .as_mut()
            .expect("debug stdin")
            .write_all(b"c\n")
            .expect("continue interactive trace session");
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for interactive traces");
    assert!(output.status.success(), "interactive trace session failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.ends_with("done\n"), "{stdout}");
    assert!(stderr.contains("elephc-trace: start work"), "{stderr}");
    assert!(
        stderr.contains("[work] elephc_dump[snap]: string(1) \"x\""),
        "{stderr}"
    );
    assert!(stderr.contains("elephc-trace: end work"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an empty trace segment name raises ValueError before the native
/// event recorder runs.
#[test]
fn test_cli_debug_trace_empty_name_raises_value_error() {
    let dir = make_cli_test_dir("elephc_cli_debug_trace_empty_name");
    fs::write(
        dir.join("main.php"),
        "<?php\nelephc_trace_start(\"\");\n",
    )
    .expect("write empty trace fixture");
    let compile = elephc_cli_command(&dir)
        .args(["--debug", "main.php"])
        .output()
        .expect("compile empty trace fixture");
    assert!(compile.status.success());
    let run = std::process::Command::new(dir.join("main"))
        .output()
        .expect("run empty trace fixture");
    assert!(!run.status.success(), "empty segment name must throw");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(output.contains("ValueError"), "{output}");
    assert!(
        output.contains("elephc_trace_start(): Argument #1 ($name) must not be empty"),
        "{output}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `--calls` always carries the exact profile and still writes the
/// Chrome/Perfetto trace requested by `--trace`.
#[test]
fn test_cli_monitor_calls_exact_profile_and_trace() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_trace");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction work(int $n): int {\n    $sum = 0;\n    for ($i = 0; $i < $n; $i++) {\n        $sum += $i;\n    }\n    return $sum;\n}\necho work(100), \"\\n\";\n",
    )
    .expect("write monitor calls trace fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=scalars",
            "--trace",
            "trace.json",
        ])
        .output()
        .expect("run monitor --calls --trace");
    assert!(
        output.status.success(),
        "monitor --calls --trace failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("exact profile —"), "{stdout}");
    assert!(stdout.contains("work($n = int(100)) -> int(4950)"), "{stdout}");
    assert!(stderr.contains("trace written to trace.json"), "{stderr}");
    let trace = fs::read_to_string(dir.join("trace.json")).expect("trace.json");
    assert!(trace.starts_with("{\"traceEvents\":["), "{trace}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies every captured call carries the caller source site, timestamp,
/// running fiber, allocation/free deltas and per-call exception state, and that
/// the unified Calls panel renders them.
#[test]
fn test_cli_monitor_calls_capture_source_fiber_exception_and_alloc_delta() {
    let dir = make_cli_test_dir("elephc_cli_monitor_calls_rich_fields");
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction work(int $n): int {\n    $buffer = str_repeat(\"x\", $n);\n    return strlen($buffer);\n}\nfunction throwing(): void {\n    throw new RuntimeException(\"boom\", 7);\n}\nfunction fiberBody(): void {\n    try {\n        throwing();\n    } catch (RuntimeException $error) {\n        echo \"caught\\n\";\n    }\n}\n$result = work(3);\n$fiber = new Fiber('fiberBody');\n$fiber->start();\necho $result, \"\\n\";\n",
    )
    .expect("write rich call-field fixture");
    let output = elephc_cli_command(&dir)
        .args([
            "monitor",
            "main.php",
            "--calls=scalars",
            "--calls-out",
            "events.jsonl",
            "--html",
            "panel.html",
        ])
        .output()
        .expect("run rich call-field capture");
    assert!(
        output.status.success(),
        "rich call-field capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("caught"), "program output missing: {stdout}");
    assert!(stdout.contains("3"), "program output missing: {stdout}");

    let text = fs::read_to_string(dir.join("events.jsonl")).expect("events.jsonl");
    let events: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid call JSONL line"))
        .collect();
    let work = events
        .iter()
        .find(|event| event["kind"] == "call" && event["function"] == "work")
        .unwrap_or_else(|| panic!("missing work call: {events:#?}"));
    assert_eq!(work["caller"]["file"], "main.php", "{work}");
    assert_eq!(work["caller"]["line"], 16, "{work}");
    assert!(work["timestamp_us"].is_u64(), "{work}");
    assert_eq!(work["fiber"], 0, "{work}");
    assert!(work["allocs"].is_u64(), "{work}");
    assert!(work["frees"].is_u64(), "{work}");
    assert!(work["alloc_delta"].is_u64(), "{work}");
    assert!(work["free_delta"].is_u64(), "{work}");
    assert!(work["end_timestamp_us"].is_u64(), "{work}");

    let throwing = events
        .iter()
        .find(|event| event["kind"] == "call" && event["function"] == "throwing")
        .unwrap_or_else(|| panic!("missing throwing call: {events:#?}"));
    assert!(
        throwing["fiber"].as_u64().unwrap_or(0) > 0,
        "the throwing call must carry its fiber id: {throwing}"
    );
    assert_eq!(throwing["exception"]["class"], "RuntimeException", "{throwing}");
    assert_eq!(throwing["exception"]["message"], "boom", "{throwing}");
    assert_eq!(throwing["exception"]["code"], 7, "{throwing}");
    assert_eq!(throwing["exception"]["caught"], true, "{throwing}");

    let html = fs::read_to_string(dir.join("panel.html")).expect("panel.html");
    assert!(
        html.contains("<title>elephc monitor — profile + calls</title>"),
        "{html}"
    );
    assert!(html.contains("call-meta"), "call metadata missing: {html}");
    assert!(html.contains("main.php:16"), "caller site missing: {html}");
    assert!(html.contains("fiber "), "fiber id missing: {html}");
    assert!(html.contains("alloc / -"), "allocation delta missing: {html}");
    assert!(html.contains("exception"), "exception state missing: {html}");
    assert!(html.contains("RuntimeException"), "{html}");
    assert!(html.contains("caught"), "{html}");

    let _ = fs::remove_dir_all(&dir);
}
