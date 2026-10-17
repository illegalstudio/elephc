//! Purpose:
//! Regressions for PR #1822 literal property types and call-boundary guards.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Errors remain catchable and preserve the value's runtime class name.
//! - Failed construction releases its receiver without invoking its destructor.

use crate::support::{compile_and_run_with_heap_debug, ProgramOutput};

/// An otherwise pure static method still has a throwing argument boundary.
#[test]
fn test_generics_review_pure_static_false_guard_keeps_its_catch() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public static function s(false $v): void {} }
function run(mixed $x): void {
    try { C::s($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(false); run(true); run(0); run("");
"#), concat!(
        "accepted\n",
        "C::s(): Argument #1 ($v) must be of type false, true given\n",
        "C::s(): Argument #1 ($v) must be of type false, int given\n",
        "C::s(): Argument #1 ($v) must be of type false, string given\n",
    ));
}

/// Checks PHP-visible output and balanced ownership on all error paths.
fn assert_clean(output: ProgramOutput, expected: &str) {
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Instance and static calls reject original source types before bool coercion.
#[test]
fn test_generics_review_false_method_and_static_arguments() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C {
    public function m(false $v): string { return gettype($v); }
    public static function s(false $v): string { return gettype($v); }
}
function run(mixed $x): void {
    $c = new C();
    try { echo $c->m($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo C::s($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(false); run(true); run(0); run(""); run(null); run(new stdClass());
"#), concat!(
        "boolean\nboolean\n",
        "C::m(): Argument #1 ($v) must be of type false, true given\n",
        "C::s(): Argument #1 ($v) must be of type false, true given\n",
        "C::m(): Argument #1 ($v) must be of type false, int given\n",
        "C::s(): Argument #1 ($v) must be of type false, int given\n",
        "C::m(): Argument #1 ($v) must be of type false, string given\n",
        "C::s(): Argument #1 ($v) must be of type false, string given\n",
        "C::m(): Argument #1 ($v) must be of type false, null given\n",
        "C::s(): Argument #1 ($v) must be of type false, null given\n",
        "C::m(): Argument #1 ($v) must be of type false, stdClass given\n",
        "C::s(): Argument #1 ($v) must be of type false, stdClass given\n",
    ));
}

/// A failing parent constructor is catchable and retires the partially constructed child.
#[test]
fn test_generics_review_false_parent_constructor_owns_failure_cleanup() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public function __construct(false $v) { echo gettype($v), "\n"; } }
class D extends C {
    public function __construct(mixed $x) { parent::__construct($x); }
    public function __destruct() { echo "drop\n"; }
}
function run(mixed $x): void {
    try { $d = new D($x); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(false); run(true); run(0); run("");
"#), concat!(
        "boolean\ndrop\n",
        "C::__construct(): Argument #1 ($v) must be of type false, true given\n",
        "C::__construct(): Argument #1 ($v) must be of type false, int given\n",
        "C::__construct(): Argument #1 ($v) must be of type false, string given\n",
    ));
}

/// A standalone null declaration keeps initialization checks and rejects non-null stores.
#[test]
fn test_generics_review_null_property_preserves_the_complete_value() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
namespace N;
class C { public null $v; }
function run(mixed $x): void {
    $c = new C();
    try { var_dump($c->v); } catch (\Error $e) { echo $e->getMessage(), "\n"; }
    $c->v = null;
    try { $c->v = $x; } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }
    var_dump($c->v);
    var_dump(get_object_vars($c));
}
run(null); run(0); run(false);
"#), concat!(
        "Typed property N\\C::$v must not be accessed before initialization\nNULL\narray(1) {\n  [\"v\"]=>\n  NULL\n}\n",
        "Typed property N\\C::$v must not be accessed before initialization\n",
        "Cannot assign int to property N\\C::$v of type null\nNULL\narray(1) {\n  [\"v\"]=>\n  NULL\n}\n",
        "Typed property N\\C::$v must not be accessed before initialization\n",
        "Cannot assign false to property N\\C::$v of type null\nNULL\narray(1) {\n  [\"v\"]=>\n  NULL\n}\n",
    ));
}

