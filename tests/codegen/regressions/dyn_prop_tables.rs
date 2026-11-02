//! Purpose:
//! Regression tests for literal-name property writes on untyped (`Mixed`) receivers going through
//! one shared helper per name instead of a per-class arm at every site, and for the miss path no
//! longer repeating the arms the first ladder already answered.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The behaviour test compares with the output php prints and demands a clean `--heap-debug`
//!   leak summary: typed-slot coercions and their TypeErrors, readonly and visibility errors,
//!   scope-private names, `__set` and stdClass all go through the same helper.
//! - The assembly tests check that many sites call ONE helper per name on both targets, that the
//!   environment switch restores the inline ladders, and that the helper is smaller than the
//!   sites it replaces.

use crate::support::*;
use std::fs;

/// Typed and untyped declared slots across several classes, coercion and TypeErrors, readonly and
/// visibility refusals from outside the class, a scope-private name written from inside its class,
/// a subclass that redeclares a parent's private name as public, `__set` and stdClass.
#[test]
fn test_named_mixed_property_writes_through_the_shared_helper_match_php() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Plain { public $a = 1; public ?int $n = null; public int $i = 0; public string $s = ""; public float $f = 0.0; public array $arr = []; }
class Plain2 { public $a = 2; public ?int $n = 5; public int $i = 0; public string $s = ""; public float $f = 0.0; public array $arr = []; }
class Plain3 { public $a = 3; public ?int $n = 5; public int $i = 0; public string $s = ""; public float $f = 0.0; public array $arr = []; }
class Ro { public function __construct(public readonly int $r = 1, public $a = 0) {} }
class Priv { private $p = 1; protected $q = 2; public $a = 0;
    public function setP($o, $v) { $o->p = $v; $o->q = $v; return [$o->p ?? null, $o->q ?? null]; }
    public function dump() { return $this->p . "/" . $this->q; } }
class PrivChild extends Priv { public $p = 'child-public';
    public function setP2($o, $v) { $o->p = $v; } }
class Magic { private $data = []; public $a = 0; public function __set($n, $v) { echo "__set($n)\n"; $this->data[$n] = $v; } }

function setA($o, $v) { $o->a = $v; }
function setN($o, $v) { $o->n = $v; }
function setI($o, $v) { $o->i = $v; }
function setS($o, $v) { $o->s = $v; }
function setF($o, $v) { $o->f = $v; }
function setArr($o, $v) { $o->arr = $v; }
function setR($o, $v) { $o->r = $v; }
function setP($o, $v) { $o->p = $v; }
function setQ($o, $v) { $o->q = $v; }
function t(string $label, callable $f) {
    try { $f(); echo "$label ok\n"; } catch (Throwable $e) { echo "$label ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
foreach ([new Plain, new Plain2, new Plain3] as $o) {
    setA($o, 10); setN($o, 7); setN($o, null); setI($o, "42"); setS($o, 5); setF($o, 3); setArr($o, [1, 2]);
    echo json_encode($o), "\n";
    t("badI", fn() => setI($o, "abc"));
    t("badArr", fn() => setArr($o, "x"));
    t("badN", fn() => setN($o, []));
}
$a = new stdClass; setA($a, 5); echo json_encode($a), "\n";
$r = new Ro(3); t("ro-outside", fn() => setR($r, 9)); echo $r->r, "\n";
$p = new Priv; t("priv-outside", fn() => setP($p, 9)); t("prot-outside", fn() => setQ($p, 9)); t("priv-inside", fn() => print_r($p->setP($p, 5)));
echo $p->dump(), "\n";
$c = new PrivChild; t("child-public-p", fn() => setP($c, 'x')); echo $c->p, "\n"; t("child-inside", fn() => $c->setP2($c, 'y')); echo $c->p, "\n"; t("child-priv-q", fn() => $c->setP($c, 1));
$m = new Magic; setA($m, 1);
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"a\":10,\"n\":null,\"i\":42,\"s\":\"5\",\"f\":3,\"arr\":[1,2]}\nbadI TypeError: Cannot assign string to property Plain::$i of type int\nbadArr TypeError: Cannot assign string to property Plain::$arr of type array\nbadN TypeError: Cannot assign array to property Plain::$n of type ?int\n{\"a\":10,\"n\":null,\"i\":42,\"s\":\"5\",\"f\":3,\"arr\":[1,2]}\nbadI TypeError: Cannot assign string to property Plain2::$i of type int\nbadArr TypeError: Cannot assign string to property Plain2::$arr of type array\nbadN TypeError: Cannot assign array to property Plain2::$n of type ?int\n{\"a\":10,\"n\":null,\"i\":42,\"s\":\"5\",\"f\":3,\"arr\":[1,2]}\nbadI TypeError: Cannot assign string to property Plain3::$i of type int\nbadArr TypeError: Cannot assign string to property Plain3::$arr of type array\nbadN TypeError: Cannot assign array to property Plain3::$n of type ?int\n{\"a\":5}\nro-outside Error: Cannot modify readonly property Ro::$r\n3\npriv-outside Error: Cannot access private property Priv::$p\nprot-outside Error: Cannot access protected property Priv::$q\nArray\n(\n    [0] => 5\n    [1] => 5\n)\npriv-inside ok\n5/5\nchild-public-p ok\nx\nchild-inside ok\ny\nchild-priv-q ok\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A property created on an `#[AllowDynamicProperties]` instance through an untyped receiver (a
/// parameter, a Mixed array element, a property chain, a runtime name) is kept AND encoded:
/// `json_encode()` used to walk the declared rows only and print `{"a":0}` for `{"a":0,"fresh":1}`.
/// A `JsonSerializable` class keeps its method, an empty instance stays `{}`, pretty printing and
/// nested instances follow, and the temporary projection leaves no leak behind.
#[test]
fn test_json_encode_includes_dynamic_properties_of_allow_dynamic_classes() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
#[AllowDynamicProperties] class Dyn { public $a = 0; public ?int $n = null; private $hidden = 9; }
#[AllowDynamicProperties] class Empty0 { }
#[AllowDynamicProperties] class Js implements JsonSerializable { public $a = 1; public function jsonSerialize(): mixed { return ['js' => $this->a]; } }
class Holder { public $o; public $list = []; public function __construct() { $this->o = new Dyn; } }
function viaParam($o) { $o->fresh = 1; return $o; }
function viaElem(array $arr) { $arr[0]->fresh = 3; return $arr[0]; }
function viaChain($h) { $h->o->fresh = 4; return $h->o; }
function viaName($o, $k, $v) { $o->$k = $v; return $o; }
$d = viaParam(new Dyn);
echo json_encode($d), "\n";
echo json_encode(viaElem([new Dyn])), "\n";
echo json_encode(viaChain(new Holder)), "\n";
$h = new Holder; $h->o->x = [1, 2]; $h->o->y = "s"; $h->list[] = $h->o;
echo json_encode($h), "\n";
echo json_encode([$d, 5, new Dyn]), "\n";
echo json_encode($d, JSON_PRETTY_PRINT), "\n";
$e = new Empty0; echo json_encode($e), "\n"; $e->z = 1.5; echo json_encode($e), "\n";
echo json_encode(viaName(new Dyn, "k", null)), "\n";
$j = new Js; $j->extra = 2; echo json_encode($j), "\n";
echo json_encode((object) ["q" => 1]), "\n";
echo $d->fresh, "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"a\":0,\"n\":null,\"fresh\":1}\n{\"a\":0,\"n\":null,\"fresh\":3}\n{\"a\":0,\"n\":null,\"fresh\":4}\n{\"o\":{\"a\":0,\"n\":null,\"x\":[1,2],\"y\":\"s\"},\"list\":[{\"a\":0,\"n\":null,\"x\":[1,2],\"y\":\"s\"}]}\n[{\"a\":0,\"n\":null,\"fresh\":1},5,{\"a\":0,\"n\":null}]\n{\n    \"a\": 0,\n    \"n\": null,\n    \"fresh\": 1\n}\n{}\n{\"z\":1.5}\n{\"a\":0,\"n\":null,\"k\":null}\n{\"js\":1}\n{\"q\":1}\n1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new $class(...)` on classes that INHERIT a constructor (variadic, optional and plain) builds
/// them through the padding thunk of the class that implements it; output matches php.
#[test]
fn test_dynamic_new_of_inherited_constructors_matches_php() {
    let out = compile_and_run(
        r####"<?php
class Base { public $log = ""; public function __construct(int ...$r) { $this->log = "Base:" . implode(",", $r); } }
class D1 extends Base {}
class D2 extends Base {}
class D3 extends D1 {}
class Own extends Base { public function __construct(int ...$r) { parent::__construct(...$r); $this->log .= "+Own"; } }
class OwnChild extends Own {}
class Opt { public $log = ""; public function __construct(public int $a = 7, public string $b = "dflt") { $this->log = "Opt:$a:$b"; } }
class OptKid extends Opt {}
class OptKid2 extends Opt { public $extra = 1; }
class Tail { public $log; public function __construct($x, ...$rest) { $this->log = "Tail:$x:" . count($rest); } }
class TailKid extends Tail {}
function make(string $c, ...$args) {
    switch (count($args)) {
        case 0: return new $c();
        case 1: return new $c($args[0]);
        case 2: return new $c($args[0], $args[1]);
        default: return new $c($args[0], $args[1], $args[2]);
    }
}
foreach (["Base", "D1", "D2", "D3", "Own", "OwnChild"] as $c) {
    echo $c, ": ", make($c)->log, " | ", make($c, 1)->log, " | ", make($c, 1, 2)->log, " | ", make($c, 1, 2, 3)->log, "\n";
}
foreach (["Opt", "OptKid", "OptKid2"] as $c) {
    echo $c, ": ", make($c)->log, " | ", make($c, 1)->log, " | ", make($c, 1, "x")->log, "\n";
}
foreach (["Tail", "TailKid"] as $c) {
    echo $c, ": ", make($c, 1)->log, " | ", make($c, 1, 2)->log, " | ", make($c, 1, 2, 3)->log, "\n";
}
echo get_class(make("D3", 5)), " ", make("OptKid2")->extra, "\n";
try { make("D1", "abc"); } catch (Throwable $e) { echo get_class($e), "\n"; }
"####,
    );
    assert_eq!(out, "Base: Base: | Base:1 | Base:1,2 | Base:1,2,3\nD1: Base: | Base:1 | Base:1,2 | Base:1,2,3\nD2: Base: | Base:1 | Base:1,2 | Base:1,2,3\nD3: Base: | Base:1 | Base:1,2 | Base:1,2,3\nOwn: Base:+Own | Base:1+Own | Base:1,2+Own | Base:1,2,3+Own\nOwnChild: Base:+Own | Base:1+Own | Base:1,2+Own | Base:1,2,3+Own\nOpt: Opt:7:dflt | Opt:1:dflt | Opt:1:x\nOptKid: Opt:7:dflt | Opt:1:dflt | Opt:1:x\nOptKid2: Opt:7:dflt | Opt:1:dflt | Opt:1:x\nTail: Tail:1:0 | Tail:1:1 | Tail:1:2\nTailKid: Tail:1:0 | Tail:1:1 | Tail:1:2\nD3 1\nTypeError\n");
}

/// A padding thunk exists once per constructor IMPLEMENTATION and arity, not once per
/// instantiated class: 20 and 80 descendants of the same constructors emit the same number of
/// thunks. (They were emitted per descendant: 4915 of them, 2.7 M instructions, in Symfony.)
#[test]
fn test_dynamic_new_padding_thunks_do_not_scale_with_inheriting_classes() {
    fn source(classes: usize) -> String {
        let mut php = String::from(
            "<?php\nclass Base { public $log = ''; public function __construct(int ...$r) { $this->log = 'B' . count($r); } }\nclass Opt { public $log = ''; public function __construct(public int $a = 7, public string $b = 'd') { $this->log = 'O' . $a; } }\n",
        );
        for i in 0..classes {
            php.push_str(&format!("class V{i} extends Base {{}}\nclass P{i} extends Opt {{}}\n"));
        }
        php.push_str("function mk(string $c, $x, $y) { return [new $c(), new $c($x), new $c($x, $y)]; }\n$names = [");
        for i in 0..classes {
            php.push_str(&format!("'V{i}', 'P{i}', "));
        }
        php.push_str("];\nforeach ($names as $c) { foreach (mk($c, 1, 2) as $o) { echo $o->log, ','; } }\n");
        php
    }
    let thunks = |classes: usize, target: &str| {
        let asm = emit_asm_with_env("elephc_ctor_thunks", &source(classes), target, &[]);
        asm.lines().filter(|line| line.contains("; @fn name=_class_ctor_")).count()
    };
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = thunks(20, target);
        let large = thunks(80, target);
        assert_eq!(small, large, "{target}: thunks must not grow with the inheriting classes");
    }
}

/// Compiles `source` with `--emit-asm` for `target`, with extra environment, and returns the
/// assembly text.
fn emit_asm_with_env(prefix: &str, source: &str, target: &str, env: &[(&str, &str)]) -> String {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command
        .arg("--target")
        .arg(target)
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI with --emit-asm");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("main.s")).expect("failed to read emitted assembly");
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// Counts the assembly lines that define a label starting with `prefix`.
fn count_definitions(asm: &str, prefix: &str) -> usize {
    asm.lines()
        .filter(|line| line.starts_with(prefix) && line.trim_end().ends_with("_0:"))
        .count()
}

/// Counts the assembly lines that call a label starting with `prefix`.
fn count_calls(asm: &str, prefix: &str) -> usize {
    asm.lines()
        .filter(|line| {
            let line = line.trim();
            (line.starts_with("bl ") || line.starts_with("call ")) && line.contains(prefix)
        })
        .count()
}

/// Four classes declare `a` (untyped) and `n` (typed); five sites write `a`, three write `n`.
const MANY_SITES: &str = r#"<?php
class W1 { public $a = 1; public ?int $n = null; }
class W2 { public $a = 2; public ?int $n = null; }
class W3 { public $a = 3; public ?int $n = null; }
class W4 { public $a = 4; public ?int $n = null; }
function w1($o) { $o->a = 1; $o->n = 1; }
function w2($o) { $o->a = 2; $o->n = 2; }
function w3($o) { $o->a = 3; $o->n = 3; }
function w4($o) { $o->a = 4; }
function w5($o) { $o->a = 5; }
foreach ([new W1, new W2, new W3, new W4] as $o) { w1($o); w2($o); w3($o); w4($o); w5($o); echo $o->a, $o->n ?? 'x', "\n"; }
"#;

/// Eight sites write two names: two helpers, every site calls one, on both targets; the switch
/// brings the per-site ladders back.
#[test]
fn test_named_mixed_property_writes_share_one_helper_per_name() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let asm = emit_asm_with_env("elephc_named_set", MANY_SITES, target, &[]);
        let defs = count_definitions(&asm, "_eir_shared_named_prop_set_");
        let calls = count_calls(&asm, "_eir_shared_named_prop_set_");
        assert_eq!(defs, 2, "{target}: expected one helper per name, found {defs}");
        assert_eq!(calls, 8, "{target}: expected every site to call a helper, found {calls}");
    }
    let off = emit_asm_with_env(
        "elephc_named_set_off",
        MANY_SITES,
        "macos-aarch64",
        &[("ELEPHC_NO_OUTLINED_DYNAMIC_PROPS", "1")],
    );
    assert_eq!(count_definitions(&off, "_eir_shared_named_prop_set_"), 0);
    assert_eq!(count_calls(&off, "_eir_shared_named_prop_set_"), 0);
}

