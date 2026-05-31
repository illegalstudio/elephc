//! Purpose:
//! Project fixture helpers for CLI and multi-file codegen tests.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Creates isolated temporary projects, invokes the elephc binary, and supports include/require fixtures.

use super::*;

/// Combines checker-required libraries with libraries required by feature-gated runtime helpers.
fn required_libraries_for_codegen(
    program: &elephc::parser::ast::Program,
    check_result: &elephc::types::CheckResult,
) -> Vec<String> {
    let runtime_features =
        elephc::codegen::runtime_features_for_program_and_classes(program, &check_result.classes);
    let mut required_libraries = check_result.required_libraries.clone();
    for lib in elephc::codegen::required_libraries_for_runtime_features(runtime_features) {
        if !required_libraries.contains(&lib) {
            required_libraries.push(lib);
        }
    }
    required_libraries
}

// Creates an isolated temporary directory for CLI tests using a unique prefix,
// process ID, thread ID, and auto-incrementing counter. Used to avoid file collisions
// when tests run in parallel.
/// Creates cli test dir for the surrounding test or metadata fixture.
pub(crate) fn make_cli_test_dir(prefix: &str) -> std::path::PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// Returns the path to the `elephc` CLI binary built by cargo.
// Resolves `CARGO_BIN_EXE_elephc` env var or falls back to locating the binary
// relative to the current test executable (handles `deps/` suffix stripping).
/// Provides the Elephc cli bin helper used by the projects module.
pub(crate) fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

