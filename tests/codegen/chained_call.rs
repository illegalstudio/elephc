//! Purpose:
//! End-to-end coverage for method calls whose receiver is another call's result (the fluent
//! style, `$m->a()->b($v)`), and for calls whose dispatched body is an override below the
//! receiver's static class.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.
//! - Every case used to compile and print something else: the untyped parameter of a method
//!   reached only through a chain kept the type its first direct call taught it, so a boxed
//!   argument was stored into a slot typed for something else.

use crate::support::*;

/// The second call of `$m->add('a')->add([1])` widens `add`'s untyped parameter, so the array survives in the list.
#[test]
fn test_chained_call_widens_untyped_param_of_second_call() {
    let out = compile_and_run(
        r#"<?php
class M { public $to = [];
  public function add($v) { $this->to[] = $v; return $this; } }
$m = new M;
$m->add('a')->add([1]);
echo json_encode($m->to), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",[1]]\n");
}

/// A five-level fluent chain alternating two methods widens both methods' untyped parameters.
#[test]
fn test_chained_call_three_or_more_levels_widen_every_method() {
    let out = compile_and_run(
        r#"<?php
class M { public $to = []; public $cc = [];
  public function addTo($v) { $this->to[] = $v; return $this; }
  public function addCc($v) { $this->cc[] = $v; return $this; } }
$m = new M;
$m->addTo('a')->addCc('b')->addTo(['address' => 'x', 'name' => null])->addCc(7)->addTo(2.5);
echo json_encode($m->to), json_encode($m->cc), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",{\"address\":\"x\",\"name\":null},2.5][\"b\",7]\n");
}

/// `: static` and `: self` fluent returns, a subclass calling inherited fluent methods, and an override reached through the parent-typed `: self` result.
#[test]
fn test_fluent_static_and_self_returns_widen_inherited_and_overriding_methods() {
    let out = compile_and_run(
        r#"<?php
class B { public $xs = [];
  public function s($v): static { $this->xs[] = $v; return $this; }
  public function f($v): self { $this->xs[] = $v; return $this; }
  public function h($v) { $this->xs[] = 'B'; return $this; } }
class C extends B { public $ys = [];
  public function g($v) { $this->xs[] = $v; return $this; }
  public function h($v) { $this->ys[] = $v; return $this; } }
$c = new C;
$c->s('a')->g([2])->f(1)->s(true)->f(null);
$c->f('x')->h('s')->h([3]);
echo json_encode($c->xs), json_encode($c->ys), "\n";
$b = new B;
$b->f('x')->s(['k' => 1])->h(0);
echo json_encode($b->xs), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",[2],1,true,null,\"x\"][\"s\",[3]]\n[\"x\",{\"k\":1},\"B\"]\n");
}

/// A chain on `Q::make()` (a `new static` factory) widens the untyped parameters, including through a subclass.
#[test]
fn test_fluent_chain_on_static_factory_result_widens_params() {
    let out = compile_and_run(
        r#"<?php
class Q { public $w = [];
  public static function make() { return new static(); }
  public function where($k, $v) { $this->w[] = [$k => $v]; return $this; } }
class Q2 extends Q {}
$q = Q::make()->where('a', 1)->where('b', 'two')->where('c', [3]);
echo json_encode($q->w), "\n";
echo get_class(Q2::make()->where('z', false)), "\n";
"#,
    );
    assert_eq!(out, "[{\"a\":1},{\"b\":\"two\"},{\"c\":[3]}]\nQ2\n");
}

/// A chain on `(new Bag())` widens the untyped parameter.
#[test]
fn test_fluent_chain_on_parenthesized_new_widens_params() {
    let out = compile_and_run(
        r#"<?php
class Bag { public $items = [];
  public function add($v) { $this->items[] = $v; return $this; } }
$b = (new Bag())->add('s')->add(3)->add(['x' => 1]);
echo json_encode($b->items), "\n";
echo json_encode((new Bag)->add(1.5)->add('t')->items), "\n";
"#,
    );
    assert_eq!(out, "[\"s\",3,{\"x\":1}]\n[1.5,\"t\"]\n");
}

/// Nullsafe links (`?->`) in a fluent chain resolve their receiver and widen the callee, and a null short-circuit skips the call.
#[test]
fn test_fluent_chain_through_nullsafe_results_widens_params() {
    let out = compile_and_run(
        r#"<?php
class N { public $vs = []; public ?N $next = null;
  public function push($v) { $this->vs[] = $v; return $this; }
  public function nxt() { return $this->next; } }
$n = new N; $n->next = new N;
$n->push('a')->nxt()?->push(1)?->push(['q']);
$n?->push(2.5)->push('z');
echo json_encode($n->vs), json_encode($n->next->vs), "\n";
$o = new N;
var_dump($o->nxt()?->push('never'));
echo json_encode($o->vs), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",2.5,\"z\"][1,[\"q\"]]\nNULL\n[]\n");
}