/// Closures written in three different classes (and bound to a fourth scope, or to none) write and
/// remove runtime-named properties on one target. Every closure carries the hidden scope capture,
/// so each takes the runtime-scope helper; the output is php's, scope refusals included, and the
/// heap stays clean.
#[test]
fn test_runtime_scope_dynamic_property_helper_matches_php_for_every_lexical_class() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
#[AllowDynamicProperties] class Target { private $secret = 's0'; protected $prot = 'p0'; public $pub = 'u0'; public ?int $n = null; }
class ScopeA {
    public static function make() { return function ($o, $name, $v) { $o->$name = $v; }; }
    public static function unmake() { return function ($o, $name) { unset($o->$name); }; }
}
class ScopeB {
    private $own = 'b';
    public static function make() { return function ($o, $name, $v) { $o->$name = $v; }; }
    public static function unmake() { return function ($o, $name) { unset($o->$name); }; }
}
class ScopeC {
    public static function make() { return function ($o, $name, $v) { $o->$name = $v; }; }
    public static function unmake() { return function ($o, $name) { unset($o->$name); }; }
}
function t($label, $f) { try { $f(); echo "$label ok\n"; } catch (Throwable $e) { echo "$label ", get_class($e), ": ", $e->getMessage(), "\n"; } }
function dump($o) { static $r = null; $r ??= Closure::bind(fn($o) => json_encode([$o->secret ?? "-", $o->prot ?? "-", $o->pub ?? "-", $o->n ?? "-", $o->fresh ?? "-"]), null, Target::class); echo $r($o), "\n"; }
$val = $argc > 5 ? 1 : "v";
foreach (['A', 'B', 'C'] as $k) {
    $cls = "Scope$k";
    $set = $cls::make();
    $o = new Target;
    t("$k pub", fn() => $set($o, 'pub', "$val-$k"));
    t("$k secret", fn() => $set($o, 'secret', "$val-$k"));
    t("$k prot", fn() => $set($o, 'prot', "$val-$k"));
    t("$k n", fn() => $set($o, 'n', "bad"));
    dump($o);
    $bound = Closure::bind($set, null, Target::class);
    t("$k bound secret", fn() => $bound($o, 'secret', "b$val$k"));
    t("$k bound prot", fn() => $bound($o, 'prot', "b$val$k"));
    t("$k bound n", fn() => $bound($o, 'n', 5));
    dump($o);
    $other = Closure::bind($set, null, $k === 'A' ? ScopeB::class : ScopeA::class);
    t("$k other secret", fn() => $other($o, 'secret', "o"));
    t("$k other pub", fn() => $other($o, 'pub', "o$k"));
    dump($o);
    $unset = $cls::unmake();
    t("$k unset pub", fn() => $unset($o, 'pub'));
    t("$k unset secret", fn() => $unset($o, 'secret'));
    $ub = Closure::bind($unset, null, Target::class);
    t("$k bound unset secret", fn() => $ub($o, 'secret'));
    t("$k dyn", fn() => $set($o, 'fresh', 1));
    dump($o);
}
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "A pub ok\nA secret Error: Cannot access private property Target::$secret\nA prot Error: Cannot access protected property Target::$prot\nA n TypeError: Cannot assign string to property Target::$n of type ?int\n[\"s0\",\"p0\",\"v-A\",\"-\",\"-\"]\nA bound secret ok\nA bound prot ok\nA bound n ok\n[\"bvA\",\"bvA\",\"v-A\",5,\"-\"]\nA other secret Error: Cannot access private property Target::$secret\nA other pub ok\n[\"bvA\",\"bvA\",\"oA\",5,\"-\"]\nA unset pub ok\nA unset secret Error: Cannot access private property Target::$secret\nA bound unset secret ok\nA dyn ok\n[\"-\",\"bvA\",\"-\",5,1]\nB pub ok\nB secret Error: Cannot access private property Target::$secret\nB prot Error: Cannot access protected property Target::$prot\nB n TypeError: Cannot assign string to property Target::$n of type ?int\n[\"s0\",\"p0\",\"v-B\",\"-\",\"-\"]\nB bound secret ok\nB bound prot ok\nB bound n ok\n[\"bvB\",\"bvB\",\"v-B\",5,\"-\"]\nB other secret Error: Cannot access private property Target::$secret\nB other pub ok\n[\"bvB\",\"bvB\",\"oB\",5,\"-\"]\nB unset pub ok\nB unset secret Error: Cannot access private property Target::$secret\nB bound unset secret ok\nB dyn ok\n[\"-\",\"bvB\",\"-\",5,1]\nC pub ok\nC secret Error: Cannot access private property Target::$secret\nC prot Error: Cannot access protected property Target::$prot\nC n TypeError: Cannot assign string to property Target::$n of type ?int\n[\"s0\",\"p0\",\"v-C\",\"-\",\"-\"]\nC bound secret ok\nC bound prot ok\nC bound n ok\n[\"bvC\",\"bvC\",\"v-C\",5,\"-\"]\nC other secret Error: Cannot access private property Target::$secret\nC other pub ok\n[\"bvC\",\"bvC\",\"oC\",5,\"-\"]\nC unset pub ok\nC unset secret Error: Cannot access private property Target::$secret\nC bound unset secret ok\nC dyn ok\n[\"-\",\"bvC\",\"-\",5,1]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Emits `scopes` classes whose closures each write and remove a runtime-named property.
fn runtime_scope_program(scopes: usize) -> String {
    let mut source = String::from("<?php\n#[AllowDynamicProperties] class T { public $a = 1; public ?int $n = null; private $p = 2; }\nclass Other { public $b = 1; }\n");
    for index in 0..scopes {
        source.push_str(&format!(
            "class S{index} {{ public static function w() {{ return function ($o, $name, $v) {{ $o->$name = $v; }}; }} public static function u() {{ return function ($o, $name) {{ unset($o->$name); }}; }} }}\n"
        ));
    }
    source.push_str("$t = new T;\n");
    for index in 0..scopes {
        source.push_str(&format!("S{index}::w()($t, 'a', {index}); S{index}::u()($t, 'a');\n"));
    }
    source.push_str("echo count(get_object_vars($t)), \"\\n\";\n");
    source
}

/// The runtime-scope write/removal helpers hold the whole `(class, name)` ladder, so they must be
/// emitted ONCE for the program, not once per lexical class that binds a scope. Tripling the number
/// of scope-carrying classes must not add a helper definition, on both targets.
#[test]
fn test_runtime_scope_dynamic_property_helpers_are_shared_across_lexical_classes() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let few = emit_asm_with_env("elephc_rs_few", &runtime_scope_program(2), target, &[]);
        let many = emit_asm_with_env("elephc_rs_many", &runtime_scope_program(6), target, &[]);
        for prefix in ["_eir_shared_dyn_prop_set_", "_eir_shared_dyn_prop_unset_"] {
            let before = count_definitions(&few, prefix);
            let after = count_definitions(&many, prefix);
            assert!(before >= 1, "{target}: expected a {prefix} helper in the small program");
            assert_eq!(
                before, after,
                "{target}: {prefix} helpers must not scale with the scope classes ({before} -> {after})"
            );
        }
    }
}

/// `count` functions that each build one array literal of boxed values and one of strings.
fn array_literal_program(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        source.push_str(&format!(
            "function lit{index}(mixed $a, mixed $b) {{ $x = [$a, $b]; $y = ['p{index}', 'q']; return count($x) + count($y); }}\n"
        ));
    }
    source.push_str("$t = 0;\n");
    for index in 0..count {
        source.push_str(&format!("$t += lit{index}(1, 'two');\n"));
    }
    source.push_str("echo $t, \"\\n\";\n");
    source
}

/// `count` functions that each return a `mixed`-element array literal through a `mixed` return.
fn container_boxing_program(count: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..count {
        source.push_str(&format!(
            "function box{index}(mixed $a, mixed $b): mixed {{ $x = [$a, $b, {index}]; return $x; }}\n"
        ));
    }
    source.push_str("$t = 0;\n");
    for index in 0..count {
        source.push_str(&format!("$t += count(box{index}(1, 'two'));\n"));
    }
    source.push_str("echo $t, \"\\n\";\n");
    source
}

