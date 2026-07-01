//! Purpose:
//! Integration tests verifying that the Windows x86_64 cross-compilation target
//! produces valid, runnable PE32+ executables. Compile-only tests require the
//! MinGW-w64 toolchain (`x86_64-w64-mingw32-as`, `x86_64-w64-mingw32-gcc`) and are
//! skipped when it is not available. Execution tests additionally require Wine
//! (`wine64` or `wine`) to run the cross-compiled binary and are skipped when
//! Wine is not available.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Compile-only tests assemble + link and validate PE32+ output via `file`.
//! - Execution tests run the produced `.exe` under Wine and assert exact stdout,
//!   which is the only signal that catches syscall/ABI regressions — compile-only
//!   checks cannot detect those.
//! - Uses the CLI directly with `--target windows-x86_64`.

use crate::support::*;

/// Checks whether the MinGW-w64 x86_64 toolchain is available on the host.
/// Returns `true` if `x86_64-w64-mingw32-gcc` is found in PATH.
fn has_mingw() -> bool {
    Command::new("x86_64-w64-mingw32-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compiles a PHP source string to a Windows PE32+ binary and verifies the output.
/// Skips the test if MinGW-w64 is not installed.
fn compile_windows_pe(source: &str) {
    if !has_mingw() {
        eprintln!("skipping Windows PE test: x86_64-w64-mingw32-gcc not found");
        return;
    }
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("elephc_win_test_{}_{}", std::process::id(), id));
    fs::create_dir_all(&dir).expect("create temp dir");
    let php_path = dir.join("test.php");
    fs::write(&php_path, source).expect("write php source");

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("windows-x86_64")
        .arg(&php_path)
        .output()
        .expect("run elephc");

    assert!(
        output.status.success(),
        "elephc failed to compile for windows-x86_64:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let exe_path = dir.join("test.exe");
    assert!(
        exe_path.exists(),
        "expected output binary '{}' does not exist",
        exe_path.display()
    );

    let file_output = Command::new("file")
        .arg(&exe_path)
        .output()
        .expect("run file");

    let file_str = String::from_utf8_lossy(&file_output.stdout).to_string();
    let _ = fs::remove_dir_all(&dir);
    assert!(
        file_str.contains("PE32+"),
        "expected PE32+ executable, got: {}",
        file_str
    );
    assert!(
        file_str.contains("x86-64"),
        "expected x86-64 architecture, got: {}",
        file_str
    );
}

/// Verifies that a simple `echo "hello"` compiles to a valid Windows PE32+ binary.
#[test]
fn test_windows_echo_hello() {
    compile_windows_pe("<?php echo 'hello';");
}

/// Verifies that an arithmetic expression compiles to a valid Windows PE32+ binary.
#[test]
fn test_windows_arithmetic() {
    compile_windows_pe("<?php echo 1 + 2 * 3;");
}

/// Verifies that a function call compiles to a valid Windows PE32+ binary.
#[test]
fn test_windows_function_call() {
    compile_windows_pe("<?php function add($a, $b) { return $a + $b; } echo add(1, 2);");
}

/// Verifies that a loop construct compiles to a valid Windows PE32+ binary.
#[test]
fn test_windows_loop() {
    compile_windows_pe("<?php for ($i = 0; $i < 3; $i++) { echo $i; } echo 'done';");
}

/// Verifies that string concatenation compiles to a valid Windows PE32+ binary.
#[test]
fn test_windows_string_concat() {
    compile_windows_pe("<?php echo 'Hello' . ' ' . 'World';");
}

/// Checks whether Wine is available on the host, needed to actually execute
/// cross-compiled Windows PE binaries. Tries `wine64` first (the native 64-bit
/// loader), then falls back to `wine` (which also runs PE32+ binaries on most
/// modern distros). Returns `true` if either responds to `--version`.
fn has_wine() -> bool {
    Command::new("wine64")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        || Command::new("wine")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// Returns the preferred Wine binary name: `wine64` if present, else `wine`.
fn wine_binary() -> &'static str {
    if Command::new("wine64")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "wine64"
    } else {
        "wine"
    }
}

/// Compiles a PHP source string to a Windows PE32+ binary, executes it under
/// Wine, and asserts BOTH that its stdout matches `expected_stdout` and that its
/// process exit code equals `expected_code`. Skips the test if MinGW-w64 or Wine
/// is not installed. This is the primary regression net for Windows syscall/ABI
/// codegen bugs (WriteFile/ReadFile lowering, ExitProcess exit-code propagation),
/// since compile-only tests never run the produced binary and cannot catch them.
fn compile_and_run_windows_expect_code(source: &str, expected_stdout: &str, expected_code: i32) {
    if !has_mingw() || !has_wine() {
        eprintln!("skipping Windows PE execution test: mingw-w64 or wine not found");
        return;
    }
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("elephc_win_run_{}_{}", std::process::id(), id));
    fs::create_dir_all(&dir).expect("create temp dir");
    let php_path = dir.join("test.php");
    fs::write(&php_path, source).expect("write php source");

    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg("windows-x86_64")
        .arg(&php_path)
        .output()
        .expect("run elephc");

    assert!(
        output.status.success(),
        "elephc failed to compile for windows-x86_64:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let exe_path = dir.join("test.exe");
    assert!(
        exe_path.exists(),
        "expected output binary '{}' does not exist",
        exe_path.display()
    );

    let wine_bin = wine_binary();
    let run_result = Command::new(wine_bin)
        .arg(&exe_path)
        .env("WINEDEBUG", "-all")
        .current_dir(&dir)
        .output();

    let run_output = match run_result {
        Ok(o) => o,
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            panic!(
                "failed to execute '{}' under {}: {}",
                exe_path.display(),
                wine_bin,
                e
            );
        }
    };

    let stdout = String::from_utf8_lossy(&run_output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run_output.stderr).to_string();
    let actual_code = run_output.status.code();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        actual_code,
        Some(expected_code),
        "windows binary exited with code {:?}, expected {} under {}\nstdout: {:?}\nstderr: {:?}",
        actual_code,
        expected_code,
        wine_bin,
        stdout,
        stderr
    );

    // The elephc `echo` runtime path does not append a trailing newline, but
    // tolerate one here in case Wine's console emulation adds one, so this
    // helper stays robust to that Wine-specific detail rather than the compiler's.
    let normalized = stdout.strip_suffix('\n').unwrap_or(&stdout);
    assert_eq!(
        normalized, expected_stdout,
        "unexpected stdout from windows binary under {}\nfull stdout: {:?}\nstderr: {:?}",
        wine_bin, stdout, stderr
    );
}

