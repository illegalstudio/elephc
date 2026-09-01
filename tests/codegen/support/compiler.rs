//! Purpose:
//! Compiler fixture helpers for turning inline PHP snippets into assembly or expected compile failures.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Centralizes compile options, define handling, runtime harness injection, and diagnostic capture.

use super::*;

/// Returns true when codegen fixtures are compiling through the EIR backend.
pub(crate) fn codegen_fixture_uses_ir_backend() -> bool {
    true
}

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
) -> (String, String, TestLinkRequirements) {
    compile_source_to_asm_with_counters(source, dir, heap_size, gc_stats, false, heap_debug)
}

/// Like `compile_source_to_asm_with_options`, with the `--counters` exit dump enabled.
pub(crate) fn compile_source_to_asm_with_counters(
    source: &str,
    dir: &Path,
    heap_size: usize,
    gc_stats: bool,
    counters: bool,
    heap_debug: bool,
) -> (String, String, TestLinkRequirements) {
    compile_source_to_asm_with_options_and_regex(
        source,
        dir,
        heap_size,
        gc_stats,
        counters,
        heap_debug,
        false,
    )
}

/// Compiles one fixture while optionally mirroring the CLI's explicit `--with-regex` capability.
fn compile_source_to_asm_with_options_and_regex(
    source: &str,
    dir: &Path,
    heap_size: usize,
    gc_stats: bool,
    counters: bool,
    heap_debug: bool,
    with_regex: bool,
) -> (String, String, TestLinkRequirements) {
    compile_source_to_asm_with_defines_repr_regex_and_php_version(
        source,
        dir,
        &HashSet::new(),
        heap_size,
        gc_stats,
        counters,
        heap_debug,
        default_null_repr(),
        with_regex,
        elephc::php_version::PhpVersion::default(),
    )
}

// Runs the full compiler pipeline with user-supplied conditional defines.
// Substitutes magic constants (`__FILE__`, `__DIR__`, etc.), applies `ifdef` conditionals,
// builds the autoload registry, resolves includes, runs name resolution, optimizes,
// type-checks, and generates ARM64/x86_64 assembly for the current target.
// Returns user assembly, runtime assembly, and library names required for linking.
/// Provides the Compile source to asm with defines helper used by the compiler module.
/// Uses the environment-selected null representation (`ELEPHC_NULL_REPR`).
pub(crate) fn compile_source_to_asm_with_defines(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
) -> (String, String, TestLinkRequirements) {
    compile_source_to_asm_with_defines_repr(
        source,
        dir,
        defines,
        heap_size,
        gc_stats,
        heap_debug,
        default_null_repr(),
    )
}

/// Returns the null representation selected for this test process: `ELEPHC_NULL_REPR` can
/// force either mode; without it the compiler default (tagged) applies.
pub(crate) fn default_null_repr() -> elephc::codegen::NullRepr {
    match std::env::var("ELEPHC_NULL_REPR").as_deref() {
        Ok("tagged") => elephc::codegen::NullRepr::Tagged,
        Ok("sentinel") => elephc::codegen::NullRepr::Sentinel,
        _ => elephc::codegen::NullRepr::default(),
    }
}

/// Full compile-to-assembly pipeline with an explicit null representation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_source_to_asm_with_defines_repr(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
    null_repr: elephc::codegen::NullRepr,
) -> (String, String, TestLinkRequirements) {
    compile_source_to_asm_with_defines_repr_regex_and_php_version(
        source,
        dir,
        defines,
        heap_size,
        gc_stats,
        false,
        heap_debug,
        null_repr,
        false,
        elephc::php_version::PhpVersion::default(),
    )
}

/// Runs the full fixture pipeline for an explicit PHP compatibility version.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_source_to_asm_with_defines_repr_and_php_version(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    heap_debug: bool,
    null_repr: elephc::codegen::NullRepr,
    php_version: elephc::php_version::PhpVersion,
) -> (String, String, TestLinkRequirements) {
    compile_source_to_asm_with_defines_repr_regex_and_php_version(
        source,
        dir,
        defines,
        heap_size,
        gc_stats,
        false,
        heap_debug,
        null_repr,
        false,
        php_version,
    )
}

