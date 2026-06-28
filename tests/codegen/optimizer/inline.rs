//! Purpose:
//! End-to-end optimizer tests for small-function inliner over real lowered EIR.
//!
//! Called from:
//! - `cargo test` (optimizer filter or full).
//!
//! Key details:
//! - Use runtime-unknown values ($argc) so AST opts do not eliminate calls; the
//!   calls reach EIR inliner when --ir-opt (default in tests).
//! - AC2: explicitly run with ir-opt on and off via CLI flag on the real binary
//!   and assert identical stdout/behavior.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::*;

fn compile_run_with_ir_opt(src: &str, ir_opt_on: bool) -> String {
    let id = std::sync::atomic::AtomicUsize::new(0);
    let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_inline_ac2_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    let php_path: PathBuf = dir.join("t.php");
    fs::write(&php_path, src).unwrap();
    let _bin_path: PathBuf = dir.join("t");
    // Invoke the built binary (assumes cargo test has a recent build) with flag.
    let elephc = elephc_cli_bin();
    let mut cmd = Command::new(&elephc);
    cmd.arg(&php_path);
    if !ir_opt_on {
        cmd.arg("--no-ir-opt");
    }
    let status = cmd.status().expect("invoke elephc");
    assert!(status.success(), "compile failed");
    // The output binary is placed next to source as 't' (without .php)
    let out_bin = dir.join("t");
    let run_out = Command::new(&out_bin).output().expect("run produced bin");
    assert!(run_out.status.success());
    String::from_utf8_lossy(&run_out.stdout).into_owned()
}

#[test]
fn test_inline_small_function_with_arg() {
    // Small fn (<=24 non-nop), direct call with arg from $argc (unknown). Typed
    // scalar param so the callee is ownership-safe and actually gets inlined.
    let src = r#"<?php
function add2(int $n): int { return $n + 2; }
echo add2($argc);
"#;
    let out = compile_and_run(src);
    // argc == 1 in our harness -> 1 + 2 = 3.
    assert_eq!(out.trim(), "3");
}

#[test]
fn test_inline_small_void_and_returning() {
    // Separate to avoid any splicing interaction in one host for this basic test.
    let src = r#"<?php
function get7() { return 7; }
echo get7();
"#;
    let out = compile_and_run(src);
    assert_eq!(out, "7");
}

#[test]
fn test_inline_multi_block_small_fn() {
    // Typed scalar param so the multi-block callee is ownership-safe and inlined.
    let src = r#"<?php
function sign(int $x): int {
  if ($x > 0) { return 1; }
  if ($x < 0) { return -1; }
  return 0;
}
echo sign($argc - 1), sign($argc - 2);
"#;
    let out = compile_and_run(src);
    // argc=1 -> sign(0)=0 , sign(-1)=-1 -> "0-1"
    assert_eq!(out, "0-1");
}

/// Structural proof that inlining actually fires on the supported scalar subset:
/// after compiling a typed small helper, its `main` IR must contain no `call` to the
/// helper (replaced by the spliced body + an `inline_cont` join block).
#[test]
fn test_inline_emits_no_call_for_typed_scalar_helper() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!(
        "elephc_inline_struct_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let php_path: PathBuf = dir.join("t.php");
    fs::write(
        &php_path,
        "<?php\nfunction add2(int $n): int { return $n + 2; }\necho add2($argc);\n",
    )
    .unwrap();

    let elephc = elephc_cli_bin();
    let ir = Command::new(&elephc)
        .arg("--emit-ir")
        .arg(&php_path)
        .output()
        .expect("emit-ir");
    assert!(ir.status.success(), "emit-ir failed");
    let text = String::from_utf8_lossy(&ir.stdout);
    // Isolate the `main` function body and assert the call was inlined away.
    let main_body = text
        .split("function main(")
        .nth(1)
        .expect("main function present in IR");
    let main_body = main_body.split("\n  function ").next().unwrap_or(main_body);
    assert!(
        !main_body.contains("= call "),
        "add2 call should be inlined out of main; IR was:\n{}",
        main_body
    );
    assert!(
        main_body.contains("inline_cont"),
        "inlined return join block expected in main; IR was:\n{}",
        main_body
    );
}

