//! Purpose:
//! Integration tests for a parent's private property that a subclass shadows with a
//! same-named declaration of its own.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The object keeps BOTH slots. Every by-name lookup reaches the subclass's, so each
//!   consumer that walks the layout has to know which slot is which: the var_dump, print_r,
//!   var_export and serialize rows, the `(array)` cast, a scoped `get_object_vars()` or
//!   `foreach ($this)`, the by-name allocator's defaults, and an object-owned reference cell.
//! - Expected output is php's own (`php -n`) for the same program.

use super::*;

/// Verifies each renderer shows the parent's hidden slot under the parent's name and value.
///
/// The rows were built by NAME, so the parent's slot printed as `"p":"B":private` with the
/// child's value, `(array)` and serialize lost it, and unserialize never restored it. Inside the
/// parent, `get_object_vars($this)` and `foreach ($this)` must see the parent's own private and
/// keep it when a child's public property reuses the name.
#[test]
fn test_shadowed_parent_private_keeps_its_own_slot_in_every_renderer() {
    let out = compile_and_run(
        r###"<?php
class A {
    public $x = 1;
    private $p = 'a-p';
    private $q = 'a-q';
    public function aVars() { return get_object_vars($this); }
    public function aP() { return $this->p; }
    public function aIter() { $out = []; foreach ($this as $k => $v) { $out[] = "$k=$v"; } return implode(',', $out); }
}
class B extends A {
    private $p = 'b-p';
    public $q = 'b-q';
    public function bVars() { return get_object_vars($this); }
}
$b = new B();
var_dump($b);
print_r($b); echo "\n";
var_export($b); echo "\n";
echo json_encode($b), "\n";
echo str_replace("\0", '\0', serialize($b)), "\n";
foreach ((array) $b as $k => $v) { echo str_replace("\0", '\0', $k), '=', $v, "\n"; }
echo json_encode($b->aVars()), "\n";
echo json_encode($b->bVars()), "\n";
echo json_encode(get_object_vars($b)), "\n";
echo $b->aP(), "\n";
echo $b->aIter(), "\n";
$c = clone $b;
echo $c->aP(), "\n";
$u = unserialize(serialize($b));
echo $u->aP(), ' ', $u->q, "\n";
"###,
    );
    assert_eq!(
        out,
        "object(B)#1 (5) {\n  [\"x\"]=>\n  int(1)\n  [\"p\":\"A\":private]=>\n  string(3) \"a-p\"\n  [\"q\":\"A\":private]=>\n  string(3) \"a-q\"\n  [\"p\":\"B\":private]=>\n  string(3) \"b-p\"\n  [\"q\"]=>\n  string(3) \"b-q\"\n}\nB Object\n(\n    [x] => 1\n    [p:A:private] => a-p\n    [q:A:private] => a-q\n    [p:B:private] => b-p\n    [q] => b-q\n)\n\n\\B::__set_state(array(\n   'x' => 1,\n   'p' => 'a-p',\n   'q' => 'a-q',\n   'p' => 'b-p',\n   'q' => 'b-q',\n))\n{\"x\":1,\"q\":\"b-q\"}\nO:1:\"B\":5:{s:1:\"x\";i:1;s:4:\"\\0A\\0p\";s:3:\"a-p\";s:4:\"\\0A\\0q\";s:3:\"a-q\";s:4:\"\\0B\\0p\";s:3:\"b-p\";s:1:\"q\";s:3:\"b-q\";}\nx=1\n\\0A\\0p=a-p\n\\0A\\0q=a-q\n\\0B\\0p=b-p\nq=b-q\n{\"x\":1,\"p\":\"a-p\",\"q\":\"a-q\"}\n{\"x\":1,\"p\":\"b-p\",\"q\":\"b-q\"}\n{\"x\":1,\"q\":\"b-q\"}\na-p\nx=1,p=a-p,q=a-q\na-p\na-p b-q\n"
    );
}

