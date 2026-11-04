//! Purpose:
//! End-to-end tests for PHP reference machinery: aliasing a local to an object
//! property (`$x = &$obj->prop`) with write-through in both directions, by-reference
//! function/method returns (`function &f()`, `function &m()`), capturing them with
//! `$x = &call()`, object properties passed to by-reference method/constructor parameters,
//! and the constant-propagation soundness fix for reference-bound locals.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A reference property's slot holds a pointer to a 16-byte ref-cell; reads and writes
//!   on either the local alias or the property dereference the shared cell, so a write
//!   through one side is observed through the other.
//! - Resolved by-reference returns transfer a managed cell owner to `$x = &call()`;
//!   borrowed cells remain non-owning. Ordinary value calls copy the referenced value.
//! - Every raw cell pointer travels in the integer result register, including cells
//!   containing a string pair or a float, without using string/float result registers.

use crate::support::*;

/// `$s = &$_SESSION` must alias the superglobal itself, in a function and at top level.
///
/// A superglobal has no frame slot, and the local-to-local alias used to bind the target to a
/// stray, empty slot named `_SESSION`: reads through the alias saw null and writes never reached
/// the superglobal (Symfony's session storage binds its bags this way). Expected output is
/// `php -n` 8.5.10's.
#[test]
fn test_reference_alias_of_superglobal_writes_through() {
    let out = compile_and_run(
        r#"<?php
function load(): void {
    $session = &$_SESSION;
    $r = &$session["a"];
    echo $r["x"], "|";
    $session["b"] = str_repeat("cd", 2);
}
$_SESSION = ["a" => ["x" => str_repeat("ab", 3)]];
load();
echo json_encode($_SESSION), "|";
$s2 = &$_SESSION;
$s2["c"] = 1;
echo json_encode($_SESSION);
"#,
    );
    assert_eq!(
        out,
        r#"ababab|{"a":{"x":"ababab"},"b":"cdcd"}|{"a":{"x":"ababab"},"b":"cdcd","c":1}"#
    );
}

/// A property and an array element can take a reference to a superglobal, and writes through
/// either reach the superglobal itself, as in php.
///
/// `$this->serverData = &$_SERVER` (Monolog's `WebProcessor`) bound a stray frame slot named
/// `_SERVER` and left the property a copy, and `['q' => &$_GET]` threw on the CLI and dropped a
/// nested write (`$box['q']['hit'] = ...`) everywhere. The superglobal's shared cell now holds a
/// boxed Mixed in every mode, which properties, elements and locals all share. Expected output
/// is `php -n` 8.5.10's; the heap must end clean.
#[test]
fn test_property_and_element_references_to_superglobals_write_through() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class WebProcessor {
    protected $serverData;
    public function __construct() { $this->serverData = &$_SERVER; }
    public function uri() { return $this->serverData['REQUEST_URI'] ?? 'none'; }
    public function set($k, $v) { $this->serverData[$k] = $v; }
}
$_SERVER['REQUEST_URI'] = '/before';
$p = new WebProcessor();
echo $p->uri(), "|";
$_SERVER['REQUEST_URI'] = '/after';
echo $p->uri(), "|";
$p->set('ADDED', 'yes');
echo $_SERVER['ADDED'] ?? 'missing', "|";
$_GET['seed'] = 's';
$box = ['q' => &$_GET];
$box['q']['hit'] = 'x';
echo $_GET['hit'] ?? 'missing', ":", count($_GET), "|";
function local_alias() { $s = &$_SERVER; $s['LOCAL'] = 'l'; }
local_alias();
echo $_SERVER['LOCAL'] ?? 'missing';
unset($p, $box);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "/before|/after|yes|x:2|l");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A by-reference parameter forwarded to another by-reference parameter after an update keeps
/// the caller's cell representation.
///
/// `$c = $c + 1` (or `++$c`) types `$c` gradual, and the forwarding call site then re-stored the
/// PARAMETER's slot as boxed `Mixed`, while the cell is the caller's raw `int`: the body unboxed
/// the integer as a pointer and segfaulted. Symfony's `FlattenException::flattenArgs(array
/// $args, int $level = 0, int &$count = 0)` has this shape and killed every error page.
/// Expected output is `php -n` 8.5.10's.
#[test]
fn test_updated_by_ref_param_forwarded_by_reference_keeps_caller_cell() {
    let out = compile_and_run(
        r#"<?php
function h(int &$c): void { ++$c; }
function g(int &$c): int { $c = $c + 1; h($c); return $c; }
function flatten(array $args, int $level = 0, int &$count = 0): int {
    foreach ($args as $value) {
        ++$count;
        if (is_array($value)) { flatten($value, $level + 1, $count); }
    }
    return $count;
}
$x = 5;
echo g($x), "|", $x, "|", flatten([1, [2, [3, 4]], 5]);
"#,
    );
    assert_eq!(out, "7|7|7");
}

/// A local bound to a newly appended nested slot must share writes with that slot.
#[test]
fn test_reference_to_nested_array_append_slot_writes_through() {
    let out = compile_and_run(
        r#"<?php
$items = ["event" => []];
$prior = "keep";
$slot = &$prior;
$slot = &$items["event"][];
$slot = "ready";
echo $prior, "|", $items["event"][0];
"#,
    );
    assert_eq!(out, "keep|ready");
}

/// A fresh by-reference foreach value must be canonical null when the source is empty.
/// Capturing it by reference immediately afterwards exercises the raw-to-ref-cell path,
/// which previously promoted an uninitialized frame word when the loop ran zero times.
#[test]
fn test_empty_foreach_initializes_fresh_by_ref_value_before_capture() {
    let out = compile_and_run(
        r#"<?php
$items = [1];
array_pop($items);
foreach ($items as &$value) {
}
$read = function () use (&$value): mixed {
    return $value;
};
echo is_null($read()) ? "null" : "not-null";
$value = "updated";
echo "|", $read();
"#,
    );
    assert_eq!(out, "null|updated");
}

/// `$x = &$obj->prop` aliases a scalar property: writing the local updates the property
/// and writing the property updates the local (write-through in both directions).
#[test]
fn test_reference_to_scalar_property_writes_through_both_ways() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 1; }
        $o = new C();
        $r = &$o->v;
        $r = 5;
        echo $o->v, \"\\n\";
        $o->v = 9;
        echo $r, \"\\n\";",
    );
    assert_eq!(out, "5\n9\n");
}

/// A later local alias promotes the unbound path after a conditional property reference.
#[test]
fn test_conditional_property_reference_then_local_alias() {
    let out = compile_and_run(
        r#"<?php
class ConditionalRefBox { public int $value = 1; }
function update_conditional_ref(bool $bind): int {
    $local = 1;
    $box = new ConditionalRefBox();
    if ($bind) {
        $local =& $box->value;
    }
    $alias =& $local;
    $alias = 7;
    return $local;
}
echo update_conditional_ref(false) . '|' . update_conditional_ref(true);
"#,
    );
    assert_eq!(out, "7|7");
}

/// `$x = &$obj->prop` aliases an array property: appends through the alias are observed
/// through the property, and clearing the alias to `[]` empties the property (the shape
/// used by `$instanceof = []` after capturing a reference).
#[test]
fn test_reference_to_array_property_appends_and_clears() {
    let out = compile_and_run(
        "<?php
        class C { public array $v = []; }
        $o = new C();
        $r = &$o->v;
        $r[] = 1;
        $r[] = 2;
        echo implode(',', $o->v), \"\\n\";
        $r = [];
        echo count($o->v), \"\\n\";
        $r[] = 9;
        echo $o->v[0], \"\\n\";",
    );
    assert_eq!(out, "1,2\n0\n9\n");
}

/// Two declared properties bound by reference share one cell and observe writes both ways.
#[test]
fn test_property_to_property_reference_assignment_writes_through() {
    let out = compile_and_run(
        "<?php
        class RefPropertyBox {
            public array $refs = [];
            public function bind(RefPropertyBox $source): void {
                $this->refs = &$source->refs;
            }
        }
        $source = new RefPropertyBox();
        $target = new RefPropertyBox();
        $target->bind($source);
        $source->refs[] = 'source';
        echo $target->refs[0], \"\\n\";
        $target->refs[] = 'target';
        echo $source->refs[1], \"\\n\";",
    );
    assert_eq!(out, "source\ntarget\n");
}