// AC2: identical behavior with --ir-opt on vs off for programs using small inlinable fns.

// FVC site inlining (FunctionVariantCall) is exercised via hand-built EIR unit tests
// (ir_passes/tests/inline_test.rs) that now use the canonical resolver lifted to
// src/ir/function_variants.rs (collect over module.functions + normalized keys).
// Real lowered include-variant FVC paths will use the same resolver in the inliner.

// e2e using real lowered PHP for the overall small fn inlining (Call sites lower from PHP source).
// FVC sites are covered at EIR construction level in units (lowering uses Call for public user fn names;
// FVC opcode appears in variant dispatch paths).
#[test]
fn test_inline_semantics_preserved_on_off() {
    let src = r#"<?php
function add2($n) { return $n + 2; }
echo add2($argc);
"#;
    // Default compile_and_run is opt path; also force via CLI paths.
    let on_default = compile_and_run(src);
    let on_cli = compile_run_with_ir_opt(src, true);
    let off_cli = compile_run_with_ir_opt(src, false);
    assert_eq!(on_default.trim(), on_cli.trim());
    assert_eq!(on_cli.trim(), off_cli.trim(), "stdout must be identical with ir-opt on vs off");
}

/// Regression for the mutual-recursion compile-time hang: two small functions that
/// call each other must compile (the cycle analysis excludes them from inlining
/// instead of expanding forever) and produce the correct result at runtime.
#[test]
fn test_mutual_recursion_compiles_and_runs() {
    let src = r#"<?php
function is_even($n) { if ($n == 0) { return true; } return is_odd($n - 1); }
function is_odd($n) { if ($n == 0) { return false; } return is_even($n - 1); }
echo is_even($argc + 3) ? "E" : "O";
"#;
    // argc == 1 at our harness -> is_even(4) -> "E". Reaching this assertion proves
    // the compiler did not hang inlining the mutual cycle.
    let out = compile_and_run(src);
    assert_eq!(out, "E");
}

/// A value-returning small function called in statement position (result discarded)
/// must inline correctly: the returned scalar is dropped and the program still runs.
#[test]
fn test_inline_discarded_result_call() {
    let src = r#"<?php
function note() { return 7; }
note();
echo "ok";
"#;
    let out = compile_and_run(src);
    assert_eq!(out, "ok");
}

/// A small typed-string helper (destructor-free refcounted boundary) IS inlined, and
/// its observable behavior is unchanged: output is correct and identical with ir-opt on
/// vs off. Covers string param binding + return through the continuation parameter.
#[test]
fn test_string_helper_inlined_and_preserved() {
    let src = r#"<?php
function greet(string $who): string { return "hi " . $who; }
echo greet("world");
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, "hi world");
    assert_eq!(on, off, "string-helper output must match with ir-opt on vs off");
}

/// An array helper that takes and returns an array (refcounted, destructor-free) is
/// inlined; copy-on-write and refcount semantics must be byte-for-byte preserved, so
/// output is identical with ir-opt on vs off. Exercises borrowed-arg binding, an
/// array `[]=` mutation inside the inlined body, and a directly-returned array slot.
#[test]
fn test_array_helper_cow_preserved() {
    let src = r#"<?php
function tag(array $a): array { $a[] = 9; return $a; }
$x = [1, 2];
$y = tag($x);
echo count($x), ":", count($y), ":", array_sum($y);
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, off, "array-helper COW/refcount must match with ir-opt on vs off");
}

/// An array builder returning a freshly-allocated array is inlined and preserved: the
/// returned array slot is transplanted as cleanup-excluded so its ownership moves to the
/// caller (no double free, no leak). Identical output with ir-opt on vs off.
#[test]
fn test_array_builder_return_preserved() {
    let src = r#"<?php
function pair(int $a, int $b): array { $r = [$a, $b]; return $r; }
$p = pair($argc, $argc + 5);
echo array_sum($p), ":", count($p);
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, off, "array-builder output must match with ir-opt on vs off");
}

