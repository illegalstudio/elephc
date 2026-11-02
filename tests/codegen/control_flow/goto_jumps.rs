//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of `goto`, covering both
//! supported shapes: the backward "restart" jump that desugars to a loop, and the forward jump
//! to a label whose tail always returns.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Every expectation below was taken from reference PHP running the same fixture.

use super::*;

/// Verifies the restart idiom Symfony's `ControllerAttributesListener::beforeController()` uses:
/// a label at the top of a `foreach` that is the last statement of a `void` method, jumped back to
/// from inside that same `foreach`. The tail neither returns nor throws, so the terminal-tail
/// rewrite cannot apply; the jump is a loop and is desugared as one.
#[test]
fn test_goto_restarts_the_labelled_tail() {
    let out = compile_and_run(
        r#"<?php
function run(array $items): void {
    $budget = 3;
    restart:
    foreach ($items as $item) {
        echo $item, "|";
        if ($item === 'b' && --$budget >= 0) {
            $items = ['x', 'y'];
            goto restart;
        }
    }
}
run(['a', 'b', 'c']);
echo "end";
"#,
    );
    assert_eq!(out, "a|b|x|y|end");
}

/// Verifies the `continue` level a restart jump needs is counted from the loops it actually sits
/// inside: two `foreach` levels below the label means the synthetic loop is level three.
#[test]
fn test_goto_restart_from_two_loops_deep() {
    let out = compile_and_run(
        r#"<?php
function deep(): void {
    $round = 0;
    top:
    foreach ([1, 2] as $a) {
        foreach ([3, 4] as $b) {
            echo "$a$b|";
            if ($a === 2 && $b === 4 && ++$round < 2) {
                goto top;
            }
        }
    }
}
deep();
echo "end";
"#,
    );
    assert_eq!(out, "13|14|23|24|13|14|23|24|end");
}

/// Verifies a `continue 2` already present in the tail keeps targeting the loop it named. It
/// reaches past the label's own statement list, so it has to count the synthetic loop inserted
/// between them — the renumbering is what makes it still land on the outer `foreach`.
#[test]
fn test_goto_restart_preserves_outer_continue_levels() {
    let out = compile_and_run(
        r#"<?php
function outerbreak(): void {
    $n = 0;
    foreach (['p', 'q'] as $outer) {
        echo "[$outer]";
        again:
        foreach ([1, 2, 3] as $i) {
            if ($i === 2 && ++$n === 1) {
                goto again;
            }
            if ($i === 3) {
                continue 2;
            }
            echo "$outer$i|";
        }
        echo "UNREACHABLE";
    }
}
outerbreak();
echo "end";
"#,
    );
    assert_eq!(out, "[p]p1|p1|p2|[q]q1|q2|end");
}

/// Verifies a `break` sitting at the label's own level still leaves the enclosing `while` rather
/// than only the synthetic loop, and that the label may live inside an `if` body.
#[test]
fn test_goto_restart_inside_if_body_keeps_outer_break() {
    let out = compile_and_run(
        r#"<?php
function inif(bool $flag): void {
    $k = 0;
    while (true) {
        if ($flag) {
            spin:
            ++$k;
            echo "$k|";
            if ($k < 3) {
                goto spin;
            }
            break;
        }
        break;
    }
    echo "k=$k|";
}
inif(true);
inif(false);
echo "end";
"#,
    );
    assert_eq!(out, "1|2|3|k=3|k=0|end");
}

/// Verifies `switch` is counted in the level arithmetic, exactly as PHP counts it for
/// `break N` / `continue N`.
#[test]
fn test_goto_restart_counts_switch_as_a_level() {
    let out = compile_and_run(
        r#"<?php
function withswitch(): void {
    $t = 0;
    start:
    foreach ([1, 2] as $v) {
        switch ($v) {
            case 2:
                if (++$t < 2) {
                    goto start;
                }
                echo "t=$t|";
                break;
            default:
                echo "case$v|";
        }
    }
}
withswitch();
echo "end";
"#,
    );
    assert_eq!(out, "case1|case1|t=2|end");
}

/// Verifies a `return` inside a restarted tail leaves the function, not just the synthetic loop.
#[test]
fn test_goto_restart_tail_return_leaves_the_function() {
    let out = compile_and_run(
        r#"<?php
function earlyreturn(): string {
    $c = 0;
    loop:
    foreach ([1, 2, 3] as $i) {
        if ($i === 3) {
            return "returned c=$c";
        }
        if ($i === 2 && ++$c < 2) {
            goto loop;
        }
    }
    return "fellthrough";
}
echo earlyreturn();
"#,
    );
    assert_eq!(out, "returned c=2");
}

/// Verifies the forward jump to a label whose tail always returns still resolves by cloning that
/// tail at the jump site — the shape that was supported before the restart form existed.
#[test]
fn test_goto_forward_jump_to_terminal_tail() {
    let out = compile_and_run(
        r#"<?php
function forwardjump(int $x): string {
    if ($x > 0) {
        goto fallback;
    }
    return "positive-path";
    fallback:
    return "fallback $x";
}
echo forwardjump(5), "|", forwardjump(-1);
"#,
    );
    assert_eq!(out, "fallback 5|positive-path");
}