/// Runs the full fixture pipeline with explicit regex and PHP-version settings.
#[allow(clippy::too_many_arguments)]
fn compile_source_to_asm_with_defines_repr_regex_and_php_version(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    counters: bool,
    heap_debug: bool,
    null_repr: elephc::codegen::NullRepr,
    with_regex: bool,
    php_version: elephc::php_version::PhpVersion,
) -> (String, String, TestLinkRequirements) {
    let (user_asm, runtime_asm, link_requirements) = try_compile_source_to_asm_with_defines_repr(
        source,
        dir,
        defines,
        heap_size,
        gc_stats,
        counters,
        heap_debug,
        null_repr,
        with_regex,
        php_version,
    );
    (
        user_asm.expect("EIR backend codegen failed for codegen fixture"),
        runtime_asm,
        link_requirements,
    )
}

/// Compiles a snippet and returns the EIR backend's diagnostic text, asserting that the
/// backend refused the program.
///
/// Backend refusals (`unsupported EIR backend feature: …`) are raised after type checking,
/// so `tests/error_tests.rs` — which stops at the checker — cannot observe them. Use this
/// for shapes elephc deliberately declines to compile.
pub(crate) fn compile_source_expect_backend_error(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    let (user_asm, _runtime_asm, _link_requirements) = try_compile_source_to_asm_with_defines_repr(
        source,
        &dir,
        &HashSet::new(),
        8_388_608,
        false,
        false,
        false,
        default_null_repr(),
        false,
        elephc::php_version::PhpVersion::default(),
    );
    let _ = fs::remove_dir_all(&dir);
    match user_asm {
        Ok(_) => panic!("expected the EIR backend to reject this program, but it compiled"),
        Err(error) => error.to_string(),
    }
}

