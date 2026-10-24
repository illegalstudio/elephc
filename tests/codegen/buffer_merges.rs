//! Purpose:
//! Integration and regression tests for ternary, `?:`, `match` and `??` merges whose arms are
//! buffers, and for the truthiness of a buffer handle.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Regression for task 7c3f5df2: a ternary or `match` of two `buffer<T>` values used to merge
//!   into a boxed `mixed` temp, so an element read through it printed nothing, a write through a
//!   local that held it was lost, and a typed return failed to compile. `?:` failed with
//!   "is_truthy for PHP type Buffer". The merge now keeps the buffer handle.
//! - Fixtures use `$argc` (1 when the test binary runs without arguments) so the merges survive
//!   constant folding.

use crate::support::*;

/// Reads and writes through a ternary, `match` and `?:` of two buffers of one element type, read
/// directly, through a local, inside a literal, and through an element read of an array of
/// buffers, all reach the chosen buffer.
#[test]
fn test_buffer_merges_read_and_write_the_chosen_buffer() {
    let out = compile_and_run(
        r#"<?php
$a = buffer_new<float>(2);
$b = buffer_new<float>(2);
$a[0] = 1.5;
$a[1] = 2.5;
$b[0] = 3.5;
$b[1] = 4.5;
echo ($argc > 0 ? $a : $b)[0], "|", ($argc > 5 ? $a : $b)[1], "|";
$list = [($argc > 0 ? $a : $b)[1]];
echo $list[0], "|";
$cur = $argc > 0 ? $a : $b;
$cur[1] = 9.5;
echo $a[1], "|";
$picked = match ($argc) { 1 => $b, default => $a };
$picked[0] = 7.25;
echo $b[0], "|", (match ($argc) { 1 => $b, default => $a })[0], "|";
$short = $a ?: $b;
echo $short[0], "|", ($b ?: $a)[1], "\n";
$narrow = buffer_new<i32>(1);
$other = buffer_new<i32>(1);
$narrow[0] = 70000;
echo ($argc > 0 ? $narrow : $other)[0] + 1, "|";
$streams = [$a, $b];
echo ($argc > 0 ? $streams[1] : $a)[1], "|";
$wrapped = [$streams[0]];
echo $wrapped[0][0], "\n";
"#,
    );
    assert_eq!(out, "1.5|4.5|2.5|9.5|7.25|7.25|1.5|4.5\n70001|4.5|1.5\n");
}

/// A buffer in a condition tests its handle: a live buffer is true, while the invalid handle 0
/// of a missed array read and a freed local are false, for `if`, `!`, `(bool)`, a ternary
/// condition and `?:`.
#[test]
fn test_buffer_truthiness_tests_the_handle() {
    let out = compile_and_run(
        r#"<?php
$a = buffer_new<int>(1);
$a[0] = 5;
$list = [$a];
$miss = @$list[3];
echo $a ? "live" : "dead", "|", $miss ? "live" : "dead", "|", !$miss ? "not" : "is", "|";
echo (bool) $a ? "t" : "f", "|", ($miss ?: $a)[0], "|", (@$list[7] ?: $a)[0], "|";
if ($a) {
    echo "if|";
}
buffer_free($a);
echo $a ? "live" : "freed", "\n";
"#,
    );
    assert_eq!(out, "live|dead|not|t|5|5|if|freed\n");
}

/// A `throw` arm beside buffer arms keeps the merge a buffer: `match` with a throwing default,
/// a ternary, `??`, `??=` expressions on an element and on a literal dynamic property name
/// (whose write is skipped once the throw arm ends its branch), and `?:` hand back the buffer
/// when the throw is not taken and raise the exception when it is.
#[test]
fn test_buffer_merges_with_throw_arms() {
    let out = compile_and_run(
        r#"<?php
function pick(int $k, buffer<int> $a, buffer<int> $b): buffer<int> {
    return match ($k) {
        0 => $a,
        1 => $b,
        default => throw new InvalidArgumentException("no buffer " . $k),
    };
}
function either(bool $first, buffer<int> $a, buffer<int> $b): buffer<int> {
    return $first ? $a : throw new RuntimeException("not first");
}
final class Holder { public buffer<int> $buf; }
$a = buffer_new<int>(1);
$b = buffer_new<int>(1);
$a[0] = 3;
$b[0] = 4;
echo pick(0, $a, $b)[0], pick(1, $a, $b)[0], "|", either(true, $a, $b)[0], "|";
try {
    pick(2, $a, $b);
} catch (InvalidArgumentException $e) {
    echo $e->getMessage(), "|";
}
try {
    either(false, $a, $b);
} catch (RuntimeException $e) {
    echo $e->getMessage(), "|";
}
$list = [$a];
$got = $list[0] ?? throw new RuntimeException("missing");
echo $got[0], "|";
$kept = ($list[0] ??= throw new RuntimeException("unset slot"));
echo $kept[0], "|";
$holder = new Holder();
$holder->buf = $b;
$viaName = ($holder->{'buf'} ??= throw new RuntimeException("no buf"));
echo $viaName[0], "|";
try {
    $none = @$list[5] ?: throw new RuntimeException("empty slot");
    echo $none[0];
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(out, "34|3|no buffer 2|not first|3|3|4|empty slot\n");
}

/// Double-buffered state held in properties and chosen by methods with a ternary and a `match`:
/// packed fields read and written through the chosen buffer, and through a pick indexed
/// directly, reach that buffer. Runs the documented example.
#[test]
fn test_buffer_double_buffering_example() {
    let out = compile_and_run(include_str!(
        "../../examples/buffer-double-buffering/main.php"
    ));
    assert_eq!(
        out,
        "frame 5: 2.5 15 27.5\npaused body 0: vx=0\nfallback layer x0=2.5\n"
    );
}

/// Buffers of packed records chosen by a ternary on property reads: field reads through the
/// merged value, a field write through it, and a typed method return of the merge.
#[test]
fn test_packed_buffer_ternary_field_access() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public float $x; public float $y; }
final class Pair {
    public buffer<Vec2> $left;
    public buffer<Vec2> $right;
    public function __construct() {
        $this->left = buffer_new<Vec2>(1);
        $this->right = buffer_new<Vec2>(1);
    }
    public function side(bool $left): buffer<Vec2> {
        return $left ? $this->left : $this->right;
    }
}
$pair = new Pair();
$pair->left[0]->x = 1.25;
$pair->right[0]->x = 2.75;
echo ($argc > 0 ? $pair->left : $pair->right)[0]->x, "|";
($argc > 5 ? $pair->left : $pair->right)[0]->y = 8.5;
echo $pair->right[0]->y, "|", $pair->side(false)[0]->y, "|", $pair->side(true)[0]->x, "\n";
"#,
    );
    assert_eq!(out, "1.25|8.5|8.5|1.25\n");
}