/// Reassigning the reference to a non-empty, differently-typed array literal boxes the
/// literal's elements so the property's `Array(Mixed)` reads stay valid (regression: the
/// raw `Array(Int)`/`Array(Str)` payload was stored unboxed and read back as garbage).
#[test]
fn test_reference_array_reassigned_to_typed_literal_boxes_elements() {
    let out = compile_and_run(
        "<?php
        class C { public array $v = []; }
        $o = new C();
        $r = &$o->v;
        $r[] = 1;
        echo implode(',', $o->v), \"\\n\";
        $r = [42, 43];
        echo implode(',', $o->v), \"\\n\";
        echo $o->v[0], \"\\n\";
        echo count($o->v), \"\\n\";
        $r = ['a', 'b', 'c'];
        echo implode('-', $o->v), \"\\n\";",
    );
    assert_eq!(out, "1\n42,43\n42\n2\na-b-c\n");
}

/// The property keeps its declared default until the reference writes through it.
#[test]
fn test_reference_property_keeps_default_until_written() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 7; }
        $o = new C();
        echo $o->v, \"\\n\";
        $r = &$o->v;
        echo $r, \"\\n\";
        $r = 11;
        echo $o->v, \"\\n\";",
    );
    assert_eq!(out, "7\n7\n11\n");
}

/// `isset()` probes the owner-slot marker and dereferenced value without triggering a typed
/// uninitialized-property read, including properties promoted to reference-cell storage.
#[test]
fn test_isset_reference_properties_respects_initialization_and_null() {
    let out = compile_and_run(
        r#"<?php
class ReferenceIssetBox {
    public int $typed;
    public mixed $nullable = null;
    public int $zero = 0;

    public function &typedRef(): int { return $this->typed; }
    public function &nullableRef(): mixed { return $this->nullable; }
    public function &zeroRef(): int { return $this->zero; }
}
$box = new ReferenceIssetBox();
var_dump(isset($box->typed), isset($box->nullable), isset($box->zero));
$box->typed = 7;
$alias =& $box->nullable;
$alias = "set";
var_dump(isset($box->typed), isset($box->nullable));
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

/// Unsetting an element through an array by-reference parameter publishes a relocated sparse hash
/// through the shared cell, so string and integer-key removals are both visible to the caller.
#[test]
fn test_unset_array_element_through_by_ref_parameter() {
    let out = compile_and_run(
        r#"<?php
function remove_ref_key(array &$values, mixed $key): void {
    unset($values[$key]);
}
$assoc = ["keep" => 1, "drop" => 2];
$copy = $assoc;
remove_ref_key($assoc, "drop");
echo count($assoc), ":", isset($assoc["drop"]) ? "bad" : "assoc", ":", count($copy), "|";
$list = [10, 20, 30];
remove_ref_key($list, 1);
echo count($list), ":", array_is_list($list) ? "list" : "assoc", ":", $list[2];
"#,
    );
    assert_eq!(out, "1:assoc:2|2:assoc:30");
}

/// A by-reference free function returns a reference to a property; `$x = &f()` aliases it
/// and a write through `$x` updates the property.
#[test]
fn test_by_reference_function_return_aliases_property() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 10; }
        function &getv(C $o) { return $o->v; }
        $o = new C();
        $r = &getv($o);
        $r = 77;
        echo $o->v, \"\\n\";",
    );
    assert_eq!(out, "77\n");
}

/// A by-reference method returns a reference to `$this->prop`; `$x = &$o->m()` aliases it
/// and appends through the alias update the property's array.
#[test]
fn test_by_reference_method_return_aliases_property() {
    let out = compile_and_run(
        "<?php
        class Box {
            public array $items = [];
            public function &ref() { return $this->items; }
        }
        $b = new Box();
        $r = &$b->ref();
        $r[] = 'x';
        $r[] = 'y';
        echo implode(',', $b->items), \"\\n\";",
    );
    assert_eq!(out, "x,y\n");
}

/// Plain local-to-local aliasing writes through in both directions, even when a write
/// goes through the other alias between reads (regression: reference-bound locals must
/// not carry stale propagated constants).
#[test]
fn test_local_alias_write_through_not_constant_folded() {
    let out = compile_and_run(
        "<?php
        $a = 1;
        $b = &$a;
        $b = 5;
        $a = 7;
        echo $b, \"\\n\";",
    );
    assert_eq!(out, "7\n");
}

/// A by-reference closure returning a captured object's property, called through a variable,
/// aliases the property so appends through the captured reference reach it.
#[test]
fn test_by_reference_closure_return_via_variable() {
    let out = compile_and_run(
        "<?php
        class C { public array $items = []; }
        $o = new C();
        $f = function &() use ($o) { return $o->items; };
        $ref = &$f();
        $ref[] = 'a';
        $ref[] = 'b';
        echo implode(',', $o->items), \"\\n\";",
    );
    assert_eq!(out, "a,b\n");
}

/// An immediately-invoked by-reference closure returning a captured object's property
/// aliases that property.
#[test]
fn test_by_reference_closure_immediate_invoke() {
    let out = compile_and_run(
        "<?php
        class C { public array $items = []; }
        $o = new C();
        $ref = &(function &() use ($o) { return $o->items; })();
        $ref[] = 'x';
        echo implode(',', $o->items), \"\\n\";",
    );
    assert_eq!(out, "x\n");
}

/// Binds an arrow closure that returns a reference to `$this->prop`, captures the reference,
/// mutates it through the alias, and clears it through the same shared property cell.
#[test]
fn test_closure_bind_by_reference_return_writes_through() {
    let out = compile_and_run(
        "<?php
        class Loader { public array $instanceof = []; }
        $loader = new Loader();
        $instanceof = &\\Closure::bind(fn &() => $this->instanceof, $loader, $loader)();
        $instanceof[] = 'RouteA';
        echo implode(',', $loader->instanceof), \"\\n\";
        $instanceof = [];
        echo count($loader->instanceof), \"\\n\";",
    );
    assert_eq!(out, "RouteA\n0\n");
}

/// Verifies an immediate by-reference bind inside an unrelated method resolves `$this` against
/// the receiver object and writes through the returned property cell.
#[test]
fn test_closure_bind_by_reference_inside_method_uses_bound_receiver() {
    let out = compile_and_run(
        r#"<?php
class BoundStore { public array $items = []; }
class StoreWriter {
    public function write(BoundStore $store): string {
        $items = &\Closure::bind(fn &() => $this->items, $store, $store)();
        $items[] = "written";
        return implode(",", $store->items);
    }
}
echo (new StoreWriter())->write(new BoundStore());
"#,
    );
    assert_eq!(out, "written");
}

/// `$x = &$obj->prop` aliases a `string` property: the cell pointer is one word, so the
/// write-through works despite the string ABI normally using a `{ptr,len}` register pair.
#[test]
fn test_reference_to_string_property_writes_through_both_ways() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        $o = new C();
        $r = &$o->s;
        $r = \"viaref\";
        echo $o->s, \"\\n\";
        $o->s = \"viaprop\";
        echo $r, \"\\n\";",
    );
    assert_eq!(out, "viaref\nviaprop\n");
}

/// `$x = &$obj->prop` aliases a `float` property: the cell pointer is one word, so the
/// write-through works despite floats normally returning in a floating-point register.
#[test]
fn test_reference_to_float_property_writes_through_both_ways() {
    let out = compile_and_run(
        "<?php
        class C { public float $f = 1.5; }
        $o = new C();
        $r = &$o->f;
        $r = 3.25;
        echo $o->f, \"\\n\";
        $o->f = 9.75;
        echo $r, \"\\n\";",
    );
    assert_eq!(out, "3.25\n9.75\n");
}

/// A by-reference free function returning a `string` property: the caller binds the cell
/// pointer (one word) and a write through the alias updates the property.
#[test]
fn test_by_reference_function_returns_string_property() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        function &slot(C $o): string { return $o->s; }
        $o = new C();
        $r = &slot($o);
        $r = \"viafunc\";
        echo $o->s, \"\\n\";",
    );
    assert_eq!(out, "viafunc\n");
}

/// A by-reference free function returning a `float` property aliases it through the cell
/// pointer rather than the float result register.
#[test]
fn test_by_reference_function_returns_float_property() {
    let out = compile_and_run(
        "<?php
        class C { public float $f = 1.5; }
        function &slot(C $o): float { return $o->f; }
        $o = new C();
        $r = &slot($o);
        $r = 9.75;
        echo $o->f, \"\\n\";",
    );
    assert_eq!(out, "9.75\n");
}

/// A by-reference method returning a `string` property aliases `$this->prop`; the method
/// call result is stored single-word so the alias dereferences the right cell.
#[test]
fn test_by_reference_method_returns_string_property() {
    let out = compile_and_run(
        "<?php
        class Holder {
            public string $tag = \"h0\";
            public function &tagSlot(): string { return $this->tag; }
        }
        $h = new Holder();
        $t = &$h->tagSlot();
        $t = \"viamethod\";
        echo $h->tag, \"\\n\";",
    );
    assert_eq!(out, "viamethod\n");
}