/// Boxing a dynamically shaped container (a `mixed`-element array, an iterable) is ONE call into
/// `__rt_mixed_box_container_<kind>`, not the heap-kind probe and tag mapping at every site.
#[test]
fn test_dynamic_container_boxing_is_one_call_per_site() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let few = emit_asm_with_env("elephc_box_few", &container_boxing_program(2), target, &[]);
        let many = emit_asm_with_env("elephc_box_many", &container_boxing_program(10), target, &[]);
        let probes = |asm: &str| {
            asm.lines()
                .filter(|line| {
                    let line = line.trim();
                    line == "cset x11, ls" || line == "setbe dl"
                })
                .count()
        };
        let calls = |asm: &str| {
            asm.lines()
                .filter(|line| {
                    let line = line.trim();
                    (line.starts_with("bl ") || line.starts_with("call "))
                        && line.contains("__rt_mixed_box_container_")
                })
                .count()
        };
        assert_eq!(
            probes(&few),
            probes(&many),
            "{target}: the heap-kind range probe must not be repeated per boxing site"
        );
        assert!(
            calls(&many) >= calls(&few) + 8,
            "{target}: expected one routine call per boxing site ({} -> {})",
            calls(&few),
            calls(&many)
        );
    }
}

/// Every array-creation site stamps the header's value type with ONE call into the shared
/// `__rt_array_stamp_<tag>` routine; the read-modify-write sequence (`0x80ff` mask, shift, or) must
/// not be repeated per site. Going from 2 to 10 literal-building functions must add stamp calls
/// but no inline mask constants, on both targets.
#[test]
fn test_array_literal_value_type_stamp_is_one_call_per_site() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let few = emit_asm_with_env("elephc_stamp_few", &array_literal_program(2), target, &[]);
        let many = emit_asm_with_env("elephc_stamp_many", &array_literal_program(10), target, &[]);
        let masks = |asm: &str| asm.lines().filter(|line| line.contains("0x80ff")).count();
        let calls = |asm: &str| {
            asm.lines()
                .filter(|line| {
                    let line = line.trim();
                    (line.starts_with("bl ") || line.starts_with("call ")) && line.contains("__rt_array_stamp_")
                })
                .count()
        };
        assert_eq!(
            masks(&few),
            masks(&many),
            "{target}: the stamp mask must not be repeated per array-creation site"
        );
        assert!(
            calls(&many) >= calls(&few) + 16,
            "{target}: expected one stamp call per site ({} -> {})",
            calls(&few),
            calls(&many)
        );
    }
}

/// Typed, untyped, readonly-neutral, private/protected and shadowed properties, magic accessors,
/// dynamic properties and stdClass, written, read and unset through a runtime name on instances of
/// a three-level hierarchy. The arms answering them are shared per subtree, so the output (the
/// TypeError text naming the DECLARING class, the visibility refusals naming the RECEIVER's class,
/// the hidden private slot) must stay exactly php's, and the heap clean.
#[test]
fn test_runtime_name_property_ladders_match_php_across_a_hierarchy() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Base {
    public int $i = 1;
    public ?string $s = null;
    public $u = 'u';
    public readonly int $ro;
    protected int $prot = 5;
    private int $priv = 6;
    public array $arr = [];
    public function __construct() { $this->ro = 7; }
    public function setPriv($name, $v) { $this->$name = $v; return $this->priv; }
    public function readPriv($name) { return $this->$name; }
}
class Mid extends Base { public float $f = 1.5; protected $extra = 'x'; }
class Leaf extends Mid { public int $i = 10; }
class ShadowParent {
    private $p = 'parent-private';
    public function getP() { return $this->p; }
    public function setP($v) { $this->p = $v; }
}
class ShadowChild extends ShadowParent { public $p = 'child-public'; }
#[AllowDynamicProperties] class Dyn { public $a = 1; public int $n = 0; }
class Magic {
    private $data = [];
    public $real = 0;
    public function __set($n, $v) { $this->data[$n] = $v; echo "__set($n)\n"; }
    public function __get($n) { echo "__get($n)\n"; return $this->data[$n] ?? null; }
}
function w($o, $n, $v) {
    try { $o->$n = $v; echo get_class($o), "::$n ok\n"; }
    catch (Throwable $e) { echo get_class($o), "::$n ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
function r($o, $n) {
    try { $v = $o->$n; echo get_class($o), "::$n = ", json_encode($v), "\n"; }
    catch (Throwable $e) { echo get_class($o), "::$n ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
function u($o, $n) {
    try { unset($o->$n); echo get_class($o), "::$n unset\n"; }
    catch (Throwable $e) { echo get_class($o), "::$n ", get_class($e), ": ", $e->getMessage(), "\n"; }
}
$bad = $argc > 5 ? 1 : 'abc';
$arr = $argc > 5 ? 1 : [1, 2];
foreach ([new Base, new Mid, new Leaf] as $o) {
    w($o, 'i', 5); w($o, 'i', $bad); w($o, 's', 'three'); w($o, 's', null); w($o, 'u', [1]);
    w($o, 'prot', 1); w($o, 'arr', $arr); w($o, 'arr', $bad);
    r($o, 'i'); r($o, 's'); r($o, 'u'); r($o, 'prot'); r($o, 'ro'); r($o, 'arr');
    echo $o->setPriv('priv', 11), " ", json_encode($o->readPriv('priv')), " ", json_encode($o->readPriv('prot')), "\n";
    u($o, 'u'); u($o, 'prot');
}
$b = new Base;
w($b, 'priv', 1); r($b, 'priv'); u($b, 'priv');
$m = new Mid;
w($m, 'f', 3); w($m, 'f', $bad); r($m, 'f'); w($m, 'extra', 'y'); r($m, 'extra');
$l = new Leaf;
w($l, 'i', '42'); r($l, 'i'); w($l, 'i', $bad);
$c = new ShadowChild;
w($c, 'p', 'x'); r($c, 'p'); echo $c->getP(), "\n"; $c->setP('y'); echo $c->getP(), "|", $c->p, "\n";
$sp = new ShadowParent;
w($sp, 'p', 'z'); r($sp, 'p');
$g = new Magic;
w($g, 'real', 5); w($g, 'zzz', 1); r($g, 'zzz'); r($g, 'real'); w($g, 'data', 1);
$d = new Dyn;
w($d, 'a', 2); w($d, 'newprop', 3); r($d, 'newprop'); u($d, 'newprop'); w($d, 'n', 4); w($d, 'n', $bad); r($d, 'n');
$s = new stdClass;
w($s, 'x', 1); r($s, 'x'); u($s, 'x');
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Base::i ok\nBase::i TypeError: Cannot assign string to property Base::$i of type int\nBase::s ok\nBase::s ok\nBase::u ok\nBase::prot Error: Cannot access protected property Base::$prot\nBase::arr ok\nBase::arr TypeError: Cannot assign string to property Base::$arr of type array\nBase::i = 5\nBase::s = null\nBase::u = [1]\nBase::prot Error: Cannot access protected property Base::$prot\nBase::ro = 7\nBase::arr = [1,2]\n11 11 5\nBase::u unset\nBase::prot Error: Cannot access protected property Base::$prot\nMid::i ok\nMid::i TypeError: Cannot assign string to property Base::$i of type int\nMid::s ok\nMid::s ok\nMid::u ok\nMid::prot Error: Cannot access protected property Mid::$prot\nMid::arr ok\nMid::arr TypeError: Cannot assign string to property Base::$arr of type array\nMid::i = 5\nMid::s = null\nMid::u = [1]\nMid::prot Error: Cannot access protected property Mid::$prot\nMid::ro = 7\nMid::arr = [1,2]\n11 11 5\nMid::u unset\nMid::prot Error: Cannot access protected property Mid::$prot\nLeaf::i ok\nLeaf::i TypeError: Cannot assign string to property Leaf::$i of type int\nLeaf::s ok\nLeaf::s ok\nLeaf::u ok\nLeaf::prot Error: Cannot access protected property Leaf::$prot\nLeaf::arr ok\nLeaf::arr TypeError: Cannot assign string to property Base::$arr of type array\nLeaf::i = 5\nLeaf::s = null\nLeaf::u = [1]\nLeaf::prot Error: Cannot access protected property Leaf::$prot\nLeaf::ro = 7\nLeaf::arr = [1,2]\n11 11 5\nLeaf::u unset\nLeaf::prot Error: Cannot access protected property Leaf::$prot\nBase::priv Error: Cannot access private property Base::$priv\nBase::priv Error: Cannot access private property Base::$priv\nBase::priv Error: Cannot access private property Base::$priv\nMid::f ok\nMid::f TypeError: Cannot assign string to property Mid::$f of type float\nMid::f = 3\nMid::extra Error: Cannot access protected property Mid::$extra\nMid::extra Error: Cannot access protected property Mid::$extra\nLeaf::i ok\nLeaf::i = 42\nLeaf::i TypeError: Cannot assign string to property Leaf::$i of type int\nShadowChild::p ok\nShadowChild::p = \"x\"\nparent-private\ny|x\nShadowParent::p Error: Cannot access private property ShadowParent::$p\nShadowParent::p Error: Cannot access private property ShadowParent::$p\nMagic::real ok\n__set(zzz)\nMagic::zzz ok\n__get(zzz)\nMagic::zzz = 1\nMagic::real = 5\n__set(data)\nMagic::data ok\nDyn::a ok\nDyn::newprop ok\nDyn::newprop = 3\nDyn::newprop unset\nDyn::n ok\nDyn::n TypeError: Cannot assign string to property Dyn::$n of type int\nDyn::n = 4\nstdClass::x ok\nstdClass::x = 1\nstdClass::x unset\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `classes` subclasses of one base, written, read and unset through runtime names.
fn hierarchy_program(classes: usize) -> String {
    let mut source = String::from(
        "<?php\nclass Base { public int $i = 1; public ?string $s = null; public $u = 1; protected int $prot = 5; private int $priv = 6; }\n",
    );
    for index in 0..classes {
        source.push_str(&format!("class K{index} extends Base {{ public float $f{index} = 1.5; }}\n"));
    }
    source.push_str(
        "function w($o, $n, $v) { $o->$n = $v; }\nfunction r($o, $n) { return $o->$n; }\nfunction u($o, $n) { unset($o->$n); }\n$bad = $argc > 5 ? 1 : 'x';\n",
    );
    for index in 0..classes {
        source.push_str(&format!("$o = new K{index}; w($o, 'i', 3); w($o, 'u', 'v'); echo r($o, 'i'), r($o, 'u'); u($o, 'u');\n"));
    }
    source
}

/// The instruction lines inside every function whose label starts with `prefix`.
fn instructions_in_functions(asm: &str, prefix: &str) -> usize {
    let mut inside = false;
    let mut count = 0;
    for line in asm.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') && line.ends_with(':') {
            let label = line.trim_end_matches(':');
            if !label.starts_with("L_") && !label.starts_with(".L") && !label.starts_with("L") {
                inside = label.starts_with(prefix);
            }
            continue;
        }
        let trimmed = line.trim_start();
        if inside && !trimmed.is_empty() && !trimmed.starts_with('.') && !trimmed.starts_with(';') && !trimmed.starts_with('@') {
            count += 1;
        }
    }
    count
}

/// The runtime-name ladders hold one arm per (property, subtree), not per (class, property): a
/// subclass that inherits its parent's slots adds no arm. Quadrupling the number of subclasses must
/// not grow any shared ladder, on both targets; with the grouping switched off the same programs
/// grow, which is what proves the test sees the ladders.
#[test]
fn test_runtime_name_ladders_do_not_grow_with_inheriting_classes() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_ladder_small", &hierarchy_program(20), target, &[]);
        let large = emit_asm_with_env("elephc_ladder_large", &hierarchy_program(80), target, &[]);
        for prefix in ["_eir_shared_dyn_prop_set_", "_eir_shared_dyn_prop_get_", "_eir_shared_dyn_prop_unset_"] {
            let before = instructions_in_functions(&small, prefix);
            let after = instructions_in_functions(&large, prefix);
            assert!(before > 0, "{target}: no {prefix} helper in the 20-class program");
            // Each subclass adds its own `f<n>` property, so allow ~40 instructions per class for it
            // (~90 for the read and removal ladders, whose class-id search also
            // gains an entry per class), but nothing like the inherited properties times the class
            // count: ungrouped, each class added ~8 arms of ~40 instructions per helper.
            let per_class = if prefix.contains("_get_") {
                90
            } else if prefix.contains("_unset_") {
                90
            } else {
                40
            };
            assert!(
                after <= before + per_class * 60,
                "{target}: {prefix} ladder grew with inheriting classes ({before} -> {after})"
            );
        }
        let ungrouped = emit_asm_with_env(
            "elephc_ladder_off",
            &hierarchy_program(80),
            target,
            &[("ELEPHC_NO_ARM_GROUPS", "1")],
        );
        let grouped_total = instructions_in_functions(&large, "_eir_shared_dyn_prop_");
        let ungrouped_total = instructions_in_functions(&ungrouped, "_eir_shared_dyn_prop_");
        assert!(
            ungrouped_total > grouped_total + grouped_total / 4,
            "{target}: grouping saved too little ({ungrouped_total} -> {grouped_total})"
        );
    }
}

