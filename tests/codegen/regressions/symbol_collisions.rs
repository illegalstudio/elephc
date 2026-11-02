//! Purpose:
//! Regression tests for generated-symbol and assembly-label collisions between unrelated PHP
//! declarations whose names differ only in where an underscore or a non-ASCII character falls.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture used to either miscompile (two declarations sharing one storage cell) or fail
//!   to assemble with a duplicate-symbol error, so "compiles and matches PHP" is the assertion.
//! - Expected outputs are PHP 8.4 reference outputs; the fixtures avoid runtime-unknown values so
//!   they stay readable, and none of them are constant-foldable across calls.

use crate::support::*;

/// Verifies two classes whose static properties share a mangled `<class>_<property>` spelling
/// keep separate storage. `a::$u_b` and `a_u::$b` both mangled to `_static_prop_a_u_u_b`, so the
/// duplicate `.comm` directives merged and both reads observed the last writer.
#[test]
fn test_static_properties_of_underscore_ambiguous_classes_do_not_share_storage() {
    let out = compile_and_run(
        r#"<?php
class a   { static $u_b = 1; }
class a_u { static $b   = 2; }
echo a::$u_b, ",", a_u::$b;
"#,
    );
    assert_eq!(out, "1,2");
}

/// Verifies methods on two classes whose mangled `<class>_<method>` spellings coincide still
/// assemble. `a::u_b()` and `a_u::b()` both produced `_method_a_u_u_b`, so valid PHP failed to
/// compile with an `already defined` assembler error.
#[test]
fn test_methods_of_underscore_ambiguous_classes_compile_and_dispatch() {
    let out = compile_and_run(
        r#"<?php
class a   { function u_b() { echo "A"; } }
class a_u { function b()   { echo "B"; } }
$x = new a();   $x->u_b();
$y = new a_u(); $y->b();
"#,
    );
    assert_eq!(out, "AB");
}

/// Verifies static methods and enum cases survive the same ambiguous class/member join that broke
/// instance methods.
#[test]
fn test_static_methods_and_enum_cases_of_underscore_ambiguous_names_compile() {
    let out = compile_and_run(
        r#"<?php
class s   { static function u_m() { echo "S"; } }
class s_u { static function m()   { echo "T"; } }
enum e   { case u_c; }
enum e_u { case c; }
s::u_m();
s_u::m();
echo e::u_c->name, e_u::c->name;
"#,
    );
    assert_eq!(out, "STu_cc");
}

/// Verifies a static local named `$x_init` no longer aliases the one-shot initialization flag of
/// static `$x`. The flag used to be `<storage symbol>_init` built from the raw PHP variable name,
/// so writing `$x_init = 0` cleared `$x`'s flag and re-ran its initializer on the next call.
#[test]
fn test_static_local_named_init_does_not_alias_another_statics_flag() {
    let out = compile_and_run(
        r#"<?php
function f($n) {
    static $x = 5;
    static $x_init = 7;
    $x_init = 0;
    $x = $x + $n;
    return $x;
}
echo f(1), ";", f(1);
"#,
    );
    assert_eq!(out, "6;7");
}

/// Verifies static locals of two distinct functions keep separate cells when the function names
/// differ only by an underscore versus a non-ASCII character. The old fragment helper mapped every
/// non-ASCII byte to `_`, so `fragment("aéb") + "_c"` collided with `fragment("a") + "_b_c"`.
#[test]
fn test_static_locals_do_not_collide_across_underscore_ambiguous_functions() {
    let out = compile_and_run(
        r#"<?php
function a($n)   { static $b_c = 1; $b_c = $b_c + $n; return $b_c; }
function aéb($n) { static $c   = 2; $c   = $c   + $n; return $c; }
echo a(0), ",", aéb(0), ",", a(20), ",", aéb(40);
"#,
    );
    assert_eq!(out, "1,2,21,42");
}

/// Verifies a class's static method and a same-named function's static local no longer share the
/// `_static_` prefix. `A::m()` and `function A() { static $m; }` both produced `_static_A_m`,
/// which the assembler rejected as an `invalid symbol redefinition` of a code label by a `.comm`.
#[test]
fn test_static_method_and_static_local_of_same_named_function_do_not_collide() {
    let out = compile_and_run(
        r#"<?php
class A { static function m() { return 7; } }
function A($n) { static $m = 5; $m = $m + $n; return $m; }
echo A::m(), ",", A(1);
"#,
    );
    assert_eq!(out, "7,6");
}

/// Verifies two functions whose names differ only by an underscore versus a non-ASCII character
/// emit distinct internal labels. Both used to produce `_eir_a_b_if_else_3`, so the assembler
/// rejected the program with a duplicate-label error.
#[test]
fn test_internal_labels_do_not_collide_across_underscore_ambiguous_functions() {
    let out = compile_and_run(
        r#"<?php
function a_b($n) { if ($n > 0) { echo "p"; } else { echo "n"; } }
function aéb($n) { if ($n > 0) { echo "P"; } else { echo "N"; } }
a_b(1); aéb(1); a_b(-1); aéb(-1);
"#,
    );
    assert_eq!(out, "pPnN");
}


/// `class_alias()` with a literal `false` third argument is still statically resolvable.
///
/// That argument is PHP's `$autoload` -- whether to autoload the ORIGINAL class before aliasing.
/// It says nothing about whether the two names are known at compile time, but refusing it made
/// the whole call "requires statically resolvable class names", which is what kept Symfony's
/// generated container out of the compiled world: it emits
/// `\class_alias(\ContainerXXXX\App_KernelProdContainer::class, App_KernelProdContainer::class, false)`,
/// both names `::class` constants.
///
/// Oracle: `php -n` prints the asserted line.
#[test]
fn test_class_alias_accepts_a_literal_false_autoload_flag() {
    let out = compile_and_run(
        r#"<?php
namespace Inner {
    class Original { public function hello(): string { return 'hi'; } }
}
namespace {
    if (!\class_exists(\Aliased::class, false)) {
        \class_alias(\Inner\Original::class, \Aliased::class, false);
    }
    $a = new \Aliased();
    echo $a->hello(), '|', var_export($a instanceof \Inner\Original, true),
         '|', var_export(\class_exists(\Aliased::class, false), true);
}
"#,
    );
    assert_eq!(out, "hi|true|true");
}
