//! Purpose:
//! End-to-end tests for dumps and conversions that produce far more than the 64 KiB `_concat_buf`
//! scratch holds: `print_r`, `var_dump` and `implode` over 50 000-element arrays, and a `print_r`
//! return value above 1 MB.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext` through Rust's test harness.
//!
//! Key details:
//! - Every integer key and value is converted by `__rt_itoa`, which hands out a 21-byte window of
//!   `_concat_buf` per call. The walkers never rewind `_concat_off`, so a long walk used to run
//!   off the end of the buffer into the neighbouring runtime globals and crash at exit (or spin
//!   forever); `__rt_itoa` now wraps its window to the buffer start instead.
//! - `print_r($x, true)` used to stop at 64 KiB. Its capture buffer now moves to a growing heap
//!   block, so the whole text comes back; the expectations are built in Rust from PHP's layout.
//! - A live object is kept in each program because the original crash surfaced while releasing
//!   object children at exit.

use crate::support::*;

/// Builds PHP's `print_r` text for a flat int array with the given keys and values.
fn print_r_text(entries: &[(i64, i64)]) -> String {
    let mut out = String::from("Array\n(\n");
    for (key, value) in entries {
        out.push_str(&format!("    [{key}] => {value}\n"));
    }
    out.push_str(")\n");
    out
}

/// Builds PHP's `var_dump` text for a flat int array with the given keys and values.
fn var_dump_text(entries: &[(i64, i64)]) -> String {
    let mut out = format!("array({}) {{\n", entries.len());
    for (key, value) in entries {
        out.push_str(&format!("  [{key}]=>\n  int({value})\n"));
    }
    out.push_str("}\n");
    out
}

/// The list `range(1, n)` as key/value pairs.
fn list_entries(n: i64) -> Vec<(i64, i64)> {
    (0..n).map(|i| (i, i + 1)).collect()
}

/// The int => int hash `[$i * 3 => $i]` for `$i < n` as key/value pairs.
fn hash_entries(n: i64) -> Vec<(i64, i64)> {
    (0..n).map(|i| (i * 3, i)).collect()
}

/// Wraps a statement list with a live object, the way the crashing programs looked.
fn program(body: &str) -> String {
    format!("<?php\nclass K {{ public $a = 1; public $b = [1, 2, 3]; }}\n$k = new K();\n$o = new stdClass();\n$o->x = new K();\n$o->list = [1, 2, 3];\n{body}\necho \"END\\n\";\n")
}

/// `print_r` and `var_dump` of `range(1, 50000)` walk 50 000 integer keys and values without leaving `_concat_buf`.
#[test]
fn test_large_list_print_r_and_var_dump_exit_cleanly() {
    let out = compile_and_run_capture(&program("print_r(range(1, 50000));\nvar_dump(range(1, 50000));"));
    assert!(out.success, "{}", out.stderr);
    let entries = list_entries(50000);
    let expected = format!("{}{}END\n", print_r_text(&entries), var_dump_text(&entries));
    assert!(out.stdout == expected, "stdout differs: {} vs {} bytes", out.stdout.len(), expected.len());
}

/// The same walks over a 20 000-entry int => int hash, which takes the hash walkers instead of the packed ones.
#[test]
fn test_large_int_hash_print_r_and_var_dump_exit_cleanly() {
    let out = compile_and_run_capture(&program(
        "$h = [];\nfor ($i = 0; $i < 20000; $i++) { $h[$i * 3] = $i; }\nprint_r($h);\nvar_dump($h);",
    ));
    assert!(out.success, "{}", out.stderr);
    let entries = hash_entries(20000);
    let expected = format!("{}{}END\n", print_r_text(&entries), var_dump_text(&entries));
    assert!(out.stdout == expected, "stdout differs: {} vs {} bytes", out.stdout.len(), expected.len());
}

/// `implode` over 50 000 integers converts each one through the scratch buffer and returns the full text.
#[test]
fn test_large_implode_of_ints_returns_everything() {
    let out = compile_and_run_capture(&program("$s = implode(',', range(1, 50000));\necho strlen($s), ' ', substr($s, 0, 12), ' ', substr($s, -12), \"\\n\";"));
    assert!(out.success, "{}", out.stderr);
    let joined: String = (1..=50000).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let expected = format!("{} {} {}\nEND\n", joined.len(), &joined[..12], &joined[joined.len() - 12..]);
    assert_eq!(out.stdout, expected);
}

/// A `print_r($x, true)` result above 1 MB comes back in full, byte for byte.
#[test]
fn test_print_r_return_mode_is_not_truncated_at_64_kib() {
    let out = compile_and_run_capture(&program("$s = print_r(range(1, 70000), true);\necho $s;\necho strlen($s), \"\\n\";"));
    assert!(out.success, "{}", out.stderr);
    let text = print_r_text(&list_entries(70000));
    assert!(text.len() > 1_000_000);
    let expected = format!("{text}{}\nEND\n", text.len());
    assert!(out.stdout == expected, "stdout differs: {} vs {} bytes", out.stdout.len(), expected.len());
}

/// Small and mid-size captures keep working across the switch from the static buffer to the heap block, in a loop.
#[test]
fn test_print_r_return_mode_grows_repeatedly_and_stays_correct() {
    let out = compile_and_run_capture(&program(
        "$total = 0;\nfor ($n = 1; $n <= 6000; $n += 1500) {\n    $s = print_r(range(1, $n), true);\n    $total += strlen($s);\n}\necho $total, \"\\n\";\n$small = print_r([1, 2], true);\necho $small;",
    ));
    assert!(out.success, "{}", out.stderr);
    let mut total = 0usize;
    let mut n = 1;
    while n <= 6000 {
        total += print_r_text(&list_entries(n)).len();
        n += 1500;
    }
    assert_eq!(out.stdout, format!("{total}\nArray\n(\n    [0] => 1\n    [1] => 2\n)\nEND\n"));
}

/// A large `print_r` inside `ob_start()` lands in the output buffer whole, and a large return-mode capture frees its block (heap-debug clean).
#[test]
fn test_large_print_r_under_output_buffering_and_heap_debug_is_clean() {
    let source = program(
        "ob_start();\nprint_r(range(1, 30000));\n$buffered = ob_get_clean();\necho strlen($buffered), \"\\n\";\n$s = print_r(range(1, 30000), true);\necho strlen($s), ' ', $s === $buffered ? 'same' : 'different', \"\\n\";",
    );
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    let len = print_r_text(&list_entries(30000)).len();
    assert_eq!(output.stdout, format!("{len}\n{len} same\nEND\n"), "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// In a program with a run-time `__debugInfo()` class, `print_r` is deferred into the capture buffer: a 50 000-integer
/// walk grows it past 64 KiB and still prints everything.
#[test]
fn test_large_deferred_print_r_with_debug_info_class_prints_everything() {
    let out = compile_and_run_capture(
        "<?php\nclass H { public function __debugInfo() { return [\"n\" => count([1, 2])]; } }\n$h = new H();\nprint_r(range(1, 50000));\nprint_r($h);\necho \"END\\n\";\n",
    );
    assert!(out.success, "{}", out.stderr);
    let expected = format!("{}H Object\n(\n    [n] => 2\n)\nEND\n", print_r_text(&list_entries(50000)));
    assert!(out.stdout == expected, "stdout differs: {} vs {} bytes", out.stdout.len(), expected.len());
}