/// `readonly` through a name only known at run time. An initialized property refuses a write and a
/// removal from every scope; an uninitialized one is `protected(set)`, so only its class and the
/// subclasses may initialize it. The receiver is untyped, typed, `$this` inside the class, `$this`
/// inside a subclass and an unrelated class, with named and runtime-name forms. The text is php's,
/// naming the DECLARING class. The one inexact corner is a write from an unrelated class scope
/// into an uninitialized property through a shared helper, which says `global scope`; it is kept
/// out of this program.
#[test]
fn test_readonly_properties_refuse_runtime_name_writes_and_unsets_like_php() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Base {
    public readonly int $ro;
    public readonly int $late;
    public function __construct() { $this->ro = 7; }
    public function initLate($v) { $this->late = $v; }
    public function initLateByName($n, $v) { $this->$n = $v; }
    public function setByName($n, $v) { $this->$n = $v; }
    public function unsetByName($n) { unset($this->$n); }
}
class Child extends Base { public function childInit($n, $v) { $this->$n = $v; } public function childZap($n) { unset($this->$n); } }
class Other { public function poke($o, $n, $v) { $o->$n = $v; } public function zap($o, $n) { unset($o->$n); } }
function e($f) { try { $f(); echo "ok\n"; } catch (Throwable $x) { echo get_class($x), ": ", $x->getMessage(), "\n"; } }
function wm($o, $n, $v) { $o->$n = $v; }
function wt(Base $o, $n, $v) { $o->$n = $v; }
function um($o, $n) { unset($o->$n); }
function ut(Base $o, $n) { unset($o->$n); }
function wn($o, $v) { $o->ro = $v; }
function wnl($o, $v) { $o->late = $v; }
foreach ([new Base, new Child] as $o) {
    echo get_class($o), "\n";
    e(fn() => wm($o, 'ro', 9));
    e(fn() => wt($o, 'ro', 9));
    e(fn() => wn($o, 9));
    e(fn() => um($o, 'ro'));
    e(fn() => ut($o, 'ro'));
    e(fn() => wm($o, 'late', 1));
    e(fn() => um($o, 'late'));
    e(fn() => $o->setByName('ro', 3));
    e(fn() => $o->unsetByName('ro'));
    e(fn() => (new Other)->zap($o, 'ro'));
    e(fn() => $o->initLateByName('late', 4));
    echo json_encode([$o->ro, isset($o->late) ? $o->late : null]), "\n";
    e(fn() => $o->initLateByName('late', 5));
    e(fn() => um($o, 'late'));
}
$k = new Child;
e(fn() => $k->childZap('late'));
e(fn() => $k->childInit('late', 8));
e(fn() => $k->childInit('late', 9));
e(fn() => $k->childZap('late'));
e(fn() => $k->childZap('ro'));
echo json_encode([$k->ro, $k->late]), "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Base\nError: Cannot modify readonly property Base::$ro\nError: Cannot modify readonly property Base::$ro\nError: Cannot modify readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nError: Cannot modify protected(set) readonly property Base::$late from global scope\nError: Cannot unset protected(set) readonly property Base::$late from global scope\nError: Cannot modify readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nok\n[7,4]\nError: Cannot modify readonly property Base::$late\nError: Cannot unset readonly property Base::$late\nChild\nError: Cannot modify readonly property Base::$ro\nError: Cannot modify readonly property Base::$ro\nError: Cannot modify readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nError: Cannot modify protected(set) readonly property Base::$late from global scope\nError: Cannot unset protected(set) readonly property Base::$late from global scope\nError: Cannot modify readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nError: Cannot unset readonly property Base::$ro\nok\n[7,4]\nError: Cannot modify readonly property Base::$late\nError: Cannot unset readonly property Base::$late\nok\nok\nError: Cannot modify readonly property Base::$late\nError: Cannot unset readonly property Base::$late\nError: Cannot unset readonly property Base::$ro\n[7,8]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An int, float or bool written into a `?string`, `string`, `?int` or `float` property through a
/// runtime name is coerced like php, and the persisted string the coercion builds is handed to the
/// slot's box instead of being copied and leaked.
#[test]
fn test_runtime_name_scalar_coercion_into_string_properties_is_php_and_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class Holder { public ?string $s = null; public string $t = ""; public ?int $n = null; public float $f = 0.0; }
function w($o, $name, $v) { $o->$name = $v; }
$int = $argc > 5 ? "x" : 3;
$float = $argc > 5 ? "x" : 2.5;
$bool = $argc > 5 ? "x" : true;
$o = new Holder;
w($o, 's', $int);
w($o, 't', $float);
w($o, 's', $bool);
w($o, 'n', "17");
w($o, 'f', $int);
w($o, 's', $int);
echo json_encode($o), "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "{\"s\":\"3\",\"t\":\"2.5\",\"n\":17,\"f\":3}\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new C(...)` over a class mixing every kind of default (ints, floats, bools, static strings,
/// nulls, nullable ints, arrays, constant expressions, class constants), typed-uninitialized
/// properties, promoted readonly and protected constructor parameters, a `parent::__construct`
/// chain, a hidden private slot, clone, a destructor and a constructor that throws. The output is
/// php's and the heap stays clean.
#[test]
fn test_new_from_class_templates_matches_php_for_every_default_kind() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
const LIMIT = 10;
class Base {
    public int $i = 1;
    public float $f = 1.5;
    public string $s = "base";
    public bool $b = true;
    public ?int $n = null;
    public ?int $m = 7;
    public $u = 5;
    public $un;
    public array $arr = [1, 2, 3];
    public array $assoc = ['a' => 1, 'b' => [2, 3]];
    public int $uninit;
    public ?string $nullable;
    public int $limit = LIMIT * 2;
    public const K = 4;
    public int $fromConst = self::K + 1;
    private string $secret = "s3";
    public function __construct(public readonly int $id = 0, protected string $tag = "t") {}
    public function secret() { return $this->secret; }
    public function tag() { return $this->tag; }
}
class Child extends Base {
    public int $extra = 9;
    public array $list = ['x'];
    public function __construct(int $id, public string $name = "child") { parent::__construct($id, "from-child"); }
}
class Grand extends Child {
    private string $secret = "grand";
    public function __construct() { parent::__construct(3, "g"); }
    public function grandSecret() { return $this->secret; }
}
class Plain { public $a; public $b = 2; public int $c = 3; }
class Dtor { public static array $log = []; public array $d = [1]; public function __construct(public string $n) {} public function __destruct() { self::$log[] = "d:" . $this->n; } }
class Bad { public int $x = 1; public function __construct() { throw new RuntimeException("no"); } public function __destruct() { echo "never\n"; } }
function show($o) {
    $a = (array) $o;
    $out = [];
    foreach ($a as $k => $v) { $out[str_replace("\0", "~", $k)] = $v instanceof UnitEnum ? $v->name : $v; }
    echo get_class($o), " ", json_encode($out), "\n";
}
show(new Base);
show(new Base(5));
show(new Child(6, "kid"));
$g = new Grand;
show($g);
echo $g->secret(), " ", $g->grandSecret(), " ", $g->tag(), "\n";
show(new Plain);
$p = new Plain; $p->a = 'x'; $q = new Plain; var_dump($q->a, $q->b);
$c = new Child(1);
$c->arr[] = 4;
$d = new Child(2);
echo json_encode([$c->arr, $d->arr, (new Child(3))->arr]), "\n";
try { $u = new Base; echo $u->uninit; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { $u = new Base; echo $u->nullable; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$cl = clone $c; $cl->arr[] = 5; $cl->list[] = 'y';
echo json_encode([$c->arr, $cl->arr, $c->list, $cl->list, $cl->id]), "\n";
try { new Bad; } catch (RuntimeException $e) { echo "caught ", $e->getMessage(), "\n"; }
$x = new Dtor("one"); $y = new Dtor("two"); unset($x); $y = null;
echo json_encode(Dtor::$log), "\n";
function make(int $n) { $r = []; for ($i = 0; $i < $n; $i++) { $r[] = new Child($i); } return $r; }
$objs = make(3);
echo json_encode(array_map(fn($o) => [$o->id, $o->name, $o->arr, $o->extra], $objs)), "\n";
echo json_encode(array_map(fn($o) => spl_object_id($o) > 0, $objs)), "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Base {\"i\":1,\"f\":1.5,\"s\":\"base\",\"b\":true,\"n\":null,\"m\":7,\"u\":5,\"un\":null,\"arr\":[1,2,3],\"assoc\":{\"a\":1,\"b\":[2,3]},\"limit\":20,\"fromConst\":5,\"~Base~secret\":\"s3\",\"id\":0,\"~*~tag\":\"t\"}\nBase {\"i\":1,\"f\":1.5,\"s\":\"base\",\"b\":true,\"n\":null,\"m\":7,\"u\":5,\"un\":null,\"arr\":[1,2,3],\"assoc\":{\"a\":1,\"b\":[2,3]},\"limit\":20,\"fromConst\":5,\"~Base~secret\":\"s3\",\"id\":5,\"~*~tag\":\"t\"}\nChild {\"i\":1,\"f\":1.5,\"s\":\"base\",\"b\":true,\"n\":null,\"m\":7,\"u\":5,\"un\":null,\"arr\":[1,2,3],\"assoc\":{\"a\":1,\"b\":[2,3]},\"limit\":20,\"fromConst\":5,\"~Base~secret\":\"s3\",\"id\":6,\"~*~tag\":\"from-child\",\"extra\":9,\"list\":[\"x\"],\"name\":\"kid\"}\nGrand {\"i\":1,\"f\":1.5,\"s\":\"base\",\"b\":true,\"n\":null,\"m\":7,\"u\":5,\"un\":null,\"arr\":[1,2,3],\"assoc\":{\"a\":1,\"b\":[2,3]},\"limit\":20,\"fromConst\":5,\"~Base~secret\":\"s3\",\"id\":3,\"~*~tag\":\"from-child\",\"extra\":9,\"list\":[\"x\"],\"name\":\"g\",\"~Grand~secret\":\"grand\"}\ns3 grand from-child\nPlain {\"a\":null,\"b\":2,\"c\":3}\nNULL\nint(2)\n[[1,2,3,4],[1,2,3],[1,2,3]]\nError: Typed property Base::$uninit must not be accessed before initialization\nError: Typed property Base::$nullable must not be accessed before initialization\n[[1,2,3,4],[1,2,3,4,5],[\"x\"],[\"x\",\"y\"],1]\ncaught no\n[\"d:one\",\"d:two\"]\n[[0,\"child\",[1,2,3],9],[1,\"child\",[1,2,3],9],[2,\"child\",[1,2,3],9]]\n[true,true,true]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Enum-case, constant and enum-array defaults go through the class's property-init thunk after the
/// template; the values must be php's. (Enum singletons stay allocated until exit, so no heap check.)
#[test]
fn test_new_from_class_templates_assigns_enum_and_array_defaults_like_php() {
    let out = compile_and_run(
        r####"<?php
enum Suit: string { case Hearts = 'H'; case Spades = 'S'; }
const LIMIT = 3;
class A { public Suit $suit = Suit::Spades; public ?Suit $maybe = null; public array $suits = [Suit::Hearts, Suit::Spades]; public int $n = LIMIT; public array $m = ['k' => Suit::Hearts]; }
class B extends A { public Suit $other = Suit::Hearts; }
$a = new A; $b = new B;
echo $a->suit->name, $b->suit->name, $b->other->name, $a->suits[0]->name, $a->suits[1]->value, count($a->suits), $a->n, $b->m['k']->name, var_export($a->maybe, true), "\n";
$a->suit = Suit::Hearts;
echo $a->suit->name, (new A)->suit->name, "\n";
"####,
    );
    assert_eq!(out, "SpadesSpadesHeartsHeartsS23HeartsNULL\nHeartsSpades\n");
}

/// A class with `properties` extra properties (all default kinds, one array) on top of one array
/// property, instantiated at one site.
fn wide_class_program(properties: usize) -> String {
    let mut source = String::from("<?php\nclass W { public array $base = [1];\n");
    for index in 0..properties {
        let declaration = match index % 7 {
            0 => format!("public int $p{index} = {index};"),
            1 => format!("public string $p{index} = \"s{index}\";"),
            2 => format!("public ?int $p{index} = null;"),
            3 => format!("public float $p{index} = 1.5;"),
            4 => format!("public bool $p{index} = true;"),
            5 => format!("public int $p{index};"),
            _ => format!("public $p{index} = null;"),
        };
        source.push_str(&format!("    {declaration}\n"));
    }
    source.push_str("}\nfunction mk() { return new W(); }\n$w = mk();\necho count($w->base), \"\\n\";\n");
    source
}

/// The instructions of the function `mk` that holds the `new` site: ONE call into the shared
/// allocation routine plus the property-init thunk call, whatever the property count. The inline
/// sequence was a store per slot word and one to three instructions per default, ~345 per site for
/// a seven-property class.
#[test]
fn test_new_site_size_does_not_depend_on_the_property_count() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let narrow = emit_asm_with_env("elephc_new_narrow", &wide_class_program(1), target, &[]);
        let wide = emit_asm_with_env("elephc_new_wide", &wide_class_program(50), target, &[]);
        let before = instructions_in_functions(&narrow, "_fn_mk");
        let after = instructions_in_functions(&wide, "_fn_mk");
        assert!(before > 0, "{target}: no mk function found");
        assert_eq!(before, after, "{target}: the new site grew with the property count ({before} -> {after})");
        let inline = emit_asm_with_env(
            "elephc_new_inline",
            &wide_class_program(50),
            target,
            &[("ELEPHC_NO_OBJECT_TEMPLATE", "1")],
        );
        let inline_size = instructions_in_functions(&inline, "_fn_mk");
        assert!(
            inline_size > after + 100,
            "{target}: the inline allocation should be much larger ({inline_size} vs {after})"
        );
    }
}

/// A class with a property hash (`#[AllowDynamicProperties]`) is built from the same template, with
/// the trailing hash word zeroed in the image and the hash created at the site; its array default
/// still comes from the property-init thunk. Dynamic properties, a subclass and a typed
/// uninitialized property behave like php and the heap stays clean.
#[test]
fn test_new_from_class_templates_keeps_the_dynamic_property_hash() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
#[AllowDynamicProperties]
class Bag { public int $a = 1; public string $s = "s"; public array $list = [1, 2]; public ?int $n = null; public int $u; }
class Sub extends Bag { public float $f = 2.5; }
function build(string $extra) { $b = new Bag; $b->$extra = 42; return $b; }
$x = build("dyn1");
$y = new Sub;
$y->other = "o";
$z = new Bag;
$z->list[] = 3;
echo $x->a, $x->s, count($x->list), var_export($x->n, true), $x->dyn1, "\n";
echo $y->a, $y->s, count($y->list), $y->f, $y->other, "\n";
echo json_encode([$z->list, (new Bag)->list, isset($z->dyn1)]), "\n";
echo count((array) $x), count((array) $y), "\n";
try { echo $x->u; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1s2NULL42\n1s22.5o\n[[1,2,3],[1,2],false]\n56\nError: Typed property Bag::$u must not be accessed before initialization\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `count` sequential `if` statements (a join block after each) and no call or loop.
fn branching_program(count: usize) -> String {
    let mut source = String::from("<?php\npcntl_async_signals(true);\nfunction f(int $a): int { $r = 0;\n");
    for index in 0..count {
        source.push_str(&format!("  if ($a > {index}) {{ $r = $r + 1; }}\n"));
    }
    source.push_str("  return $r; }\necho f(3), \"\\n\";\n");
    source
}

/// Async-signal safe points sit at function entry, loop back edges and after calls, not at every
/// block start: fifty `if` joins add none, and a loop adds one.
#[test]
fn test_async_signal_safe_points_are_not_emitted_at_plain_block_starts() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let points = |asm: &str| {
            asm.lines()
                .filter(|line| {
                    let line = line.trim();
                    (line.starts_with("bl ") || line.starts_with("call "))
                        && (line.contains("__rt_pcntl_safe_point") || line.contains("__rt_pcntl_async_dispatch_preserving"))
                })
                .count()
        };
        let few = emit_asm_with_env("elephc_sp_few", &branching_program(5), target, &[]);
        let many = emit_asm_with_env("elephc_sp_many", &branching_program(50), target, &[]);
        assert!(points(&few) > 0, "{target}: expected at least the entry safe point");
        assert_eq!(points(&few), points(&many), "{target}: safe points grew with the block count ({} -> {})", points(&few), points(&many));
        let looped = emit_asm_with_env(
            "elephc_sp_loop",
            "<?php\npcntl_async_signals(true);\nfunction g(int $a): int { $i = 0; while ($i < $a) { $i = $i + 1; } return $i; }\necho g(3), \"\\n\";\n",
            target,
            &[],
        );
        let straight = emit_asm_with_env(
            "elephc_sp_straight",
            "<?php\npcntl_async_signals(true);\nfunction g(int $a): int { $i = $a + 1; return $i; }\necho g(3), \"\\n\";\n",
            target,
            &[],
        );
        assert!(points(&looped) > points(&straight), "{target}: a loop must keep its back-edge safe point");
    }
}

