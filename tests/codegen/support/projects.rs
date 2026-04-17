use super::*;

pub(crate) fn make_cli_test_dir(prefix: &str) -> std::path::PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

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

pub(crate) fn compile_and_run_with_defines(source: &str, defines: &[&str]) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_defines(source, &dir, &define_set, 8_388_608, false, false);
    let elephc_out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

pub(crate) fn compile_cli_file_and_run(source: &str, defines: &[&str]) -> String {
    let dir = make_cli_test_dir("elephc_cli_test");

    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();

    let mut compile_cmd = Command::new(elephc_cli_bin());
    for define in defines {
        compile_cmd.arg("--define").arg(define);
    }
    compile_cmd.arg(&php_path).current_dir(&dir);
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

/// Compile a PHP source string and assert the generated binary fails at runtime.
pub(crate) fn compile_and_run_expect_failure(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let output = assemble_and_run_expect_failure(
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

/// Compile a PHP project with multiple files using the library directly.
pub(crate) fn compile_and_run_files(files: &[(&str, &str)], main_file: &str) -> String {
    compile_and_run_files_with_defines(files, main_file, &[])
}

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
    let define_set: HashSet<String> = defines.iter().map(|define| (*define).to_string()).collect();
    let ast = elephc::conditional::apply(ast, &define_set);
    let resolved = elephc::resolver::resolve(ast, base_dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result =
        elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::prune_constant_control_flow(resolved);
    let (user_asm, _runtime_asm) = elephc::codegen::generate(
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
        8_388_608,
        false,
        false,
        target(),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let elephc_out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    elephc_out
}

/// Write multiple files and attempt compilation. Returns true if compilation fails.
pub(crate) fn compile_files_fails(files: &[(&str, &str)], main_file: &str) -> bool {
    compile_files_fails_with_defines(files, main_file, &[])
}

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
        let define_set: HashSet<String> =
            defines.iter().map(|define| (*define).to_string()).collect();
        let ast = elephc::conditional::apply(ast, &define_set);
        let resolved = elephc::resolver::resolve(ast, base_dir)?;
        let resolved = elephc::name_resolver::resolve(resolved)?;
        let resolved = elephc::optimize::fold_constants(resolved);
        elephc::types::check_with_target(&resolved, target())?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&dir);
    result.is_err()
}

pub(crate) fn compile_and_run_with_stdin(source: &str, stdin_data: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::prune_constant_control_flow(resolved);
    let (user_asm, _runtime_asm) = elephc::codegen::generate(
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
        8_388_608,
        false,
        false,
        target(),
    );
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
        get_runtime_obj(),
        &bin_path,
        &check_result.required_libraries,
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

/// Compile and run in a specific temp dir (returns dir path for file I/O tests).
pub(crate) fn compile_and_run_in_dir(source: &str) -> (String, std::path::PathBuf) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let tokens = elephc::lexer::tokenize(source).expect("tokenize failed");
    let ast = elephc::parser::parse(&tokens).expect("parse failed");
    let resolved = elephc::resolver::resolve(ast, &dir).expect("resolve failed");
    let resolved = elephc::name_resolver::resolve(resolved).expect("name resolve failed");
    let resolved = elephc::optimize::fold_constants(resolved);
    let check_result = elephc::types::check_with_target(&resolved, target()).expect("type check failed");
    let optimized = elephc::optimize::prune_constant_control_flow(resolved);
    let (user_asm, _runtime_asm) = elephc::codegen::generate(
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
        8_388_608,
        false,
        false,
        target(),
    );
    // user assembly is already platform-correct (emitters handle platform at emit time)

    let elephc_out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &check_result.required_libraries,
        &default_link_paths(),
        &[],
    );
    (elephc_out, dir)
}
