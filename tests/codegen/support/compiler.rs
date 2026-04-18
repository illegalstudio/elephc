use super::*;

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
    let ast = elephc::conditional::apply(ast, defines);
    let resolved = elephc::resolver::resolve(ast, dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::prune_constant_control_flow(resolved);
    let optimized = elephc::optimize::eliminate_dead_code(optimized);
    let (user_asm, runtime_asm) = elephc::codegen::generate(
        &optimized,
        &check_result.global_env,
        &check_result.functions,
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
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)
    (user_asm, runtime_asm, check_result.required_libraries)
}

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

pub(crate) fn compile_harness_expect_failure(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let runtime_obj = assemble_custom_runtime(heap_size, &dir);
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

pub(crate) fn compile_harness_and_run(source: &str, heap_size: usize, harness: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);
    let runtime_obj = assemble_custom_runtime(heap_size, &dir);
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

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, true);
    let runtime_obj = assemble_custom_runtime(heap_size, &dir);
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

pub(crate) fn compile_and_run_with_gc_stats(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, true, false);
    let output = assemble_and_run_capture(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

pub(crate) fn compile_and_run_with_heap_debug(source: &str) -> ProgramOutput {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, true);
    let output = assemble_and_run_capture(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    output
}

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

/// Compile a PHP source string to a native binary, run it, and return stdout.
/// Uses the elephc library directly (no subprocess) for tokenize → parse → check → codegen.
/// Only spawns as + ld + binary execution.
pub(crate) fn compile_and_run_with_heap_size(source: &str, heap_size: usize) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, heap_size, false, false);

    let custom_rt;
    let runtime_obj: &Path = if heap_size == 8_388_608 {
        get_runtime_obj()
    } else {
        custom_rt = assemble_custom_runtime(heap_size, &dir);
        &custom_rt
    };

    let elephc_out = assemble_and_run(
        &user_asm,
        runtime_obj,
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

pub(crate) fn compile_and_run(source: &str) -> String {
    compile_and_run_with_heap_size(source, 8_388_608)
}
