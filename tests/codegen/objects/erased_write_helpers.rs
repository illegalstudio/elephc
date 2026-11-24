//! Purpose:
//! Protects the shared outlined property writes of receivers that lost their class (`mixed`).
//!
//! Called from:
//! - The codegen integration test harness through `objects`.
//!
//! Key details:
//! - The assembly of one write site must not grow with the classes that declare `__set` or with
//!   SimpleXML in the program: the class-dependent ladder lives once in a shared helper.
//! - Behaviour fixtures are `php -n` output; errors keep the line of the caller's statement.

use super::*;

/// Every write form a probe measures, with the statement of its `probe<form>()` function.
const PROBE_FORMS: [(&str, &str); 7] = [
    ("set", "$o->p[\"k\"] = $v; return 1;"),
    ("ret", "return $o->p[\"k\"] = $v;"),
    ("push", "$o->p[] = $v; return 1;"),
    ("compound", "$o->p[\"k\"] .= $v; return 1;"),
    ("coalesce", "return $o->p[\"k\"] ??= $v;"),
    ("incdec", "$o->p[\"k\"]++; return 1;"),
    ("assign", "$o->p = $v; return 1;"),
];

/// Assembly bytes of each `probe<form>()` in ONE program (one compile for all the forms), in a
/// program with `classes` `__set` classes that declare the written property and, when `xml`, a
/// `SimpleXMLElement` in the program. A write site calls a shared helper, so a site's size does
/// not depend on the other sites beside it.
fn probe_bytes(classes: usize, xml: bool) -> Vec<(&'static str, usize)> {
    let declared: String = (0..classes)
        .map(|i| format!("class C{i} {{ public array $p = []; public function __set($n, $v) {{}} }}\n"))
        .collect();
    let make = if xml { "$n > 9 ? new SimpleXMLElement(\"<r/>\") : new H" } else { "new H" };
    let probes: String = PROBE_FORMS.iter()
        .map(|(form, body)| format!("function probe{form}(mixed $o, mixed $v): mixed {{ {body} }}\n"))
        .collect();
    let calls: String = PROBE_FORMS.iter()
        .map(|(form, _)| format!("echo json_encode(probe{form}(make($argc), $argc));\n"))
        .collect();
    let source = format!(
        "<?php\n{declared}class H {{ public array $p = []; protected array $q = []; }}\n\
         function make(int $n): mixed {{ return {make}; }}\n{probes}{calls}"
    );
    let dir = make_cli_test_dir("erased_write_size");
    let (asm, _runtime, _libs) = compile_source_to_asm_with_options(&source, &dir, 8_388_608, false, false);
    let _ = std::fs::remove_dir_all(&dir);
    PROBE_FORMS.iter().map(|(form, _)| {
        let start = asm.find(&format!("fn_probe{form}:")).unwrap_or_else(|| panic!("the probe{form}() label"));
        let end = asm[start..].find(&format!("fn_probe{form}_epilogue:"))
            .unwrap_or_else(|| panic!("the probe{form}() epilogue"));
        (*form, end)
    }).collect()
}

/// A site's assembly bytes (comments included) stay below this whatever the program holds: the
/// inline ladders were 150 000 bytes with SimpleXML in the program and 400 000 with twenty `__set`
/// classes beside it.
const MAX_SITE_BYTES: usize = 32_000;

/// One erased write is a call plus argument staging: it does not grow with the `__set` classes
/// or with SimpleXML, and stays near the size it had before the ladders were inlined.
#[test]
fn test_erased_property_writes_do_not_grow_with_classes_or_simplexml() {
    // Four compiles for all seven forms: the bounds of every form stay.
    let (few, many) = (probe_bytes(1, false), probe_bytes(20, false));
    let (xml_few, xml_many) = (probe_bytes(1, true), probe_bytes(20, true));
    for (at, (form, _)) in PROBE_FORMS.iter().enumerate() {
        let (few, many, xml_few, xml_many) = (few[at].1, many[at].1, xml_few[at].1, xml_many[at].1);
        assert!(many < few + 512, "{form}: grew with __set classes: {few} -> {many}");
        assert!(xml_many < xml_few + 512, "{form}: grew with __set classes beside SimpleXML: {xml_few} -> {xml_many}");
        // `??=` also holds the quiet element read of its probe, whose native XML arm is the read
        // side's own (flat in the number of classes, as the assertion above shows).
        let xml_allowance = if *form == "coalesce" { 12_288 } else { 1_024 };
        assert!(xml_few < few + xml_allowance, "{form}: SimpleXML in the program grew the site: {few} -> {xml_few}");
        assert!(xml_many < MAX_SITE_BYTES, "{form}: a write site is {xml_many} bytes");
    }
}

/// php -n output of the program below: every erased write form, its errors (with the caller line),
/// `__get`/`__set`/`ArrayAccess` receivers, COW, references, evaluation order and destructors.
#[test]
fn test_erased_property_writes_match_php() {
    let out = compile_and_run_with_heap_debug(r####"<?php
set_error_handler(function($no,$str,$file,$line){ echo "E[$no] $str @$line\n"; return true; });
class Plain { public array $p = []; public $u; public ?array $n = null; public int $i = 0; }
class Magic { private array $data = []; public function __set($n, $v) { echo "set($n)\n"; $this->data[$n] = $v; } public function __get($n) { echo "get($n)\n"; return $this->data[$n] ?? null; } }
class MagicSet { public function __set($n, $v) { echo "set($n)\n"; } }
class RO { public function __construct(public readonly array $p = [1]) {} }
class Asym { public private(set) array $p = []; public function add($k, $v) { $this->p[$k] = $v; } }
class AA implements ArrayAccess { public array $store = []; public function offsetExists(mixed $o): bool { return isset($this->store[$o]); } public function offsetGet(mixed $o): mixed { return $this->store[$o]; } public function offsetSet(mixed $o, mixed $v): void { echo "offsetSet\n"; if ($o === null) $this->store[] = $v; else $this->store[$o] = $v; } public function offsetUnset(mixed $o): void {} }
class Priv { private array $p = []; function dump() { return $this->p; } }
class PrivKid extends Priv { function write(mixed $o, $k, $v) { $o->p[$k] = $v; } }
class Typed { public int $q = 1; public ?Plain $o = null; public string $s = "a"; }
function w(mixed $o, $k, $v) { $o->p[$k] = $v; return $o; }
function a(mixed $o, $v) { $o->p[] = $v; return $o; }
function c(mixed $o, $k, $v) { $o->p[$k] .= $v; return $o; }
function e(mixed $o, $k, $v) { return $o->p[$k] = $v; }
function s(mixed $o, $v) { $o->p = $v; return $o; }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), " @", $x->getLine(), " tr=", count($x->getTrace()), "\n"; } }
function f($s) { echo "f($s) "; return $s; }