/// `ReflectionClass::newInstanceArgs()` (named and positional) and `newInstanceWithoutConstructor()`
/// over a namespace of classes with typed and promoted constructors, defaults, a property hash, an
/// abstract class, an enum and a missing class. The Reflection bodies are `new $name(...$args)`
/// over a runtime argument container, which finds the class in the shared name table: the case
/// folding, the leading backslash, named and positional arguments and the errors must be php's.
/// (The Reflection layer leaks its temporary reflectors in both paths, so no heap check here.)
#[test]
fn test_dynamic_new_through_the_class_table_matches_php() {
    let out = compile_and_run_with_heap_debug_and_regex(
        r####"<?php
namespace App;
interface Shape { }
abstract class Base implements Shape {
    public array $tags = ['t'];
    public int $created = 5;
    public function __construct(public string $name = "base") {}
}
class Circle extends Base { public float $r = 1.0; public function __construct(string $name = "circle", float $r = 2.0) { parent::__construct($name); $this->r = $r; } }
class Square extends Base { public ?int $side = null; public function __construct(string $name, int $side = 3) { parent::__construct($name); $this->side = $side; } }
class Plain { public int $a = 1; public $list = [1, 2]; }
class Promo { public function __construct(public readonly int $id, protected string $label = "L", public array $opts = ['o' => 1]) {} public function label() { return $this->label; } }
#[\AllowDynamicProperties] class Dyn { public int $x = 1; public function __construct(public string $s = "s") {} }
class Priv { private function __construct() {} public static function make() { return new static; } }
class Strict { public function __construct(public int $n) {} }
enum Color { case Red; }
class Many1 { public $v = 1; } class Many2 { public $v = 2; } class Many3 { public $v = 3; } class Many4 { public $v = 4; }
function clean(string $m): string { foreach ([", called in ", " in /"] as $n) { $p = strpos($m, $n); if ($p !== false) { $m = substr($m, 0, $p); } } return $m; }
function show($o): string { $out = []; foreach (get_object_vars($o) as $k => $v) { $out[] = $k . "=" . (is_array($v) ? "array(" . count($v) . ")" : (is_null($v) ? "null" : (string) $v)); } return implode(",", $out); }
function build(string $cls, array $args = []) {
    try { $o = (new \ReflectionClass($cls))->newInstanceArgs($args); echo get_class($o), ": ", show($o), "\n"; return $o; }
    catch (\Throwable $e) { echo get_class($e), ": ", clean($e->getMessage()), "\n"; }
}
function bare(string $cls) {
    try { $o = (new \ReflectionClass($cls))->newInstanceWithoutConstructor(); echo get_class($o), ": ", show($o), "\n"; }
    catch (\Throwable $e) { echo get_class($e), ": ", clean($e->getMessage()), "\n"; }
}
build('App\Circle');
build('App\Circle', ['name' => 'big', 'r' => 9.5]);
build('app\circle', ['r' => 4.5]);
build('\App\Square', ['sq', 7]);
build('App\Square', ['name' => 'named']);
build('App\Plain');
build('App\Promo', ['id' => 4]);
build('App\Promo', [9, 'custom', ['z' => 2]]);
build('App\Dyn', ['dd']);
build('App\Many3');
build('App\Base');
build('App\Color');
build('App\Missing');
$p = build('App\Promo', [5]);
echo $p->label(), "\n";
bare('App\Plain'); bare('App\Circle'); bare('App\Promo'); bare('App\Dyn');
$a = build('App\Circle', ['a']); $b = build('App\Circle', ['b']);
echo spl_object_id($a) !== spl_object_id($b) ? "distinct\n" : "same\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "App\\Circle: tags=array(1),created=5,name=circle,r=2\nApp\\Circle: tags=array(1),created=5,name=big,r=9.5\nApp\\Circle: tags=array(1),created=5,name=circle,r=4.5\nApp\\Square: tags=array(1),created=5,name=sq,side=7\nApp\\Square: tags=array(1),created=5,name=named,side=3\nApp\\Plain: a=1,list=array(2)\nApp\\Promo: id=4,opts=array(1)\nApp\\Promo: id=9,opts=array(1)\nApp\\Dyn: x=1,s=dd\nApp\\Many3: v=3\nError: Cannot instantiate abstract class App\\Base\nError: Cannot instantiate enum App\\Color\nReflectionException: Class \"App\\Missing\" does not exist\nApp\\Promo: id=5,opts=array(1)\nL\nApp\\Plain: a=1,list=array(2)\nApp\\Circle: tags=array(1),created=5,r=1\nApp\\Promo: \nApp\\Dyn: x=1\nApp\\Circle: tags=array(1),created=5,name=a,r=2\nApp\\Circle: tags=array(1),created=5,name=b,r=2\ndistinct\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `classes` constructible classes and two Reflection sites that instantiate them dynamically.
fn dynamic_new_program(classes: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..classes {
        source.push_str(&format!(
            "class K{index} {{ public array $d = [{index}]; public function __construct(public string $s = \"k{index}\") {{}} }}\n"
        ));
    }
    source.push_str(
        "function make($c, array $a) { return (new ReflectionClass($c))->newInstanceArgs($a); }\nfunction bare($c) { return (new ReflectionClass($c))->newInstanceWithoutConstructor(); }\n$o = make('K1', ['x']);\n$p = bare('K2');\necho get_class($o), get_class($p), \"\\n\";\n",
    );
    source
}

/// The instructions of every function that holds a dynamic-new ladder or its helper.
fn dynamic_new_instructions(asm: &str) -> usize {
    instructions_in_functions(asm, "_eir_shared_dyn_new_")
        + instructions_in_functions(asm, "_method_ReflectionClass_newinstance")
        + instructions_in_functions(asm, "_fn_make")
        + instructions_in_functions(asm, "_fn_bare")
}

/// The generic dynamic-new ladder holds no per-class code: ten times the classes must not add an
/// instruction to the shared helpers or the Reflection bodies, on both targets; the per-class arms
/// (switch) make the same program grow.
#[test]
fn test_dynamic_new_ladders_do_not_grow_with_the_class_count() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_dn_small", &dynamic_new_program(10), target, &[]);
        let large = emit_asm_with_env("elephc_dn_large", &dynamic_new_program(100), target, &[]);
        let before = dynamic_new_instructions(&small);
        let after = dynamic_new_instructions(&large);
        assert!(before > 0, "{target}: found no dynamic-new code");
        assert_eq!(before, after, "{target}: dynamic-new code grew with the class count ({before} -> {after})");
        let arms = emit_asm_with_env(
            "elephc_dn_arms",
            &dynamic_new_program(100),
            target,
            &[("ELEPHC_NO_DYNAMIC_NEW_TABLE", "1")],
        );
        let arm_size = dynamic_new_instructions(&arms);
        assert!(arm_size > after + 1000, "{target}: the per-class arms should be much larger ({arm_size} vs {after})");
    }
}