/// Runs the codegen-fixture pipeline and hands back the backend's `Result` instead of
/// unwrapping it, so callers can assert on either outcome.
#[allow(clippy::too_many_arguments)]
fn try_compile_source_to_asm_with_defines_repr(
    source: &str,
    dir: &Path,
    defines: &HashSet<String>,
    heap_size: usize,
    gc_stats: bool,
    counters: bool,
    heap_debug: bool,
    null_repr: elephc::codegen::NullRepr,
    with_regex: bool,
    php_version: elephc::php_version::PhpVersion,
) -> (
    std::result::Result<String, elephc::codegen::CodegenIrError>,
    String,
    TestLinkRequirements,
) {
    elephc::codegen::set_null_repr(null_repr);
    elephc::codegen::set_compile_profile(php_version, false);
    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let synthetic_main = dir.join("test.php");
    let ast = elephc::magic_constants::substitute_file_and_scope_constants(ast, &synthetic_main);
    let ast = elephc::conditional::apply(ast, defines);
    let (autoload_registry, ast) = elephc::autoload::Registry::build(dir, ast);
    elephc::codegen::set_autoload_rule_count(autoload_registry.rule_count());
    let resolved = elephc::resolver::resolve(ast, dir).expect("resolve failed");
    let resolved = elephc::autoload::collect_aliases(resolved);
    let mut prelude_inventory = elephc::optimize::reachability::PreludeInventory::new();
    // Surface usage is decided BEFORE injection, mirroring `pipeline::compile`:
    // the harness seeds `set_linked_extensions` from the same bits so
    // `extension_loaded('PDO'/'mysqli')` agrees between `compile_and_run` and
    // the CLI. mysqli injects after PDO so the shared `elephc_pdo` externs
    // (merged in idempotently by either) are declared exactly once.
    let pdo_used = elephc::pdo_prelude::program_uses_pdo(&resolved);
    let resolved = elephc::pdo_prelude::inject_if_used_for_version(
        resolved,
        false,
        php_version,
        &mut prelude_inventory,
    );
    let mysqli_used = elephc::mysqli_prelude::program_uses_mysqli(&resolved);
    let resolved = elephc::mysqli_prelude::inject_if_used(
        resolved,
        false,
        php_version,
        &mut prelude_inventory,
    );
    let mut linked_php_surfaces: Vec<String> = Vec::new();
    if pdo_used {
        linked_php_surfaces.push("PDO".to_string());
    }
    if mysqli_used {
        linked_php_surfaces.push("mysqli".to_string());
    }
    elephc::codegen::set_linked_extensions(linked_php_surfaces);
    let tz_used = elephc::tz_prelude::program_uses_tz(&resolved);
    let resolved =
        elephc::tz_prelude::inject_if_used(resolved, tz_used, &mut prelude_inventory);
    let resolved = elephc::list_id_prelude::inject_if_used(resolved, &mut prelude_inventory);
    let resolved = elephc::var_export_prelude::inject_if_used(resolved, &mut prelude_inventory);
    let resolved =
        elephc::image_prelude::inject_if_used(resolved, false, &mut prelude_inventory);
    let resolved = elephc::hash_prelude::inject_if_used(resolved, false, &mut prelude_inventory);
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved =
        elephc::autoload::run(resolved, dir, &autoload_registry).expect("autoload failed");
    // Mirrors `pipeline::compile`: `func_num_args`/`func_get_args`/`func_get_arg` are
    // desugared into a hidden variadic parameter plus plain PHP after autoloading and
    // before the optimizer, so the checker and the backend only ever see ordinary PHP.
    let resolved = elephc::func_args::desugar(resolved).expect("func_args desugar failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let mut check_result =
        elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized =
        elephc::optimize::propagate_constants(resolved, check_result.mixed_storage_local_names());
    let optimized = elephc::optimize::prune_constant_control_flow(
        optimized,
        check_result.local_binding_decision_spans(),
    );
    let optimized = elephc::optimize::normalize_control_flow(
        optimized,
        check_result.local_binding_decision_spans(),
    );
    let optimized = elephc::optimize::eliminate_dead_code(
        optimized,
        check_result.local_binding_decision_spans(),
    );
    let empty_roots = HashSet::new();
    let structural_groups = if tz_used {
        HashSet::from([
            elephc::tz_prelude::TIMELIB_RUNTIME_REACHABILITY_GROUP.to_string(),
        ])
    } else {
        HashSet::new()
    };
    let optimized = elephc::optimize::prune_unreachable_declarations(
        optimized,
        &mut check_result,
        elephc::optimize::reachability::PruneOptions {
            inventory: &prelude_inventory,
            forced_groups: &empty_roots,
            structural_groups: &structural_groups,
            exported_functions: &empty_roots,
            eval_forced: false,
        },
    );
    let requires_elephc_tls = check_result
        .required_libraries
        .iter()
        .any(|lib| lib == "elephc_tls");
    let mut ir_module =
        lower_and_validate_ir_for_codegen_fixture(&optimized, &check_result, &synthetic_main);
    if with_regex {
        ir_module.required_runtime_features.regex = true;
    }
    let exported_functions = HashMap::new();
    // Honor ELEPHC_REGALLOC so the whole codegen suite can be run under both
    // the linear-scan allocator (default) and the stack fallback.
    let regalloc_linear = !matches!(std::env::var("ELEPHC_REGALLOC").as_deref(), Ok("stack"));
    let user_asm = elephc::codegen::generate_user_asm_from_ir_with_options(
        &ir_module,
        gc_stats,
        counters,
        elephc::codegen::Instrumentation::Off, // the exact profiler has its own tests
        false, // probe
        heap_debug,
        requires_elephc_tls,
        elephc::codegen::Emit::Executable,
        &exported_functions,
        regalloc_linear,
        false,
        elephc::codegen::WebIsolation::Worker,
    );
    let mut runtime_features = ir_module.required_runtime_features;
    runtime_features.php_profile = php_version.minor() as u8;
    let runtime_asm =
        elephc::codegen::generate_runtime_with_features(heap_size, target(), runtime_features);
    let link_requirements = TestLinkRequirements::new(
        check_result.required_libraries,
        elephc::codegen::link_requirements_for_runtime_features(runtime_features),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)
    (user_asm, runtime_asm, link_requirements)
}

/// Lowers codegen fixtures to EIR, runs the default-on IR optimizer, and validates the result.
pub(crate) fn lower_and_validate_ir_for_codegen_fixture(
    program: &elephc::parser::ast::Program,
    check_result: &elephc::types::CheckResult,
    source_path: &Path,
) -> elephc::ir::Module {
    let mut module = elephc::ir_lower::lower_program_with_source_path(
        program,
        check_result,
        target(),
        source_path,
    )
        .expect("AST-to-EIR lowering failed for codegen fixture");
    if ir_opt_enabled_for_codegen_fixture() {
        elephc::ir_passes::optimize_module(&mut module);
    }
    elephc::ir::validate_module(&module).expect("EIR validation failed for codegen fixture");
    module
}

/// Returns whether the codegen fixture should run EIR optimization passes,
/// matching the CLI's `ELEPHC_IR_OPT=off|on` default-on behavior.
fn ir_opt_enabled_for_codegen_fixture() -> bool {
    match std::env::var("ELEPHC_IR_OPT").as_deref() {
        Ok("off") => false,
        Ok("on") => true,
        _ => true,
    }
}

/// Returns the process-exit epilogue emitted for a supported test target.
fn main_exit_needle(target: Target) -> &'static str {
    match (target.platform, target.arch) {
        (Platform::MacOS, Arch::AArch64) => "    mov x0, #0\n    mov x16, #1\n    svc #0x80",
        (Platform::Linux, Arch::AArch64) => "    mov x0, #0\n    mov x8, #94\n    svc #0",
        (Platform::Linux, Arch::X86_64) => "    mov edi, 0\n    mov eax, 231\n    syscall",
        (_, Arch::AArch64) => panic!(
            "main exit harness is not implemented yet for target {}",
            target
        ),
        (_, Arch::X86_64) => panic!(
            "main exit harness is not implemented yet for target {}",
            target
        ),
    }
}

