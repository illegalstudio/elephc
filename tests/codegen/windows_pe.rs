//! Purpose:
//! Integration tests verifying that the Windows x86_64 cross-compilation target
//! produces valid, runnable PE32+ executables. Compile-only tests require the
//! MinGW-w64 toolchain (`x86_64-w64-mingw32-as`, `x86_64-w64-mingw32-gcc`) and are
//! skipped when it is not available. Execution tests run directly on a Windows
//! host and otherwise require Wine (`wine64` or `wine`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Compile-only tests assemble + link and validate PE32+ output via `file`.
//! - Execution tests run the produced `.exe` natively or under Wine and assert exact stdout,
//!   which is the only signal that catches syscall/ABI regressions — compile-only
//!   checks cannot detect those.
//! - Uses the CLI directly with `--target windows-x86_64`.

use crate::support::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Stdio;
use std::time::{Duration, Instant};

// `has_mingw`, `has_wine`, and `wine_binary` are shared with the parameterized
// codegen harness and live in `crate::support` (imported via the glob above), so
// there is a single source of truth for MinGW/Wine detection.

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

/// Verifies `--web` cross-builds its Rust bridge, links a PE executable without
/// force-loading duplicate Windows import objects, and retains the generated-to-
/// bridge ABI exports used by request and response helpers.
#[test]
fn test_windows_web_bridge_compiles_and_exports() {
    if !has_mingw() {
        eprintln!("skipping Windows web bridge test: MinGW-w64 not found");
        return;
    }
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_win_web_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("create Windows web temp dir");
    let php_path = dir.join("test.php");
    fs::write(
        &php_path,
        "<?php header('Content-Type: text/plain'); if (!isset($_GET['name'])) { \
         http_response_code(400); echo \"missing 'name' query parameter\\n\"; } else { \
         header('X-Powered-By: elephc'); echo 'Hello, ' . $_GET['name'] . \"!\\n\"; }",
    )
    .expect("write Windows web PHP source");

    let compile = elephc_cli_command(&dir)
        .arg("--target")
        .arg("windows-x86_64")
        .arg("--web")
        .arg(&php_path)
        .output()
        .expect("run elephc Windows web compilation");
    assert!(
        compile.status.success(),
        "Windows --web compilation failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let exe_path = dir.join("test.exe");
    assert!(exe_path.exists(), "Windows web PE was not produced");
    let symbols = Command::new("x86_64-w64-mingw32-nm")
        .arg("-g")
        .arg(&exe_path)
        .output()
        .expect("inspect Windows web PE exports");
    assert!(symbols.status.success(), "MinGW nm could not inspect web PE");
    let symbols = String::from_utf8_lossy(&symbols.stdout);
    for symbol in [
        "elephc_web_run",
        "elephc_web_write",
        "elephc_web_set_status",
        "elephc_web_header",
        "elephc_web_body_len",
        "elephc_web_body_ptr",
    ] {
        assert!(symbols.contains(symbol), "missing {symbol} in Windows web PE");
    }

    if cfg!(windows) || has_wine() {
        let probe = TcpListener::bind("127.0.0.1:0").expect("reserve Windows web test port");
        let port = probe.local_addr().expect("read Windows web test port").port();
        drop(probe);
        let mut command = if cfg!(windows) {
            Command::new(&exe_path)
        } else {
            let mut command = Command::new(wine_binary());
            command.arg(&exe_path).env("WINEDEBUG", "-all");
            command
        };
        let mut child = command
            .args([
                "--listen",
                &format!("127.0.0.1:{port}"),
                "--workers",
                "2",
                "--max-requests",
                "2",
            ])
            .current_dir(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start Windows web PE");

        let request = |path: &str| -> String {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut stream = loop {
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(stream) => break stream,
                    Err(error) if Instant::now() < deadline => {
                        let _ = error;
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(error) => panic!("Windows web PE did not listen: {error}"),
                }
            };
            write!(
                stream,
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
            )
            .expect("write Windows web request");
            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .expect("read Windows web response");
            response
        };

        let ok = request("/?name=ada");
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.to_ascii_lowercase().contains("x-powered-by: elephc"), "{ok}");
        assert!(ok.ends_with("Hello, ada!\n"), "{ok}");
        let missing = request("/");
        assert!(missing.starts_with("HTTP/1.1 400"), "{missing}");
        assert!(missing.ends_with("missing 'name' query parameter\n"), "{missing}");

        let deadline = Instant::now() + Duration::from_secs(10);
        let exit = loop {
            if let Some(status) = child.try_wait().expect("poll Windows web PE") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("Windows web PE did not stop after --max-requests 2");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(exit.success(), "Windows web PE exited with {exit}");
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies PE loader mitigations and generated-frame unwind records using the
/// MinGW inspection tools, without executing the resulting binary.
#[test]
fn test_windows_pe_hardening_and_generated_unwind_metadata() {
    if !has_mingw() {
        eprintln!("skipping Windows PE hardening test: MinGW-w64 not found");
        return;
    }
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_win_hardening_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("create hardening temp dir");
    let php_path = dir.join("test.php");
    fs::write(
        &php_path,
        "<?php function hardened($x) { return $x + 1; } echo hardened(1);",
    )
    .expect("write hardening PHP source");

    let compile = elephc_cli_command(&dir)
        .arg("--target")
        .arg("windows-x86_64")
        .arg(&php_path)
        .output()
        .expect("run elephc for PE hardening inspection");
    assert!(
        compile.status.success(),
        "elephc failed to compile hardened PE fixture:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let exe_path = dir.join("test.exe");
    let asm_path = dir.join("test.s");
    let object_path = dir.join("unwind-audit.o");
    let assembly = fs::read_to_string(&asm_path).expect("read retained Windows assembly");
    let hardened_region = assembly
        .split_once("# @fn name=hardened symbol=_fn_hardened")
        .and_then(|(_, region)| region.split_once("# @endfn name=hardened"))
        .map(|(region, _)| region)
        .expect("locate generated hardened function assembly region");
    assert!(hardened_region.contains(".globl _fn_hardened"));
    assert!(hardened_region.contains(".seh_proc "));
    assert!(hardened_region.contains(".seh_pushreg rbp"));
    assert!(hardened_region.contains(".seh_stackalloc"));
    assert!(hardened_region.contains(".seh_endprologue"));
    assert!(hardened_region.contains(".seh_endproc"));

    let assembled = Command::new("x86_64-w64-mingw32-as")
        .arg("-o")
        .arg(&object_path)
        .arg(&asm_path)
        .output()
        .expect("assemble retained Windows user assembly");
    assert!(
        assembled.status.success(),
        "MinGW assembler rejected generated unwind metadata:\n{}",
        String::from_utf8_lossy(&assembled.stderr)
    );
    let object_headers = Command::new("x86_64-w64-mingw32-objdump")
        .arg("-x")
        .arg(&object_path)
        .output()
        .expect("inspect generated Windows object unwind records");
    let object_headers = String::from_utf8_lossy(&object_headers.stdout);
    assert!(object_headers.contains(".pdata"), "missing .pdata:\n{object_headers}");
    assert!(object_headers.contains(".xdata"), "missing .xdata:\n{object_headers}");
    assert!(
        object_headers.contains("The Function Table") && object_headers.contains("UnwindData"),
        "missing decoded PE function table:\n{object_headers}"
    );
    assert!(
        object_headers.contains("_fn_hardened"),
        "generated function is absent from the unwind-bearing object:\n{object_headers}"
    );

    let image_headers = Command::new("x86_64-w64-mingw32-objdump")
        .arg("-x")
        .arg(&exe_path)
        .output()
        .expect("inspect linked PE loader flags");
    let image_headers = String::from_utf8_lossy(&image_headers.stdout);
    for mitigation in ["HIGH_ENTROPY_VA", "DYNAMIC_BASE", "NX_COMPAT"] {
        assert!(
            image_headers.contains(mitigation),
            "linked PE is missing {mitigation}:\n{image_headers}"
        );
    }
    assert!(
        !image_headers.contains("GUARD_CF"),
        "GNU-linked PE must not advertise CFG until it carries call-site instrumentation and a valid guard function table:\n{image_headers}"
    );
    let uses_llvm = matches!(
        std::env::var("ELEPHC_WINDOWS_TOOLCHAIN").as_deref(),
        Ok("llvm" | "gnullvm")
    );
    if !uses_llvm {
        assert!(
            image_headers
                .contains("Entry a 0000000000000000 00000000 Load Configuration Directory"),
            "unexpected GNU PE load configuration without an audited Guard CF table:\n{image_headers}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
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

/// Verifies cross-function exception paths assemble and link with PE unwind tables.
#[test]
fn test_windows_cross_function_exception_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php
function fail(): void {
    throw new Exception("boom");
}
try {
    fail();
} catch (Exception $error) {
    echo $error->getMessage();
}
"#,
    );
}

/// Verifies that the manual Fiber stack switch, including its Windows x64 TEB
/// metadata resynchronization, assembles and links into a PE32+ executable.
#[test]
fn test_windows_fiber_suspend_resume_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php
$fiber = new Fiber(function (): void {
    $value = Fiber::suspend("ready");
    echo $value;
});
echo $fiber->start();
$fiber->resume("done");
"#,
    );
}

/// Verifies the Windows socketpair rejection path assembles and links.
#[test]
fn test_windows_stream_socket_pair_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
fwrite($pair[0], "ping");
echo fread($pair[1], 4);
fclose($pair[0]);
fclose($pair[1]);
"#,
    );
}

/// Verifies PHP/Windows transport discovery, netdb edge cases, and Unix-domain
/// socket failures.
#[test]
fn test_windows_does_not_expose_unix_socket_transports() {
    compile_and_run_windows(
        r#"<?php
$transports = stream_get_transports();
echo in_array("unix", $transports, true) ? "1" : "0";
echo in_array("udg", $transports, true) ? "1" : "0";
echo in_array("sslv2", $transports, true) ? "1" : "0";
echo in_array("sslv3", $transports, true) ? "1" : "0";
echo getservbyname("http", "") === false ? "G" : "g";
echo stream_socket_server("unix://elephc.sock") === false ? "S" : "s";
echo stream_socket_client("udg://elephc.sock") === false ? "C" : "c";
echo stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0) === false ? "P" : "p";
"#,
        "0000GSCP",
    );
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

/// Verifies multi-digit `number_format()` precision assembles and links through
/// the Windows precision-plus-double variadic `snprintf` bridge.
#[test]
fn test_windows_number_format_high_precision_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php echo number_format(0.285, 23, ".", ""), number_format(0.285, 30, ".", "");"#,
    );
}

