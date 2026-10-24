//! Purpose:
//! Compiles small C fixtures into target assembly and appends them to generated PHP assembly.
//! Lets codegen tests call real C callees (dirty-register returns, native hosts) without a shared library.
//!
//! Called from:
//! - `tests/codegen/ffi/nullable_strings.rs`
//! - `tests/codegen/ffi/return_normalization.rs`
//! - `tests/codegen/ffi/struct_layout.rs`
//! - `tests/codegen/runtime_gc/mbstring_capture_hash/*`
//!
//! Key details:
//! - The C compiler emits the assembly dialect the generated PHP assembly uses (Intel syntax on x86_64).
//! - Linux targets compile with the target linker driver, so a cross-compiled AArch64 fixture matches the binary.
//! - Clang's file-wide `.subsections_via_symbols` directive is dropped so EIR local branch labels stay local.

use super::*;

/// Returns the C compiler used for test fixtures on the current test target.
///
/// Linux reuses the linker driver (`gcc` or the AArch64 cross `gcc`); macOS links with `ld`,
/// so it compiles with the system `cc` instead.
fn c_fixture_compiler() -> &'static str {
    match target().platform {
        Platform::Linux => gcc_cmd(),
        _ => "cc",
    }
}

/// Compiles `c_source` into assembly for the test target and returns that assembly.
///
/// The source is written to `<dir>/<stem>.c`. Warnings are errors so a broken fixture fails loudly.
pub(crate) fn compile_c_fixture_asm(
    dir: &Path,
    stem: &str,
    c_source: &str,
    defines: &[&str],
) -> String {
    let c_path = dir.join(format!("{stem}.c"));
    let asm_path = dir.join(format!("{stem}.s"));
    fs::write(&c_path, c_source).unwrap();
    let mut compiler = Command::new(c_fixture_compiler());
    compiler.args(["-S", "-O2", "-Wall", "-Wextra", "-Werror"]);
    if target().arch == Arch::X86_64 {
        compiler.arg("-masm=intel");
    }
    let built = compiler
        .args(defines)
        .arg(&c_path)
        .arg("-o")
        .arg(&asm_path)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}: {}",
        dir.display(),
        String::from_utf8_lossy(&built.stderr)
    );
    fs::read_to_string(&asm_path).unwrap()
}

/// Appends C fixture assembly without changing the atom boundaries of the preceding PHP assembly.
pub(crate) fn append_c_fixture_asm(assembly: &mut String, fixture_asm: &str) {
    for line in fixture_asm.lines() {
        // Clang emits this file-wide Mach-O directive at the end of its assembly. In a
        // combined file it makes EIR conditional branch targets separate symbols.
        if line.trim() != ".subsections_via_symbols" {
            assembly.push_str(line);
            assembly.push('\n');
        }
    }
}

/// Compiles PHP `source` together with a C fixture and returns the program's stdout.
///
/// The fixture's functions are assembled into the same object as the PHP program, so plain
/// `extern function` declarations resolve to them without a library or search path.
pub(crate) fn compile_and_run_with_c_fixture(source: &str, c_source: &str) -> String {
    let dir = make_cli_test_dir("elephc_test_c_fixture");
    let (mut assembly, runtime, libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let fixture_asm = compile_c_fixture_asm(&dir, "fixture", c_source, &[]);
    append_c_fixture_asm(&mut assembly, &fixture_asm);
    let output = assemble_and_run_capture(
        &assembly,
        &runtime_obj_for_asm(&runtime),
        &dir,
        &libraries,
        &default_link_paths(),
        &[],
    );
    let _ = fs::remove_dir_all(&dir);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    output.stdout
}