/// A chain on a method typed to return an interface with two implementations reaches each implementation with boxed parameters.
#[test]
fn test_fluent_chain_on_interface_return_with_several_implementations() {
    let out = compile_and_run(
        r#"<?php
interface Sink { public function put($v); }
class A implements Sink { public $got = []; public function put($v) { $this->got[] = $v; return $this; } }
class B2 implements Sink { public $got = []; public function put($v) { $this->got[] = $v; return $this; } }
class F { public static function pick(int $i): Sink { return $i ? new A : new B2; } }
$a = F::pick(1)->put('s')->put([1]);
$b = F::pick(0)->put(2)->put(['k' => 'v']);
echo get_class($a), json_encode($a->got), get_class($b), json_encode($b->got), "\n";
"#,
    );
    assert_eq!(out, "A[\"s\",[1]]B2[2,{\"k\":\"v\"}]\n");
}

/// A call made on a local that holds another call's result widens the callee like a direct chain.
#[test]
fn test_chained_call_result_stored_in_local_widens_params() {
    let out = compile_and_run(
        r#"<?php
class M { public $to = [];
  public function add($v) { $this->to[] = $v; return $this; }
  public function self_() { return $this; } }
$m = new M;
$m->add('a');
$x = $m->self_();
$x->add([1]);
$y = $x->self_()->self_();
$y->add(3);
echo json_encode($m->to), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",[1],3]\n");
}

/// Chains inside a free function and a method body widen the callee, and the function return is typed from the chain.
#[test]
fn test_chained_call_inside_function_and_method_bodies_widens_params() {
    let out = compile_and_run(
        r#"<?php
class M { public $to = [];
  public function add($v) { $this->to[] = $v; return $this; } }
function build() { $m = new M; return $m->add('a')->add([1, 2]); }
class Builder { public function run() { return (new M)->add(true)->add('b'); } }
echo json_encode(build()->to), json_encode((new Builder)->run()->to), "\n";
function show($o) { echo get_class($o), "\n"; }
show((new M)->add(1));
"#,
    );
    assert_eq!(out, "[\"a\",[1,2]][true,\"b\"]\nM\n");
}

/// A function body checked before the method pass widens the callee through a local that holds a call result.
#[test]
fn test_chained_call_in_function_body_checked_before_method_returns() {
    let out = compile_and_run(
        r#"<?php
class M { public $to = [];
  public function add($v) { $this->to[] = $v; return $this; }
  public function me() { return $this; } }
function build($m) { $m->add('a'); $x = $m->add('b'); $x->add([1]); return $m; }
function make() { $m = new M; return $m->me(); }
echo json_encode(build(new M)->to), json_encode(make()->add(2.5)->add('c')->to), "\n";
"#,
    );
    assert_eq!(out, "[\"a\",\"b\",[1]][2.5,\"c\"]\n");
}

/// A fluent dynamic-property push chain on a subclass instance widens the inherited method.
#[test]
fn test_fluent_dynamic_push_chain_on_subclass_widens_params() {
    let out = compile_and_run(
        r#"<?php
class P { public $a = []; public function add($p, $v) { $this->{$p}[] = $v; return $this; } }
class C extends P { public $only = []; public $s = 'str'; }
$c = new C; $c->add('only', 1)->add('a', 2)->add('only', 'x');
echo json_encode($c->only), json_encode($c->a), "\n";
$p = new P; $p->add('a', 3); echo json_encode($p->a), "\n";
"#,
    );
    assert_eq!(out, "[1,\"x\"][2]\n[3]\n");
}

/// A pass-through method reached through a chain returns each argument with its own type.
#[test]
fn test_chained_call_passthrough_return_keeps_argument_type() {
    let out = compile_and_run(
        r#"<?php
class V { public function id($x) { return $x; } public function me() { return $this; } }
$v = new V;
echo $v->me()->id('str'), "|", $v->me()->me()->id(42), "|", json_encode($v->id([1, 'a'])), "\n";
var_dump($v->me()->id(null), $v->me()->id(1.5));
"#,
    );
    assert_eq!(out, "str|42|[1,\"a\"]\nNULL\nfloat(1.5)\n");
}

/// A call on a parent-typed receiver widens the untyped parameters of the subclass override that actually runs.
#[test]
fn test_untyped_override_reached_through_parent_typed_receiver_is_widened() {
    let out = compile_and_run(
        r#"<?php
class B { public $xs = [];
  public function h($v) { $this->xs[] = 'B'; return $this; }
  public function dump() { echo json_encode($this->xs), "\n"; } }
class C extends B { public $ys = []; public function h($v) { $this->ys[] = $v; return $this; }
  public function dump() { echo json_encode($this->ys), "\n"; } }
function mk(int $i): B { return $i ? new C : new B; }
$c = mk(1);
$c->h('s');
$c->h([2]);
$c->dump();
"#,
    );
    assert_eq!(out, "[\"s\",[2]]\n");
}
