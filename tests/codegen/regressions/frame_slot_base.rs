//! Purpose:
//! Regression tests for the AArch64 frame-base register: a frame whose local slots reach past the
//! 255 bytes `ldur`/`stur` address below `x29` keeps `x28 = x29 - (frame_size - 16)`, so every
//! such slot is ONE `ldr`/`str [x28, #off]` instead of an address computation plus the access.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - The size test counts instructions, never time: the `sub xN, x29, #imm` address computations
//!   left in a large function must stay a handful (the frame-record publication, cells whose
//!   address escapes), where they were one per far slot access -- 15% of the compiled Symfony
//!   methods.
//! - The behaviour test runs the same large frame through a normal return, a caught exception
//!   (`setjmp`/`longjmp` restores x28 with the other callee-saved registers) and the exceptional
//!   cleanup callback, which borrows the abandoned frame's `x29` and must not use x28.

use crate::support::*;
use std::fs;

/// Compiles `source` with `--emit-asm` for `target` and returns the assembly text.
fn emit_asm(prefix: &str, source: &str, target: &str) -> String {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let output = elephc_cli_command(&dir)
        .arg("--target")
        .arg(target)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-asm");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read emitted assembly");
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// The body of one function, from its entry label to its epilogue label.
fn function_body<'a>(asm: &'a str, symbol: &str) -> &'a str {
    let start = asm.find(&format!("\n{symbol}:")).expect("function entry");
    let rest = &asm[start..];
    let end = rest.find(&format!("\n{symbol}_epilogue:")).unwrap_or(rest.len());
    &rest[..end]
}

/// A function with enough locals to push its frame well past 255 bytes, with a catch and an
/// owned object so it has both a `setjmp` landing and an exceptional cleanup callback.
const LARGE_FRAME: &str = r#"<?php
class Owned { public function __destruct() { echo "d;"; } }
function many_locals(int $seed): string {
    $o = new Owned();
    $a0 = $seed + 0; $a1 = $seed + 1; $a2 = $seed + 2; $a3 = $seed + 3; $a4 = $seed + 4;
    $a5 = $seed + 5; $a6 = $seed + 6; $a7 = $seed + 7; $a8 = $seed + 8; $a9 = $seed + 9;
    $s0 = "x$a0"; $s1 = "x$a1"; $s2 = "x$a2"; $s3 = "x$a3"; $s4 = "x$a4";
    $s5 = "x$a5"; $s6 = "x$a6"; $s7 = "x$a7"; $s8 = "x$a8"; $s9 = "x$a9";
    $l = [$s0, $s1, $s2, $s3, $s4, $s5, $s6, $s7, $s8, $s9];
    $m = ['k' => $a9, 'j' => $s9];
    try {
        if ($seed > 100) { throw new RuntimeException("caught"); }
    } catch (RuntimeException $e) {
        return $e->getMessage() . '|' . $s9;
    }
    if ($seed > 50) { throw new LogicException("escapes $s3"); }
    return implode(',', $l) . '|' . ($a0 + $a1 + $a2 + $a3 + $a4 + $a5 + $a6 + $a7 + $a8 + $a9) . '|' . $m['j'];
}
echo many_locals(1), "\n";
echo many_locals(200), "\n";
try { many_locals(60); } catch (LogicException $e) { echo $e->getMessage(), "\n"; }
echo "end\n";
"#;

/// Far local slots are addressed through the frame-base register in one instruction.
#[test]
fn test_far_local_slots_are_one_instruction_through_the_frame_base() {
    let asm = emit_asm("elephc_frame_base", LARGE_FRAME, "macos-aarch64");
    let body = function_body(&asm, "_fn_many_u_locals");
    let based = body.matches(", [x28, #").count();
    let computed = body
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("sub x") && line.contains(", x29, #")
        })
        .count();
    assert!(based > 100, "far slots must go through x28: {based} based accesses");
    assert!(
        computed <= 20,
        "far slot accesses still compute their address from x29: {computed} `sub xN, x29, #imm`"
    );
}

/// The large frame behaves like php on a normal return, a caught exception and an escaping one.
#[test]
fn test_large_frame_with_frame_base_matches_php() {
    let out = compile_and_run_with_heap_debug(LARGE_FRAME);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "d;x1,x2,x3,x4,x5,x6,x7,x8,x9,x10|55|x10\nd;caught|x209\nd;escapes x63\nend\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