/// Compiles a PHP source string to a Windows PE32+ binary, runs it under Wine,
/// and asserts its stdout matches `expected_stdout` and it exits successfully
/// (code 0). Thin wrapper over `compile_and_run_windows_expect_code`.
fn compile_and_run_windows(source: &str, expected_stdout: &str) {
    compile_and_run_windows_expect_code(source, expected_stdout, 0);
}

/// Verifies that `echo 'hello'` produces exactly "hello" on stdout when the
/// cross-compiled Windows binary is executed under Wine, proving the
/// WriteFile-based echo syscall shim works end-to-end on the real ABI.
#[test]
fn test_windows_run_echo_hello() {
    compile_and_run_windows("<?php echo 'hello';", "hello");
}

/// Verifies that arithmetic evaluation and the integer-to-string conversion
/// path produce correct output when run natively on Windows under Wine.
#[test]
fn test_windows_run_arithmetic() {
    compile_and_run_windows("<?php echo 1 + 2 * 3;", "7");
}

/// Verifies that string concatenation produces correct output when run
/// natively on Windows under Wine.
#[test]
fn test_windows_run_string_concat() {
    compile_and_run_windows("<?php echo 'Hello' . ' ' . 'World';", "Hello World");
}

/// Verifies that a for-loop with repeated echo calls produces correct,
/// ordered output when run natively on Windows under Wine.
#[test]
fn test_windows_run_loop() {
    compile_and_run_windows(
        "<?php for ($i=0;$i<3;$i++){ echo $i; } echo 'done';",
        "012done",
    );
}

/// Verifies that a user-defined function call and return value produce
/// correct output when run natively on Windows under Wine.
#[test]
fn test_windows_run_function_call() {
    compile_and_run_windows(
        "<?php function add($a,$b){ return $a+$b; } echo add(1,2);",
        "3",
    );
}

/// Verifies the file I/O round-trip end-to-end under Wine: writes "abc" to a
/// relative file (landing in the test's isolated temp cwd), reads it back, prints
/// the contents and its length, then removes it. Exercises the CreateFileA /
/// WriteFile / ReadFile / CloseHandle / DeleteFileA shims (open/write/close/read/
/// stat/unlink). Expected value locked from the native host run (`abc 3`).
#[test]
fn test_windows_run_file_roundtrip() {
    compile_and_run_windows(
        "<?php file_put_contents('t.txt','abc'); echo file_get_contents('t.txt'); echo ' '; echo strlen(file_get_contents('t.txt')); unlink('t.txt');",
        "abc 3",
    );
}

/// Verifies a heap-growth loop (100 string concatenations) so the HeapAlloc-based
/// allocation path (`__rt_sys_brk` shim) and reallocation are exercised end-to-end
/// under Wine. Expected value locked from the native host run (`100`).
#[test]
fn test_windows_run_heap_loop() {
    compile_and_run_windows(
        "<?php $s=''; for($i=0;$i<100;$i++){ $s .= 'x'; } echo strlen($s);",
        "100",
    );
}

/// Verifies array construction, foreach iteration, and integer accumulation run
/// correctly under Wine (array runtime alongside the WriteFile echo path).
/// Expected value locked from the native host run (`10`).
#[test]
fn test_windows_run_array_sum() {
    compile_and_run_windows(
        "<?php $a=[1,2,3,4]; $t=0; foreach($a as $v){ $t += $v; } echo $t;",
        "10",
    );
}

/// Verifies integer division (`intdiv`) end-to-end under Wine. Expected value
/// locked from the native host run (`12`).
#[test]
fn test_windows_run_intdiv() {
    compile_and_run_windows("<?php echo intdiv(100, 8);", "12");
}

/// Verifies that `exit(42)` propagates a non-zero process exit code through the
/// ExitProcess shim while still flushing prior stdout — asserts both stdout `x`
/// and exit code `42`. Values locked from the native host run (stdout `x`,
/// exit 42).
#[test]
fn test_windows_run_exit_code() {
    compile_and_run_windows_expect_code("<?php echo 'x'; exit(42);", "x", 42);
}

/// Verifies `random_bytes(16)` runs end-to-end on Windows under Wine, proving the
/// getrandom syscall→shim transform (`__rt_sys_getrandom` = BCryptGenRandom) fills
/// the requested number of bytes: `strlen(random_bytes(16))` prints `16`.
#[test]
fn test_windows_run_random_bytes_length() {
    compile_and_run_windows("<?php echo strlen(random_bytes(16));", "16");
}