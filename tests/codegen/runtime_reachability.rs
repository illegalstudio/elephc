//! Purpose:
//! Regression coverage for feature-gated runtime and synthetic builtin reachability.
//!
//! Called from:
//! - `cargo test` through the codegen integration-test harness.
//!
//! Key details:
//! - Plain native programs must not carry the optional eval Reflection surface.
//! - A read whose path is a constant local string must not carry the URL reader.

use crate::support::{compile_source_to_asm_with_options, fs, make_cli_test_dir};

/// Verifies a program without eval or Reflection omits their synthetic methods and metadata.
#[test]
fn test_plain_program_omits_unreferenced_reflection_surface() {
    let dir = make_cli_test_dir("elephc_plain_runtime_reachability");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);

    assert!(
        !user_asm.contains("@fn name=Reflection"),
        "plain program unexpectedly lowered synthetic Reflection methods"
    );
    assert!(
        !user_asm.contains("_eval_reflection_"),
        "plain program unexpectedly emitted eval Reflection metadata"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|library| library == "elephc_magician"),
        "plain program unexpectedly requested the Magician bridge"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a constant local path does not drag the URL reader into the program.
///
/// PHP's wrapper grammar requires `scheme://`, so a literal without the separator cannot name a
/// wrapper and the URL multiplexer's tests provably cannot succeed. Entering at the phar level
/// instead takes the URL reader out of the call graph, and with it `socket`/`connect`/`bind` and
/// the resolver — measured as 11 distinct syscalls down to 7 on `file_get_contents("/etc/hosts")`.
///
/// The regression this guards is silent: routing the literal back through the multiplexer keeps
/// every behaviour test passing while quietly restoring the whole network stack to any program
/// that reads a constant path.
#[test]
fn test_constant_local_path_omits_the_url_reader() {
    let dir = make_cli_test_dir("elephc_constant_path_reachability");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        "<?php echo file_get_contents(\"/etc/hosts\");",
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        user_asm.contains("__rt_file_get_contents_maybe_phar"),
        "a constant local path should enter the read at the phar level"
    );
    assert!(
        !user_asm.contains("__rt_file_get_contents_maybe_url"),
        "a constant local path must not reach the URL multiplexer"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a path the compiler cannot see through still reaches the URL reader.
///
/// The specialisation must be narrow. A dynamic path can name any wrapper at run time, so
/// removing the multiplexer there would break `file_get_contents($url)` — the pair of tests is
/// what distinguishes "narrowed correctly" from "removed".
#[test]
fn test_dynamic_path_keeps_the_url_reader() {
    let dir = make_cli_test_dir("elephc_dynamic_path_reachability");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        "<?php $p = \"/etc/hosts\"; echo file_get_contents($p);",
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        user_asm.contains("__rt_file_get_contents_maybe_url"),
        "a dynamic path must still reach the URL multiplexer"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an executable exports its entry point and nothing else.
///
/// Every other global is `.globl` only so the user and runtime objects can find each other. On
/// Mach-O a `.globl` is an export, and an export is a dead-strip root, so leaving them unmarked
/// put the whole per-class machinery in the export trie. Marking them costs nothing at run time —
/// intra-image references are unaffected — and the regression is invisible without this check:
/// the program still runs, the binary is just larger.
#[test]
fn test_executable_marks_its_internal_symbols() {
    let dir = make_cli_test_dir("elephc_executable_visibility");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1;", &dir, 8_388_608, false, false);

    let directive = if cfg!(target_os = "macos") {
        ".private_extern "
    } else {
        ".hidden "
    };
    assert!(
        user_asm.contains(directive),
        "an executable should mark its internal globals as non-exported"
    );
    assert!(
        !user_asm.contains(&format!("{directive}_main\n")),
        "the entry point must stay exported"
    );

    let _ = fs::remove_dir_all(&dir);
}
