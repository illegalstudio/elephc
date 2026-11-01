//! Purpose:
//! Compile-only regressions for process-control and platform-specific compatibility surfaces.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PCNTL and Windows-only SAPI calls must lower in runtime-dead library bodies while retaining
//!   an explicit fatal diagnostic if those stubs are ever reached.
//! - Windows-only SAPI names remain absent from `function_exists()` on supported macOS/Linux
//!   targets even though direct calls are recognized for static type checking.

use crate::support::*;

/// Verifies `getmypid()` calls the host process API and returns a positive boxed integer.
#[test]
fn test_getmypid_returns_positive_process_id() {
    let out = compile_and_run("<?php echo getmypid() > 0 ? 'yes' : 'no';");
    assert_eq!(out, "yes");
}

/// Verifies `posix_kill()` accepts the boxed getmypid result and maps signal-zero success to true.
#[test]
fn test_posix_kill_signal_zero_checks_current_process() {
    let out = compile_and_run(
        "<?php echo posix_kill(getmypid(), 0) ? 'reachable' : 'missing';",
    );
    assert_eq!(out, "reachable");
}

/// Verifies the PCNTL calls a signal-installing helper makes run: the extension is supported, so
/// none of them lowers to an unsupported-runtime stub any more.
#[test]
fn test_pcntl_signal_installation_runs() {
    let out = compile_and_run(
        r#"<?php
function installSignals(): void {
    pcntl_async_signals(true);
    $previous = pcntl_signal_get_handler(SIGTERM);
    pcntl_signal(SIGTERM, $previous);
    pcntl_alarm(1);
    pcntl_alarm(0);
}
installSignals();
echo "installed";
"#,
    );
    assert_eq!(out, "installed");
}

/// Verifies Windows-only SAPI names are absent on supported targets, matching native PHP.
#[test]
fn test_windows_sapi_functions_report_unavailable() {
    let out = compile_and_run(
        "<?php echo function_exists('sapi_windows_cp_get') ? 'present' : 'absent';",
    );
    assert_eq!(out, "absent");
}

/// Verifies residual direct Windows SAPI calls in vendor bodies compile to php's own answer on
/// these targets: the functions do not exist, so a call raises `Call to undefined function`.
#[test]
fn test_windows_sapi_calls_compile_to_undefined_function_errors() {
    let source = r#"<?php
function windowsConsoleHelpers($stream): void {
    $cp = sapi_windows_cp_get();
    sapi_windows_cp_set($cp);
    sapi_windows_vt100_support($stream, true);
    sapi_windows_cp_conv('oem', $cp, 'text');
}
// Reachable, so the body is emitted; never taken, so nothing raises.
if ($argc > 99) { windowsConsoleHelpers(STDOUT); }
"#;
    let dir = make_cli_test_dir("windows_sapi_runtime_fatal_stubs");
    let (user_asm, _runtime_asm, _libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    for name in ["sapi_windows_cp_get", "sapi_windows_cp_set", "sapi_windows_vt100_support", "sapi_windows_cp_conv"] {
        assert!(
            user_asm.contains(&format!("Call to undefined function {name}()")),
            "{name}: {user_asm}"
        );
    }
}