/// Verifies the TLS bridge, its C exports, and the existing-socket STARTTLS
/// attach path link into a Windows PE binary without requiring Wine or network.
#[test]
fn test_windows_tls_socket_attach_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php
$socket = @stream_socket_client("tcp://127.0.0.1:9");
if ($socket !== false) {
    stream_socket_enable_crypto($socket, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
}"#,
    );
}

/// Verifies a silent local TCP peer sets Windows stream timeout metadata without EOF.
#[test]
fn test_windows_stream_timeout_sets_metadata_without_eof() {
    if !has_mingw() || (!cfg!(windows) && !has_wine()) {
        eprintln!("skipping Windows timeout execution test: Windows runner unavailable");
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind silent TCP peer");
    let port = listener.local_addr().expect("read silent TCP port").port();
    let server = std::thread::spawn(move || {
        let (_stream, _) = listener.accept().expect("accept Windows test client");
        std::thread::sleep(Duration::from_millis(250));
    });
    compile_and_run_windows(
        &format!(
            "<?php\n$s = stream_socket_client(\"tcp://127.0.0.1:{port}\");\nstream_set_timeout($s, 0, 50000);\nfread($s, 1);\n$m = stream_get_meta_data($s);\necho ($m[\"timed_out\"] ? \"1\" : \"0\") . ($m[\"eof\"] ? \"1\" : \"0\");\nfclose($s);"
        ),
        "10",
    );
    server.join().expect("join silent TCP peer");
}

/// Verifies MSx64 stack overflow arguments assemble and link for scalar, float,
/// string, object, Mixed, direct, method, constructor, and callable call paths.
#[test]
fn test_windows_owned_stack_argument_matrix_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php
class StackOwned {
    public string $label;
    public function __construct(int $a, int $b, int $c, string $label) {
        $this->label = $label;
    }
    public function select(int $a, int $b, int $c, string $label, mixed $value): mixed {
        if ($a > 0) { return $label; }
        return $value;
    }
}
function overflow_owned(int $a, int $b, int $c, int $d, string $label, StackOwned $object, mixed $value): mixed {
    if ($a > 0) { return $label; }
    if ($b > 0) { return $object; }
    return $value;
}
function overflow_float(float $a, float $b, float $c, float $d, float $e): float {
    return $a + $b + $c + $d + $e;
}
$object = new StackOwned(1, 2, 3, "ctor");
$callable = overflow_owned(...);
$callable(1, 2, 3, 4, "callable", $object, ["mixed"]);
overflow_owned(0, 1, 2, 3, "direct", $object, ["mixed"]);
$object->select(1, 2, 3, "method", ["mixed"]);
overflow_float(1.0, 2.0, 3.0, 4.0, 5.0);
"#,
    );
}