// Constructs a `Command` preconfigured to run the `elephc` CLI in a given directory.
// Sets `XDG_CACHE_HOME` to an isolated cache subdirectory and sets the working directory.
// Used by CLI tests that invoke `elephc` as a subprocess.
/// Provides the Elephc cli command helper used by the projects module.
pub(crate) fn elephc_cli_command(dir: &Path) -> Command {
    let mut cmd = Command::new(elephc_cli_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd
}

// Compiles a PHP source string with conditional defines and runs the resulting binary.
// Uses the full compiler pipeline (no CLI subprocess) with the default 8_388_608-byte heap.
// Returns stdout. Cleans up the temporary directory after execution.
/// Provides the Compile and run with defines helper used by the projects module.
pub(crate) fn compile_and_run_with_defines(source: &str, defines: &[&str]) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_defines(source, &dir, &define_set, 8_388_608, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let elephc_out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

// Writes a PHP source file to a temp directory and compiles it using the `elephc` CLI
// (not the library). Passes `--define` flags for each define in `defines`.
// Runs the resulting binary and returns stdout. Cleans up the temp directory.
// Used for CLI integration tests that exercise the binary interface end-to-end.
/// Provides the Compile cli file and run helper used by the projects module.
pub(crate) fn compile_cli_file_and_run(source: &str, defines: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_cli_test");

    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let mut compile_cmd = elephc_cli_command(&dir);
    for define in defines {
        compile_cmd.arg("--define").arg(define);
    }
    compile_cmd.arg(&php_path);
    let compile_out = compile_cmd.output().expect("failed to run elephc CLI");
    assert!(
        compile_out.status.success(),
        "elephc CLI failed: {}",
        String::from_utf8_lossy(&compile_out.stderr)
    );

    let bin_path = dir.join("main");
    let output = run_binary(&bin_path, &dir);
    assert!(
        output.status.success(),
        "CLI-compiled binary exited with error"
    );

    let _ = fs::remove_dir_all(&dir);
    String::from_utf8(output.stdout).unwrap()
}

// Compiles a PHP source string and runs the resulting binary, asserting that it
// terminates with a non-zero exit code. Returns stderr from the failed binary.
// Uses the library directly (not CLI), with default heap size 8_388_608 bytes.
/// Provides the Compile and run expect failure helper used by the projects module.
pub(crate) fn compile_and_run_expect_failure(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run_expect_failure(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

// Compiles a multi-file PHP project (using library directly, not CLI) where the
// main entry point is `main_file`. Writes all files to an isolated temp directory,
// runs the full pipeline, links, and asserts the binary exits successfully.
// Returns stdout and cleans up.
/// Provides the Compile and run files helper used by the projects module.
pub(crate) fn compile_and_run_files(files: &[(&str, &str)], main_file: &str) -> String {
    compile_and_run_files_with_defines(files, main_file, &[])
}

// Compiles a multi-file PHP project and runs the binary, asserting it fails at runtime.
// Builds the full pipeline (lexer, parser, resolver, optimizer, type checker, codegen),
// links with the runtime, and captures stderr from the failed process.
// Used for error/regression fixtures that verify runtime failures (e.g., type mismatches,
// missing properties, undefined behavior that only surfaces at execution time).
/// Provides the Compile and run files expect failure helper used by the projects module.
pub(crate) fn compile_and_run_files_expect_failure(
    files: &[(&str, &str)],
    main_file: &str,
) -> String {
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
    let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &php_path);
    let define_set = HashSet::new();
    let ast = elephc::conditional::apply(ast, &define_set);
    let resolved = elephc::resolver::resolve(ast, base_dir).expect("resolve failed");
    let resolved = elephc::autoload::collect_aliases(resolved);
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result =
        elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::propagate_constants(resolved);
    let optimized = elephc::optimize::prune_constant_control_flow(optimized);
    let optimized = elephc::optimize::normalize_control_flow(optimized);
    let optimized = elephc::optimize::eliminate_dead_code(optimized);
    let (user_asm, runtime_asm) = elephc::codegen::generate(
        &optimized,
        &check_result.global_env,
        &check_result.functions,
        &check_result.callable_param_sigs,
        &check_result.callable_return_sigs,
        &check_result.callable_array_return_sigs,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    let required_libraries = required_libraries_for_codegen(&optimized, &check_result);

    let elephc_err = assemble_and_run_expect_failure(
        &user_asm,
        &runtime_obj_for_asm(&runtime_asm),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    elephc_err
}

// Compiles a multi-file PHP project with user-supplied conditional defines.
// Writes all files to an isolated temp directory, builds the autoload registry,
// resolves includes, and runs the full pipeline. Returns stdout from the binary.
/// Provides the Compile and run files with defines helper used by the projects module.
pub(crate) fn compile_and_run_files_with_defines(
    files: &[(&str, &str)],
    main_file: &str,
    defines: &[&str],
) -> String {
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
    let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &php_path);
    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let ast = elephc::conditional::apply(ast, &define_set);
    let (autoload_registry, ast) = elephc::autoload::Registry::build(base_dir, ast);
    elephc::codegen::set_autoload_rule_count(autoload_registry.rule_count());
    let resolved = elephc::resolver::resolve(ast, base_dir).expect("resolve failed");
    let resolved = elephc::autoload::collect_aliases(resolved);
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::autoload::run(resolved, base_dir, &autoload_registry)
        .expect("autoload failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result =
        elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::propagate_constants(resolved);
    let optimized = elephc::optimize::prune_constant_control_flow(optimized);
    let optimized = elephc::optimize::normalize_control_flow(optimized);
    let optimized = elephc::optimize::eliminate_dead_code(optimized);
    let (user_asm, runtime_asm) = elephc::codegen::generate(
        &optimized,
        &check_result.global_env,
        &check_result.functions,
        &check_result.callable_param_sigs,
        &check_result.callable_return_sigs,
        &check_result.callable_array_return_sigs,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    let required_libraries = required_libraries_for_codegen(&optimized, &check_result);
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let elephc_out = assemble_and_run(
        &user_asm,
        &runtime_obj_for_asm(&runtime_asm),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

// Returns true if compilation of a multi-file PHP project fails (type-check or earlier).
// Writes all files to an isolated temp directory. Runs the full pipeline up to type checking;
// does not assemble or link. Used for negative test fixtures.
/// Provides the Compile files fails helper used by the projects module.
pub(crate) fn compile_files_fails(files: &[(&str, &str)], main_file: &str) -> bool {
    compile_files_fails_with_defines(files, main_file, &[])
}

// Attempts compilation of a multi-file PHP project with conditional defines.
// Returns true if the type-check pass fails. Does not assemble or link.
// Used for negative test fixtures that require specific defines to trigger the failure.
/// Provides the Compile files fails with defines helper used by the projects module.
pub(crate) fn compile_files_fails_with_defines(
    files: &[(&str, &str)],
    main_file: &str,
    defines: &[&str],
) -> bool {
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
        let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &php_path);
        let define_set: HashSet<String> =
            defines.iter().map(|define| (*define).to_string()).collect();
        let ast = elephc::conditional::apply(ast, &define_set);
        let resolved = elephc::resolver::resolve(ast, base_dir)?;
        let resolved = elephc::autoload::collect_aliases(resolved);
        let resolved = elephc::name_resolver::resolve(resolved)?;
        let resolved = elephc::optimize::fold_constants(resolved);
        elephc::types::check_with_target(&resolved, target())?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&dir);
    result.is_err()
}

// Compiles a PHP source string, links it, runs it with stdin wired to `stdin_data`,
// and returns stdout. Writes the binary to an isolated temp directory.
// Used for tests that verify runtime behavior with specific input (e.g., read(), fgets).
/// Provides the Compile and run with stdin helper used by the projects module.
pub(crate) fn compile_and_run_with_stdin(source: &str, stdin_data: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let synthetic_main = dir.join("test.php");
    let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &synthetic_main);
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let resolved = elephc::autoload::collect_aliases(resolved);
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::propagate_constants(resolved);
    let optimized = elephc::optimize::prune_constant_control_flow(optimized);
    let optimized = elephc::optimize::normalize_control_flow(optimized);
    let optimized = elephc::optimize::eliminate_dead_code(optimized);
    let (user_asm, runtime_asm) = elephc::codegen::generate(
        &optimized,
        &check_result.global_env,
        &check_result.functions,
        &check_result.callable_param_sigs,
        &check_result.callable_return_sigs,
        &check_result.callable_array_return_sigs,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    let required_libraries = required_libraries_for_codegen(&optimized, &check_result);
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, &user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        &runtime_obj_for_asm(&runtime_asm),
        &bin_path,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    use std::io::Write;
    let bin_cmd = if target().platform == Platform::Linux
        && target().arch == Arch::AArch64
        && cfg!(target_arch = "x86_64")
    {
        "qemu-aarch64-static"
    } else {
        bin_path.to_str().unwrap()
    };
    let mut cmd = if target().platform == Platform::Linux
        && target().arch == Arch::AArch64
        && cfg!(target_arch = "x86_64")
    {
        let mut c = Command::new(bin_cmd);
        c.arg(&bin_path);
        c
    } else {
        Command::new(&bin_path)
    };
    let mut child = cmd
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

// Compiles a PHP source string, runs the binary, and returns stdout alongside the
// temp directory path. The directory is preserved after the run so callers can
// inspect written files (e.g., for file I/O fixture verification).
/// Provides the Compile and run in dir helper used by the projects module.
pub(crate) fn compile_and_run_in_dir(source: &str) -> (String, std::path::PathBuf) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let synthetic_main = dir.join("test.php");
    let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &synthetic_main);
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let resolved = elephc::autoload::collect_aliases(resolved);
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::propagate_constants(resolved);
    let optimized = elephc::optimize::prune_constant_control_flow(optimized);
    let optimized = elephc::optimize::normalize_control_flow(optimized);
    let optimized = elephc::optimize::eliminate_dead_code(optimized);
    let (user_asm, runtime_asm) = elephc::codegen::generate(
        &optimized,
        &check_result.global_env,
        &check_result.functions,
        &check_result.callable_param_sigs,
        &check_result.callable_return_sigs,
        &check_result.callable_array_return_sigs,
        &check_result.interfaces,
        &check_result.classes,
        &check_result.enums,
        &check_result.packed_classes,
        &check_result.extern_functions,
        &check_result.extern_classes,
        &check_result.extern_globals,
        8_388_608,
        false,
        false,
        target(),
    );
    let required_libraries = required_libraries_for_codegen(&optimized, &check_result);
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let elephc_out = assemble_and_run(
        &user_asm,
        &runtime_obj_for_asm(&runtime_asm),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    (elephc_out, dir)
}
