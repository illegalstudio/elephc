//! Purpose:
//! Regression tests for issues #689 and #640: `implode()` reading an array whose ELEMENT
//! LAYOUT is only knowable at run time, because the operand arrived through a `?array`,
//! `mixed` or union slot — or because the element type has no dedicated renderer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A raw scalar element is 8 bytes and carries no length; the string slot the generic
//!   renderer used to assume is 16 bytes of `{pointer, length}`. Reading one as the other
//!   dereferenced an int's VALUE as a pointer, which is why these fixtures used to abort the
//!   test process rather than fail an assertion.
//! - Every expectation here is verbatim host PHP 8.5.10 output, including `-0`, the
//!   exponential float layout, and `true`/`false` joining as `"1"`/`""`.
//! - The array shapes are deliberately built two ways — as literals and by appending to an
//!   empty array — because the header tag they carry is written at different moments.

use crate::support::*;

/// Verifies every element type survives `implode()` through a declared nullable array.
///
/// This is issue #689's table. The declared `?array` is what makes the operand a boxed cell,
/// so the renderer has only the array header to go on: int and float elements were read as
/// `{pointer, length}` pairs and dereferenced (SIGSEGV), and bools produced empty output.
/// String elements always worked, which is what made the bug look type-specific.
#[test]
fn test_implode_on_a_nullable_array_renders_every_element_type() {
    let out = compile_and_run(
        r#"<?php
function f(?array $a): string { return implode(',', $a); }
echo f([1, 2]), "|";
echo f([1.5, 2.5]), "|";
echo f([true, false]), "|";
echo f(["a", "b"]), "|";
echo f([]), "|";
echo f([1, "a", 2.5]), "|";
echo f([-7, 0, 9223372036854775807]), "|";
echo f([0.1, -0.0, 1.0E+25, 1.0E-7]);
"#,
    );
    assert_eq!(
        out,
        "1,2|1.5,2.5|1,|a,b||1,a,2.5|-7,0,9223372036854775807|0.1,-0,1.0E+25,1.0E-7"
    );
}

/// Verifies the same through `mixed` and a union, and through a property and a local copy.
///
/// The issue's own repro was a property; copying it into a local does not help, because the
/// local inherits the declared type. `?? []` narrows the null away and was the documented
/// workaround — it must keep working beside the fixed path, not instead of it.
#[test]
fn test_implode_on_a_boxed_array_reached_through_a_property_or_union() {
    let out = compile_and_run(
        r#"<?php
class C { public ?array $x = null; }
function g(mixed $a): string { return implode('|', $a); }
function h(array|string $a): string { return implode('-', $a); }
$o = new C();
$o->x = [1, 2];
$t = $o->x;
echo implode(',', $o->x), ";";
echo implode(',', $t), ";";
echo implode(',', $o->x ?? []), ";";
echo g([1, 2, 3]), ";";
echo h([4.5, 5.5]), ";";
echo ImPlOdE(',', [6, 7]), ";";
echo JoIn([8, 9]);
"#,
    );
    assert_eq!(out, "1,2;1,2;1,2;1|2|3;4.5-5.5;6,7;89");
}

/// Verifies the layouts survive an `(array)` cast of a `mixed` RETURN value.
///
/// The other boxed shapes reach the renderer through a declared slot -- a nullable property,
/// a `mixed` parameter, a union. This one reaches it through a cast of a call result, which
/// is the shape that crashes hardest on `main`: `implode(",", (array) f())` over an int array
/// dereferences each element's VALUE as a pointer and takes the process down with SIGSEGV, so
/// there is no wrong output to notice first. Every layout is here because the cast keeps the
/// array's own `value_type` tag, which is the only thing the renderer can dispatch on.
#[test]
fn test_implode_on_an_array_cast_from_a_mixed_return_value() {
    let out = compile_and_run(
        r#"<?php
function ints(): mixed { return [1, 2]; }
function floats(): mixed { return [1.5, 2.5]; }
function bools(): mixed { return [true, false]; }
function strs(): mixed { return ["a", "b"]; }
function none(): mixed { return []; }
function one(): mixed { return [42]; }
function signed(): mixed { return [-7, 0, 7]; }
echo implode(",", (array) ints()), ";";
echo implode(",", (array) floats()), ";";
echo implode(",", (array) bools()), ";";
echo implode(",", (array) strs()), ";";
echo implode(",", (array) none()), ";";
echo implode(",", (array) one()), ";";
echo ImPlOdE(",", (array) signed()), ";";
echo JoIn((array) bools());
"#,
    );
    assert_eq!(out, "1,2;1.5,2.5;1,;a,b;;42;-7,0,7;1");
}

