//! Purpose:
//! End-to-end regressions for array-element null-coalesce assignment.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;
use crate::support::compile_and_run_with_heap_debug;

/// Array defaults established with `??=` survive a parent constructor call and
/// assignment into a declared typed property.
#[test]
fn test_array_element_coalesce_defaults_survive_parent_constructor_storage() {
    let out = compile_and_run(
        r#"<?php
class CoalesceDefaultsBase {
    protected array $options;

    public function __construct(array $options) {
        $options['debug_var_name'] ??= 'APP_DEBUG';
        $this->options = $options;
    }

    public function debugVarName(): string {
        return $this->options['debug_var_name'];
    }
}

class CoalesceDefaultsChild extends CoalesceDefaultsBase {
    public function __construct(array $options) {
        $options['env_var_name'] ??= 'APP_ENV';
        parent::__construct($options);
    }

    public function envVarName(): string {
        return $this->options['env_var_name'];
    }
}

$runtime = new CoalesceDefaultsChild([]);
echo $runtime->envVarName(), ':', $runtime->debugVarName();
"#,
    );
    assert_eq!(out, "APP_ENV:APP_DEBUG");
}

/// `$object[$k] ?? $d` over an `ArrayAccess` receiver asks `offsetExists()` first and reads
/// `offsetGet()` only when it answered true (Laravel's `$this->app['session'] ?? null`), and a
/// closure returning `$x ?? null` keeps the value it read instead of being typed `null`.
/// Expected output verified against php 8.5.
#[test]
fn test_array_access_coalesce_asks_offset_exists_and_closure_coalesce_keeps_value() {
    let out = compile_and_run(
        r#"<?php
class Cont implements ArrayAccess {
  function offsetExists($o): bool { echo "exists($o);"; return $o === "session"; }
  function offsetGet($o): mixed { echo "get($o);"; return "S"; }
  function offsetSet($o, $v): void {}
  function offsetUnset($o): void {}
}
class Prov {
  public $app;
  function __construct($a) { $this->app = $a; }
  function direct() { return $this->app['session'] ?? 'none'; }
  function missing() { return $this->app['nope'] ?? 'none'; }
  function clo() { return function () { return $this->app['session'] ?? null; }; }
}
$p = new Prov(new Cont);
echo $p->direct(), "\n";
echo $p->missing(), "\n";
$f = $p->clo();
var_dump($f());
$c = new Cont;
echo $c['session'] ?? 'd', "\n";
$g = function ($a) { return $a['s'] ?? null; };
var_dump($g(['s' => 'x']), $g([]));
$h = fn($a) => $a ?? null;
var_dump($h('y'));
"#,
    );
    assert_eq!(
        out,
        r#"exists(session);get(session);S
exists(nope);none
exists(session);get(session);string(1) "S"
exists(session);get(session);S
string(1) "x"
NULL
string(1) "y"
"#
    );
}

