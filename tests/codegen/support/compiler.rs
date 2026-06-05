//! Purpose:
//! Compiler fixture helpers for turning inline PHP snippets into assembly or expected compile failures.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Centralizes compile options, define handling, runtime harness injection, and diagnostic capture.

use super::*;

// Variant of `compile_source_to_asm_with_defines` that uses an empty define set.
// Runs the full pipeline (tokenize → parse → resolve → type check → optimize → codegen)
// and returns user assembly, runtime assembly, and required libraries for linking.
/// Provides the Compile source to asm with options helper used by the compiler module.
pub(crate) fn compile_source_to_asm_with_options(
    source: &str,
    dir: &Path,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, String, Vec<String>) {
    compile_source_to_asm_with_defines(
        source,
        dir,
        &HashSet::new(),
        heap_size,
        gc_stats,
        heap_debug,
    )
}

// Runs the full compiler pipeline with user-supplied conditional defines.
// Substitutes magic constants (`__FILE__`, `__DIR__`, etc.), applies `ifdef` conditionals,
// builds the autoload registry, resolves includes, runs name resolution, optimizes,
// type-checks, and generates ARM64/x86_64 assembly for the current target.
// Returns user assembly, runtime assembly, and library names required for linking.
/// Provides the Compile source to asm with defines helper used by the compiler module.
pub(crate) fn compile_source_to_asm_with_defines(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, String, Vec<String>) {
    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let synthetic_main = dir.join("test.php");
    let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &synthetic_main);
    let ast = elephc::conditional::apply(ast, defines);
    let (autoload_registry, ast) = elephc::autoload::Registry::build(dir, ast);
    elephc::codegen::set_autoload_rule_count(autoload_registry.rule_count());
    let resolved = elephc::resolver::resolve(ast, dir).expect("resolve failed");
    let resolved = elephc::autoload::collect_aliases(resolved);
    let resolved = elephc::pdo_prelude::inject_if_used(resolved);
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::autoload::run(resolved, dir, &autoload_registry).expect("autoload failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::propagate_constants(resolved);
    let optimized = elephc::optimize::prune_constant_control_flow(optimized);
    let optimized = elephc::optimize::normalize_control_flow(optimized);
    let optimized = elephc::optimize::eliminate_dead_code(optimized);
    let requires_elephc_tls = check_result
        .required_libraries
        .iter()
        .any(|lib| lib == "elephc_tls");
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
        heap_size,
        gc_stats,
        heap_debug,
        target(),
        requires_elephc_tls,
    );
    let runtime_features =
        elephc::codegen::runtime_features_for_program_and_classes(&optimized, &check_result.classes);
    let mut required_libraries = check_result.required_libraries;
    for lib in elephc::codegen::required_libraries_for_runtime_features(runtime_features) {
        if !required_libraries.contains(&lib) {
            required_libraries.push(lib);
        }
    }
    // user assembly is already platform-correct (emitters handle platform at emit time)
    (user_asm, runtime_asm, required_libraries)
}

// Injects an exit harness into user assembly before the final `ret` instruction.
// Rewrites macOS-style syscall sequence to Linux-style syscall sequence if needed,
// then patches the assembly in-place using a target-specific needle. Panics if the
// needle is not found (indicates a codegen emit change that broke the harness injection).
/// Injects main exit harness into the compiler metadata registry.
pub(crate) fn inject_main_exit_harness(asm: &str, harness: &str) -> String {
    let needle = match (target().platform, target().arch) {
        (Platform::MacOS, Arch::AArch64) => "    mov x0, #0\n    mov x16, #1\n    svc #0x80",
        (Platform::Linux, Arch::AArch64) => "    mov x0, #0\n    mov x8, #93\n    svc #0",
        (Platform::Linux, Arch::X86_64) => "    mov edi, 0\n    mov eax, 60\n    syscall",
        (_, Arch::X86_64) => panic!(
            "main exit harness is not implemented yet for target {}",
            target()
        ),
    };
    // Harness strings are written in macOS assembly dialect; transform for Linux if needed
    let harness = target().transform_assembly(harness);
    let replacement = format!("{harness}\n{needle}");
    let patched = asm.replacen(needle, &replacement, 1);
    assert_ne!(patched, asm, "failed to inject main exit harness");
    patched
}

