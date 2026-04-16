use super::*;

pub(crate) fn get_runtime_obj() -> &'static Path {
    RUNTIME_OBJ.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("elephc_test_runtime_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target());
        let asm_path = dir.join("runtime.s");
        let obj_path = dir.join("runtime.o");
        fs::write(&asm_path, &runtime_asm).unwrap();

        let mut cmd = Command::new(assembler_cmd());
        if target().platform == Platform::MacOS {
            cmd.args(["-arch", target().darwin_arch_name()]);
        }
        cmd.arg("-o").arg(&obj_path).arg(&asm_path);
        let status = cmd.status().expect("failed to assemble runtime");
        assert!(status.success(), "runtime assembler failed");
        obj_path
    })
}

/// Assemble a custom runtime for tests that need a non-default heap size.
pub(crate) fn assemble_custom_runtime(heap_size: usize, dir: &Path) -> std::path::PathBuf {
    let runtime_asm = elephc::codegen::generate_runtime(heap_size, target());
    let asm_path = dir.join("runtime.s");
    let obj_path = dir.join("runtime.o");
    fs::write(&asm_path, &runtime_asm).unwrap();

    let mut cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        cmd.args(["-arch", target().darwin_arch_name()]);
    }
    cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let status = cmd.status().expect("failed to assemble custom runtime");
    assert!(status.success(), "custom runtime assembler failed");
    obj_path
}


pub(crate) fn link_binary(
    obj_path: &Path,
    runtime_obj: &Path,
    bin_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) {
    let actual_link_libs = effective_link_libs(extra_link_libs);

    match target().platform {
        Platform::MacOS => {
            let mut ld_cmd = Command::new("ld");
            ld_cmd.args(["-arch", target().darwin_arch_name(), "-e", "_main", "-o"]);
            ld_cmd.arg(bin_path);
            ld_cmd.arg(obj_path);
            ld_cmd.arg(runtime_obj);
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
            for lib in &actual_link_libs {
                ld_cmd.arg(format!("-l{}", lib));
            }
            for framework in extra_frameworks {
                ld_cmd.args(["-framework", framework]);
            }
            let ld_status = ld_cmd.status().expect("failed to run linker");
            assert!(ld_status.success(), "linker failed");
        }
        Platform::Linux => {
            let mut ld_cmd = Command::new(gcc_cmd());
            ld_cmd.arg("-o").arg(bin_path);
            ld_cmd.arg(obj_path);
            ld_cmd.arg(runtime_obj);
            if actual_link_libs.is_empty() {
                ld_cmd.arg("-static");
            }
            if !actual_link_libs.is_empty() {
                ld_cmd.arg("-Wl,--no-as-needed");
            }
            for path in extra_link_paths {
                ld_cmd.arg(format!("-L{}", path));
            }
            for lib in &actual_link_libs {
                ld_cmd.arg(format!("-l{}", lib));
            }
            if !actual_link_libs.is_empty() {
                ld_cmd.arg("-Wl,--as-needed");
            }
            // Math and POSIX regex libraries needed on Linux
            ld_cmd.args(["-lm", "-lpthread"]);
            let ld_status = ld_cmd.status().expect("failed to run linker");
            assert!(ld_status.success(), "linker failed");
        }
    }
}

/// Run a compiled binary, using qemu on Linux x86_64 for ARM64 binaries.
pub(crate) fn run_binary(bin_path: &Path, dir: &Path) -> std::process::Output {
    if target().platform == Platform::Linux
        && target().arch == Arch::AArch64
        && cfg!(target_arch = "x86_64")
    {
        let mut cmd = Command::new("qemu-aarch64-static");
        if let Some(sysroot) = qemu_sysroot() {
            cmd.args(["-L", sysroot]);
        }
        cmd.arg(bin_path)
            .current_dir(dir)
            .output()
            .expect("failed to run compiled binary via qemu")
    } else {
        Command::new(bin_path)
            .current_dir(dir)
            .output()
            .expect("failed to run compiled binary")
    }
}

pub(crate) fn assemble_and_run(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        runtime_obj,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = run_binary(&bin_path, dir);
    assert!(
        output.status.success(),
        "binary exited with error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).unwrap()
}

pub(crate) struct ProgramOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) success: bool,
}

pub(crate) fn assemble_and_run_capture(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> ProgramOutput {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        runtime_obj,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = run_binary(&bin_path, dir);

    ProgramOutput {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        success: output.status.success(),
    }
}

pub(crate) fn assemble_and_run_expect_failure(
    user_asm: &str,
    runtime_obj: &Path,
    dir: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
) -> String {
    let asm_path = dir.join("test.s");
    let obj_path = dir.join("test.o");
    let bin_path = dir.join("test");

    fs::write(&asm_path, user_asm).unwrap();

    let mut as_cmd = Command::new(assembler_cmd());
    if target().platform == Platform::MacOS {
        as_cmd.args(["-arch", target().darwin_arch_name()]);
    }
    as_cmd.arg("-o").arg(&obj_path).arg(&asm_path);
    let as_status = as_cmd.status().expect("failed to run assembler");
    assert!(as_status.success(), "assembler failed");

    link_binary(
        &obj_path,
        runtime_obj,
        &bin_path,
        extra_link_libs,
        extra_link_paths,
        extra_frameworks,
    );

    let output = run_binary(&bin_path, dir);
    assert!(!output.status.success(), "binary unexpectedly succeeded");

    String::from_utf8(output.stderr).unwrap()
}