/// Verifies a native MSx64 extern callback trampoline, including descriptor
/// dispatch through internal helpers, assembles and links into a PE executable.
#[test]
fn test_windows_ffi_descriptor_callback_compiles_to_pe() {
    compile_windows_pe(
        r#"<?php
extern function signal(int $signal, callable $handler): ptr;
function handle_signal(int $signal): void {}
$handler = handle_signal(...);
signal(15, $handler);
"#,
    );
}

/// Verifies the Win64 receiver/vtable, descriptor-callback, stack-overflow,
/// destructor, and nested-array ownership paths execute with PHP-equivalent output.
#[test]
fn test_windows_spl_oop_gc_abi_matrix_runs() {
    compile_and_run_windows(
        r#"<?php
class WindowsSplBox implements ArrayAccess {
    private array $items = [];
    public int $limit = 2;

    public function offsetExists(mixed $offset): bool { return isset($this->items[$offset]); }
    public function offsetGet(mixed $offset): mixed { return $this->items[$offset]; }
    public function offsetSet(mixed $offset, mixed $value): void { $this->items[$offset] = $value; }
    public function offsetUnset(mixed $offset): void { $this->items = []; }
    public function keep(int $value): bool { return $value <= $this->limit; }
    public function overflow($a, $b, $c, $d, $e, $f, string $tail): string { return $tail; }
    public function __destruct() { echo count($this->items); }
}
class WindowsStaticFilter {
    public static int $limit = 2;
    public static function keep(int $value): bool { return $value <= static::$limit; }
}

$box = new WindowsSplBox();
$box["nested"] = [10, 11];
$alias = $box["nested"];
$filtered = array_filter([1, 2, 3], $box->keep(...));
$staticFiltered = array_filter([1, 2, 3], WindowsStaticFilter::keep(...));
echo $box->overflow(1, 2, 3, 4, 5, 6, "stack");
unset($box);
echo $alias[1], count($filtered), count($staticFiltered);
"#,
        "stack11122",
    );
}

/// Verifies destructor dispatch, heap-backed properties, overwrite cleanup,
/// and early-return cleanup execute without losing or duplicating releases.
#[test]
fn test_windows_destructor_and_early_exit_ownership_runs() {
    compile_and_run_windows(
        r#"<?php
class WindowsOwnedDestructor {
    private array $items;
    private string $tag;
    public function __construct(string $tag) {
        $this->tag = $tag;
        $this->items = ["a", "b", "c"];
    }
    public function __destruct() {
        echo $this->tag . count($this->items);
    }
}
function replace_and_return(bool $early): mixed {
    $value = new WindowsOwnedDestructor("first");
    $value = new WindowsOwnedDestructor("second");
    if ($early) { return $value; }
    return "done";
}
replace_and_return(true);
replace_and_return(false);
"#,
        "first3second3first3second3",
    );
}

/// Compiles a PHP source string to a Windows PE32+ binary, executes it natively
/// on Windows or through Wine elsewhere, and asserts stdout plus exit status.
/// This is the primary regression net for Windows syscall/ABI codegen bugs,
/// since compile-only tests never run the produced binary.
fn compile_and_run_windows_expect_code(source: &str, expected_stdout: &str, expected_code: i32) {
    compile_and_run_windows_expect_code_with_helpers(source, expected_stdout, expected_code, &[]);
}

