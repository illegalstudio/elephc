//! Purpose:
//! Exercises INI wire materialization, PHP value copies, and final lease retirement in the native runtime.
//!
//! Called from:
//! - Focused runtime-GC tests with a C fixture linked beside compiled PHP callers.
//!
//! Key details:
//! - Engine getters create real scalar and graph results; only the private fixture entry is replaced.
//! - Identity checks use the shared Rust registry after materialization has consumed wire ownership.
//! - Public INI wrappers and eval invocation are separate integration surfaces.

use super::*;
use std::process::Command;

/// Preserves scalar/array string identities through boxing, PHP aliases, casts, and final cleanup.
#[test]
fn test_mbstring_ini_materialize_native_ownership() {
    let source = r#"<?php
function ini_test_result(int $mode): mixed {
    mb_internal_encoding("UTF-8");
    if ($mode == 0) { return "fixture"; }
    if ($mode == 1) { return ["fixture" => 1]; }
    return false;
}
function ini_test_identity(mixed $value, int $index): bool { return is_string($value) && $index >= 0; }
function ini_test_retired(): bool { return mb_internal_encoding() == "UTF-8"; }
function ini_check_tree(mixed $value, int &$index): bool {
    if (is_string($value)) {
        $copy = (string)$value;
        $valid = ini_test_identity($copy, $index);
        $index++;
        return $valid;
    }
    if (is_array($value)) {
        foreach ($value as $child) { if (!ini_check_tree($child, $index)) { return false; } }
    }
    return true;
}
function run_ini(int $mode): void {
    $value = ini_test_result($mode);
    $copy = $value;
    unset($value);
    $index = 0;
    echo $mode, ":", ini_check_tree($copy, $index), ":", $index;
    if (is_string($copy)) { echo ":", bin2hex($copy); }
    if ($mode == 7) { echo ":", $copy; }
    unset($copy);
    echo ":", ini_test_retired(), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { for ($mode = 0; $mode < 8; $mode++) { run_ini($mode); } }
echo "done\n";
"#;
    let expected = concat!("0:1:1:7800ff:1\n", "1:1:1::1\n", "2:1:1:61:1\n",
        "3:1:4:1\n", "4:1:8:1\n", "5:1:MB_STRINGS:1\n", "6:1:3:1\n", "7:1:0:1:1\n");
    let directory = make_cli_test_dir("mbstring_ini_materialize");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let jump = if target().arch == Arch::AArch64 { "b" } else { "jmp" };
    let mut patched = assembly;
    for (name, native) in [("ini_test_result", "_ini_fixture_result"), ("ini_test_identity", "_ini_fixture_identity"),
        ("ini_test_retired", "_ini_fixture_retired")] {
        patched = replace_function(&patched, name, &format!("{jump} {native}\n"));
    }
    let provider = directory.join("provider.c");
    let provider_asm = directory.join("provider.s");
    std::fs::write(&provider, include_str!("ini_materialize/provider.c")).unwrap();
    let mut compiler = Command::new("cc");
    compiler.args(["-S", "-O2", "-Wall", "-Wextra", "-Werror"]);
    if target().arch == Arch::X86_64 { compiler.arg("-masm=intel"); }
    let built = compiler.arg(&provider).arg("-o").arg(&provider_asm).output().unwrap();
    assert!(built.status.success(), "{}: {}", directory.display(), String::from_utf8_lossy(&built.stderr));
    patched.push_str(&shims());
    patched.push_str(&std::fs::read_to_string(provider_asm).unwrap());
    std::fs::write(directory.join("caller.s"), &patched).unwrap();
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
    let state = elephc_mbstring::state::State::default();
    let strings = state.ini_get_all_strings(true).1.len();
    assert_eq!(output.stdout, expected.replace("MB_STRINGS", &strings.to_string()).repeat(8) + "done\n", "{}", directory.display());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
    std::fs::remove_dir_all(directory).unwrap();
}

/// Adapts private C fixture calls to the native result tuple and common consuming Mixed boxer.
fn shims() -> String {
    use elephc_builtin_contract::mbstring_abi::RESULT_STRING_ARRAY;
    let failed = format!(
        "{}ini_fixture_materialize_failed",
        target().platform.local_label_prefix()
    );
    if target().arch == Arch::AArch64 {
        format!("\n.text\n.globl _ini_fixture_materialize\n_ini_fixture_materialize:\n\
            stp x29, x30, [sp, #-16]!\nbl __rt_mbstring_materialize\n\
            cbnz x1, {failed}\nbl __rt_mbstring_box_result\n\
            ldp x29, x30, [sp], #16\nret\n{failed}:\n\
            mov x0, #0\nldp x29, x30, [sp], #16\nret\n\
            .globl _ini_fixture_restore\n_ini_fixture_restore:\nstp x29, x30, [sp, #-16]!\n\
            bl __rt_mbstring_restore_ini_array\ncbz x0, {failed}\n\
            mov x1, #0\nmov x2, #0\nmov x3, #{RESULT_STRING_ARRAY}\n\
            bl __rt_mbstring_box_result\nldp x29, x30, [sp], #16\nret\n")
    } else {
        format!("\n.text\n.globl _ini_fixture_materialize\n_ini_fixture_materialize:\n\
            sub rsp, 8\ncall __rt_mbstring_materialize\ntest rdx, rdx\n\
            jnz {failed}\ncall __rt_mbstring_box_result\n\
            add rsp, 8\nret\n{failed}:\nxor eax, eax\nadd rsp, 8\nret\n\
            .globl _ini_fixture_restore\n_ini_fixture_restore:\nsub rsp, 8\n\
            call __rt_mbstring_restore_ini_array\ntest rax, rax\njz {failed}\n\
            xor edx, edx\nxor ecx, ecx\nmov r8d, {RESULT_STRING_ARRAY}\n\
            call __rt_mbstring_box_result\nadd rsp, 8\nret\n")
    }
}