/// Injects an exit harness before the target's final process-exit epilogue.
///
/// Transforms macOS-dialect harness assembly for Linux and panics when codegen no
/// longer emits the expected target-specific epilogue.
pub(crate) fn inject_main_exit_harness(asm: &str, harness: &str) -> String {
    let needle = main_exit_needle(target());
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
pub(crate) fn compile_harness_expect_failure(
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

// Compiles a PHP source snippet with `--counters` and runs it, capturing stdout and
// stderr; stderr is expected to contain one `elephc-counters: <name> <count>` line per
// non-synthetic PHP function.
/// Provides the Compile and run with call counters helper used by the compiler module.
pub(crate) fn compile_and_run_with_counters(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_counters(source, &dir, 8_388_608, false, true, false);
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
    compile_and_run_capture_with_optional_regex(source, false)
}

/// Compiles and runs a fixture with the same explicit regex capability as `--with-regex`.
pub(crate) fn compile_and_run_capture_with_regex(source: &str) -> ProgramOutput {
    compile_and_run_capture_with_optional_regex(source, true)
}

/// Compiles and captures one fixture while optionally enabling managed regex support.
fn compile_and_run_capture_with_optional_regex(
    source: &str,
    with_regex: bool,
) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options_and_regex(
            source, &dir, 8_388_608, false, false, false, with_regex,
        );
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
    compile_and_run_with_heap_size_and_optional_regex(source, heap_size, false)
}

/// Compiles and runs one fixture while optionally enabling managed regex support.
fn compile_and_run_with_heap_size_and_optional_regex(
    source: &str,
    heap_size: usize,
    with_regex: bool,
) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options_and_regex(
            source, &dir, heap_size, false, false, false, with_regex,
        );
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

/// Compiles and runs a fixture with the same explicit regex capability as `--with-regex`.
pub(crate) fn compile_and_run_with_regex(source: &str) -> String {
    compile_and_run_with_heap_size_and_optional_regex(source, 8_388_608, true)
}

/// Compiles and runs PHP source with an isolated `PHPRC` file containing `ini`.
pub(crate) fn compile_and_run_with_php_ini(source: &str, ini: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!(
        "elephc_test_php_ini_{}_{:?}_{}",
        pid, tid, id
    ));
    fs::create_dir_all(&dir).unwrap();
    let ini_path = dir.join("php.ini");
    fs::write(&ini_path, ini).unwrap();

    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run_with_env(
        &user_asm,
        &runtime_obj,
        &dir,
        &requirements,
        &default_link_paths(),
        &[],
        &[("PHPRC", ini_path.as_os_str())],
    );
    let _ = fs::remove_dir_all(&dir);
    output
}

/// Compiles and runs PHP source with an explicit PHP compatibility version.
pub(crate) fn compile_and_run_with_php_version(
    source: &str,
    php_version: elephc::php_version::PhpVersion,
) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!(
        "elephc_test_php_version_{}_{:?}_{}",
        pid, tid, id
    ));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, requirements) =
        compile_source_to_asm_with_defines_repr_and_php_version(
            source,
            &dir,
            &HashSet::new(),
            8_388_608,
            false,
            false,
            default_null_repr(),
            php_version,
        );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let output = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &requirements,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    output
}