/// A nested `??=` borrows only the value it writes: the parent `offsetGet` hands back is a
/// temporary the write owns. It was classified under the borrowed-operand scope and leaked.
#[test]
fn test_nested_coalesce_assignment_releases_its_array_access_parent() {
    let source = r#"<?php
class A implements ArrayAccess {
  public array $d = [];
  public function offsetExists(mixed $o): bool { return isset($this->d[$o]); }
  public function offsetGet(mixed $o): mixed { return $this->d[$o] ?? null; }
  public function offsetSet(mixed $o, mixed $v): void { $this->d[$o] = $v; }
  public function offsetUnset(mixed $o): void { unset($this->d[$o]); }
}
function pick(int $i): mixed { return $i ? new A : []; }
function run(mixed $x, string $k): string {
  $x["a"] = new A;
  return $x["a"][$k] ??= "v" . $k;
}
$a = pick(1 + (int) ($argc > 5));
echo run($a, "k"), "|", run($a, "j");
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "vk|vj", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Selecting the parent of a nested `??=` calls a method on the boxed receiver. In a program with
/// SimpleXML and a `__call` class (the Symfony Demo) that call builds an argument container, which
/// had no owner under the borrowed-operand scope: lowering panicked. Assembly only, no libxml2.
#[test]
fn test_nested_coalesce_assignment_lowers_with_simplexml_and_a_magic_call_class() {
    let dir = make_cli_test_dir("nested_coalesce_magic_parent");
    compile_source_to_asm_with_options(
        r#"<?php
class M { public function __call(string $n, array $a): mixed { return $n . count($a); } }
function pick(int $i): mixed {
  return match ($i) { 0 => new M, 1 => new SimpleXMLElement("<r/>"), default => ["a" => []] };
}
class H { public mixed $p; }
function boxed(mixed $x, string $k): string { if ($x["a"][$k] ??= "v" . $k) { return "set"; } return "no"; }
function deep(mixed &$x, string $k): string { return $x["a"]["b"][$k] ??= "v" . $k; }
function property(H $h, string $k): string { return $h->p["a"][$k] ??= "v" . $k; }
function erased(mixed $o, string $k): string { return $o->p[$k] ??= "v" . $k; }
$x = pick($argc + 2);
$h = new H;
$h->p = $x;
echo boxed($x, "k"), deep($x, "k"), property($h, "k"), erased($h, "j");
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The compiler's `__elephc_*` helpers are native: a program's `__call` classes must not become
/// candidates for them, so adding such classes leaves an erased write's assembly unchanged in size.
#[test]
fn test_internal_helper_calls_do_not_select_magic_call_classes() {
    let asm_len = |magic_classes: usize| {
        let classes: String = (0..magic_classes)
            .map(|i| format!("class M{i} {{ public function __call(string $n, array $a): mixed {{ return $n; }} }}\n"))
            .collect();
        let source = format!(
            "<?php\n{classes}function erased(mixed $o, string $k): string {{ return $o->p[$k] ??= \"v\" . $k; }}\n\
             class H {{ public mixed $p = []; }}\n$h = $argc > 9 ? new SimpleXMLElement(\"<r/>\") : new H;\necho erased($h, \"k\");\n"
        );
        let dir = make_cli_test_dir("internal_helper_magic_classes");
        let (asm, _runtime, _libs) = compile_source_to_asm_with_options(&source, &dir, 8_388_608, false, false);
        let _ = std::fs::remove_dir_all(&dir);
        let start = asm.find("fn_erased:").expect("the erased() label");
        let body = &asm[start..];
        body.find("fn_erased_epilogue:").expect("the erased() epilogue")
    };
    let (one, many) = (asm_len(1), asm_len(12));
    assert!(many < one + one / 4, "erased() grows with __call classes: {one} -> {many} bytes");
}

/// `$this->prop[$k] ?? $d` over a declared string or array property reads the typed value (or the
/// default while the property is still uninitialized) without boxing leaks, in a loop, under
/// `--heap-debug`. Expected output verified against php 8.5 (`php -n`).
#[test]
fn test_coalesce_over_declared_string_and_array_properties_is_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class OffsetCoalesceHolder {
    public string $bytes;
    public array $items;
    public array $rows = [];
    public function __construct(?string $bytes, ?array $items) {
        if ($bytes !== null) { $this->bytes = $bytes; }
        if ($items !== null) { $this->items = $items; }
        $this->rows = ['a' => [1, 2], 'b' => []];
    }
    public function char(int $i): string { return $this->bytes[$i] ?? '-'; }
    public function item(string $k): string { return $this->items[$k] ?? 'none'; }
    public function row(string $k): int { return $this->rows[$k][1] ?? -1; }
}
$set = new OffsetCoalesceHolder('xyz', ['k' => 'v']);
$unset = new OffsetCoalesceHolder(null, null);
$out = '';
for ($round = 0; $round < 50; $round++) {
    $out = $set->char(1) . $set->char(9) . $set->item('k') . $set->item('z')
        . $unset->char(0) . $unset->item('k') . $set->row('a') . $set->row('b') . $set->row('c');
}
echo $out, "\n";
"#,
    );
    assert_eq!(out.stdout, "y-vnone-none2-1-1\n", "stderr: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "expected clean heap, got: {}", out.stderr);
}