/// An immediately invoked `Closure::bind` over a string property returns a reference to
/// `$this->prop`, which remains shared when captured and mutated through the alias.
#[test]
fn test_closure_bind_by_reference_string_property() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        $c = new C();
        $ref = &\\Closure::bind(fn &() => $this->s, $c, $c)();
        $ref = \"bound\";
        echo $c->s, \"\\n\";
        $c->s = \"viaprop\";
        echo $ref, \"\\n\";",
    );
    assert_eq!(out, "bound\nviaprop\n");
}

/// A by-reference `Closure::bind` stored in a variable and called separately (not invoked
/// immediately) still aliases the bound property: the assignment tracks the bound closure as a
/// static callable so `$bound()` lowers to a direct call carrying the cell pointer, instead of
/// the generic descriptor invoker which would box the result.
#[test]
fn test_closure_bind_by_reference_stored_in_variable() {
    let out = compile_and_run(
        "<?php
        class C { public array $items = []; }
        $o = new C();
        $bound = \\Closure::bind(fn &() => $this->items, $o, $o);
        $ref = &$bound();
        $ref[] = 'x';
        $ref[] = 'y';
        echo implode(',', $o->items), \"\\n\";
        $ref = [];
        echo count($o->items), \"\\n\";",
    );
    assert_eq!(out, "x,y\n0\n");
}

/// The same variable-stored by-reference `Closure::bind` over a `string` property: the cell
/// pointer survives the call boundary and write-through works both ways.
#[test]
fn test_closure_bind_by_reference_stored_in_variable_string() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        $o = new C();
        $bound = \\Closure::bind(fn &() => $this->s, $o, $o);
        $ref = &$bound();
        $ref = \"changed\";
        echo $o->s, \"\\n\";
        $o->s = \"viaprop\";
        echo $ref, \"\\n\";",
    );
    assert_eq!(out, "changed\nviaprop\n");
}

/// Two locals aliasing the same property both observe a write through either side.
#[test]
fn test_two_locals_aliasing_same_property() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 0; }
        $o = new C();
        $a = &$o->v;
        $b = &$o->v;
        $a = 3;
        echo $b, \"\\n\";
        $b = 8;
        echo $o->v, \"\\n\";",
    );
    assert_eq!(out, "3\n8\n");
}

/// `$b =& $a[0]` aliases an indexed-array int element: writing the local updates the array
/// element in place (write-through from the alias to the array).
#[test]
fn test_ref_alias_array_element_int() {
    let out = compile_and_run(r#"<?php $a = [1, 2]; $b =& $a[0]; $b = 9; echo $a[0];"#);
    assert_eq!(out, "9");
}

/// `$b =& $a[0]` aliases an indexed-array int element: reading the local after the array is
/// mutated through another path reflects the change (write-through from the array to the alias).
#[test]
fn test_ref_alias_array_element_int_readback() {
    let out = compile_and_run(
        r#"<?php $a = [1, 2]; $b =& $a[0]; $a[0] = 7; echo $b;"#,
    );
    assert_eq!(out, "7");
}

/// `$b =& $a[0]` aliases an indexed-array string element: writing the local updates the array
/// element's pointer and length in place.
#[test]
fn test_ref_alias_array_element_string() {
    let out = compile_and_run(
        r#"<?php $a = ["hello", "world"]; $b =& $a[0]; $b = "HEY"; echo $a[0];"#,
    );
    assert_eq!(out, "HEY");
}

/// `$b =& $a[1]` aliases a non-zero indexed-array int element: the address computation must
/// scale by the element size and skip the header correctly.
#[test]
fn test_ref_alias_array_element_nonzero_index() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $b =& $a[1]; $b = 99; echo $a[1];"#,
    );
    assert_eq!(out, "99");
}

/// A reference assignment expression vivifies a static hash entry and returns its aliased value.
#[test]
fn test_ref_assignment_expression_static_hash_element() {
    let out = compile_and_run(
        "<?php
        class C {
            private static array $cache = [];
            public static function read(string $key) {
                if (null !== $value = &self::$cache[$key]) {
                    echo $value;
                    return;
                }
                $value = 7;
            }
        }
        C::read('key');
        C::read('key');",
    );
    assert_eq!(out, "7");
}

/// A reference-vivified static hash keeps the relocated container published after growth.
#[test]
fn test_ref_assignment_expression_static_hash_element_growth_writeback() {
    let out = compile_and_run(
        "<?php
        class C {
            private static array $cache = [];
            public static function remember(string $key, int $newValue): int {
                if (null !== $value = &self::$cache[$key]) {
                    return $value;
                }
                return $value = $newValue;
            }
        }
        for ($i = 0; $i < 40; $i++) {
            C::remember('key-'.$i, $i);
        }
        echo C::remember('key-0', -1), ':', C::remember('key-39', -1);",
    );
    assert_eq!(out, "0:39");
}

/// A static-property post-increment expression returns the old value and stores the increment.
#[test]
fn test_static_property_post_increment_expression() {
    let out = compile_and_run(
        "<?php
        class C {
            private static int $level = 0;
            public static function enter() {
                if (!self::$level++) {
                    echo 'first';
                }
            }
        }
        C::enter();
        C::enter();",
    );
    assert_eq!(out, "first");
}

/// A runtime-named property reference aliases the suffix-constrained declared array slot.
#[test]
fn test_dynamic_property_reference_with_known_suffix() {
    let out = compile_and_run(
        "<?php
        class C {
            private array $beforePasses = [[1]];
            private array $afterPasses = [[3]];
            public function append(string $type, int $value) {
                $property = $type.'Passes';
                $passes = &$this->$property;
                $passes[0][] = $value;
            }
            public function printBefore() {
                echo $this->beforePasses[0][0], $this->beforePasses[0][1];
            }
        }
        $passes = new C();
        $passes->append('before', 2);
        $passes->printBefore();",
    );
    assert_eq!(out, "12");
}

/// A dynamic property reference replaces raw array storage without reading it as a Mixed marker.
#[test]
fn test_dynamic_property_reference_replaces_raw_element_with_array() {
    let out = compile_and_run(
        "<?php
        class C {
            private array $beforePasses = [4];
            public function replace(string $type): void {
                $property = $type.'Passes';
                $passes = &$this->$property;
                $passes[0] = [];
                $passes[0][] = 'ready';
            }
            public function result(): string {
                return $this->beforePasses[0][0];
            }
        }
        $passes = new C();
        $passes->replace('before');
        echo $passes->result();",
    );
    assert_eq!(out, "ready");
}

/// A dynamic-property reference retains distinct signed integer map keys on an empty array.
///
/// The referenced property starts as `[]`, then receives nested appends under positive and
/// negative priorities. These keys must promote the storage to an associative map and keep the
/// dynamic-property reference attached to that same map.
#[test]
fn test_dynamic_property_reference_preserves_signed_priority_map_keys() {
    let out = compile_and_run(
        r#"<?php
class PassStore {
    private array $beforePasses = [];

    public function append(string $type, int $priority, string $pass): void {
        $property = $type.'Passes';
        $passes = &$this->$property;
        if (!isset($passes[$priority])) {
            $passes[$priority] = [];
        }
        $passes[$priority][] = $pass;
    }

    public function values(): array {
        return $this->beforePasses;
    }
}

$store = new PassStore();
$store->append('before', 200, 'first');
$store->append('before', -32, 'second');
$passes = $store->values();
echo $passes[200][0].':'.$passes[-32][0];
"#,
    );
    assert_eq!(out, "first:second");
}

/// A nested array append by reference retains the promoted local cell after its frame returns.
#[test]
fn test_nested_array_reference_append_outlives_source_frame() {
    let out = compile_and_run(
        "<?php
        function append_ref(array &$loops): void {
            $path = [1];
            $loops[0][] = &$path;
            $path[] = 2;
        }
        function print_ref(mixed $loops): void {
            echo $loops[0][0][0], $loops[0][0][1];
        }
        $loops = [[]];
        append_ref($loops);
        print_ref($loops);",
    );
    assert_eq!(out, "12");
}