/// Property and constructor refusals use the runtime class rather than the generic object tag.
#[test]
fn test_generics_review_false_errors_name_the_runtime_class() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class P { public false $v; }
class C { public function __construct(public false $v) {} }
class User {}
function run(mixed $x): void {
    $p = new P();
    try { $p->v = $x; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { new C($x); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(new stdClass()); run(new User());
"#), concat!(
        "Cannot assign stdClass to property P::$v of type false\n",
        "C::__construct(): Argument #1 ($v) must be of type false, stdClass given\n",
        "Cannot assign User to property P::$v of type false\n",
        "C::__construct(): Argument #1 ($v) must be of type false, User given\n",
    ));
}

/// Nullable receiver calls apply the same guard after establishing a live receiver.
#[test]
fn test_generics_review_nullable_receiver_false_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public function m(false $v): string { return gettype($v); } }
function run(?C $c, mixed $x): void {
    try { echo $c->m($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(new C(), false); run(new C(), 0);
"#), "boolean\nC::m(): Argument #1 ($v) must be of type false, int given\n");
}


/// Promoted null accepts only null and keeps its property value and rejected ownership intact.
#[test]
fn test_generics_review_promoted_null_runtime_contract() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public function __construct(public null $v) {} }
$literal = new C(null); var_dump($literal->v);
function run(mixed $x): void {
    try { $c = new C($x); var_dump($c->v); }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(null); run(false); run(true); run(0); run(""); run(new stdClass());
"#), concat!(
        "NULL\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, false given\n",
        "C::__construct(): Argument #1 ($v) must be of type null, true given\n",
        "C::__construct(): Argument #1 ($v) must be of type null, int given\n",
        "C::__construct(): Argument #1 ($v) must be of type null, string given\n",
        "C::__construct(): Argument #1 ($v) must be of type null, stdClass given\n",
    ));
}

/// Interface dispatch rejects truthiness-coercible values before the concrete false ABI.
#[test]
fn test_generics_review_interface_false_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
interface I { public function m(false $v): string; }
class C implements I { public function m(false $v): string { return gettype($v); } }
function run(I $c, mixed $x): void {
    try { echo $c->m($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(new C(), false); run(new C(), true); run(new C(), 0); run(new C(), ""); run(new C(), new stdClass());
"#), concat!(
        "boolean\n",
        "C::m(): Argument #1 ($v) must be of type false, true given\n",
        "C::m(): Argument #1 ($v) must be of type false, int given\n",
        "C::m(): Argument #1 ($v) must be of type false, string given\n",
        "C::m(): Argument #1 ($v) must be of type false, stdClass given\n",
    ));
}

/// Nullable interface receivers keep the null error ahead of argument rejection.
#[test]
fn test_generics_review_nullable_interface_false_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
interface I { public function m(false $v): string; }
class C implements I { public function m(false $v): string { return gettype($v); } }
function run(?I $c, mixed $x): void {
    try { echo $c->m($x), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
run(new C(), false); run(new C(), 0); run(null, true);
"#), "boolean\nC::m(): Argument #1 ($v) must be of type false, int given\nCall to a member function m() on null\n");
}

/// Interface guards follow inherited declarations and allow wider implementation types.
#[test]
fn test_generics_review_interface_false_contravariant_and_inherited() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
interface I { public function m(false $v): string; }
class Base implements I { public function m(false $v): string { return gettype($v); } }
class Child extends Base {}
class Wide implements I { public function m(bool $v): string { return gettype($v); } }
function run(I $c, mixed $x): void {
    try { echo $c->m($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(new Child(), false); run(new Child(), 0); run(new Wide(), true); run(new Wide(), 0);
"#), "boolean\nBase::m(): Argument #1 ($v) must be of type false, int given\nboolean\nboolean\n");
}


/// Mixed-receiver constructor calls validate null before any property or body mutation.
#[test]
fn test_generics_review_mixed_receiver_promoted_null_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public int $calls = 0; public function __construct(public null $v) { $this->calls++; } }
function run(mixed $c, mixed $x): void {
    try { $c->__construct($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    echo $c->calls, "\n"; var_dump($c->v);
}
$c = new C(null);
run($c, null); run($c, false); run($c, true); run($c, 0); run($c, ""); run($c, new stdClass());
"#), concat!(
        "accepted\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, false given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, true given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, int given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, string given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, stdClass given\n2\nNULL\n",
    ));
}

/// The same candidate guard preserves false subtype checks and the existing stored value.
#[test]
fn test_generics_review_mixed_receiver_promoted_false_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public int $calls = 0; public function __construct(public false $v) { $this->calls++; } }
function run(mixed $c, mixed $x): void {
    try { $c->__construct($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    echo $c->calls, "\n"; var_dump($c->v);
}
$c = new C(false);
run($c, false); run($c, true); run($c, 0); run($c, new stdClass());
"#), concat!(
        "accepted\n2\nbool(false)\n",
        "C::__construct(): Argument #1 ($v) must be of type false, true given\n2\nbool(false)\n",
        "C::__construct(): Argument #1 ($v) must be of type false, int given\n2\nbool(false)\n",
        "C::__construct(): Argument #1 ($v) must be of type false, stdClass given\n2\nbool(false)\n",
    ));
}