/// `methods` methods that each call one open Mixed callback.
fn open_callback_program(methods: usize) -> String {
    let mut source = String::from(
        "<?php\nfunction advance(mixed $value): mixed { return (int)$value + 1; }\nclass Runner {\n    public mixed $callback;\n    public function __construct(mixed $callback) { $this->callback = $callback; }\n",
    );
    for index in 0..methods {
        source.push_str(&format!(
            "    public function m{index}(int $value): int {{ return (int)call_user_func($this->callback, $value); }}\n"
        ));
    }
    source.push_str("}\n$runner = new Runner('advance');\necho $runner->m0(1);\n");
    source
}

/// The open Mixed callable dispatcher is decided module-wide before any body is emitted, and a
/// codegen worker must see that decision: a worker that read "not decided" as "not shared"
/// inlined the whole dispatcher (about 600 instructions) at each of its sites. Serial and parallel
/// emission must produce the same method bodies, every site must call the shared dispatcher, and
/// the per-site cost must not depend on how many sites there are, on both targets.
#[test]
fn test_open_callable_dispatch_stays_shared_in_parallel_codegen_workers() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let mut sizes = Vec::new();
        for jobs in ["1", "4"] {
            let env = [("ELEPHC_CODEGEN_PARALLEL_MIN", "1"), ("ELEPHC_CODEGEN_JOBS", jobs)];
            let large = emit_asm_with_env("elephc_open_cb_large", &open_callback_program(40), target, &env);
            let calls = large
                .lines()
                .filter(|line| line.contains("_eir_shared_mixed_callable_invoke") && !line.ends_with(':'))
                .count();
            assert!(calls >= 40, "{target} jobs={jobs}: only {calls} sites call the shared dispatcher");
            sizes.push(instructions_in_functions(&large, "_method_Runner_"));
        }
        assert!(sizes[0] > 0, "{target}: found no Runner methods");
        assert_eq!(sizes[0], sizes[1], "{target}: parallel workers emitted different method bodies than the serial pass");
    }
}

/// Descriptor invokers over every callback entry point -- positional, named, by-reference, default,
/// variadic, capture-bound, invokable and static callables, plus catchable argument-count, unknown
/// named-parameter, thrown and type errors -- must match php and leave the heap clean. The invoker
/// frame setup and both exits run in shared out-of-line routines, so every error path crosses them.
#[test]
fn test_descriptor_invokers_with_shared_tails_match_php() {
    let out = compile_and_run_with_heap_debug_and_regex(
        r####"<?php
function cl(string $m): string { foreach ([", called in ", " in /"] as $n) { $p = strpos($m, $n); if ($p !== false) { $m = substr($m, 0, $p); } } return $m; }
function add(int $a, int $b = 10, int ...$rest) { $t = 0; foreach ($rest as $v) { $t += $v; } return $a + $b + $t; }
function name(string $s, ?string $t = null) { return $s . ($t ?? "-"); }
function byref(array &$arr, $v) { $arr[] = $v; return count($arr); }
class K { public function m($x, $y = 5) { return $x * $y; } public static function s(string ...$p) { return implode("+", $p); } public function __invoke($z) { return $z + 1; } }
function thrower($x) { if ($x < 0) { return 1; } throw new \RuntimeException("boom$x"); }
$ks = new K();
$fa = add(...);
$fn = name(...);
$fb = byref(...);
$ft = thrower(...);
echo call_user_func($fa, 1), "\n";
echo call_user_func_array($fa, [1, 2, 3, 4]), "\n";
echo call_user_func_array($fa, ['b' => 5, 'a' => 1]), "\n";
echo implode(",", array_map($fn, ['a', 'b'], ['x', 'y'])), "\n";
echo call_user_func_array([$ks, 'm'], [3]), call_user_func([$ks, 'm'], 3, 4), "\n";
echo call_user_func([K::class, 's'], 'a', 'b', 'c'), "\n";
echo call_user_func($ks, 41), "\n";
$arr = [1];
$r = call_user_func_array($fb, [&$arr, 7]);
echo $r, count($arr), "\n";
$f = function (int $q) use ($ks) { return $ks->m($q); };
echo implode(",", array_map($f, [1, 2, 3])), "\n";
usort($arr, fn($a, $b) => $b <=> $a);
echo implode(",", $arr), "\n";
try { call_user_func_array($fa, $argc > 5 ? [1] : []); } catch (\Throwable $e) { echo get_class($e), "\n"; }
try { call_user_func_array($fn, $argc > 5 ? [1] : ['zz' => 1]); } catch (\Throwable $e) { echo get_class($e), ": ", cl($e->getMessage()), "\n"; }
try { call_user_func($ft, 5); } catch (\Throwable $e) { echo get_class($e), ": ", cl($e->getMessage()), "\n"; }
echo "done\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "11\n10\n6\nax,by\n1512\na+b+c\n42\n22\n5,10,15\n7,1\nArgumentCountError\nError: Unknown named parameter $zz\nRuntimeException: boom5\ndone\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A Throwable subclass built through `ReflectionClass::newInstanceArgs()` is a record of the
/// shared class table like any other class; the table tail writes the file and line of the `new`
/// for it. Messages, codes, promoted properties, named arguments and a throw/catch round trip must
/// match php, among plain classes and with a clean heap.
#[test]
fn test_dynamic_new_of_throwable_subclasses_through_the_class_table_matches_php() {
    let out = compile_and_run(
        r####"<?php
namespace App;
class AppException extends \RuntimeException {
    public function __construct(string $message = "app", int $code = 11, public string $extra = "x") { parent::__construct($message, $code); }
}
class NotFound extends AppException {}
class Denied extends \LogicException {}
class Quiet extends \Exception { public function __construct(public int $level = 3) { parent::__construct("quiet" . $level); } }
class Plain { public int $a = 1; }
class P2 { public int $a = 2; } class P3 { public int $a = 3; } class P4 { public int $a = 4; }
function build(string $cls, array $args = []) {
    try {
        $o = (new \ReflectionClass($cls))->newInstanceArgs($args);
        $d = $o instanceof \Throwable ? $o->getMessage() . "/" . $o->getCode() : "obj";
        echo get_class($o), ": ", $d, "\n";
        return $o;
    } catch (\Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
}
build('App\AppException');
build('App\AppException', ['boom', 5]);
build('App\NotFound', ['message' => 'nf', 'extra' => 'e']);
build('App\Denied', ['denied', 9]);
build('app\quiet', [7]);
build('App\Quiet');
build('App\Plain');
build('App\P2'); build('App\P3'); build('App\P4');
$t = build('App\NotFound', ['thrown', 2]);
try { throw $t; } catch (\Throwable $e) { echo get_class($e), " caught ", $e->getMessage(), " ", $e->getCode(), " ", $e->extra, "\n"; }
"####,
    );
    assert_eq!(out, "App\\AppException: app/11\nApp\\AppException: boom/5\nApp\\NotFound: nf/11\nApp\\Denied: denied/9\nApp\\Quiet: quiet7/0\nApp\\Quiet: quiet3/0\nApp\\Plain: obj\nApp\\P2: obj\nApp\\P3: obj\nApp\\P4: obj\nApp\\NotFound: thrown/2\nApp\\NotFound caught thrown 2 x\n");
}

/// `classes` Throwable subclasses with constructors and the two Reflection sites that build them.
fn dynamic_new_throwable_program(classes: usize) -> String {
    let mut source = String::from("<?php\n");
    for index in 0..classes {
        source.push_str(&format!(
            "class E{index} extends \\RuntimeException {{ public function __construct(string $m = \"e{index}\") {{ parent::__construct($m); }} }}\n"
        ));
    }
    source.push_str(
        "function make($c, array $a) { return (new ReflectionClass($c))->newInstanceArgs($a); }\n$o = make('E1', ['x']);\necho get_class($o), $o->getMessage(), \"\\n\";\n",
    );
    source
}

/// Throwable subclasses are records of the same class table: a creation site records the file and
/// line of the `new`, and the table tail writes them for a record flagged as a Throwable. Ten
/// times the exception classes must not add an instruction to the Reflection bodies or the shared
/// helpers on either target, while the per-class arms (switch) make the same program grow.
#[test]
fn test_dynamic_new_throwable_arms_do_not_grow_with_the_class_count() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_dn_thr_small", &dynamic_new_throwable_program(10), target, &[]);
        let large = emit_asm_with_env("elephc_dn_thr_large", &dynamic_new_throwable_program(100), target, &[]);
        let before = dynamic_new_instructions(&small);
        let after = dynamic_new_instructions(&large);
        assert!(before > 0, "{target}: found no dynamic-new code");
        assert_eq!(before, after, "{target}: throwable arms grew with the class count ({before} -> {after})");
        let arms = emit_asm_with_env(
            "elephc_dn_thr_arms",
            &dynamic_new_throwable_program(100),
            target,
            &[("ELEPHC_NO_DYNAMIC_NEW_TABLE", "1")],
        );
        let arm_size = dynamic_new_instructions(&arms);
        assert!(arm_size > after + 1000, "{target}: the per-class arms should be much larger ({arm_size} vs {after})");
    }
}

/// `signatures` functions with pairwise different parameter shapes, collected as first-class
/// callables and invoked through `call_user_func_array` in a loop, so each needs its descriptor
/// invoker.
fn invoker_shapes_program(signatures: usize) -> String {
    const TYPES: [(&str, &str); 5] = [
        ("int", "1"),
        ("string", "'s'"),
        ("float", "1.5"),
        ("bool", "true"),
        ("array", "[1]"),
    ];
    let mut source = String::from("<?php\n");
    let mut callables = Vec::new();
    let mut arguments = Vec::new();
    for index in 0..signatures {
        let count = index % 5 + 1;
        let params = (0..count)
            .map(|position| format!("{} $a{}", TYPES[(index + position) % 5].0, position))
            .collect::<Vec<_>>()
            .join(", ");
        let args = (0..count)
            .map(|position| TYPES[(index + position) % 5].1)
            .collect::<Vec<_>>()
            .join(", ");
        source.push_str(&format!("function sig{index}({params}) {{ return {count}; }}\n"));
        callables.push(format!("sig{index}(...)"));
        arguments.push(format!("[{args}]"));
    }
    source.push_str(&format!(
        "$fs = [{}];\n$as = [{}];\n$r = 0;\nforeach ($fs as $i => $f) {{ $r += (int) call_user_func_array($f, $as[$i]); }}\necho $r, \"\\n\";\n",
        callables.join(", "),
        arguments.join(", ")
    ));
    source
}

/// The instructions of every descriptor-invoker body (`_eir_cbody_*`), stubs excluded by size.
fn invoker_body_instructions(asm: &str) -> usize {
    instructions_in_functions(asm, "_eir_cbody_")
}

/// The invoker frame setup and both exits (owner release, boundary pop, register restore) and the
/// missing-argument constructor run in shared routines, so a body carries only what its signature
/// needs. Twenty distinct signatures must cost much less than they did with the inline copies, and
/// every added signature must stay under a fixed instruction budget, on both targets.
#[test]
fn test_descriptor_invoker_bodies_share_their_frame_setup_and_exits() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let few = emit_asm_with_env("elephc_inv_few", &invoker_shapes_program(5), target, &[]);
        let many = emit_asm_with_env("elephc_inv_many", &invoker_shapes_program(25), target, &[]);
        let many_inline = emit_asm_with_env(
            "elephc_inv_inline",
            &invoker_shapes_program(25),
            target,
            &[("ELEPHC_NO_SHARED_INVOKER_TAILS", "1")],
        );
        let (few_size, many_size, inline_size) = (
            invoker_body_instructions(&few),
            invoker_body_instructions(&many),
            invoker_body_instructions(&many_inline),
        );
        assert!(few_size > 0, "{target}: found no invoker bodies");
        assert!(
            many_size * 10 < inline_size * 8,
            "{target}: shared tails should cut the invoker bodies by 20% ({many_size} vs {inline_size} inline)"
        );
        let per_signature = (many_size - few_size) / 20;
        assert!(
            per_signature < 450,
            "{target}: each added signature costs {per_signature} instructions"
        );
        for routine in ["_eir_invoker_enter:", "_eir_invoker_finish:", "_eir_invoker_escape:", "_eir_invoker_leave:"] {
            assert_eq!(many.matches(routine).count(), 1, "{target}: {routine} must be emitted exactly once");
        }
    }
}