// Compiles a PHP source snippet and runs it with an injected harness, expecting a failure.
// Captures stderr from the resulting process and returns it for assertion.
// Used for error-test fixtures that verify compile-time diagnostic messages.
// Cleans up the temporary directory after execution.
/// Provides the Compile harness expect failure helper used by the compiler module.
pub(crate) fn compile_harness_expect_failure(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let patched = inject_main_exit_harness(&user_asm, harness);
    let stderr = assemble_and_run_expect_failure(
        &patched,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stderr
}

// Compiles a PHP source snippet and runs it with an injected harness, capturing stdout.
// Used for codegen tests that verify output against expected strings. Harness is provided
// by the caller (e.g., a printf replacement). Cleans up the temporary directory after execution.
/// Provides the Compile harness and run helper used by the compiler module.
pub(crate) fn compile_harness_and_run(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let patched = inject_main_exit_harness(&user_asm, harness);
    let stdout = assemble_and_run(
        &patched,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stdout
}

// Same as `compile_harness_and_run` but enables heap debug mode for ownership/GC testing.
// Runs with a custom runtime assembled from the provided heap size.
/// Provides the Compile harness and run with heap debug helper used by the compiler module.
pub(crate) fn compile_harness_and_run_with_heap_debug(
    source: &str,
    heap_size: usize,
    harness: &str,
) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let patched = inject_main_exit_harness(&user_asm, harness);
    let stdout = assemble_and_run(
        &patched,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stdout
}

// Compiles a PHP source snippet and runs it with GC statistics enabled.
// Captures stdout and stderr; stderr is expected to contain `GC: allocs=N frees=N`.
// Uses the default 8_388_608-byte heap and enables gc_stats during codegen.
/// Provides the Compile and run with GC stats helper used by the compiler module.
pub(crate) fn compile_and_run_with_gc_stats(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, true, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run_capture(
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

// Compiles a PHP source snippet and runs it with the default 8_388_608-byte heap,
// capturing stdout and stderr from the resulting binary. Cleans up the temp directory.
/// Provides the Compile and run capture helper used by the compiler module.
pub(crate) fn compile_and_run_capture(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run_capture(
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

// Compiles a PHP source snippet and runs it with heap debug mode enabled.
// Heap debug adds guard bytes and poisoning around allocations to catch GC bugs.
// Uses the default 8_388_608-byte heap and enables heap_debug during codegen.
/// Provides the Compile and run with heap debug helper used by the compiler module.
pub(crate) fn compile_and_run_with_heap_debug(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, true);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run_capture(
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

// Parses GC statistics from stderr output produced when gc_stats is enabled.
// Expects a line matching `GC: allocs=N frees=N` and returns (allocs, frees).
// Panics if the line is missing or the numbers cannot be parsed.
/// Provides the Parse GC stats helper used by the compiler module.
pub(crate) fn parse_gc_stats(stderr: &str) -> (u64, u64) {
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

// Compile a PHP source string to a native binary, run it, and return stdout.
// Uses the elephc library directly (no subprocess) for tokenize → parse → check → codegen.
// Only spawns as + ld + binary execution.
/// Provides the Compile and run with heap size helper used by the compiler module.
pub(crate) fn compile_and_run_with_heap_size(source: &str, heap_size: usize) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);

    let elephc_out = assemble_and_run(
        &user_asm,
        &runtime_obj,
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

// Convenience wrapper that calls `compile_and_run_with_heap_size` with the default
// 8_388_608-byte heap. Most codegen tests use this directly.
/// Provides the Compile and run helper used by the compiler module.
pub(crate) fn compile_and_run(source: &str) -> String {
    compile_and_run_with_heap_size(source, 8_388_608)
}