/// A pure mixed-receiver method still throws before its false ABI conversion.
#[test]
fn test_generics_review_mixed_receiver_pure_false_method_guard() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public function m(false $v): void {} }
function run(mixed $c, mixed $x): void {
    try { $c->m($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
$c = new C(); run($c, false); run($c, true); run($c, 0);
"#), "accepted\nC::m(): Argument #1 ($v) must be of type false, true given\nC::m(): Argument #1 ($v) must be of type false, int given\n");
}

/// A subclass that widens `false` to `bool` accepts values the base method rejects.
#[test]
fn test_generics_review_false_override_accepts_wider_instance_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class Base {
    public function m(false $v): string { return "base:" . gettype($v); }
}
class Child extends Base {}
class Wide extends Base {
    public function m(bool $v): string { return "wide:" . gettype($v); }
}
function run(Base $c, mixed $x): void {
    try { echo $c->m($x), "\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(new Wide(), true); run(new Wide(), false); run(new Wide(), 0);
run(new Base(), true); run(new Base(), false);
run(new Child(), true); run(new Child(), false);
"#), concat!(
        "wide:boolean\nwide:boolean\nwide:boolean\n",
        "Base::m(): Argument #1 ($v) must be of type false, true given\nbase:boolean\n",
        "Base::m(): Argument #1 ($v) must be of type false, true given\nbase:boolean\n",
    ));
}

/// Nullable base receivers still reject null first, then follow the runtime override.
#[test]
fn test_generics_review_nullable_false_override_accepts_wider_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class Base {
    public function m(false $v): string { return "base:" . gettype($v); }
}
class Wide extends Base {
    public function m(bool $v): string { return "wide:" . gettype($v); }
}
function run(?Base $c, mixed $x): void {
    try { echo $c->m($x), "\n"; }
    catch (Error $e) { echo $e->getMessage(), "\n"; }
}
run(new Wide(), true); run(new Base(), true); run(null, true);
"#), concat!(
        "wide:boolean\n",
        "Base::m(): Argument #1 ($v) must be of type false, true given\n",
        "Call to a member function m() on null\n",
    ));
}