/// Compiles optional named PE helpers beside the main Windows fixture before
/// executing it, so process tests can exercise direct argv and environment
/// transport without introducing shell parsing into their acceptance signal.
fn compile_and_run_windows_expect_code_with_helpers(
    source: &str,
    expected_stdout: &str,
    expected_code: i32,
    helpers: &[(&str, &str)],
) {
    if !has_mingw() {
        eprintln!("skipping Windows PE execution test: MinGW not found");
        return;
    }
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("elephc_win_run_{}_{}", std::process::id(), id));
    fs::create_dir_all(&dir).expect("create temp dir");
    let mut expanded_source = source.to_string();
    for (name, helper_source) in helpers {
        let helper_path = dir.join(format!("{name}.php"));
        fs::write(&helper_path, helper_source).expect("write Windows PE helper source");
        let helper_output = elephc_cli_command(&dir)
            .arg("--target")
            .arg("windows-x86_64")
            .arg(&helper_path)
            .output()
            .expect("compile Windows PE helper");
        assert!(
            helper_output.status.success(),
            "elephc failed to compile Windows PE helper '{name}':\n{}",
            String::from_utf8_lossy(&helper_output.stderr)
        );
        assert!(
            dir.join(format!("{name}.exe")).exists(),
            "expected Windows PE helper '{name}.exe' was not produced"
        );
        let helper_exe = dir
            .join(format!("{name}.exe"))
            .canonicalize()
            .expect("canonicalize Windows PE helper path");
        let helper_windows_path = if cfg!(windows) {
            helper_exe.to_string_lossy().replace('\\', "/")
        } else {
            format!("Z:{}", helper_exe.to_string_lossy())
        };
        let marker = format!(
            "__WINDOWS_HELPER_{}__",
            name.to_ascii_uppercase().replace('-', "_")
        );
        expanded_source = expanded_source.replace(&marker, &helper_windows_path);
    }
    let php_path = dir.join("test.php");
    fs::write(&php_path, expanded_source).expect("write php source");

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

    if !cfg!(windows) && !has_wine() {
        let _ = fs::remove_dir_all(&dir);
        eprintln!("Windows PE compiled successfully; skipping execution because Wine is absent");
        return;
    }

    let (run_result, runner_label) = if cfg!(windows) {
        (
            Command::new(&exe_path).current_dir(&dir).output(),
            "native Windows",
        )
    } else {
        let wine_bin = wine_binary();
        (
            Command::new(wine_bin)
                .arg(&exe_path)
                .env("WINEDEBUG", "-all")
                .current_dir(&dir)
                .output(),
            wine_bin,
        )
    };

    let run_output = match run_result {
        Ok(o) => o,
        Err(e) => {
            let _ = fs::remove_dir_all(&dir);
            panic!(
                "failed to execute '{}' with {}: {}",
                exe_path.display(),
                runner_label,
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
        "windows binary exited with code {:?}, expected {} with {}\nstdout: {:?}\nstderr: {:?}",
        actual_code,
        expected_code,
        runner_label,
        stdout,
        stderr
    );

    // The elephc `echo` runtime path does not append a trailing newline, but
    // tolerate one here in case Wine's console emulation adds one, so this
    // helper stays robust to that Wine-specific detail rather than the compiler's.
    let normalized = stdout.strip_suffix('\n').unwrap_or(&stdout);
    assert_eq!(
        normalized, expected_stdout,
        "unexpected stdout from windows binary with {}\nfull stdout: {:?}\nstderr: {:?}",
        runner_label, stdout, stderr
    );
}

/// Compiles a PHP source string to a Windows PE32+ binary, runs it natively or
/// through Wine, and asserts stdout plus a successful exit status.
fn compile_and_run_windows(source: &str, expected_stdout: &str) {
    compile_and_run_windows_expect_code(source, expected_stdout, 0);
}

/// Verifies an exception crosses a generated frame and reaches its Windows catch handler.
#[test]
fn test_windows_cross_function_exception_unwinds_at_runtime() {
    compile_and_run_windows(
        r#"<?php
function fail(): void {
    throw new Exception("boom");
}
try {
    fail();
} catch (Exception $error) {
    echo $error->getMessage();
}
"#,
        "boom",
    );
}

/// Verifies a complete Windows Fiber start/suspend/resume cycle while the fiber
/// stack performs runtime heap allocation and a CRT-backed numeric formatting call.
#[test]
fn test_windows_fiber_teb_stack_metadata_runs() {
    compile_and_run_windows(
        r#"<?php
$fiber = new Fiber(function (): void {
    $payload = str_repeat("ab", 64);
    $formatted = number_format(1234.5, 2, ".", "");
    $resumed = Fiber::suspend(strlen($payload) . ":" . $formatted);
    echo $resumed, "|";
    $decoded = json_decode('{"value":"done"}', true);
    echo $decoded["value"];
});
echo $fiber->start(), "|";
$fiber->resume("resume");
"#,
        "128:1234.50|resume|done",
    );
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

/// Verifies that `PHP_OS_FAMILY` resolves to `"Windows"` when run natively on
/// Windows under Wine, proving the constant is genuinely target-aware end-to-end
/// (not just at compile time) — the cross-platform feature-detection idiom
/// `PHP_OS_FAMILY === 'Windows'` (finding F15) depends on this.
#[test]
fn test_windows_run_php_os_family() {
    compile_and_run_windows("<?php echo PHP_OS_FAMILY;", "Windows");
}

/// Verifies the Windows PHP builtin surface omits the `HAVE_LCHOWN` functions.
#[test]
fn test_windows_run_lchown_function_availability() {
    compile_and_run_windows(
        r#"<?php
echo function_exists("chown") ? "1" : "0";
echo function_exists("chgrp") ? "1" : "0";
echo function_exists("lchown") ? "1" : "0";
echo function_exists("lchgrp") ? "1" : "0";
"#,
        "1100",
    );
}

/// Verifies runtime string-callable lookup omits the Windows-only absent
/// `HAVE_LCHOWN` functions rather than exposing the platform-neutral catalog.
#[test]
fn test_windows_run_dynamic_lchown_is_callable_availability() {
    compile_and_run_windows(
        r#"<?php
foreach (["lchown", "lchgrp"] as $callback) {
    echo is_callable($callback) ? "1" : "0";
}
"#,
        "00",
    );
}

/// Verifies that `PHP_VERSION_ID >= 80400` — the canonical feature-detection
/// idiom — evaluates true when run natively on Windows under Wine, proving the
/// emulated-PHP version constants resolve identically across targets.
#[test]
fn test_windows_run_php_version_id_feature_detection() {
    compile_and_run_windows(
        "<?php echo PHP_VERSION_ID >= 80400 ? 'yes' : 'no';",
        "yes",
    );
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
/// the contents and its length, then removes it. Exercises the CreateFileW /
/// WriteFile / ReadFile / CloseHandle / DeleteFileW shims (open/write/close/read/
/// stat/unlink). Expected value locked from the native host run (`abc 3`).
#[test]
fn test_windows_run_file_roundtrip() {
    compile_and_run_windows(
        "<?php file_put_contents('t.txt','abc'); echo file_get_contents('t.txt'); echo ' '; echo strlen(file_get_contents('t.txt')); unlink('t.txt');",
        "abc 3",
    );
}

/// Verifies the Win32 `glob` shim returns every match after its result vector
/// grows beyond the former fixed 1024-entry ceiling.
#[test]
fn test_windows_run_glob_more_than_1024_matches() {
    compile_and_run_windows(
        r#"<?php
mkdir("glob-many");
for ($i = 0; $i < 1030; $i++) {
    file_put_contents("glob-many/f" . $i . ".txt", "");
}
$matches = glob("glob-many/*.txt");
echo count($matches);
foreach ($matches as $path) {
    unlink($path);
}
rmdir("glob-many");
"#,
        "1030",
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

/// Verifies that target-aware path constants (`PHP_EOL`, `DIRECTORY_SEPARATOR`,
/// `PATH_SEPARATOR`) compile to a valid Windows PE32+ binary. Compile-only because
/// the path-constant resolution fires for the Windows target at prescan time; the
/// values themselves (`"\r\n"`, `"\\"`, `";"`) are exercised by CI under Wine.
#[test]
fn test_windows_path_constants_compile() {
    compile_windows_pe("<?php echo PHP_EOL; echo DIRECTORY_SEPARATOR; echo PATH_SEPARATOR;");
}

/// Verifies that the target-aware `PHP_OS_FAMILY` constant compiles to a valid
/// Windows PE32+ binary. Compile-only because the constant resolution fires for the
/// Windows target at prescan time; the value itself (`"Windows"`) is exercised
/// end-to-end under Wine by `test_windows_run_php_os_family`.
#[test]
fn test_windows_php_os_family_compile() {
    compile_windows_pe("<?php echo PHP_OS_FAMILY;");
}

/// Verifies that `sleep()` and `usleep()` compile to a valid Windows PE32+ binary.
/// Compile-only because the actual delay behavior is exercised by CI under Wine;
/// this catches link failures from the `sleep`/`usleep` C-symbol stubs (which
/// delegate to Win32 `Sleep`) that the shared `lower_sleep`/`lower_usleep`
/// lowering emits as `call sleep`/`call usleep`.
#[test]
fn test_windows_sleep_usleep_compile() {
    compile_windows_pe("<?php sleep(0); usleep(0); echo 'ok';");
}

/// Verifies that the Windows runtime — which now includes the `__rt_sys_getrusage`
/// shim (syscall 98 → `GetProcessTimes`) — assembles and links into a valid PE32+
/// binary. `getrusage` is not a user-visible PHP builtin in elephc, so no PHP
/// source directly triggers syscall 98; the shim is nonetheless always emitted
/// into the Windows runtime object, so any PE compile exercises it. This test is a
/// dedicated marker that catches assembly/link failures from the getrusage shim
/// (bad register use, missing `GetProcessTimes` import, frame-alignment errors).
/// Runtime behavior (non-zero process times for RUSAGE_SELF) is CI-validated.
#[test]
fn test_windows_getrusage_runtime_links() {
    compile_windows_pe("<?php echo 'ok';");
}

/// Verifies that `popen`, `pclose`, `system`, and `shell_exec` compile to a
/// valid Windows PE32+ binary. Compile-only because the actual subprocess
/// behavior is exercised by CI under Wine; this catches link failures from the
/// new `popen`/`pclose`/`fileno`/`fgetc`/`system` C-symbol stubs (which delegate
/// to msvcrt `_popen`/`_pclose`/`_fileno`/`fgetc`/`system`) that the shared
/// `popen`/`pclose`/`shell_exec`/`system` lowering emits as `call popen`/
/// `call pclose`/`call fgetc`/`call system`.
#[test]
fn test_windows_popen_pclose_system_shell_exec_compile() {
    compile_windows_pe(
        "<?php $p = popen('echo hi', 'r'); pclose($p); echo system('echo ok'); echo shell_exec('echo done');",
    );
}

/// Verifies that `stream_select` compiles to a valid Windows PE32+ binary.
/// Compile-only because `__rt_stream_select` always emits the `pselect6`
/// syscall (syscall 270) regardless of the PHP argument shape, so no specific
/// PHP path is needed to trigger it — compiling any `stream_select` call
/// validates that the new `__rt_sys_pselect6` shim (which calls ws2_32
/// `select`) assembles and links, resolving the `.extern select` import.
/// Runtime behavior (ready-descriptor writeback) is CI-validated under Wine.
#[test]
fn test_windows_stream_select_compile() {
    compile_windows_pe("<?php $r=[]; $w=[]; $e=[]; stream_select($r, $w, $e, 0); echo 'ok';");
}

/// Verifies the Windows runtime links PHP's cmd.exe shell escaping helpers into a PE program.
#[test]
fn test_windows_shell_escaping_compile() {
    compile_windows_pe(
        r#"<?php
echo escapeshellarg("a%!^&|<>()\\\""), ":";
echo escapeshellcmd("a%!^&|<>()\\\"");
"#,
    );
}

/// Executes PHP's Windows-specific cmd.exe escaping rules under the native Windows CI runner.
#[test]
fn test_windows_shell_escaping_run() {
    compile_and_run_windows(
        r#"<?php
echo escapeshellarg('a%!^&|<>()'), ':';
echo escapeshellcmd('a%!^&|<>()');
"#,
        "\"a  ^&|<>()\":a^%^!^^^&^|^<^>^(^)",
    );
}

/// Executes php-src's Windows backslash rules: internal runs pass through and only an odd final run doubles.
#[test]
fn test_windows_shell_escaping_backslash_runs_run() {
    compile_and_run_windows(
        r#"<?php
echo escapeshellarg("a\\b"), '|';
echo escapeshellarg("a\\\\b"), '|';
echo escapeshellarg("a\\\"b"), '|';
echo escapeshellarg("a\\"), '|';
echo escapeshellarg("a\\\\"), '|';
echo escapeshellarg("a\\\\\\");
"#,
        "\"a\\b\"|\"a\\\\b\"|\"a\\ b\"|\"a\\\\\"|\"a\\\\\"|\"a\\\\\\\\\"",
    );
}

/// Verifies Windows PHP command-length guards throw only the php-src-reachable `ValueError` messages.
///
/// `escapeshellarg()` replaces Windows `%`, `!`, and `"` bytes with one space,
/// so an accepted 8,189-byte argument cannot grow past the output limit.
#[test]
fn test_windows_shell_escaping_length_limit_run() {
    compile_and_run_windows(
        r#"<?php
try { escapeshellarg(str_repeat('a', 8190)); } catch (\ValueError $e) { echo $e->getMessage(), '|'; }
try { escapeshellcmd(str_repeat('a', 8190)); } catch (\ValueError $e) { echo $e->getMessage(), '|'; }
try { escapeshellarg(str_repeat('%', 8189)); } catch (\ValueError $e) { echo $e->getMessage(), '|'; }
try { escapeshellcmd(str_repeat('&', 8189)); } catch (\ValueError $e) { echo $e->getMessage(); }
"#,
        "Argument exceeds the allowed length of 8192 bytes|Command exceeds the allowed length of 8192 bytes|Escaped command exceeds the allowed length of 8192 bytes",
    );
}

/// Verifies proc_open/proc_close compile for the Windows PE target (C1c: real
/// `CreatePipe`/`CreateProcessW`, not a stub). Compile-only because there is no
/// Wine available in this harness; this catches link failures against the
/// `CreatePipe`/`CreateProcessW`/`WaitForSingleObject`/`GetExitCodeProcess`/
/// `SetHandleInformation`/`SetErrorMode` Win32 imports pulled in by
/// `__rt_proc_open`/`__rt_proc_close`, and the kind-5 mixed-free destructor
/// arm. Runtime behavior (actually spawning `cmd.exe` and reading the pipes)
/// is exercised by CI under Wine.
#[test]
fn test_windows_proc_open_close_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$r = proc_open("echo hi", [0 => ["pipe", "rb"], 1 => ["pipe", "wb"]], $pipes);
proc_close($r);
"#);
}

/// Verifies the Windows PE runtime compiles an integer pipe mode through the
/// PHP scalar-coercion/read-direction path rather than rejecting the descriptor.
#[test]
fn test_windows_proc_open_integer_pipe_mode_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$r = proc_open("cmd.exe /d /s /c exit 0", [0 => ["pipe", 1], 1 => ["pipe", "w"]], $pipes);
if ($r !== false) { proc_close($r); }
"#);
}

/// Verifies a sparse integer descriptor spec compiles through the Windows PE
/// backend, exercising the keyed `$pipes` publication/writeback ABI.
#[test]
fn test_windows_proc_open_sparse_descriptor_keys_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$process = proc_open('cmd.exe /d /s /c exit 0', [1 => ['pipe', 'w']], $pipes);
echo count($pipes);
if ($process !== false) { proc_close($process); }
"#,
    );
}

/// Compiles Windows-native non-pipe descriptor sources. This covers the
/// `CreateFileW` strict-path route, child-handle duplication for redirects and
/// supplied stream resources, and explicit `null` descriptors without
/// publishing any of those child-only handles into `$pipes`.
#[test]
fn test_windows_proc_open_child_only_descriptors_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$source = fopen('NUL', 'rb');
$process = proc_open(
    'cmd.exe /d /s /c exit 0',
    [
        0 => ['null'],
        1 => ['file', 'proc-open-output.txt', 'ab'],
        2 => ['redirect', 1],
        3 => $source,
    ],
    $pipes,
);
if ($process !== false) { proc_close($process); }
fclose($source);
"#,
    );
}