/// A loop back-edge can promote a local after the nested mutation was lowered. Its next
/// iteration must not retire the referenced value twice when publishing the new container.
#[test]
fn test_nested_array_reference_loop_mutation_preserves_shared_paths() {
    let out = compile_and_run(
        r#"<?php
function createPaths(array &$loops): void {
    $path = ['a', []];
    foreach (['b', 'c'] as $key) {
        $path[1][$key] = true;
        $loops[$key][] = &$path;
    }
}
$loops = [];
createPaths($loops);
echo json_encode($loops);
"#,
    );
    assert_eq!(out, r#"{"b":[["a",{"b":true,"c":true}]],"c":[["a",{"b":true,"c":true}]]}"#);
}

#[test]
fn test_nested_array_reference_loop_mutation_has_single_owner_cleanup() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function createPaths(array &$loops): void {
    $path = ['a', []];
    foreach (['b', 'c'] as $key) {
        $path[1][$key] = true;
        $loops[$key][] = &$path;
    }
}
$loops = [];
createPaths($loops);
echo json_encode($loops);
unset($loops);
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, r#"{"b":[["a",{"b":true,"c":true}]],"c":[["a",{"b":true,"c":true}]]}"#);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

#[test]
fn test_nested_array_reference_loop_mutation_handles_hash_and_dynamic_keys() {
    let out = compile_and_run(
        r#"<?php
function hashPaths(array &$out): void {
    $items = ['slot' => []];
    foreach (['a', 'b'] as $key) {
        $items['slot'][$key] = true;
        $out[] = &$items;
    }
}
function dynamicPaths(array &$out, $index): void {
    $items = ['seed', []];
    foreach (['a', 'b'] as $key) {
        $items[$index][$key] = true;
        $out[] = &$items;
    }
}
$hash = [];
hashPaths($hash);
echo json_encode($hash), "\n";
$dynamic = [];
dynamicPaths($dynamic, 1);
echo json_encode($dynamic);
"#,
    );
    assert_eq!(out, concat!(
        "[{\"slot\":{\"a\":true,\"b\":true}},{\"slot\":{\"a\":true,\"b\":true}}]\n",
        "[[\"seed\",{\"a\":true,\"b\":true}],[\"seed\",{\"a\":true,\"b\":true}]]",
    ));
}

/// Escaping paths are mutated inside a recursive walk and destructured after their frame returns.
#[test]
fn test_nested_array_reference_loop_mutation_recursive_paths() {
    let out = compile_and_run(
        r#"<?php
function visit(string $source, array $graph, array &$checked, array &$loops = [], array $path = []): void {
    $path[$source] = true;
    $checked[$source] = true;
    foreach ($graph[$source] as $id) {
        if (isset($path[$id])) {
            $loop = null;
            $pathInLoop = [$id, []];
            foreach ($path as $key => $byConstructor) {
                if (null !== $loop) {
                    $loop[] = $key;
                    $pathInLoop[1][$key] = $byConstructor;
                    $loops[$key][] = &$pathInLoop;
                } elseif ($key === $id) {
                    $loop = [];
                }
            }
        } elseif (!isset($checked[$id])) {
            visit($id, $graph, $checked, $loops, $path);
        } elseif (isset($loops[$id])) {
            foreach ($loops[$id] as [$first, $loopPath]) {
                if (!isset($path[$first])) { continue; }
                $fillPath = null;
                foreach ($loopPath as $key => $byConstructor) {
                    if (null !== $fillPath) { $fillPath[$key] = $byConstructor; }
                    elseif ($key === $id) { $fillPath = $path; $fillPath[$key] = $byConstructor; }
                }
                $loop = null;
                foreach ($fillPath as $key => $byConstructor) {
                    if (null !== $loop) { $loop[] = $key; }
                    elseif ($key === $first) { $loop = []; }
                }
                echo 'reuse:', $first, ':', implode(',', $loop), "\n";
                break;
            }
        }
    }
    unset($path[$source]);
}
$graph = ['a' => ['b', 'd'], 'b' => ['c'], 'c' => ['a'], 'd' => ['b']];
$checked = [];
$loops = [];
visit('a', $graph, $checked, $loops);
echo json_encode($loops);
"#,
    );
    assert_eq!(out, concat!(
        "reuse:a:d,b,c\n",
        "{\"b\":[[\"a\",{\"b\":true,\"c\":true}]],\"c\":[[\"a\",{\"b\":true,\"c\":true}]]}",
    ));
}

/// A local array element bound to a local variable shares one promoted cell in both directions;
/// unsetting the local alias leaves the array-owned marker alive and writable.
#[test]
fn test_local_array_element_reference_assignment_writes_through() {
    let out = compile_and_run(
        "<?php
        $values = [1];
        $stub = 5;
        $values[0] = &$stub;
        $stub = 7;
        echo $values[0];
        $values[0] = 8;
        echo $stub;
        unset($stub);
        $values[0] = 9;
        echo $values[0];",
    );
    assert_eq!(out, "789");
}

/// A nested associative element retains a local reference cell in
/// both directions, including dynamically concatenated keys and an autovivified parent hash.
#[test]
fn test_nested_assoc_array_element_reference_assignment_writes_through() {
    let out = compile_and_run(
        "<?php
        $data = [];
        $prefix = 'p:';
        $key = 'item';
        $value = 5;
        $data[$prefix.'use']['$'.$key] = &$value;
        $value = 7;
        echo $data['p:use']['$item'];
        $data['p:use']['$item'] = 9;
        echo $value;",
    );
    assert_eq!(out, "79");
}

/// A static-property hash element bound by reference observes a later scalar write through its alias.
#[test]
fn test_ref_static_property_array_element_scalar_write_through() {
    let out = compile_and_run(
        "<?php
        class C { public static array $a = []; }
        C::$a['source'] = 1;
        C::$a['alias'] = &C::$a['source'];
        C::$a['alias'] = 9;
        echo C::$a['source'];",
    );
    assert_eq!(out, "9");
}

/// The DebugClassLoader-shaped heterogeneous array entry remains shared after reference binding.
#[test]
fn test_ref_static_property_array_element_nested_array_alias() {
    let out = compile_and_run(
        "<?php
        class C { public static array $a = []; }
        C::$a['source'] = ['X', []];
        C::$a['alias'] = &C::$a['source'];
        C::$a['source'][0] = 'MUT';
        echo C::$a['source'][0], C::$a['alias'][0];",
    );
    assert_eq!(out, "MUTMUT");
}

/// An untyped by-reference parameter materializes an undefined caller variable as writable
/// mixed storage before the call and exposes the assigned value afterwards.
#[test]
fn test_undefined_variable_can_be_initialized_through_by_ref_parameter() {
    let out = compile_and_run(
        "<?php
        function initialize(&$destination): void {
            $destination = ['ready'];
        }
        initialize($result);
        echo $result[0];",
    );
    assert_eq!(out, "ready");
}

/// By-reference output discovery follows callable signatures for named arguments as well as
/// positional calls, without relying on a particular function name.
#[test]
fn test_named_by_ref_argument_can_initialize_undefined_variable() {
    let out = compile_and_run(
        "<?php
        function initialize_named($prefix, &$destination): void {
            $destination = $prefix.'done';
        }
        initialize_named(destination: $result, prefix: 'all-');
        echo $result;",
    );
    assert_eq!(out, "all-done");
}

/// A nullable declared by-reference parameter widens an existing compatible array local to the
/// full writable parameter storage contract before a static method call.
#[test]
fn test_nullable_array_by_ref_parameter_accepts_existing_array_variable() {
    let out = compile_and_run(
        "<?php
        final class OutputWriter {
            public static function populate(?array &$destination = null): void {
                $destination = ['ready'];
            }
        }
        $result = [];
        OutputWriter::populate($result);
        echo $result[0];",
    );
    assert_eq!(out, "ready");
}

/// A static call through `self` creates writable caller storage for an undefined variable passed
/// to a declared nullable by-reference parameter.
#[test]
fn test_self_static_by_ref_call_initializes_undefined_variable() {
    let out = compile_and_run(
        "<?php
        final class OutputWriter {
            public static function populate(?array &$destination = null): void {
                $destination = ['ready'];
            }
            public static function render(): string {
                self::populate($result);
                return $result[0];
            }
        }
        echo OutputWriter::render();",
    );
    assert_eq!(out, "ready");
}

/// A by-reference output produced by the right side of `&&` is available in the true branch.
#[test]
fn test_short_circuit_by_ref_output_is_visible_in_true_branch() {
    let out = compile_and_run(
        "<?php
        final class OutputWriter {
            public static function populate(?array &$destination = null): int {
                $destination = ['ready'];
                return 1;
            }
            public static function render(): string {
                if (true && self::populate($result)) {
                    return $result[0];
                }
                return 'missing';
            }
        }
        echo OutputWriter::render();",
    );
    assert_eq!(out, "ready");
}

/// A successful null probe publishes a branch-local fact for an output that is created on only
/// one side of an earlier short-circuit expression.
#[test]
fn test_isset_guards_conditionally_initialized_by_ref_output() {
    let out = compile_and_run(
        "<?php
        final class OutputWriter {
            public static function populate(?array &$destination = null): int {
                $destination = ['value' => 'ready'];
                return 1;
            }
            public static function render(bool $lead, bool $skip): void {
                if ($lead && ($skip || self::populate($result))) {
                    if (isset($result['value']) && $result['value'] === 'ready') {
                        echo $result['value'];
                    }
                }
            }
        }
        OutputWriter::render(true, false);",
    );
    assert_eq!(out, "ready");
}

/// An unresolved direct call throws before evaluating its arguments, so an undefined variable
/// argument does not cause a stale follow-on diagnostic in code after the non-returning call.
#[test]
fn test_late_bound_call_undefined_argument_does_not_cascade() {
    let out = compile_and_run(
        "<?php
        try {
            unavailable_runtime_function('key', $result);
            echo $result;
        } catch (Error $error) {
            echo 'caught';
        }",
    );
    assert_eq!(out, "caught");
}

/// Capturing an undefined local by reference creates a writable null cell in the outer scope.
#[test]
fn test_by_ref_closure_capture_initializes_undefined_variable() {
    let out = compile_and_run(
        "<?php
        $writer = static function () use (&$result): void {
            $result = 'ready';
        };
        $writer();
        echo $result;",
    );
    assert_eq!(out, "ready");
}

/// A by-reference array parameter the body writes string keys into holds a hash in the CALLER's
/// cell, and an element assignment whose right-hand side grows that array by reference must
/// write into the grown array. Both went wrong in Symfony's recursive
/// `DefinitionErrorExceptionPass::isErrorForRuntime(string $id, array &$visitedIds)`: the next
/// entry re-promoted the hash as a list, and the write went into the block the call had replaced.
#[test]
fn test_by_ref_array_param_promoted_to_hash_survives_recursion_and_growth() {
    let out = compile_and_run(
        r#"<?php
function rec(int $d, array &$v): bool {
    $v['d' . $d] = true;
    if ($d > 0) {
        $v['r' . $d] = rec($d - 1, $v);
    }
    return true;
}
function grow(array &$a, int $d): int {
    for ($i = 0; $i < 20; $i++) { $a["k$d-$i"] = $i; }
    return $d;
}
function memo(array &$a): void { $a['y'] ?? $a['y'] = grow($a, 7); }
$v = [];
rec(30, $v);
$a = [];
$a['x'] = grow($a, 5);
$c = [];
memo($c);
echo count($v), ' ', count($a), ' ', $a['x'], ' ', count($c), ' ', $c['y'];
"#,
    );
    assert_eq!(out, "61 21 5 21 7");
}


/// A by-reference parameter forwarded into a widened by-reference parameter is widened too, so
/// both sides agree the shared cell is boxed. `ContainerBuilder::doGet(?array &$inline)` forwards
/// its cell to `createService(array &$inline)` exactly like this.
#[test]
fn test_forwarded_by_ref_array_param_follows_the_callee_widening() {
    let out = compile_and_run(
        r#"<?php
function writer(array &$a, string $id): void { $a[$id] = strtoupper($id); }
function forwarder(?array &$a, string $id): void { $a ??= []; writer($a, $id); }
function outer(?array &$a): void { forwarder($a, 'x'); forwarder($a, 'y'); }
$v = null;
outer($v);
echo json_encode($v);
"#,
    );
    assert_eq!(out, r#"{"x":"X","y":"Y"}"#);
}

/// A concrete by-reference parameter forwarded into a by-reference parameter whose DECLARED type
/// is already boxed (`?array &`) is widened too. Without it the callee read the caller's raw
/// array pointer as a box and raised "Value must be of type array, int given" -- Symfony's
/// `createService(array &$inlineServices)` recursing into `doGet(?array &$inlineServices)`.
#[test]
fn test_forwarded_by_ref_array_param_into_declared_nullable_cell() {
    let out = compile_and_run(
        r#"<?php
function nullable(?array &$cell): void { $cell[] = count($cell ?? []); }
function forward(array &$cell): void { nullable($cell); nullable($cell); }
$c = [];
forward($c);
echo json_encode($c);
"#,
    );
    assert_eq!(out, "[0,1]");
}

/// An UNDEFINED variable passed by reference is created by the call, and the callee may vivify it
/// with an element write; destructuring a null element afterwards is silent. Symfony's
/// `PhpDumper::dump()` calls `addServices($services)` with a fresh `$services`, which
/// `addServices(?array &$services = null)` fills with `$services[$id] = …`, then destructures
/// `[$file, $code] = $services[$id]` where a synthetic service stored null.
#[test]
fn test_undefined_by_ref_argument_is_vivified_through_its_cell() {
    let out = compile_and_run(
        r#"<?php
function add(?array &$services = null): string {
    foreach (['b' => 1, 'a' => 0] as $id => $kind) {
        $services[$id] = $kind ? [null, "code-$id"] : null;
    }
    $out = '';
    foreach (['a', 'b'] as $id) {
        if (!([$file, $code] = $services[$id]) || null !== $file) {
            continue;
        }
        $out .= $code;
    }
    return $out;
}
function untyped(&$s): void { $s['k'] = 1; }
echo add($services), ' ', json_encode($services), "\n";
untyped($fresh);
echo json_encode($fresh);
"#,
    );
    assert_eq!(out, "code-b {\"b\":[null,\"code-b\"],\"a\":null}\n{\"k\":1}");
}

/// A by-reference parameter a closure binds with `use (&$param)` shares one boxed cell with
/// every caller, and an OMITTED one gets a cell that outlives the call. The closure's write used
/// to store a box the caller read as a raw array, and the omitted default lived in the caller's
/// frame -- Symfony's `doResolveServices(mixed $value, array &$inlineServices = [])` returning a
/// `RewindableGenerator` over such a closure that runs long after the call.
#[test]
fn test_by_ref_param_captured_by_closure_shares_and_outlives_its_cell() {
    let out = compile_and_run(
        r#"<?php
function writes(array &$inline): string {
    $f = function () use (&$inline) { $inline['k'] = 1; return 'w'; };
    return $f() . count($inline);
}
final class Maker {
    public function mk(array &$inline = []): Closure {
        return function (string $k) use (&$inline) {
            $inline[$k] = strtoupper($k);
            return count($inline);
        };
    }
    public function outer(): Closure { return $this->mk(); }
}
function clobber(): string { return str_repeat(' ', 64) . strtolower('XX'); }
$x = [1];
echo writes($x), json_encode($x), "\n";
$f = (new Maker())->outer();
clobber();
echo $f('a'), $f('b'), "\n";
"#,
    );
    assert_eq!(out, "w2{\"0\":1,\"k\":1}\n12\n");
}

/// A by-reference parameter returned after a builtin mutated it keeps the caller's variable
/// alive whether the caller keeps the result or discards it.
#[test]
fn test_returned_by_ref_parameter_survives_a_discarded_result() {
    let out = compile_and_run(
        r#"<?php
function f1(array &$t) { $t['x'] = 1; return $t; }
function f2(array &$t) { sort($t); return $t; }
function f3(&$t) { sort($t); return $t; }
$e = [];
f1($e);
echo json_encode($e), "\n";
$g = [3, 1];
f2($g);
echo json_encode($g), "\n";
$h = [3, 1];
f3($h);
echo json_encode($h), "\n";
$i = [5, 4];
$r = f2($i);
echo json_encode($i), json_encode($r), "\n";
"#,
    );
    assert_eq!(out, "{\"x\":1}\n[1,3]\n[1,3]\n[4,5][4,5]\n");
}

/// An element of a gradual by-reference container, and a property named at run time, pass by
/// reference: the callee's writes reach the caller's storage, a missing element is created
/// silently, and a recursive dot-path setter (Laravel's `data_set` shape) builds nested arrays.
#[test]
fn test_by_ref_elements_of_gradual_containers_and_dynamic_properties() {
    let out = compile_and_run(
        r#"<?php
if (! function_exists('path_set')) {
    function path_set(&$target, $key, $value)
    {
        $segments = is_array($key) ? $key : explode('.', $key);
        $segment = array_shift($segments);
        if (is_array($target)) {
            if ($segments) {
                if (! array_key_exists($segment, $target)) {
                    $target[$segment] = [];
                }
                path_set($target[$segment], $segments, $value);
            } else {
                $target[$segment] = $value;
            }
        } elseif (is_object($target)) {
            if ($segments) {
                if (! isset($target->{$segment})) {
                    $target->{$segment} = [];
                }
                path_set($target->{$segment}, $segments, $value);
            } else {
                $target->{$segment} = $value;
            }
        }
        return $target;
    }
}
function put(&$t, $k, $v) { $t[$k] = $v; }
function nest(&$t, $keys, $v) { $k = array_shift($keys); if ($keys) { nest($t[$k], $keys, $v); } else { $t[$k] = $v; } }
$a = ['x' => ['y' => 1]];
path_set($a, 'x.z', 2);
echo json_encode($a), "\n";
$o = new stdClass();
path_set($o, 'p.q', 3);
echo json_encode($o), "\n";
$b = [];
put($b, 'k', 'v');
nest($b, ['m', 'n'], 4);
echo json_encode($b), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"x\":{\"y\":1,\"z\":2}}\n{\"p\":{\"q\":3}}\n{\"k\":\"v\",\"m\":{\"n\":4}}\n"
    );
}

/// Keeps a captured reference and its originally typed array on the same boxed storage representation.
#[test]
fn test_ref_alias_array_element_capture_retyping() {
    let out = compile_and_run(r#"<?php
$a = [1, 2];
$copy = $a;
$b =& $a[0];
$set = function (mixed $value) use (&$b): void { $b = $value; };
$set(9);
echo $a[0], ":", $b, "\n";
$set("updated");
echo $a[0], ":", $b, "\n";
$set(null);
echo is_null($a[0]) ? "null" : "wrong", ":", is_null($b) ? "null" : "wrong", "\n";
echo $copy[0], ":", $copy[1], "\n";
"#);
    assert_eq!(out, "9:9\nupdated:updated\nnull:null\n1:2\n");
}

/// An object property may be passed to a user method's by-reference parameter.
///
/// The call is lowered as a hidden temporary plus a write-back, so the callee's mutation
/// reaches the property without evaluating the receiver twice.
#[test]
fn test_property_argument_binds_to_method_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public function __construct(public Value $value) {} }
class Ops {
    public function bump(Value &$value): void {
        $value = new Value($value->number + 1);
    }
    public function swap(Value &$left, Value &$right): void {
        $temp = $left;
        $left = $right;
        $right = $temp;
    }
}
$first = new Box(new Value(1));
$second = new Box(new Value(3));
$ops = new Ops();
$ops->bump($first->value);
$ops->swap($first->value, $second->value);
echo $first->value->number, ",", $second->value->number, "\n";
"#,
    );
    assert_eq!(out, "3,2\n");
}