$p = w(new Plain, "x", 1); a($p, 2); $p->p["x"] = 5; c($p, "x", "z"); echo json_encode($p->p), "\n"; echo json_encode(e(new Plain, "q", [1,2])), "\n";

t(fn() => w(null, "k", 1)); t(fn() => a(null, 1)); t(fn() => s(null, 1)); t(fn() => c(null, "k", 1));

$m = new Magic; w($m, "k", 3); echo json_encode(s($m, 5)), "\n";

$o = new stdClass; w($o, "k", 7); a($o, 8); s($o, 9); echo json_encode($o), "\n";

$y = new class { public $p; }; w($y, "z", 1); a($y, 9); c($y, "z", "w"); echo json_encode($y->p), "\n";

$aa = new class { public $p; function __construct() { $this->p = new AA; } }; w($aa, 3, 4); a($aa, 5); echo json_encode($aa->p->store), "\n";

$n = new Plain; $n->p = [1,2,3]; $z = $n; w($z, 1, "x"); echo json_encode([$n->p, $z->p]), "\n";

function ord_(mixed $o) { $o->p[f("k")] = f("v"); } ord_(new Plain); echo "\n";

function intk(mixed $o) { $o->p[1.5] = 2; $o->p["7"] = 8; $o->p[true] = 1; return $o->p; } echo json_encode(intk(new Plain)), "\n";

function late(mixed $o) { $k = "a"; $o->p[$k] = ($k = "b"); return $o->p; } echo json_encode(late(new Plain)), "\n";

function cnt(mixed $o) { $o->p["n"] = 1; $o->p["n"] += 4; $o->p["m"] ??= 3; $o->p["m"] ??= 9; $o->p["n"]++; --$o->p["n"]; $o->p["z"] = $o->p["n"]++; return $o->p; } echo json_encode(cnt(new Plain)), "\n";

function expr(mixed $o) { $x = ($o->p["k"] = 5) + 1; $y = ($o->p["j"] .= "a"); $z = ($o->p = [1]); return [$x, $y, $z, $o->p]; } echo json_encode(expr(new Plain)), "\n";

function nest(mixed $o) { $o->p["a"]["b"] = 1; $o->p["a"][] = 2; return $o->p; } echo json_encode(nest(new Plain)), "\n";

function ref(mixed $o) { $v = [1]; $o->p["r"] = $v; $v[] = 2; $o->p["s"] = &$v; $v[] = 3; return $o->p; } echo json_encode(ref(new Plain)), "\n";

function tmp() { return new Plain; } function wt() { $r = (tmp())->p["a"] = 3; return $r; } echo wt(), "\n";

class D { function __destruct() { echo "D!\n"; } } function dv(mixed $o) { $o->p["d"] = new D; $o->p["d"] = 1; echo "after\n"; } dv(new Plain);

function bigloop(mixed $o) { for ($i = 0; $i < 1000; $i++) { $o->p["k$i"] = $i; $o->p[] = $i; $o->p["k0"] .= "x"; } return count($o->p); } echo bigloop(new Plain), "\n";
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"{"x":"5z","0":2}
[1,2]
Error: Attempt to modify property "p" on null @12 tr=3
Error: Attempt to modify property "p" on null @13 tr=3
Error: Attempt to assign property "p" on null @16 tr=3
Error: Attempt to modify property "p" on null @14 tr=3
get(p)
E[8] Indirect modification of overloaded property Magic::$p has no effect @12
set(p)
{}
{"p":9}
{"z":"1w","0":9}
offsetSet
offsetSet
{"3":4,"4":5}
[[1,"x",3],[1,"x",3]]
f(k) f(v) 
E[8192] Implicit conversion from float 1.5 to int loses precision @36
{"1":1,"7":8}
{"b":"b"}
{"n":6,"m":3,"z":5}
E[2] Undefined array key "j" @42
[6,"a",[1],[1]]
{"a":{"b":1,"0":2}}
{"r":[1],"s":[1,2,3]}
3
D!
after
2000
"####, "{}", out.stderr);
}