/// Compiles the Windows-only `proc_open(["socket"])` descriptor path. The
/// endpoint is deliberately a raw Winsock resource rather than a CRT fd, so
/// this catches missing imports and accidental `_open_osfhandle` adoption.
#[test]
fn test_windows_proc_open_socket_descriptor_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$process = proc_open(
    'cmd.exe /d /s /c more',
    [0 => ['socket'], 1 => ['pipe', 'w']],
    $pipes,
);
if ($process !== false) {
    stream_filter_append($pipes[0], 'string.rot13');
    fwrite($pipes[0], "socket-ok\r\n");
    fclose($pipes[0]);
    stream_set_blocking($pipes[1], true);
    echo fread($pipes[1], 64);
    fclose($pipes[1]);
    echo ':' . proc_close($process);
}
"#,
    );
}

/// Runs a filtered child exchange across the private loopback socket pair when
/// a Windows host or Wine is available. This verifies the bounded Windows
/// stream slot lets raw Winsock resources use the same filter semantics as PHP
/// streams without indexing tables by an opaque `SOCKET`. A compiled helper
/// echoes stdin byte-for-byte so shell utilities cannot normalize line endings.
#[test]
fn test_windows_proc_open_socket_descriptor_runs() {
    compile_and_run_windows_expect_code_with_helpers(
        r#"<?php
$pipes = [];
$options = ['bypass_shell' => true];
$process = proc_open(
    ['__WINDOWS_HELPER_PROC_SOCKET_HELPER__'],
    [0 => ['socket'], 1 => ['pipe', 'w']],
    $pipes,
    null,
    null,
    $options,
);
if ($process === false) { echo 'spawn-failed'; exit(1); }
if (stream_filter_append($pipes[0], 'string.rot13') === false) { echo 'filter-failed:'; }
fwrite($pipes[0], "socket-ok\r\n");
fclose($pipes[0]);
stream_set_blocking($pipes[1], true);
echo fread($pipes[1], 64);
fclose($pipes[1]);
echo ':' . proc_close($process);
"#,
        "fbpxrg-bx\r\n:0",
        0,
        &[("proc-socket-helper", "<?php echo fread(STDIN, 64);")],
    );
}

