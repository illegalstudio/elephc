//! Purpose:
//! Interpreter tests for `goto`: jumps completed inside one statement list, jumps that leave
//! loops, `switch`es and `try` blocks, and jumps that land inside an `if` branch or a `try` body.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Every expected string was produced by running the same fragment under PHP 8.5.12.
//! - A jump travels as an `EvalControl::Goto` signal to the nearest enclosing statement list that
//!   holds its label, so these cases concentrate on what the signal passes on the way: loop
//!   executors, `finally` blocks, and function boundaries.

use super::super::*;
use super::support::*;

/// Runs one fragment on the fake runtime and returns what it echoed.
fn run(source: &[u8]) -> String {
    let program = parse_fragment(source).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    values.output
}

/// Verifies a backward jump repeats the statements after its label.
#[test]
fn execute_program_goto_backward_jump_repeats_code() {
    let output = run(
        br#"$i = 0; again: echo $i; $i = $i + 1; if ($i < 3) { goto again; } echo "|done";"#,
    );
    assert_eq!(output, "012|done");
}

/// Verifies a top-level function a forward jump skips is still declared, as PHP binds it when
/// it compiles the fragment.
#[test]
fn execute_program_goto_forward_jump_keeps_skipped_function_declared() {
    let output = run(br#"goto L; function f() { return "ok"; } L: echo f();"#);
    assert_eq!(output, "ok");
}

/// Verifies a backward jump across a top-level function declaration does not declare it twice.
#[test]
fn execute_program_goto_backward_jump_across_function_declaration() {
    let output = run(
        br#"$i = 0; again: function f() { return "f"; } echo f(); $i = $i + 1; if ($i < 2) { goto again; } echo "|done";"#,
    );
    assert_eq!(output, "ff|done");
}

/// Verifies a forward jump skips the statements before its label.
#[test]
fn execute_program_goto_forward_jump_skips_code() {
    let output = run(
        br#"echo "a"; goto end; echo "skipped"; end: echo "b";"#,
    );
    assert_eq!(output, "ab");
}

/// Verifies a jump out of two nested loops ends both and lands after them.
#[test]
fn execute_program_goto_jump_out_of_nested_loops() {
    let output = run(
        br#"foreach ([1, 2, 3] as $i) { foreach ([1, 2, 3] as $j) { if ($i * $j == 6) { goto found; } } } echo "none"; goto done; found: echo "found ", $i, "*", $j; done: echo ".";"#,
    );
    assert_eq!(output, "found 2*3.");
}

/// Verifies a jump leaves a switch nested in a while loop.
#[test]
fn execute_program_goto_jump_out_of_while_and_switch() {
    let output = run(
        br#"$n = 0; while (true) { $n = $n + 1; switch ($n) { case 3: goto out; default: echo $n; } } out: echo "|out";"#,
    );
    assert_eq!(output, "12|out");
}

/// Verifies a jump out of a try body runs its finally block first.
#[test]
fn execute_program_goto_jump_out_of_try_runs_finally() {
    let output = run(
        br#"try { echo "t "; goto out; } finally { echo "fin "; } echo "skipped "; out: echo "out";"#,
    );
    assert_eq!(output, "t fin out");
}

/// Verifies a jump whose label is in the same finally block stays inside it.
#[test]
fn execute_program_goto_jump_within_finally_stays_inside() {
    let output = run(
        br#"try { echo "body "; } finally { $i = 0; again: $i = $i + 1; if ($i < 3) { goto again; } echo "f", $i; }"#,
    );
    assert_eq!(output, "body f3");
}

/// Verifies a jump into an else branch does not evaluate the condition.
#[test]
fn execute_program_goto_jump_into_if_branch_skips_condition() {
    let output = run(
        br#"function side() { echo "side "; return true; } goto inside; if (side()) { echo "then "; } else { echo "skipped "; inside: echo "in-else "; } echo "after";"#,
    );
    assert_eq!(output, "in-else after");
}

/// Verifies a jump into a try body is covered by its catch and finally.
#[test]
fn execute_program_goto_jump_into_try_body_is_caught() {
    let output = run(
        br#"goto inside; try { echo "skipped "; inside: echo "in-try "; throw new Exception("x"); } catch (Exception $e) { echo "caught "; } finally { echo "fin"; }"#,
    );
    assert_eq!(output, "in-try caught fin");
}

/// Verifies a catch jumping to a label in its own try body does not run finally early.
#[test]
fn execute_program_goto_catch_jumping_back_into_try_runs_finally_once() {
    let output = run(
        br#"$n = 0; try { again: $n = $n + 1; echo "try", $n, " "; if ($n < 3) { throw new Exception("e"); } echo "ok "; } catch (Exception $e) { echo "catch", $n, " "; goto again; } finally { echo "fin"; }"#,
    );
    assert_eq!(output, "try1 catch1 try2 catch2 try3 ok fin");
}

/// Verifies the retry idiom: a catch jumps back to a label before its try.
#[test]
fn execute_program_goto_retry_from_catch() {
    let output = run(
        br#"$attempt = 0; retry: try { $attempt = $attempt + 1; if ($attempt < 3) { throw new Exception("fail"); } echo "ok after ", $attempt; } catch (Exception $e) { echo "retry "; goto retry; }"#,
    );
    assert_eq!(output, "retry retry ok after 3");
}

/// Verifies several labels reached in an order different from their source order.
#[test]
fn execute_program_goto_labels_reached_out_of_source_order() {
    let output = run(
        br#"goto first; third: echo "3"; goto done; second: echo "2"; goto third; first: echo "1"; goto second; done: echo "!";"#,
    );
    assert_eq!(output, "123!");
}

/// Verifies a function body and the fragment may reuse one label name.
#[test]
fn execute_program_goto_function_body_is_its_own_scope() {
    let output = run(
        br#"function count_to($n) { $s = ""; $i = 0; next: $i = $i + 1; $s = $s . $i; if ($i < $n) { goto next; } return $s; } $k = 0; next: $k = $k + 1; if ($k < 2) { goto next; } echo count_to(3), "|", $k;"#,
    );
    assert_eq!(output, "123|2");
}

/// Verifies a closure body is a label scope of its own.
#[test]
fn execute_program_goto_closure_body_is_its_own_scope() {
    let output = run(
        br#"$twice = function ($s) { $n = 0; $out = ""; more: $out = $out . $s; $n = $n + 1; if ($n < 2) { goto more; } return $out; }; echo $twice("ab");"#,
    );
    assert_eq!(output, "abab");
}

/// Verifies a label inside a loop body and the loop's own break and continue.
#[test]
fn execute_program_goto_label_inside_loop_body() {
    let output = run(
        br#"foreach ([0, 1, 2, 3, 4, 5] as $i) { $tries = 0; again: $tries = $tries + 1; if ($i == 1) { continue; } if ($i == 4) { break; } if ($tries < 2) { goto again; } echo $i, ":", $tries, " "; } echo "end";"#,
    );
    assert_eq!(output, "0:2 2:2 3:2 end");
}

/// Verifies a label no jump targets does nothing.
#[test]
fn execute_program_goto_unused_label_is_inert() {
    let output = run(
        br#"echo "a"; unused: echo "b";"#,
    );
    assert_eq!(output, "ab");
}

/// Verifies a fragment with a jump is declined by the bytecode slice and still runs, on the
/// tree walker.
#[test]
fn execute_program_goto_falls_back_from_the_bytecode_slice() {
    let program = parse_fragment(b"$i = 0; again: $i = $i + 1; if ($i < 3) { goto again; } return $i;")
        .expect("parse eval fragment");
    let gap = bytecode::compile_program(&program).expect_err("jumps are outside the slice");
    assert_eq!(gap.what(), "Label");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    assert_eq!(values.get(result), FakeValue::Int(3));
    assert!(!bytecode::last_execution_used_vm());
}