/// Subclasses that redeclare, shadow, narrow or extend a slot (magic, ArrayAccess, readonly, traits, uninitialized typed, unset) write exactly as php does, through one shared arm per group.
#[test]
fn test_inherited_slot_writes_match_php() {
    let out = compile_and_run_with_heap_debug(r####"<?php
set_error_handler(function($no,$str,$file,$line){ echo "E[$no] $str\n"; return true; });
function w(mixed $o, $k, $v) { $o->p[$k] = $v; }
function a(mixed $o, $v) { $o->p[] = $v; }
function s(mixed $o, $v) { $o->p = $v; }
function c(mixed $o, $k, $v) { $o->p[$k] .= $v; }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
function show($o) { echo get_class($o), ' ', json_encode($o->p ?? 'unset'), "\n"; }
function all($o) { t(fn() => w($o, 'k', 1)); t(fn() => a($o, 2)); t(fn() => s($o, [3])); t(fn() => c($o, 'k', 'x')); t(fn() => show($o)); }

class Base { public array $p = ['base']; }
class Same extends Base {}
class Deeper extends Same {}
class OtherDefault extends Base { public array $p = ['other']; }
class Prot { protected array $p = ['prot']; function open() { return $this->p; } }
class ProtKid extends Prot {}
class ProtPublic extends Prot { public array $p = ['widened']; }
class Priv { private array $p = ['priv']; function dump() { return $this->p; } }
class PrivKid extends Priv { public array $p = ['kid']; }
class PrivKid2 extends Priv {}
class RO { public function __construct(public readonly array $p = ['ro']) {} }
class ROKid extends RO {}
class MagicKid extends Base { public function __set($n, $v) { echo "set($n)\n"; } public function __get($n) { echo "get($n)\n"; return []; } }
class AAKid extends Base implements ArrayAccess {
  public function offsetExists(mixed $o): bool { return true; } public function offsetGet(mixed $o): mixed { return 1; }
  public function offsetSet(mixed $o, mixed $v): void { echo "offsetSet\n"; } public function offsetUnset(mixed $o): void {} }
class UnsetBase { public array $p = ['x']; function drop() { unset($this->p); } }
class Uninit { public array $p; }
class UninitKid extends Uninit {}
trait T { public array $p = ['trait']; }
class UsesT { use T; }
class UsesTKid extends UsesT {}
final class Fin extends Base {}
abstract class Abs { public array $p = ['abs']; }
class AbsKid extends Abs {}

$objs = [new Base, new Same, new Deeper, new OtherDefault, new Prot, new ProtKid, new ProtPublic,
  new Priv, new PrivKid, new PrivKid2, new RO, new ROKid, new MagicKid, new AAKid, new UsesT, new UsesTKid, new Fin, new AbsKid];
foreach ($objs as $o) { echo "== ", get_class($o), "\n"; all($o); }
echo "== unset\n";
$u = new UnsetBase; $u->drop(); t(fn() => w($u, 'a', 1)); t(fn() => show($u));
echo "== uninit\n";
foreach ([new Uninit, new UninitKid] as $o) { t(fn() => w($o, 'a', 1)); t(fn() => a($o, 1)); t(fn() => show($o)); }
echo "== scope\n";
class ScopeProt extends Prot { function go(mixed $o) { $o->p['x'] = 1; $o->p[] = 2; return $o->p; } }
$sp = new ScopeProt; echo json_encode($sp->go($sp)), json_encode($sp->go(new ProtKid)), json_encode($sp->go(new Prot)), "\n";
t(fn() => $sp->go(new Base));
class ScopePriv extends Priv { function go(mixed $o) { $o->p['x'] = 1; return $o->p; } }
$sq = new ScopePriv; t(fn() => print(json_encode($sq->go($sq)) . "\n")); t(fn() => print(json_encode($sq->go(new PrivKid)) . "\n")); t(fn() => print(json_encode($sq->go(new Priv)) . "\n"));
class PrivWriter extends Priv { function go(mixed $o) { $o->p[] = 1; } }
t(fn() => (new PrivWriter)->go(new PrivKid2));
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"== Base
E[2] Undefined array key "k"
Base {"0":3,"k":"x"}
== Same
E[2] Undefined array key "k"
Same {"0":3,"k":"x"}
== Deeper
E[2] Undefined array key "k"
Deeper {"0":3,"k":"x"}
== OtherDefault
E[2] Undefined array key "k"
OtherDefault {"0":3,"k":"x"}
== Prot
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Prot "unset"
== ProtKid
Error: Cannot access protected property ProtKid::$p
Error: Cannot access protected property ProtKid::$p
Error: Cannot access protected property ProtKid::$p
Error: Cannot access protected property ProtKid::$p
ProtKid "unset"
== ProtPublic
E[2] Undefined array key "k"
ProtPublic {"0":3,"k":"x"}
== Priv
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Priv "unset"
== PrivKid
E[2] Undefined array key "k"
PrivKid {"0":3,"k":"x"}
== PrivKid2
E[8192] Creation of dynamic property PrivKid2::$p is deprecated
E[2] Undefined array key "k"
PrivKid2 {"0":3,"k":"x"}
== RO
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
RO ["ro"]
== ROKid
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
ROKid ["ro"]
== MagicKid
E[2] Undefined array key "k"
MagicKid {"0":3,"k":"x"}
== AAKid
E[2] Undefined array key "k"
AAKid {"0":3,"k":"x"}
== UsesT
E[2] Undefined array key "k"
UsesT {"0":3,"k":"x"}
== UsesTKid
E[2] Undefined array key "k"
UsesTKid {"0":3,"k":"x"}
== Fin
E[2] Undefined array key "k"
Fin {"0":3,"k":"x"}
== AbsKid
E[2] Undefined array key "k"
AbsKid {"0":3,"k":"x"}
== unset
UnsetBase {"a":1}
== uninit
Uninit {"a":1,"0":1}
UninitKid {"a":1,"0":1}
== scope
{"0":"prot","x":1,"1":2}{"0":"prot","x":1,"1":2}{"0":"prot","x":1,"1":2}
E[8192] Creation of dynamic property ScopePriv::$p is deprecated
{"x":1}
{"0":"kid","x":1}
Error: Cannot access private property Priv::$p
E[8192] Creation of dynamic property PrivKid2::$p is deprecated
"####, "{}", out.stderr);
}

/// The same program with eval linked: the helpers are shared per scope group and the caller's scope reaches the interpreter as an operand.
#[test]
fn test_inherited_slot_writes_match_php_with_eval_linked() {
    let out = compile_and_run_with_heap_debug(r####"<?php
eval($argc > 5 ? "echo 1;" : "");
set_error_handler(function($no,$str,$file,$line){ echo "E[$no] $str\n"; return true; });
function w(mixed $o, $k, $v) { $o->p[$k] = $v; }
function a(mixed $o, $v) { $o->p[] = $v; }
function s(mixed $o, $v) { $o->p = $v; }
function c(mixed $o, $k, $v) { $o->p[$k] .= $v; }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
function show($o) { echo get_class($o), ' ', json_encode($o->p ?? 'unset'), "\n"; }
function all($o) { t(fn() => w($o, 'k', 1)); t(fn() => a($o, 2)); t(fn() => s($o, [3])); t(fn() => c($o, 'k', 'x')); t(fn() => show($o)); }

class Base { public array $p = ['base']; }
class Same extends Base {}
class Deeper extends Same {}
class OtherDefault extends Base { public array $p = ['other']; }
class Prot { protected array $p = ['prot']; function open() { return $this->p; } }
class ProtKid extends Prot {}
class ProtPublic extends Prot { public array $p = ['widened']; }
class Priv { private array $p = ['priv']; function dump() { return $this->p; } }
class PrivKid extends Priv { public array $p = ['kid']; }
class PrivKid2 extends Priv {}
class RO { public function __construct(public readonly array $p = ['ro']) {} }
class ROKid extends RO {}
class MagicKid extends Base { public function __set($n, $v) { echo "set($n)\n"; } public function __get($n) { echo "get($n)\n"; return []; } }
class AAKid extends Base implements ArrayAccess {
  public function offsetExists(mixed $o): bool { return true; } public function offsetGet(mixed $o): mixed { return 1; }
  public function offsetSet(mixed $o, mixed $v): void { echo "offsetSet\n"; } public function offsetUnset(mixed $o): void {} }
class UnsetBase { public array $p = ['x']; function drop() { unset($this->p); } }
class Uninit { public array $p; }
class UninitKid extends Uninit {}
trait T { public array $p = ['trait']; }
class UsesT { use T; }
class UsesTKid extends UsesT {}
final class Fin extends Base {}
abstract class Abs { public array $p = ['abs']; }
class AbsKid extends Abs {}

$objs = [new Base, new Same, new Deeper, new OtherDefault, new Prot, new ProtKid, new ProtPublic,
  new Priv, new PrivKid, new PrivKid2, new RO, new ROKid, new MagicKid, new AAKid, new UsesT, new UsesTKid, new Fin, new AbsKid];
foreach ($objs as $o) { echo "== ", get_class($o), "\n"; all($o); }
echo "== unset\n";
$u = new UnsetBase; $u->drop(); t(fn() => w($u, 'a', 1)); t(fn() => show($u));
echo "== uninit\n";
foreach ([new Uninit, new UninitKid] as $o) { t(fn() => w($o, 'a', 1)); t(fn() => a($o, 1)); t(fn() => show($o)); }
echo "== scope\n";
class ScopeProt extends Prot { function go(mixed $o) { $o->p['x'] = 1; $o->p[] = 2; return $o->p; } }
$sp = new ScopeProt; echo json_encode($sp->go($sp)), json_encode($sp->go(new ProtKid)), json_encode($sp->go(new Prot)), "\n";
t(fn() => $sp->go(new Base));
class ScopePriv extends Priv { function go(mixed $o) { $o->p['x'] = 1; return $o->p; } }
$sq = new ScopePriv; t(fn() => print(json_encode($sq->go($sq)) . "\n")); t(fn() => print(json_encode($sq->go(new PrivKid)) . "\n")); t(fn() => print(json_encode($sq->go(new Priv)) . "\n"));
class PrivWriter extends Priv { function go(mixed $o) { $o->p[] = 1; } }
t(fn() => (new PrivWriter)->go(new PrivKid2));
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"== Base
E[2] Undefined array key "k"
Base {"0":3,"k":"x"}
== Same
E[2] Undefined array key "k"
Same {"0":3,"k":"x"}
== Deeper
E[2] Undefined array key "k"
Deeper {"0":3,"k":"x"}
== OtherDefault
E[2] Undefined array key "k"
OtherDefault {"0":3,"k":"x"}
== Prot
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Prot "unset"
== ProtKid
Error: Cannot access protected property ProtKid::$p
Error: Cannot access protected property ProtKid::$p
Error: Cannot access protected property ProtKid::$p
Error: Cannot access protected property ProtKid::$p
ProtKid "unset"
== ProtPublic
E[2] Undefined array key "k"
ProtPublic {"0":3,"k":"x"}
== Priv
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Priv "unset"
== PrivKid
E[2] Undefined array key "k"
PrivKid {"0":3,"k":"x"}
== PrivKid2
E[8192] Creation of dynamic property PrivKid2::$p is deprecated
E[2] Undefined array key "k"
PrivKid2 {"0":3,"k":"x"}
== RO
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
RO ["ro"]
== ROKid
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
ROKid ["ro"]
== MagicKid
E[2] Undefined array key "k"
MagicKid {"0":3,"k":"x"}
== AAKid
E[2] Undefined array key "k"
AAKid {"0":3,"k":"x"}
== UsesT
E[2] Undefined array key "k"
UsesT {"0":3,"k":"x"}
== UsesTKid
E[2] Undefined array key "k"
UsesTKid {"0":3,"k":"x"}
== Fin
E[2] Undefined array key "k"
Fin {"0":3,"k":"x"}
== AbsKid
E[2] Undefined array key "k"
AbsKid {"0":3,"k":"x"}
== unset
UnsetBase {"a":1}
== uninit
Uninit {"a":1,"0":1}
UninitKid {"a":1,"0":1}
== scope
{"0":"prot","x":1,"1":2}{"0":"prot","x":1,"1":2}{"0":"prot","x":1,"1":2}
E[8192] Creation of dynamic property ScopePriv::$p is deprecated
{"x":1}
{"0":"kid","x":1}
Error: Cannot access private property Priv::$p
E[8192] Creation of dynamic property PrivKid2::$p is deprecated
"####, "{}", out.stderr);
}

/// Classes declared by eval() that extend the scope class or the declaring class (protected, private, readonly, with and without __set): the output is merge-main's, whose differences from php (messages naming the interpreted class, a private slot of an interpreted child) are old gaps.
#[test]
fn test_inherited_slot_writes_with_eval_declared_classes_keep_merge_main_behaviour() {
    let out = compile_and_run_with_heap_debug(r####"<?php
set_error_handler(function($no,$str,$file,$line){ echo "E[$no] $str\n"; return true; });
function w(mixed $o, $k, $v) { $o->p[$k] = $v; }
function a(mixed $o, $v) { $o->p[] = $v; }
function s(mixed $o, $v) { $o->p = $v; }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
function show($o) { echo get_class($o), ' ', json_encode($o->p ?? 'unset'), "\n"; }
function all($o) { t(fn() => w($o, 'k', 1)); t(fn() => a($o, 2)); t(fn() => s($o, [3])); t(fn() => show($o)); }

class Base { public array $p = ['base']; }
class Prot { protected array $p = ['prot']; function open() { return $this->p; }
  function go(mixed $o) { $o->p['x'] = 1; $o->p[] = 2; $o->p = ['set']; return $o->p; } }
class Priv { private array $p = ['priv']; function go(mixed $o) { $o->p['x'] = 1; return $o->p; } }
class RO { public function __construct(public readonly array $p = ['ro']) {} }
class ProtScope extends Prot {}
class Other { function go(mixed $o) { $o->p['x'] = 1; return $o->p; } }
eval($argc > 0 ? '
class EvBase extends Base {}
class EvProt extends Prot { function evgo(mixed $o) { $o->p["e"] = 1; return $o->p; } }
class EvProtMagic extends Prot { function __set($n, $v) { echo "evset($n)\n"; } }
class EvPriv extends Priv {}
class EvRO extends RO {}
class EvMagic extends Base { function __set($n, $v) { echo "evset($n)\n"; } function __get($n) { echo "evget($n)\n"; return []; } }
class EvScope extends ProtScope { function evgo(mixed $o) { $o->p["s"] = 1; return $o->p; } }
class EvRedeclare extends Base { public array $p = ["evredeclared"]; }
' : '');
$objs = [new Base, new EvBase, new Prot, new EvProt, new EvProtMagic, new Priv, new EvPriv, new RO, new EvRO, new EvMagic, new EvRedeclare];
foreach ($objs as $o) { echo "== ", get_class($o), "\n"; all($o); }
echo "== scopes\n";
$ps = new ProtScope; $ev = new EvProt; $evs = new EvScope;
foreach ([$ps, $ev, new Prot, new EvProtMagic, new Base, new EvBase] as $o) { t(fn() => print(get_class($o) . ' ' . json_encode((new Prot)->go($o)) . "\n")); }
foreach ([$ps, $ev, new Prot, new EvProtMagic] as $o) { t(fn() => print(get_class($o) . ' ' . json_encode($ps->go($o)) . "\n")); }
foreach ([$ps, $ev, new Prot, new EvProtMagic] as $o) { t(fn() => print(get_class($o) . ' ' . json_encode($evs->evgo($o)) . "\n")); }
foreach ([new Priv, new EvPriv] as $o) { t(fn() => print(get_class($o) . ' ' . json_encode((new Priv)->go($o)) . "\n")); }
foreach ([new Prot, new Priv, new EvProt] as $o) { t(fn() => print(get_class($o) . ' ' . json_encode((new Other)->go($o)) . "\n")); }
t(fn() => print(json_encode($ev->evgo($ev)) . json_encode($ev->evgo(new Prot)) . "\n"));
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"== Base
Base [3]
== EvBase
EvBase [3]
== Prot
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Prot "unset"
== EvProt
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
EvProt "unset"
== EvProtMagic
Error: Cannot access protected property Prot::$p
Error: Cannot access protected property Prot::$p
evset(p)
EvProtMagic "unset"
== Priv
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Error: Cannot access private property Priv::$p
Priv "unset"
== EvPriv
E[8192] Creation of dynamic property EvPriv::$p is deprecated
EvPriv [3]
== RO
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot indirectly modify readonly property RO::$p
Error: Cannot modify readonly property RO::$p
RO ["ro"]
== EvRO
Error: Cannot modify readonly property RO::$p
Error: Cannot modify readonly property RO::$p
EvRO ["ro"]
== EvMagic
EvMagic [3]
== EvRedeclare
EvRedeclare [3]
== scopes
ProtScope ["set"]
EvProt ["set"]
Prot ["set"]
EvProtMagic ["set"]
Base ["set"]
EvBase ["set"]
ProtScope ["set"]
EvProt ["set"]
Prot ["set"]
EvProtMagic ["set"]
ProtScope {"0":"set","s":1}
EvProt {"0":"set","s":1}
Prot {"0":"prot","s":1}
EvProtMagic {"0":"prot","s":1}
Priv {"0":"priv","x":1}
EvPriv ["priv"]
Error: Cannot access protected property Prot::$p
Error: Cannot access private property Priv::$p
Error: Cannot access protected property Prot::$p
{"0":"set","s":1,"e":1}{"0":"prot","e":1}
"####, "{}", out.stderr);
}

/// Callable arrays and callable strings nested inside array_map inside usort, in a generator and in a fiber.
const NESTED_WRAPPER_PROGRAM: &str = r####"<?php
class K {
  public static function cmp($a, $b) { return $a <=> $b; }
  public function dbl($x) { return $x * 2; }
  public function neg($x) { return -$x; }
  public static function up($s) { return strtoupper($s); }
}
function inner(array $xs) { return array_map([new K, 'dbl'], $xs); }
function outer(array $xs) { $r = inner($xs); usort($r, ['K', 'cmp']); return $r; }
echo json_encode(outer([5, 3, 9, 1])), "\n";
function nested(array $xs) { return array_map('strrev', array_map(['K', 'up'], array_map('trim', $xs))); }
echo json_encode(nested([' ab ', ' cd'])), "\n";
$fns = ['strlen', 'strtoupper', ['K', 'up'], [new K, 'neg'], 'abs', 'intdiv', 'max', 'array_sum', 'implode', 'str_repeat'];
foreach ($fns as $f) { echo is_callable($f) ? 'c' : 'n'; } echo "\n";
echo json_encode(array_map('array_sum', [[1, 2], [3]])), json_encode(array_filter([1, 0, 2], 'boolval')), json_encode(array_map(null, [1], [2])), "\n";
function gen() { foreach (array_map([new K, 'dbl'], [1, 2, 3]) as $v) { $t = [3, 1]; usort($t, ["K", "cmp"]); yield $v . $t[0]; } }
$arr = [3, 1];
foreach (gen() as $v) echo $v, ",";
echo "\n";
$fib = new Fiber(function () { $r = array_map(['K', 'up'], ['a', 'b']); Fiber::suspend(json_encode($r)); $q = [3, 1, 2]; usort($q, ['K', 'cmp']); return json_encode($q); });
echo $fib->start(), $fib->resume() ?? $fib->getReturn(), "\n";
echo call_user_func_array([new K, 'dbl'], [21]), call_user_func(["K", "up"], "z"), "\n";
"####;

/// Callable arrays inside array_map inside usort, in a generator and in a fiber: every wrapper reached while another is being emitted is queued and emitted once.
#[test]
fn test_nested_runtime_call_wrappers_are_all_emitted() {
    let out = compile_and_run_with_heap_debug(NESTED_WRAPPER_PROGRAM);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"[2,6,10,18]
["BA","DC"]
cccccccccc
[3,3]{"0":1,"2":2}[[1,2]]
21,41,61,
["A","B"][1,2,3]
42Z
"####, "{}", out.stderr);
}

/// Spells each `callable_builtin_*` / `callable_extern_*` wrapper symbol of one assembly line, with
/// the suffix of its companion labels (`_epilogue`, `__gc_frame_owners`, ...) cut off, and whether
/// the line defines that very symbol.
fn wrapper_symbols(line: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("callable_") {
        let tail = &rest[at..];
        let end = tail.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(tail.len());
        let symbol = &tail[..end];
        rest = &tail[end..];
        if !(symbol.starts_with("callable_builtin_") || symbol.starts_with("callable_extern_")) { continue; }
        let base = symbol.split("_epilogue").next().unwrap_or(symbol).split("__").next().unwrap_or(symbol);
        let defined = symbol == base && line.trim().trim_start_matches('_').trim_start_matches("eir_shared_") == format!("{symbol}:");
        found.push((base.to_string(), defined));
    }
    found
}

/// Every wrapper symbol the nested-callable program references is defined exactly once: a wrapper
/// queued behind the module copy another wrapper holds is emitted, and emitted a single time, also
/// when the last take of the copy is the one that queued it.
#[test]
fn test_every_referenced_runtime_call_wrapper_is_defined_exactly_once() {
    let dir = make_cli_test_dir("erased_write_wrappers");
    let (asm, runtime, _libs) = compile_source_to_asm_with_options(NESTED_WRAPPER_PROGRAM, &dir, 8_388_608, false, false);
    let _ = std::fs::remove_dir_all(&dir);
    let mut definitions: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut references: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in asm.lines().chain(runtime.lines()) {
        for (symbol, defined) in wrapper_symbols(line) {
            if defined { *definitions.entry(symbol.clone()).or_default() += 1; }
            references.insert(symbol);
        }
    }
    assert!(references.len() > 100, "the program reaches the runtime callable table: {} wrappers", references.len());
    for symbol in &references {
        assert_eq!(definitions.get(symbol).copied().unwrap_or(0), 1, "{symbol} is defined once");
    }
}

/// Redeclared, typed or untyped-as-mixed, magic, ArrayAccess, readonly, conditionally declared and shadowed-private slots, uninitialized typed slots after magic, and the lines of the diagnostics of each write form: php -n output, one program compiled twice (without and with eval linked).
const REDECLARED_SLOT_PROGRAM: &str = r####"<?php
set_error_handler(function($no,$str,$file,$line){ echo "E[$no] $str @$line\n"; return true; });
function w(mixed $o, $k, $v) { $o->p[$k] = $v; }
function a(mixed $o, $v) { $o->p[] = $v; }
function s(mixed $o, $v) { $o->p = $v; }
function c(mixed $o, $k, $v) { $o->p[$k] .= $v; }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
function show($o) { echo get_class($o), ' ', json_encode($o->p ?? 'unset'), "\n"; }
function all($o) { t(fn() => w($o, 'k', 1)); t(fn() => a($o, 2)); t(fn() => s($o, [3])); t(fn() => c($o, 'k', 'x')); t(fn() => show($o)); }

class Base { public array $p = ['base']; }
class MixedTyped { public mixed $p = ["mixed"]; }
class MixedKid extends MixedTyped { public array $q = []; }
class MagicSetKid extends Base { public function __set($n, $v) { echo "set($n)\n"; } }
class UninitMagic { public array $p; public function __set($n, $v) { echo "magic-set($n)\n"; } public function __get($n) { echo "magic-get($n)\n"; return []; } }
class UninitMagicKid extends UninitMagic {}
class AAOnly implements ArrayAccess { public array $p = ['aa']; public function offsetExists(mixed $o): bool { return true; } public function offsetGet(mixed $o): mixed { return 1; } public function offsetSet(mixed $o, mixed $v): void { echo "offsetSet\n"; } public function offsetUnset(mixed $o): void {} }
class WithRO { public function __construct(public readonly array $p = []) {} }
if (!class_exists("Cond", false)) { class Cond { public array $p = ["cond"]; } }
class Lv1 { private array $p = ['lv1']; function mine() { return $this->p; } }
class Lv2 extends Lv1 { private array $p = ['lv2']; function w2(mixed $o, $k, $v) { $o->p[$k] = $v; return $o->p; } }
class Lv3 extends Lv2 {}
$objs = [new Base, new MixedTyped, new MixedKid, new MagicSetKid, new AAOnly, new Cond, new WithRO, new Lv2, new Lv3];
foreach ($objs as $o) { echo "== ", get_class($o), "\n"; all($o); }
echo "== uninit magic\n";
foreach ([new UninitMagic, new UninitMagicKid] as $o) { t(fn() => w($o, 'a', 1)); t(fn() => a($o, 1)); t(fn() => c($o, 'a', 'z')); t(fn() => show($o)); }
echo "== lv\n";
$l2 = new Lv2; t(fn() => print(json_encode($l2->w2($l2, 'x', 1)) . json_encode($l2->mine()) . "\n"));
$l3 = new Lv3; t(fn() => print(json_encode($l3->w2($l3, 'x', 1)) . json_encode($l3->mine()) . "\n"));
$l1 = new Lv1; t(fn() => print(json_encode($l2->w2($l1, 'x', 1)) . "\n"));
echo "== lines\n";
function lines(mixed $o) {
    try {
        $o->p['a'] = 1;
    } catch (Error $x) { echo $x->getMessage(), " @", $x->getLine(), "\n"; }
    try {
        $o->p['b'] .= 'x';
    } catch (Error $x) { echo $x->getMessage(), " @", $x->getLine(), "\n"; }
    try {
        $o->p['n']++;
    } catch (Error $x) { echo $x->getMessage(), " @", $x->getLine(), "\n"; }
}
echo "-- base\n"; lines(new Base); echo "-- null\n"; lines(null); echo "-- lv2\n"; lines(new Lv2);
function multi(mixed $o, $k) {
    $o
      ->p[$k]
      = 1;
}
t(fn() => multi(null, 'k'));
function nullpush(mixed $o) {
    try {
        $o->p[] = 1;
    } catch (Error $x) { echo $x->getMessage(), " @", $x->getLine(), "\n"; }
    try {
        $o->p = [1];
    } catch (Error $x) { echo $x->getMessage(), " @", $x->getLine(), "\n"; }
}
nullpush(null);
"####;

/// php -n output of [`REDECLARED_SLOT_PROGRAM`].
const REDECLARED_SLOT_OUTPUT: &str = r####"== Base
E[2] Undefined array key "k" @6
Base {"0":3,"k":"x"}
== MixedTyped
E[2] Undefined array key "k" @6
MixedTyped {"0":3,"k":"x"}
== MixedKid
E[2] Undefined array key "k" @6
MixedKid {"0":3,"k":"x"}
== MagicSetKid
E[2] Undefined array key "k" @6
MagicSetKid {"0":3,"k":"x"}
== AAOnly
E[2] Undefined array key "k" @6
AAOnly {"0":3,"k":"x"}
== Cond
E[2] Undefined array key "k" @6
Cond {"0":3,"k":"x"}
== WithRO
Error: Cannot indirectly modify readonly property WithRO::$p
Error: Cannot indirectly modify readonly property WithRO::$p
Error: Cannot modify readonly property WithRO::$p
Error: Cannot indirectly modify readonly property WithRO::$p
WithRO []
== Lv2
Error: Cannot access private property Lv2::$p
Error: Cannot access private property Lv2::$p
Error: Cannot access private property Lv2::$p
Error: Cannot access private property Lv2::$p
Lv2 "unset"
== Lv3
E[8192] Creation of dynamic property Lv3::$p is deprecated @3
E[2] Undefined array key "k" @6
Lv3 {"0":3,"k":"x"}
== uninit magic
UninitMagic {"a":"1z","0":1}
UninitMagicKid {"a":"1z","0":1}
== lv
{"0":"lv2","x":1}["lv1"]
{"0":"lv2","x":1}["lv1"]
Error: Cannot access private property Lv1::$p
== lines
-- base
E[2] Undefined array key "b" @37
E[2] Undefined array key "n" @40
-- null
Attempt to modify property "p" on null @34
Attempt to modify property "p" on null @37
Attempt to modify property "p" on null @40
-- lv2
Cannot access private property Lv2::$p @34
Cannot access private property Lv2::$p @37
Cannot access private property Lv2::$p @40
Error: Attempt to modify property "p" on null
Attempt to modify property "p" on null @52
Attempt to assign property "p" on null @55
"####;

/// The redeclared-slot program, compiled without eval.
#[test]
fn test_redeclared_slot_writes_match_php() {
    let out = compile_and_run_with_heap_debug(REDECLARED_SLOT_PROGRAM);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, REDECLARED_SLOT_OUTPUT, "{}", out.stderr);
}

/// The same program with eval linked: the helpers are shared per scope group and the caller's scope travels as an operand (line 1 carries the eval so the lines stay put).
#[test]
fn test_redeclared_slot_writes_match_php_with_eval_linked() {
    let source = REDECLARED_SLOT_PROGRAM.replacen("<?php\n", "<?php eval($argc > 5 ? \"echo 1;\" : \"\");\n", 1);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, REDECLARED_SLOT_OUTPUT, "{}", out.stderr);
}

/// Classes whose dynamic property hash sits at different offsets (0, 2 and 5 declared properties, stdClass) written through one erased receiver: push, index set and compound. A shared arm read the hash of a class at the offset of another (the property count is part of an arm's identity).
#[test]
fn test_dynamic_hash_writes_use_the_layout_of_each_class() {
    let out = compile_and_run_with_heap_debug(r####"<?php
#[AllowDynamicProperties]
class HashNone {}
#[AllowDynamicProperties]
class HashTwo { public int $a = 1; public string $b = "x"; }
#[AllowDynamicProperties]
class HashFive { public $a = 1; public $b = 2; public $c = 3; public $d = 4; public array $e = []; }
set_error_handler(function($no, $str) { echo "E[$no] $str\n"; return true; });
function pushIt(mixed $o) { $o->items[] = 7; return $o; }
function setIt(mixed $o) { $o->items['k'] = 7; return $o; }
function catIt(mixed $o) { $o->items['k'] .= 'z'; return $o; }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
foreach ([new HashNone, new HashTwo, new HashFive, new stdClass] as $o) {
    echo get_class($o), "\n";
    t(function() use ($o) { echo json_encode(pushIt($o)), "\n"; });
    t(function() use ($o) { echo json_encode(pushIt($o)), "\n"; });
    t(function() use ($o) { echo json_encode(setIt($o)), "\n"; });
    t(function() use ($o) { echo json_encode(catIt($o)), "\n"; });
}
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"HashNone
{"items":[7]}
{"items":[7,7]}
{"items":{"0":7,"1":7,"k":7}}
{"items":{"0":7,"1":7,"k":"7z"}}
HashTwo
{"a":1,"b":"x","items":[7]}
{"a":1,"b":"x","items":[7,7]}
{"a":1,"b":"x","items":{"0":7,"1":7,"k":7}}
{"a":1,"b":"x","items":{"0":7,"1":7,"k":"7z"}}
HashFive
{"a":1,"b":2,"c":3,"d":4,"e":[],"items":[7]}
{"a":1,"b":2,"c":3,"d":4,"e":[],"items":[7,7]}
{"a":1,"b":2,"c":3,"d":4,"e":[],"items":{"0":7,"1":7,"k":7}}
{"a":1,"b":2,"c":3,"d":4,"e":[],"items":{"0":7,"1":7,"k":"7z"}}
stdClass
{"items":[7]}
{"items":[7,7]}
{"items":{"0":7,"1":7,"k":7}}
{"items":{"0":7,"1":7,"k":"7z"}}
"####, "{}", out.stderr);
}

/// Element writes to private and protected properties from closures bound to a class (the scope is an object, a class name or `null` with a scope class): the write reaches the object's array, not a detached copy of it. php -n output.
#[test]
fn test_bound_closure_writes_reach_private_array_properties() {
    let out = compile_and_run_with_heap_debug(r####"<?php
set_error_handler(function($n,$s,$f,$l){ echo "E[$n] $s @$l\n"; return true; });
class Holder {
    public int $n = 7;
    private array $options = ['exceptions' => true];
    private array $list = [1];
    protected array $prot = [];
    public function shown(): string { return var_export($this->options['exceptions'], true) . json_encode($this->list) . json_encode($this->prot); }
}
function makeHolder(): mixed { return new Holder(); }
$gradual = makeHolder();
\Closure::bind(function () { $this->options['exceptions'] = false; }, $gradual, $gradual)();
echo $gradual->shown(), "\n";
\Closure::bind(function () { $this->list[] = 2; $this->list[0] .= 'x'; $this->prot['a'] = 1; $this->options['n']['m'] = 2; }, $gradual, Holder::class)();
echo $gradual->shown(), "\n";
\Closure::bind(function () { $this->list[0] .= "y"; unset($this->options['exceptions']); $this->prot[] = 9; $this->prot['a'] ??= 4; }, $gradual, Holder::class)();
echo $gradual->shown(), "\n";
$f = function ($o) { $o->list[] = 'viaarg'; return $o; };
echo \Closure::bind($f, null, Holder::class)($gradual)->shown(), "\n";
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"false[1][]
false["1x",2]{"a":1}
E[2] Undefined array key "exceptions" @8
NULL["1xy",2]{"a":1,"0":9}
E[2] Undefined array key "exceptions" @8
NULL["1xy",2,"viaarg"]{"a":1,"0":9}
"####, "{}", out.stderr);
}

/// A closure bound to a class writing array elements of a readonly property, of a slot a reference aliases, and of a property whose object its own body releases mid-write; a 3000-element loop of appends. php -n output.
#[test]
fn test_bound_closure_writes_respect_readonly_references_and_rebinding() {
    let out = compile_and_run_with_heap_debug(r####"<?php
set_error_handler(function($n,$s,$f,$l){ echo "E[$n] $s @$l\n"; return true; });
class Holder {
    private readonly array $ro;
    private array $plain = [];
    private array $ref = ['r' => 0];
    private array $big = [];
    public function __construct() { $this->ro = ['a' => 1]; }
    public function &refSlot() { return $this->ref; }
    public function dump() { return json_encode([$this->ro, $this->ref, count($this->big), count($this->plain)]); }
}
class Rebinder { private array $items = []; public static $log = []; public function __destruct() { echo "destruct\n"; } public function dump() { return json_encode($this->items); } }
function t(callable $f) { try { $f(); } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
$h = new Holder;
t(function() use ($h) { \Closure::bind(function () { $this->ro['b'] = 2; }, $h, $h)(); });
t(function() use ($h) { \Closure::bind(function () { $this->ro[] = 2; }, $h, $h)(); });
t(function() use ($h) { \Closure::bind(function () { $this->ro['a'] .= 'x'; }, $h, $h)(); });
$alias = &$h->refSlot();
t(function() use ($h) { \Closure::bind(function () { $this->ref['r'] = 5; $this->ref[] = 6; }, $h, $h)(); });
echo json_encode($alias), "\n";
$r = new Rebinder;
$g = \Closure::bind(function () use (&$r) { $this->items['a'] = 1; $r = null; $this->items['b'] = 2; }, $r, $r);
$g();
echo "after\n";
$n = 3000;
$h2 = new Holder;
\Closure::bind(function () use ($n) { for ($i = 0; $i < $n; $i++) { $this->big[] = $i; $this->plain['k' . ($i & 255)] = $i; } }, $h2, $h2)();
echo $h2->dump(), "\n";
"####);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, r####"Error: Cannot indirectly modify readonly property Holder::$ro
Error: Cannot indirectly modify readonly property Holder::$ro
Error: Cannot indirectly modify readonly property Holder::$ro
{"r":5,"0":6}
after
[{"a":1},{"r":0},3000,256]
destruct
"####, "{}", out.stderr);
}
