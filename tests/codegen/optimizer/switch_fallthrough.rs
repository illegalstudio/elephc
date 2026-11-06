//! Purpose:
//! Tests switch fallthrough, label side effects and exit modeling in AST optimization.
//!
//! Called from:
//! - The codegen optimizer integration suite through Rust's test harness.
//!
//! Key details:
//! - Covers constant propagation and control-flow pruning without changing fixture semantics.
//! - Nested breaks, default placement and post-switch reads preserve PHP execution order.

use crate::support::*;

/// A local initialized from a literal keeps what earlier code in the switch wrote: the body a
/// case falls into, a later label after an earlier label assigned it, a `default` written
/// between cases (falling into the next one) or first, a nested switch, and the read after the
/// switch. Constant propagation started every body from the value before the switch, so each of
/// these folded to the stale literal.
#[test]
fn test_switch_constant_propagation_follows_fallthrough_and_labels() {
    let out = compile_and_run(
        r#"<?php
function labels(int $n): string {
    $s = "a";
    switch ($n) {
        case ($s = "b") === "x": return "never";
        case ($s = "zz") ? 2 : 2: return $s;
        default: return "d" . $s;
    }
}
function mid_default(int $n): string {
    $k = 0;
    switch ($n) {
        case 1: $k = 5; break;
        default: $k = 7;
        case 2: $k += 1; break;
        case 3: $k = 100;
    }
    return (string) $k;
}
function default_first(int $n): string {
    $t = "t";
    switch ($n) {
        default: $t = "d";
        case 1: $t .= "1";
    }
    return $t;
}
function nested(int $n): int {
    $a = 1;
    switch ($n) {
        case 1:
            $a = 2;
            switch ($n + 1) {
                case 2: $a *= 10;
                case 3: $a += 1;
            }
        case 5:
            $a += 100;
            break;
    }
    return $a;
}
function after(int $n): int {
    $v = 3;
    switch ($n) {
        case 1: $v = 4;
        case 2: $v *= 2; break;
    }
    return $v;
}
function known(): int {
    $w = 1;
    switch (2) {
        case 1: $w = 10;
        case 2: $w += 5;
        case 3: $w += 7; break;
        case 4: $w = 99;
    }
    return $w;
}
echo labels(2), " ", labels(9), "|", mid_default(1), " ", mid_default(2), " ", mid_default(9), " ", mid_default(3), "|";
echo default_first(1), " ", default_first(9), "|", nested(1), " ", nested(5), "|", after(1), " ", after(2), " ", after(3), "|", known(), "\n";
"#,
    );
    assert_eq!(out, "zz dzz|5 1 8 100|t1 d1|121 101|8 6 3|13\n");
}

/// The switch exit models follow execution order and every way out of a body: a `default` written
/// before a case that falls off the end (nested, in a loop, and before code after the switch)
/// does not make the switch "always exit"; `continue` in a body leaves the switch like `break`;
/// and a `break` nested under an `if` keeps its target and the writes before it. Each of these
/// read an uninitialized value, hung, crashed, or folded a stale constant. Review follow-up for
/// #1631.
#[test]
fn test_switch_exit_models_follow_execution_order_and_nested_breaks() {
    let out = compile_and_run(
        r#"<?php
function nested_mid_default(int $n): int {
    $x = 0;
    switch ($n) {
        case 1:
            switch ($n + 1) {
                default: return 9;
                case 2: $x = 5;
            }
            break;
    }
    return $x;
}
function loop_mid_default(int $n): int {
    $x = 0;
    while (true) {
        if ($n === 0) { $x = 7; break; }
        switch ($n) {
            case 1: $x = 1;
            default: return 9;
            case 2: $x = 5;
        }
        break;
    }
    return $x;
}
function continue_in_switch(int $n): int {
    $x = 1;
    switch ($n) {
        case 0: $x = 2; continue 1;
        default: break;
    }
    return $x;
}
function nested_break(int $n, bool $c): int {
    $x = 1;
    switch ($n) {
        case 0:
            if ($c) { $x = 2; break; }
            return 5;
        default: break;
    }
    return $x;
}
function after_mid_default(int $n): int {
    $x = 1;
    switch ($n) {
        case 0: return 10;
        default: return 20;
        case 9: $y = $x;
    }
    $x = 2;
    return $x;
}
echo nested_mid_default(1), nested_mid_default(2), " ", loop_mid_default(0), loop_mid_default(2), " ";
echo continue_in_switch(0), " ", nested_break(0, true), nested_break(0, false), " ";
echo after_mid_default(9), after_mid_default(0), after_mid_default(5), "\n";
"#,
    );
    assert_eq!(out, "50 75 2 25 21020\n");
}