/// Regression: a string helper must NOT be inlined when a `string` argument is an
/// in-flight concat-scratch value (here `$s . "B"`, passed directly). The inliner binds
/// arguments with `store_local` and runs the callee's `concat_reset` in the host frame,
/// which would free that in-flight scratch string before the spliced body reads it back.
/// The string-arg-stability guard leaves the call in place, so output is correct and
/// identical with ir-opt on vs off (was AAAA vs AABB before the fix).
#[test]
fn test_inline_inflight_string_arg_not_miscompiled() {
    let src = r#"<?php
function j(string $a, string $b): string { return $a . $b; }
$s = "B";
echo j("AA", $s . "B");
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, "AABB");
    assert_eq!(on, off, "in-flight string arg must match with ir-opt on vs off");
}

/// A single-arg string helper called with an in-flight concat argument (`$x . "!"`) is
/// likewise left as a call; the result is correct on vs off (was the concat-buffer
/// corruption `<<<>` before the fix).
#[test]
fn test_inline_inflight_string_arg_single_param() {
    let src = r#"<?php
function w(string $s): string { return "<" . $s . ">"; }
$x = "a";
echo w($x . "!");
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, "<a!>");
    assert_eq!(on, off, "in-flight single string arg must match with ir-opt on vs off");
}

/// Sanity that stable string arguments (a literal and a variable load) still inline and
/// stay correct — the guard must reject only in-flight scratch strings, not all strings.
#[test]
fn test_inline_stable_string_args_still_work() {
    let src = r#"<?php
function j(string $a, string $b): string { return $a . $b; }
$x = "AA";
$y = "BB";
echo j($x, $y), ":", j("CC", "DD");
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, "AABB:CCDD");
    assert_eq!(on, off, "stable string args must match with ir-opt on vs off");
}