/// Verifies a 3-pipe descriptor_spec (stdin/stdout/stderr all piped) compiles
/// for the Windows PE target, covering the `STARTUPINFOW`/NUL-fill/
/// `CreateProcessW` emission path with all three standard handles wired from
/// `child_handle[]` (no `CreateFileW("NUL", ...)` redirection needed). Compile-
/// only for the same reason as `test_windows_proc_open_close_compile` (no Wine
/// available locally); the 3-pipe case is otherwise untested by that simpler
/// 2-descriptor program.
#[test]
fn test_windows_proc_open_three_pipe_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$r = proc_open(
    "echo hi",
    [0 => ["pipe", "r"], 1 => ["pipe", "w"], 2 => ["pipe", "w"]],
    $pipes
);
proc_close($r);
"#);
}

/// Compiles a quoting/Unicode corpus through the Windows Wide proc_open path:
/// non-ASCII text, nested quotes, shell metacharacters, percent expansion, and
/// trailing backslashes must all survive PHP parsing and PE assembly/linking.
#[test]
fn test_windows_proc_open_unicode_and_quoting_corpus_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$a = proc_open('echo café 日本語', [1 => ['pipe', 'w']], $pipes); proc_close($a);
$b = proc_open('echo "quoted value" ^& echo second', [1 => ['pipe', 'w']], $pipes); proc_close($b);
$c = proc_open('echo %PATH% ^| findstr Windows', [1 => ['pipe', 'w']], $pipes); proc_close($c);
$d = proc_open('echo C:\\Temp\\', [1 => ['pipe', 'w']], $pipes); proc_close($d);
"#);
}

/// Verifies a non-ASCII Windows working directory is transported through the
/// extended EIR/runtime ABI and strictly converted for CreateProcessW.
#[test]
fn test_windows_proc_open_unicode_cwd_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$p = proc_open('echo cwd', [1 => ['pipe', 'w']], $pipes, 'C:\\Données 日本語');
proc_close($p);
"#);
}

/// Verifies array commands, a Unicode/scalar environment, and bypass_shell are
/// lowered into the direct CreateProcessW ABI and link as a PE32+ executable.
#[test]
fn test_windows_proc_open_advanced_marshalling_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$a = proc_open(
    ['C:\\Program Files\\PHP\\php.exe', '-r', 'echo "café 日本語";', 'C:\\trail\\'],
    [1 => ['pipe', 'w']],
    $pipes,
    'C:\\Données 日本語',
    ['ELEPHC_TEXT' => 'été 日本語', 'EMPTY' => '', 'COUNT' => 42, 'ENABLED' => true],
    ['bypass_shell' => true]
);
proc_close($a);
"#);
}

/// Verifies reordered PHP named arguments retain source evaluation order while
/// the hidden Windows marshalling operands use canonical proc_open parameters.
#[test]
fn test_windows_proc_open_named_advanced_marshalling_compile() {
    compile_windows_pe(r#"<?php
$pipes = [];
$process = proc_open(
    options: ['bypass_shell' => true],
    env_vars: ['LANG' => 'fr_FR.UTF-8', 'MESSAGE' => 'café 日本語'],
    cwd: 'C:\\Données',
    pipes: $pipes,
    descriptor_spec: [1 => ['pipe', 'w']],
    command: ['C:\\PHP\\php.exe', '-v']
);
proc_close($process);
"#);
}

/// Verifies computed argv, environment, and options arrays traverse the runtime
/// marshalling ABI and link into a valid PE image.
#[test]
fn test_windows_proc_open_dynamic_arrays_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$command = ['cmd.exe', '/d', '/s', '/c', 'exit 0'];
$environment = ['TEXT' => 'café 日本語', 'COUNT' => 42, 'RATIO' => 1.5, 'YES' => true];
$options = ['bypass_shell' => true];
$process = proc_open($command, [['pipe', 'r'], ['pipe', 'w']], $pipes, null, $environment, $options);
proc_close($process);
"#,
    );
}

/// Verifies dynamic associative command arrays and numeric/raw environment
/// entries compile through the php-src-compatible marshalling paths.
#[test]
fn test_windows_proc_open_dynamic_scalar_array_marshalling_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$command = ['program' => 'cmd.exe', 8 => '/d', 'switch' => '/c', 'body' => 'exit 0'];
$environment = [4 => 'RAW=1', '=C:' => 'C:\\Temp', 'DROP' => false, 'COUNT' => 42];
$process = proc_open($command, [["pipe", "r"], ["pipe", "w"]], $pipes, null, $environment);
if ($process !== false) { proc_close($process); }
"#,
    );
}

