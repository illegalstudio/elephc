//! Purpose:
//! End-to-end tests that every local the compiler synthesizes for its own bookkeeping stays out
//! of the PHP-visible variable set, and that `$this` does too for the separate reason PHP gives.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` integration harness.
//!
//! Key details:
//! - Every expected value is the verbatim stdout of `LC_ALL=C php` 8.4 for the same fixture.
//! - `get_defined_vars()` is the observation point because it is the only PHP surface that
//!   enumerates the frame, so a hidden slot that reaches it is a user-visible wrong answer.
//! - Each fixture takes the variable list ONCE, into `$vars`, before minting any further locals.
//!   PHP defines a variable only after its assignment completes, so `$vars` itself is absent from
//!   its own snapshot and the expectation stays a short, exact list rather than a moving target.
//! - The hidden slots under test are ABI slots, not `LocalKind`s of their own: they are ordinary
//!   `PhpLocal`s with ordinary ownership. So each fixture also reads back the PHP-level behaviour
//!   that slot exists to implement, which is what would break if the fix had hidden the slot by
//!   changing its kind instead of its name.

use crate::support::*;

/// A static method's frame does not report its hidden late-static-binding argument.
///
/// Every static method receives the id of the class the call was made THROUGH as a leading hidden
/// argument, which is what `get_called_class()` reads. It is a calling-convention slot, so PHP has
/// no name for it and `get_defined_vars()` must list only the real parameter and the real local.
/// The trailing `get_called_class()` proves the slot still carries late static binding: the call
/// goes through the subclass, so a fix that had disturbed the argument would answer `LsbFrameBase`.
#[test]
fn test_static_method_defined_vars_excludes_the_called_class_argument() {
    // The fixture greps the variable names for the `#` marker, so it needs a `r##` raw string:
    // the `"#` inside `str_contains($name, "#")` would close an `r#` one.
    let out = compile_and_run(
        r##"<?php
class LsbFrameBase {
    public static function probe($first) {
        $local = "kept";
        $vars = get_defined_vars();
        $marked = 0;
        foreach (array_keys($vars) as $name) {
            if (str_contains($name, "#") || str_contains($name, "called_class")) {
                $marked = $marked + 1;
            }
        }
        return implode(",", array_keys($vars)) . ":" . $marked . ":" . get_called_class();
    }
}
class LsbFrameChild extends LsbFrameBase {}
echo LsbFrameChild::probe(1);
"##,
    );
    assert_eq!(out, "first,local:0:LsbFrameChild");
}

/// An eval fragment inside a static method can neither see nor overwrite the called-class slot.
///
/// The fragment probes for the slot by its readable stem, then scans every name it CAN see for
/// the unforgeable marker, so a hidden local added later cannot slip through by not being
/// enumerated here. It then assigns the stem, which is a perfectly legal PHP variable name: the
/// write has to land in the fragment's own scope and nowhere near the frame's late-static-binding
/// state, so `get_called_class()` after the eval still answers the class the call came through.
///
/// `$visible` is written by the fragment and read afterwards, so the test also fails if the
/// filtering broke ordinary scope synchronization instead of only excluding the hidden slot.
#[test]
fn test_eval_in_static_method_cannot_expose_or_clobber_late_static_binding() {
    // The fixture greps eval scope names for the `#` marker, so it needs a `r##` raw string:
    // the `"#` inside `str_contains($name, "#")` would close an `r#` one.
    let out = compile_and_run(
        r##"<?php
class LsbEvalBase {
    public static function probe($tag) {
        $visible = "before";
        $before = get_called_class();
        $fragment = 'echo array_key_exists("__elephc_called_class_id", get_defined_vars())
                ? "leak" : "clean";
            $marked = 0;
            foreach (array_keys(get_defined_vars()) as $name) {
                if (str_contains($name, "#")) { $marked = $marked + 1; }
            }
            echo $marked;
            $__elephc_called_class_id = 0;
            $visible = "after";';
        eval($fragment . ' // ' . $tag);
        return ":" . $visible . ":" . $before . ":" . get_called_class();
    }
}
class LsbEvalChild extends LsbEvalBase {}
echo LsbEvalChild::probe(1);
"##,
    );
    assert_eq!(out, "clean0:after:LsbEvalChild:LsbEvalChild");
}