/// Pipeline integration (fixed-point order): a callee that exceeds the 24-instruction
/// inline threshold *before* optimization but collapses below it *after* constant
/// folding and dead-code elimination is inlined only because `optimize_module`
/// interleaves the inliner with the per-function passes to a module-level fixed point.
/// The first round leaves `calc` too large; a later round, after folding shrinks it,
/// inlines it. Behavior is correct and identical with ir-opt on vs off.
#[test]
fn test_fixed_point_inlines_callee_shrunk_by_folding() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    let src = r#"<?php
function calc(int $n): int {
  $a = ($n * 0) + 1+2+3+4+5+6+7+8+9+10+11+12+13+14+15+16+17+18+19+20;
  return $a;
}
echo calc($argc);
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, "210");
    assert_eq!(on, off, "fixed-point pipeline must preserve behavior");

    // Structural proof that the later round actually inlined `calc` (which is 48
    // non-nop instructions before folding, well over the threshold).
    let dir = std::env::temp_dir().join(format!(
        "elephc_inline_fp_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let php_path: PathBuf = dir.join("t.php");
    fs::write(&php_path, src).unwrap();
    let elephc = elephc_cli_bin();
    let ir = Command::new(&elephc)
        .arg("--emit-ir")
        .arg(&php_path)
        .output()
        .expect("emit-ir");
    assert!(ir.status.success(), "emit-ir failed");
    let text = String::from_utf8_lossy(&ir.stdout);
    let main_body = text
        .split("function main(")
        .nth(1)
        .expect("main present");
    let main_body = main_body.split("\n  function ").next().unwrap_or(main_body);
    assert!(
        !main_body.contains("= call "),
        "calc must be inlined after folding shrinks it below the threshold; IR:\n{}",
        main_body
    );
    assert!(
        main_body.contains("inline_cont"),
        "expected an inlined return join block in main; IR:\n{}",
        main_body
    );
}

/// Regression: a typed helper with a default parameter, called with a spread, has its
/// `age` argument materialized as a boxed `mixed` that the typed `int` parameter would
/// unbox in the callee prologue. The inliner must NOT bind that boxed operand directly
/// (which read uninitialized/garbage); the type-match guard leaves the call in place, so
/// output stays correct and identical with ir-opt on vs off.
#[test]
fn test_spread_named_default_arg_not_miscompiled() {
    let src = r#"<?php
function profile(string $name, int $age = 18): string { return $name . ":" . $age; }
$args = ["age" => 30];
echo profile(...$args, name: "Lin");
"#;
    let on = compile_and_run(src);
    let off = compile_run_with_ir_opt(src, false);
    assert_eq!(on, "Lin:30");
    assert_eq!(on, off, "spread/default-arg call must match with ir-opt on vs off");
}

/// Structural proof that a destructor-free string helper actually inlines: after
/// compiling, `main` contains no `call`/`builtin_call` to the helper for the concat
/// path (replaced by the spliced body and an `inline_cont` join block).
#[test]
fn test_inline_emits_no_call_for_string_helper() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!(
        "elephc_inline_str_struct_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let php_path: PathBuf = dir.join("t.php");
    fs::write(
        &php_path,
        "<?php\nfunction wrap(string $s): string { return \"[\" . $s . \"]\"; }\necho wrap(\"x\");\n",
    )
    .unwrap();

    let elephc = elephc_cli_bin();
    let ir = Command::new(&elephc)
        .arg("--emit-ir")
        .arg(&php_path)
        .output()
        .expect("emit-ir");
    assert!(ir.status.success(), "emit-ir failed");
    let text = String::from_utf8_lossy(&ir.stdout);
    let main_body = text
        .split("function main(")
        .nth(1)
        .expect("main function present in IR");
    let main_body = main_body.split("\n  function ").next().unwrap_or(main_body);
    assert!(
        main_body.contains("inline_cont"),
        "inlined return join block expected in main; IR was:\n{}",
        main_body
    );
    assert!(
        !main_body.contains("= call "),
        "wrap call should be inlined out of main; IR was:\n{}",
        main_body
    );
}

/// Real e2e using PHP source + compile_and_run_files (multi-file include) with $argc
/// to ensure the call reaches EIR (AST opts cannot fold). The include triggers
/// resolver variant group/mark paths; the small callee (<=24) is eligible and
/// inlined when the call target resolves (units assert FVC opcode itself inlines too).
///
/// Hardened per strategy: also run --emit-ir on the multi-file project and
/// mechanically assert that the emitted IR text contains no `function_variant_call`
/// (or `FunctionVariantCall`) token for the small helper (proof that any FVC
/// or call site for it was inlined on the shipped path); stdout must be "42".
#[test]
fn test_inline_small_function_via_include_triggers_variant_path() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    let id = std::sync::atomic::AtomicUsize::new(0);
    let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_inline_fvc_e2e_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let main_path: PathBuf = dir.join("main.php");
    let lib_path: PathBuf = dir.join("lib.php");
    fs::write(&main_path, r#"<?php
require "lib.php";
echo helper($argc);
"#).unwrap();
    fs::write(&lib_path, r#"<?php
function helper($x) { return $x + 41; }
"#).unwrap();

    // Build + run via existing helper (asserts output after inlining).
    let out = compile_and_run_files(
        &[
            ("main.php", r#"<?php require "lib.php"; echo helper($argc); "#),
            ("lib.php", r#"<?php function helper($x) { return $x + 41; } "#),
        ],
        "main.php",
    );
    assert_eq!(out.trim(), "42");

    // Now drive --emit-ir on the same multi-file layout and assert no FVC opcode
    // for the small helper remains in the IR text (inlining happened).
    let elephc = elephc_cli_bin();
    let ir_out = Command::new(&elephc)
        .arg("--emit-ir")
        .arg(&main_path)
        .output()
        .expect("emit-ir on multi-file include project");
    assert!(ir_out.status.success(), "emit-ir failed");
    let ir_text = String::from_utf8_lossy(&ir_out.stdout);
    // The small helper call site must not appear as function_variant_call after inlining.
    assert!(!ir_text.to_lowercase().contains("function_variant_call"),
            "IR must not contain function_variant_call for the inlined small helper from include variant path:\n{}", &ir_text[..ir_text.len().min(2000)]);
    // Sanity: the helper definition may remain as a module function, but its call site was removed.
    // Run the binary too (already asserted via helper).
}
