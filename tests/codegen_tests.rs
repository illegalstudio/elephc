use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static TEST_ID: AtomicU64 = AtomicU64::new(0);
static SDK_PATH: OnceLock<String> = OnceLock::new();
static SDK_VERSION: OnceLock<String> = OnceLock::new();

fn get_sdk_path() -> &'static str {
    SDK_PATH.get_or_init(|| {
        Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    })
}

fn get_sdk_version() -> &'static str {
    SDK_VERSION.get_or_init(|| {
        match Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-version"])
            .output()
        {
            Ok(output) => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if version.is_empty() {
                    "15.0".to_string()
                } else {
                    version
                }
            }
            Err(_) => "15.0".to_string(),
        }
    })
}

/// Compile ASM string to binary via as + ld, then run it and return stdout.
fn default_link_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for candidate in ["/opt/homebrew/lib", "/usr/local/lib"] {
        if std::path::Path::new(candidate).exists() {
            paths.push(candidate.to_string());
        }
    }
    paths
}

fn link_binary(
    obj_path: &Path,
    bin_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) {
    let mut ld_cmd = Command::new("ld");
    ld_cmd.args(["-arch", "arm64", "-e", "_main", "-o"]);
    ld_cmd.arg(bin_path);
    ld_cmd.arg(obj_path);
    ld_cmd.args(["-lSystem", "-syslibroot"]);
    ld_cmd.arg(get_sdk_path());
    ld_cmd.args([
        "-platform_version",
        "macos",
        get_sdk_version(),
        get_sdk_version(),
    ]);
    for path in extra_link_paths {
        ld_cmd.arg(format!("-L{}", path));
    }
    for lib in extra_link_libs {
        if lib != "System" {
            ld_cmd.arg(format!("-l{}", lib));
        }
    }
    for framework in extra_frameworks {
        ld_cmd.args(["-framework", framework]);
    }

    let ld_status = ld_cmd.status().expect("failed to run linker");
    assert!(ld_status.success(), "linker failed");
}

fn assemble_and_run(
    asm: &str,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, asm).unwrap();

    let as_status = Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .status()
        .expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = Command::new(&bin_path)
        .current_dir(dir)
        .output()
        .expect("failed to run compiled binary");
    assert!(output.status.success(), "binary exited with error");

    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn test_exception_try_catch_same_function() {
    let out = compile_and_run(
        "<?php class MyException extends Exception {} try { throw new MyException(); } catch (MyException $e) { echo 42; }",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_builtin_exception_try_catch() {
    let out = compile_and_run("<?php try { throw new Exception(); } catch (Exception $e) { echo 11; }");
    assert_eq!(out, "11");
}

#[test]
fn test_builtin_throwable_catches_exception() {
    let out = compile_and_run("<?php try { throw new Exception(); } catch (Throwable $e) { echo 12; }");
    assert_eq!(out, "12");
}

#[test]
fn test_exception_throw_during_concat_resets_concat_cursor() {
    let out = compile_and_run(
        "<?php function boom() { throw new Exception(); } try { echo \"left-\" . boom(); } catch (Exception $e) { echo json_encode([\"ok\"]); }",
    );
    assert_eq!(out, "[\"ok\"]");
}

#[test]
fn test_exception_multi_catch_matches_each_type() {
    let out = compile_and_run(
        "<?php class AException extends Exception {} class BException extends Exception {} function boom($flag) { if ($flag) { throw new AException(); } throw new BException(); } try { boom(true); } catch (AException | BException $e) { echo 1; } try { boom(false); } catch (AException | BException $e) { echo 2; }",
    );
    assert_eq!(out, "12");
}

#[test]
fn test_exception_catch_without_variable() {
    let out = compile_and_run(
        "<?php try { throw new Exception(); } catch (Exception) { echo 21; }",
    );
    assert_eq!(out, "21");
}

#[test]
fn test_throw_expression_in_null_coalesce() {
    let out = compile_and_run(
        "<?php $value = 42; echo $value ?? throw new Exception(); try { $missing = null; echo $missing ?? throw new Exception(); } catch (Exception) { echo 22; }",
    );
    assert_eq!(out, "4222");
}

#[test]
fn test_throw_expression_in_ternary() {
    let out = compile_and_run(
        "<?php try { echo false ? 1 : throw new Exception(); } catch (Exception) { echo 23; }",
    );
    assert_eq!(out, "23");
}

#[test]
fn test_exception_try_catch_cross_function() {
    let out = compile_and_run(
        "<?php class MyException extends Exception {} function boom() { throw new MyException(); } try { boom(); } catch (MyException $e) { echo 7; }",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_exception_nested_try_catch() {
    let out = compile_and_run(
        "<?php class InnerException extends Exception {} try { try { throw new InnerException(); } catch (InnerException $e) { echo 31; } } catch (Exception $e) { echo 99; }",
    );
    assert_eq!(out, "31");
}

#[test]
fn test_exception_throw_in_catch_rethrows() {
    let out = compile_and_run(
        "<?php class FirstException extends Exception {} class SecondException extends Exception {} try { try { throw new FirstException(); } catch (FirstException $e) { echo 32; throw new SecondException(); } } catch (SecondException $e) { echo 33; }",
    );
    assert_eq!(out, "3233");
}

#[test]
fn test_exception_throw_in_finally_overrides_prior_exception() {
    let out = compile_and_run(
        "<?php class FirstException extends Exception {} class FinalException extends Exception {} try { try { throw new FirstException(); } finally { throw new FinalException(); } } catch (FinalException $e) { echo 34; }",
    );
    assert_eq!(out, "34");
}

#[test]
fn test_exception_uncaught_reports_fatal_error() {
    let err = compile_and_run_expect_failure("<?php throw new Exception();");
    assert!(err.contains("Fatal error: uncaught exception"), "{err}");
}

#[test]
fn test_exception_with_properties() {
    let out = compile_and_run(
        "<?php class HttpException extends Exception { public $status; public function __construct() { $this->status = 404; } } try { throw new HttpException(); } catch (HttpException $e) { echo $e->status; }",
    );
    assert_eq!(out, "404");
}

#[test]
fn test_exception_try_catch_inside_loop() {
    let out = compile_and_run(
        "<?php class LoopException extends Exception {} for ($i = 0; $i < 3; $i++) { try { if ($i == 1) { throw new LoopException(); } echo $i; } catch (LoopException $e) { echo 9; } }",
    );
    assert_eq!(out, "092");
}

#[test]
fn test_exception_finally_runs_on_return_break_continue() {
    let out = compile_and_run(
        "<?php function f() { try { return 5; } finally { echo 1; } } echo f(); for ($i = 0; $i < 1; $i++) { try { echo 2; break; } finally { echo 3; } } for ($j = 0; $j < 2; $j++) { try { echo $j; continue; } finally { echo 9; } }",
    );
    assert_eq!(out, "15230919");
}

struct ProgramOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

fn assemble_and_run_capture(
    asm: &str,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> ProgramOutput {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, asm).unwrap();

    let as_status = Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .status()
        .expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = Command::new(&bin_path)
        .current_dir(dir)
        .output()
        .expect("failed to run compiled binary");

    ProgramOutput {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        success: output.status.success(),
    }
}

fn assemble_and_run_expect_failure(
    asm: &str,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, asm).unwrap();

    let as_status = Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .status()
        .expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = Command::new(&bin_path)
        .current_dir(dir)
        .output()
        .expect("failed to run compiled binary");
    assert!(!output.status.success(), "binary unexpectedly succeeded");

    String::from_utf8(output.stderr).unwrap()
}

fn compile_source_to_asm_with_options(
    source: &str,
    dir: &Path,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, Vec<String>) {
    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, dir).expect("resolve failed");
    let check_result = elephc::types::check(&resolved).expect("type check failed");
    let asm = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        heap_size,
        gc_stats,
        heap_debug,
    );
    (asm, check_result.required_libraries)
}

fn inject_main_exit_harness(asm: &str, harness: &str) -> String {
    let needle = "    mov x0, #0\n    mov x16, #1\n    svc #0x80";
    let replacement = format!("{harness}\n    mov x0, #0\n    mov x16, #1\n    svc #0x80");
    let patched = asm.replacen(needle, &replacement, 1);
    assert_ne!(patched, asm, "failed to inject main exit harness");
    patched
}

fn compile_harness_expect_failure(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let patched = inject_main_exit_harness(&asm, harness);
    let stderr = assemble_and_run_expect_failure(
        &patched,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stderr
}

fn compile_harness_and_run(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);
    let patched = inject_main_exit_harness(&asm, harness);
    let stdout = assemble_and_run(
        &patched,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stdout
}

fn compile_harness_and_run_with_heap_debug(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let patched = inject_main_exit_harness(&asm, harness);
    let stdout = assemble_and_run(
        &patched,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stdout
}

fn compile_and_run_with_gc_stats(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, true, false);
    let output = assemble_and_run_capture(
        &asm,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

fn compile_and_run_with_heap_debug(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, true);
    let output = assemble_and_run_capture(
        &asm,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

fn parse_gc_stats(stderr: &str) -> (u64, u64) {
    let line = stderr
        .lines()
        .find(|line| line.starts_with("GC: allocs="))
        .unwrap_or_else(|| panic!("missing gc stats line: {stderr}"));
    let allocs = line
        .split("allocs=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing alloc count: {stderr}"));
    let frees = line
        .split("frees=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("missing free count: {stderr}"));
    (allocs, frees)
}

/// Compile a PHP source string to a native binary, run it, and return stdout.
/// Uses the elephc library directly (no subprocess) for tokenize → parse → check → codegen.
/// Only spawns as + ld + binary execution.
fn compile_and_run_with_heap_size(source: &str, heap_size: usize) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);

    let elephc_out = assemble_and_run(
        &asm,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    // PHP cross-check (opt-in via ELEPHC_PHP_CHECK=1)
    if std::env::var("ELEPHC_PHP_CHECK").is_ok() {
        let php_path = dir.join("test.php");
        fs::write(&php_path, source).unwrap();
        if let Ok(php_output) = Command::new("php").arg(&php_path).output() {
            if php_output.status.success() {
                let php_out = String::from_utf8_lossy(&php_output.stdout);
                if elephc_out != php_out.as_ref() {
                    eprintln!(
                        "PHP compat note: output differs for test.\n  elephc: {:?}\n  php:    {:?}",
                        elephc_out, php_out
                    );
                }
            }
        }
    }

    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

fn compile_and_run(source: &str) -> String {
    compile_and_run_with_heap_size(source, 8_388_608)
}

/// Compile a PHP source string and assert the generated binary fails at runtime.
fn compile_and_run_expect_failure(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let output =
        assemble_and_run_expect_failure(&asm, &dir, &required_libraries, &default_link_paths(), &[]);

    let _ = fs::remove_dir_all(&dir);
    output
}

/// Compile a PHP project with multiple files using the library directly.
fn compile_and_run_files(files: &[(&str, &str)], main_file: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let php_path = dir.join(main_file);
    let source = fs::read_to_string(&php_path).unwrap();
    let base_dir = php_path.parent().unwrap();

    let tokens = elephc::lexer::tokenize(&source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, base_dir).expect("resolve failed");
    let check_result = elephc::types::check(&resolved).expect("type check failed");
    let asm = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
    );

    let elephc_out = assemble_and_run(
        &asm,
        &dir,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

/// Write multiple files and attempt compilation. Returns true if compilation fails.
fn compile_files_fails(files: &[(&str, &str)], main_file: &str) -> bool {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    for (path, content) in files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    let php_path = dir.join(main_file);
    let source = fs::read_to_string(&php_path).unwrap();
    let base_dir = php_path.parent().unwrap();

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let tokens = elephc::lexer::tokenize(&source)?;
        let ast = elephc::parser::parse(&tokens)?;
        let resolved = elephc::resolver::resolve(ast, base_dir)?;
        elephc::types::check(&resolved)?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&dir);
    result.is_err()
}

/// Compile a PHP source string and run with piped stdin data.
fn compile_and_run_with_stdin(source: &str, stdin_data: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let check_result = elephc::types::check(&resolved).expect("type check failed");
    let asm = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
    );

    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, &asm).unwrap();

    let as_status = Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&obj_path)
        .arg(&asm_path)
        .status()
        .expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        &bin_path,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );

    use std::io::Write;
    let mut child = Command::new(&bin_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(stdin_data.as_bytes()).unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("failed to wait for binary");
    assert!(output.status.success(), "binary exited with error");

    let _ = fs::remove_dir_all(&dir);
    String::from_utf8(output.stdout).unwrap()
}

/// Compile and run in a specific temp dir (returns dir path for file I/O tests).
fn compile_and_run_in_dir(source: &str) -> (String, std::path::PathBuf) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let check_result = elephc::types::check(&resolved).expect("type check failed");
    let asm = elephc::codegen::generate(
        &resolved,
        &check_result.global_env,
        &check_result.functions,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
    );

    let elephc_out = assemble_and_run(
        &asm,
        &dir,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );
    (elephc_out, dir)
}

// --- Phase 1: Echo strings ---

#[test]
fn test_echo_hello_world() {
    let out = compile_and_run("<?php echo \"Hello, World!\\n\";");
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn test_echo_empty_string() {
    let out = compile_and_run("<?php echo \"\";");
    assert_eq!(out, "");
}

#[test]
fn test_echo_multiple_strings() {
    let out = compile_and_run("<?php echo \"foo\"; echo \"bar\"; echo \"\\n\";");
    assert_eq!(out, "foobar\n");
}

#[test]
fn test_echo_escape_sequences() {
    let out = compile_and_run("<?php echo \"a\\tb\\nc\";");
    assert_eq!(out, "a\tb\nc");
}

// --- Phase 2: Variables and integers ---

#[test]
fn test_echo_integer() {
    let out = compile_and_run("<?php echo 42;");
    assert_eq!(out, "42");
}

#[test]
fn test_echo_zero() {
    let out = compile_and_run("<?php echo 0;");
    assert_eq!(out, "0");
}

#[test]
fn test_echo_negative() {
    let out = compile_and_run("<?php echo -7;");
    assert_eq!(out, "-7");
}

#[test]
fn test_echo_large_number() {
    let out = compile_and_run("<?php echo 1000000;");
    assert_eq!(out, "1000000");
}

#[test]
fn test_variable_int() {
    let out = compile_and_run("<?php $x = 42; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_variable_string() {
    let out = compile_and_run("<?php $s = \"hello\"; echo $s;");
    assert_eq!(out, "hello");
}

#[test]
fn test_variable_reassign_same_type() {
    let out = compile_and_run("<?php $x = 1; $x = 2; echo $x;");
    assert_eq!(out, "2");
}

#[test]
fn test_multiple_variables() {
    let out =
        compile_and_run("<?php $a = 10; $b = 20; echo $a; echo \" \"; echo $b; echo \"\\n\";");
    assert_eq!(out, "10 20\n");
}

#[test]
fn test_variable_negative_int() {
    let out = compile_and_run("<?php $x = -100; echo $x;");
    assert_eq!(out, "-100");
}

#[test]
fn test_echo_int_zero_variable() {
    let out = compile_and_run("<?php $z = 0; echo $z;");
    assert_eq!(out, "0");
}

// --- Phase 3: Arithmetic ---

#[test]
fn test_addition() {
    let out = compile_and_run("<?php echo 10 + 32;");
    assert_eq!(out, "42");
}

#[test]
fn test_subtraction() {
    let out = compile_and_run("<?php echo 100 - 58;");
    assert_eq!(out, "42");
}

#[test]
fn test_multiplication() {
    let out = compile_and_run("<?php echo 6 * 7;");
    assert_eq!(out, "42");
}

#[test]
fn test_division() {
    let out = compile_and_run("<?php echo 84 / 2;");
    assert_eq!(out, "42");
}

#[test]
fn test_arithmetic_with_variables() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo $a + $b;");
    assert_eq!(out, "42");
}

#[test]
fn test_operator_precedence() {
    let out = compile_and_run("<?php echo 2 + 3 * 4;");
    assert_eq!(out, "14");
}

#[test]
fn test_parenthesized_arithmetic() {
    let out = compile_and_run("<?php echo (2 + 3) * 4;");
    assert_eq!(out, "20");
}

#[test]
fn test_complex_expression() {
    let out = compile_and_run("<?php echo (10 + 5) * 2 - 7;");
    assert_eq!(out, "23");
}

#[test]
fn test_arithmetic_assign_and_echo() {
    let out = compile_and_run("<?php $a = 10; $b = 32; $c = $a + $b; echo $c;");
    assert_eq!(out, "42");
}

#[test]
fn test_subtraction_negative_result() {
    let out = compile_and_run("<?php echo 3 - 10;");
    assert_eq!(out, "-7");
}

#[test]
fn test_nested_arithmetic() {
    let out = compile_and_run("<?php echo 1 + 2 + 3 + 4;");
    assert_eq!(out, "10");
}

// --- Phase 3: Concatenation ---

#[test]
fn test_concat_literals() {
    let out = compile_and_run("<?php echo \"Hello, \" . \"World!\";");
    assert_eq!(out, "Hello, World!");
}

#[test]
fn test_concat_variables() {
    let out = compile_and_run("<?php $a = \"Hello, \"; $b = \"World!\"; echo $a . $b;");
    assert_eq!(out, "Hello, World!");
}

#[test]
fn test_concat_chain() {
    let out = compile_and_run("<?php echo \"a\" . \"b\" . \"c\";");
    assert_eq!(out, "abc");
}

#[test]
fn test_concat_assign() {
    let out = compile_and_run("<?php $msg = \"foo\" . \"bar\"; echo $msg;");
    assert_eq!(out, "foobar");
}

#[test]
fn test_concat_with_newline() {
    let out = compile_and_run("<?php echo \"hello\" . \"\\n\";");
    assert_eq!(out, "hello\n");
}

// --- Phase 3: Mixed-type concatenation ---

#[test]
fn test_concat_string_and_int() {
    let out = compile_and_run("<?php echo \"Value: \" . 42;");
    assert_eq!(out, "Value: 42");
}

#[test]
fn test_concat_int_and_string() {
    let out = compile_and_run("<?php echo 42 . \" is the answer\";");
    assert_eq!(out, "42 is the answer");
}

#[test]
fn test_concat_int_and_int() {
    let out = compile_and_run("<?php echo 1 . 2;");
    assert_eq!(out, "12");
}

#[test]
fn test_concat_expr_result() {
    let out = compile_and_run("<?php $a = 10; $b = 32; echo \"Result: \" . ($a + $b);");
    assert_eq!(out, "Result: 42");
}

#[test]
fn test_concat_chain_mixed() {
    let out = compile_and_run("<?php echo \"x=\" . 5 . \" y=\" . 10;");
    assert_eq!(out, "x=5 y=10");
}

#[test]
fn test_concat_negative_int() {
    let out = compile_and_run("<?php echo \"num: \" . -7;");
    assert_eq!(out, "num: -7");
}

// --- Modulo ---

#[test]
fn test_modulo() {
    let out = compile_and_run("<?php echo 10 % 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_modulo_zero_remainder() {
    let out = compile_and_run("<?php echo 15 % 5;");
    assert_eq!(out, "0");
}

// --- Comparison operators ---

#[test]
fn test_equal_true() {
    let out = compile_and_run("<?php echo 1 == 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_equal_false() {
    let out = compile_and_run("<?php echo 1 == 2;");
    assert_eq!(out, ""); // echo false prints nothing in PHP
}

#[test]
fn test_not_equal() {
    let out = compile_and_run("<?php echo 1 != 2;");
    assert_eq!(out, "1");
}

// --- Loose comparison across types ---

#[test]
fn test_loose_eq_empty_string_false() {
    let out = compile_and_run("<?php var_dump(\"\" == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_loose_eq_zero_false() {
    let out = compile_and_run("<?php var_dump(0 == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_loose_eq_one_true() {
    let out = compile_and_run("<?php var_dump(1 == true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_loose_eq_string_vs_int() {
    let out = compile_and_run("<?php var_dump(\"0\" == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_loose_neq_empty_string_true() {
    let out = compile_and_run("<?php var_dump(\"\" != true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_loose_eq_null_false() {
    let out = compile_and_run("<?php var_dump(null == false);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_less_than() {
    let out = compile_and_run("<?php echo 1 < 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_greater_than() {
    let out = compile_and_run("<?php echo 2 > 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_less_equal() {
    let out = compile_and_run("<?php echo 2 <= 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_greater_equal() {
    let out = compile_and_run("<?php echo 1 >= 2;");
    assert_eq!(out, "");
}

// --- if/else ---

#[test]
fn test_if_true() {
    let out = compile_and_run("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(out, "yes");
}

#[test]
fn test_if_false() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"yes\"; }");
    assert_eq!(out, "");
}

#[test]
fn test_if_else() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"a\"; } else { echo \"b\"; }");
    assert_eq!(out, "b");
}

#[test]
fn test_if_elseif_else() {
    let out = compile_and_run(
        "<?php $x = 2; if ($x == 1) { echo \"one\"; } elseif ($x == 2) { echo \"two\"; } else { echo \"other\"; }",
    );
    assert_eq!(out, "two");
}

#[test]
fn test_if_else_falls_through() {
    let out = compile_and_run(
        "<?php $x = 99; if ($x == 1) { echo \"a\"; } elseif ($x == 2) { echo \"b\"; } else { echo \"c\"; }",
    );
    assert_eq!(out, "c");
}

// --- while ---

#[test]
fn test_while_loop() {
    let out = compile_and_run("<?php $i = 0; while ($i < 5) { echo $i; $i = $i + 1; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_while_zero_iterations() {
    let out = compile_and_run("<?php while (0) { echo \"no\"; }");
    assert_eq!(out, "");
}

#[test]
fn test_while_break() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 10) { if ($i == 3) { break; } echo $i; $i = $i + 1; }",
    );
    assert_eq!(out, "012");
}

#[test]
fn test_while_continue() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 5) { $i = $i + 1; if ($i == 3) { continue; } echo $i; }",
    );
    assert_eq!(out, "1245");
}

// --- for ---

#[test]
fn test_for_loop() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i = $i + 1) { echo $i; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_for_break() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 10; $i = $i + 1) { if ($i == 3) { break; } echo $i; }",
    );
    assert_eq!(out, "012");
}

// --- FizzBuzz ---

#[test]
fn test_fizzbuzz() {
    let source = r#"<?php
$i = 1;
while ($i <= 15) {
    if ($i % 15 == 0) {
        echo "FizzBuzz\n";
    } elseif ($i % 3 == 0) {
        echo "Fizz\n";
    } elseif ($i % 5 == 0) {
        echo "Buzz\n";
    } else {
        echo $i;
        echo "\n";
    }
    $i = $i + 1;
}
"#;
    let out = compile_and_run(source);
    assert_eq!(
        out,
        "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n"
    );
}

// --- Increment/Decrement ---

#[test]
fn test_pre_increment() {
    let out = compile_and_run("<?php $i = 1; $k = ++$i; echo $i . \" \" . $k;");
    assert_eq!(out, "2 2");
}

#[test]
fn test_post_increment() {
    let out = compile_and_run("<?php $i = 1; $k = $i++; echo $i . \" \" . $k;");
    assert_eq!(out, "2 1");
}

#[test]
fn test_pre_decrement() {
    let out = compile_and_run("<?php $i = 5; $k = --$i; echo $i . \" \" . $k;");
    assert_eq!(out, "4 4");
}

#[test]
fn test_post_decrement() {
    let out = compile_and_run("<?php $i = 5; $k = $i--; echo $i . \" \" . $k;");
    assert_eq!(out, "4 5");
}

#[test]
fn test_standalone_increment() {
    let out = compile_and_run("<?php $x = 0; $x++; $x++; $x++; echo $x;");
    assert_eq!(out, "3");
}

#[test]
fn test_standalone_decrement() {
    let out = compile_and_run("<?php $x = 10; $x--; $x--; echo $x;");
    assert_eq!(out, "8");
}

#[test]
fn test_for_with_increment() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i++) { echo $i; }");
    assert_eq!(out, "01234");
}

#[test]
fn test_while_with_pre_increment() {
    let out = compile_and_run("<?php $i = 0; while ($i < 3) { ++$i; echo $i; }");
    assert_eq!(out, "123");
}

// --- Functions ---

#[test]
fn test_function_call_int() {
    let out = compile_and_run("<?php function add($a, $b) { return $a + $b; } echo add(10, 32);");
    assert_eq!(out, "42");
}

#[test]
fn test_function_call_string() {
    let out = compile_and_run(
        "<?php function greet($name) { return \"Hello, \" . $name; } echo greet(\"World\");",
    );
    assert_eq!(out, "Hello, World");
}

#[test]
fn test_function_void() {
    let out = compile_and_run("<?php function say() { echo \"hi\"; return; } say();");
    assert_eq!(out, "hi");
}

#[test]
fn test_function_local_scope() {
    let out = compile_and_run(
        "<?php $x = 1; function get_two() { $x = 2; return $x; } echo $x . \" \" . get_two();",
    );
    assert_eq!(out, "1 2");
}

#[test]
fn test_function_recursive() {
    let out = compile_and_run(
        "<?php function fact($n) { if ($n <= 1) { return 1; } return $n * fact($n - 1); } echo fact(5);",
    );
    assert_eq!(out, "120");
}

#[test]
fn test_function_multiple_calls() {
    let out = compile_and_run(
        "<?php function double($x) { return $x * 2; } echo double(3) . \" \" . double(7);",
    );
    assert_eq!(out, "6 14");
}