/// Sorting an object property in place does not leave its write-back temporary in the frame.
///
/// A mutating builtin whose by-reference argument is a property rather than a plain local is
/// lowered as `$tmp = <place>; sort($tmp); <place> = $tmp;`. `$tmp` deliberately has ordinary PHP
/// local ownership, which is exactly why it used to be enumerated as a PHP variable; the sorted
/// property here proves the rewrite still runs and still writes back.
#[test]
fn test_property_place_rewrite_temporary_is_not_a_defined_var() {
    let out = compile_and_run(
        r#"<?php
class PlaceTempOwner { public $items = [3, 1, 2]; }
function placeTempProbe() {
    $owner = new PlaceTempOwner();
    sort($owner->items);
    $vars = get_defined_vars();
    return implode(",", $owner->items) . ":" . implode(",", array_keys($vars));
}
echo placeTempProbe();
"#,
    );
    assert_eq!(out, "1,2,3:owner");
}

/// The key-sort path's stabilized parent copy is not a PHP variable either.
///
/// `ksort()` on a nested cell of a heterogeneous property stabilizes the parent into its own
/// synthetic local before sorting the child and writing the parent back. That is a second minting
/// site for the same kind of temporary, so it gets its own fixture rather than riding on the
/// property case. The untouched sibling element pins that the write-back still happened.
#[test]
fn test_key_sort_place_rewrite_temporary_is_not_a_defined_var() {
    let out = compile_and_run(
        r#"<?php
class KeySortTempOwner { public array $rows = [["b" => 2, "a" => 1], "keep"]; }
function keySortTempProbe() {
    $owner = new KeySortTempOwner();
    ksort($owner->rows[0]);
    $vars = get_defined_vars();
    return implode(",", array_keys($owner->rows[0])) . ":" . $owner->rows[1]
        . ":" . implode(",", array_keys($vars));
}
echo keySortTempProbe();
"#,
    );
    assert_eq!(out, "a,b:keep:owner");
}

/// A literal eval fragment that writes a caller variable does not see its own scope handle.
///
/// A literal fragment with a write is lowered as an internal EIR function that receives the
/// caller's scope handle by value and reads and writes selected names through it. The handle is a
/// pointer-sized bookkeeping argument, so the only name the fragment may report is `$visible`,
/// the one real variable the enclosing frame has. Reading `$visible` back as `"after"` proves the
/// same fix did not disturb ordinary scope synchronization, which needs that handle to work.
#[test]
fn test_literal_eval_fragment_defined_vars_excludes_the_scope_handle() {
    let out = compile_and_run(
        r#"<?php
function evalScopeFragmentProbe() {
    $visible = "before";
    eval('$visible = "after"; echo implode(",", array_keys(get_defined_vars())), ":";');
    return $visible;
}
echo evalScopeFragmentProbe();
"#,
    );
    assert_eq!(out, "visible:after");
}

/// The enclosing frame of a literal eval reports no bookkeeping name of its own.
///
/// The companion to the fixture above, from the caller's side: the frame that CONTAINS the eval
/// carries its own scope bookkeeping, and none of it is a PHP variable. The scan is written over
/// the marker and the internal prefix rather than a fixed list, so a bookkeeping local added
/// later cannot pass by not being named here.
#[test]
fn test_frame_containing_a_literal_eval_reports_only_php_variables() {
    let out = compile_and_run(
        r##"<?php
function evalScopeCallerProbe() {
    $visible = "before";
    eval('$visible = "after";');
    $vars = get_defined_vars();
    $hidden = 0;
    foreach (array_keys($vars) as $name) {
        if (str_contains($name, "#") || str_starts_with($name, "__eir")) {
            $hidden = $hidden + 1;
        }
    }
    return implode(",", array_keys($vars)) . ":" . $hidden . ":" . $visible;
}
echo evalScopeCallerProbe();
"##,
    );
    assert_eq!(out, "visible:0:after");
}