/// A constructor by-reference parameter accepts an object property place too.
#[test]
fn test_property_argument_binds_to_constructor_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public Value $value; }
class Reset { public function __construct(Value &$value) { $value = new Value(9); } }
$box = new Box();
$box->value = new Value(1);
new Reset($box->value);
echo $box->value->number, "\n";
"#,
    );
    assert_eq!(out, "9\n");
}

/// A union holding the declared class plus `false`/`null` is guarded at the call boundary.
#[test]
fn test_object_union_arguments_throw_type_error_for_non_object_alternative() {
    let out = compile_and_run(
        r#"<?php
class Marker {}
class Provider {
    public function absent(): Marker|false { return false; }
    public function nullValue(): ?Marker { return null; }
    public function present(): Marker|false { return new Marker(); }
}
class Sink {
    public function __construct(Marker $value) {}
    public function take(Marker $value): string { return "taken"; }
}
$sink = new Sink(new Marker());
$provider = new Provider();
try {
    $sink->take($provider->absent());
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'Sink::take(): Argument #1 ($value) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
try {
    $sink->take($provider->nullValue());
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'Sink::take(): Argument #1 ($value) must be of type Marker, null given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
try {
    new Sink($provider->absent());
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'Sink::__construct(): Argument #1 ($value) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
echo $sink->take($provider->present()), "\n";
"#,
    );
    assert_eq!(
        out,
        "prefix-ok:line-ok\nprefix-ok:line-ok\nprefix-ok:line-ok\ntaken\n"
    );
}

/// Guards a union-typed local even after its storage widened to a boxed Mixed cell.
///
/// The checker accepted `Marker|false`, but lowering saw only Mixed and skipped the guard,
/// then passed `false` as a raw object pointer. The catch block must also stay reachable:
/// the callee is pure, so the exception analysis has to treat the guard as a throw site.
#[test]
fn test_object_union_locals_throw_catchable_type_error() {
    let out = compile_and_run(
        r#"<?php
class Marker {}
function take(Marker $m): string { return "taken"; }
$w = $argc > 5 ? new Marker() : false;
try {
    take($w);
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'take(): Argument #1 ($m) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
$v = (function (): Marker|false { return false; })();
try {
    take($v);
} catch (TypeError $error) {
    echo str_starts_with($error->getMessage(), 'take(): Argument #1 ($m) must be of type Marker, false given, called in ') ? "prefix-ok\n" : $error->getMessage();
}
$ok = $argc < 5 ? new Marker() : false;
echo take($ok), "\n";
"#,
    );
    assert_eq!(out, "prefix-ok:line-ok
prefix-ok
taken
");
}

/// A nullsafe method call rewrites object-property by-reference arguments in its call branch.
#[test]
fn test_property_argument_binds_through_nullsafe_method_call() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public function __construct(public Value $value) {} }
class Ops {
    public function swap(Value &$left, Value &$right): void {
        $temp = $left;
        $left = $right;
        $right = $temp;
    }
}
$ops = new Ops();
$first = new Box(new Value(1));
$second = new Box(new Value(2));
$ops?->swap($first->value, $second->value);
echo $first->value->number, ",", $second->value->number, "\n";
"#,
    );
    assert_eq!(out, "2,1\n");
}

/// Scalar, string, array, and mixed properties bind by reference through a method call.
#[test]
fn test_scalar_and_array_property_arguments_bind_by_reference() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $number = 1;
    public string $label = "a";
    public array $items = [1];
    public mixed $mixed = 0;
}
class Ops {
    public function bump(int &$number): void { $number++; }
    public function append(string &$label): void { $label .= "!"; }
    public function addItem(array &$items): void { $items[] = 9; }
    public function setMixed(mixed &$mixed): void { $mixed = "x"; }
}
$box = new Box();
$ops = new Ops();
$ops->bump($box->number);
$ops->append($box->label);
$ops->addItem($box->items);
$ops->setMixed($box->mixed);
echo $box->number, ",", $box->label, ",", count($box->items), ",", $box->mixed, "\n";
"#,
    );
    assert_eq!(out, "2,a!,2,x\n");
}

