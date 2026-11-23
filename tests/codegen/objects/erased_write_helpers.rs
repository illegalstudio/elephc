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

/// Assembly bytes of `probe()` for one write form, in a program with `classes` `__set` classes
/// that declare the written property and, when `xml`, a `SimpleXMLElement` in the program.
fn probe_bytes(form: &str, classes: usize, xml: bool) -> usize {
    let declared: String = (0..classes)
        .map(|i| format!("class C{i} {{ public array $p = []; public function __set($n, $v) {{}} }}\n"))
        .collect();
    let make = if xml { "$n > 9 ? new SimpleXMLElement(\"<r/>\") : new H" } else { "new H" };
    let body = match form {
        "set" => "$o->p[\"k\"] = $v; return 1;",
        "ret" => "return $o->p[\"k\"] = $v;",
        "push" => "$o->p[] = $v; return 1;",
        "compound" => "$o->p[\"k\"] .= $v; return 1;",
        "coalesce" => "return $o->p[\"k\"] ??= $v;",
        "incdec" => "$o->p[\"k\"]++; return 1;",
        "assign" => "$o->p = $v; return 1;",
        other => panic!("unknown form {other}"),
    };
    let source = format!(
        "<?php\n{declared}class H {{ public array $p = []; protected array $q = []; }}\n\
         function make(int $n): mixed {{ return {make}; }}\n\
         function probe(mixed $o, mixed $v): mixed {{ {body} }}\n\
         echo json_encode(probe(make($argc), $argc));"
    );
    let dir = make_cli_test_dir("erased_write_size");
    let (asm, _runtime, _libs) = compile_source_to_asm_with_options(&source, &dir, 8_388_608, false, false);
    let _ = std::fs::remove_dir_all(&dir);
    let body = &asm[asm.find("fn_probe:").expect("the probe() label")..];
    body.find("fn_probe_epilogue:").expect("the probe() epilogue")
}

/// A site's assembly bytes (comments included) stay below this whatever the program holds: the
/// inline ladders were 150 000 bytes with SimpleXML in the program and 400 000 with twenty `__set`
/// classes beside it.
const MAX_SITE_BYTES: usize = 24_000;

/// One erased write is a call plus argument staging: it does not grow with the `__set` classes
/// or with SimpleXML, and stays near the size it had before the ladders were inlined.
#[test]
fn test_erased_property_writes_do_not_grow_with_classes_or_simplexml() {
    for form in ["set", "ret", "push", "compound", "coalesce", "incdec", "assign"] {
        let (few, many) = (probe_bytes(form, 1, false), probe_bytes(form, 20, false));
        let (xml_few, xml_many) = (probe_bytes(form, 1, true), probe_bytes(form, 20, true));
        assert!(many < few + 512, "{form}: grew with __set classes: {few} -> {many}");
        assert!(xml_many < xml_few + 512, "{form}: grew with __set classes beside SimpleXML: {xml_few} -> {xml_many}");
        assert!(xml_few < few + 1_024, "{form}: SimpleXML in the program grew the site: {few} -> {xml_few}");
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
