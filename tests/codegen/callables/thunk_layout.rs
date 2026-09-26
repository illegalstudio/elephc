//! Purpose:
//! Regression tests for WHERE descriptor thunks land in the emitted text, relative to the
//! function that first needed them.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A descriptor invoker or PHP-ABI wrapper is discovered mid-body and used to be spliced into
//!   its caller's text behind a jump. Every later block of the caller then sat as far away as the
//!   thunk was long, and an AArch64 conditional branch only reaches ±1MB: the callable-argument
//!   normalizer of any program using `eval` carried 464 thunks between its dispatch and its arms,
//!   and `as` refused it with "fixup value out of range" on macos-aarch64.
//! - On Linux the same splice opened the thunk's own `.text.<name>` section mid-caller, so the
//!   caller's tail was filed under another symbol and `--debug-info` failed to assemble: "can't
//!   resolve .text._eir_clean_preg_replace_descriptor_callback_wrapper_6 - _fn_clean".
//! - The invariant below is checked on whichever target runs the test, so it holds the layout, not
//!   the assembler's tolerance on that target.

use crate::support::*;

/// Counts how many of `caller`'s own blocks follow the first thunk minted inside it.
///
/// A thunk minted in `caller` is named `_eir_<caller>_<prefix>_<id>`, and the caller's own blocks
/// are local labels `L_eir_<caller>_…` (`.L` on ELF). The thunk's OWN blocks are
/// `L_eir__eir_<caller>_…`, which does not contain the caller's prefix, so the count measures
/// exactly how much of the caller was pushed behind a thunk.
fn displaced_caller_blocks(source: &str, caller: &str, dir_name: &str) -> usize {
    let dir = make_cli_test_dir(dir_name);
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(dir);
    let thunk = user_asm
        .find(&format!(".globl _eir_{caller}_"))
        .unwrap_or_else(|| panic!("the fixture must mint a thunk inside {caller}:\n{user_asm}"));
    user_asm[thunk..].matches(&format!("L_eir_{caller}_")).count()
}

/// A thunk minted inside a function must not be followed by any of that function's own blocks.
///
/// `strlen(...)` is what mints the thunk: a first-class callable naming a builtin needs a PHP-ABI
/// wrapper, and it is needed in the middle of the body, before the `foreach` and the `if` whose
/// blocks follow it. A pristine `origin/main` displaces 304 of this caller's blocks. Restoring the
/// old splice — jump included — at the one site this goes through makes the test fail with 302
/// while the program still prints `12`, so it fails on the layout and not on a side effect.
#[test]
fn test_a_thunk_minted_mid_body_does_not_split_its_caller() {
    let source = r#"<?php
function layout_probe_caller(array $a, int $n): int {
    $len = strlen(...);
    $total = 0;
    foreach ($a as $s) { $total += $len($s); }
    if ($n > 0) { $total += 7; } else { $total += 9; }
    return $total;
}
echo layout_probe_caller(["ab", "cde"], $argc);
"#;
    assert_eq!(compile_and_run(source), "12");
    let displaced = displaced_caller_blocks(source, "layout_probe_caller", "elephc_thunk_layout");
    assert_eq!(
        displaced, 0,
        "{displaced} of the caller's own blocks follow a thunk minted inside it; the thunk was \
         spliced into its caller instead of being emitted out of line"
    );
}

/// The callback-wrapper sites go out of line too, not only the invokers.
///
/// `preg_replace_callback`, `array_map` with a descriptor callback, and a sort's direct-callback
/// adapter each emit a wrapper through `emit_callback_wrapper`. They were missed by the first
/// conversion because, unlike the invoker sites, they never re-opened the caller's section — which
/// also means they were already broken under `--debug-info` on Linux in `origin/main`. A reviewer
/// found them; a pristine `origin/main` displaces 8 of this caller's blocks.
#[test]
fn test_a_callback_wrapper_minted_mid_body_does_not_split_its_caller() {
    let source = r#"<?php
function clean(string $s): string {
    $out = preg_replace_callback('/a/', fn($m) => strtoupper($m[0]), $s);
    if (strlen($out) > 3) { $out .= "!"; } else { $out .= "?"; }
    return $out;
}
echo clean("banana");
"#;
    assert_eq!(compile_and_run(source), "bAnAnA!");
    let displaced = displaced_caller_blocks(source, "clean", "elephc_callback_wrapper_layout");
    assert_eq!(
        displaced, 0,
        "{displaced} of the caller's own blocks follow a callback wrapper minted inside it"
    );
}