/// A `ReflectionClass` built from a runtime class name is owned by the eval bridge, and its methods
/// run through the bridge. A call that boxed its receiver and arguments for the bridge released
/// them only when the call came back normally: a `ReflectionException` (an unknown method or
/// property) skipped the release and kept the whole member graph alive (18 blocks per throw).
/// Throwing, catching and succeeding calls on the same reflector must match php and end clean.
#[test]
fn test_throwing_calls_on_a_runtime_named_reflector_release_their_boxed_operands() {
    let out = compile_and_run_with_heap_debug(
        r####"<?php
class K1 { public function go() {} public int $p = 1; }
class K2 { public function __construct(public string $name = "k2") {} }
function probe(string $c): string {
    $out = "";
    $r = new ReflectionClass($c);
    try { $r->getMethod('zz'); } catch (\ReflectionException $e) { $out .= "m:" . $e->getMessage() . ";"; }
    try { $r->getProperty('zz'); } catch (\ReflectionException $e) { $out .= "p:" . $e->getMessage() . ";"; }
    try { $r->getMethod('go'); $out .= "found;"; } catch (\ReflectionException $e) { $out .= "unexpected;"; }
    return $out;
}
function build(string $c): string {
    try { $o = (new ReflectionClass($c))->newInstanceArgs([]); return get_class($o) . ";"; }
    catch (\Throwable $e) { return get_class($e) . ":" . $e->getMessage() . ";"; }
}
for ($i = 0; $i < 3; $i++) { echo probe('K1'), "\n"; }
echo build('K2'), "\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "m:Method K1::zz() does not exist;p:Property K1::$zz does not exist;found;\nm:Method K1::zz() does not exist;p:Property K1::$zz does not exist;found;\nm:Method K1::zz() does not exist;p:Property K1::$zz does not exist;found;\nK2;\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An invokable object and a typed by-reference function handed to a `callable` parameter: the
/// invokable object used to travel as the parameter's storage and the first call through it jumped
/// into the object's first word (SIGBUS); both must now go through a descriptor, match php and
/// end with a clean heap.
#[test]
fn test_invokable_objects_and_by_ref_functions_reach_callable_parameters_as_descriptors() {
    let out = compile_and_run_with_heap_debug_and_regex(
        r####"<?php
function add(int $a, int $b = 10, int ...$rest) { $t = 0; foreach ($rest as $v) { $t += $v; } return $a + $b + $t; }
function plain(int $a, int $b = 2) { return $a * 100 + $b; }
function byref(array &$arr, int $v) { $arr[] = $v; return count($arr); }
class K { public function m(int $x, int $y = 5) { return $x * $y; } public static function s(string ...$p) { return count($p); } public function __invoke(int $z) { return $z + 1; } }
function thrower(int $x) { if ($x < 0) { return 1; } throw new \RuntimeException("boom$x"); }
function sh(callable $f, array $a) { return (int) call_user_func_array($f, $a); }
$ks = new K();
$fa = add(...); $fp = plain(...); $fb = byref(...); $ft = thrower(...);
echo (int) call_user_func($fa, 1), "\n";
echo sh($fa, [1, 2, 3, 4]), "\n";
echo sh($fa, ['b' => 5, 'a' => 1]), "\n";
echo sh($fp, ['b' => 7, 'a' => 3]), "\n";
echo sh($fp, [4]), "\n";
echo sh([$ks, 'm'], [3]), sh([$ks, 'm'], [3, 4]), "\n";
echo sh([K::class, 's'], ['a', 'b', 'c']), "\n";
echo sh($ks, [41]), "\n";
$arr = [1];
echo (int) call_user_func_array($fb, [&$arr, 7]), count($arr), "\n";
$f = function (int $q) use ($ks) { return $ks->m($q); };
echo array_sum(array_map($f, [1, 2, 3])), "\n";
usort($arr, fn($a, $b) => $b <=> $a);
echo implode(",", $arr), "\n";
try { sh($fa, $argc > 5 ? [1] : []); } catch (\Throwable $e) { echo get_class($e), "\n"; }
try { sh($fp, $argc > 5 ? [1] : ['zz' => 1]); } catch (\Throwable $e) { echo get_class($e), "\n"; }
try { sh($ft, [5]); } catch (\Throwable $e) { echo get_class($e), "\n"; }
echo "done\n";
"####,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "11\n10\n6\n307\n402\n1512\n3\n42\n22\n30\n7,1\nArgumentCountError\nError\nRuntimeException\ndone\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `exceptions` exception subclasses and three plain classes built through one `new $class($m)`.
fn dynamic_new_throwables_program(exceptions: usize) -> String {
    let mut source = String::from("<?php\nclass Plain { public function __construct(public string $m) {} }\nclass Other { public function __construct(public string $m) {} }\nclass Third { public function __construct(public string $m) {} }\n");
    for index in 0..exceptions {
        source.push_str(&format!("class AppError{index} extends RuntimeException {{}}\n"));
    }
    source.push_str("function make(string $class, string $message): object\n{\n    return new $class($message);\n}\nfunction outer(string $class): object { return make($class, \"boom-$class\"); }\nforeach (['AppError0', 'LogicException', 'Plain', 'Other'] as $class) {\n    $o = outer($class);\n    if ($o instanceof Throwable) {\n        $frames = array_map(fn ($f) => ($f['function'] ?? '?') . '@' . ($f['line'] ?? '-'), $o->getTrace());\n        echo get_class($o), ' ', $o->getMessage(), ' line ', $o->getLine(), ' [', implode(' ', $frames), \"]\\n\";\n    } else {\n        echo get_class($o), ' ', $o->m, \"\\n\";\n    }\n}\n");
    source
}

/// A `new $class(...)` site does not grow with the program's exception classes: their arms live in
/// the shared constructor helper, which stamps the file and line the site publishes. The site kept
/// one arm per Throwable class for those, ~22 000 instructions at each of 48 Symfony console sites.
/// The output pins the line and the trace php records for the `new` statement.
#[test]
fn test_dynamic_new_sites_do_not_grow_with_the_exception_classes() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_dnt_small", &dynamic_new_throwables_program(3), target, &[]);
        let large = emit_asm_with_env("elephc_dnt_large", &dynamic_new_throwables_program(30), target, &[]);
        let before = instructions_in_functions(&small, "_fn_make");
        let after = instructions_in_functions(&large, "_fn_make");
        assert!(before > 0, "{target}: found no make() body");
        assert_eq!(before, after, "{target}: make() grew with the exception classes");
    }
    let out = compile_and_run_with_heap_debug(&dynamic_new_throwables_program(3));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "AppError0 boom-AppError0 line 10 [make@12 outer@14]\nLogicException boom-LogicException line 10 [make@12 outer@14]\nPlain boom-Plain\nOther boom-Other\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A function holding `locals` refcounted locals that an exception unwinds through.
fn unwound_locals_program(locals: usize) -> String {
    let mut source = String::from("<?php\nclass Noisy { public function __construct(public string $name) {} public function __destruct() { echo \"d{$this->name} \"; } }\nfunction hold(int $n): int {\n");
    for index in 0..locals {
        source.push_str(&format!("    $o{index} = new Noisy('{index}');\n    $a{index} = [$n, {index}];\n"));
    }
    source.push_str("    if ($n > 0) { throw new RuntimeException('unwound'); }\n    return 1;\n}\ntry { hold(1); } catch (RuntimeException $e) { echo $e->getMessage(), \"\\n\"; }\n");
    source
}

/// The exceptional cleanup callback of a function releases its locals through the shared
/// exception-preserving slot-table walker: its size does not grow with the number of locals. The
/// callbacks unrolled one release sequence per slot, 7.4 MB of the Symfony console. The output
/// pins php's destruction order and the clean heap.
#[test]
fn test_exception_cleanup_callbacks_do_not_grow_with_the_locals() {
    let callback = |asm: &str| instructions_in_functions(asm, "_fn_hold__cdylib_exception_cleanup");
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_unwind_small", &unwound_locals_program(4), target, &[]);
        let large = emit_asm_with_env("elephc_unwind_large", &unwound_locals_program(16), target, &[]);
        let before = callback(&small);
        let after = callback(&large);
        assert!(before > 0, "{target}: found no cleanup callback");
        assert_eq!(before, after, "{target}: the cleanup callback grew with the locals");
    }
    let out = compile_and_run_with_heap_debug(&unwound_locals_program(4));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "d0 d1 d2 d3 unwound\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A base class reading `$this->{$registry}[$id]` over a subclass with `props` array properties.
fn typed_receiver_runtime_read_program(props: usize) -> String {
    let mut source = String::from(
        "<?php\nclass Base {\n    protected array $services = [];\n    protected array $privates = [];\n    private array $hidden = ['h' => 'hidden-base'];\n    public function get(string|false $registry, string $id): mixed {\n        if (false === $registry) { return null; }\n        return $this->{$registry}[$id] ?? null;\n    }\n    public function set(string $registry, string $id, mixed $value): void { $this->{$registry}[$id] = $value; }\n}\nclass Built extends Base {\n",
    );
    for index in 0..props {
        source.push_str(&format!("    protected array $reg{index} = ['k' => 'v{index}'];\n"));
    }
    source.push_str("    private string $own = 'own';\n}\n$c = new Built();\n$c->set('services', 'a', 'A');\n$c->set('privates', 'b', 'B');\n$out = [$c->get('services', 'a'), $c->get('privates', 'b'), $c->get('services', 'zz') ?? 'null', $c->get(false, 'x') ?? 'false', $c->get('hidden', 'h')];\n");
    for index in (0..props).step_by((props / 4).max(1)) {
        source.push_str(&format!("$out[] = $c->get('reg{index}', 'k');\n"));
    }
    source.push_str("echo implode(',', $out), \"\\n\";\n");
    source
}

/// A runtime-name read on a CLASS-typed receiver (`$this->{$registry}`) goes through the shared
/// runtime-name helper: the method holding it does not grow with the properties of the subclass
/// tree. Inline, Symfony's `Container::getService()` carried the compiled container's whole ladder,
/// 1.86 MB for one method. The output pins the visibility answers php gives from `Base`'s scope.
#[test]
fn test_typed_receiver_runtime_name_reads_do_not_grow_with_the_subclass_properties() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_trr_small", &typed_receiver_runtime_read_program(8), target, &[]);
        let large = emit_asm_with_env("elephc_trr_large", &typed_receiver_runtime_read_program(80), target, &[]);
        let before = instructions_in_functions(&small, "_method_Base_get");
        let after = instructions_in_functions(&large, "_method_Base_get");
        assert!(before > 0, "{target}: found no Base::get body");
        assert_eq!(before, after, "{target}: Base::get grew with the subclass properties");
    }
    let out = compile_and_run_with_heap_debug(&typed_receiver_runtime_read_program(80));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A,B,null,false,hidden-base,v0,v20,v40,v60\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new $class($arg)` sites that pass five different argument types share ONE outlined constructor
/// helper per arity: the arguments reach it boxed and its ladder coerces them per constructor.
/// Keyed by the argument types, each shape carried its own copy of the whole class ladder (29
/// copies, 19 MB of the compiled Symfony console). The output pins the coercions, `"6", 9`
/// included, which the int-typed helper turned into garbage.
#[test]
fn test_dynamic_new_sites_share_one_constructor_helper_whatever_the_argument_types() {
    let source = r#"<?php
class PointA { public function __construct(public int $x, public string $tag = 'a') {} public function show(): string { return "A{$this->x}{$this->tag}"; } }
class PointB { public function __construct(public int $x, public string $tag = 'b') {} public function show(): string { return "B{$this->x}{$this->tag}"; } }
class Labeled { public function __construct(public string $label, public array $items = []) {} public function show(): string { return "L{$this->label}" . count($this->items); } }
function pick(int $i): string { return ['PointA', 'PointB', 'Labeled'][$i % 3]; }
$out = [];
for ($i = 0; $i < 3; $i++) {
    $c = pick($i);
    $out[] = (new $c(7))->show();
    $out[] = (new $c("42"))->show();
    $out[] = (new $c(3.0))->show();
    $out[] = (new $c(true))->show();
}
for ($i = 0; $i < 2; $i++) {
    $c = pick($i);
    $out[] = (new $c(5, 'z'))->show();
    $out[] = (new $c("6", 9))->show();
}
echo implode(',', $out), "\n";
"#;
    for target in ["macos-aarch64", "linux-x86_64"] {
        let asm = emit_asm_with_env("elephc_dn_shapes", source, target, &[]);
        let helpers = asm
            .lines()
            .filter(|line| {
                line.starts_with("_eir_shared_dyn_new_ctor_")
                    && line.ends_with(':')
                    && !line.contains("_epilogue")
                    && !line.contains("_cleanup")
            })
            .count();
        // One helper per arity (one and two arguments), not one per argument-type shape.
        assert_eq!(helpers, 2, "{target}: constructor helpers");
    }
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A7a,A42a,A3a,A1a,B7b,B42b,B3b,B1b,L70,L420,L30,L10,A5z,A69,B5z,B69\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The clone probe: public, private and protected `__clone()`, an enum, a plain class, a typed and
/// an untyped site, a scoped site and a `clone()` with overrides, plus `extra` classes with a
/// `__clone()` and `extra` enums appended after the code (so the lines php reports do not move).
fn clone_program(extra: usize) -> String {
    let mut source = String::from(concat!(
        "<?php\n",
        "enum Suit { case Hearts; }\n",
        "class Point { public function __construct(public int $x = 1) {} public function __clone() { $this->x++; } }\n",
        "class Secret { private function __clone() {} public static function dup(self $s): self { return clone $s; } }\n",
        "class Guarded { protected function __clone() {} public function copy(): static { return clone $this; } }\n",
        "class Plain { public $v = 'p'; }\n",
        "function dup(mixed $o): mixed { return clone $o; }\n",
        "function typed(Point $p): Point { return clone $p; }\n",
        "$objects = $argc > 5 ? [] : [new Point(), new Plain(), Suit::Hearts, new Secret(), new Guarded()];\n",
        "foreach ($objects as $o) {\n",
        "    try { $c = dup($o); echo get_class($c), $c === $o ? ' same' : ' copy', \"\\n\"; }\n",
        "    catch (Error $e) { echo $e->getMessage(), ' @', $e->getLine(), \"\\n\"; }\n",
        "}\n",
        "echo typed(new Point(5))->x, ' ', get_class(Secret::dup(new Secret())), ' ', get_class((new Guarded())->copy()), ' ', (clone(new Point(3), ['x' => 9]))->x, \"\\n\";\n",
    ));
    for index in 0..extra {
        source.push_str(&format!(
            "class C{index} {{ public function __clone() {{}} }}\nenum E{index} {{ case A; }}\n"
        ));
    }
    source
}

/// A `clone` site calls one shared helper instead of expanding, at the site, the enum refusals
/// and the `__clone()` dispatch over every class (about 3 000 instructions per site in the
/// compiled Symfony console): the untyped and typed probing functions must not grow when the
/// program declares more cloneable classes and enums. The helper raises its errors on the line
/// of the calling statement, and private/protected hooks still see the site's scope.
#[test]
fn test_clone_sites_do_not_grow_with_the_class_count() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_clone_small", &clone_program(5), target, &[]);
        let large = emit_asm_with_env("elephc_clone_large", &clone_program(50), target, &[]);
        for function in ["_fn_dup", "_fn_typed"] {
            let before = instructions_in_functions(&small, function);
            let after = instructions_in_functions(&large, function);
            assert!(before > 0, "{target}: found no {function}");
            assert_eq!(before, after, "{target}: {function} grew with the class count ({before} -> {after})");
        }
    }
    let out = compile_and_run_with_heap_debug(&clone_program(2));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "Point copy\nPlain copy\nTrying to clone an uncloneable object of class Suit @7\nCall to private method Secret::__clone() from global scope @7\nCall to protected method Guarded::__clone() from global scope @7\n6 Secret Guarded 9\n"
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `classes` classes, an interface, a trait, an enum and one function that asks the
/// `class_exists()` family about a runtime name.
fn class_exists_program(classes: usize) -> String {
    let mut source = String::from("<?php\ninterface Shape {}\ntrait Named {}\nenum Suit { case Hearts; }\nclass Root {}\n");
    for index in 0..classes {
        source.push_str(&format!("class K{index} extends Root implements Shape {{ use Named; }}\n"));
    }
    source.push_str(concat!(
        "function probe(string $n): string { return var_export(class_exists($n), true) . ' ' . var_export(interface_exists($n), true) . ' ' . var_export(trait_exists($n), true) . ' ' . var_export(enum_exists($n), true); }\n",
        "function parent_of(string $n): string { return get_parent_class($n); }\n",
        "function parent_mixed(mixed $n): string { try { return get_parent_class($n); } catch (TypeError $e) { return 'TypeError'; } }\n",
        r"$names = $argc > 5 ? [] : ['K1', '\\k1', 'K1X', '\\\\K1', 'Shape', 'named', 'SUIT', '', 'Exception', '\\ArrayObject', 'Closure', 'nope'];",
        "\nforeach ($names as $n) { echo $n, ': ', probe($n), \"\\n\"; }\n",
        r"echo parent_of('k2'), ' ', parent_of('\\K1'), ' ', parent_mixed('K0'), ' ', parent_mixed('InvalidArgumentException'), ' ', parent_mixed('Nope'), ' ', parent_mixed(1), ' ', parent_mixed(new K1()), PHP_EOL;",
        "\n",
    ));
    source
}

/// A dynamic `class_exists($name)` looks the name up in one shared packed list through
/// `__rt_name_list_find_ci`: the probing function must not grow when the program declares ten
/// times the classes (it used to unroll two case-insensitive compares per class), and the answers
/// match php, including a leading `\`, case folding and a doubled separator.
#[test]
fn test_dynamic_class_exists_sites_do_not_grow_with_the_class_count() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_ce_small", &class_exists_program(10), target, &[]);
        let large = emit_asm_with_env("elephc_ce_large", &class_exists_program(100), target, &[]);
        for function in ["_fn_probe", "_fn_parent_u_of", "_fn_parent_u_mixed"] {
            let before = instructions_in_functions(&small, function);
            let after = instructions_in_functions(&large, function);
            assert!(before > 0, "{target}: found no {function}");
            assert_eq!(before, after, "{target}: {function} grew with the class count ({before} -> {after})");
        }
    }
    let out = compile_and_run_with_heap_debug(&class_exists_program(3));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "K1: true false false false\n",
            r"\k1: true false false false",
            "\nK1X: false false false false\n",
            r"\\K1: false false false false",
            "\nShape: false true false false\nnamed: false false true false\nSUIT: true false false true\n: false false false false\nException: true false false false\n",
            r"\ArrayObject: true false false false",
            "\nClosure: true false false false\nnope: false false false false\n",
            "Root Root Root LogicException TypeError TypeError Root\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `new $class($arg)` sites whose argument is already boxed but typed differently -- `mixed`, a
/// nullable object, a union -- share the one constructor helper of their arity too. The PHP type
/// of a boxed operand changes nothing the helper does with the box, and keying on it left three
/// identical 0.8 MB four-argument helpers in the compiled Symfony app.
#[test]
fn test_dynamic_new_sites_with_differently_typed_boxed_arguments_share_one_helper() {
    let source = r#"<?php
class Box { public function __construct(public mixed $v = null) {} public function show(): string { return get_debug_type($this->v); } }
class Tag { public function __construct(public mixed $v = null) {} public function show(): string { return 'T' . get_debug_type($this->v); } }
class Pin { public function __construct(public mixed $v = null) {} public function show(): string { return 'P' . get_debug_type($this->v); } }
function pick(int $i): string { return ['Box', 'Tag', 'Pin'][$i % 3]; }
function mixedArg(mixed $m): string { $c = pick(0); return (new $c($m))->show(); }
function nullableArg(?Box $b): string { $c = pick(1); return (new $c($b))->show(); }
function unionArg(int|string $u): string { $c = pick(2); return (new $c($u))->show(); }
echo mixedArg(1.5), ',', nullableArg(null), ',', nullableArg(new Box()), ',', unionArg('x'), ',', unionArg(3), "\n";
"#;
    for target in ["macos-aarch64", "linux-x86_64"] {
        let asm = emit_asm_with_env("elephc_dn_boxed", source, target, &[]);
        let helpers = asm
            .lines()
            .filter(|line| {
                line.starts_with("_eir_shared_dyn_new_ctor_")
                    && line.ends_with(':')
                    && !line.contains("_epilogue")
                    && !line.contains("_cleanup")
            })
            .count();
        assert_eq!(helpers, 1, "{target}: constructor helpers");
    }
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "float,Tnull,TBox,Pstring,Pint\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A program of `classes` candidate classes with `props` literal-default properties each, all
/// built through one `new $class($arg)` site.
fn dynamic_new_property_program(classes: usize, props: usize) -> String {
    let mut source = String::from("<?php\n");
    for class in 0..classes {
        source.push_str(&format!("class C{class} {{\n"));
        for prop in 0..props {
            source.push_str(&format!("    public int $p{prop} = {prop};\n    public string $s{prop} = 's{prop}';\n"));
        }
        source.push_str(&format!(
            "    public function __construct(public mixed $v = null) {{}}\n    public function show(): string {{ return 'C{class}:' . $this->v . ':' . ($this->p0 ?? '-') . ':' . ($this->s0 ?? '-'); }}\n}}\n"
        ));
    }
    source.push_str(&format!(
        "$names = [{}];\n$out = [];\nforeach ($names as $i => $n) {{ $o = new $n($i); $out[] = $o->show(); }}\necho implode(',', $out), \"\\n\";\n",
        (0..classes).map(|class| format!("'C{class}'")).collect::<Vec<_>>().join(", ")
    ));
    source
}

/// The arms of a `new $class(...)` ladder build each candidate from the class's static template
/// (`__rt_object_new_template`), so the helper does not grow with the candidates' properties. The
/// arms used the inline allocation, one store per slot and default: about 40 instructions per
/// class in each dynamic-new helper of a Symfony build.
#[test]
fn test_dynamic_new_arms_do_not_grow_with_the_candidate_properties() {
    for target in ["macos-aarch64", "linux-x86_64"] {
        let small = emit_asm_with_env("elephc_dnp_small", &dynamic_new_property_program(4, 1), target, &[]);
        let large = emit_asm_with_env("elephc_dnp_large", &dynamic_new_property_program(4, 12), target, &[]);
        let before = instructions_in_functions(&small, "_eir_shared_dyn_new_ctor_");
        let after = instructions_in_functions(&large, "_eir_shared_dyn_new_ctor_");
        assert!(before > 0, "{target}: found no dynamic-new helper");
        assert_eq!(before, after, "{target}: the dynamic-new helper grew with the candidate properties");
    }
    let out = compile_and_run_with_heap_debug(&dynamic_new_property_program(4, 12));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "C0:0:0:s0,C1:1:0:s0,C2:2:0:s0,C3:3:0:s0\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