/// Late `static::` calls use the called class's signature, not the lexical `false` parameter.
#[test]
fn test_generics_review_late_static_false_override_accepts_wider_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class Base {
    public static function s(false $v): string { return "base:" . gettype($v); }
    public static function late(mixed $x): string { return static::s($x); }
}
class Child extends Base {}
class Wide extends Base {
    public static function s(bool $v): string { return "wide:" . gettype($v); }
}
function run(mixed $x): void {
    try { echo Base::late($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo Wide::late($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo Child::late($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { echo Child::s($x), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run(true); run(false);
"#), concat!(
        "Base::s(): Argument #1 ($v) must be of type false, true given\n",
        "wide:boolean\n",
        "Base::s(): Argument #1 ($v) must be of type false, true given\n",
        "Base::s(): Argument #1 ($v) must be of type false, true given\n",
        "base:boolean\nwide:boolean\nbase:boolean\nbase:boolean\n",
    ));
}

/// Builds a hierarchy crossing the old 64-link cutoff, including a widening override.
fn deep_literal_hierarchy(literal: &str, wider: &str, accepted: &str) -> String {
    let mut source = format!(r#"<?php
class Root {{
    public function m({literal} $v): string {{ return "body:" . gettype($v); }}
    public static function s({literal} $v): string {{ return "static:" . gettype($v); }}
    public static function late(mixed $v): string {{ return static::s($v); }}
}}
"#);
    for depth in 1..=70 {
        let parent = if depth == 1 { "Root".to_string() } else { format!("D{}", depth - 1) };
        source.push_str(&format!("class D{depth} extends {parent} {{}}\n"));
    }
    source.push_str(&format!(r#"
class Wide extends D70 {{
    public function m({wider} $v): string {{ return "wide:" . gettype($v); }}
    public static function s({wider} $v): string {{ return "wide-static:" . gettype($v); }}
}}
function run(Root $c, mixed $v): void {{
    try {{ echo $c->m($v), "\n"; }} catch (TypeError $e) {{ echo $e->getMessage(), "\n"; }}
}}
function nullable(?Root $c, mixed $v): void {{
    try {{ echo $c->m($v), "\n"; }} catch (Error $e) {{ echo $e->getMessage(), "\n"; }}
}}
function late(mixed $v): void {{
    try {{ echo D70::late($v), "\n"; }} catch (TypeError $e) {{ echo $e->getMessage(), "\n"; }}
    try {{ echo Wide::late($v), "\n"; }} catch (TypeError $e) {{ echo $e->getMessage(), "\n"; }}
}}
run(new D64(), true); run(new D70(), true); run(new D70(), {accepted});
run(new Wide(), true); nullable(new D70(), true); nullable(null, true);
late(true); late({accepted});
"#));
    source
}

/// Deep inherited false contracts reject true while wider overrides remain callable.
#[test]
fn test_generics_review_deep_false_method_contracts() {
    assert_clean(compile_and_run_with_heap_debug(&deep_literal_hierarchy("false", "bool", "false")), concat!(
        "Root::m(): Argument #1 ($v) must be of type false, true given\n",
        "Root::m(): Argument #1 ($v) must be of type false, true given\n",
        "body:boolean\nwide:boolean\n",
        "Root::m(): Argument #1 ($v) must be of type false, true given\n",
        "Call to a member function m() on null\n",
        "Root::s(): Argument #1 ($v) must be of type false, true given\n",
        "wide-static:boolean\nstatic:boolean\nwide-static:boolean\n",
    ));
}

/// Deep inherited promoted-null constructors reject mutation and keep null-receiver precedence.
#[test]
fn test_generics_review_deep_null_method_contracts() {
    let mut source = String::from(r#"<?php
class Root {
    public int $calls = 0;
    public function __construct(public null $v) { $this->calls++; }
}
"#);
    for depth in 1..=70 {
        let parent = if depth == 1 { "Root".to_string() } else { format!("D{}", depth - 1) };
        source.push_str(&format!("class D{depth} extends {parent} {{}}\n"));
    }
    source.push_str(r#"
function run(Root $c, mixed $v): void {
    try { $c->__construct($v); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    echo $c->calls, "\n"; var_dump($c->v);
}
function nullable(?Root $c, mixed $v): void {
    try { $c->__construct($v); }
    catch (Error $e) { echo $e->getMessage(), "\n"; }
}
$c = new D70(null);
run($c, true); run($c, null);
nullable($c, true); nullable(null, true);
echo $c->calls, "\n"; var_dump($c->v);
"#);
    assert_clean(compile_and_run_with_heap_debug(&source), concat!(
        "Root::__construct(): Argument #1 ($v) must be of type null, true given\n1\nNULL\n",
        "accepted\n2\nNULL\n",
        "Root::__construct(): Argument #1 ($v) must be of type null, true given\n",
        "Call to a member function __construct() on null\n2\nNULL\n",
    ));
}

/// Runtime array callables reject non-null constructor arguments before mutating the receiver.
#[test]
fn test_generics_review_array_callable_promoted_null_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public int $calls = 0; public function __construct(public null $v) { $this->calls++; } }
class Child extends C {}
function run(mixed $c, mixed $x): void {
    $callable = [$c, "__construct"];
    try { $callable($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    echo $c->calls, "\n"; var_dump($c->v);
}
$c = new Child(null);
run($c, null); run($c, false); run($c, true); run($c, 0); run($c, ""); run($c, new stdClass());
"#), concat!(
        "accepted\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, false given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, true given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, int given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, string given\n2\nNULL\n",
        "C::__construct(): Argument #1 ($v) must be of type null, stdClass given\n2\nNULL\n",
    ));
}

/// Array-callable false constructors preserve the stored value and body count on rejection.
#[test]
fn test_generics_review_array_callable_promoted_false_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public int $calls = 0; public function __construct(public false $v) { $this->calls++; } }
function run(mixed $c, mixed $x): void {
    $callable = [$c, "__construct"];
    try { $callable($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    echo $c->calls, "\n"; var_dump($c->v);
}
$c = new C(false);
run($c, false); run($c, true); run($c, 0); run($c, new stdClass());
"#), concat!(
        "accepted\n2\nbool(false)\n",
        "C::__construct(): Argument #1 ($v) must be of type false, true given\n2\nbool(false)\n",
        "C::__construct(): Argument #1 ($v) must be of type false, int given\n2\nbool(false)\n",
        "C::__construct(): Argument #1 ($v) must be of type false, stdClass given\n2\nbool(false)\n",
    ));
}

/// Pure array-callable methods keep throwing guards while wider overrides accept true.
#[test]
fn test_generics_review_array_callable_false_method_contract() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public function m(false $v): void {} }
class Wide extends C { public function m(bool $v): void { var_dump($v); } }
function run(mixed $c, mixed $x): void {
    $callable = [$c, "m"];
    try { $callable($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
$c = new C(); run($c, false); run($c, true); run($c, 0); run(new Wide(), true);
"#), concat!(
        "accepted\n",
        "C::m(): Argument #1 ($v) must be of type false, true given\n",
        "C::m(): Argument #1 ($v) must be of type false, int given\n",
        "bool(true)\naccepted\n",
    ));
}

/// Inherited first-class constructor descriptors retain the declaring class's argument contract.
#[test]
fn test_generics_review_first_class_inherited_null_argument() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public int $calls = 0; public function __construct(public null $v) { $this->calls++; } }
class Child extends C {}
function run(mixed $callable, mixed $x): void {
    try { $callable($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
$c = new Child(null); $callback = $c->__construct(...);
run($callback, 0); echo $c->calls, "\n";
run($callback, null); echo $c->calls, "\n"; var_dump($c->v);
"#), "C::__construct(): Argument #1 ($v) must be of type null, int given\n1\naccepted\n2\nNULL\n");
}

/// Named null arguments reject before construction and release earlier coerced string owners.
#[test]
fn test_generics_review_named_array_callable_null_argument_cleans_earlier_args() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C {
    public int $calls = 0;
    public function __construct(public string $prefix, public null $v) { $this->calls++; }
}
function run(mixed $c, mixed $x): void {
    $callable = [$c, "__construct"];
    try { call_user_func_array($callable, ["v" => $x, "prefix" => 123]); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
$c = new C("seed", null);
run($c, new stdClass()); echo $c->calls, "|", $c->prefix, "\n"; var_dump($c->v);
run($c, null); echo $c->calls, "|", $c->prefix, "\n"; var_dump($c->v);
"#), concat!(
        "C::__construct(): Argument #2 ($v) must be of type null, stdClass given\n",
        "1|seed\nNULL\naccepted\n2|123\nNULL\n",
    ));
}

/// Shared invoker signatures retain each free/static callable's diagnostic and wider overrides.
#[test]
fn test_generics_review_shared_invoker_literal_argument_names() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function accept_false(false $v): void { echo "body\n"; }
class C { public static function m(false $v): void { echo "body\n"; } }
class Child extends C {}
class Wide extends C { public static function m(bool $v): void { echo "wide\n"; } }
function run(mixed $callable, mixed $x): void {
    try { $callable($x); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
run("accept_false", true);
run([Child::class, "m"], true);
run([Wide::class, "m"], true);
"#), concat!(
        "accept_false(): Argument #1 ($v) must be of type false, true given\n",
        "C::m(): Argument #1 ($v) must be of type false, true given\n",
        "wide\naccepted\n",
    ));
}

/// A scalar argument array rejects an integer before null conversion and releases its fresh box.
#[test]
fn test_generics_review_array_callable_literal_integer_for_null() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class C { public int $calls = 0; public function __construct(public null $v) { $this->calls++; } }
function run(mixed $c): void {
    $callable = [$c, "__construct"];
    try { $callable(42); echo "accepted\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    echo $c->calls, "\n"; var_dump($c->v);
}
$c = new C(null); run($c);
"#), "C::__construct(): Argument #1 ($v) must be of type null, int given\n1\nNULL\n");
}
