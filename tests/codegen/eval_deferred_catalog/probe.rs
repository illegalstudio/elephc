//! Purpose:
//! Counts actual catalog publications while exercising the generated eval bridge.
//!
//! Called from:
//! - The focused deferred-catalog codegen integration suite.
//!
//! Key details:
//! - Test-only assembly wraps the publication ABI without changing its arguments or result.
//! - Float cells must postpone publication; string cells must request the full catalog.
//! - This detects drift between the Rust proof's tags and the emitted runtime wire tags.
//! - Mach-O common storage needs a relocatable symbol, rather than an assembler-local L label.

use crate::support::*;
use super::FixtureDir;

/// The call-site patch and emitted trampoline share one assembler-local text label.
const PUBLICATION_WRAPPER: &str = "Ldeferred_catalog_publish_probe";

/// Emits the actual instrumentation, shared by native execution and cross-assembly coverage.
fn publication_probe_assembly(target: Target) -> String {
    let real = target.extern_symbol("__elephc_eval_context_publish_native_catalog");
    let probe = target.extern_symbol("__elephc_test_catalog_publications");
    let wrapper = PUBLICATION_WRAPPER;
    // -- common storage must remain relocatable on Mach-O --
    let counter = target.extern_symbol("__elephc_test_catalog_publication_count");
    let code = if target.arch == Arch::AArch64 {
        format!(".text\n.globl {probe}\n{probe}:\n    adrp x16, {counter}@PAGE\n    add x16, x16, {counter}@PAGEOFF\n    ldr x0, [x16]\n    ret\n{wrapper}:\n    adrp x16, {counter}@PAGE\n    add x16, x16, {counter}@PAGEOFF\n    ldr x17, [x16]\n    add x17, x17, #1\n    str x17, [x16]\n    b {real}\n.comm {counter}, 8\n")
    } else {
        format!(".text\n.globl {probe}\n{probe}:\n    mov rax, QWORD PTR [rip + {counter}]\n    ret\n{wrapper}:\n    inc QWORD PTR [rip + {counter}]\n    jmp {real}\n.comm {counter}, 8\n")
    };
    target.transform_assembly(&code)
}

/// Instruments catalog publication without changing the real bridge call's ABI or lifetime.
fn run_publication_probe(source: &str) -> String {
    run_publication_probe_with_ir_opt(source, true)
}

/// Counts the same publication events with either optimizer mode.
fn run_publication_probe_with_ir_opt(source: &str, ir_opt: bool) -> String {
    let dir = FixtureDir(make_cli_test_dir("elephc_deferred_catalog_probe"));
    let (user, runtime, requirements) =
        compile_source_to_asm_with_options(source, &dir.0, 8 * 1024 * 1024, false, ir_opt);
    let target = target();
    let real = target.extern_symbol("__elephc_eval_context_publish_native_catalog");
    let mnemonic = if target.arch == Arch::AArch64 { "bl" } else { "call" };
    let needle = format!("    {mnemonic} {real}\n");
    assert!(user.contains(&needle), "catalog publication call must remain instrumentable");
    let mut user = user.replace(&needle, &format!("    {mnemonic} {PUBLICATION_WRAPPER}\n"));
    user.push_str(&publication_probe_assembly(target));
    let runtime_obj = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime_obj, &dir.0,
        &requirements, &default_link_paths(), &[]);
    output
}

/// The catalog stays absent through512 numeric evals, then publishes once before the next flush.
#[test]
fn test_deferred_native_catalog_budget_publication_boundary() {
    let source = r#"<?php
extern function __elephc_test_catalog_publications(): int;
function run(int $n) {
    global $code;
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { eval($code); }
    return $sum;
}
$code = str_repeat('$sum=$sum+3;', $argc);
echo run(511), ':', __elephc_test_catalog_publications(), '|';
echo run(512), ':', __elephc_test_catalog_publications(), '|';
echo run(513), ':', __elephc_test_catalog_publications(), '|';
echo run(1), ':', __elephc_test_catalog_publications();
"#;
    for ir_opt in [false, true] {
        assert_eq!(run_publication_probe_with_ir_opt(source, ir_opt), "1533:0|1536:0|1539:1|3:1");
    }
}

/// The same counter definitions and addressing expressions assemble on all five targets.
#[test]
fn test_deferred_native_catalog_probe_assembles_all_targets() {
    if Command::new("clang").arg("--version").output().is_err() { return; }
    let dir = FixtureDir(make_cli_test_dir("elephc_deferred_catalog_probe_asm"));
    for (name, triple) in [
        ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
        ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
        ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
    ] {
        let target = Target::parse(name).unwrap();
        let path = dir.0.join(format!("{name}.s"));
        let prefix = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
        fs::write(&path, format!("{prefix}{}", publication_probe_assembly(target))).unwrap();
        let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&path)
            .arg("-o").arg(path.with_extension("o")).output().unwrap();
        assert!(result.status.success(), "{name}: {}", String::from_utf8_lossy(&result.stderr));
    }
}

/// Verifies both runtime tag gates and late publication with a target-native ABI counter.
#[test]
fn test_deferred_native_catalog_float_then_string_publication_count() {
    let source = r#"<?php
extern function __elephc_test_catalog_publications(): int;
function run($initial) {
    global $code;
    $sum = $initial;
    eval($code);
    return $sum;
}
$code = str_repeat('$sum=$sum+3;', $argc);
echo run(1.5), ':', __elephc_test_catalog_publications(), '|';
echo run('1.5'), ':', __elephc_test_catalog_publications();
"#;
    assert_eq!(run_publication_probe(source), "4.5:0|4.5:1");
}

/// A hash-mask collision can retain an unused object until the next scope flush releases it.
#[test]
fn test_deferred_native_catalog_initializes_before_scope_flush_destructor() {
    assert_eq!(elephc_magician::eval_ir::eval_scope_name_mask("sum"),
        elephc_magician::eval_ir::eval_scope_name_mask("held73"));
    let source = r#"<?php
extern function __elephc_test_catalog_publications(): int;
class Victim {
    public function __destruct() { echo 'd', __elephc_test_catalog_publications(); }
}
function run() {
    global $code;
    $held73 = new Victim();
    $sum = 0;
    eval($code);
    unset($held73);
    eval($code);
    echo ':', __elephc_test_catalog_publications();
}
$code = str_repeat('$sum=$sum+3;', $argc);
run();
"#;
    assert_eq!(run_publication_probe(source), "d1:1");
}