/// Compiles process status and termination together so the PE linker resolves
/// the retained-command registry, `GetProcessId`, `GetExitCodeProcess`, and
/// `TerminateProcess` without consuming the process before `proc_close`. The
/// numeric string signal also covers weak scalar coercion in x86_64 lowering.
#[test]
fn test_windows_proc_status_and_terminate_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$process = proc_open(['cmd.exe', '/d', '/s', '/c', 'timeout /t 5 > NUL'], [1 => ['pipe', 'w']], $pipes);
if ($process === false) { echo 'false'; }
else {
    $status = proc_get_status($process);
    echo $status['command'] . ':' . $status['pid'] . ':' . ($status['cached'] ? '1' : '0');
    echo proc_terminate($process, "15") ? ':terminated' : ':failed';
    proc_close($process);
}
"#,
    );
}

/// Runs the Windows process-status path and verifies php-src's Windows-specific
/// contract: all nine status fields exist, `cached` remains false, and
/// `proc_terminate` uses `TerminateProcess(..., 255)` before `proc_close`.
#[test]
fn test_windows_proc_status_and_terminate_run() {
    compile_and_run_windows(
        r#"<?php
$pipes = [];
$process = proc_open(['cmd.exe', '/d', '/s', '/c', 'ping -n 6 127.0.0.1 > NUL'], [1 => ['pipe', 'w']], $pipes);
if ($process === false) {
    echo 'false';
} else {
    $status = proc_get_status($process);
    $all = isset(
        $status['command'], $status['pid'], $status['cached'],
        $status['running'], $status['signaled'], $status['stopped'],
        $status['exitcode'], $status['termsig'], $status['stopsig'],
    );
    echo $all ? 'keys' : 'missing';
    echo $status['cached'] ? ':cached' : ':fresh';
    echo proc_terminate($process) ? ':terminated:' : ':failed:';
    echo proc_close($process);
}
"#,
        "keys:fresh:terminated:255",
    );
}

/// Executes the pre-existing string-command proc_open path to distinguish
/// general CreateProcessW regressions from computed-array marshalling defects.
#[test]
fn test_windows_proc_open_static_command_run() {
    compile_and_run_windows(
        r#"<?php
$pipes = [];
$process = proc_open('echo static-ok>proc-static.txt', [['pipe', 'w']], $pipes);
if ($process === false) { echo 'false'; } else {
    echo proc_close($process) . ':' . trim(file_get_contents('proc-static.txt'));
}
"#,
        "0:static-ok",
    );
}

/// Executes computed proc_open settings under native Windows/Wine and verifies
/// case-insensitive environment de-duplication uses the last PHP entry. A
/// directly executed helper observes the environment without shell expansion.
#[test]
fn test_windows_proc_open_dynamic_arrays_run() {
    compile_and_run_windows_expect_code_with_helpers(
        r#"<?php
$pipes = [];
$command = ['__WINDOWS_HELPER_PROC_ENV_HELPER__'];
$environment = ['Elephc_Dynamic' => 'first', 'ELEPHC_DYNAMIC' => 'second'];
$options = ['bypass_shell' => true];
$process = proc_open($command, [0 => ['null']], $pipes, null, $environment, $options);
if ($process === false) { echo 'false'; } else {
    echo proc_close($process) . ':' . trim(file_get_contents('proc-env.txt'));
}
"#,
        "0:second",
        0,
        &[(
            "proc-env-helper",
            "<?php file_put_contents('proc-env.txt', getenv('ELEPHC_DYNAMIC'));",
        )],
    );
}

/// Executes a computed command array without optional maps, isolating the argv
/// marshaller and its implicit direct-execution flag through a PE helper.
#[test]
fn test_windows_proc_open_dynamic_command_run() {
    compile_and_run_windows_expect_code_with_helpers(
        r#"<?php
$pipes = [];
$command = ['__WINDOWS_HELPER_PROC_COMMAND_HELPER__', 'argv-ok'];
$process = proc_open($command, [0 => ['null']], $pipes);
if ($process === false) { echo 'false'; } else {
    echo proc_close($process) . ':' . trim(file_get_contents('proc-command.txt'));
}
"#,
        "0:argv-ok",
        0,
        &[(
            "proc-command-helper",
            "<?php file_put_contents('proc-command.txt', $argv[1]);",
        )],
    );
}

/// Executes a computed command plus options map to isolate runtime
/// `bypass_shell` extraction independently from environment conversion.
#[test]
fn test_windows_proc_open_dynamic_options_run() {
    compile_and_run_windows_expect_code_with_helpers(
        r#"<?php
$pipes = [];
$command = ['__WINDOWS_HELPER_PROC_OPTIONS_HELPER__', 'options-ok'];
$options = ['bypass_shell' => true];
$process = proc_open($command, [0 => ['null']], $pipes, null, null, $options);
if ($process === false) { echo 'false'; } else {
    echo proc_close($process) . ':' . trim(file_get_contents('proc-options.txt'));
}
"#,
        "0:options-ok",
        0,
        &[(
            "proc-options-helper",
            "<?php file_put_contents('proc-options.txt', $argv[1]);",
        )],
    );
}

/// Compiles every documented Windows `proc_open` option together, including an
/// ignored unknown key and integer truthiness. This guards the packed option
/// ABI independently of the runtime-only Wine acceptance cases.
#[test]
fn test_windows_proc_open_all_documented_options_compile() {
    compile_windows_pe(
        r#"<?php
$pipes = [];
$options = [
    'bypass_shell' => 1,
    'suppress_errors' => true,
    'blocking_pipes' => false,
    'create_process_group' => true,
    'create_new_console' => true,
    'ignored_by_php' => 'value',
];
$process = proc_open('cmd.exe /d /s /c exit 0', [["pipe", "w"]], $pipes, null, null, $options);
proc_close($process);
"#,
    );
}