/// Union and nullable properties bind by reference with their runtime type preserved.
#[test]
fn test_union_and_nullable_property_arguments_bind_by_reference() {
    let out = compile_and_run(
        r#"<?php
class Box { public int|string $value = 1; public ?int $maybe = 1; }
class Ops {
    public function setUnion(int|string &$value): void { $value = "x"; }
    public function setNullable(?int &$value): void { $value = 5; }
}
$box = new Box();
$ops = new Ops();
$ops->setUnion($box->value);
$ops->setNullable($box->maybe);
var_dump($box->value, $box->maybe);
"#,
    );
    assert_eq!(out, "string(1) \"x\"\nint(5)\n");
}

/// A free function accepts an object property as a by-reference argument.
#[test]
fn test_property_argument_binds_to_free_function_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $number = 1; }
function bump(int &$number): void { $number++; }
$box = new Box();
bump($box->number);
echo $box->number, "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// A static property binds to a static method's by-reference parameter.
#[test]
fn test_static_property_argument_binds_to_static_method_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Box { public static int $number = 1; public static array $items = [1]; }
class Ops {
    public static function bump(int &$number): void { $number++; }
    public static function addItem(array &$items): void { $items[] = 9; }
}
Ops::bump(Box::$number);
Ops::addItem(Box::$items);
echo Box::$number, ",", count(Box::$items), "\n";
"#,
    );
    assert_eq!(out, "2,2\n");
}

/// By-reference variadics accept scalar, string, mixed, object, static, union, and nullable
/// property places through the same hidden write-back.
#[test]
fn test_variadic_by_ref_property_arguments() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box {
    public int $integer = 1;
    public string $label = "a";
    public mixed $mixed = 0;
    public Value $value;
    public int|string $union = 1;
    public ?int $nullable = 1;
    public static int $static = 1;
}
function collect(&...$refs): void {
    $refs[0] = 9;
    $refs[1] = "b";
    $refs[2] = "x";
    $refs[3] = new Value(7);
    $refs[4] = "u";
    $refs[5] = 5;
    $refs[6] = 6;
}
$box = new Box();
$box->value = new Value(1);
collect(
    $box->integer,
    $box->label,
    $box->mixed,
    $box->value,
    $box->union,
    $box->nullable,
    Box::$static,
);
var_dump($box->integer, $box->label, $box->mixed, $box->value->number, $box->union, $box->nullable, Box::$static);
"#,
    );
    assert_eq!(
        out,
        "int(9)\nstring(1) \"b\"\nstring(1) \"x\"\nint(7)\nstring(1) \"u\"\nint(5)\nint(6)\n"
    );
}