#[test]
fn test_function_as_argument() {
    let out = compile_and_run(
        "<?php function add($a, $b) { return $a + $b; } echo add(add(1, 2), add(3, 4));",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_function_no_args() {
    let out = compile_and_run("<?php function answer() { return 42; } echo answer();");
    assert_eq!(out, "42");
}

// --- Logical operators ---

#[test]
fn test_and_true() {
    let out = compile_and_run("<?php echo 1 && 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_and_false() {
    let out = compile_and_run("<?php echo 1 && 0;");
    assert_eq!(out, "");
}

#[test]
fn test_or_true() {
    let out = compile_and_run("<?php echo 0 || 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_or_false() {
    let out = compile_and_run("<?php echo 0 || 0;");
    assert_eq!(out, "");
}

#[test]
fn test_not_zero() {
    let out = compile_and_run("<?php $x = 0; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_not_nonzero() {
    let out = compile_and_run("<?php $x = 42; echo !$x;");
    assert_eq!(out, "");
}

#[test]
fn test_short_circuit_and() {
    let out = compile_and_run(
        r#"<?php
$count = 0;
function inc() { return 1; }
$r = 0 && inc();
echo $r;
"#,
    );
    assert_eq!(out, ""); // false prints nothing
}

#[test]
fn test_short_circuit_or() {
    // With ||, if left is true the right side should not be evaluated.
    let out = compile_and_run(
        r#"<?php
function inc() { return 1; }
$r = 1 || inc();
echo $r;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_boolean_true() {
    let out = compile_and_run("<?php echo true;");
    assert_eq!(out, "1");
}

#[test]
fn test_boolean_false() {
    let out = compile_and_run("<?php echo false;");
    assert_eq!(out, "");
}

#[test]
fn test_boolean_in_condition() {
    let out = compile_and_run("<?php if (true) { echo \"yes\"; } if (false) { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Assignment operators ---

#[test]
fn test_plus_assign() {
    let out = compile_and_run("<?php $x = 10; $x += 5; echo $x;");
    assert_eq!(out, "15");
}

#[test]
fn test_minus_assign() {
    let out = compile_and_run("<?php $x = 10; $x -= 3; echo $x;");
    assert_eq!(out, "7");
}

#[test]
fn test_star_assign() {
    let out = compile_and_run("<?php $x = 6; $x *= 7; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_slash_assign() {
    let out = compile_and_run("<?php $x = 84; $x /= 2; echo $x;");
    assert_eq!(out, "42");
}

#[test]
fn test_percent_assign() {
    let out = compile_and_run("<?php $x = 10; $x %= 3; echo $x;");
    assert_eq!(out, "1");
}

#[test]
fn test_dot_assign() {
    let out = compile_and_run("<?php $s = \"hello\"; $s .= \" world\"; echo $s;");
    assert_eq!(out, "hello world");
}

#[test]
fn test_logical_with_comparison() {
    let out = compile_and_run("<?php $x = 5; echo ($x > 3 && $x < 10);");
    assert_eq!(out, "1");
}

// --- Logical operators with null ---

#[test]
fn test_null_and_true() {
    // null && true → false (null coerces to false)
    let out = compile_and_run("<?php echo null && true;");
    assert_eq!(out, "");
}

#[test]
fn test_true_and_null() {
    let out = compile_and_run("<?php echo true && null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_false() {
    // null || false → false
    let out = compile_and_run("<?php echo null || false;");
    assert_eq!(out, "");
}

#[test]
fn test_false_or_null() {
    let out = compile_and_run("<?php echo false || null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_or_true() {
    // null || true → true
    let out = compile_and_run("<?php echo null || true;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_and_false() {
    let out = compile_and_run("<?php echo null && false;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_and() {
    let out = compile_and_run("<?php $x = null; echo $x && true;");
    assert_eq!(out, "");
}

#[test]
fn test_null_var_or() {
    let out = compile_and_run("<?php $x = null; echo $x || false;");
    assert_eq!(out, "");
}

#[test]
fn test_not_null_is_true() {
    // !null → true
    let out = compile_and_run("<?php $x = null; echo !$x;");
    assert_eq!(out, "1");
}

#[test]
fn test_if_null_is_falsy() {
    let out = compile_and_run(
        r#"<?php
$x = null;
if ($x) {
    echo "true";
} else {
    echo "false";
}
"#,
    );
    assert_eq!(out, "false");
}

#[test]
fn test_ternary_null_is_falsy() {
    let out = compile_and_run("<?php $x = null; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

#[test]
fn test_while_null_no_loop() {
    let out = compile_and_run("<?php $x = null; while ($x) { echo \"bad\"; } echo \"ok\";");
    assert_eq!(out, "ok");
}

// --- Ternary operator ---

#[test]
fn test_ternary_true() {
    let out = compile_and_run("<?php echo 1 == 1 ? \"yes\" : \"no\";");
    assert_eq!(out, "yes");
}

#[test]
fn test_ternary_false() {
    let out = compile_and_run("<?php echo 1 == 2 ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

#[test]
fn test_ternary_int() {
    let out = compile_and_run("<?php $x = 3; $y = 7; echo $x > $y ? $x : $y;");
    assert_eq!(out, "7");
}

#[test]
fn test_ternary_in_assignment() {
    let out = compile_and_run("<?php $a = 10; $b = 20; $max = $a > $b ? $a : $b; echo $max;");
    assert_eq!(out, "20");
}

#[test]
fn test_ternary_mixed_types_str_vs_int() {
    let out = compile_and_run(
        "<?php $a = [1]; array_pop($a); $v = array_pop($a); echo is_null($v) ? \"null\" : \"has value\";",
    );
    assert_eq!(out, "null");
}

#[test]
fn test_ternary_mixed_types_then_branch_str() {
    let out = compile_and_run("<?php $x = 0; echo $x ? \"yes\" : \"no\";");
    assert_eq!(out, "no");
}

#[test]
fn test_ternary_int_string() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? 42 : "none";
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ternary_string_int() {
    let out = compile_and_run(
        r#"<?php
$x = false;
echo $x ? "yes" : 0;
"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_ternary_string_string() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? "hello" : "world";
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_ternary_int_int() {
    let out = compile_and_run(
        r#"<?php
$x = true;
echo $x ? 1 : 0;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ternary_mixed_in_concat() {
    let out = compile_and_run(
        r#"<?php
$count = 5;
echo "Items: " . ($count > 0 ? $count : "none");
"#,
    );
    assert_eq!(out, "Items: 5");
}

#[test]
fn test_ternary_float_string() {
    let out = compile_and_run(
        r#"<?php
$x = false;
echo $x ? 3.14 : "zero";
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_ternary_nested_mixed() {
    let out = compile_and_run(
        r#"<?php
$a = 0;
echo $a ? "yes" : ($a === 0 ? "zero" : "no");
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_ternary_variable_string() {
    let out = compile_and_run(
        r#"<?php
$name = "Alice";
$greeting = true ? $name : "nobody";
echo $greeting;
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_ternary_function_result() {
    let out = compile_and_run(
        r#"<?php
function get_name() { return "Bob"; }
echo true ? get_name() : "default";
"#,
    );
    assert_eq!(out, "Bob");
}

#[test]
fn test_ternary_variable_int_vs_string() {
    let out = compile_and_run(
        r#"<?php
$count = 5;
$label = "none";
echo ($count > 0) ? $count : $label;
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_ternary_method_call_result() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val;
    public function __construct($v) { $this->val = $v; }
    public function get() { return $this->val; }
}
$b = new Box("hello");
echo true ? $b->get() : "fallback";
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_chained_closure_call() {
    let out = compile_and_run(
        "<?php $f = function() { return function() { return 99; }; }; echo $f()();",
    );
    assert_eq!(out, "99");
}

// --- do...while ---

#[test]
fn test_do_while() {
    let out = compile_and_run("<?php $i = 0; do { $i++; } while ($i < 5); echo $i;");
    assert_eq!(out, "5");
}

#[test]
fn test_do_while_runs_once() {
    let out = compile_and_run("<?php $i = 0; do { $i++; } while (false); echo $i;");
    assert_eq!(out, "1");
}

// --- Single-quoted strings ---

#[test]
fn test_single_quoted_string() {
    let out = compile_and_run("<?php echo 'hello';");
    assert_eq!(out, "hello");
}

#[test]
fn test_single_quoted_no_escape() {
    let out = compile_and_run(r"<?php echo 'no\n escape';");
    assert_eq!(out, "no\\n escape");
}

#[test]
fn test_single_quoted_escaped_quote() {
    let out = compile_and_run("<?php echo 'it\\'s';");
    assert_eq!(out, "it's");
}

// --- null ---

#[test]
fn test_null_echo_nothing() {
    let out = compile_and_run("<?php echo null;");
    assert_eq!(out, "");
}

#[test]
fn test_null_variable_echo_nothing() {
    let out = compile_and_run("<?php $x = null; echo $x;");
    assert_eq!(out, "");
}

#[test]
fn test_is_null_true() {
    let out = compile_and_run("<?php $x = null; echo is_null($x);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_null_false() {
    let out = compile_and_run("<?php $x = 42; echo is_null($x);");
    assert_eq!(out, "");
}

#[test]
fn test_null_plus_int() {
    let out = compile_and_run("<?php $x = null; echo $x + 5;");
    assert_eq!(out, "5");
}

#[test]
fn test_null_concat() {
    let out = compile_and_run("<?php $x = null; echo $x . \"hello\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_null_equals_zero() {
    let out = compile_and_run("<?php $x = null; echo $x == 0;");
    assert_eq!(out, "1");
}

#[test]
fn test_null_plus_assign() {
    let out = compile_and_run("<?php $y = null; $y += 10; echo $y;");
    assert_eq!(out, "10");
}

#[test]
fn test_null_reassign() {
    let out = compile_and_run("<?php $x = null; $x = 42; echo $x;");
    assert_eq!(out, "42");
}

// --- Built-in functions ---

#[test]
fn test_strlen() {
    let out = compile_and_run("<?php echo strlen(\"hello\");");
    assert_eq!(out, "5");
}

#[test]
fn test_strlen_empty() {
    let out = compile_and_run("<?php echo strlen(\"\");");
    assert_eq!(out, "0");
}

#[test]
fn test_intval_string() {
    let out = compile_and_run("<?php echo intval(\"42\");");
    assert_eq!(out, "42");
}

#[test]
fn test_intval_negative() {
    let out = compile_and_run("<?php echo intval(\"-7\");");
    assert_eq!(out, "-7");
}

#[test]
fn test_intval_int_passthrough() {
    let out = compile_and_run("<?php echo intval(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_exit_code() {
    // We can't easily test exit code in compile_and_run, so test that
    // exit stops execution (nothing after exit is printed)
    let out = compile_and_run("<?php echo \"before\"; exit(0); echo \"after\";");
    assert_eq!(out, "before");
}

// --- $argc ---

#[test]
fn test_argc_exists() {
    let out = compile_and_run("<?php echo $argc;");
    // When run as a test, argc is 1 (just the binary name)
    assert_eq!(out, "1");
}

// --- Arrays ---

#[test]
fn test_array_literal_and_count() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; echo count($a);");
    assert_eq!(out, "3");
}

#[test]
fn test_array_access() {
    let out =
        compile_and_run("<?php $a = [10, 20, 30]; echo $a[0] . \" \" . $a[1] . \" \" . $a[2];");
    assert_eq!(out, "10 20 30");
}

#[test]
fn test_array_access_variable_index() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; $i = 2; echo $a[$i];");
    assert_eq!(out, "30");
}

#[test]
fn test_array_assign() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; $a[1] = 99; echo $a[1];");
    assert_eq!(out, "99");
}

#[test]
fn test_array_assign_into_empty_array_updates_length() {
    let out = compile_and_run(r#"<?php $a = []; $a[0] = 7; echo count($a) . "|" . $a[0];"#);
    assert_eq!(out, "1|7");
}

#[test]
fn test_array_push() {
    let out = compile_and_run("<?php $a = [1, 2]; $a[] = 3; echo count($a) . \" \" . $a[2];");
    assert_eq!(out, "3 3");
}

#[test]
fn test_array_push_builtin() {
    let out =
        compile_and_run("<?php $a = [10]; array_push($a, 20); echo count($a) . \" \" . $a[1];");
    assert_eq!(out, "2 20");
}

#[test]
fn test_foreach_int() {
    let out = compile_and_run("<?php $a = [1, 2, 3]; foreach ($a as $v) { echo $v; }");
    assert_eq!(out, "123");
}

#[test]
fn test_foreach_string() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "abc");
}

#[test]
fn test_foreach_break() {
    let out = compile_and_run(
        "<?php $a = [1, 2, 3, 4, 5]; foreach ($a as $v) { if ($v == 3) { break; } echo $v; }",
    );
    assert_eq!(out, "12");
}

#[test]
fn test_array_in_function() {
    let out = compile_and_run(
        r#"<?php
function sum($arr) {
    $total = 0;
    foreach ($arr as $v) {
        $total += $v;
    }
    return $total;
}
echo sum([1, 2, 3, 4, 5]);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_string_array() {
    let out = compile_and_run(
        r#"<?php
$names = ["Alice", "Bob"];
$names[] = "Charlie";
echo count($names) . ": ";
foreach ($names as $n) { echo $n . " "; }
"#,
    );
    assert_eq!(out, "3: Alice Bob Charlie ");
}

// --- Array functions ---

#[test]
fn test_array_pop() {
    let out =
        compile_and_run("<?php $a = [1, 2, 3]; $v = array_pop($a); echo $v . \" \" . count($a);");
    assert_eq!(out, "3 2");
}

#[test]
fn test_array_pop_empty() {
    let out = compile_and_run("<?php $a = [1]; array_pop($a); echo array_pop($a);");
    assert_eq!(out, "");
}

#[test]
fn test_in_array_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(20, $a);");
    assert_eq!(out, "1");
}

#[test]
fn test_in_array_not_found() {
    let out = compile_and_run("<?php $a = [10, 20, 30]; echo in_array(99, $a);");
    assert_eq!(out, "0");
}

#[test]
fn test_in_array_string_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("b", $a);"#);
    assert_eq!(out, "1");
}

#[test]
fn test_in_array_string_not_found() {
    let out = compile_and_run(r#"<?php $a = ["a", "b", "c"]; echo in_array("x", $a);"#);
    assert_eq!(out, "0");
}

#[test]
fn test_sort() {
    let out =
        compile_and_run(r#"<?php $a = [5, 3, 1, 4, 2]; sort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "12345");
}

#[test]
fn test_rsort() {
    let out =
        compile_and_run(r#"<?php $a = [1, 3, 2]; rsort($a); foreach ($a as $v) { echo $v; }"#);
    assert_eq!(out, "321");
}

#[test]
fn test_array_keys() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $k = array_keys($a); foreach ($k as $v) { echo $v; }"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_isset() {
    let out = compile_and_run("<?php $x = 42; echo isset($x);");
    assert_eq!(out, "1");
}

#[test]
fn test_array_values() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $v = array_values($a); foreach ($v as $x) { echo $x; }"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_die() {
    let out = compile_and_run("<?php echo \"before\"; die(); echo \"after\";");
    assert_eq!(out, "before");
}

// --- Nested control flow ---

#[test]
fn test_nested_if() {
    let out = compile_and_run(
        "<?php $x = 5; if ($x > 0) { if ($x > 3) { echo \"big\"; } else { echo \"small\"; } }",
    );
    assert_eq!(out, "big");
}

#[test]
fn test_nested_loops() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 3; $i++) { for ($j = 0; $j < 2; $j++) { echo $i . $j . \" \"; } }",
    );
    assert_eq!(out, "00 01 10 11 20 21 ");
}

#[test]
fn test_for_continue() {
    let out =
        compile_and_run("<?php for ($i = 0; $i < 5; $i++) { if ($i == 2) { continue; } echo $i; }");
    assert_eq!(out, "0134");
}

#[test]
fn test_while_with_function() {
    let out = compile_and_run(
        r#"<?php
function sum_to($n) {
    $s = 0;
    $i = 1;
    while ($i <= $n) {
        $s = $s + $i;
        $i++;
    }
    return $s;
}
echo sum_to(10);
"#,
    );
    assert_eq!(out, "55");
}

#[test]
fn test_function_with_if_return() {
    let out = compile_and_run(
        r#"<?php
function abs_val($x) {
    if ($x < 0) {
        return -$x;
    }
    return $x;
}
echo abs_val(-5) . " " . abs_val(3);
"#,
    );
    assert_eq!(out, "5 3");
}

#[test]
fn test_function_calling_function() {
    let out = compile_and_run(
        r#"<?php
function square($x) { return $x * $x; }
function sum_of_squares($a, $b) { return square($a) + square($b); }
echo sum_of_squares(3, 4);
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_multiple_elseif() {
    let out = compile_and_run(
        r#"<?php
$x = 4;
if ($x == 1) { echo "one"; }
elseif ($x == 2) { echo "two"; }
elseif ($x == 3) { echo "three"; }
elseif ($x == 4) { echo "four"; }
else { echo "other"; }
"#,
    );
    assert_eq!(out, "four");
}

// --- Edge cases ---

#[test]
fn test_comments_ignored() {
    let out = compile_and_run("<?php\n// this is a comment\necho \"ok\";\n/* block comment */\n");
    assert_eq!(out, "ok");
}

#[test]
fn test_no_output_program() {
    let out = compile_and_run("<?php $x = 1;");
    assert_eq!(out, "");
}

#[test]
fn test_empty_string_concat() {
    let out = compile_and_run("<?php echo \"\" . \"hello\" . \"\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_deeply_nested_arithmetic() {
    let out = compile_and_run("<?php echo ((((1 + 2) * 3) - 4) / 5);");
    assert_eq!(out, "1");
}

// --- Float literals ---

#[test]
fn test_echo_float() {
    let out = compile_and_run("<?php echo 3.14;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_echo_float_integer_value() {
    let out = compile_and_run("<?php echo 4.0;");
    assert_eq!(out, "4");
}

#[test]
fn test_echo_negative_float() {
    let out = compile_and_run("<?php echo -3.14;");
    assert_eq!(out, "-3.14");
}

#[test]
fn test_echo_dot_prefix_float() {
    let out = compile_and_run("<?php echo .5;");
    assert_eq!(out, "0.5");
}

// --- Float arithmetic ---

#[test]
fn test_float_addition() {
    let out = compile_and_run("<?php echo 1.5 + 2.3;");
    assert_eq!(out, "3.8");
}

#[test]
fn test_float_subtraction() {
    let out = compile_and_run("<?php echo 5.5 - 2.2;");
    assert_eq!(out, "3.3");
}

#[test]
fn test_float_multiplication() {
    let out = compile_and_run("<?php echo 3.0 * 2.5;");
    assert_eq!(out, "7.5");
}

#[test]
fn test_float_division() {
    let out = compile_and_run("<?php echo 7.5 / 2.5;");
    assert_eq!(out, "3");
}

// --- Mixed int+float ---

#[test]
fn test_int_plus_float() {
    let out = compile_and_run("<?php echo 10 + 0.5;");
    assert_eq!(out, "10.5");
}

#[test]
fn test_float_plus_int() {
    let out = compile_and_run("<?php echo 0.5 + 10;");
    assert_eq!(out, "10.5");
}

#[test]
fn test_int_times_float() {
    let out = compile_and_run("<?php echo 3 * 1.5;");
    assert_eq!(out, "4.5");
}

// --- Float comparison ---

#[test]
fn test_float_greater_than() {
    let out = compile_and_run("<?php echo 3.14 > 2.0;");
    assert_eq!(out, "1");
}

#[test]
fn test_float_less_than() {
    let out = compile_and_run("<?php echo 1.5 < 2.5;");
    assert_eq!(out, "1");
}

#[test]
fn test_float_equal() {
    let out = compile_and_run("<?php echo 3.14 == 3.14;");
    assert_eq!(out, "1");
}

#[test]
fn test_float_not_equal() {
    let out = compile_and_run("<?php echo 3.14 != 2.0;");
    assert_eq!(out, "1");
}

// --- Float concatenation ---

#[test]
fn test_float_concat() {
    let out = compile_and_run("<?php echo \"pi=\" . 3.14;");
    assert_eq!(out, "pi=3.14");
}

#[test]
fn test_float_concat_reverse() {
    let out = compile_and_run("<?php echo 3.14 . \" is pi\";");
    assert_eq!(out, "3.14 is pi");
}

// --- Math functions ---

#[test]
fn test_floor() {
    let out = compile_and_run("<?php echo floor(3.7);");
    assert_eq!(out, "3");
}

#[test]
fn test_ceil() {
    let out = compile_and_run("<?php echo ceil(3.2);");
    assert_eq!(out, "4");
}

#[test]
fn test_round() {
    let out = compile_and_run("<?php echo round(3.5);");
    assert_eq!(out, "4");
}

#[test]
fn test_round_down() {
    let out = compile_and_run("<?php echo round(3.4);");
    assert_eq!(out, "3");
}

#[test]
fn test_sqrt() {
    let out = compile_and_run("<?php echo sqrt(16.0);");
    assert_eq!(out, "4");
}

#[test]
fn test_sqrt_non_perfect() {
    let out = compile_and_run("<?php echo sqrt(2.0);");
    assert_eq!(out, "1.4142135623731");
}

#[test]
fn test_abs_float() {
    let out = compile_and_run("<?php echo abs(-3.14);");
    assert_eq!(out, "3.14");
}

#[test]
fn test_abs_int() {
    let out = compile_and_run("<?php echo abs(-42);");
    assert_eq!(out, "42");
}

#[test]
fn test_pow() {
    let out = compile_and_run("<?php echo pow(2.0, 10.0);");
    assert_eq!(out, "1024");
}

#[test]
fn test_min_int() {
    let out = compile_and_run("<?php echo min(3, 7);");
    assert_eq!(out, "3");
}

#[test]
fn test_max_int() {
    let out = compile_and_run("<?php echo max(3, 7);");
    assert_eq!(out, "7");
}

#[test]
fn test_min_float() {
    let out = compile_and_run("<?php echo min(1.5, 2.5);");
    assert_eq!(out, "1.5");
}

#[test]
fn test_max_float() {
    let out = compile_and_run("<?php echo max(1.5, 2.5);");
    assert_eq!(out, "2.5");
}

#[test]
fn test_intdiv() {
    let out = compile_and_run("<?php echo intdiv(7, 2);");
    assert_eq!(out, "3");
}

// --- Type checking builtins ---

#[test]
fn test_floatval() {
    let out = compile_and_run("<?php echo floatval(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_is_float_true() {
    let out = compile_and_run("<?php echo is_float(3.14);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_float_false() {
    let out = compile_and_run("<?php echo is_float(42);");
    assert_eq!(out, "");
}

#[test]
fn test_is_int_true() {
    let out = compile_and_run("<?php echo is_int(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_int_false() {
    let out = compile_and_run("<?php echo is_int(3.14);");
    assert_eq!(out, "");
}

// --- Float variable ---

#[test]
fn test_float_variable() {
    let out = compile_and_run("<?php $x = 3.14; echo $x;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_float_variable_arithmetic() {
    let out = compile_and_run("<?php $a = 1.5; $b = 2.5; echo $a + $b;");
    assert_eq!(out, "4");
}

#[test]
fn test_float_in_condition() {
    let out =
        compile_and_run("<?php $x = 3.14; if ($x > 3.0) { echo \"yes\"; } else { echo \"no\"; }");
    assert_eq!(out, "yes");
}

// --- Strict comparison (=== / !==) ---

#[test]
fn test_strict_eq_int_same() {
    let out = compile_and_run("<?php echo 1 === 1;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_different() {
    let out = compile_and_run("<?php echo 1 === 2;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_same() {
    let out = compile_and_run("<?php echo 1 !== 1;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_different() {
    let out = compile_and_run("<?php echo 1 !== 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_vs_bool() {
    // 1 === true should be false (different types)
    let out = compile_and_run("<?php echo 1 === true;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_int_vs_bool() {
    // 1 !== true should be true (different types)
    let out = compile_and_run("<?php echo 1 !== true;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_int_vs_string() {
    // 1 === "1" should be false (different types)
    let out = compile_and_run("<?php echo 1 === \"1\";");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_string_same() {
    let out = compile_and_run("<?php echo \"hello\" === \"hello\";");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_string_different() {
    let out = compile_and_run("<?php echo \"hello\" === \"world\";");
    assert_eq!(out, "");
}

#[test]
fn test_strict_neq_string() {
    let out = compile_and_run("<?php echo \"abc\" !== \"def\";");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_true() {
    let out = compile_and_run("<?php echo true === true;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_false() {
    let out = compile_and_run("<?php echo false === false;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_bool_mixed() {
    let out = compile_and_run("<?php echo true === false;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_null() {
    let out = compile_and_run("<?php echo null === null;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_null_vs_int() {
    // null === 0 should be false
    let out = compile_and_run("<?php echo null === 0;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_null_vs_false() {
    // null === false should be false (different types)
    let out = compile_and_run("<?php echo null === false;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_float_same() {
    let out = compile_and_run("<?php echo 3.14 === 3.14;");
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_float_different() {
    let out = compile_and_run("<?php echo 3.14 === 2.71;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_float_vs_int() {
    // 1.0 === 1 should be false (different types)
    let out = compile_and_run("<?php echo 1.0 === 1;");
    assert_eq!(out, "");
}

#[test]
fn test_strict_eq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x === 5) {
    echo "yes";
} else {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_strict_neq_in_if() {
    let out = compile_and_run(
        r#"<?php
$x = "hello";
if ($x !== "world") {
    echo "different";
} else {
    echo "same";
}
"#,
    );
    assert_eq!(out, "different");
}

#[test]
fn test_strict_eq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "test";
$b = "test";
echo $a === $b;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_neq_string_variables() {
    let out = compile_and_run(
        r#"<?php
$a = "foo";
$b = "bar";
echo $a !== $b;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_eq_side_effects_preserved() {
    // Both operands must be evaluated even when types differ
    let out = compile_and_run(
        r#"<?php
function effect() { echo "X"; return 1; }
$r = 1.0 === effect();
echo $r;
"#,
    );
    assert_eq!(out, "X");
}

#[test]
fn test_strict_eq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 === 1;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_neq_assign_result() {
    let out = compile_and_run(
        r#"<?php
$x = 1 !== 2;
echo $x;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_strict_compare_mixed_uses_payload_type_and_value() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int_a" => 42,
    "int_b" => 42,
    "int_c" => 7,
    "str_a" => "42",
    "str_b" => "42",
    "bool_t" => true,
];
echo $map["int_a"] === $map["int_b"] ? "1" : "0";
echo $map["int_a"] === $map["int_c"] ? "1" : "0";
echo $map["int_a"] === $map["str_a"] ? "1" : "0";
echo $map["str_a"] === $map["str_b"] ? "1" : "0";
echo $map["int_a"] !== $map["str_a"] ? "1" : "0";
echo $map["bool_t"] === true ? "1" : "0";
"#,
    );
    assert_eq!(out, "100111");
}

// --- Include / Require ---

#[test]
fn test_include_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'helper.php'; echo greet();"),
            ("helper.php", "<?php function greet() { return \"hello\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_require_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php require 'math.php'; echo add(3, 4);"),
            ("math.php", "<?php function add($a, $b) { return $a + $b; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_include_with_parens() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include('helper.php'); echo greet();"),
            ("helper.php", "<?php function greet() { return \"hi\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hi");
}

#[test]
fn test_include_top_level_code() {
    // Top-level code in included file executes at the include point
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php echo \"before\"; include 'mid.php'; echo \"after\";",
            ),
            ("mid.php", "<?php echo \"middle\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "beforemiddleafter");
}

#[test]
fn test_include_once() {
    // include_once should only include the file once
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include_once 'counter.php';
include_once 'counter.php';
echo $x;
"#,
            ),
            ("counter.php", "<?php $x = 42;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_require_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once 'lib.php';
require_once 'lib.php';
echo double(5);
"#,
            ),
            ("lib.php", "<?php function double($n) { return $n * 2; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_include_nested() {
    // a.php includes b.php which includes c.php
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'a.php'; echo c_func();"),
            ("a.php", "<?php include 'b.php';"),
            ("b.php", "<?php include 'c.php';"),
            ("c.php", "<?php function c_func() { return \"deep\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "deep");
}

#[test]
fn test_include_subdirectory() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'lib/utils.php'; echo greet();"),
            (
                "lib/utils.php",
                "<?php function greet() { return \"from lib\"; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "from lib");
}

#[test]
fn test_include_variables_shared_scope() {
    // Variables from included file are in the same scope
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$prefix = "Hello";
include 'greet.php';
"#,
            ),
            ("greet.php", "<?php echo $prefix . \" World\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_include_multiple_files() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include 'a.php';
include 'b.php';
echo add(1, 2) . " " . mul(3, 4);
"#,
            ),
            ("a.php", "<?php function add($x, $y) { return $x + $y; }"),
            ("b.php", "<?php function mul($x, $y) { return $x * $y; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "3 12");
}

#[test]
fn test_circular_include_error() {
    assert!(compile_files_fails(
        &[
            ("main.php", "<?php include 'a.php';"),
            ("a.php", "<?php include 'b.php';"),
            ("b.php", "<?php include 'a.php';"),
        ],
        "main.php"
    ));
}

#[test]
fn test_require_missing_file_error() {
    assert!(compile_files_fails(
        &[("main.php", "<?php require 'nonexistent.php';"),],
        "main.php"
    ));
}

// --- Division returns float ---

#[test]
fn test_int_division_returns_float() {
    let out = compile_and_run("<?php echo 10 / 3;");
    assert_eq!(out, "3.3333333333333");
}

#[test]
fn test_int_division_exact() {
    // Even exact division returns float-formatted output
    let out = compile_and_run("<?php echo 10 / 2;");
    assert_eq!(out, "5");
}

#[test]
fn test_division_assign_updates_type() {
    let out = compile_and_run("<?php $x = 10; $x /= 3; echo $x;");
    assert_eq!(out, "3.3333333333333");
}

#[test]
fn test_division_in_expression() {
    let out = compile_and_run("<?php echo 1 / 3 + 1 / 3 + 1 / 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_intdiv_still_returns_int() {
    let out = compile_and_run("<?php echo intdiv(10, 3);");
    assert_eq!(out, "3");
}

#[test]
fn test_intdiv_exact() {
    let out = compile_and_run("<?php echo intdiv(10, 5);");
    assert_eq!(out, "2");
}

#[test]
fn test_intdiv_negative() {
    let out = compile_and_run("<?php echo intdiv(-7, 2);");
    assert_eq!(out, "-3");
}

// --- INF, NAN, is_nan, is_finite, is_infinite ---

#[test]
fn test_inf_constant() {
    let out = compile_and_run("<?php echo INF;");
    assert_eq!(out, "INF");
}

#[test]
fn test_nan_constant() {
    let out = compile_and_run("<?php echo NAN;");
    assert_eq!(out, "NAN");
}

#[test]
fn test_negative_inf() {
    let out = compile_and_run("<?php echo -INF;");
    assert_eq!(out, "-INF");
}

#[test]
fn test_is_nan_true() {
    let out = compile_and_run("<?php echo is_nan(NAN);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_nan_false() {
    let out = compile_and_run("<?php echo is_nan(42.0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_nan_int() {
    let out = compile_and_run("<?php echo is_nan(0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_infinite_true() {
    let out = compile_and_run("<?php echo is_infinite(INF);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_infinite_neg_inf() {
    let out = compile_and_run("<?php echo is_infinite(-INF);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_infinite_false() {
    let out = compile_and_run("<?php echo is_infinite(42.0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_finite_true() {
    let out = compile_and_run("<?php echo is_finite(42.0);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_finite_inf() {
    let out = compile_and_run("<?php echo is_finite(INF);");
    assert_eq!(out, "");
}

#[test]
fn test_is_finite_nan() {
    let out = compile_and_run("<?php echo is_finite(NAN);");
    assert_eq!(out, "");
}

#[test]
fn test_inf_arithmetic() {
    let out = compile_and_run("<?php echo INF + 1;");
    assert_eq!(out, "INF");
}

#[test]
fn test_division_by_zero_inf() {
    let out = compile_and_run("<?php echo 1.0 / 0.0;");
    assert_eq!(out, "INF");
}

// --- Type casting ---

#[test]
fn test_cast_int_from_float() {
    let out = compile_and_run("<?php echo (int)3.7;");
    assert_eq!(out, "3");
}

#[test]
fn test_cast_int_from_string() {
    let out = compile_and_run("<?php echo (int)\"42\";");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_int_from_bool() {
    let out = compile_and_run("<?php echo (int)true;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_float_from_int() {
    let out = compile_and_run("<?php echo (float)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_float_from_string() {
    let out = compile_and_run("<?php echo (float)'3.14';");
    assert_eq!(out, "3.14");
}

#[test]
fn test_cast_float_from_string_integer() {
    let out = compile_and_run("<?php echo (float)'42';");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_float_from_string_non_numeric() {
    let out = compile_and_run("<?php echo (float)'abc';");
    assert_eq!(out, "0");
}

#[test]
fn test_cast_string_from_int() {
    let out = compile_and_run("<?php echo (string)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_string_from_float() {
    let out = compile_and_run("<?php echo (string)3.14;");
    assert_eq!(out, "3.14");
}

#[test]
fn test_cast_string_from_bool_true() {
    let out = compile_and_run("<?php echo (string)true;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_string_from_bool_false() {
    let out = compile_and_run("<?php echo (string)false;");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_int_zero() {
    let out = compile_and_run("<?php echo (bool)0;");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_int_nonzero() {
    let out = compile_and_run("<?php echo (bool)42;");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_bool_from_string_empty() {
    let out = compile_and_run("<?php echo (bool)\"\";");
    assert_eq!(out, "");
}

#[test]
fn test_cast_bool_from_string_nonempty() {
    let out = compile_and_run("<?php echo (bool)\"hello\";");
    assert_eq!(out, "1");
}

#[test]
fn test_cast_mixed_unboxes_payload() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "int" => 42,
    "float" => 3.75,
    "true" => true,
    "false" => false,
    "null" => null,
    "text" => "27",
];
echo (int)$map["float"];
echo "|";
echo (int)$map["text"];
echo "|";
echo (bool)$map["int"] ? "1" : "0";
echo (bool)$map["false"] ? "1" : "0";
echo "|";
echo (string)$map["true"];
echo "|";
echo (string)$map["null"];
echo "|";
echo (string)$map["int"];
"#,
    );
    assert_eq!(out, "3|27|10|1||42");
}

#[test]
fn test_cast_integer_alias() {
    let out = compile_and_run("<?php echo (integer)3.7;");
    assert_eq!(out, "3");
}

#[test]
fn test_cast_double_alias() {
    let out = compile_and_run("<?php echo (double)42;");
    assert_eq!(out, "42");
}

#[test]
fn test_cast_boolean_alias() {
    let out = compile_and_run("<?php echo (boolean)1;");
    assert_eq!(out, "1");
}

// --- gettype ---

#[test]
fn test_gettype_int() {
    let out = compile_and_run("<?php echo gettype(42);");
    assert_eq!(out, "integer");
}

#[test]
fn test_gettype_float() {
    let out = compile_and_run("<?php echo gettype(3.14);");
    assert_eq!(out, "double");
}

#[test]
fn test_gettype_string() {
    let out = compile_and_run("<?php echo gettype(\"hi\");");
    assert_eq!(out, "string");
}

#[test]
fn test_gettype_bool() {
    let out = compile_and_run("<?php echo gettype(true);");
    assert_eq!(out, "boolean");
}

#[test]
fn test_gettype_null() {
    let out = compile_and_run("<?php echo gettype(null);");
    assert_eq!(out, "NULL");
}

#[test]
fn test_gettype_mixed_returns_concrete_payload_type() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "i" => 42,
    "s" => "hi",
    "n" => null,
    "a" => [1, 2],
    "b" => true,
];
echo gettype($map["i"]);
echo "|";
echo gettype($map["s"]);
echo "|";
echo gettype($map["n"]);
echo "|";
echo gettype($map["a"]);
echo "|";
echo gettype($map["b"]);
"#,
    );
    assert_eq!(out, "integer|string|NULL|array|boolean");
}

// --- empty ---

#[test]
fn test_empty_zero() {
    let out = compile_and_run("<?php echo empty(0);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_nonzero() {
    let out = compile_and_run("<?php echo empty(42);");
    assert_eq!(out, "");
}

#[test]
fn test_empty_empty_string() {
    let out = compile_and_run("<?php echo empty(\"\");");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_nonempty_string() {
    let out = compile_and_run("<?php echo empty(\"hi\");");
    assert_eq!(out, "");
}

#[test]
fn test_empty_null() {
    let out = compile_and_run("<?php echo empty(null);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_false() {
    let out = compile_and_run("<?php echo empty(false);");
    assert_eq!(out, "1");
}

#[test]
fn test_empty_true() {
    let out = compile_and_run("<?php echo empty(true);");
    assert_eq!(out, "");
}

#[test]
fn test_empty_mixed_uses_boxed_payload_semantics() {
    let out = compile_and_run(
        r#"<?php
$map = [
    "zero" => 0,
    "blank" => "",
    "null" => null,
    "arr" => [],
    "one" => 1,
    "text" => "hi",
];
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["blank"]) ? "1" : "0";
echo empty($map["null"]) ? "1" : "0";
echo empty($map["arr"]) ? "1" : "0";
echo empty($map["one"]) ? "1" : "0";
echo empty($map["text"]) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111100");
}

// --- unset ---

#[test]
fn test_unset_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
unset($x);
echo is_null($x);
"#,
    );
    assert_eq!(out, "1");
}

// --- settype ---

#[test]
fn test_settype_to_string() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
settype($x, "string");
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_settype_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = 3.7;
settype($x, "integer");
echo $x;
"#,
    );
    assert_eq!(out, "3");
}

// --- Missing type function tests ---

#[test]
fn test_boolval_true() {
    let out = compile_and_run("<?php echo boolval(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_boolval_false() {
    let out = compile_and_run("<?php echo boolval(0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_bool_true() {
    let out = compile_and_run("<?php echo is_bool(true);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_bool_false_for_int() {
    let out = compile_and_run("<?php echo is_bool(1);");
    assert_eq!(out, "");
}

#[test]
fn test_is_string_true() {
    let out = compile_and_run("<?php echo is_string(\"hello\");");
    assert_eq!(out, "1");
}

#[test]
fn test_is_string_false() {
    let out = compile_and_run("<?php echo is_string(42);");
    assert_eq!(out, "");
}

#[test]
fn test_is_numeric_int() {
    let out = compile_and_run("<?php echo is_numeric(42);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_numeric_float() {
    let out = compile_and_run("<?php echo is_numeric(3.14);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_numeric_string() {
    let out = compile_and_run("<?php echo is_numeric(\"hello\");");
    assert_eq!(out, "");
}

// --- Exponentiation operator ** ---

#[test]
fn test_pow_operator() {
    let out = compile_and_run("<?php echo 2 ** 10;");
    assert_eq!(out, "1024");
}

#[test]
fn test_pow_operator_float() {
    let out = compile_and_run("<?php echo 2.0 ** 0.5;");
    assert_eq!(out, "1.4142135623731");
}

#[test]
fn test_pow_right_associative() {
    // 2 ** 3 ** 2 = 2 ** 9 = 512
    let out = compile_and_run("<?php echo 2 ** 3 ** 2;");
    assert_eq!(out, "512");
}

#[test]
fn test_pow_higher_than_unary() {
    // -2 ** 2 = -(2**2) = -4
    let out = compile_and_run("<?php echo -2 ** 2;");
    assert_eq!(out, "-4");
}

#[test]
fn test_pow_higher_than_multiply() {
    // 3 * 2 ** 3 = 3 * 8 = 24
    let out = compile_and_run("<?php echo 3 * 2 ** 3;");
    assert_eq!(out, "24");
}

// --- fmod, fdiv ---

#[test]
fn test_fmod() {
    let out = compile_and_run("<?php echo fmod(10.5, 3.2);");
    assert_eq!(out, "0.9");
}

#[test]
fn test_fdiv() {
    let out = compile_and_run("<?php echo fdiv(10, 3);");
    assert_eq!(out, "3.3333333333333");
}

#[test]
fn test_fdiv_by_zero() {
    let out = compile_and_run("<?php echo fdiv(1, 0);");
    assert_eq!(out, "INF");
}

// --- rand, mt_rand, random_int ---

#[test]
fn test_rand_range() {
    // rand(1, 1) always returns 1
    let out = compile_and_run("<?php echo rand(1, 1);");
    assert_eq!(out, "1");
}

#[test]
fn test_mt_rand_range() {
    let out = compile_and_run("<?php echo mt_rand(5, 5);");
    assert_eq!(out, "5");
}

#[test]
fn test_random_int_range() {
    let out = compile_and_run("<?php echo random_int(42, 42);");
    assert_eq!(out, "42");
}

#[test]
fn test_rand_no_args() {
    // Just verify it doesn't crash and returns a non-negative number
    let out = compile_and_run("<?php $r = rand(); echo ($r >= 0 ? \"ok\" : \"bad\");");
    assert_eq!(out, "ok");
}

// --- number_format ---

#[test]
fn test_number_format_no_decimals() {
    let out = compile_and_run("<?php echo number_format(1234567);");
    assert_eq!(out, "1,234,567");
}

#[test]
fn test_number_format_with_decimals() {
    let out = compile_and_run("<?php echo number_format(1234.5678, 2);");
    assert_eq!(out, "1,234.57");
}

#[test]
fn test_number_format_small() {
    let out = compile_and_run("<?php echo number_format(42, 2);");
    assert_eq!(out, "42.00");
}

#[test]
fn test_number_format_negative() {
    let out = compile_and_run("<?php echo number_format(-1234.5, 1);");
    assert_eq!(out, "-1,234.5");
}

#[test]
fn test_number_format_custom_separators() {
    // European style: comma for decimal, dot for thousands
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ",", ".");"#);
    assert_eq!(out, "1.234.567,89");
}

#[test]
fn test_number_format_no_thousands() {
    // Empty string = no thousands separator
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ".", "");"#);
    assert_eq!(out, "1234567.89");
}

#[test]
fn test_number_format_space_thousands() {
    let out = compile_and_run(r#"<?php echo number_format(1234567, 0, ".", " ");"#);
    assert_eq!(out, "1 234 567");
}

// --- Constants ---

#[test]
fn test_php_int_max() {
    let out = compile_and_run("<?php echo PHP_INT_MAX;");
    assert_eq!(out, "9223372036854775807");
}

#[test]
fn test_php_int_min() {
    let out = compile_and_run("<?php echo PHP_INT_MIN;");
    assert_eq!(out, "-9223372036854775808");
}

#[test]
fn test_m_pi() {
    let out = compile_and_run("<?php echo M_PI;");
    assert_eq!(out, "3.1415926535898");
}

#[test]
fn test_php_float_max() {
    // Just verify it compiles and echoes without crash
    let out = compile_and_run("<?php echo is_float(PHP_FLOAT_MAX);");
    assert_eq!(out, "1");
}

// --- String functions (v0.4) ---

#[test]
fn test_substr_basic() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", 6);"#);
    assert_eq!(out, "World");
}

#[test]
fn test_substr_with_length() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", 0, 5);"#);
    assert_eq!(out, "Hello");
}

#[test]
fn test_substr_negative_offset() {
    let out = compile_and_run(r#"<?php echo substr("Hello World", -5);"#);
    assert_eq!(out, "World");
}

#[test]
fn test_strpos_found() {
    let out = compile_and_run(r#"<?php echo strpos("Hello World", "World");"#);
    assert_eq!(out, "6");
}

#[test]
fn test_strpos_not_found() {
    let out = compile_and_run(r#"<?php echo strpos("Hello", "xyz");"#);
    assert_eq!(out, "-1");
}

#[test]
fn test_strrpos() {
    let out = compile_and_run(r#"<?php echo strrpos("abcabc", "bc");"#);
    assert_eq!(out, "4");
}

#[test]
fn test_strstr_found() {
    let out = compile_and_run(r#"<?php echo strstr("user@example.com", "@");"#);
    assert_eq!(out, "@example.com");
}

#[test]
fn test_strtolower() {
    let out = compile_and_run(r#"<?php echo strtolower("Hello WORLD");"#);
    assert_eq!(out, "hello world");
}

#[test]
fn test_strtoupper() {
    let out = compile_and_run(r#"<?php echo strtoupper("Hello World");"#);
    assert_eq!(out, "HELLO WORLD");
}

#[test]
fn test_ucfirst() {
    let out = compile_and_run(r#"<?php echo ucfirst("hello");"#);
    assert_eq!(out, "Hello");
}

#[test]
fn test_lcfirst() {
    let out = compile_and_run(r#"<?php echo lcfirst("Hello");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_trim() {
    let out = compile_and_run("<?php echo trim(\"  hello  \");");
    assert_eq!(out, "hello");
}

#[test]
fn test_ltrim() {
    let out = compile_and_run("<?php echo ltrim(\"  hello\");");
    assert_eq!(out, "hello");
}

#[test]
fn test_rtrim() {
    let out = compile_and_run("<?php echo rtrim(\"hello  \");");
    assert_eq!(out, "hello");
}

#[test]
fn test_str_repeat() {
    let out = compile_and_run(r#"<?php echo str_repeat("ab", 3);"#);
    assert_eq!(out, "ababab");
}

#[test]
fn test_strrev() {
    let out = compile_and_run(r#"<?php echo strrev("Hello");"#);
    assert_eq!(out, "olleH");
}

#[test]
fn test_ord() {
    let out = compile_and_run(r#"<?php echo ord("A");"#);
    assert_eq!(out, "65");
}

#[test]
fn test_ord_empty_string() {
    let out = compile_and_run(r#"<?php echo ord("");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_chr() {
    let out = compile_and_run("<?php echo chr(65);");
    assert_eq!(out, "A");
}

#[test]
fn test_strcmp_equal() {
    let out = compile_and_run(r#"<?php echo strcmp("abc", "abc");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_strcmp_less() {
    let out = compile_and_run(r#"<?php echo (strcmp("abc", "abd") < 0 ? "yes" : "no");"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_strcasecmp() {
    let out = compile_and_run(r#"<?php echo strcasecmp("Hello", "hello");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_str_contains_true() {
    let out = compile_and_run(r#"<?php echo str_contains("Hello World", "World");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_str_contains_false() {
    let out = compile_and_run(r#"<?php echo str_contains("Hello", "xyz");"#);
    assert_eq!(out, "");
}

#[test]
fn test_str_starts_with_true() {
    let out = compile_and_run(r#"<?php echo str_starts_with("Hello World", "Hello");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_str_starts_with_false() {
    let out = compile_and_run(r#"<?php echo str_starts_with("Hello", "World");"#);
    assert_eq!(out, "");
}

#[test]
fn test_str_ends_with_true() {
    let out = compile_and_run(r#"<?php echo str_ends_with("Hello World", "World");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_str_ends_with_false() {
    let out = compile_and_run(r#"<?php echo str_ends_with("Hello", "xyz");"#);
    assert_eq!(out, "");
}

#[test]
fn test_str_replace() {
    let out = compile_and_run(r#"<?php echo str_replace("World", "PHP", "Hello World");"#);
    assert_eq!(out, "Hello PHP");
}

#[test]
fn test_str_replace_multiple() {
    let out = compile_and_run(r#"<?php echo str_replace("o", "0", "Hello World");"#);
    assert_eq!(out, "Hell0 W0rld");
}

#[test]
fn test_explode() {
    let out = compile_and_run(
        r#"<?php
$parts = explode(",", "a,b,c");
echo count($parts);
echo " ";
echo $parts[0] . " " . $parts[1] . " " . $parts[2];
"#,
    );
    assert_eq!(out, "3 a b c");
}

#[test]
fn test_implode() {
    let out = compile_and_run(
        r#"<?php
$arr = ["Hello", "World"];
echo implode(" ", $arr);
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_explode_implode_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$str = "one-two-three";
$parts = explode("-", $str);
echo implode(", ", $parts);
"#,
    );
    assert_eq!(out, "one, two, three");
}

// --- v0.4 batch 2: more string functions ---

#[test]
fn test_ucwords() {
    let out = compile_and_run(r#"<?php echo ucwords("hello world foo");"#);
    assert_eq!(out, "Hello World Foo");
}

#[test]
fn test_str_ireplace() {
    let out = compile_and_run(r#"<?php echo str_ireplace("WORLD", "PHP", "Hello World");"#);
    assert_eq!(out, "Hello PHP");
}

#[test]
fn test_substr_replace() {
    let out = compile_and_run(r#"<?php echo substr_replace("hello world", "PHP", 6, 5);"#);
    assert_eq!(out, "hello PHP");
}

#[test]
fn test_substr_replace_no_length() {
    let out = compile_and_run(r#"<?php echo substr_replace("hello world", "!", 5);"#);
    assert_eq!(out, "hello!");
}

#[test]
fn test_str_pad_right() {
    let out = compile_and_run(r#"<?php echo str_pad("hi", 5);"#);
    assert_eq!(out, "hi   ");
}

#[test]
fn test_str_pad_left() {
    let out = compile_and_run(r#"<?php echo str_pad("hi", 5, " ", 0);"#);
    assert_eq!(out, "   hi");
}

#[test]
fn test_str_pad_both() {
    let out = compile_and_run(r#"<?php echo str_pad("hi", 6, "-", 2);"#);
    assert_eq!(out, "--hi--");
}

#[test]
fn test_str_pad_custom_char() {
    let out = compile_and_run(r#"<?php echo str_pad("42", 5, "0", 0);"#);
    assert_eq!(out, "00042");
}

#[test]
fn test_str_split() {
    let out = compile_and_run(
        r#"<?php
$parts = str_split("Hello", 2);
echo count($parts) . " " . $parts[0] . " " . $parts[1] . " " . $parts[2];
"#,
    );
    assert_eq!(out, "3 He ll o");
}

#[test]
fn test_addslashes() {
    let out = compile_and_run(r#"<?php echo addslashes("He said \"hi\" and it's ok");"#);
    assert_eq!(out, r#"He said \"hi\" and it\'s ok"#);
}

#[test]
fn test_stripslashes() {
    let out = compile_and_run(r#"<?php echo stripslashes("He said \\\"hi\\\"");"#);
    assert_eq!(out, r#"He said "hi""#);
}

#[test]
fn test_nl2br() {
    let out = compile_and_run("<?php echo nl2br(\"line1\\nline2\");");
    assert_eq!(out, "line1<br />\nline2");
}

#[test]
fn test_wordwrap() {
    let out = compile_and_run(
        r#"<?php echo wordwrap("The quick brown fox jumped over the lazy dog", 15, "\n");"#,
    );
    assert!(out.contains('\n'));
}

#[test]
fn test_bin2hex() {
    let out = compile_and_run(r#"<?php echo bin2hex("AB");"#);
    assert_eq!(out, "4142");
}

#[test]
fn test_hex2bin() {
    let out = compile_and_run(r#"<?php echo hex2bin("4142");"#);
    assert_eq!(out, "AB");
}

#[test]
fn test_bin2hex_hex2bin_roundtrip() {
    let out = compile_and_run(r#"<?php echo hex2bin(bin2hex("Hello"));"#);
    assert_eq!(out, "Hello");
}

// --- v0.4 batch 3: encoding, URL, base64, ctype ---

#[test]
fn test_htmlspecialchars() {
    let out = compile_and_run(r#"<?php echo htmlspecialchars("<b>\"Hi\" & 'bye'</b>");"#);
    assert_eq!(
        out,
        "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;"
    );
}

#[test]
fn test_htmlentities() {
    let out = compile_and_run(r#"<?php echo htmlentities("<a>");"#);
    assert_eq!(out, "&lt;a&gt;");
}

#[test]
fn test_html_entity_decode() {
    let out = compile_and_run(r#"<?php echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;");"#);
    assert_eq!(out, "<b>hi</b>");
}

#[test]
fn test_htmlspecialchars_roundtrip() {
    let out = compile_and_run(
        r#"<?php echo html_entity_decode(htmlspecialchars("<div>\"test\"</div>"));"#,
    );
    assert_eq!(out, "<div>\"test\"</div>");
}

#[test]
fn test_urlencode() {
    let out = compile_and_run(r#"<?php echo urlencode("hello world&foo=bar");"#);
    assert_eq!(out, "hello+world%26foo%3Dbar");
}

#[test]
fn test_urldecode() {
    let out = compile_and_run(r#"<?php echo urldecode("hello+world%26foo%3Dbar");"#);
    assert_eq!(out, "hello world&foo=bar");
}

#[test]
fn test_rawurlencode() {
    let out = compile_and_run(r#"<?php echo rawurlencode("hello world");"#);
    assert_eq!(out, "hello%20world");
}

#[test]
fn test_rawurldecode() {
    let out = compile_and_run(r#"<?php echo rawurldecode("hello%20world");"#);
    assert_eq!(out, "hello world");
}

#[test]
fn test_base64_encode() {
    let out = compile_and_run(r#"<?php echo base64_encode("Hello");"#);
    assert_eq!(out, "SGVsbG8=");
}

#[test]
fn test_base64_decode() {
    let out = compile_and_run(r#"<?php echo base64_decode("SGVsbG8=");"#);
    assert_eq!(out, "Hello");
}

#[test]
fn test_base64_roundtrip() {
    let out = compile_and_run(r#"<?php echo base64_decode(base64_encode("Test 123!"));"#);
    assert_eq!(out, "Test 123!");
}

#[test]
fn test_ctype_alpha_true() {
    let out = compile_and_run(r#"<?php echo ctype_alpha("Hello");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_ctype_alpha_false() {
    let out = compile_and_run(r#"<?php echo ctype_alpha("Hello123");"#);
    assert_eq!(out, "");
}

#[test]
fn test_ctype_digit_true() {
    let out = compile_and_run(r#"<?php echo ctype_digit("12345");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_ctype_digit_false() {
    let out = compile_and_run(r#"<?php echo ctype_digit("123abc");"#);
    assert_eq!(out, "");
}

#[test]
fn test_ctype_alnum_true() {
    let out = compile_and_run(r#"<?php echo ctype_alnum("Hello123");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_ctype_alnum_false() {
    let out = compile_and_run(r#"<?php echo ctype_alnum("Hello 123");"#);
    assert_eq!(out, "");
}

#[test]
fn test_ctype_space_true() {
    let out = compile_and_run("<?php echo ctype_space(\" \\t\\n\");");
    assert_eq!(out, "1");
}

#[test]
fn test_ctype_space_false() {
    let out = compile_and_run(r#"<?php echo ctype_space("hello");"#);
    assert_eq!(out, "");
}

// --- sprintf / printf ---

#[test]
fn test_sprintf_string() {
    let out = compile_and_run(r#"<?php echo sprintf("Hello %s", "World");"#);
    assert_eq!(out, "Hello World");
}

#[test]
fn test_sprintf_int() {
    let out = compile_and_run(r#"<?php echo sprintf("Value: %d", 42);"#);
    assert_eq!(out, "Value: 42");
}

#[test]
fn test_sprintf_multiple() {
    let out = compile_and_run(r#"<?php echo sprintf("%s is %d", "age", 30);"#);
    assert_eq!(out, "age is 30");
}

#[test]
fn test_sprintf_percent() {
    let out = compile_and_run(r#"<?php echo sprintf("100%%");"#);
    assert_eq!(out, "100%");
}

#[test]
fn test_sprintf_hex() {
    let out = compile_and_run(r#"<?php echo sprintf("%x", 255);"#);
    assert_eq!(out, "ff");
}

#[test]
fn test_sprintf_zero_padded_int() {
    let out = compile_and_run(r#"<?php echo sprintf("%05d", 42);"#);
    assert_eq!(out, "00042");
}

#[test]
fn test_sprintf_precision_float() {
    let out = compile_and_run(r#"<?php echo sprintf("%.2f", 3.14159);"#);
    assert_eq!(out, "3.14");
}

#[test]
fn test_sprintf_width_string() {
    let out = compile_and_run(r#"<?php echo sprintf("%10s", "hi");"#);
    assert_eq!(out, "        hi");
}

#[test]
fn test_sprintf_left_align_string() {
    let out = compile_and_run(r#"<?php echo sprintf("%-10s|", "hi");"#);
    assert_eq!(out, "hi        |");
}

#[test]
fn test_sprintf_plus_sign() {
    let out = compile_and_run(r#"<?php echo sprintf("%+d", 42);"#);
    assert_eq!(out, "+42");
}

#[test]
fn test_sprintf_precision_float_trailing_zeros() {
    let out = compile_and_run(r#"<?php echo sprintf("%.5f", 1.0);"#);
    assert_eq!(out, "1.00000");
}

#[test]
fn test_sprintf_float_default() {
    let out = compile_and_run(r#"<?php echo sprintf("%f", 3.14);"#);
    assert_eq!(out, "3.140000");
}

#[test]
fn test_printf() {
    let out = compile_and_run(r#"<?php printf("Hello %s", "World");"#);
    assert_eq!(out, "Hello World");
}

// --- String interpolation ---

#[test]
fn test_string_interpolation_simple() {
    let out = compile_and_run(r#"<?php $name = "World"; echo "Hello $name";"#);
    assert_eq!(out, "Hello World");
}

#[test]
fn test_string_interpolation_multiple() {
    let out = compile_and_run(r#"<?php $a = "foo"; $b = "bar"; echo "$a and $b";"#);
    assert_eq!(out, "foo and bar");
}

#[test]
fn test_string_interpolation_at_start() {
    let out = compile_and_run(r#"<?php $x = "hi"; echo "$x there";"#);
    assert_eq!(out, "hi there");
}

#[test]
fn test_string_interpolation_at_end() {
    let out = compile_and_run(r#"<?php $x = "world"; echo "hello $x";"#);
    assert_eq!(out, "hello world");
}

#[test]
fn test_string_no_interpolation() {
    // Single-quoted strings should NOT interpolate
    let out = compile_and_run("<?php $x = 42; echo '$x';");
    assert_eq!(out, "$x");
}

#[test]
fn test_string_escaped_dollar() {
    let out = compile_and_run(r#"<?php echo "price is \$5";"#);
    assert_eq!(out, "price is $5");
}

// --- md5 / sha1 ---

#[test]
fn test_md5_empty() {
    let out = compile_and_run(r#"<?php echo md5("");"#);
    assert_eq!(out, "d41d8cd98f00b204e9800998ecf8427e");
}

#[test]
fn test_md5_hello() {
    let out = compile_and_run(r#"<?php echo md5("Hello");"#);
    assert_eq!(out, "8b1a9953c4611296a827abf8c47804d7");
}

#[test]
fn test_sha1_empty() {
    let out = compile_and_run(r#"<?php echo sha1("");"#);
    assert_eq!(out, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

#[test]
fn test_sha1_hello() {
    let out = compile_and_run(r#"<?php echo sha1("Hello");"#);
    assert_eq!(out, "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
}

// --- hash() ---

#[test]
fn test_hash_md5() {
    let out = compile_and_run(r#"<?php echo hash("md5", "Hello");"#);
    assert_eq!(out, "8b1a9953c4611296a827abf8c47804d7");
}

#[test]
fn test_hash_sha1() {
    let out = compile_and_run(r#"<?php echo hash("sha1", "Hello");"#);
    assert_eq!(out, "f7ff9e8b7bb2e09b70935a5d785e0cc5d9d0abf0");
}

#[test]
fn test_hash_sha256() {
    let out = compile_and_run(r#"<?php echo hash("sha256", "Hello");"#);
    assert_eq!(
        out,
        "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
    );
}

// --- sscanf() ---

#[test]
fn test_sscanf_int() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("Age: 25", "Age: %d");
echo $result[0];
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_sscanf_string() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("Name: Alice", "Name: %s");
echo $result[0];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_sscanf_multiple() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("John 30", "%s %d");
echo $result[0] . " " . $result[1];
"#,
    );
    assert_eq!(out, "John 30");
}

// --- Phase 11: v0.5 — I/O and file system ---

#[test]
fn test_print_basic() {
    let out = compile_and_run("<?php print \"hello\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_print_int() {
    let out = compile_and_run("<?php print 42;");
    assert_eq!(out, "42");
}

#[test]
fn test_stdin_constant() {
    let out = compile_and_run("<?php echo STDIN;");
    assert_eq!(out, "0");
}

#[test]
fn test_stdout_constant() {
    let out = compile_and_run("<?php echo STDOUT;");
    assert_eq!(out, "1");
}

#[test]
fn test_stderr_constant() {
    let out = compile_and_run("<?php echo STDERR;");
    assert_eq!(out, "2");
}

#[test]
fn test_var_dump_int() {
    let out = compile_and_run("<?php var_dump(42);");
    assert_eq!(out, "int(42)\n");
}

#[test]
fn test_var_dump_string() {
    let out = compile_and_run(r#"<?php var_dump("hello");"#);
    assert_eq!(out, "string(5) \"hello\"\n");
}

#[test]
fn test_var_dump_bool_true() {
    let out = compile_and_run("<?php var_dump(true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_var_dump_bool_false() {
    let out = compile_and_run("<?php var_dump(false);");
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_var_dump_null() {
    let out = compile_and_run("<?php var_dump(null);");
    assert_eq!(out, "NULL\n");
}

#[test]
fn test_var_dump_float() {
    let out = compile_and_run("<?php var_dump(3.14);");
    assert_eq!(out, "float(3.14)\n");
}

#[test]
fn test_var_dump_mixed_prints_concrete_payload() {
    let out = compile_and_run(
        r#"<?php
class Box {}

$map = [
    "i" => 42,
    "s" => "hello",
    "b" => true,
    "n" => null,
    "a" => [1, 2],
    "o" => new Box(),
];

var_dump($map["i"]);
var_dump($map["s"]);
var_dump($map["b"]);
var_dump($map["n"]);
var_dump($map["a"]);
var_dump($map["o"]);
"#,
    );
    assert_eq!(
        out,
        "int(42)\nstring(5) \"hello\"\nbool(true)\nNULL\narray(2) {\n}\nobject(Box)\n"
    );
}

#[test]
fn test_print_r_int() {
    let out = compile_and_run("<?php print_r(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_print_r_string() {
    let out = compile_and_run(r#"<?php print_r("hello");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_print_r_bool_true() {
    let out = compile_and_run("<?php print_r(true);");
    assert_eq!(out, "1");
}

#[test]
fn test_print_r_bool_false() {
    let out = compile_and_run("<?php print_r(false);");
    assert_eq!(out, "");
}

#[test]
fn test_print_r_array() {
    let out = compile_and_run("<?php print_r([1, 2, 3]);");
    assert_eq!(out, "Array\n");
}

#[test]
fn test_file_put_get_contents() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("test.txt", "hello world");
echo file_get_contents("test.txt");
"#,
    );
    assert_eq!(out, "hello world");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_exists() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("exists.txt", "data");
if (file_exists("exists.txt")) {
    echo "yes";
}
if (!file_exists("nope.txt")) {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yesno");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_filesize() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("size.txt", "12345");
echo filesize("size.txt");
"#,
    );
    assert_eq!(out, "5");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_is_file_is_dir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("afile.txt", "x");
mkdir("adir");
if (is_file("afile.txt")) { echo "F"; }
if (!is_dir("afile.txt")) { echo "!D"; }
if (is_dir("adir")) { echo "D"; }
if (!is_file("adir")) { echo "!F"; }
rmdir("adir");
"#,
    );
    assert_eq!(out, "F!DD!F");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_mkdir_rmdir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("testdir");
if (is_dir("testdir")) { echo "made"; }
rmdir("testdir");
if (!is_dir("testdir")) { echo "gone"; }
"#,
    );
    assert_eq!(out, "madegone");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_copy_unlink() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "content");
copy("orig.txt", "dup.txt");
echo file_get_contents("dup.txt");
unlink("dup.txt");
if (!file_exists("dup.txt")) { echo "|gone"; }
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "content|gone");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_rename_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("old.txt", "data");
rename("old.txt", "new.txt");
echo file_get_contents("new.txt");
if (!file_exists("old.txt")) { echo "|moved"; }
unlink("new.txt");
"#,
    );
    assert_eq!(out, "data|moved");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fopen_fwrite_fclose_fread() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("rw.txt", "w");
fwrite($f, "test data");
fclose($f);
$f = fopen("rw.txt", "r");
$content = fread($f, 9);
fclose($f);
echo $content;
unlink("rw.txt");
"#,
    );
    assert_eq!(out, "test data");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fgets_stdin() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = fgets(STDIN);
echo "got: " . $line;
"#,
        "hello\n",
    );
    assert_eq!(out, "got: hello\n");
}

#[test]
fn test_fopen_nonexistent_fgets_no_hang() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("no_such_file.txt", "r");
$line = fgets($f);
echo "done";
"#,
    );
    assert_eq!(out, "done");
}

#[test]
fn test_readline() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = readline();
echo "read: " . trim($line);
"#,
        "world\n",
    );
    assert_eq!(out, "read: world");
}

#[test]
fn test_file_lines() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("lines.txt", "one\ntwo\nthree\n");
$lines = file("lines.txt");
echo count($lines);
unlink("lines.txt");
"#,
    );
    assert_eq!(out, "3");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_getcwd() {
    let out = compile_and_run(
        r#"<?php
$cwd = getcwd();
if (strlen($cwd) > 0) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_sys_get_temp_dir() {
    let out = compile_and_run(
        r#"<?php
$tmp = sys_get_temp_dir();
echo $tmp;
"#,
    );
    assert!(out.contains("tmp") || out.contains("Tmp"));
}

#[test]
fn test_fseek_ftell() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek.txt", "abcdefghij");
$f = fopen("seek.txt", "r");
$result = fseek($f, 5);
echo $result;
echo ftell($f);
$data = fread($f, 5);
echo $data;
fclose($f);
unlink("seek.txt");
"#,
    );
    assert_eq!(out, "05fghij");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fseek_return_value() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek2.txt", "hello world");
$f = fopen("seek2.txt", "r");
$r1 = fseek($f, 0);
echo $r1;
$r2 = fseek($f, 3, 0);
echo $r2;
$r3 = fseek($f, 2, 1);
echo $r3;
echo ftell($f);
fclose($f);
unlink("seek2.txt");
"#,
    );
    assert_eq!(out, "0005");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_is_readable_writable() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("perm.txt", "x");
if (is_readable("perm.txt")) { echo "R"; }
if (is_writable("perm.txt")) { echo "W"; }
unlink("perm.txt");
"#,
    );
    assert_eq!(out, "RW");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_chdir_getcwd() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("subdir");
$before = getcwd();
chdir("subdir");
$after = getcwd();
if (strlen($after) > strlen($before)) { echo "changed"; }
chdir("..");
rmdir("subdir");
"#,
    );
    assert_eq!(out, "changed");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_var_dump_multiple() {
    let out = compile_and_run(
        r#"<?php
var_dump(1);
var_dump("hi");
var_dump(true);
"#,
    );
    assert_eq!(out, "int(1)\nstring(2) \"hi\"\nbool(true)\n");
}

// --- File I/O: CSV, timestamps, directory listing, temp files, seek/rewind/eof ---

#[test]
fn test_fgetcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("data.csv", "alice,30,NY\n");
$f = fopen("data.csv", "r");
$row = fgetcsv($f);
echo $row[0];
fclose($f);
unlink("data.csv");
"#,
    );
    assert_eq!(out, "alice");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fputcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("out.csv", "w");
$data = ["hello", "world"];
fputcsv($f, $data);
fclose($f);
$content = file_get_contents("out.csv");
echo trim($content);
unlink("out.csv");
"#,
    );
    assert_eq!(out, "hello,world");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_filemtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ts.txt", "x");
$t = filemtime("ts.txt");
if ($t > 1000000000) { echo "ok"; }
unlink("ts.txt");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_scandir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("sd");
file_put_contents("sd/a.txt", "a");
file_put_contents("sd/b.txt", "b");
$files = scandir("sd");
echo count($files);
unlink("sd/a.txt");
unlink("sd/b.txt");
rmdir("sd");
"#,
    );
    assert_eq!(out, "4");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_glob_fn() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gd");
file_put_contents("gd/g1.txt", "a");
file_put_contents("gd/g2.txt", "b");
$matches = glob("gd/*.txt");
if (count($matches) >= 2) { echo "ok"; }
unlink("gd/g1.txt");
unlink("gd/g2.txt");
rmdir("gd");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_tempnam() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = tempnam(".", "test");
if (file_exists($tmp)) { echo "ok"; }
unlink($tmp);
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_rewind() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rw.txt", "abcdef");
$f = fopen("rw.txt", "r");
$first = fread($f, 3);
rewind($f);
$again = fread($f, 3);
fclose($f);
echo $first . "|" . $again;
unlink("rw.txt");
"#,
    );
    assert_eq!(out, "abc|abc");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_feof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("eof.txt", "hi");
$f = fopen("eof.txt", "r");
$data = fread($f, 2);
$data = fread($f, 1);
if (feof($f)) { echo "eof"; }
fclose($f);
unlink("eof.txt");
"#,
    );
    assert_eq!(out, "eof");
    let _ = fs::remove_dir_all(&dir);
}

// --- Phase 12: v0.6 — Associative arrays, switch, match ---

#[test]
fn test_assoc_array_basic() {
    let out = compile_and_run(
        r#"<?php
$m = ["name" => "Alice", "city" => "NYC"];
echo $m["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_assoc_array_int_values() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1, "b" => 2, "c" => 3];
echo $m["a"] + $m["b"] + $m["c"];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_assoc_array_assign() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 10];
$m["y"] = 20;
echo $m["x"] + $m["y"];
"#,
    );
    assert_eq!(out, "30");
}

#[test]
fn test_assoc_array_update() {
    let out = compile_and_run(
        r#"<?php
$m = ["key" => "old"];
$m["key"] = "new";
echo $m["key"];
"#,
    );
    assert_eq!(out, "new");
}

#[test]
fn test_assoc_foreach_key_value() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "1", "b" => "2"];
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "a=1 b=2 ");
}

#[test]
fn test_assoc_foreach_preserves_order_after_overwrite() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "1", "b" => "2"];
$m["a"] = "3";
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(out, "a=3 b=2 ");
}

#[test]
fn test_assoc_foreach_preserves_order_after_growth() {
    let out = compile_and_run(
        r#"<?php
$m = ["k0" => "0"];
$m["k1"] = "1";
$m["k2"] = "2";
$m["k3"] = "3";
$m["k4"] = "4";
$m["k5"] = "5";
$m["k6"] = "6";
$m["k7"] = "7";
$m["k8"] = "8";
$m["k9"] = "9";
$m["k10"] = "10";
$m["k11"] = "11";
$m["k12"] = "12";
foreach ($m as $k => $v) {
    echo $k . "=" . $v . " ";
}
"#,
    );
    assert_eq!(
        out,
        "k0=0 k1=1 k2=2 k3=3 k4=4 k5=5 k6=6 k7=7 k8=8 k9=9 k10=10 k11=11 k12=12 "
    );
}

#[test]
fn test_indexed_foreach_key_value() {
    let out = compile_and_run(
        r#"<?php
$arr = [10, 20, 30];
foreach ($arr as $i => $v) {
    echo $i . ":" . $v . " ";
}
"#,
    );
    assert_eq!(out, "0:10 1:20 2:30 ");
}

#[test]
fn test_switch_basic() {
    let out = compile_and_run(
        r#"<?php
$x = 2;
switch ($x) {
    case 1:
        echo "one";
        break;
    case 2:
        echo "two";
        break;
    case 3:
        echo "three";
        break;
}
"#,
    );
    assert_eq!(out, "two");
}

#[test]
fn test_switch_default() {
    let out = compile_and_run(
        r#"<?php
$x = 99;
switch ($x) {
    case 1:
        echo "one";
        break;
    default:
        echo "other";
        break;
}
"#,
    );
    assert_eq!(out, "other");
}

#[test]
fn test_switch_fallthrough() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
switch ($x) {
    case 1:
        echo "a";
    case 2:
        echo "b";
        break;
    case 3:
        echo "c";
        break;
}
"#,
    );
    assert_eq!(out, "ab");
}

#[test]
fn test_switch_string() {
    let out = compile_and_run(
        r#"<?php
$s = "hello";
switch ($s) {
    case "hi":
        echo "A";
        break;
    case "hello":
        echo "B";
        break;
    default:
        echo "C";
        break;
}
"#,
    );
    assert_eq!(out, "B");
}

#[test]
fn test_match_basic() {
    let out = compile_and_run(
        r#"<?php
$x = 2;
$result = match($x) {
    1 => "one",
    2 => "two",
    3 => "three",
    default => "other",
};
echo $result;
"#,
    );
    assert_eq!(out, "two");
}

#[test]
fn test_match_default() {
    let out = compile_and_run(
        r#"<?php
$x = 99;
echo match($x) {
    1 => "one",
    default => "unknown",
};
"#,
    );
    assert_eq!(out, "unknown");
}

// --- Phase 13: v0.6 — Array functions ---

#[test]
fn test_array_reverse() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
$b = array_reverse($a);
echo $b[0] . $b[1] . $b[2];
"#,
    );
    assert_eq!(out, "213");
}

#[test]
fn test_array_sum() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_sum($a);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_array_product() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3, 4];
echo array_product($a);
"#,
    );
    assert_eq!(out, "24");
}

#[test]
fn test_array_search() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo array_search(20, $a);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
if (array_key_exists(1, $a)) { echo "yes"; }
if (!array_key_exists(5, $a)) { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_array_merge() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = array_merge($a, $b);
echo count($c);
echo $c[0] . $c[1] . $c[2] . $c[3];
"#,
    );
    assert_eq!(out, "41234");
}

#[test]
fn test_array_slice() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30, 40, 50];
$b = array_slice($a, 1, 3);
echo $b[0] . " " . $b[1] . " " . $b[2];
"#,
    );
    assert_eq!(out, "20 30 40");
}

#[test]
fn test_array_shift() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$first = array_shift($a);
echo $first . " " . count($a);
"#,
    );
    assert_eq!(out, "10 2");
}

#[test]
fn test_array_shift_empty() {
    let out = compile_and_run("<?php $a = [1]; array_shift($a); echo array_shift($a);");
    assert_eq!(out, "");
}

#[test]
fn test_array_unshift() {
    let out = compile_and_run(
        r#"<?php
$a = [2, 3];
$n = array_unshift($a, 1);
echo $n . " " . $a[0];
"#,
    );
    assert_eq!(out, "3 1");
}

#[test]
fn test_range() {
    let out = compile_and_run(
        r#"<?php
$a = range(1, 5);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:12345");
}

#[test]
fn test_range_descending() {
    let out = compile_and_run(
        r#"<?php
$a = range(5, 1);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "5:54321");
}

#[test]
fn test_range_single_element() {
    let out = compile_and_run(
        r#"<?php
$a = range(3, 3);
echo count($a) . ":";
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "1:3");
}

#[test]
fn test_array_unique() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 2, 3, 3, 3];
$b = array_unique($a);
echo count($b);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_array_fill() {
    let out = compile_and_run(
        r#"<?php
$a = array_fill(0, 3, 42);
echo $a[0] . " " . $a[1] . " " . $a[2];
"#,
    );
    assert_eq!(out, "42 42 42");
}

#[test]
fn test_array_diff() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4];
$c = array_diff($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_intersect() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4];
$b = [2, 4, 6];
$c = array_intersect($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_rand() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$i = array_rand($a);
if ($i >= 0 && $i < 3) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_shuffle() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
shuffle($a);
echo count($a);
echo array_sum($a);
"#,
    );
    assert_eq!(out, "515");
}

#[test]
fn test_array_pad() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = array_pad($a, 5, 0);
echo count($b);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_array_splice() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$removed = array_splice($a, 1, 2);
echo count($removed) . " " . count($a);
"#,
    );
    assert_eq!(out, "2 3");
}

#[test]
fn test_array_combine() {
    let out = compile_and_run(
        r#"<?php
$keys = ["a", "b"];
$vals = [1, 2];
$m = array_combine($keys, $vals);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_flip() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$f = array_flip($a);
echo count($f);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_array_chunk() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3, 4, 5];
$c = array_chunk($a, 2);
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_array_fill_keys() {
    let out = compile_and_run(
        r#"<?php
$keys = ["x", "y"];
$m = array_fill_keys($keys, 0);
echo count($m);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_diff_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_diff_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_gc_array_diff_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [1, 2], "drop" => [3, 4]];
$mask = ["drop" => 1];
$filtered = array_diff_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_intersect_key() {
    let out = compile_and_run(
        r#"<?php
$a = ["a" => "1", "b" => "2"];
$b = ["a" => "9"];
$c = array_intersect_key($a, $b);
echo count($c);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_gc_array_intersect_key_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$src = ["keep" => [5, 6], "drop" => [7, 8]];
$mask = ["keep" => 1];
$filtered = array_intersect_key($src, $mask);
unset($src);
$saved = $filtered["keep"];
echo $saved[0] . "|" . $saved[1];
"#,
    );
    assert_eq!(out, "5|6");
}

#[test]
fn test_asort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
asort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_arsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 3, 2];
arsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_ksort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
ksort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_krsort() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
krsort($a);
echo count($a);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_natsort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natsort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_natcasesort() {
    let out = compile_and_run(
        r#"<?php
$a = [3, 1, 2];
natcasesort($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "1");
}

// --- Associative array function tests ---

#[test]
fn test_assoc_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$m = ["name" => "Alice", "age" => "30"];
if (array_key_exists("name", $m)) { echo "yes"; }
if (array_key_exists("missing", $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_assoc_in_array_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "apple", "b" => "banana"];
if (in_array("apple", $m)) { echo "yes"; }
if (in_array("cherry", $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_assoc_in_array_int() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 10, "y" => 20];
if (in_array(10, $m)) { echo "yes"; }
if (in_array(99, $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

#[test]
fn test_assoc_array_search_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "Alice", "second" => "Bob"];
$key = array_search("Bob", $m);
echo $key;
"#,
    );
    assert_eq!(out, "second");
}

#[test]
fn test_assoc_array_keys() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 1, "y" => 2];
$keys = array_keys($m);
$n = count($keys);
for ($i = 0; $i < $n; $i++) {
    echo $keys[$i] . " ";
}
"#,
    );
    assert_eq!(out, "x y ");
}

#[test]
fn test_assoc_array_search_returns_first_inserted_matching_key() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "same", "second" => "same", "third" => "other"];
$key = array_search("same", $m);
echo $key;
"#,
    );
    assert_eq!(out, "first");
}

#[test]
fn test_assoc_array_values_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "one", "b" => "two"];
$vals = array_values($m);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i] . " ";
}
"#,
    );
    assert_eq!(out, "one two ");
}

#[test]
fn test_assoc_array_values_int() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 10, "b" => 20, "c" => 30];
$vals = array_values($m);
echo $vals[0] + $vals[1] + $vals[2];
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_assoc_array_mixed_foreach() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
foreach ($m as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "id=7;name=Alice;score=12;");
}

#[test]
fn test_assoc_array_values_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
$vals = array_values($m);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i];
    echo ",";
}
"#,
    );
    assert_eq!(out, "7,Alice,12,");
}

#[test]
fn test_assoc_in_array_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
if (in_array("Alice", $m)) { echo "name"; }
if (in_array(12, $m)) { echo " score"; }
if (!in_array("Bob", $m)) { echo " missing"; }
"#,
    );
    assert_eq!(out, "name score missing");
}

#[test]
fn test_assoc_array_search_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
echo array_search("Alice", $m);
echo ":";
echo array_search(12, $m);
"#,
    );
    assert_eq!(out, "name:score");
}

#[test]
fn test_assoc_array_access_mixed_echo() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
echo $m["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_gc_assoc_array_values_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$map = ["nums" => [7, 8, 9]];
$vals = array_values($map);
unset($map);
$saved = $vals[0];
echo $saved[1];
"#,
    );
    assert_eq!(out, "8");
}

// --- Phase 14: Multi-dimensional arrays ---

#[test]
fn test_nested_array_create_access() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2], [3, 4]];
echo $a[0][0] . " " . $a[0][1] . " " . $a[1][0] . " " . $a[1][1];
"#,
    );
    assert_eq!(out, "1 2 3 4");
}

#[test]
fn test_nested_array_count() {
    let out = compile_and_run(
        r#"<?php
$a = [[10, 20], [30, 40], [50, 60]];
echo count($a) . " " . count($a[0]);
"#,
    );
    assert_eq!(out, "3 2");
}

#[test]
fn test_nested_array_push() {
    let out = compile_and_run(
        r#"<?php
$a = [[1, 2]];
$a[] = [3, 4];
echo count($a) . " " . $a[1][0];
"#,
    );
    assert_eq!(out, "2 3");
}

#[test]
fn test_nested_array_foreach() {
    let out = compile_and_run(
        r#"<?php
$matrix = [[1, 2], [3, 4]];
foreach ($matrix as $row) {
    foreach ($row as $v) {
        echo $v . " ";
    }
}
"#,
    );
    assert_eq!(out, "1 2 3 4 ");
}

#[test]
fn test_nested_array_3_levels() {
    let out = compile_and_run(
        r#"<?php
$a = [[[1]]];
echo $a[0][0][0];
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_nested_array_string_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [["hello", "world"], ["foo", "bar"]];
echo $a[0][0] . " " . $a[1][1];
"#,
    );
    assert_eq!(out, "hello bar");
}

#[test]
fn test_array_column() {
    let out = compile_and_run(
        r#"<?php
$users = [
    ["name" => "Alice", "age" => "30"],
    ["name" => "Bob", "age" => "25"],
    ["name" => "Charlie", "age" => "35"],
];
$names = array_column($users, "name");
echo count($names);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_gc_array_column_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$rows = [
    ["nums" => [4, 5]],
    ["nums" => [6, 7]],
];
$cols = array_column($rows, "nums");
unset($rows);
$first = $cols[0];
$second = $cols[1];
echo $first[1] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "5|6");
}

// --- Callback-based array functions ---

#[test]
fn test_array_map() {
    let out = compile_and_run(
        r#"<?php
function double($x) { return $x * 2; }
$a = [1, 2, 3];
$b = array_map("double", $a);
echo $b[0] . $b[1] . $b[2];
"#,
    );
    assert_eq!(out, "246");
}

#[test]
fn test_array_map_single() {
    let out = compile_and_run(
        r#"<?php
function inc($x) { return $x + 1; }
$a = [10];
$b = array_map("inc", $a);
echo $b[0];
"#,
    );
    assert_eq!(out, "11");
}

#[test]
fn test_array_filter() {
    let out = compile_and_run(
        r#"<?php
function is_even($x) { return $x % 2 == 0; }
$a = [1, 2, 3, 4, 5, 6];
$b = array_filter($a, "is_even");
echo count($b);
foreach ($b as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "3246");
}

#[test]
fn test_array_filter_none_pass() {
    let out = compile_and_run(
        r#"<?php
function never($x) { return 0; }
$a = [1, 2, 3];
$b = array_filter($a, "never");
echo count($b);
"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_array_reduce() {
    let out = compile_and_run(
        r#"<?php
function add($carry, $item) { return $carry + $item; }
$a = [1, 2, 3, 4, 5];
$sum = array_reduce($a, "add", 0);
echo $sum;
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_array_reduce_with_initial() {
    let out = compile_and_run(
        r#"<?php
function mul($carry, $item) { return $carry * $item; }
$a = [2, 3, 4];
$product = array_reduce($a, "mul", 1);
echo $product;
"#,
    );
    assert_eq!(out, "24");
}

#[test]
fn test_array_walk() {
    let out = compile_and_run(
        r#"<?php
function show($x) { echo $x; }
$a = [10, 20, 30];
array_walk($a, "show");
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_usort() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [5, 3, 1, 4, 2];
usort($a, "cmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "12345");
}

#[test]
fn test_usort_reverse() {
    let out = compile_and_run(
        r#"<?php
function rcmp($a, $b) { return $b - $a; }
$a = [1, 3, 2];
usort($a, "rcmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "321");
}

#[test]
fn test_uksort() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [5, 3, 1, 4, 2];
uksort($a, "cmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "12345");
}

#[test]
fn test_uasort() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [30, 10, 20];
uasort($a, "cmp");
foreach ($a as $value) { echo $value . " "; }
"#,
    );
    assert_eq!(out, "10 20 30 ");
}

#[test]
fn test_call_user_func() {
    let out = compile_and_run(
        r#"<?php
function greet($x) { return $x + 100; }
$result = call_user_func("greet", 42);
echo $result;
"#,
    );
    assert_eq!(out, "142");
}

#[test]
fn test_call_user_func_no_args() {
    let out = compile_and_run(
        r#"<?php
function get_value() { return 99; }
$result = call_user_func("get_value");
echo $result;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_function_exists_true() {
    let out = compile_and_run(
        r#"<?php
function my_func() { return 1; }
if (function_exists("my_func")) { echo "yes"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_function_exists_false() {
    let out = compile_and_run(
        r#"<?php
if (function_exists("nonexistent")) { echo "yes"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "no");
}

#[test]
fn test_usort_already_sorted() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [1, 2, 3];
usort($a, "cmp");
foreach ($a as $v) { echo $v; }
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_usort_single_element() {
    let out = compile_and_run(
        r#"<?php
function cmp($a, $b) { return $a - $b; }
$a = [42];
usort($a, "cmp");
echo $a[0];
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_array_map_with_complex_callback() {
    let out = compile_and_run(
        r#"<?php
function square($x) { return $x * $x; }
$a = [1, 2, 3, 4];
$b = array_map("square", $a);
echo $b[0] . " " . $b[1] . " " . $b[2] . " " . $b[3];
"#,
    );
    assert_eq!(out, "1 4 9 16");
}

#[test]
fn test_array_reduce_single() {
    let out = compile_and_run(
        r#"<?php
function add($carry, $item) { return $carry + $item; }
$a = [42];
$sum = array_reduce($a, "add", 100);
echo $sum;
"#,
    );
    assert_eq!(out, "142");
}

// --- Anonymous functions (closures) and arrow functions ---

#[test]
fn test_closure_basic() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(5);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_closure_multiple_params() {
    let out = compile_and_run(
        r#"<?php
$add = function($a, $b) { return $a + $b; };
echo $add(3, 7);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_arrow_function_basic() {
    let out = compile_and_run(
        r#"<?php
$triple = fn($x) => $x * 3;
echo $triple(4);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_arrow_function_expression() {
    let out = compile_and_run(
        r#"<?php
$calc = fn($x) => $x * $x + 1;
echo $calc(5);
"#,
    );
    assert_eq!(out, "26");
}

#[test]
fn test_closure_array_map() {
    let out = compile_and_run(
        r#"<?php
$result = array_map(function($x) { return $x * 10; }, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_arrow_function_array_map() {
    let out = compile_and_run(
        r#"<?php
$result = array_map(fn($x) => $x + 100, [1, 2, 3]);
echo $result[0];
echo $result[1];
echo $result[2];
"#,
    );
    assert_eq!(out, "101102103");
}

#[test]
fn test_closure_array_filter() {
    let out = compile_and_run(
        r#"<?php
$evens = array_filter([1, 2, 3, 4, 5, 6], function($x) { return $x % 2 == 0; });
echo count($evens);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_arrow_function_array_filter() {
    let out = compile_and_run(
        r#"<?php
$big = array_filter([1, 5, 10, 15, 20], fn($x) => $x > 8);
echo count($big);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_closure_as_variable_then_call() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x) { return $x + 1; };
$a = $fn(10);
$b = $fn(20);
echo $a;
echo $b;
"#,
    );
    assert_eq!(out, "1121");
}

#[test]
fn test_closure_no_params() {
    let out = compile_and_run(
        r#"<?php
$hello = function() { return 42; };
echo $hello();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_arrow_no_params() {
    let out = compile_and_run(
        r#"<?php
$val = fn() => 99;
echo $val();
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_closure_array_reduce() {
    let out = compile_and_run(
        r#"<?php
$sum = array_reduce([1, 2, 3, 4], function($carry, $item) { return $carry + $item; }, 0);
echo $sum;
"#,
    );
    assert_eq!(out, "10");
}

// --- IIFE (Immediately Invoked Function Expression) ---

#[test]
fn test_iife_basic() {
    let out = compile_and_run(
        r#"<?php
echo (function() { return 42; })();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_iife_with_args() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 3; })(7);
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_iife_arrow() {
    let out = compile_and_run(
        r#"<?php
echo (fn($x) => $x + 100)(5);
"#,
    );
    assert_eq!(out, "105");
}

// --- Calling closures from array access ---

#[test]
fn test_closure_from_array_call() {
    let out = compile_and_run(
        r#"<?php
$fns = [function($x) { return $x * 10; }];
echo $fns[0](5);
"#,
    );
    assert_eq!(out, "50");
}

#[test]
fn test_closure_from_array_no_args() {
    let out = compile_and_run(
        r#"<?php
$fns = [function() { return 99; }];
echo $fns[0]();
"#,
    );
    assert_eq!(out, "99");
}

// --- Closure returning closure ---

#[test]
fn test_closure_returning_closure() {
    let out = compile_and_run(
        r#"<?php
$f = function() { return function() { return 99; }; };
$g = $f();
echo $g();
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_closure_returning_closure_with_args() {
    let out = compile_and_run(
        r#"<?php
$maker = function() { return function($x) { return $x * 3; }; };
$fn = $maker();
echo $fn(7);
"#,
    );
    assert_eq!(out, "21");
}

// ===== Feature 1: Default parameter values =====

#[test]
fn test_default_param_string() {
    let out = compile_and_run(
        r#"<?php
function greet($name = "world") {
    echo "Hello " . $name;
}
greet();
"#,
    );
    assert_eq!(out, "Hello world");
}

#[test]
fn test_default_param_override() {
    let out = compile_and_run(
        r#"<?php
function greet($name = "world") {
    echo "Hello " . $name;
}
greet("PHP");
"#,
    );
    assert_eq!(out, "Hello PHP");
}

#[test]
fn test_default_param_int() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b = 0) {
    return $a + $b;
}
echo add(5);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_default_param_int_override() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b = 0) {
    return $a + $b;
}
echo add(5, 3);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_default_param_multiple() {
    let out = compile_and_run(
        r#"<?php
function multi($a = 1, $b = 2, $c = 3) {
    echo $a + $b + $c;
}
multi();
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_default_param_partial() {
    let out = compile_and_run(
        r#"<?php
function multi($a = 1, $b = 2, $c = 3) {
    echo $a + $b + $c;
}
multi(10);
"#,
    );
    assert_eq!(out, "15");
}

// ===== Feature 2: Null coalescing operator ?? =====

#[test]
fn test_null_coalesce_null_value() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? "default";
"#,
    );
    assert_eq!(out, "default");
}

#[test]
fn test_null_coalesce_non_null() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
echo $x ?? 0;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_null_coalesce_chained() {
    let out = compile_and_run(
        r#"<?php
$x = null;
$y = null;
echo $x ?? $y ?? "found";
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_null_coalesce_literal_null() {
    let out = compile_and_run(
        r#"<?php
echo null ?? "fallback";
"#,
    );
    assert_eq!(out, "fallback");
}

#[test]
fn test_null_coalesce_string() {
    let out = compile_and_run(
        r#"<?php
$name = "Alice";
echo $name ?? "default";
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_null_coalesce_null_to_string() {
    let out = compile_and_run(
        r#"<?php
$name = null;
echo $name ?? "default";
"#,
    );
    assert_eq!(out, "default");
}

#[test]
fn test_null_coalesce_empty_string() {
    let out = compile_and_run(
        r#"<?php
$val = "";
echo ($val ?? "fallback") . "|done";
"#,
    );
    assert_eq!(out, "|done");
}

#[test]
fn test_null_coalesce_int() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
echo $x ?? 0;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_null_coalesce_null_to_int() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? 99;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_null_coalesce_chain() {
    let out = compile_and_run(
        r#"<?php
$a = null;
$b = null;
$c = "found";
echo $a ?? $b ?? $c;
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_null_coalesce_float() {
    let out = compile_and_run(
        r#"<?php
$x = 3.14;
echo $x ?? 0.0;
"#,
    );
    assert_eq!(out, "3.14");
}

#[test]
fn test_null_coalesce_null_to_float() {
    let out = compile_and_run(
        r#"<?php
$x = null;
echo $x ?? 2.718;
"#,
    );
    assert_eq!(out, "2.718");
}

#[test]
fn test_null_coalesce_float_in_calc() {
    let out = compile_and_run(
        r#"<?php
$pi = null;
$val = $pi ?? 3.14159;
echo round($val * 2, 4);
"#,
    );
    assert_eq!(out, "6.2832");
}

#[test]
fn test_null_coalesce_result_survives_nested_function_calls_in_concat() {
    let out = compile_and_run(
        r#"<?php
function fallback_pi($x) {
    return $x ?? 3.14159;
}

echo round(fallback_pi(2), 1) . "|" . round(fallback_pi(null), 4);
"#,
    );
    assert_eq!(out, "2|3.1416");
}

// ===== Feature 3: Bitwise operators =====

#[test]
fn test_bitwise_and() {
    let out = compile_and_run("<?php echo 5 & 3;");
    assert_eq!(out, "1");
}

#[test]
fn test_bitwise_or() {
    let out = compile_and_run("<?php echo 5 | 3;");
    assert_eq!(out, "7");
}

#[test]
fn test_bitwise_xor() {
    let out = compile_and_run("<?php echo 5 ^ 3;");
    assert_eq!(out, "6");
}

#[test]
fn test_bitwise_not() {
    let out = compile_and_run("<?php echo ~0;");
    assert_eq!(out, "-1");
}

#[test]
fn test_shift_left() {
    let out = compile_and_run("<?php echo 1 << 4;");
    assert_eq!(out, "16");
}

#[test]
fn test_shift_right() {
    let out = compile_and_run("<?php echo 16 >> 2;");
    assert_eq!(out, "4");
}

#[test]
fn test_bitwise_combined() {
    let out = compile_and_run("<?php echo (255 & 15) | 48;");
    assert_eq!(out, "63");
}

#[test]
fn test_bitwise_not_positive() {
    let out = compile_and_run("<?php echo ~255;");
    assert_eq!(out, "-256");
}

#[test]
fn test_shift_left_multiply() {
    let out = compile_and_run("<?php echo 3 << 3;");
    assert_eq!(out, "24");
}

#[test]
fn test_shift_right_negative() {
    // Arithmetic shift preserves sign
    let out = compile_and_run("<?php echo -16 >> 2;");
    assert_eq!(out, "-4");
}

// ===== Feature 4: Spaceship operator <=> =====

#[test]
fn test_spaceship_less() {
    let out = compile_and_run("<?php echo 1 <=> 2;");
    assert_eq!(out, "-1");
}

#[test]
fn test_spaceship_equal() {
    let out = compile_and_run("<?php echo 2 <=> 2;");
    assert_eq!(out, "0");
}

#[test]
fn test_spaceship_greater() {
    let out = compile_and_run("<?php echo 3 <=> 2;");
    assert_eq!(out, "1");
}

#[test]
fn test_spaceship_negative() {
    let out = compile_and_run("<?php echo -5 <=> 5;");
    assert_eq!(out, "-1");
}

// ===== Feature 5: Heredoc / Nowdoc strings =====

#[test]
fn test_heredoc_basic() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_multiline() {
    let out = compile_and_run("<?php\necho <<<EOT\nLine 1\nLine 2\nLine 3\nEOT;\n");
    assert_eq!(out, "Line 1\nLine 2\nLine 3");
}

#[test]
fn test_heredoc_escapes() {
    let out = compile_and_run("<?php\necho <<<EOT\nHello\\tWorld\\n\nEOT;\n");
    assert_eq!(out, "Hello\tWorld\n");
}

#[test]
fn test_nowdoc_basic() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello World\nEOT;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_nowdoc_no_escapes() {
    let out = compile_and_run("<?php\necho <<<'EOT'\nHello\\tWorld\nEOT;\n");
    assert_eq!(out, "Hello\\tWorld");
}

#[test]
fn test_heredoc_interpolation() {
    let out =
        compile_and_run("<?php\n$name = \"World\";\n$s = <<<EOT\nHello $name\nEOT;\necho $s;\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_interpolation_multiple_vars() {
    let out = compile_and_run(
        "<?php\n$first = \"Hello\";\n$second = \"World\";\necho <<<EOT\n$first $second\nEOT;\n",
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_heredoc_interpolation_multiline() {
    let out = compile_and_run(
        "<?php\n$name = \"Alice\";\necho <<<EOT\nHello $name\nWelcome $name\nEOT;\n",
    );
    assert_eq!(out, "Hello Alice\nWelcome Alice");
}

#[test]
fn test_nowdoc_no_interpolation() {
    let out = compile_and_run("<?php\n$name = \"World\";\necho <<<'EOT'\nHello $name\nEOT;\n");
    assert_eq!(out, "Hello $name");
}

#[test]
fn test_heredoc_escaped_dollar() {
    let out = compile_and_run("<?php\necho <<<EOT\nPrice is \\$100\nEOT;\n");
    assert_eq!(out, "Price is $100");
}

// --- Constants (const / define) ---

#[test]
fn test_const_int() {
    let out = compile_and_run("<?php\nconst MAX = 100;\necho MAX;\n");
    assert_eq!(out, "100");
}

#[test]
fn test_const_string() {
    let out = compile_and_run("<?php\nconst GREETING = \"hello\";\necho GREETING;\n");
    assert_eq!(out, "hello");
}

#[test]
fn test_const_float() {
    let out = compile_and_run("<?php\nconst PI = 3.14;\necho PI;\n");
    assert_eq!(out, "3.14");
}

#[test]
fn test_const_bool() {
    let out = compile_and_run("<?php\nconst DEBUG = true;\necho DEBUG;\n");
    assert_eq!(out, "1");
}

#[test]
fn test_define_int() {
    let out = compile_and_run("<?php\ndefine(\"MAX_SIZE\", 256);\necho MAX_SIZE;\n");
    assert_eq!(out, "256");
}

#[test]
fn test_define_string() {
    let out = compile_and_run("<?php\ndefine(\"APP_NAME\", \"elephc\");\necho APP_NAME;\n");
    assert_eq!(out, "elephc");
}

#[test]
fn test_const_in_expression() {
    let out = compile_and_run("<?php\nconst X = 10;\nconst Y = 20;\necho X + Y;\n");
    assert_eq!(out, "30");
}

#[test]
fn test_const_in_function() {
    let out =
        compile_and_run("<?php\nconst LIMIT = 42;\nfunction test() { echo LIMIT; }\ntest();\n");
    assert_eq!(out, "42");
}

#[test]
fn test_define_in_function() {
    let out =
        compile_and_run("<?php\ndefine(\"RATE\", 100);\nfunction show() { echo RATE; }\nshow();\n");
    assert_eq!(out, "100");
}

#[test]
fn test_const_concat() {
    let out = compile_and_run("<?php\nconst PREFIX = \"hello\";\necho PREFIX . \" world\";\n");
    assert_eq!(out, "hello world");
}

// --- List unpacking ---

#[test]
fn test_list_unpack_int() {
    let out = compile_and_run(
        "<?php\n[$a, $b, $c] = [10, 20, 30];\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "10 20 30");
}

#[test]
fn test_list_unpack_string() {
    let out = compile_and_run("<?php\n[$x, $y] = [\"hello\", \"world\"];\necho $x . \" \" . $y;\n");
    assert_eq!(out, "hello world");
}

#[test]
fn test_list_unpack_from_variable() {
    let out = compile_and_run(
        "<?php\n$arr = [1, 2, 3];\n[$a, $b, $c] = $arr;\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "1 2 3");
}

#[test]
fn test_list_unpack_two_vars() {
    let out = compile_and_run("<?php\n[$first, $second] = [42, 99];\necho $first + $second;\n");
    assert_eq!(out, "141");
}

// --- call_user_func_array ---

#[test]
fn test_call_user_func_array_basic() {
    let out = compile_and_run("<?php\nfunction add($a, $b) { return $a + $b; }\necho call_user_func_array(\"add\", [3, 4]);\n");
    assert_eq!(out, "7");
}

#[test]
fn test_call_user_func_array_single_arg() {
    let out = compile_and_run("<?php\nfunction double($n) { return $n * 2; }\necho call_user_func_array(\"double\", [21]);\n");
    assert_eq!(out, "42");
}

#[test]
fn test_call_user_func_array_string_return() {
    let out = compile_and_run("<?php\nfunction greet($name) { return \"Hello \" . $name; }\necho call_user_func_array(\"greet\", [\"World\"]);\n");
    assert_eq!(out, "Hello World");
}

// -- v0.8 constants --

#[test]
fn test_php_eol() {
    let out = compile_and_run("<?php echo \"a\" . PHP_EOL . \"b\";");
    assert_eq!(out, "a\nb");
}

#[test]
fn test_php_os() {
    let out = compile_and_run("<?php echo PHP_OS;");
    assert_eq!(out, "Darwin");
}

#[test]
fn test_directory_separator() {
    let out = compile_and_run("<?php echo DIRECTORY_SEPARATOR;");
    assert_eq!(out, "/");
}

// -- v0.8 time / microtime --

#[test]
fn test_time() {
    let out = compile_and_run("<?php $t = time(); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

#[test]
fn test_microtime() {
    let out = compile_and_run("<?php $t = microtime(true); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// -- v0.8 sleep / usleep --

#[test]
fn test_sleep_zero() {
    let out = compile_and_run("<?php sleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

#[test]
fn test_usleep_zero() {
    let out = compile_and_run("<?php usleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

// -- v0.8 getenv --

#[test]
fn test_getenv_home() {
    let out =
        compile_and_run("<?php $home = getenv(\"HOME\"); if (strlen($home) > 0) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

#[test]
fn test_getenv_nonexistent() {
    let out = compile_and_run(
        "<?php $missing = getenv(\"ELEPHC_NONEXISTENT_VAR_XYZ\"); echo strlen($missing);",
    );
    assert_eq!(out, "0");
}

#[test]
fn test_putenv() {
    let out = compile_and_run(
        r#"<?php
putenv("ELEPHC_TEST_VAR=hello");
echo getenv("ELEPHC_TEST_VAR");
"#,
    );
    assert_eq!(out, "hello");
}

// -- v0.8 phpversion / php_uname --

#[test]
fn test_phpversion() {
    let out = compile_and_run("<?php echo phpversion();");
    assert_eq!(out, "0.7.1");
}

#[test]
fn test_php_uname() {
    let out = compile_and_run("<?php $os = php_uname(); if (strlen($os) > 0) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// -- v0.8 exec / shell_exec / system / passthru --

#[test]
fn test_shell_exec() {
    let out = compile_and_run("<?php $out = shell_exec(\"echo hello\"); echo trim($out);");
    assert_eq!(out, "hello");
}

#[test]
fn test_exec() {
    let out = compile_and_run("<?php $out = exec(\"echo test\"); echo trim($out);");
    assert_eq!(out, "test");
}

#[test]
fn test_system() {
    let out = compile_and_run("<?php system(\"echo hi\");");
    assert_eq!(out, "hi\n");
}

#[test]
fn test_passthru() {
    let out = compile_and_run("<?php passthru(\"echo bye\");");
    assert_eq!(out, "bye\n");
}

// --- Global variables ---

#[test]
fn test_global_read() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
}
test();
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_global_write() {
    let out = compile_and_run(
        r#"<?php
$y = 5;
function modify() {
    global $y;
    $y = 99;
}
modify();
echo $y;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_global_read_write() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
function test() {
    global $x;
    echo $x;
    $x = 20;
}
test();
echo $x;
"#,
    );
    assert_eq!(out, "1020");
}

#[test]
fn test_global_multiple_vars() {
    let out = compile_and_run(
        r#"<?php
$a = 1;
$b = 2;
function sum() {
    global $a, $b;
    echo $a + $b;
}
sum();
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_global_increment() {
    let out = compile_and_run(
        r#"<?php
$counter = 0;
function inc() {
    global $counter;
    $counter++;
}
inc();
inc();
inc();
echo $counter;
"#,
    );
    assert_eq!(out, "3");
}

// --- Static variables ---

#[test]
fn test_static_counter() {
    let out = compile_and_run(
        r#"<?php
function counter() {
    static $n = 0;
    $n++;
    echo $n;
}
counter();
counter();
counter();
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_static_preserves_value() {
    let out = compile_and_run(
        r#"<?php
function acc() {
    static $total = 0;
    $total = $total + 10;
    return $total;
}
echo acc();
echo acc();
echo acc();
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_static_separate_functions() {
    let out = compile_and_run(
        r#"<?php
function a() {
    static $x = 0;
    $x++;
    echo $x;
}
function b() {
    static $x = 0;
    $x = $x + 10;
    echo $x;
}
a();
b();
a();
b();
"#,
    );
    assert_eq!(out, "110220");
}

// --- Pass by reference ---

#[test]
fn test_ref_increment() {
    let out = compile_and_run(
        r#"<?php
function increment(&$val) {
    $val++;
}
$x = 5;
increment($x);
echo $x;
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_ref_assign() {
    let out = compile_and_run(
        r#"<?php
function set_value(&$v, $new_val) {
    $v = $new_val;
}
$x = 1;
set_value($x, 42);
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ref_swap() {
    let out = compile_and_run(
        r#"<?php
function swap(&$a, &$b) {
    $tmp = $a;
    $a = $b;
    $b = $tmp;
}
$p = 1;
$q = 2;
swap($p, $q);
echo $p . $q;
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_ref_mixed_params() {
    let out = compile_and_run(
        r#"<?php
function add_to(&$target, $amount) {
    $target = $target + $amount;
}
$x = 10;
add_to($x, 5);
echo $x;
"#,
    );
    assert_eq!(out, "15");
}

// --- Variadic functions ---

#[test]
fn test_variadic_sum() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_variadic_five_args() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3, 4, 5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_variadic_multiple_calls_same_function() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum(1, 2, 3);
echo ":";
echo sum(10, 20, 30, 40, 50);
"#,
    );
    assert_eq!(out, "6:150");
}

#[test]
fn test_variadic_empty() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
echo sum();
"#,
    );
    assert_eq!(out, "0");
}

#[test]
fn test_variadic_with_regular_params() {
    let out = compile_and_run(
        r#"<?php
function greet($greeting, ...$names) {
    foreach ($names as $name) {
        echo $greeting . " " . $name . "\n";
    }
}
greet("Hello", "Alice", "Bob");
"#,
    );
    assert_eq!(out, "Hello Alice\nHello Bob\n");
}

#[test]
fn test_variadic_count() {
    let out = compile_and_run(
        r#"<?php
function num_args(...$args) {
    return count($args);
}
echo num_args(10, 20, 30, 40);
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_variadic_single_arg() {
    let out = compile_and_run(
        r#"<?php
function wrap(...$items) {
    return $items;
}
$arr = wrap(42);
echo $arr[0];
"#,
    );
    assert_eq!(out, "42");
}

// --- Spread operator ---

#[test]
fn test_spread_in_function_call() {
    let out = compile_and_run(
        r#"<?php
function sum(...$nums) {
    $total = 0;
    foreach ($nums as $n) {
        $total += $n;
    }
    return $total;
}
$args = [10, 20, 30];
echo sum(...$args);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_spread_in_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
echo count($c);
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_spread_array_values() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [3, 4];
$c = [...$a, ...$b];
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "1234");
}

#[test]
fn test_spread_mixed_with_elements() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2];
$b = [5, 6];
$c = [...$a, 3, 4, ...$b];
echo count($c);
echo " ";
foreach ($c as $v) {
    echo $v;
}
"#,
    );
    assert_eq!(out, "6 123456");
}

#[test]
fn test_spread_single_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$c = [...$a];
echo count($c);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_variadic_with_regular_and_no_extra() {
    let out = compile_and_run(
        r#"<?php
function prefix($pre, ...$items) {
    echo count($items);
}
prefix("x");
"#,
    );
    assert_eq!(out, "0");
}

// --- Date/time functions ---

#[test]
fn test_date_year() {
    let out = compile_and_run("<?php echo date(\"Y\", 1700000000);");
    assert_eq!(out, "2023");
}

#[test]
fn test_date_full_format() {
    let out = compile_and_run("<?php echo date(\"Y-m-d\", 1700000000);");
    assert_eq!(out, "2023-11-14");
}

#[test]
fn test_date_time_format() {
    let out = compile_and_run("<?php echo date(\"H:i:s\", 1700000000);");
    // The exact output depends on the timezone, but it should have the format HH:MM:SS
    let out_trimmed = out.trim();
    assert_eq!(out_trimmed.len(), 8);
    assert_eq!(&out_trimmed[2..3], ":");
    assert_eq!(&out_trimmed[5..6], ":");
}

#[test]
fn test_date_day_no_padding() {
    let out = compile_and_run("<?php echo date(\"j\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 31);
}

#[test]
fn test_date_am_pm() {
    let out = compile_and_run("<?php echo date(\"A\", 1700000000);");
    assert!(out == "AM" || out == "PM");
}

#[test]
fn test_date_am_pm_lower() {
    let out = compile_and_run("<?php echo date(\"a\", 1700000000);");
    assert!(out == "am" || out == "pm");
}

#[test]
fn test_date_unix_timestamp() {
    let out = compile_and_run("<?php echo date(\"U\", 1700000000);");
    assert_eq!(out, "1700000000");
}

#[test]
fn test_date_short_day() {
    let out = compile_and_run("<?php echo date(\"D\", 1700000000);");
    let valid_days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    assert!(valid_days.contains(&out.as_str()), "Got: {}", out);
}

#[test]
fn test_date_short_month() {
    let out = compile_and_run("<?php echo date(\"M\", 1700000000);");
    assert_eq!(out, "Nov");
}

#[test]
fn test_date_iso_day_of_week() {
    let out = compile_and_run("<?php echo date(\"N\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 7);
}

#[test]
fn test_date_12_hour() {
    let out = compile_and_run("<?php echo date(\"g\", 1700000000);");
    let val: i32 = out.trim().parse().unwrap();
    assert!(val >= 1 && val <= 12);
}

#[test]
fn test_date_literal_text() {
    let out = compile_and_run("<?php echo date(\"Y/m/d\", 1700000000);");
    assert_eq!(out, "2023/11/14");
}

#[test]
fn test_mktime() {
    let out = compile_and_run(
        "<?php
$ts = mktime(0, 0, 0, 1, 1, 2000);
echo date(\"Y-m-d\", $ts);
",
    );
    assert_eq!(out, "2000-01-01");
}

#[test]
fn test_mktime_specific_time() {
    let out = compile_and_run(
        "<?php
$ts = mktime(12, 30, 45, 6, 15, 2024);
echo date(\"H:i:s\", $ts);
",
    );
    assert_eq!(out, "12:30:45");
}

#[test]
fn test_strtotime_date() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("2000-01-01");
echo date("Y-m-d", $ts);
"#,
    );
    assert_eq!(out, "2000-01-01");
}

#[test]
fn test_strtotime_datetime() {
    let out = compile_and_run(
        r#"<?php
$ts = strtotime("2024-06-15 12:30:45");
echo date("Y-m-d H:i:s", $ts);
"#,
    );
    assert_eq!(out, "2024-06-15 12:30:45");
}

#[test]
fn test_strtotime_mktime_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$ts1 = mktime(10, 30, 0, 3, 25, 2024);
$ts2 = strtotime("2024-03-25 10:30:00");
if ($ts1 == $ts2) {
    echo "match";
}
"#,
    );
    assert_eq!(out, "match");
}

#[test]
fn test_date_current_time() {
    // date() with no timestamp should use current time
    let out = compile_and_run(
        "<?php $y = date(\"Y\"); $val = intval($y); if ($val >= 2024) { echo \"ok\"; }",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_date_full_day_name() {
    let out = compile_and_run("<?php echo date(\"l\", 1700000000);");
    let valid_days = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    assert!(valid_days.contains(&out.as_str()), "Got: {}", out);
}

#[test]
fn test_date_full_month_name() {
    let out = compile_and_run("<?php echo date(\"F\", 1700000000);");
    assert_eq!(out, "November");
}

#[test]
fn test_date_epoch_zero_timestamp() {
    // Regression test for GitHub issue #9: date("Y-m-d", 0) should format Unix epoch,
    // not return the current date. Timestamp 0 = 1970-01-01 00:00:00 UTC.
    let out = compile_and_run("<?php echo date(\"Y\", 0);");
    assert_eq!(out, "1970");
}

// --- JSON functions ---

#[test]
fn test_json_encode_int() {
    let out = compile_and_run("<?php echo json_encode(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_json_encode_string() {
    let out = compile_and_run(r#"<?php echo json_encode("hello");"#);
    assert_eq!(out, r#""hello""#);
}

#[test]
fn test_json_encode_string_with_escaping() {
    let out = compile_and_run("<?php echo json_encode(\"hello\\nworld\");");
    assert_eq!(out, r#""hello\nworld""#);
}

#[test]
fn test_json_encode_string_with_quotes() {
    let out = compile_and_run(r#"<?php echo json_encode("say \"hi\"");"#);
    assert_eq!(out, r#""say \"hi\"""#);
}

#[test]
fn test_json_encode_bool_true() {
    let out = compile_and_run("<?php echo json_encode(true);");
    assert_eq!(out, "true");
}

#[test]
fn test_json_encode_bool_false() {
    let out = compile_and_run("<?php echo json_encode(false);");
    assert_eq!(out, "false");
}

#[test]
fn test_json_encode_null() {
    let out = compile_and_run("<?php echo json_encode(null);");
    assert_eq!(out, "null");
}

#[test]
fn test_json_encode_int_array() {
    let out = compile_and_run("<?php echo json_encode([1, 2, 3]);");
    assert_eq!(out, "[1,2,3]");
}

#[test]
fn test_json_encode_string_array() {
    let out = compile_and_run(r#"<?php echo json_encode(["a", "b", "c"]);"#);
    assert_eq!(out, r#"["a","b","c"]"#);
}

#[test]
fn test_json_encode_single_element_array() {
    let out = compile_and_run("<?php $arr = [42]; echo json_encode($arr);");
    assert_eq!(out, "[42]");
}

#[test]
fn test_json_encode_assoc() {
    let out = compile_and_run(r#"<?php echo json_encode(["name" => "Alice", "age" => "30"]);"#);
    assert_eq!(out, r#"{"name":"Alice","age":"30"}"#, "Got: {}", out);
}

#[test]
fn test_json_encode_assoc_mixed_values() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["id" => 7, "name" => "Alice", "ok" => true, "note" => null]);"#,
    );
    assert_eq!(out, r#"{"id":7,"name":"Alice","ok":true,"note":null}"#);
}

#[test]
fn test_json_encode_assoc_nested_nonstring_indexed_arrays() {
    let out = compile_and_run(
        r#"<?php
class Box {}
echo json_encode([
    "floats" => [1.5, 2.25],
    "bools" => [true, false],
    "objects" => [new Box()],
]);
"#,
    );
    assert_eq!(out, r#"{"floats":[1.5,2.25],"bools":[true,false],"objects":[null]}"#);
}

#[test]
fn test_json_encode_float() {
    let out = compile_and_run("<?php echo json_encode(3.14);");
    assert!(out.starts_with("3.14"), "Got: {}", out);
}

#[test]
fn test_json_last_error() {
    let out = compile_and_run("<?php echo json_last_error();");
    assert_eq!(out, "0");
}

#[test]
fn test_json_decode_string() {
    let out = compile_and_run(r#"<?php echo json_decode("\"hello\"");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_json_decode_number() {
    let out = compile_and_run(r#"<?php echo json_decode("42");"#);
    assert_eq!(out, "42");
}

#[test]
fn test_json_decode_escaped() {
    let out = compile_and_run(r#"<?php $s = json_decode("\"hello\\nworld\""); echo strlen($s);"#);
    assert_eq!(out, "11"); // "hello" + newline + "world" = 11 chars
}

// --- Regex functions ---

#[test]
fn test_preg_match_simple() {
    let out = compile_and_run(r#"<?php echo preg_match("/hello/", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_no_match() {
    let out = compile_and_run(r#"<?php echo preg_match("/xyz/", "hello world");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_preg_match_case_insensitive() {
    let out = compile_and_run(r#"<?php echo preg_match("/HELLO/i", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_pattern() {
    let out = compile_and_run(r#"<?php echo preg_match("/[0-9]+/", "abc123def");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_no_digits() {
    let out = compile_and_run(r#"<?php echo preg_match("/[0-9]+/", "abcdef");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_preg_match_all_count() {
    let out = compile_and_run(r#"<?php echo preg_match_all("/[0-9]+/", "a1b2c3");"#);
    assert_eq!(out, "3");
}

#[test]
fn test_preg_match_all_no_matches() {
    let out = compile_and_run(r#"<?php echo preg_match_all("/[0-9]+/", "abcdef");"#);
    assert_eq!(out, "0");
}

#[test]
fn test_preg_replace_simple() {
    let out = compile_and_run(r#"<?php echo preg_replace("/world/", "PHP", "hello world");"#);
    assert_eq!(out, "hello PHP");
}

#[test]
fn test_preg_replace_pattern() {
    let out = compile_and_run(r#"<?php echo preg_replace("/[0-9]+/", "X", "a1b2c3");"#);
    assert_eq!(out, "aXbXcX");
}

#[test]
fn test_preg_replace_no_match() {
    let out = compile_and_run(r#"<?php echo preg_replace("/xyz/", "ABC", "hello world");"#);
    assert_eq!(out, "hello world");
}

#[test]
fn test_preg_split_simple() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/,/", "a,b,c");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1] . "|" . $parts[2];
"#,
    );
    assert_eq!(out, "3|a|b|c");
}

#[test]
fn test_preg_split_whitespace() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/[ ]+/", "hello   world");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1];
"#,
    );
    assert_eq!(out, "2|hello|world");
}

#[test]
fn test_preg_replace_case_insensitive() {
    let out = compile_and_run(r#"<?php echo preg_replace("/WORLD/i", "PHP", "hello World");"#);
    assert_eq!(out, "hello PHP");
}

// -- Issue #25: \0 null byte in strings --
#[test]
fn test_null_byte_in_string() {
    let out = compile_and_run(r#"<?php echo strlen("ab\0cd");"#);
    assert_eq!(out, "5");
}

// -- Issue #26: empty string should be falsy --
#[test]
fn test_not_empty_string_is_true() {
    let out = compile_and_run(r#"<?php echo !!"";"#);
    assert_eq!(out, "");
}

#[test]
fn test_not_nonempty_string_is_false() {
    let out = compile_and_run(r#"<?php echo !!"hello";"#);
    assert_eq!(out, "1");
}

// -- Issue #27: is_numeric() should work for numeric strings --
#[test]
fn test_is_numeric_string_digits() {
    let out = compile_and_run(r#"<?php if (is_numeric("42")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_float() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("3.14")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_negative() {
    let out = compile_and_run(r#"<?php if (is_numeric("-5")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_is_numeric_string_not_numeric() {
    let out =
        compile_and_run(r#"<?php if (is_numeric("abc")) { echo "yes"; } else { echo "no"; }"#);
    assert_eq!(out, "no");
}

// -- Issue #29: function_exists() should recognize builtins --
#[test]
fn test_function_exists_builtin() {
    let out = compile_and_run(r#"<?php echo function_exists("strlen") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_function_exists_builtin_array_push() {
    let out = compile_and_run(r#"<?php echo function_exists("array_push") ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

// --- Issue #12: preg_split with \s shorthand ---

#[test]
fn test_preg_split_backslash_s() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\s+/", "hello  world");
echo $parts[1];
"#,
    );
    assert_eq!(out, "world");
}

#[test]
fn test_preg_split_backslash_d() {
    let out = compile_and_run(
        r#"<?php
$parts = preg_split("/\d+/", "abc123def456ghi");
echo count($parts) . "|" . $parts[0] . "|" . $parts[1] . "|" . $parts[2];
"#,
    );
    assert_eq!(out, "3|abc|def|ghi");
}

#[test]
fn test_preg_match_backslash_s() {
    let out = compile_and_run(r#"<?php echo preg_match("/\s/", "hello world");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_backslash_d() {
    let out = compile_and_run(r#"<?php echo preg_match("/\d+/", "abc123");"#);
    assert_eq!(out, "1");
}

#[test]
fn test_preg_match_backslash_w() {
    let out = compile_and_run(r#"<?php echo preg_match("/^\w+$/", "hello_world");"#);
    assert_eq!(out, "1");
}

// --- Issue #14: hex integer literals ---

#[test]
fn test_hex_literal_0xff() {
    let out = compile_and_run("<?php echo 0xFF;");
    assert_eq!(out, "255");
}

#[test]
fn test_hex_literal_0x1a() {
    let out = compile_and_run("<?php echo 0x1A;");
    assert_eq!(out, "26");
}

#[test]
fn test_hex_literal_0x0() {
    let out = compile_and_run("<?php echo 0x0;");
    assert_eq!(out, "0");
}

#[test]
fn test_hex_literal_uppercase_prefix() {
    let out = compile_and_run("<?php echo 0XFF;");
    assert_eq!(out, "255");
}

#[test]
fn test_hex_literal_arithmetic() {
    let out = compile_and_run("<?php echo 0xFF + 1;");
    assert_eq!(out, "256");
}

// --- Issue #23: modulo by zero ---

#[test]
fn test_modulo_normal() {
    let out = compile_and_run("<?php echo 5 % 1;");
    assert_eq!(out, "0");
}

#[test]
fn test_modulo_by_zero() {
    let out = compile_and_run("<?php echo 5 % 0;");
    assert_eq!(out, "0");
}

#[test]
fn test_modulo_normal_remainder() {
    let out = compile_and_run("<?php echo 7 % 3;");
    assert_eq!(out, "1");
}

// --- Issue #24: negative array index ---

#[test]
fn test_negative_array_index_returns_null() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$v = $a[-1];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_array_out_of_bounds_returns_null() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$v = $a[5];
if (is_null($v)) { echo "null"; } else { echo "not null"; }
"#,
    );
    assert_eq!(out, "null");
}

#[test]
fn test_array_valid_index_still_works() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo $a[0] . "|" . $a[1] . "|" . $a[2];
"#,
    );
    assert_eq!(out, "10|20|30");
}

// -- Issue #20: assoc array missing key should return null, not garbage --

#[test]
fn test_assoc_array_missing_key_returns_null() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1];
echo $m["missing"];
"#,
    );
    assert_eq!(out, "");
}

// -- Issue #28: array_map should handle string return values from callbacks --

#[test]
fn test_array_map_str_callback() {
    let out = compile_and_run(
        r#"<?php
$r = array_map(fn($x) => "v" . $x, [1, 2, 3]);
echo $r[0];
"#,
    );
    assert_eq!(out, "v1");
}

#[test]
fn test_array_map_str_callback_all_elements() {
    let out = compile_and_run(
        r#"<?php
$r = array_map(fn($x) => "item" . $x, [1, 2, 3]);
echo $r[0] . "|" . $r[1] . "|" . $r[2];
"#,
    );
    assert_eq!(out, "item1|item2|item3");
}

// -- Issue #13: empty array literal should be accepted by type checker --

#[test]
fn test_empty_array_literal() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = 1;
echo count($a);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_empty_array_json_encode() {
    let out = compile_and_run(
        r#"<?php
echo json_encode([]);
"#,
    );
    assert_eq!(out, "[]");
}

// -- Issue #16: Spread operator unpacking into named parameters --

#[test]
fn test_spread_into_named_params() {
    let out = compile_and_run(
        r#"<?php
function add($a, $b) { return $a + $b; }
$args = [3, 4];
echo add(...$args);
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_spread_into_named_params_three() {
    let out = compile_and_run(
        r#"<?php
function sum3($a, $b, $c) { return $a + $b + $c; }
$args = [10, 20, 30];
echo sum3(...$args);
"#,
    );
    assert_eq!(out, "60");
}

#[test]
fn test_spread_mixed_with_regular_args() {
    let out = compile_and_run(
        r#"<?php
function add3($a, $b, $c) { return $a + $b + $c; }
$rest = [20, 30];
echo add3(10, ...$rest);
"#,
    );
    assert_eq!(out, "60");
}

// -- Issue #17: Braceless single-statement bodies --

#[test]
fn test_braceless_if() {
    let out = compile_and_run(
        r#"<?php
if (true) echo "yes";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_braceless_if_else() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else echo "small";
"#,
    );
    assert_eq!(out, "small");
}

#[test]
fn test_braceless_for() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) echo $i;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_while() {
    let out = compile_and_run(
        r#"<?php
$i = 0;
while ($i < 3) echo $i++;
"#,
    );
    assert_eq!(out, "012");
}

#[test]
fn test_braceless_foreach() {
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
foreach ($arr as $v) echo $v;
"#,
    );
    assert_eq!(out, "123");
}

#[test]
fn test_braceless_else_if() {
    let out = compile_and_run(
        r#"<?php
$x = 5;
if ($x > 10) echo "big";
else if ($x > 3) echo "medium";
else echo "small";
"#,
    );
    assert_eq!(out, "medium");
}

// --- Bug regression tests ---

#[test]
fn test_closure_default_param() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_closure_default_param_overridden() {
    let out = compile_and_run(
        r#"<?php
$fn = function($x, $y = 10) { return $x + $y; };
echo $fn(5, 20);
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_implode_int_array() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
echo implode(", ", $a);
"#,
    );
    assert_eq!(out, "1, 2, 3");
}

#[test]
fn test_implode_chained_array_builtins() {
    let out = compile_and_run(
        r#"<?php
echo implode(",", array_reverse([3, 1, 2]));
"#,
    );
    assert_eq!(out, "2,1,3");
}

#[test]
fn test_str_replace_in_foreach_assoc_function() {
    let out = compile_and_run(
        r#"<?php
function transform($map, $text) {
    foreach ($map as $key => $value) {
        $text = str_replace($key, $value, $text);
    }
    return $text;
}
$map = ["hello" => "world", "foo" => "bar"];
echo transform($map, "hello foo");
"#,
    );
    assert_eq!(out, "world bar");
}

// --- Bug fix: fmod sign (frintm → frintz) ---

#[test]
fn test_fmod_negative_dividend() {
    let out = compile_and_run("<?php echo fmod(-10, 3);");
    assert_eq!(out, "-1");
}

#[test]
fn test_float_modulo_negative() {
    let out = compile_and_run("<?php echo -10.0 % 3;");
    assert_eq!(out, "-1");
}

// --- Bug fix: string "0" is falsy ---

#[test]
fn test_string_zero_falsy_if() {
    let out = compile_and_run(
        r#"<?php
if ("0") { echo "bad"; } else { echo "good"; }
"#,
    );
    assert_eq!(out, "good");
}

#[test]
fn test_string_zero_falsy_ternary() {
    let out = compile_and_run(r#"<?php echo "0" ? "truthy" : "falsy";"#);
    assert_eq!(out, "falsy");
}

#[test]
fn test_string_zero_falsy_not() {
    let out = compile_and_run(r#"<?php echo !"0" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_string_nonempty_truthy() {
    let out = compile_and_run(r#"<?php echo "hello" ? "yes" : "no";"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_string_empty_falsy() {
    let out = compile_and_run(r#"<?php echo "" ? "yes" : "no";"#);
    assert_eq!(out, "no");
}

// --- Bug fix: compound assignment in for-loop update ---

#[test]
fn test_for_compound_subtract() {
    let out = compile_and_run(
        r#"<?php
for ($i = 10; $i > 0; $i -= 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "10 7 4 1 ");
}

#[test]
fn test_for_compound_add() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 10; $i += 3) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "0 3 6 9 ");
}

#[test]
fn test_for_compound_multiply() {
    let out = compile_and_run(
        r#"<?php
for ($i = 1; $i < 100; $i *= 2) { echo $i . " "; }
"#,
    );
    assert_eq!(out, "1 2 4 8 16 32 64 ");
}

// --- Bug fix: array push with concat expression ---

#[test]
fn test_array_push_string_to_empty() {
    let out = compile_and_run(
        r#"<?php
$a = [];
$a[] = "hello";
echo $a[0];
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_array_push_concat_expr() {
    let out = compile_and_run(
        r#"<?php
$tokens = [];
$word = "42";
$tokens[] = "NUM:" . $word;
echo $tokens[0];
"#,
    );
    assert_eq!(out, "NUM:42");
}

#[test]
fn test_many_local_vars() {
    // Issue #22: stur/ldur offset overflow with >32 local variables
    let mut php = String::from("<?php\nfunction f() {\n");
    for i in 0..50 {
        php.push_str(&format!("$v{} = {};\n", i, i));
    }
    // Sum some vars to ensure they're stored/loaded correctly
    php.push_str("echo $v0 + $v49;\n");
    php.push_str("}\nf();\n");
    let out = compile_and_run(&php);
    assert_eq!(out, "49");
}

#[test]
fn test_ref_array_assign() {
    // Issue #32: pass-by-reference array mutation via index assignment
    let out = compile_and_run(
        r#"<?php
function swap(&$a) {
    $t = $a[0];
    $a[0] = $a[1];
    $a[1] = $t;
}
$x = [1, 2];
swap($x);
echo $x[0];
echo $x[1];
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_ref_array_push() {
    // Issue #32: pass-by-reference array mutation via push
    let out = compile_and_run(
        r#"<?php
function append(&$arr, $val) {
    $arr[] = $val;
}
$x = [10, 20];
append($x, 30);
echo count($x);
echo $x[2];
"#,
    );
    assert_eq!(out, "330");
}

#[test]
fn test_array_column_string_implode() {
    // Issue #33: array_column on arrays of assoc arrays with string values + implode
    let out = compile_and_run(
        r#"<?php
$s = [["n" => "Alice"], ["n" => "Bob"]];
$names = array_column($s, "n");
echo implode(",", $names);
"#,
    );
    assert_eq!(out, "Alice,Bob");
}

#[test]
fn test_round_precision_1() {
    let out = compile_and_run("<?php echo round(1.55, 1);");
    assert_eq!(out, "1.6");
}

#[test]
fn test_round_precision_2() {
    let out = compile_and_run("<?php echo round(3.14159, 2);");
    assert_eq!(out, "3.14");
}

#[test]
fn test_rtrim_mask() {
    let out = compile_and_run(r#"<?php echo rtrim("hello...", ".");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_ltrim_mask() {
    let out = compile_and_run(r#"<?php echo ltrim("000123", "0");"#);
    assert_eq!(out, "123");
}

#[test]
fn test_trim_mask() {
    let out = compile_and_run(r#"<?php echo trim("**hello**", "*");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_min_three_args() {
    let out = compile_and_run("<?php echo min(3, 1, 2);");
    assert_eq!(out, "1");
}

#[test]
fn test_max_three_args() {
    let out = compile_and_run("<?php echo max(1, 3, 2);");
    assert_eq!(out, "3");
}

#[test]
fn test_min_five_args() {
    let out = compile_and_run("<?php echo min(5, 4, 3, 2, 1);");
    assert_eq!(out, "1");
}

#[test]
fn test_closure_use_int() {
    let out = compile_and_run(
        r#"<?php
$factor = 3;
$mul = function($x) use ($factor) { return $x * $factor; };
echo $mul(5);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_closure_use_string() {
    let out = compile_and_run(
        r#"<?php
$greeting = "Hello";
$greet = function($name) use ($greeting) { return $greeting . " " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_closure_use_multiple() {
    let out = compile_and_run(
        r#"<?php
$a = 10;
$b = 20;
$sum = function() use ($a, $b) { return $a + $b; };
echo $sum();
"#,
    );
    assert_eq!(out, "30");
}

#[test]
fn test_closure_use_no_params() {
    let out = compile_and_run(
        r#"<?php
$name = "World";
$greet = function() use ($name) {
    echo "Hello " . $name;
};
$greet();
"#,
    );
    assert_eq!(out, "Hello World");
}

// === Memory management regression tests ===

#[test]
fn test_concat_loop_1000() {
    // Regression test for issue #21: concat buffer overflow after ~362 iterations
    let out = compile_and_run(
        r#"<?php
$s = "";
for ($i = 0; $i < 1000; $i++) {
    $s .= "x";
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "1000");
}

#[test]
fn test_string_function_in_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 500; $i++) {
    $x = strtolower("HELLO WORLD");
}
echo $x;
"#,
    );
    assert_eq!(out, "hello world");
}

#[test]
fn test_hash_table_computed_keys_loop() {
    // Tests that hash keys survive concat_buf reset (persisted to heap)
    let out = compile_and_run(
        r#"<?php
$h = ["init" => 0];
for ($i = 0; $i < 10; $i++) {
    $h["k" . $i] = $i;
}
echo $h["k9"];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_string_reassignment_loop() {
    // Tests that old string values are freed on reassignment (free-list reuse)
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2000; $i++) {
    $s = str_repeat("a", 100);
}
echo strlen($s);
"#,
    );
    assert_eq!(out, "100");
}

#[test]
fn test_string_variables_survive_statements() {
    // Tests that string persist works: values survive across statement boundaries
    let out = compile_and_run(
        r#"<?php
$a = "foo" . "bar";
$b = "baz" . "qux";
echo $a . $b;
"#,
    );
    assert_eq!(out, "foobarbazqux");
}

#[test]
fn test_unset_frees_string() {
    let out = compile_and_run(
        r#"<?php
$x = "hello" . " world";
echo strlen($x);
unset($x);
echo is_null($x) ? "1" : "0";
"#,
    );
    assert_eq!(out, "111");
}

#[test]
fn test_multiple_string_vars_independent() {
    // Ensure multiple string variables don't interfere after concat_buf reset
    let out = compile_and_run(
        r#"<?php
$a = "hello";
$b = "world";
$c = $a . " " . $b;
$d = strtoupper($a);
echo $c . "|" . $d;
"#,
    );
    assert_eq!(out, "hello world|HELLO");
}

#[test]
fn test_str_replace_in_loop() {
    let out = compile_and_run(
        r#"<?php
$result = "";
for ($i = 0; $i < 100; $i++) {
    $result = str_replace("x", "y", "xox");
}
echo $result;
"#,
    );
    assert_eq!(out, "yoy");
}

#[test]
fn test_array_dynamic_growth_int() {
    // Array grows beyond initial capacity via reallocation
    let out = compile_and_run(
        r#"<?php
$arr = [1, 2, 3];
for ($i = 4; $i <= 100; $i++) {
    $arr[] = $i;
}
echo count($arr) . "|" . $arr[0] . "|" . $arr[99];
"#,
    );
    assert_eq!(out, "100|1|100");
}

#[test]
fn test_array_dynamic_growth_str() {
    // String array grows beyond initial capacity
    let out = compile_and_run(
        r#"<?php
$arr = ["first"];
for ($i = 0; $i < 50; $i++) {
    $arr[] = "item" . $i;
}
echo count($arr) . "|" . $arr[0] . "|" . $arr[50];
"#,
    );
    assert_eq!(out, "51|first|item49");
}

#[test]
fn test_array_push_function_growth() {
    // array_push() triggers growth
    let out = compile_and_run(
        r#"<?php
$arr = [10];
for ($i = 0; $i < 20; $i++) {
    array_push($arr, $i * 10);
}
echo count($arr) . "|" . $arr[20];
"#,
    );
    assert_eq!(out, "21|190");
}

#[test]
fn test_array_reassign_after_function_growth() {
    let out = compile_and_run(
        r#"<?php
function grow($arr) {
    for ($i = 0; $i < 32; $i++) {
        array_push($arr, $i);
    }
    return $arr;
}

$arr = [100];
for ($j = 0; $j < 20; $j++) {
    $arr = grow($arr);
}
echo count($arr) > 100 ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_array_push_float() {
    let out = compile_and_run(
        r#"<?php
$arr = [1.1];
array_push($arr, 2.2);
echo count($arr) . "|" . $arr[1];
"#,
    );
    assert_eq!(out, "2|2.2");
}

#[test]
fn test_array_push_bool() {
    let out = compile_and_run(
        r#"<?php
$arr = [true];
array_push($arr, false);
echo count($arr);
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_array_push_object() {
    let out = compile_and_run(
        r#"<?php
class Item { public $name;
    public function __construct($n) { $this->name = $n; }
}
$items = [new Item("a")];
array_push($items, new Item("b"));
echo count($items) . "|" . $items[1]->name;
"#,
    );
    assert_eq!(out, "2|b");
}

#[test]
fn test_array_push_syntax_float() {
    // $arr[] = float syntax
    let out = compile_and_run(
        r#"<?php
$arr = [1.0];
$arr[] = 2.5;
$arr[] = 3.7;
echo count($arr) . "|" . $arr[2];
"#,
    );
    assert_eq!(out, "3|3.7");
}

// =============================================================================
// Class edge cases
// =============================================================================

#[test]
fn test_class_empty() {
    // Empty class with no properties or methods
    let out = compile_and_run(
        r#"<?php
class Blank {}
$e = new Blank();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_object_aliasing() {
    // Assigning object to another variable shares the same instance
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }
$a = new Box();
$a->val = 42;
$b = $a;
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_gc_array_alias_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
$b = $a;
unset($a);
echo $b[0];
echo $b[1];
echo $b[2];
"#,
    );
    assert_eq!(out, "102030");
}

#[test]
fn test_gc_returned_array_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
function share($arr) {
    return $arr;
}

$a = [7, 8];
$b = share($a);
unset($a);
echo $b[0];
echo $b[1];
"#,
    );
    assert_eq!(out, "78");
}

#[test]
fn test_gc_returned_object_alias_survives_caller_unset() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }

function share($box) {
    return $box;
}

$a = new Box();
$a->val = 41;
$b = share($a);
unset($a);
echo $b->val;
"#,
    );
    assert_eq!(out, "41");
}

#[test]
fn test_gc_array_push_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$outer = [];
$outer[] = $inner;
unset($inner);
echo $outer[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_gc_indexed_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3, 4];
$outer = [$inner];
unset($inner);
echo $outer[0][1];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$outer = [[1], [2]];
$outer[1] = $inner;
unset($inner);
echo $outer[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_property_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
class Holder { public $value; }

$inner = [7];
$h = new Holder();
$h->value = $inner;
unset($inner);
$saved = $h->value;
echo $saved[0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_gc_static_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function hold_once() {
    static $saved = [];
    $tmp = [5];
    $saved = $tmp;
    unset($tmp);
    echo $saved[0];
}

hold_once();
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_gc_spread_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [8];
$src = [$inner];
$dst = [...$src];
unset($src);
unset($inner);
echo $dst[0][0];
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_gc_array_merge_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner];
$right = [[7]];
$merged = array_merge($left, $right);
unset($left);
unset($inner);
echo $merged[0][0] . "|" . $merged[1][0];
"#,
    );
    assert_eq!(out, "6|7");
}

#[test]
fn test_gc_array_chunk_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$rows = [$inner, [9]];
$chunks = array_chunk($rows, 1);
unset($rows);
unset($inner);
echo $chunks[0][0][0] . "|" . $chunks[1][0][0];
"#,
    );
    assert_eq!(out, "5|9");
}

#[test]
fn test_gc_array_slice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [2];
$src = [[1], $inner, [3]];
$slice = array_slice($src, 1, 1);
unset($src);
unset($inner);
echo $slice[0][0];
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_gc_array_reverse_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4];
$src = [[1], $inner, [7]];
$rev = array_reverse($src);
unset($src);
unset($inner);
echo $rev[1][0];
"#,
    );
    assert_eq!(out, "4");
}

#[test]
fn test_gc_array_pad_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [5];
$src = [[1]];
$padded = array_pad($src, 3, $inner);
unset($src);
unset($inner);
echo $padded[1][0] . "|" . $padded[2][0];
"#,
    );
    assert_eq!(out, "5|5");
}

#[test]
fn test_gc_array_unique_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [3];
$src = [$inner, $inner, [4]];
$uniq = array_unique($src);
unset($src);
unset($inner);
echo count($uniq) . "|" . $uniq[0][0] . "|" . $uniq[1][0];
"#,
    );
    assert_eq!(out, "2|3|4");
}

#[test]
fn test_gc_array_splice_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7];
$src = [[1], $inner, [9]];
$removed = array_splice($src, 1, 1);
unset($src);
unset($inner);
echo $removed[0][0];
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_gc_array_diff_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [6];
$left = [$inner, [8]];
$right = [[8]];
$diff = array_diff($left, $right);
unset($left);
unset($inner);
echo $diff[0][0];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_gc_array_intersect_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [9];
$left = [[1], $inner];
$right = [$inner];
$both = array_intersect($left, $right);
unset($left);
unset($right);
unset($inner);
echo $both[0][0];
"#,
    );
    assert_eq!(out, "9");
}

#[test]
fn test_gc_array_filter_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
function keep_pair($x) { return count($x) == 2; }
$inner = [10, 11];
$rows = [[1], $inner, [2, 3]];
$filtered = array_filter($rows, "keep_pair");
unset($rows);
unset($inner);
echo $filtered[0][1] . "|" . $filtered[1][0];
"#,
    );
    assert_eq!(out, "11|2");
}

#[test]
fn test_gc_array_fill_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [12];
$filled = array_fill(0, 2, $inner);
unset($inner);
echo $filled[0][0] . "|" . $filled[1][0];
"#,
    );
    assert_eq!(out, "12|12");
}

#[test]
fn test_gc_array_combine_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [13];
$keys = ["keep"];
$vals = [$inner];
$map = array_combine($keys, $vals);
unset($vals);
unset($inner);
$saved = $map["keep"];
echo $saved[0];
"#,
    );
    assert_eq!(out, "13");
}

#[test]
fn test_gc_array_fill_keys_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [14];
$keys = ["a", "b"];
$map = array_fill_keys($keys, $inner);
unset($inner);
$first = $map["a"];
$second = $map["b"];
echo $first[0] . "|" . $second[0];
"#,
    );
    assert_eq!(out, "14|14");
}

#[test]
fn test_class_constructor_calls_method() {
    // Constructor calling another method on the same object
    let out = compile_and_run(
        r#"<?php
class Init { public $ready = 0;
    public function __construct() { $this->setup(); }
    public function setup() { $this->ready = 1; }
}
$i = new Init();
echo $i->ready;
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_class_multiple_classes_composing() {
    // Two classes where one holds an instance of the other
    let out = compile_and_run(
        r#"<?php
class Address { public $city;
    public function __construct($c) { $this->city = $c; }
}
class Person { public $name; public $address;
    public function __construct($n, $addr) { $this->name = $n; $this->address = $addr; }
    public function info() { return $this->name . " from " . $this->address->city; }
}
$addr = new Address("Rome");
$p = new Person("Marco", $addr);
echo $p->info();
"#,
    );
    assert_eq!(out, "Marco from Rome");
}

#[test]
fn test_class_empty_string_property() {
    // Empty string property and strlen on it
    let out = compile_and_run(
        r#"<?php
class Tag { public $label = "";
    public function __construct($l) { $this->label = $l; }
}
$t = new Tag("");
echo strlen($t->label) . "|" . $t->label . "|done";
"#,
    );
    assert_eq!(out, "0||done");
}

#[test]
fn test_class_long_string_property() {
    // String property holding a long (1000 char) string
    let out = compile_and_run(
        r#"<?php
class Buffer { public $data;
    public function __construct($d) { $this->data = $d; }
}
$b = new Buffer(str_repeat("x", 1000));
echo strlen($b->data);
"#,
    );
    assert_eq!(out, "1000");
}

#[test]
fn test_class_string_concat_in_method() {
    // Method concatenating multiple string properties
    let out = compile_and_run(
        r#"<?php
class Row { public $a; public $b; public $c;
    public function __construct($a, $b, $c) { $this->a = $a; $this->b = $b; $this->c = $c; }
    public function csv() { return $this->a . "," . $this->b . "," . $this->c; }
}
$r = new Row("x", "y", "z");
echo $r->csv();
"#,
    );
    assert_eq!(out, "x,y,z");
}

#[test]
fn test_class_bool_property() {
    // Boolean property used in ternary
    let out = compile_and_run(
        r#"<?php
class Flag { public $on;
    public function __construct($v) { $this->on = $v; }
}
$f = new Flag(true);
echo $f->on ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_class_array_property() {
    // Array property with count()
    let out = compile_and_run(
        r#"<?php
class Stack { public $items;
    public function __construct() { $this->items = [1, 2, 3]; }
    public function size() { return count($this->items); }
}
$s = new Stack();
echo $s->size();
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_class_1000_objects_in_loop() {
    // Stress test: create 1000 objects in a loop
    let out = compile_and_run(
        r#"<?php
class Obj { public $id;
    public function __construct($id) { $this->id = $id; }
}
$last = new Obj(0);
for ($i = 1; $i < 1000; $i++) {
    $last = new Obj($i);
}
echo $last->id;
"#,
    );
    assert_eq!(out, "999");
}

#[test]
fn test_class_many_properties() {
    // Object with 10 properties and a method summing them
    let out = compile_and_run(
        r#"<?php
class Big { public $a; public $b; public $c; public $d; public $e;
    public $f; public $g; public $h; public $i; public $j;
    public function __construct() {
        $this->a = 1; $this->b = 2; $this->c = 3; $this->d = 4; $this->e = 5;
        $this->f = 6; $this->g = 7; $this->h = 8; $this->i = 9; $this->j = 10;
    }
    public function sum() {
        return $this->a + $this->b + $this->c + $this->d + $this->e +
               $this->f + $this->g + $this->h + $this->i + $this->j;
    }
}
$b = new Big();
echo $b->sum();
"#,
    );
    assert_eq!(out, "55");
}

// =============================================================================
// Non-class regression edge cases
// =============================================================================

#[test]
fn test_deeply_nested_string_function_calls() {
    // Deeply nested function calls building nested HTML tags
    let out = compile_and_run(
        r#"<?php
function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
echo wrap(wrap(wrap("hello", "b"), "i"), "p");
"#,
    );
    assert_eq!(out, "<p><i><b>hello</b></i></p>");
}

#[test]
fn test_recursive_string_building() {
    // Recursive function that builds a string via concatenation
    let out = compile_and_run(
        r#"<?php
function repeat_str($s, $n) {
    if ($n <= 0) { return ""; }
    return $s . repeat_str($s, $n - 1);
}
echo repeat_str("ab", 5);
"#,
    );
    assert_eq!(out, "ababababab");
}

#[test]
fn test_closure_capturing_object() {
    // Closure capturing an object via use()
    let out = compile_and_run(
        r#"<?php
class Counter { public $n = 0; public function inc() { $this->n = $this->n + 1; } }
$c = new Counter();
$c->inc();
$c->inc();
$fn = function() use ($c) { return $c; };
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_float_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Circle {
    public $radius;
    public function __construct($r) { $this->radius = $r; }
    public function area() { return 3.14159 * $this->radius * $this->radius; }
}
$c = new Circle(5.0);
echo $c->area();
"#,
    );
    assert_eq!(out, "78.53975");
}

#[test]
fn test_class_method_returns_float_property() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public $x;
    public function __construct($v) { $this->x = $v; }
    public function getX() { return $this->x; }
}
$f = new Foo(3.14);
echo $f->getX();
"#,
    );
    assert_eq!(out, "3.14");
}

#[test]
fn test_class_chained_property_access() {
    let out = compile_and_run(
        r#"<?php
class Node {
    public $value;
    public $next;
    public function __construct($v) { $this->value = $v; }
}
$a = new Node(1);
$b = new Node(2);
$a->next = $b;
echo $a->next->value;
"#,
    );
    assert_eq!(out, "2");
}

#[test]
fn test_class_array_of_objects_property_access() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;
    public $price;
    public function __construct($n, $p) { $this->name = $n; $this->price = $p; }
}
$items = [];
$items[] = new Item("Apple", 1);
$items[] = new Item("Banana", 2);
$total = 0;
for ($i = 0; $i < count($items); $i++) {
    $total = $total + $items[$i]->price;
}
echo $total;
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_class_static_method_string_param() {
    let out = compile_and_run(
        r#"<?php
class Utils {
    public static function greet($name) { return "Hello " . $name; }
}
echo Utils::greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_class_method_returns_this() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    public $parts = "";
    public function add($s) { $this->parts = $this->parts . $s; return $this; }
}
$b = new Builder();
$b->add("hello");
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_private_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private $value;
    public function __construct($value) { $this->value = $value; }
    public function reveal() { return $this->value; }
}
$s = new Secret("ok");
echo $s->reveal();
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class User {
    public readonly $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$u = new User(7);
echo $u->id();
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_class_static_and_instance() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $n;
    public function __construct($n) { $this->n = $n; }
    public function next() { return $this->n + 1; }
    public static function make($n) { return new Counter($n); }
}
$c = Counter::make(4);
echo $c->next();
"#,
    );
    assert_eq!(out, "5");
}

// === Nested array access tests ===

#[test]
fn test_nested_indexed_assoc_direct() {
    let out = compile_and_run(
        r#"<?php
$data = [["name" => "Alice"]];
echo $data[0]["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_nested_assoc_indexed() {
    let out = compile_and_run(
        r#"<?php
$map = ["items" => [10, 20, 30]];
$items = $map["items"];
echo $items[1];
"#,
    );
    assert_eq!(out, "20");
}

#[test]
fn test_nested_3_level_chained() {
    let out = compile_and_run(
        r#"<?php
$data = [["tags" => ["php", "rust", "asm"]]];
echo $data[0]["tags"][1];
"#,
    );
    assert_eq!(out, "rust");
}

#[test]
fn test_nested_int_assoc_in_indexed() {
    let out = compile_and_run(
        r#"<?php
$scores = [["math" => 90, "eng" => 85]];
$s = $scores[0];
echo $s["math"] . "|" . $s["eng"];
"#,
    );
    assert_eq!(out, "90|85");
}

#[test]
fn test_nested_string_assoc_loop() {
    let out = compile_and_run(
        r#"<?php
$contacts = [
    ["name" => "Alice", "email" => "alice@test"],
    ["name" => "Bob", "email" => "bob@test"]
];
for ($i = 0; $i < 2; $i++) {
    $c = $contacts[$i];
    echo $c["name"] . "|" . $c["email"] . "\n";
}
"#,
    );
    assert_eq!(out, "Alice|alice@test\nBob|bob@test\n");
}

#[test]
fn test_nested_assoc_of_indexed() {
    let out = compile_and_run(
        r#"<?php
$groups = ["fruits" => ["apple", "banana"], "vegs" => ["carrot", "pea"]];
$f = $groups["fruits"];
echo $f[0] . "|" . $f[1];
"#,
    );
    assert_eq!(out, "apple|banana");
}

#[test]
fn test_nested_dynamic_building() {
    let out = compile_and_run(
        r#"<?php
function make_user($name, $email) {
    return ["name" => $name, "email" => $email];
}
$users = [];
$users[] = make_user("Alice", "a@t");
$users[] = make_user("Bob", "b@t");
for ($i = 0; $i < count($users); $i++) {
    $u = $users[$i];
    echo $u["name"] . "|" . $u["email"] . "\n";
}
"#,
    );
    assert_eq!(out, "Alice|a@t\nBob|b@t\n");
}

#[test]
fn test_nested_explode_to_assoc() {
    let out = compile_and_run(
        r#"<?php
function parse_row($line) {
    $parts = explode("|", $line);
    return ["name" => $parts[0], "email" => $parts[1]];
}
$r = parse_row("Alice|alice@test");
echo $r["name"] . " <" . $r["email"] . ">";
"#,
    );
    assert_eq!(out, "Alice <alice@test>");
}

#[test]
fn test_nested_foreach_of_assoc() {
    let out = compile_and_run(
        r#"<?php
$people = [["name" => "Alice"], ["name" => "Bob"]];
foreach ($people as $p) {
    echo $p["name"] . " ";
}
"#,
    );
    assert_eq!(out, "Alice Bob ");
}

#[test]
fn test_nested_objects_in_assoc() {
    let out = compile_and_run(
        r#"<?php
class Item { public $name;
    public function __construct($n) { $this->name = $n; }
}
$data = ["items" => [new Item("Sword"), new Item("Shield")]];
$items = $data["items"];
$first = $items[0];
echo $first->name;
"#,
    );
    assert_eq!(out, "Sword");
}

#[test]
fn test_switch_return_string() {
    let out = compile_and_run(
        r#"<?php
function classify($n) {
    switch ($n % 3) {
        case 0: return "fizz";
        case 1: return "buzz";
        default: return "none";
    }
}
$r = classify(0);
echo $r . " ";
$r = classify(1);
echo $r . " ";
$r = classify(2);
echo $r;
"#,
    );
    assert_eq!(out, "fizz buzz none");
}

#[test]
fn test_switch_return_int() {
    let out = compile_and_run(
        r#"<?php
function score($grade) {
    switch ($grade) {
        case 1: return 100;
        case 2: return 80;
        case 3: return 60;
        default: return 0;
    }
}
echo score(1) . "|" . score(2) . "|" . score(3) . "|" . score(9);
"#,
    );
    assert_eq!(out, "100|80|60|0");
}

// === GC scope cleanup tests ===

#[test]
fn test_gc_scope_cleanup_basic() {
    // Function locals freed on return (no leak in loop)
    let out = compile_and_run(
        r#"<?php
function process() {
    $arr = [1, 2, 3];
    $map = ["a" => "b"];
    return 42;
}
for ($i = 0; $i < 1000; $i++) { process(); }
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_gc_return_array_survives() {
    // Returned array must not be freed by epilogue decref
    let out = compile_and_run(
        r#"<?php
function make() {
    $arr = [10, 20, 30];
    return $arr;
}
$result = make();
echo $result[0] . "|" . $result[1] . "|" . $result[2];
"#,
    );
    assert_eq!(out, "10|20|30");
}

#[test]
fn test_gc_return_array_loop() {
    // Return array in tight loop must not leak
    let out = compile_and_run(
        r#"<?php
function make() { return [1, 2, 3]; }
for ($i = 0; $i < 100000; $i++) { $x = make(); }
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_gc_return_assoc_array() {
    let out = compile_and_run(
        r#"<?php
function config() { return ["host" => "localhost", "port" => "3306"]; }
$c = config();
echo $c["host"];
"#,
    );
    assert_eq!(out, "localhost");
}

#[test]
fn test_gc_assoc_array_literal_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [7, 8, 9];
$map = ["nums" => $inner];
unset($inner);
$saved = $map["nums"];
echo $saved[1];
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_gc_assoc_array_assign_borrowed_array_survives_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [4, 5, 6];
$map = ["nums" => [1]];
$map["nums"] = $inner;
unset($inner);
$saved = $map["nums"];
echo $saved[2];
"#,
    );
    assert_eq!(out, "6");
}

#[test]
fn test_gc_return_object() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val;
    public function __construct($v) { $this->val = $v; }
}
function make_box($n) { return new Box($n); }
$b = make_box(42);
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_gc_explode_in_function_loop() {
    // Classic leak case: explode in function called 100K times
    let out = compile_and_run(
        r#"<?php
function parse($data) {
    $parts = explode(",", $data);
    return $parts[0];
}
for ($i = 0; $i < 1000; $i++) { $r = parse("a,b,c"); }
echo $r;
"#,
    );
    assert_eq!(out, "a");
}

#[test]
fn test_gc_multiple_locals_one_returned() {
    // Multiple array locals, only one returned — others must be freed
    let out = compile_and_run(
        r#"<?php
function work() {
    $a = [1, 2];
    $b = [3, 4];
    $c = [5, 6];
    return $b;
}
$r = work();
echo $r[0] . "|" . $r[1];
"#,
    );
    assert_eq!(out, "3|4");
}

#[test]
fn test_gc_array_reassign_in_loop() {
    // Array reassignment decrefs old value (100K iterations)
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 1000; $i++) {
    $parts = explode(",", "a,b,c");
}
echo "survived";
"#,
    );
    assert_eq!(out, "survived");
}

#[test]
fn test_gc_nested_function_arrays() {
    // Nested function calls all creating arrays
    let out = compile_and_run(
        r#"<?php
function inner() { return [1, 2, 3]; }
function outer() {
    $tmp = [4, 5, 6];
    $result = inner();
    return $result;
}
for ($i = 0; $i < 50000; $i++) { $x = outer(); }
echo $x[0];
"#,
    );
    assert_eq!(out, "1");
}

// === Regression tests from v0.9-v0.11 bug patterns ===

// Pattern: infer_local_type misses return types for builtins/assoc access
#[test]
fn test_regression_assoc_value_in_function() {
    // AssocArray element stored in local → must allocate 16 bytes for Str
    let out = compile_and_run(
        r#"<?php
function show($todo) {
    $status = $todo["done"] === "1" ? "[x]" : "[ ]";
    $pri = $todo["priority"];
    echo $status . " " . $todo["title"] . " " . $pri;
}
$t = ["title" => "Buy milk", "done" => "0", "priority" => "high", "created" => "now"];
show($t);
"#,
    );
    assert_eq!(out, "[ ] Buy milk high");
}

// Pattern: function receives assoc, iterates, accesses string values
#[test]
fn test_regression_iterate_assoc_in_function() {
    let out = compile_and_run(
        r#"<?php
function format($items) {
    $result = "";
    for ($i = 0; $i < count($items); $i++) {
        $item = $items[$i];
        $result .= $item["name"] . ":" . $item["value"] . "\n";
    }
    return $result;
}
$data = [["name" => "a", "value" => "1"], ["name" => "b", "value" => "2"]];
echo format($data);
"#,
    );
    assert_eq!(out, "a:1\nb:2\n");
}

// Pattern: $arr = func($arr) where func pushes to the array
#[test]
fn test_regression_arr_equals_func_arr() {
    let out = compile_and_run(
        r#"<?php
function add($arr, $val) {
    $arr[] = $val;
    return $arr;
}
$nums = [1];
$nums = add($nums, 2);
$nums = add($nums, 3);
echo count($nums) . "|" . $nums[0] . "|" . $nums[2];
"#,
    );
    assert_eq!(out, "3|1|3");
}

// Pattern: function creates assoc array from parameters, caller iterates
#[test]
fn test_regression_make_assoc_then_iterate() {
    let out = compile_and_run(
        r#"<?php
function make($name, $val) { return ["name" => $name, "val" => $val]; }
$items = [];
$items[] = make("x", "1");
$items[] = make("y", "2");
$items[] = make("z", "3");
for ($i = 0; $i < count($items); $i++) {
    $it = $items[$i];
    echo $it["name"] . "=" . $it["val"] . " ";
}
"#,
    );
    assert_eq!(out, "x=1 y=2 z=3 ");
}

// Pattern: save function iterates assoc array with 5-field concat chain
#[test]
fn test_regression_save_concat_chain() {
    let out = compile_and_run(
        r#"<?php
function save($items) {
    $content = "";
    for ($i = 0; $i < count($items); $i++) {
        $c = $items[$i];
        $content .= $c["a"] . "|" . $c["b"] . "|" . $c["c"] . "\n";
    }
    return $content;
}
$data = [["a" => "x", "b" => "y", "c" => "z"]];
echo save($data);
"#,
    );
    assert_eq!(out, "x|y|z\n");
}

// Pattern: pass object to function, access string property
#[test]
fn test_regression_object_string_property_in_function() {
    let out = compile_and_run(
        r#"<?php
class Dog {
    public $name;
    public $breed;
    public function __construct($n, $b) { $this->name = $n; $this->breed = $b; }
}
function describe($dog) {
    return $dog->name . " (" . $dog->breed . ")";
}
$d = new Dog("Rex", "Labrador");
echo describe($d);
"#,
    );
    assert_eq!(out, "Rex (Labrador)");
}

// Pattern: objects in array, iterated with method calls
#[test]
fn test_regression_objects_in_array_with_methods() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;
    public $price;
    public function __construct($n, $p) { $this->name = $n; $this->price = $p; }
    public function format() { return $this->name . ": $" . $this->price; }
}
$items = [new Item("Apple", 1), new Item("Banana", 2)];
for ($i = 0; $i < count($items); $i++) {
    echo $items[$i]->format() . "\n";
}
"#,
    );
    assert_eq!(out, "Apple: $1\nBanana: $2\n");
}

// Pattern: switch+return inside function called multiple times
#[test]
fn test_regression_switch_return_in_loop() {
    let out = compile_and_run(
        r#"<?php
function label($n) {
    switch ($n % 3) {
        case 0: return "A";
        case 1: return "B";
        default: return "C";
    }
}
$r = "";
for ($i = 0; $i < 6; $i++) {
    $r .= label($i);
}
echo $r;
"#,
    );
    assert_eq!(out, "ABCABC");
}

// Pattern: str_replace + strtolower inside a function
#[test]
fn test_regression_string_ops_in_function() {
    let out = compile_and_run(
        r#"<?php
function clean($s) {
    $s = strtolower($s);
    $s = str_replace(" ", "_", $s);
    return $s;
}
echo clean("Hello World");
"#,
    );
    assert_eq!(out, "hello_world");
}

// Pattern: explode inside function, use result
#[test]
fn test_regression_explode_in_function_use_parts() {
    let out = compile_and_run(
        r#"<?php
function parse($csv) {
    $parts = explode(",", $csv);
    return $parts[0] . "+" . $parts[1];
}
echo parse("foo,bar");
"#,
    );
    assert_eq!(out, "foo+bar");
}

// Pattern: function returns assoc array, caller reads multiple keys
#[test]
fn test_regression_return_assoc_read_keys() {
    let out = compile_and_run(
        r#"<?php
function config() {
    return ["host" => "localhost", "port" => "3306", "db" => "myapp"];
}
$c = config();
echo $c["host"] . ":" . $c["port"] . "/" . $c["db"];
"#,
    );
    assert_eq!(out, "localhost:3306/myapp");
}

// Pattern: multiple string locals from hash_get in same function
#[test]
fn test_regression_multiple_hash_get_locals() {
    let out = compile_and_run(
        r#"<?php
function show($row) {
    $a = $row["first"];
    $b = $row["second"];
    $c = $row["third"];
    echo $a . "|" . $b . "|" . $c;
}
show(["first" => "x", "second" => "y", "third" => "z"]);
"#,
    );
    assert_eq!(out, "x|y|z");
}

// Pattern: class method with string param + string property access
#[test]
fn test_regression_method_string_param_and_prop() {
    let out = compile_and_run(
        r#"<?php
class Greeter {
    public $prefix;
    public function __construct($p) { $this->prefix = $p; }
    public function greet($name) { return $this->prefix . " " . $name . "!"; }
}
$g = new Greeter("Hello");
echo $g->greet("World");
"#,
    );
    assert_eq!(out, "Hello World!");
}

// Pattern: static method with string params
#[test]
fn test_regression_static_method_string() {
    let out = compile_and_run(
        r#"<?php
class Fmt {
    public static function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
}
echo Fmt::wrap("hello", "b");
"#,
    );
    assert_eq!(out, "<b>hello</b>");
}

// Pattern: chained property access $a->b->c
#[test]
fn test_regression_chained_property_access() {
    let out = compile_and_run(
        r#"<?php
class Inner { public $val;
    public function __construct($v) { $this->val = $v; }
}
class Outer { public $inner;
    public function __construct($i) { $this->inner = $i; }
}
$o = new Outer(new Inner(42));
echo $o->inner->val;
"#,
    );
    assert_eq!(out, "42");
}

// Pattern: float property in class
#[test]
fn test_regression_float_property() {
    let out = compile_and_run(
        r#"<?php
class Circle {
    public $radius;
    public function __construct($r) { $this->radius = $r; }
    public function area() { return 3.14 * $this->radius * $this->radius; }
}
$c = new Circle(10.0);
echo $c->area();
"#,
    );
    assert_eq!(out, "314");
}

// ========================================================================
// Math functions — trig, inverse trig, hyperbolic, log/exp, utility
// ========================================================================

#[test]
fn test_math_trig_basic() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(0.0), 4) . "|" . round(cos(0.0), 4) . "|" . round(tan(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

#[test]
fn test_math_trig_pi() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(M_PI_2), 4) . "|" . round(cos(M_PI), 1) . "|" . round(tan(M_PI_4), 4);
"#,
    );
    assert_eq!(out, "1|-1|1");
}

#[test]
fn test_math_inverse_trig() {
    let out = compile_and_run(
        r#"<?php
echo round(asin(1.0), 4) . "|" . round(acos(0.0), 4) . "|" . round(atan(1.0), 4);
"#,
    );
    assert_eq!(out, "1.5708|1.5708|0.7854");
}

#[test]
fn test_math_atan2() {
    let out = compile_and_run(
        r#"<?php
echo round(atan2(1.0, 0.0), 4);
"#,
    );
    assert_eq!(out, "1.5708");
}

#[test]
fn test_math_hyperbolic() {
    let out = compile_and_run(
        r#"<?php
echo round(sinh(0.0), 4) . "|" . round(cosh(0.0), 4) . "|" . round(tanh(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

#[test]
fn test_math_log_exp() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4) . "|" . log2(8.0) . "|" . log10(1000.0) . "|" . exp(0.0);
"#,
    );
    assert_eq!(out, "1|3|3|1");
}

#[test]
fn test_math_hypot() {
    let out = compile_and_run(
        r#"<?php
echo hypot(3.0, 4.0);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_math_deg_rad() {
    let out = compile_and_run(
        r#"<?php
echo round(deg2rad(180.0), 4) . "|" . round(rad2deg(M_PI), 1);
"#,
    );
    assert_eq!(out, "3.1416|180");
}

#[test]
fn test_math_pi_function() {
    let out = compile_and_run(
        r#"<?php
echo round(pi(), 4);
"#,
    );
    assert_eq!(out, "3.1416");
}

#[test]
fn test_math_constants() {
    let out = compile_and_run(
        r#"<?php
echo round(M_E, 4) . "|" . round(M_SQRT2, 4) . "|" . round(M_PI_2, 4) . "|" . round(M_PI_4, 4);
"#,
    );
    assert_eq!(out, "2.7183|1.4142|1.5708|0.7854");
}

#[test]
fn test_math_int_coercion() {
    let out = compile_and_run(
        r#"<?php
echo sin(0) . "|" . cos(0) . "|" . log(1) . "|" . exp(0);
"#,
    );
    assert_eq!(out, "0|1|0|1");
}

#[test]
fn test_math_distance_calculation() {
    let out = compile_and_run(
        r#"<?php
$x1 = 1.0; $y1 = 2.0;
$x2 = 4.0; $y2 = 6.0;
$dist = hypot($x2 - $x1, $y2 - $y1);
echo round($dist, 4);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_return_type_from_foreach() {
    let out = compile_and_run(
        r#"<?php
function find($arr, $target) {
    foreach ($arr as $v) {
        if ($v === $target) { return "found"; }
    }
    return "not found";
}
echo find([1, 2, 3], 2);
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_return_type_mixed_branches() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 0) { return "positive"; }
    return 0;
}
$r = describe(5);
echo $r;
"#,
    );
    assert_eq!(out, "positive");
}

#[test]
fn test_return_type_switch_foreach() {
    let out = compile_and_run(
        r#"<?php
function classify($items) {
    foreach ($items as $item) {
        switch ($item) {
            case 0: return "zero";
            default: return "nonzero";
        }
    }
    return "empty";
}
echo classify([0]);
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_return_string_from_else() {
    let out = compile_and_run(
        r#"<?php
function check($x) {
    if ($x > 10) {
        return "big";
    } else {
        return "small";
    }
}
echo check(5) . "|" . check(15);
"#,
    );
    assert_eq!(out, "small|big");
}

#[test]
fn test_log_natural() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_log_base_10() {
    let out = compile_and_run(
        r#"<?php
echo log(1000, 10);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_gc_local_alias_survives_original_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [21];
$a = $inner;
$b = $a;
unset($a);
unset($inner);
echo $b[0];
"#,
    );
    assert_eq!(out, "21");
}

#[test]
fn test_cow_indexed_array_alias_write_does_not_mutate_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$b = $a;
$b[0] = 9;
echo $a[0];
echo $b[0];
"#,
    );
    assert_eq!(out, "19");
}

#[test]
fn test_cow_assoc_array_alias_write_does_not_mutate_source() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1];
$b = $a;
$b["x"] = 2;
echo $a["x"];
echo $b["x"];
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_cow_array_growth_after_alias_keeps_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
$b = $a;
$b[4] = 5;
echo count($a);
echo count($b);
echo $a[0];
echo $b[4];
"#,
    );
    assert_eq!(out, "1515");
}

#[test]
fn test_cow_array_push_on_alias_keeps_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$a = [7];
$b = $a;
array_push($b, 9);
echo count($a);
echo count($b);
echo $a[0];
echo $b[1];
"#,
    );
    assert_eq!(out, "1279");
}

#[test]
fn test_cow_pass_by_value_array_mutation_splits_in_callee() {
    let out = compile_and_run(
        r#"<?php
function rewrite($arr) {
    $arr[0] = 9;
    echo $arr[0];
}

$a = [1];
rewrite($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "91");
}

#[test]
fn test_cow_nested_array_mutation_stays_shallow_until_inner_write() {
    let out = compile_and_run(
        r#"<?php
$outer = [[1]];
$copy = $outer;
$inner = $copy[0];
$inner[0] = 9;
$copy[0] = $inner;
echo $outer[0][0];
echo $copy[0][0];
"#,
    );
    assert_eq!(out, "19");
}

#[test]
fn test_example_cow_compiles_and_runs() {
    let out = compile_and_run(include_str!("../examples/cow/main.php"));
    assert_eq!(
        out,
        "left: 1 2 3 \nright: 99 2 3 4 \nouterA inner: 10 20 \nouterB inner: 10 77 \n"
    );
}

#[test]
fn test_cow_split_path_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$a = [1, 2, 3];
$b = $a;
$b[0] = 9;
unset($a);
unset($b);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

#[test]
fn test_gc_return_borrowed_nested_array_alias_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
function pick_first($rows) {
    $first = $rows[0];
    return $first;
}

$inner = [31];
$rows = [$inner, [32]];
$picked = pick_first($rows);
unset($rows);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "31");
}

#[test]
fn test_gc_control_flow_merge_borrowed_or_owned_return_survives() {
    let out = compile_and_run(
        r#"<?php
function choose($flag, $borrowed) {
    if ($flag) {
        $value = $borrowed;
    } else {
        $value = [42];
    }
    return $value;
}

$inner = [41];
$picked = choose(true, $inner);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "41");
}

#[test]
fn test_gc_control_flow_merge_owned_or_borrowed_other_branch_survives() {
    let out = compile_and_run(
        r#"<?php
function choose($flag, $borrowed) {
    if ($flag) {
        $value = [51];
    } else {
        $value = $borrowed;
    }
    return $value;
}

$inner = [52];
$picked = choose(false, $inner);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "52");
}

#[test]
fn test_gc_scope_exit_after_control_flow_borrowed_alias_survives() {
    let out = compile_and_run(
        r#"<?php
function pick_value($flag, $src) {
    if ($flag) {
        $tmp = $src[0];
    } else {
        $tmp = [0];
    }
    return $tmp;
}

$inner = [61];
$src = [$inner];
$picked = pick_value(true, $src);
unset($src);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "61");
}

#[test]
fn test_gc_scope_exit_after_exhaustive_if_owned_local_is_freed() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
function build_and_drop_direct() {
    $tmp = [11];
}

build_and_drop_direct();
build_and_drop_direct();
"#,
    );
    assert!(baseline.success, "baseline program failed: {}", baseline.stderr);
    let exhaustive = compile_and_run_with_gc_stats(
        r#"<?php
function build_and_drop($flag) {
    if ($flag) {
        $tmp = [11];
    } else {
        $tmp = [22];
    }
}

build_and_drop(true);
build_and_drop(false);
"#,
    );
    assert!(exhaustive.success, "program failed: {}", exhaustive.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (exhaustive_allocs, exhaustive_frees) = parse_gc_stats(&exhaustive.stderr);
    assert_eq!(baseline_allocs, exhaustive_allocs);
    assert_eq!(baseline_frees, exhaustive_frees);
}

#[test]
fn test_gc_nested_assoc_alias_survives_outer_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = ["nums" => [71, 72]];
$outer = ["box" => $inner];
$alias = $outer["box"];
unset($outer);
unset($inner);
$nums = $alias["nums"];
echo $nums[1];
"#,
    );
    assert_eq!(out, "72");
}

#[test]
fn test_gc_collect_cycles_reclaims_object_self_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
unset($n);
"#,
    );
    assert!(acyclic.success, "acyclic program failed: {}", acyclic.stderr);

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$n->next = $n;
unset($n);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(acyclic_allocs, cyclic_allocs);
    assert_eq!(acyclic_frees, cyclic_frees);
}

#[test]
fn test_gc_collect_cycles_reclaims_array_object_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$a = [$n];
unset($a);
unset($n);
"#,
    );
    assert!(acyclic.success, "acyclic program failed: {}", acyclic.stderr);

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$a = [$n];
$n->next = $a;
unset($a);
unset($n);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(acyclic_allocs, cyclic_allocs);
    assert_eq!(acyclic_frees, cyclic_frees);
}

#[test]
fn test_cow_array_array_assignment_detaches_before_forming_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [0];
$b = [0];
$a[0] = $b;
unset($a);
unset($b);
"#,
    );
    assert!(acyclic.success, "acyclic program failed: {}", acyclic.stderr);

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [0];
$b = [0];
$a[0] = $b;
$b[0] = $a;
unset($a);
unset($b);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(cyclic_allocs, acyclic_allocs + 1);
    assert_eq!(cyclic_frees, acyclic_frees + 1);
}

#[test]
fn test_cow_empty_array_assignment_detaches_before_forming_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [];
$b = [];
$a[0] = $b;
unset($a);
unset($b);
"#,
    );
    assert!(acyclic.success, "acyclic program failed: {}", acyclic.stderr);

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [];
$b = [];
$a[0] = $b;
$b[0] = $a;
unset($a);
unset($b);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(cyclic_allocs, acyclic_allocs + 1);
    assert_eq!(cyclic_frees, acyclic_frees + 1);
}

#[test]
fn test_cow_hash_assignment_detaches_before_forming_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = ["peer" => null];
$b = ["peer" => null];
$a["peer"] = $b;
unset($a);
unset($b);
"#,
    );
    assert!(acyclic.success, "acyclic program failed: {}", acyclic.stderr);

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = ["peer" => null];
$b = ["peer" => null];
$a["peer"] = $b;
$b["peer"] = $a;
unset($a);
unset($b);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(cyclic_allocs, acyclic_allocs + 1);
    assert_eq!(cyclic_frees, acyclic_frees + 1);
}

#[test]
fn test_gc_collect_cycles_reclaims_mixed_object_hash_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$h = ["node" => $n];
unset($h);
unset($n);
"#,
    );
    assert!(acyclic.success, "acyclic program failed: {}", acyclic.stderr);

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$h = ["node" => $n];
$n->next = $h;
unset($h);
unset($n);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(acyclic_allocs, cyclic_allocs);
    assert_eq!(acyclic_frees, cyclic_frees);
}

#[test]
fn test_gc_heap_free_coalesces_adjacent_blocks() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$a = array_fill(0, 2000, 1);
$b = array_fill(0, 2000, 2);
$keep = array_fill(0, 2000, 3);
unset($a);
unset($b);
$c = array_fill(0, 3000, 4);
echo $c[0] . "|" . count($c) . "|" . $keep[0];
"#,
        65_536,
    );
    assert_eq!(out, "4|3000|3");
}

#[test]
fn test_gc_heap_free_trims_free_tail_chain() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$a = array_fill(0, 2000, 1);
$b = array_fill(0, 2000, 2);
$tail = array_fill(0, 2000, 3);
unset($b);
unset($tail);
$c = array_fill(0, 5000, 4);
echo $c[0] . "|" . count($c) . "|" . $a[0];
"#,
        65_536,
    );
    assert_eq!(out, "4|5000|1");
}

#[test]
fn test_gc_heap_alloc_splits_oversized_free_block() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$large = array_fill(0, 4000, 1);
$keep = array_fill(0, 2000, 2);
unset($large);
$small = array_fill(0, 1000, 3);
$mid = array_fill(0, 2500, 4);
echo $small[0] . "|" . count($mid) . "|" . $keep[0];
"#,
        65_536,
    );
    assert_eq!(out, "3|2500|2");
}

#[test]
fn test_gc_heap_alloc_walks_past_small_first_free_block() {
    let out = compile_harness_and_run(
        "<?php",
        256,
        r#"    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_free_list@PAGE
    add x9, x9, _heap_free_list@PAGEOFF
    str xzr, [x9]
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #48]
    bl __rt_heap_free
    ldr x0, [sp, #16]
    bl __rt_heap_free
    mov x0, #16
    bl __rt_heap_alloc
    ldr x9, [sp, #16]
    cmp x0, x9
    cset x0, eq
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_gc_heap_alloc_reuses_small_bin_before_bump() {
    let out = compile_harness_and_run(
        "<?php",
        256,
        r#"    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_free_list@PAGE
    add x9, x9, _heap_free_list@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_small_bins@PAGE
    add x9, x9, _heap_small_bins@PAGEOFF
    stp xzr, xzr, [x9]
    stp xzr, xzr, [x9, #16]
    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    bl __rt_heap_free
    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    mov x0, #12
    bl __rt_heap_alloc
    ldr x9, [sp, #32]
    cmp x0, x9
    cset x11, eq
    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    ldr x9, [x9]
    ldr x10, [sp]
    cmp x9, x10
    cset x12, eq
    and x0, x11, x12
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_heap_debug_double_free_reports_error() {
    let err = compile_harness_expect_failure(
        "<?php",
        65_536,
        r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    bl __rt_heap_free
    ldr x0, [sp, #16]
    bl __rt_heap_free"#,
    );
    assert!(err.contains("heap debug detected double free"), "{err}");
}

#[test]
fn test_heap_debug_bad_refcount_reports_error() {
    let err = compile_harness_expect_failure(
        "<?php",
        65_536,
        r#"    mov x0, #16
    bl __rt_heap_alloc
    str wzr, [x0, #-12]
    bl __rt_incref"#,
    );
    assert!(err.contains("heap debug detected bad refcount"), "{err}");
}

#[test]
fn test_heap_debug_free_list_corruption_reports_error() {
    let err = compile_harness_expect_failure(
        "<?php",
        65_536,
        r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    ldr x0, [sp], #16
    bl __rt_heap_free
    sub x9, x0, #16
    str x9, [x9, #16]
    mov x0, #8
    bl __rt_heap_alloc"#,
    );
    assert!(err.contains("heap debug detected free-list corruption"), "{err}");
}

#[test]
fn test_heap_debug_reports_exit_summary() {
    let out = compile_and_run_with_heap_debug("<?php $a = [1, 2, 3]; unset($a);");
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: allocs="), "{}", out.stderr);
    assert!(out.stderr.contains("peak_live_bytes="), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary:"), "{}", out.stderr);
}

#[test]
fn test_heap_debug_poison_freed_payload() {
    let out = compile_harness_and_run_with_heap_debug(
        "<?php",
        65_536,
        r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    bl __rt_heap_free
    ldr x0, [sp], #16
    ldrb w0, [x0, #8]
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#,
    );
    assert_eq!(out, "165");
}

#[test]
fn test_array_literal_spread_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$nums = [...range(1, 10), ...range(11, 20), ...range(21, 30)];
echo count($nums) . "|" . $nums[25];
"#,
    );
    assert_eq!(out, "30|26");
}

#[test]
fn test_array_literal_spread_refcounted_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$inner = [1];
$a = array_fill(0, 10, $inner);
$b = array_fill(0, 10, $inner);
$c = [...$a, ...$b, ...$a];
echo count($c) . "|" . count($c[25]);
"#,
    );
    assert_eq!(out, "30|1");
}

#[test]
fn test_heap_kind_tags_raw_array_hash_and_string() {
    let out = compile_harness_and_run(
        "<?php",
        65_536,
        r#"    mov x0, #16
    bl __rt_heap_alloc
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov x0, #4
    mov x1, #8
    bl __rt_array_new
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov x0, #4
    mov x1, #0
    bl __rt_hash_new
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    adrp x1, _concat_buf@PAGE
    add x1, x1, _concat_buf@PAGEOFF
    mov w3, #65
    strb w3, [x1]
    mov w3, #66
    strb w3, [x1, #1]
    mov w3, #67
    strb w3, [x1, #2]
    mov x2, #3
    bl __rt_str_persist
    mov x0, x1
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#,
    );
    assert_eq!(out, "0231");
}

#[test]
fn test_new_object_codegen_sets_heap_kind() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, _) = compile_source_to_asm_with_options(
        "<?php class Foo { public $x = 1; } $o = new Foo();",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(asm.contains("new Foo()"));
    assert!(asm.contains("str x9, [x0, #-8]"), "{asm}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_decref_hash_codegen_skips_gc_for_scalar_only_hashes() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (asm, _) = compile_source_to_asm_with_options(
        r#"<?php
$map = ["a" => 1, "b" => 2];
unset($map);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(asm.contains("__rt_hash_may_have_cyclic_values"), "{asm}");
    assert!(asm.contains("bl __rt_hash_may_have_cyclic_values"), "{asm}");
    assert!(asm.contains("cbz x0, __rt_decref_hash_skip"), "{asm}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_log_base_2() {
    let out = compile_and_run(
        r#"<?php
echo log(256, 2);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_log_base_custom() {
    let out = compile_and_run(
        r#"<?php
echo round(log(27, 3), 4);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_expr_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$greet = function($name) { return "Hello " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_expr_call_returns_float() {
    let out = compile_and_run(
        r#"<?php
$calc = function($x) { return $x * 3.14; };
echo $calc(2.0);
"#,
    );
    assert_eq!(out, "6.28");
}

#[test]
fn test_expr_call_returns_int() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(21);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_expr_call_string_in_concat() {
    let out = compile_and_run(
        r#"<?php
$tag = function($s) { return "<b>" . $s . "</b>"; };
echo "Result: " . $tag("hello");
"#,
    );
    assert_eq!(out, "Result: <b>hello</b>");
}

#[test]
fn test_closure_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$fn = function() { return "test"; };
$result = $fn();
echo $result;
"#,
    );
    assert_eq!(out, "test");
}

// --- IIFE (Immediately Invoked Function Expression) ---

#[test]
fn test_iife_returns_string() {
    let out = compile_and_run(
        r#"<?php
$result = (function() { return "hello"; })();
echo $result;
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_iife_returns_int() {
    let out = compile_and_run(
        r#"<?php
echo (function($x) { return $x * 2; })(21);
"#,
    );
    assert_eq!(out, "42");
}

// --- Empty input / EOF handling ---

#[test]
fn test_empty_php_file() {
    let out = compile_and_run("<?php\n");
    assert_eq!(out, "");
}

#[test]
fn test_only_open_tag() {
    let out = compile_and_run("<?php ");
    assert_eq!(out, "");
}

// --- Syntactic return type inference ---

#[test]
fn test_callback_return_from_dowhile() {
    let out = compile_and_run(
        r#"<?php
function find_first($arr) {
    $i = 0;
    do {
        if ($arr[$i] > 5) { return $arr[$i]; }
        $i = $i + 1;
    } while ($i < count($arr));
    return 0;
}
echo find_first([1, 3, 7, 2]);
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_mixed_return_types_widened() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 100) { return "big"; }
    if ($n < 0) { return "negative"; }
    return $n;
}
echo describe(200);
"#,
    );
    assert_eq!(out, "big");
}

#[test]
fn test_null_coalesce_allocates_for_string_default() {
    let out = compile_and_run(
        r#"<?php
function test() {
    $x = null;
    $result = $x ?? "fallback";
    echo $result;
}
test();
"#,
    );
    assert_eq!(out, "fallback");
}

#[test]
fn test_null_coalesce_runtime_null_to_string_default() {
    let out = compile_and_run(
        r#"<?php
$x = false ? 1 : null;
$result = $x ?? "fallback";
echo $result;
"#,
    );
    assert_eq!(out, "fallback");
}

#[test]
fn test_closure_return_type_from_nested_branch() {
    let out = compile_and_run(
        r#"<?php
$describe = function($n) {
    if ($n > 0) {
        return "positive";
    }
    return 0;
};
$result = $describe(3);
echo $result;
"#,
    );
    assert_eq!(out, "positive");
}

#[test]
fn test_assigned_user_function_call_string_result() {
    let out = compile_and_run(
        r#"<?php
function greet($name) {
    return "Hello, " . $name;
}
function run() {
    $message = greet("World");
    echo $message;
}
run();
"#,
    );
    assert_eq!(out, "Hello, World");
}

#[test]
fn test_ternary_allocates_for_wider_type() {
    let out = compile_and_run(
        r#"<?php
function test($flag) {
    $val = $flag ? 42 : "none";
    echo $val;
}
test(false);
"#,
    );
    assert_eq!(out, "none");
}

#[test]
fn test_ternary_both_branches_in_function() {
    let out = compile_and_run(
        r#"<?php
function label($n) {
    $result = $n > 0 ? "positive" : "zero or negative";
    return $result;
}
echo label(5) . "|" . label(-1);
"#,
    );
    assert_eq!(out, "positive|zero or negative");
}

// === Pointer tests (v0.13) ===

#[test]
fn test_ptr_null_and_is_null() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo ptr_is_null($p) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_null_echo() {
    let out = compile_and_run(
        r#"<?php
echo ptr_null();
"#,
    );
    assert_eq!(out, "0x0");
}

#[test]
fn test_ptr_take_address() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
echo ptr_is_null($p) ? "null" : "not null";
"#,
    );
    assert_eq!(out, "not null");
}

#[test]
fn test_ptr_get_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
echo ptr_get($p);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_set_modifies_variable() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
$p = ptr($x);
ptr_set($p, 99);
echo $x;
"#,
    );
    assert_eq!(out, "99");
}

#[test]
fn test_ptr_offset() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_offset($p, 0);
echo ptr_get($q);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_cast() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_cast<int>($p);
echo ptr_get($q);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_strict_equal_after_cast() {
    let out = compile_and_run(
        r#"<?php
$x = 42;
$p = ptr($x);
$q = ptr_cast<int>($p);
echo $p === $q ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_sizeof_int() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("int");
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_ptr_sizeof_string() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("string");
"#,
    );
    assert_eq!(out, "16");
}

#[test]
fn test_ptr_sizeof_float() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("float");
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_ptr_sizeof_ptr() {
    let out = compile_and_run(
        r#"<?php
echo ptr_sizeof("ptr");
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_ptr_sizeof_class() {
    let out = compile_and_run(
        r#"<?php
class Point {
    public $x;
    public $y;
}
echo ptr_sizeof("Point");
"#,
    );
    // class_id(8) + 2 properties * 16 = 40
    assert_eq!(out, "40");
}

#[test]
fn test_ptr_sizeof_extern_class() {
    let out = compile_and_run(
        r#"<?php
extern class Point {
    public int $x;
    public int $y;
}
echo ptr_sizeof("Point");
"#,
    );
    assert_eq!(out, "16");
}

#[test]
fn test_ptr_strict_equal() {
    let out = compile_and_run(
        r#"<?php
$a = ptr_null();
$b = ptr_null();
echo $a === $b ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_strict_not_equal() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$a = ptr_null();
$b = ptr($x);
echo $a !== $b ? "1" : "0";
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ptr_echo_hex() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo $p;
"#,
    );
    assert_eq!(out, "0x0");
}

#[test]
fn test_ptr_gettype() {
    let out = compile_and_run(
        r#"<?php
$p = ptr_null();
echo gettype($p);
"#,
    );
    assert_eq!(out, "pointer");
}

#[test]
fn test_ptr_empty_null_and_non_null() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$p = ptr($x);
$n = ptr_null();
echo empty($n) ? "1" : "0";
echo empty($p) ? "1" : "0";
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_ptr_in_function() {
    let out = compile_and_run(
        r#"<?php
function double_via_ptr($p) {
    $val = ptr_get($p);
    ptr_set($p, $val * 2);
}
$x = 21;
double_via_ptr(ptr($x));
echo $x;
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ptr_in_loop() {
    let out = compile_and_run(
        r#"<?php
$sum = 0;
$p = ptr($sum);
for ($i = 1; $i <= 10; $i++) {
    ptr_set($p, ptr_get($p) + $i);
}
echo $sum;
"#,
    );
    assert_eq!(out, "55");
}

#[test]
fn test_ptr_read8_and_write8() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(1);
ptr_write8($buf, 255);
echo ptr_read8($buf);
free($buf);
"#,
    );
    assert_eq!(out, "255");
}

#[test]
fn test_ptr_read32_and_write32() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(int $size): ptr;
extern function free(ptr $p): void;

$buf = malloc(4);
ptr_write32($buf, 305419896);
echo ptr_read32($buf);
free($buf);
"#,
    );
    assert_eq!(out, "305419896");
}

#[test]
fn test_ptr_null_dereference_reports_runtime_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
$p = ptr_null();
echo ptr_get($p);
"#,
    );
    assert!(err.contains("Fatal error: null pointer dereference"));
}

// === FFI tests (v0.14) ===

#[test]
fn test_ffi_extern_abs() {
    let out = compile_and_run(
        r#"<?php
extern function abs(int $n): int;
echo abs(-42);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_ffi_extern_atoi() {
    let out = compile_and_run(
        r#"<?php
extern function atoi(string $s): int;
echo atoi("12345");
"#,
    );
    assert_eq!(out, "12345");
}

#[test]
fn test_ffi_extern_strlen() {
    let out = compile_and_run(
        r#"<?php
extern function strlen(string $s): int;
echo strlen("hello world");
"#,
    );
    assert_eq!(out, "11");
}

#[test]
fn test_ffi_extern_strlen_frees_borrowed_cstr_temp() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
extern function strlen(string $s): int;
"#,
    );
    assert!(baseline.success, "baseline program failed: {}", baseline.stderr);
    let out = compile_and_run_with_gc_stats(
        r#"<?php
extern function strlen(string $s): int;
strlen("hello");
strlen("world");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees, "{}", out.stderr);
}

#[test]
fn test_ffi_malloc_and_free() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
}

$buf = malloc(16);
echo ptr_is_null($buf) ? "null" : "ok";
free($buf);
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_memset_fills_raw_buffer() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
}

$buf = malloc(4);
memset($buf, 65, 4);
echo ptr_read8($buf);
echo ",";
echo ptr_read8(ptr_offset($buf, 3));
free($buf);
"#,
    );
    assert_eq!(out, "65,65");
}

#[test]
fn test_ffi_memcpy_copies_raw_buffer() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memcpy(ptr $dest, ptr $src, int $count): ptr;
}

$src = malloc(4);
$dst = malloc(4);
ptr_write32($src, 305419896);
memcpy($dst, $src, 4);
echo ptr_read32($dst);
free($dst);
free($src);
"#,
    );
    assert_eq!(out, "305419896");
}

#[test]
fn test_ffi_extern_getpid() {
    let out = compile_and_run(
        r#"<?php
extern function getpid(): int;
$pid = getpid();
echo $pid > 0 ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

#[test]
fn test_ffi_extern_string_return() {
    let out = compile_and_run(
        r#"<?php
extern function getenv(string $name): string;
$home = getenv("HOME");
echo strlen($home) > 0 ? "ok" : "empty";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
#[ignore] // requires SDL2 library installed locally
fn test_ffi_sdl_init_and_ticks() {
    let out = compile_and_run(
        r#"<?php
extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_GetTicks(): int;
    function SDL_Delay(int $ms): void;
}

$SDL_INIT_VIDEO = 32;
echo SDL_Init($SDL_INIT_VIDEO) === 0 ? "init|" : "fail|";
$before = SDL_GetTicks();
SDL_Delay(10);
$after = SDL_GetTicks();
echo $after >= $before ? "ticks" : "bad";
SDL_Quit();
"#,
    );
    assert_eq!(out, "init|ticks");
}

#[test]
#[ignore] // requires SDL2 library installed locally
fn test_ffi_sdl_window_with_dummy_driver() {
    let out = compile_and_run(
        r#"<?php
putenv("SDL_VIDEODRIVER=dummy");

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
}

$SDL_INIT_VIDEO = 32;
if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "init fail";
    exit(1);
}

$window = SDL_CreateWindow("test", 0, 0, 64, 64, 0);
echo ptr_is_null($window) ? "null" : "ok";
if (!ptr_is_null($window)) {
    SDL_DestroyWindow($window);
}
SDL_Quit();
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
#[ignore] // requires SDL2 library installed locally
fn test_ffi_sdl_keyboard_state_pointer() {
    let out = compile_and_run(
        r#"<?php
putenv("SDL_VIDEODRIVER=dummy");

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_PumpEvents(): void;
    function SDL_GetKeyboardState(ptr $numkeys): ptr;
}

$SDL_INIT_VIDEO = 32;
if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "init fail";
    exit(1);
}

SDL_PumpEvents();
$keys = SDL_GetKeyboardState(ptr_null());
echo ptr_is_null($keys) ? "null" : "ok";
SDL_Quit();
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_extern_block_syntax() {
    let out = compile_and_run(
        r#"<?php
extern "System" {
    function abs(int $n): int;
    function atoi(string $s): int;
}
echo abs(-7) . "," . atoi("99");
"#,
    );
    assert_eq!(out, "7,99");
}

#[test]
fn test_ffi_extern_lib_function_syntax() {
    let out = compile_and_run(
        r#"<?php
extern "System" function abs(int $n): int;
echo abs(-1);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_ffi_extern_void_return() {
    let out = compile_and_run(
        r#"<?php
extern function abs(int $n): int;
$x = abs(-5);
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_ffi_extern_float_arg_and_return() {
    let out = compile_and_run(
        r#"<?php
extern function sqrt(float $x): float;
echo sqrt(144.0);
"#,
    );
    assert_eq!(out, "12");
}

#[test]
fn test_ffi_extern_multiple_args() {
    let out = compile_and_run(
        r#"<?php
extern function strtol(string $s, ptr $endptr, int $base): int;
echo strtol("FF", ptr_null(), 16);
"#,
    );
    assert_eq!(out, "255");
}

#[test]
fn test_ffi_extern_multiple_string_args() {
    let out = compile_and_run(
        r#"<?php
extern function strcmp(string $left, string $right): int;
echo strcmp("aa", "ab") < 0 ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_extern_multiple_string_args_free_all_borrowed_cstr_temps() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
extern function strcmp(string $left, string $right): int;
"#,
    );
    assert!(baseline.success, "baseline program failed: {}", baseline.stderr);
    let out = compile_and_run_with_gc_stats(
        r#"<?php
extern function strcmp(string $left, string $right): int;
strcmp("aa", "ab");
strcmp("bb", "bb");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees, "{}", out.stderr);
}

#[test]
fn test_ffi_extern_global() {
    let out = compile_and_run(
        r#"<?php
extern global ptr $environ;
echo ptr_is_null($environ) ? "fail" : "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_callback_signal_handler() {
    let out = compile_and_run(
        r#"<?php
extern function signal(int $sig, callable $handler): ptr;
extern function raise(int $sig): int;

function on_signal($sig) {
    echo $sig;
}

signal(15, "on_signal");
raise(15);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn test_ffi_extern_non_string_global_smoke() {
    let out = compile_and_run(
        r#"<?php
extern function getpid(): int;
$pid = getpid();
echo $pid > 0 ? "ok" : "fail";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_ffi_extern_in_function() {
    let out = compile_and_run(
        r#"<?php
extern function abs(int $n): int;
function my_abs($x) {
    return abs($x);
}
echo my_abs(-10);
"#,
    );
    assert_eq!(out, "10");
}

#[test]
fn test_trait_basic_method_import() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() { return "hello"; }
}
class Person {
    use Greeter;
}
$p = new Person();
echo $p->greet();
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_trait_class_method_override_wins() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() { return "trait"; }
}
class Person {
    use Greeter;
    public function greet() { return "class"; }
}
$p = new Person();
echo $p->greet();
"#,
    );
    assert_eq!(out, "class");
}

#[test]
fn test_trait_insteadof_and_alias() {
    let out = compile_and_run(
        r#"<?php
trait A {
    public function label() { return "A"; }
}
trait B {
    public function label() { return "B"; }
}
class Box {
    use A, B {
        A::label insteadof B;
        B::label as bLabel;
    }
}
$b = new Box();
echo $b->label();
echo ":";
echo $b->bLabel();
"#,
    );
    assert_eq!(out, "A:B");
}

#[test]
fn test_trait_property_default_and_method_access() {
    let out = compile_and_run(
        r#"<?php
trait Counter {
    public $value = 7;
    public function read() { return $this->value; }
}
class Box {
    use Counter;
}
$b = new Box();
echo $b->read();
"#,
    );
    assert_eq!(out, "7");
}

#[test]
fn test_trait_can_use_another_trait() {
    let out = compile_and_run(
        r#"<?php
trait BaseGreeter {
    public function greet() { return "A"; }
}
trait FancyGreeter {
    use BaseGreeter;
    public function greetTwice() { return $this->greet() . "B"; }
}
class Person {
    use FancyGreeter;
}
$p = new Person();
echo $p->greetTwice();
"#,
    );
    assert_eq!(out, "AB");
}

#[test]
fn test_trait_static_method_import() {
    let out = compile_and_run(
        r#"<?php
trait Numbers {
    public static function one() { return 1; }
}
class Box {
    use Numbers;
}
echo Box::one();
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_class_protected_members_are_accessible_inside_class_methods() {
    let out = compile_and_run(
        r#"<?php
class SecretBox {
    protected $value = 41;

    protected function next() {
        return $this->value + 1;
    }

    public function reveal() {
        return $this->next();
    }
}

$box = new SecretBox();
echo $box->reveal();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_trait_protected_alias_is_callable_inside_class() {
    let out = compile_and_run(
        r#"<?php
trait Greeter {
    public function greet() {
        return "hello";
    }
}

class Demo {
    use Greeter {
        Greeter::greet as protected innerGreet;
    }

    public function reveal() {
        return $this->innerGreet();
    }
}

$demo = new Demo();
echo $demo->reveal();
"#,
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_class_protected_static_method_is_callable_inside_class() {
    let out = compile_and_run(
        r#"<?php
class SecretMath {
    protected static function base() {
        return 41;
    }

    public static function answer() {
        return SecretMath::base() + 1;
    }
}

echo SecretMath::answer();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_inheritance_dynamic_dispatch_uses_child_override() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public function speak() {
        return "animal";
    }

    public function run() {
        return $this->speak();
    }
}

class Dog extends Animal {
    public function speak() {
        return "dog";
    }
}

$dog = new Dog();
echo $dog->run();
"#,
    );
    assert_eq!(out, "dog");
}

#[test]
fn test_inheritance_parent_private_method_stays_lexically_bound() {
    let out = compile_and_run(
        r#"<?php
class Base {
    private function secret() {
        return "base";
    }

    public function reveal() {
        return $this->secret();
    }
}

class Child extends Base {
    public function secret() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "base");
}

#[test]
fn test_self_static_call_uses_lexical_class() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function label() {
        return "base";
    }

    public function reveal() {
        return self::label();
    }
}

class Child extends Base {
    public static function label() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "base");
}

#[test]
fn test_self_instance_call_stays_lexically_bound() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public function reveal() {
        return self::label();
    }

    public function label() {
        return "base";
    }
}

class Child extends Base {
    public function label() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "base");
}

#[test]
fn test_static_late_binding_uses_child_override_from_instance_method() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function who() {
        return "base";
    }

    public function reveal() {
        return static::who();
    }
}

class Child extends Base {
    public static function who() {
        return "child";
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "child");
}

#[test]
fn test_static_late_binding_uses_child_override_from_static_method() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static function who() {
        return "base";
    }

    public static function relay() {
        return static::who();
    }
}

class Child extends Base {
    public static function who() {
        return "child";
    }
}

echo Child::relay();
"#,
    );
    assert_eq!(out, "child");
}

#[test]
fn test_named_static_call_is_non_forwarding_but_self_is_forwarding() {
    let out = compile_and_run(
        r#"<?php
class A {
    public static function who() {
        return static::tag();
    }

    public static function relayNamed() {
        return A::who();
    }

    public static function relaySelf() {
        return self::who();
    }

    public static function tag() {
        return "A";
    }
}

class B extends A {
    public static function tag() {
        return "B";
    }
}

echo B::relayNamed() . " " . B::relaySelf();
"#,
    );
    assert_eq!(out, "A B");
}

#[test]
fn test_parent_static_call_is_forwarding() {
    let out = compile_and_run(
        r#"<?php
class A {
    public static function who() {
        return static::tag();
    }

    public static function tag() {
        return "A";
    }
}

class B extends A {
    public static function relay() {
        return parent::who();
    }

    public static function tag() {
        return "B";
    }
}

echo B::relay();
"#,
    );
    assert_eq!(out, "B");
}

#[test]
fn test_inheritance_parent_method_call_and_inherited_properties() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $a = 40;

    public function greet() {
        return "hi";
    }
}

class Child extends Base {
    public $b = 2;

    public function total() {
        return $this->a + $this->b;
    }

    public function greet() {
        return parent::greet() . "!";
    }
}

$child = new Child();
echo $child->total() . " " . $child->greet();
"#,
    );
    assert_eq!(out, "42 hi!");
}

#[test]
fn test_inheritance_protected_members_are_accessible_from_subclass() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected $value = 41;

    protected function readValue() {
        return $this->value;
    }
}

class Child extends Base {
    public function reveal() {
        return $this->readValue() + 1;
    }
}

$child = new Child();
echo $child->reveal();
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_inherited_constructor_specializes_base_string_property_type() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function greet() {
        return $this->name;
    }
}

class Child extends Base {}

$child = new Child("Ada");
echo $child->greet();
"#,
    );
    assert_eq!(out, "Ada");
}

#[test]
fn test_array_literal_allows_sibling_objects_with_common_parent() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function label() {
        return $this->name;
    }
}

class Dog extends Animal {}
class Cat extends Animal {}

$animals = [new Dog("Rex"), new Cat("Mia")];
foreach ($animals as $animal) {
    echo $animal->label() . " ";
}
"#,
    );
    assert_eq!(out, "Rex Mia ");
}

#[test]
fn test_interface_contract_can_be_satisfied_by_concrete_class() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

class User implements Named {
    public function name() {
        return "Ada";
    }
}

$user = new User();
echo $user->name();
"#,
    );
    assert_eq!(out, "Ada");
}

#[test]
fn test_abstract_base_can_defer_method_to_concrete_child() {
    let out = compile_and_run(
        r#"<?php
abstract class BaseGreeter {
    abstract public function label();

    public function greet() {
        return "hi " . $this->label();
    }
}

class PersonGreeter extends BaseGreeter {
    public function label() {
        return "world";
    }
}

$g = new PersonGreeter();
echo $g->greet();
"#,
    );
    assert_eq!(out, "hi world");
}

#[test]
fn test_class_can_implement_multiple_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Tagged {
    public function tag();
}

class Item implements Named, Tagged {
    public function name() {
        return "box";
    }

    public function tag() {
        return "BX";
    }
}

$item = new Item();
echo $item->name() . ":" . $item->tag();
"#,
    );
    assert_eq!(out, "box:BX");
}

#[test]
fn test_transitive_interface_extends_is_enforced() {
    let out = compile_and_run(
        r#"<?php
interface Named {
    public function name();
}

interface Labeled extends Named {
    public function label();
}

class Product implements Labeled {
    public function name() {
        return "widget";
    }

    public function label() {
        return strtoupper($this->name());
    }
}

$product = new Product();
echo $product->label();
"#,
    );
    assert_eq!(out, "WIDGET");
}

#[test]
fn test_example_interfaces_compiles_and_runs() {
    let out = compile_and_run(include_str!("../examples/interfaces/main.php"));
    assert_eq!(out, "WIDGET\n");
}
