//! Purpose:
//! End-to-end regressions for array-element null-coalesce assignment.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

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
