//! Purpose:
//! Pins how a `finally` block that runs while an exception is PENDING settles that exception:
//! chained under any exception that escapes the block, rethrown when the block falls through,
//! discarded by a jump — and never leaked, never skipped.
//!
//! Called from:
//! - `cargo test --test finally_pending_exception_tests` through Rust's test harness.
//!
//! Key details:
//! - The two places an exception is pending while `finally` code runs are the `finally` after no
//!   `catch` matched, and the `finally` after a `catch` body itself threw. Both are lowered by
//!   `ir_lower::stmt::exceptions::lower_finally_with_pending_exception`.
//! - Every expected output was MEASURED on reference PHP 8.5.10 with the same program; every run
//!   is `--heap-debug` and must end clean.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "{}_{}_{:?}_{}",
        prefix,
        std::process::id(),
        std::thread::current().id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Returns the `live_blocks=` count of a `--heap-debug` binary's leak summary.
fn live_blocks(stderr: &str) -> usize {
    let summary = stderr
        .lines()
        .find(|line| line.contains("leak summary"))
        .unwrap_or_else(|| panic!("no leak summary in:\n{stderr}"));
    if summary.contains("clean") {
        return 0;
    }
    summary
        .split("live_blocks=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("no live_blocks in: {summary}"))
}

/// A throwing exception class that reports its own destruction, and a chain printer.
const PRELUDE: &str = r#"class X extends Exception { public function __destruct() { echo "[~", $this->getMessage(), "]"; } }
function chain(Throwable $t): string {
    $p = $t->getPrevious();
    return get_class($t) . ":" . $t->getMessage() . ($p === null ? "" : " <- " . chain($p));
}
function boom(string $m) { throw new X($m); }
"#;

/// Compiles `PRELUDE` + `body` with `--heap-debug`, runs it, and checks stdout and the heap.
fn assert_program(label: &str, body: &str, expected: &str) {
    let dir = make_test_dir(&format!("finally_pending_{label}"));
    fs::write(dir.join("main.php"), format!("<?php\n{PRELUDE}{body}")).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("--heap-debug")
        .arg(dir.join("main.php"))
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "{label}: compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(dir.join("main")).output().expect("failed to run binary");
    assert!(
        run.status.success(),
        "{label}: the binary died:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{label}");
    assert_eq!(live_blocks(&String::from_utf8_lossy(&run.stderr)), 0, "{label}: leaked");
}

/// An exception escaping a `finally` entered with one pending gets the pending one appended to
/// the END of its `previous` chain — whether the `finally` throws itself or a call inside it
/// does, in the same frame or across one, nested, and repeatedly.
///
/// elephc chained nothing and LEAKED the pending exception — on `main` too. MEASURED on
/// reference: `X:new <- X:orig`, the pending one destroyed after the new one.
#[test]
fn an_exception_escaping_a_finally_chains_the_pending_one() {
    for (label, body, expected) in [
        (
            "throw",
            "function f() { try { throw new X(\"orig\"); } finally { throw new X(\"new\"); } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "X:new <- X:orig\nend\n[~new][~orig]",
        ),
        (
            "call_throws",
            "function f() { try { throw new X(\"orig\"); } finally { boom(\"new\"); } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "X:new <- X:orig\nend\n[~new][~orig]",
        ),
        (
            "same_frame",
            "try { try { throw new X(\"orig\"); } finally { throw new X(\"new\"); } } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "X:new <- X:orig\nend\n[~new][~orig]",
        ),
        (
            "nested_finally",
            "function f() { try { try { throw new X(\"a\"); } finally { throw new X(\"b\"); } } finally { throw new X(\"c\"); } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "X:c <- X:b <- X:a\nend\n[~c][~b][~a]",
        ),
        (
            "repeated",
            "function f() { try { throw new X(\"orig\"); } finally { throw new X(\"new\"); } }\nfor ($i = 0; $i < 3; $i++) { try { f(); } catch (X $e) { echo chain($e), \" | \"; } }\necho \"end\\n\";\n",
            "X:new <- X:orig | [~new][~orig]X:new <- X:orig | [~new][~orig]X:new <- X:orig | end\n[~new][~orig]",
        ),
        (
            "finally_can_return",
            "function f($argc) { try { throw new X(\"orig\"); } finally { if ($argc > 100) { return 1; } throw new X(\"new\"); } }\ntry { f($argc); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "X:new <- X:orig\nend\n[~new][~orig]",
        ),
    ] {
        assert_program(label, body, expected);
    }
}

/// The pending exception goes at the END of an existing chain, and an exception the `finally`
/// catches itself is not chained at all.
///
/// MEASURED on reference: `X:new <- X:p <- X:orig`; and `inner` stays alone while `orig`
/// propagates unchanged.
#[test]
fn the_pending_exception_is_appended_at_the_tail_and_only_on_escape() {
    assert_program(
        "existing_chain",
        "function f() { try { throw new X(\"orig\"); } finally { throw new X(\"new\", 0, new X(\"p\")); } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
        "X:new <- X:p <- X:orig\nend\n[~new][~p][~orig]",
    );
    assert_program(
        "caught_inside",
        "function f() { try { throw new X(\"orig\"); } finally { try { throw new X(\"inner\"); } catch (X $i) { echo \"inner:\", chain($i), \" \"; } } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
        "inner:X:inner [~inner]X:orig\nend\n[~orig]",
    );
}

/// A `catch` body that throws still runs the `finally` — through a call too — and a `finally`
/// that then throws chains the catch's exception.
///
/// A throwing CALL inside a `catch` body skipped the `finally` entirely: only an explicit
/// `throw` reached it. MEASURED on reference: `F` before the propagated exception, and
/// `X:new <- X:fromcatch` when the `finally` throws too. `main` got both wrong.
#[test]
fn a_catch_body_that_throws_runs_the_finally_and_is_chained() {
    for (label, body, expected) in [
        (
            "call_throws",
            "function f() { try { throw new X(\"orig\"); } catch (X $c) { boom(\"fromcatch\"); } finally { echo \"F \"; } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "F [~orig]X:fromcatch\nend\n[~fromcatch]",
        ),
        (
            "throws",
            "function f() { try { throw new X(\"orig\"); } catch (X $c) { throw new X(\"fromcatch\"); } finally { echo \"F \"; } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "F [~orig]X:fromcatch\nend\n[~fromcatch]",
        ),
        (
            "finally_throws_too",
            "function f() { try { throw new X(\"orig\"); } catch (X $c) { throw new X(\"fromcatch\"); } finally { throw new X(\"new\"); } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
            "[~orig]X:new <- X:fromcatch\nend\n[~new][~fromcatch]",
        ),
    ] {
        assert_program(label, body, expected);
    }
}

/// A pending RETURN is not an exception: a `finally` that throws drops it and chains nothing,
/// and a `finally` that neither throws nor jumps lets the pending exception through unchanged.
///
/// MEASURED on reference: `X:new` alone; `F X:orig`.
#[test]
fn only_a_pending_exception_is_chained() {
    assert_program(
        "return_pending",
        "function f() { try { return 1; } finally { throw new X(\"new\"); } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
        "X:new\nend\n[~new]",
    );
    assert_program(
        "passes_through",
        "class Y extends Exception {}\nfunction f() { try { throw new X(\"orig\"); } catch (Y $y) {} finally { echo \"F \"; } }\ntry { f(); } catch (X $e) { echo chain($e), \"\\n\"; }\necho \"end\\n\";\n",
        "F X:orig\nend\n[~orig]",
    );
}

/// A `break`/`continue` that stays inside a protected region does NOT run its `finally` or
/// drop its handler — in a `try` body, a `catch` body, or a `finally` entered with an
/// exception pending.
///
/// Every such jump used to run the innermost `finally` whatever loop it targeted: `try { while
/// (…) { break; } throw … } catch …` ran the `finally` early (`F` printed twice), popped the
/// try's handler, and the later throw escaped its own `catch` as "Uncaught"; in a pending
/// `finally` the same `break` dropped the chaining handler (`new|none`). `main` had all of it.
/// MEASURED on reference PHP 8.5.10.
#[test]
fn a_jump_that_stays_inside_the_region_runs_no_finally() {
    let loop_ = "$i = 0; while (true) { $i++; if ($i > $n) { break; } echo \"i$i \"; }";
    for (label, body, expected) in [
        (
            "try_then_throw",
            format!("function f(int $n) {{ try {{ {loop_} throw new Exception(\"t\"); }} catch (Exception $c) {{ echo \"caught \", $c->getMessage(), \" \"; }} finally {{ echo \"F \"; }} echo \"tail\"; }}\nf($argc + 1); echo \"\\n\";\n"),
            "i1 i2 caught t F tail\n",
        ),
        (
            "try_falls_through",
            format!("function f(int $n) {{ try {{ {loop_} echo \"after \"; }} finally {{ echo \"F \"; }} echo \"tail\"; }}\nf($argc + 1); echo \"\\n\";\n"),
            "i1 i2 after F tail\n",
        ),
        (
            "foreach_in_try",
            "function f(int $n) { try { foreach ([1, 2, 3] as $v) { if ($v === $n) { break; } echo \"v$v \"; } throw new Exception(\"t\"); } catch (Exception $c) { echo \"caught \"; } finally { echo \"F \"; } echo \"tail\"; }\nf($argc + 1); echo \"\\n\";\n".to_string(),
            "v1 caught F tail\n",
        ),
        (
            "pending_finally",
            format!("function f(int $n) {{ try {{ throw new X(\"old\"); }} finally {{ {loop_} throw new X(\"new\"); }} }}\ntry {{ f($argc + 1); }} catch (X $e) {{ echo chain($e), \"\\n\"; }}\n"),
            "i1 i2 X:new <- X:old\n[~new][~old]",
        ),
        (
            "catch_body",
            format!("function f(int $n) {{ try {{ throw new X(\"a\"); }} catch (X $c) {{ {loop_} throw new X(\"b\"); }} finally {{ echo \"F \"; }} }}\ntry {{ f($argc + 1); }} catch (X $e) {{ echo chain($e), \"\\n\"; }}\n"),
            "i1 i2 F [~a]X:b\n[~b]",
        ),
    ] {
        assert_program(label, &body, expected);
    }
}

/// A jump that DOES leave a protected region still runs every `finally` it crosses, in order.
///
/// MEASURED on reference PHP 8.5.10.
#[test]
fn a_jump_that_leaves_the_region_runs_each_finally_it_crosses() {
    for (label, body, expected) in [
        (
            "break",
            "foreach ([1, 2] as $v) { try { echo \"t$v \"; break; } finally { echo \"F$v \"; } }\necho \"end\\n\";\n",
            "t1 F1 end\n",
        ),
        (
            "continue",
            "foreach ([1, 2] as $v) { try { echo \"t$v \"; continue; } finally { echo \"F$v \"; } }\necho \"end\\n\";\n",
            "t1 F1 t2 F2 end\n",
        ),
        (
            "break_2_across_two",
            "foreach ([1, 2] as $a) { try { foreach ([1, 2] as $b) { try { echo \"$a$b \"; break 2; } finally { echo \"in \"; } } } finally { echo \"out \"; } }\necho \"end\\n\";\n",
            "11 in out end\n",
        ),
        (
            "break_1_crosses_inner_only",
            "foreach ([1, 2] as $a) { try { foreach ([1, 2] as $b) { try { echo \"$a$b \"; break; } finally { echo \"in \"; } } echo \"mid \"; } finally { echo \"out \"; } }\necho \"end\\n\";\n",
            "11 in mid out 21 in mid out end\n",
        ),
        (
            "return",
            "function f() { foreach ([1] as $v) { try { return \"r\"; } finally { echo \"F \"; } } }\necho f(), \"\\nend\\n\";\n",
            "F r\nend\n",
        ),
    ] {
        assert_program(label, body, expected);
    }
}