/// Verifies an object built by name gets the defaults of a private slot its class hides.
///
/// `unserialize()` of an empty payload and `newInstanceWithoutConstructor()` allocate by class
/// id and run a property-default thunk in the class's own scope, which cannot name the parent's
/// hidden slot. The thunk used to assign the parent's default to the child's slot instead: a
/// default of another type failed to compile, and a same-typed one left the parent's slot zeroed
/// and crashed the first parent method that read it.
#[test]
fn test_dynamic_allocation_initializes_a_hidden_private_slot() {
    let out = compile_and_run(
        r###"<?php
class A {
    private $p = 'a-p';
    private $n = 7;
    private $list = [1, 2];
    public $pub = 'a-pub';
    public function get() { return var_export($this->p, true) . ' ' . var_export($this->n, true) . ' ' . json_encode($this->list); }
}
class B extends A {
    private $p = 'b-p';
    private $n = 8;
    public $pub = 'b-pub';
    public function mine() { return $this->p . ' ' . $this->n; }
}
class C extends B {
    private $list = 'c-list';
    public function cl() { return $this->list; }
}
$u = unserialize('O:1:"C":0:{}');
echo $u->get(), ' | ', $u->mine(), ' | ', $u->cl(), ' | ', $u->pub, "\n";
$r = (new ReflectionClass('C'))->newInstanceWithoutConstructor();
echo $r->get(), ' | ', $r->mine(), ' | ', $r->cl(), ' | ', $r->pub, "\n";
$s = new C();
echo $s->get(), ' | ', $s->mine(), ' | ', $s->cl(), ' | ', $s->pub, "\n";
"###,
    );
    assert_eq!(
        out,
        "'a-p' 7 [1,2] | b-p 8 | c-list | b-pub\n'a-p' 7 [1,2] | b-p 8 | c-list | b-pub\n'a-p' 7 [1,2] | b-p 8 | c-list | b-pub\n"
    );
}

/// Verifies `$r = &$this->p` in a parent works on an instance whose class shadows `p`.
///
/// Taking the reference turns the parent's property into an object-owned reference cell. The
/// promotion and the cell allocation followed the NAME, which in the subclass is its own `p`,
/// so the parent's slot never got a cell and the program died on the first write through it.
#[test]
fn test_reference_to_a_hidden_private_slot_gets_its_own_cell() {
    let out = compile_and_run(
        r###"<?php
class RefShadowBase {
    private $p = 'a-p';
    public function bump(): void { $r = &$this->p; $r .= '!'; }
    public function baseValue(): string { return $this->p; }
}
class RefShadowChild extends RefShadowBase {
    private $p = 'b-p';
    public function childValue(): string { return $this->p; }
}
$child = new RefShadowChild();
$child->bump();
echo $child->baseValue(), ' ', $child->childValue(), "\n";
$child->bump();
echo $child->baseValue(), ' ', $child->childValue(), "\n";
$base = new RefShadowBase();
$base->bump();
echo $base->baseValue(), "\n";
"###,
    );
    assert_eq!(
        out,
        "a-p! b-p\na-p!! b-p\na-p!\n"
    );
}

/// Verifies `serialize()` omits a typed property that was never assigned, as php does.
///
/// The walker serialized every declared row and wrote the static row count, so an
/// uninitialized slot's sentinel reached the value encoder and the process crashed.
#[test]
fn test_serialize_leaves_out_uninitialized_typed_properties() {
    let out = compile_and_run(
        r###"<?php
class Dep { public $v = 1; }
class S {
    public int $i;
    public string $s;
    public array $a;
    public ?Dep $o;
    public Dep $d;
    public $plain = 'p';
}
$x = new S();
$ser = serialize($x);
echo str_replace("\0", '~', $ser), "\n";
$y = unserialize($ser);
echo json_encode([isset($y->i), isset($y->s), isset($y->a), isset($y->d), $y->plain]), "\n";
$x->i = 5;
$x->d = new Dep();
echo str_replace("\0", '~', serialize($x)), "\n";
"###,
    );
    assert_eq!(
        out,
        "O:1:\"S\":1:{s:5:\"plain\";s:1:\"p\";}\n[false,false,false,false,\"p\"]\nO:1:\"S\":3:{s:1:\"i\";i:5;s:1:\"d\";O:3:\"Dep\":1:{s:1:\"v\";i:1;}s:5:\"plain\";s:1:\"p\";}\n"
    );
}