/// Verifies `disk_free_space`/`disk_total_space` compile to a valid Windows
/// PE32+ binary. Compile-only because the returned byte counts depend on the
/// host volume and are exercised by CI under Wine; this catches link failures
/// from the `__rt_sys_statfs` shim (which now calls `GetDiskFreeSpaceExA`
/// instead of the old `xor rax, rax` stub) and resolves the new
/// `GetDiskFreeSpaceExA` import.
#[test]
fn test_windows_disk_free_space_compile() {
    compile_windows_pe(r#"<?php echo disk_free_space("C:\\"); echo disk_total_space("C:\\");"#);
}

/// Verifies `sys_get_temp_dir()` compiles to a valid Windows PE32+ binary.
/// Compile-only because the actual temp path depends on the host environment
/// and is exercised by CI under Wine; this catches link failures from the new
/// `__rt_sys_get_temp_dir` shim (which calls `GetTempPathW` and heap-allocates
/// an owned string, replacing the previous hardcoded `"/tmp"` literal) and
/// resolves the new `GetTempPathW` import.
#[test]
fn test_windows_sys_get_temp_dir_compile() {
    compile_windows_pe("<?php echo sys_get_temp_dir();");
}

/// Verifies `link()` compiles to a valid Windows PE32+ binary. Compile-only
/// because the actual hard-link creation is exercised by CI under Wine; this
/// catches link failures from the `link` C-symbol shim (now translating the
/// Win32 `CreateHardLinkW` BOOL result to the POSIX convention `__rt_link`
/// expects instead of passing it through inverted).
#[test]
fn test_windows_link_compile() {
    compile_windows_pe(r#"<?php echo link("a.txt", "b.txt") ? 'ok' : 'fail';"#);
}

/// Verifies `touch()` compiles to a valid Windows PE32+ binary. Compile-only
/// because the actual timestamp update is exercised by CI under Wine; this
/// catches link failures from the rewritten `utimensat` shim (now opening the
/// file with `FILE_WRITE_ATTRIBUTES` and calling `SetFileTime` with real
/// FILETIME values instead of silently no-op'ing with NULL) and resolves the
/// `SetFileTime`/`GetSystemTimeAsFileTime` imports.
#[test]
fn test_windows_touch_compile() {
    compile_windows_pe(r#"<?php touch("a.txt", 1000000000, 1000000000); echo 'ok';"#);
}

/// Verifies `fileinode()` compiles to a valid Windows PE32+ binary. Compile-only
/// because the actual inode-equivalent value is exercised by CI under Wine;
/// this catches link failures from the extended `__rt_sys_stat` shim (which now
/// synthesizes `st_ino` via `CreateFileW` + `GetFileInformationByHandle`) and
/// resolves the new `GetFileInformationByHandle` import.
#[test]
fn test_windows_fileinode_compile() {
    compile_windows_pe(r#"<?php echo fileinode("a.txt");"#);
}

/// Verifies `php_uname()` compiles to a valid Windows PE32+ binary. Compile-only
/// because the actual sysname/nodename/machine values are exercised by CI under
/// Wine; this catches link failures from the rewritten `__rt_sys_uname` shim
/// (which now fills the utsname buffer via `GetComputerNameW`/
/// `GetNativeSystemInfo` instead of self-recursing through the local `uname`
/// C-symbol delegate) and resolves the `GetComputerNameW`/`GetNativeSystemInfo`
/// imports.
#[test]
fn test_windows_php_uname_compile() {
    compile_windows_pe(r#"<?php echo php_uname("a");"#);
}

/// Verifies the complete Windows date stack assembles and links for timestamps
/// on both sides of the 32-bit time boundary and before 1900, an IANA zone with
/// DST, synthetic `DateTime` construction/modification, and the monotonic
/// `hrtime()` path. Runtime values are asserted by Windows/Wine CI separately.
#[test]
fn test_windows_datetime_extreme_dst_and_monotonic_compile() {
    compile_windows_pe(r#"<?php
date_default_timezone_set("Europe/Paris");
$old = mktime(12, 30, 0, 3, 15, 1850);
$future = mktime(12, 30, 0, 7, 1, 2040);
echo date("Y-m-d H:i:s P I", $old), "|";
echo date("Y-m-d H:i:s P I", $future), "|";
echo strtotime("2040-07-01 12:30:00"), "|";
$d = new DateTime("1850-03-15 12:30:00");
echo $d->modify("+400 years")->format("Y-m-d H:i:s P I"), "|";
$a = hrtime(true);
$b = hrtime(true);
echo $b >= $a ? "monotonic" : "backwards";
"#);
}

/// Verifies the PDO SQLite bridge cross-builds and links into PE/COFF while
/// preserving Unicode SQL/text through the generated extern-call surface.
#[test]
fn test_windows_pdo_sqlite_unicode_compile() {
    compile_windows_pe(r#"<?php
$pdo = new PDO("sqlite::memory:");
$pdo->exec("CREATE TABLE données (valeur TEXT)");
$pdo->exec("INSERT INTO données VALUES ('été 日本語')");
$stmt = $pdo->query("SELECT valeur FROM données");
echo $stmt->fetchColumn();
"#);
}

/// Verifies the statically linked image bridge's principal GD, codec, Imagick,
/// built-in-font, Cairo, and Unicode-path call surfaces assemble and link into
/// a Windows PE binary. Codec rendering is exercised on native Windows CI.
#[test]
fn test_windows_image_bridge_primary_operations_compile() {
    compile_windows_pe(r#"<?php
$path = "Données-日本語.png";
$image = imagecreatetruecolor(16, 12);
$red = imagecolorallocate($image, 255, 0, 0);
imagefilledrectangle($image, 1, 1, 14, 10, $red);
imagestring($image, 1, 2, 2, "café", $red);
imagepng($image, $path);
$loaded = imagecreatefrompng($path);
echo imagesx($loaded), "x", imagesy($loaded);
imagewebp($loaded, "Données-日本語.webp");

$wand = new Imagick();
$wand->newImage(8, 8, "blue", "png");
$wand->resizeImage(4, 4, Imagick::FILTER_LANCZOS, 1.0);
$wand->writeImage("imagick-é東京.png");

$surface = new CairoImageSurface(CairoFormat::ARGB32, 8, 8);
$context = new CairoContext($surface);
$context->setSourceRgb(0.0, 1.0, 0.0);
$context->paint();
$surface->writeToPng("cairo-é東京.png");
"#);
}

/// Verifies the pure-Rust crypto and PHAR bridges cross-link into PE/COFF for
/// one-shot HMAC/incremental hashes plus Unicode archive paths and write features.
#[test]
fn test_windows_crypto_and_phar_unicode_compile() {
    compile_windows_pe(r#"<?php
echo hash("sha256", "abc"), hash_hmac("sha256", "abc", "clé");
$ctx = hash_init("sha512");
hash_update($ctx, "données");
echo hash_final($ctx);

$phar = new PharData("Données-日本語.tar");
$phar->addFromString("répertoire/東京.txt", "contenu");
$phar->setMetadata(["langue" => "français"]);
$phar->setSignatureAlgorithm(Phar::SHA256);
$gzip = $phar->compress(Phar::GZ);
echo $gzip["répertoire/東京.txt"]->getContent();
"#);
}