/// Rejects a variadic by-reference write that violates the target property's declared type.
///
/// The callee receives boxed Mixed ref cells, so the write-back must apply php-src's typed
/// property check before unboxing; otherwise an int assigned through a `Value` property is
/// stored as a raw object pointer.
#[test]
fn test_by_reference_variadic_property_writeback_rejects_mismatched_types() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public Value $v; public int $n = 0; function __construct() { $this->v = new Value(0); } }
function setIdx(&...$refs): void { $refs[0] = 7; }
function setStr(&...$refs): void { $refs[0] = "abc"; }
function setVal(&...$refs): void { $refs[0] = new Value(5); }
function setInt(&...$refs): void { $refs[0] = 7; }
$b = new Box();
try { setIdx($b->v); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$d = new Box();
try { setStr($d->n); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
setVal($b->v); echo $b->v->number, "\n";
setInt($d->n); echo $d->n, "\n";
"#,
    );
    assert_eq!(
        out,
        "Cannot assign int to reference held by property Box::$v of type Value\n\
Cannot assign string to reference held by property Box::$n of type int\n\
5\n\
7\n"
    );
}

/// Writes back through property receivers nested below another property or an array element.
#[test]
fn test_by_reference_nested_property_receivers_write_back() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; }
class Outer { public Box $box; function __construct() { $this->box = new Box(); } }
function inc(int &$x): void { $x++; }
$o = new Outer(); inc($o->box->n); echo $o->box->n, "\n";
$arr = [new Box()]; inc($arr[0]->n); echo $arr[0]->n, "\n";
"#,
    );
    assert_eq!(out, "2\n2\n");
}

/// Verifies property references bind at their source position and honor later property writes.
#[test]
fn test_by_reference_property_source_order_and_callee_mutation() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; }
$holder = null;
function noise(): int { global $holder; $holder->n = 50; return 2; }
function g(int $a, int &$b): void { $b = $b + 1; }
$box = new Box(); $holder = $box;
g(noise(), $box->n);
echo $box->n, "\n";

class Bump { public int $n = 1; public function bump(int &$v): void { $v = 9; $this->n = 7; } }
$bump = new Bump(); $bump->bump($bump->n);
echo $bump->n, "\n";

class Grab { public int $v = 5; }
function &grab(int &$x): int { return $x; }
$grab = new Grab(); $r = &grab($grab->v); $r = 99;
echo $grab->v, "\n";
"#,
    );
    assert_eq!(out, "51\n7\n99\n");
}

/// Verifies a numeric string outside the i64 range is rejected by a typed property reference.
#[test]
fn test_by_reference_property_writeback_rejects_out_of_range_numeric_strings() {
    let out = compile_and_run(
        r#"<?php
class C { public int $n = 0; }
$payload = null;
function set(&...$refs): void { global $payload; $refs[0] = $payload; }
$c = new C();
foreach (["42", "999999999999999999999999", "9223372036854775807", "-9223372036854775809", 1e30] as $v) {
    $payload = $v;
    try { set($c->n); echo "ok ", $c->n, "\n"; }
    catch (Throwable $e) { echo "err ", $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "ok 42\n\
err Cannot assign string to reference held by property C::$n of type int\n\
ok 9223372036854775807\n\
ok -9223372036854775808\n\
err Cannot assign float to reference held by property C::$n of type int\n"
    );
}

/// A by-value string parameter passed on to a `string &` parameter is overwritten by the inner
/// callee, which releases the value it replaces. That value is the caller's argument, held by
/// the frame only as a borrow, so the outer frame must own its parameter first. Without the
/// prologue retain the replaced string was freed twice (Symfony's `ErrorHandler::handleError()`
/// passing `$file` to `cleanTrace(..., string &$file, ...)` died under `lint:container`).
#[test]
fn test_by_value_parameter_passed_by_reference_is_owned_by_its_frame() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function rewrite(string &$file): void {
    $file = strtoupper($file) . '!';
}
function handle(string $file, $cb): string {
    rewrite($file);
    $cb($file);
    return $file;
}
$cb = static function ($f = null) {};
for ($i = 0; $i < 3; $i++) {
    echo handle("a$i.php", $cb), "|";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "A0.PHP!|A1.PHP!|A2.PHP!|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A later `($this->cb)($file)` may bind `$file` by reference, so the checker authorizes boxing
/// it, and that widens the slot to `Mixed` for the whole frame — including an EARLIER
/// `$this->rewrite($file)` into a `string &` parameter, which then received the boxed slot as a
/// raw string cell and printed empty strings. The earlier call must go through the gradual
/// adapter (a temporary of the declared type, written back after the call).
#[test]
fn test_by_reference_argument_before_its_boxing_promotion_keeps_the_declared_cell() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class H {
    public $cb;
    public function __construct() { $this->cb = static function ($f = null) {}; }
    private function rewrite(string &$file): void {
        $file = strtoupper($file) . '!';
    }
    public function handle(string $file): string {
        $this->rewrite($file);
        ($this->cb)($file);
        return $file;
    }
}
$h = new H();
for ($i = 0; $i < 3; $i++) {
    echo $h->handle("a$i.php"), "|";
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "A0.PHP!|A1.PHP!|A2.PHP!|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A by-reference parameter handed on to another by-reference callable. What the parameter holds
/// is the caller's address: inside the Mixed box of a promoted local, inside an object. The marker
/// retained and released it as if it were an allocation, which rewrote the box's header, then ran
/// a hash release on it. Symfony's `AbstractAdapter::doGet()` forwards `bool &$save` this way, and
/// every cache miss took the worker down.
#[test]
fn test_a_forwarded_by_ref_parameter_is_not_owned_by_its_marker() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Item { public bool $flag = true; }
class P {
    public function go(callable $c) {
        return $this->run(function (bool &$s) use ($c) { return $c($s); });
    }
    public function goText(callable $c) {
        return $this->runText(function (string &$s) use ($c) { return $c($s); });
    }
    private function run(callable $cb) { $s = true; $r = $cb($s); var_dump($s); return $r; }
    private function runText(callable $cb) { $s = 'a'; $r = $cb($s); var_dump($s); return $r; }
}
echo (new P())->go(function (bool &$s) { $s = false; return 'bool'; }), "\n";
echo (new P())->goText(function (string &$s) { $s .= 'b'; return 'text'; }), "\n";
function forward(bool &$p, callable $c) { return $c($p); }
$item = new Item();
echo forward($item->flag, function (bool &$v) { $v = false; return 'prop'; }), "\n";
var_dump($item->flag);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool\nstring(2) \"ab\"\ntext\nprop\nbool(false)\n"
    );
}

/// A typed by-reference parameter bound to an element whose reference cell owns a boxed Mixed
/// writes a raw value, so it must land inside the box and not over the box pointer. Reading the
/// element back then jumped through 7.
#[test]
fn test_a_typed_by_ref_parameter_writes_inside_the_elements_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function setInt(int &$p) { $p = 7; }
function setText(string &$p) { $p .= 'b'; }
function forward(int &$p, callable $c) { return $c($p); }
$h = ['k' => 1];
setInt($h['k']);
var_dump($h);
$t = ['k' => 'a'];
setText($t['k']);
var_dump($t);
$m = ['k' => 1];
echo forward($m['k'], function (int &$v) { $v = 9; return 'z'; }), "\n";
echo $m['k'], "\n";
$c = function (int &$v) { $v = 4; };
$c($h['k']);
echo $h['k'], "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(1) {\n  [\"k\"]=>\n  int(7)\n}\narray(1) {\n  [\"k\"]=>\n  string(2) \"ab\"\n}\nz\n9\n4\n"
    );
}

/// A write made before the callee throws has already reached the caller's variable, which a
/// bridge copying the value back after the call would lose.
#[test]
fn test_a_forwarded_by_ref_parameter_keeps_a_write_made_before_a_throw() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function forward(int &$p, callable $c) { return $c($p); }
$n = 1;
try {
    forward($n, function (int &$v) { $v = 2; throw new Exception('stop'); });
} catch (Exception $e) {
    var_dump($n);
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(2)\n");
}

/// An object built by name, by reflection or by cloning such a prototype takes the path that
/// parks the object on the stack, and that path wrote each default over the slot of a property
/// held in a reference cell. The first read then followed the value as a cell. Doctrine's cached
/// `ClassMetadata` is rebuilt this way, so its first entity load took the worker down.
#[test]
fn test_a_default_of_a_cell_backed_property_survives_a_dynamic_allocation() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class M {
    public array $list = [];
    public bool $flag = false;
    public int $n = 3;
    public ?string $s = null;
    public array $map = ['k' => 1];
}
function bind(M $m) { $a = &$m->list; $b = &$m->flag; $c = &$m->n; $d = &$m->s; $e = &$m->map; $a[] = 1; }
function show($o) {
    echo count($o->list), var_export($o->flag, true), $o->n, var_export($o->s, true), $o->map['k'], "\n";
    foreach ($o->list as $k => $v) { echo $k, "\n"; }
}
$name = 'M';
show(new $name());
$byReflection = (new ReflectionClass($name))->newInstanceWithoutConstructor();
show($byReflection);
show(clone $byReflection);
$o = new M();
bind($o);
show($o);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "0false3NULL1\n0false3NULL1\n0false3NULL1\n1false3NULL1\n0\n");
}