/// `get_defined_vars()` in an instance method omits `$this` and keeps the ordinary locals.
///
/// PHP never lists `$this`: it is the bound object of the call, not a variable of the frame, which
/// is also why PHP refuses to assign to it. The method still reads `$this->tag`, so the binding
/// itself is untouched; only the enumeration drops it.
#[test]
fn test_instance_method_defined_vars_omits_this_and_keeps_locals() {
    let out = compile_and_run(
        r#"<?php
class ThisFrameProbe {
    public $tag = "bound";
    public function probe($first) {
        $local = "kept";
        $vars = get_defined_vars();
        return implode(",", array_keys($vars)) . ":" . $vars["first"] . ":" . $vars["local"]
            . ":" . $this->tag;
    }
}
echo (new ThisFrameProbe())->probe(1);
"#,
    );
    assert_eq!(out, "first,local:1:kept:bound");
}

/// A closure bound to an object hides `$this` the same way a declared method does.
///
/// `$this` reaches a closure through its own frame slot rather than through the method parameter
/// list, so this is a genuinely separate binding path and not a restatement of the method case.
#[test]
fn test_bound_closure_defined_vars_omits_this_and_keeps_captures() {
    let out = compile_and_run(
        r#"<?php
class ClosureThisProbe {
    public $tag = "bound";
    public function make() {
        return function ($first) {
            $local = "kept";
            $vars = get_defined_vars();
            return implode(",", array_keys($vars)) . ":" . $this->tag;
        };
    }
}
$probe = (new ClosureThisProbe())->make();
echo $probe(1);
"#,
    );
    assert_eq!(out, "first,local:bound");
}

/// An arrow function captures a user variable merely SPELLED like a compiler temporary.
///
/// `$__elephc_tag` is a perfectly legal PHP variable: the reserved marker is `#`, which no PHP
/// identifier can contain. The arrow-capture filter used to drop every `__elephc`-prefixed name,
/// so this body read an unset variable and printed nothing. The second variable is spelled like
/// the parser's own list temporary, minus the marker, which is the closest a program can legally
/// get to a generated name: it has to be captured too. A real temporary carries `#`, which no PHP
/// identifier can contain, so no fixture can write one to check the rejecting side from source.
#[test]
fn test_arrow_function_captures_a_user_variable_spelled_like_a_temporary() {
    let out = compile_and_run(
        r#"<?php
$__elephc_tag = "captured";
$__elephc_list_1_1_0 = "also";
$show = fn($suffix) => $__elephc_tag . "-" . $__elephc_list_1_1_0 . "-" . $suffix;
echo $show("end");
"#,
    );
    assert_eq!(out, "captured-also-end");
}

/// The same variable stays PHP-visible in `get_defined_vars()` and reaches an eval fragment.
///
/// Capture and enumeration are separate filters over the same predicate, so an `__elephc`-spelled
/// user variable has to survive both. `$vars` is taken before the arrow function is built, so the
/// expectation is a short exact list.
#[test]
fn test_a_user_variable_spelled_like_a_temporary_stays_enumerated() {
    let out = compile_and_run(
        r##"<?php
function elephcSpelledProbe($first) {
    $__elephc_tag = "kept";
    $vars = get_defined_vars();
    $hidden = 0;
    foreach (array_keys($vars) as $name) {
        if (str_contains($name, "#")) {
            $hidden = $hidden + 1;
        }
    }
    $read = fn() => $__elephc_tag;
    return implode(",", array_keys($vars)) . ":" . $hidden . ":" . $read();
}
echo elephcSpelledProbe(1);
"##,
    );
    assert_eq!(out, "first,__elephc_tag:0:kept");
}
