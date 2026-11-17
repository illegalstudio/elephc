//! Purpose:
//! Regressions for review findings in IR lowering and the checker, each compared with php's output.
//!
//! Called from:
//! - The codegen_tests integration harness.
//!
//! Key details:
//! - Every program prints what `php -n` prints for it.

use crate::support::*;

/// An earlier argument's effect on a by-reference static property is kept by the write-back.
#[test]
fn test_ref_place_arg_reads_place_after_earlier_arguments() {
    let out = compile_and_run(r#"<?php
class C { public static array $items = []; static function add(): int { self::$items[] = 1; return 1; } }
function f(int $n, array &$a) { $a[] = 2; }
f(C::add(), C::$items);
echo json_encode(C::$items);
"#);
    assert_eq!(out, "[1,2]");
}

/// `unset($x[$k])` of a gradual local or property holding an ArrayAccess object calls offsetUnset.
#[test]
fn test_unset_element_of_gradual_array_access_object() {
    let out = compile_and_run(r#"<?php
function forget(mixed $bag, string $k): void { unset($bag[$k]); }
$o = new ArrayObject(['a' => 1]); forget($o, 'a'); echo count($o), "|";
class H { protected $app; function __construct($a) { $this->app = $a; } function go() { unset($this->app['x']); } }
$ao = new ArrayObject(['x' => 1, 'y' => 2]); (new H($ao))->go(); echo count($ao), "|";
function arr(mixed $m, object $k) { unset($m[$k->id]); return $m; }
$k = new stdClass; $k->id = 'a';
echo json_encode(arr(['a' => 1, 'b' => 2], $k));
"#);
    assert_eq!(out, "0|1|{\"b\":2}");
}

/// `Closure::fromCallable` leaves no scratch local behind.
#[test]
fn test_closure_from_callable_scratch_is_not_a_php_local() {
    let out = compile_and_run(r#"<?php
function g(object $o, string $n) { $c = Closure::fromCallable([$o, $n]); return array_keys(get_defined_vars()); }
echo json_encode(g(new ArrayObject, 'count'));
"#);
    assert_eq!(out, "[\"o\",\"n\",\"c\"]");
}

/// `$x::CONST` through an interface-typed receiver reads the implementer's constant.
#[test]
fn test_dynamic_constant_through_interface_receiver() {
    let out = compile_and_run(r#"<?php
interface I { const FOO = 'i'; } class A implements I { const FOO = 'a'; }
function h(I $x) { return $x::FOO; }
echo h(new A);
"#);
    assert_eq!(out, "a");
}

/// A reflected method binds the reflected class even for an untyped receiver.
#[test]
fn test_reflection_method_invoke_binds_reflected_class_on_mixed_receiver() {
    let out = compile_and_run(r#"<?php
class P { function get() { return "P"; } } class D extends P { function get() { return "D"; } }
function r($o) { $m = new ReflectionMethod(P::class, 'get'); return $m->invoke($o); }
echo r(new D);
"#);
    assert_eq!(out, "P");
}

/// A by-reference `offsetGet` of a class reaching ArrayAccess through an interface reads by value.
#[test]
fn test_by_ref_offset_get_through_array_access_sub_interface() {
    let out = compile_and_run(r#"<?php
interface Bag extends ArrayAccess {}
class B implements Bag { private array $d = ['k' => 'v'];
    public function &offsetGet(mixed $k): mixed { return $this->d[$k]; }
    public function offsetExists(mixed $k): bool { return true; }
    public function offsetSet(mixed $k, mixed $v): void {}
    public function offsetUnset(mixed $k): void {} }
echo (new B)['k'];
"#);
    assert_eq!(out, "v");
}

/// A stale known string suffix does not narrow a dynamic reference to the wrong property.
#[test]
fn test_dynamic_property_reference_after_foreach_rebinding() {
    let out = compile_and_run(r#"<?php
class C { public int $aCount = 1; public string $label = 'x'; }
function f(C $o, array $names, string $p) {
    $n = $p . 'Count';
    foreach ($names as $n) { $r = &$o->$n; $r = 'changed'; unset($r); }
    return $o->label;
}
echo f(new C, ['label'], 'a');
"#);
    assert_eq!(out, "changed");
}

/// A by-reference widened parameter forwarded or captured inside a `[...] =` or `yield`-style form.
#[test]
fn test_ref_param_captured_inside_property_array_assignment() {
    let out = compile_and_run(r#"<?php
class R {
    public array $lazy = [];
    function load(array &$inline) { $this->lazy['a'] = function () use (&$inline) { $inline['k'] = 1; }; }
}
$data = ['z' => 0];
$r = new R; $r->load($data); ($r->lazy['a'])();
echo json_encode($data);
"#);
    assert_eq!(out, "{\"z\":0,\"k\":1}");
}

/// `getChildren()` of a symlinked directory entry returns the child iterator.
#[test]
fn test_recursive_directory_iterator_get_children_ignores_has_children() {
    let out = compile_and_run(r#"<?php
$dir = sys_get_temp_dir() . '/rdi_' . getmypid();
mkdir($dir); mkdir("$dir/real"); symlink("$dir/real", "$dir/link");
$it = new RecursiveDirectoryIterator($dir, FilesystemIterator::SKIP_DOTS);
$n = 0;
foreach ($it as $f) { if ($f->getFilename() === 'link') { $c = $it->getChildren(); $n = $c instanceof RecursiveDirectoryIterator ? 1 : 0; } }
unlink("$dir/link"); rmdir("$dir/real"); rmdir($dir);
echo $n;
"#);
    assert_eq!(out, "1");
}

/// The parser's hidden temporaries for a conditional dynamic-property append and for an
/// element reference are generated locals: `get_defined_vars()` lists only the PHP variables.
#[test]
fn test_parser_desugar_temporaries_are_not_php_visible() {
    let out = compile_and_run(r#"<?php
function f($o, $c) {
    $o->{$c ? 'a' : 'b'}[] = 1;
    $arr = [];
    $src = [1];
    $arr[] = &$src[0];
    return array_keys(get_defined_vars());
}
echo json_encode(f(new stdClass, true));
"#);
    assert_eq!(out, "[\"o\",\"c\",\"arr\",\"src\"]");
}

/// An `include` inside the source of a reference assignment (`$a = &$arr[(require ...)]`) is
/// resolved like any other expression; it used to survive to the checker and panic.
#[test]
fn test_include_inside_a_reference_assignment_source_is_resolved() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php\n$arr = [];\n$a = &$arr[(require __DIR__ . '/key.php')];\n$a = 5;\necho json_encode($arr);\n"),
            ("key.php", "<?php return 0;"),
        ],
        "main.php",
    );
    assert_eq!(out, "[5]");
}

/// A float of magnitude 1e14 or more is written exponentially by php (`1.0E+14`), so the fold
/// must not compare its plain decimal form against a non-numeric string.
#[test]
fn test_float_at_1e14_compares_by_its_exponential_string_form() {
    let out = compile_and_run(r#"<?php
var_dump(1e14 < "1.0E+14x");
var_dump(1e14 > "100000000000000x");
var_dump(1e13 < "1.0E+13x");
var_dump(1.5e25 < "1.5E+25x", -2.5e-7 < "-2.5E-7x", 3.0e20 > "3.0E+2x");
"#);
    assert_eq!(out, "bool(true)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(false)\n");
}