/// Compiles and runs a PHP source with the legacy sentinel null representation forced on,
/// regardless of `ELEPHC_NULL_REPR`. Used by the sentinel opt-out guard tests.
pub(crate) fn compile_and_run_sentinel(source: &str) -> String {
    compile_and_run_with_repr(source, elephc::codegen::NullRepr::Sentinel)
}

/// Compiles and runs a PHP source with the tagged null representation forced on,
/// regardless of `ELEPHC_NULL_REPR`. Used by null-sentinel surface tests.
pub(crate) fn compile_and_run_tagged(source: &str) -> String {
    compile_and_run_with_repr(source, elephc::codegen::NullRepr::Tagged)
}

/// Compiles and runs a PHP source with an explicit null representation.
fn compile_and_run_with_repr(source: &str, null_repr: elephc::codegen::NullRepr) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_tagged_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_defines_repr(
        source,
        &dir,
        &HashSet::new(),
        8_388_608,
        false,
        false,
        null_repr,
    );
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
                        "PHP compat note: output differs for tagged test.\n  elephc: {:?}\n  php:    {:?}",
                        elephc_out, php_out
                    );
                }
            }
        }
    }

    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

/// Returns `user_asm` with the compiled script's embedded paths removed, for needle assertions.
///
/// `_script_source_file` carries the CANONICAL PATH of the compiled script, read by
/// `Throwable::getFile()` and by the ` in <file>:<line>` suffix of the uncaught-exception report.
/// For a fixture that path is the harness's own temp directory — and those directories are named
/// after the test, so a needle the test searches for is often a substring of the path it just
/// compiled from. `!user_asm.contains("pow")` in a fixture compiled under
/// `elephc_constant_folding_pow` matched the DIRECTORY NAME, not a surviving `pow` call.
///
/// `_program_source_file` carries the same canonical path for native fatal diagnostics, so its
/// duplicate bytes must be removed as well. Only those path bytes are dropped. Every instruction
/// and every other data literal survives, so an assertion keeps exactly the meaning it had — in
/// particular, string literals an optimizer was supposed to eliminate are still visible, which is
/// what several of these tests actually check.
///
/// This cannot be folded into `compile_source_to_asm_with_options`: callers pass its result on to
/// `assemble_and_run`, and a `_script_source_file` with no bytes would make the assembled program
/// report a garbage filename.
pub(crate) fn asm_without_embedded_script_path(user_asm: &str) -> String {
    let mut out = Vec::new();
    let mut drop_next_ascii = false;
    for line in user_asm.lines() {
        if drop_next_ascii && line.trim_start().starts_with(".ascii") {
            drop_next_ascii = false;
            continue;
        }
        drop_next_ascii = matches!(
            line.trim(),
            "_script_source_file:" | "_program_source_file:"
        );
        out.push(line);
    }
    out.join("\n")
}

#[cfg(test)]
mod exit_harness_tests {
    use super::*;

    /// Verifies optimizer assembly checks ignore both runtime copies of the fixture path only.
    #[test]
    fn embedded_script_path_filter_removes_both_canonical_path_copies() {
        let asm = r#"_script_source_file:
    .ascii "/tmp/dead-pow.php"
_script_source_file_len:
    .quad 17
_program_source_file:
    .ascii "/tmp/dead-pow.php"
_program_source_file_len:
    .quad 17
_live_literal:
    .ascii "dead-pow""#;

        let filtered = asm_without_embedded_script_path(asm);
        assert!(!filtered.contains("/tmp/dead-pow.php"));
        assert!(filtered.contains(".ascii \"dead-pow\""));
    }

    /// Verifies each supported target uses the process-wide exit epilogue emitted by codegen.
    #[test]
    fn main_exit_needles_match_supported_target_abis() {
        assert_eq!(
            main_exit_needle(Target::new(Platform::MacOS, Arch::AArch64)),
            "    mov x0, #0\n    mov x16, #1\n    svc #0x80"
        );
        assert_eq!(
            main_exit_needle(Target::new(Platform::Linux, Arch::AArch64)),
            "    mov x0, #0\n    mov x8, #94\n    svc #0"
        );
        assert_eq!(
            main_exit_needle(Target::new(Platform::Linux, Arch::X86_64)),
            "    mov edi, 0\n    mov eax, 231\n    syscall"
        );
    }
}