/// `ReflectionProperty::setValue()` initializes a readonly property from the property's own scope.
/// Its body is compiled in `ReflectionProperty`, which the readonly guard took for the caller's
/// scope, so Doctrine could not hydrate `App\\Entity\\Tag::$name`.
#[test]
fn test_reflection_initializes_a_readonly_property_from_its_own_scope() {
    let out = compile_and_run(
        r#"<?php
class T { public readonly string $name; private readonly int $n; public function n() { return $this->n; } }
$o = (new ReflectionClass('T'))->newInstanceWithoutConstructor();
$name = new ReflectionProperty('T', 'name');
$name->setValue($o, 'x');
(new ReflectionProperty('T', 'n'))->setValue($o, 5);
echo $o->name, $o->n(), "\n";
try { $name->setValue($o, 'y'); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "x5\nCannot modify readonly property T::$name\n");
}

/// An element write creates the array in a typed property that is still uninitialized, whatever
/// the slot's storage. A property boxed as `mixed` was read first, and the read refused it:
/// Doctrine's `$this->table['name'] = ...` on a new `ClassMetadata` failed every mapping lookup.
#[test]
fn test_an_element_write_initializes_a_typed_boxed_property() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C {
    public mixed $m;
    public mixed $l;
    public int $i;
    public function fill() { $this->m['name'] = strtolower('N'); $this->l[] = 2; return $this; }
    public function read() { return $this->m['name']; }
}
$c = (new C())->fill();
var_dump($c->m, $c->l);
try { (new C())->read(); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(1) {\n  [\"name\"]=>\n  string(1) \"n\"\n}\narray(1) {\n  [0]=>\n  int(2)\n}\nTyped property C::$m must not be accessed before initialization\n"
    );
}

/// `IteratorIterator` accepts any `Traversable` and follows an aggregate to its iterator, also
/// through `parent::__construct()`. Symfony's form tree walks an `OrderedHashMap` that way.
#[test]
fn test_iterator_iterator_follows_an_aggregate_through_its_subclass() {
    let out = compile_and_run(
        r#"<?php
class Map implements IteratorAggregate {
    public function getIterator(): Traversable { return new ArrayIterator(['a' => 1, 'b' => 2]); }
}
class Outer implements IteratorAggregate {
    public function getIterator(): Traversable { return new Map(); }
}
class Walk extends IteratorIterator {
    public function __construct(Traversable $t) { parent::__construct($t); }
}
foreach (new Walk(new Map()) as $k => $v) { echo $k, $v, "\n"; }
foreach (new Walk(new Outer()) as $k => $v) { echo $k, $v, "\n"; }
foreach (new IteratorIterator(new Map()) as $k => $v) { echo $k, $v, "\n"; }
"#,
    );
    assert_eq!(out, "a1\nb2\na1\nb2\na1\nb2\n");
}

/// A return type's `string` arm is the last resort: an object another member accepts stays an
/// object, also when that member is its parent. The comparison was by class name, so a subclass
/// with `__toString()` came back as its string form and Doctrine wrote an AST dump into its SQL.
#[test]
fn test_a_return_union_keeps_a_subclass_object_over_its_string_arm() {
    let out = compile_and_run(
        r#"<?php
abstract class Node { public function __toString(): string { return 'dump'; } }
interface Marker {}
class Path extends Node implements Marker {}
class Plain { public function __toString(): string { return 'plain'; } }
function path(): Path { return new Path(); }
function byParent(): Node|string { return path(); }
function nullable(): Node|string|null { return new Path(); }
function byInterface(): Marker|string { return new Path(); }
function exact(): Path|string { return new Path(); }
function fallback(): Node|string { return new Plain(); }
function show($v) { echo is_string($v) ? 'string ' . $v : 'object ' . get_class($v), "\n"; }
show(byParent());
show(nullable());
show(byInterface());
show(exact());
show(fallback());
"#,
    );
    assert_eq!(out, "object Path\nobject Path\nobject Path\nobject Path\nstring plain\n");
}


/// A property handed to a by-reference constructor parameter is bound, not copied and written back:
/// the object keeps the reference after the call. Symfony's `OrderedHashMap` passes its cursor table
/// this way, and each iterator removes itself from it in `__destruct()`.
#[test]
fn test_a_constructor_keeps_a_reference_to_a_property_argument() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Cursor {
    public function __construct(private array &$cursors, private int &$total) { $this->cursors[] = 1; }
    public function bump() { $this->total = 5; }
    public function __destruct() { array_splice($this->cursors, 0, 1); }
}
class Plain {
    private array $list;
    public function __construct(array &$list) { $this->list = &$list; }
    public function add() { $this->list[] = 1; }
}
class Map {
    private array $cursors = [];
    private int $total = 1;
    private array $items = [];
    public function cursor() { return new Cursor($this->cursors, $this->total); }
    public function plain() { return new Plain($this->items); }
    public function counts() { return count($this->cursors) . ':' . $this->total . ':' . count($this->items); }
}
$map = new Map();
$cursor = $map->cursor();
$plain = $map->plain();
echo $map->counts(), "\n";
$cursor->bump();
$plain->add();
echo $map->counts(), "\n";
unset($cursor);
echo $map->counts(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:1:0\n1:5:1\n0:5:1\n");
}

/// A property boxed as `mixed` keeps its box when a typed by-reference parameter writes it. The
/// cell itself would take the raw value, as `LinkStub::$inVendor` did: the next read of the
/// property jumped through a bare 1.
#[test]
fn test_a_typed_by_ref_parameter_converts_through_a_boxed_property() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class S {
    public mixed $flag = false;
    public mixed $number = 0;
    public mixed $text = 'a';
    public mixed $list = [];
    private function setFlag(bool &$x) { $x = true; }
    private function setNumber(int &$x) { $x = 7; }
    private function setText(string &$x) { $x .= 'b'; }
    private function setList(array &$x) { $x[] = 1; }
    public function go() {
        $this->setFlag($this->flag);
        $this->setNumber($this->number);
        $this->setText($this->text);
        $this->setList($this->list);
        var_dump($this->flag, $this->number, $this->text, count($this->list));
    }
}
(new S())->go();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nint(7)\nstring(2) \"ab\"\nint(1)\n");
}

/// A value narrowed to an interface can still be the class a parent constructor declares. Past a
/// few implementers the narrowing keeps the interface alone, which the checker took for a proved
/// mismatch: it compiled an unconditional `TypeError`, and `lint:twig` met it on every `default`.
#[test]
fn test_a_constructor_argument_narrowed_to_an_interface_is_checked_at_run_time() {
    let implementers: String = (0..40)
        .map(|n| format!("class Named{n} extends Node implements Marked {{ public function mark(): void {{}} }}\n"))
        .collect();
    let source = format!(
        r#"<?php
class Node {{}}
interface Marked {{ public function mark(): void; }}
{implementers}
class Base {{ public function __construct(public Node $node) {{}} }}
class Child extends Base {{
    public function __construct(Node $node) {{
        if (!$node instanceof Marked) {{ throw new Exception('unmarked'); }}
        $node->mark();
        parent::__construct($node);
    }}
}}
echo get_class((new Child(new Named7()))->node), "\n";
"#
    );
    assert_eq!(compile_and_run(&source), "Named7\n");
}

/// A word list holding `clone` is not a callable. It used to plan one override applicator per class
/// and scope profile: 237 MB of code in the Symfony Demo, for Doctrine's list of SQL keywords.
#[test]
fn test_a_list_holding_the_word_clone_generates_no_override_applicators() {
    let (out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
class A { public int $n = 1; }
class B { public string $s = 'x'; }
function words() { return ['ALTER', 'CLONE', 'SELECT']; }
$hash = ['first' => 'clone'];
echo count(words()), $hash['first'], (new A())->n, (new B())->s, "\n";
"#,
    );
    assert_eq!(out.stdout, "3clone1x\n");
    assert!(!asm.contains("clone_u_apply"), "applicators were generated");
}

/// A string that really is the callable still reaches the applicators.
#[test]
fn test_clone_called_by_name_with_overrides_still_applies_them() {
    let out = compile_and_run(
        r#"<?php
class A { public int $n = 1; }
$f = 'clone';
echo $f(new A(), ['n' => 5])->n, "\n";
"#,
    );
    assert_eq!(out, "5\n");
}