/// Verifies a `foreach` value bound from a nested container renders (issue #1081).
///
/// A loop binding taken out of a container is boxed the same way a nullable slot is, so
/// `implode()` over it hit the string-layout assumption and SEGFAULTED on int elements --
/// the crash reported for `array_chunk()`, which is only the most common way to produce
/// such a container. Reading the same chunk BY INDEX was fine, which is what made it look
/// like an `array_chunk()` bug rather than a renderer one.
///
/// Each shape lives in its own function on purpose: `array_chunk()`'s inner element type
/// merges across call sites in one scope, and a merged `Mixed` inner element is a separate
/// backend gap that would mask what this test is for.
#[test]
fn test_implode_on_a_foreach_binding_from_a_nested_container() {
    let out = compile_and_run(
        r#"<?php
function ints(): string { $o = ''; foreach (array_chunk([1, 2, 3, 4, 5], 2) as $p => $items) { $o .= $p . ":" . implode(",", $items) . ";"; } return $o; }
function keyed(): string { $o = ''; foreach (array_chunk([1, 2, 3, 4, 5], 2, true) as $p => $items) { $o .= $p . ":" . implode(",", $items) . ";"; } return $o; }
function floats(): string { $o = ''; foreach (array_chunk([1.5, 2.5, 3.5], 2) as $items) { $o .= implode(",", $items) . ";"; } return $o; }
function nested(): string { $o = ''; foreach ([[1, 2], [3, 4], [5]] as $items) { $o .= implode(",", $items) . ";"; } return $o; }
echo ints(), "|", keyed(), "|", floats(), "|", nested();
"#,
    );
    assert_eq!(
        out,
        "0:1,2;1:3,4;2:5;|0:1,2;1:3,4;2:5;|1.5,2.5;3.5;|1,2;3,4;5;"
    );
}

/// Verifies an array built by APPENDING renders like the literal of the same elements.
///
/// An array created empty is `array<never>` and its header says so; the append helper stamps
/// "scalar int" on the first write because a float and a bool share the int slot. The renderer
/// has nothing else to read, so a float array printed its bit patterns as integers and `false`
/// printed `0` instead of the empty string. A loop is what keeps the checker's view at
/// `array<never>` for every append, which is the case a straight-line fixture misses.
#[test]
fn test_implode_on_a_boxed_array_grown_by_appending() {
    let out = compile_and_run(
        r#"<?php
function f(?array $a): string { return implode(',', $a); }
$i = []; $f = []; $b = []; $s = [];
for ($n = 0; $n < 3; $n++) {
    $i[] = $n * 2;
    $f[] = $n / 8;
    $b[] = ($n % 2) === 0;
    $s[] = "v$n";
}
echo f($i), "|", f($f), "|", f($b), "|", f($s);
"#,
    );
    assert_eq!(out, "0,2,4|0,0.125,0.25|1,,1|v0,v1,v2");
}

/// Verifies a boxed ASSOCIATIVE array joins its values, as PHP does.
///
/// A hash has no dense payload for the renderers to walk. The statically typed case already
/// copied its values into an indexed array first; the boxed case did not, and read the hash
/// header words as elements.
#[test]
fn test_implode_on_a_boxed_associative_array_joins_its_values() {
    let out = compile_and_run(
        r#"<?php
function f(?array $a): string { return implode(',', $a); }
echo f(["x" => 1, "y" => 2]), "|";
echo f(["x" => "p", "y" => "q"]), "|";
echo f(["x" => 1.5, "y" => 2.5]), "|";
echo f(["x" => true, "y" => false]), "|";
echo f([0 => "u", 5 => "v", 9 => "w"]);
"#,
    );
    assert_eq!(out, "1,2|p,q|1.5,2.5|1,|u,v,w");
}

/// Verifies the one-argument `join()` form takes the same run-time dispatch.
///
/// The operand roles come from the argument count, so `join($array)` puts the array where the
/// separator normally sits — a different code path into the same renderer.
#[test]
fn test_join_single_argument_form_on_a_boxed_array() {
    let out = compile_and_run(
        r#"<?php
function f(?array $a): string { return join($a); }
echo f([1, 2, 3]), "|";
echo f([1.25, 2.25]), "|";
echo f([true, false, true]), "|";
echo f(["p", "q"]);
"#,
    );
    assert_eq!(out, "123|1.252.25|11|pq");
}

/// Verifies a long separator still lands between elements rather than over them.
///
/// The formatters write into the same shared buffer the join is filling, so each conversion
/// publishes the live destination cursor first. A separator longer than one byte is what makes
/// a stale cursor visible: the next conversion would overwrite the separator just copied.
#[test]
fn test_implode_on_a_boxed_array_with_a_long_separator() {
    let out = compile_and_run(
        r#"<?php
function f(?array $a): string { return implode('<=>', $a); }
echo f([10, 20, 30]), "|";
echo f([1.5, 2.5, 3.5]), "|";
echo f([true, false, true]);
"#,
    );
    assert_eq!(out, "10<=>20<=>30|1.5<=>2.5<=>3.5|1<=><=>1");
}

/// Verifies a homogeneous FLOAT array joins like PHP, whatever its declared type (issue #640).
///
/// Ints have a dedicated renderer and strings match the generic one; floats had neither, so a
/// statically typed `array<float>` was refused at lowering time and a boxed one printed raw
/// doubles. Both now go through the generic renderer's float arm.
#[test]
fn test_implode_on_a_homogeneous_float_array() {
    let out = compile_and_run(
        r#"<?php
$r = [1.5, 2.5];
echo implode(",", $r), "|";
echo implode(",", [1.5, 2.5, 3.5]), "|";
function s(array $a): string { return implode(":", $a); }
echo s([4.5, 5.5, 6.5]);
"#,
    );
    assert_eq!(out, "1.5,2.5|1.5,2.5,3.5|4.5:5.5:6.5");
}

/// Verifies a boxed value that is NOT an array raises PHP's own catchable `TypeError`.
///
/// It used to be read as an indexed array: null segfaulted and an object printed its header
/// words. php-src words null against the string-separator overload and every other type
/// against the parameter's declared `?array`, and names a boolean by its VALUE — all three
/// spellings are pinned here, because a wrong message is as misleading as no message.
#[test]
fn test_implode_on_a_boxed_non_array_raises_phps_type_error() {
    let out = compile_and_run(
        r#"<?php
class P {}
function g(mixed $a): string { return implode(',', $a); }
foreach ([1, 1.5, true, false, "s", new P(), null] as $bad) {
    try { echo g($bad), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
echo "still running";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "implode(): Argument #2 ($array) must be of type ?array, int given\n",
            "implode(): Argument #2 ($array) must be of type ?array, float given\n",
            "implode(): Argument #2 ($array) must be of type ?array, true given\n",
            "implode(): Argument #2 ($array) must be of type ?array, false given\n",
            "implode(): Argument #2 ($array) must be of type ?array, string given\n",
            "implode(): Argument #2 ($array) must be of type ?array, P given\n",
            "implode(): If argument #1 ($separator) is of type string, argument #2 ($array) \
must be of type array, null given\n",
            "still running",
        )
    );
}

/// Verifies a null `?array` throws instead of segfaulting, and that the throw is not optimized
/// away when its result is unused.
///
/// `implode()` is otherwise a pure call, so an unused one is eliminable — which would take the
/// diagnostic with it — and a `try` installs no handler around a call that cannot throw. The
/// throw summary is therefore decided per call site, from the operand's type.
#[test]
fn test_implode_on_a_null_nullable_array_throws_even_when_its_result_is_unused() {
    let out = compile_and_run(
        r#"<?php
function f(?array $a): string { return implode(',', $a); }
$hits = 0;
try { f(null); } catch (TypeError $e) { $hits++; }
try { $unused = f(null); } catch (TypeError $e) { $hits++; }
echo $hits;
"#,
    );
    assert_eq!(out, "2");
}
