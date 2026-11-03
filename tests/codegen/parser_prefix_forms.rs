//! Purpose:
//! End-to-end coverage for prefix and member-name forms that real-world vendor
//! code uses: reserved words as method names after a dynamic `::` receiver, and `new (expr)`
//! class-name references.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.

use crate::support::*;

/// Verifies reserved words are method names after a dynamic `::` receiver
/// (`$factory::new()`, `static::$factory::new()`, `$cls::match()`) and after `->`.
#[test]
fn test_keyword_member() {
    let out = compile_and_run(
        r#"<?php
class Factory
{
    const DEFAULT = 'def';
    const LIST = 'lst';
    public static function new(): string { return 'new!'; }
    public static function match(string $s): bool { return $s === 'x'; }
    public function list(): string { return 'list!'; }
    public function print(): string { return 'print!'; }
    public static function make(): string { $f = static::class; return $f::new(); }
}
class Holder
{
    public static $factory = Factory::class;
    public static function build(): string { return static::$factory::new(); }
}
$factory = Factory::class;
echo $factory::new(), ' ', Holder::build(), ' ', Factory::make(), "\n";
var_dump($factory::match('x'), $factory::match('y'));
$o = new Factory();
echo $o->list(), ' ', $o->print(), "\n";
echo Factory::DEFAULT, ' ', Factory::LIST, "\n";
"#,
    );
    assert_eq!(
        out,
        r#"new! new! new!
bool(true)
bool(false)
list! print!
def lst
"#
    );
}

/// Verifies `new (expr)` builds the class the expression names at runtime, and an unknown name
/// throws a catchable `Error`.
#[test]
fn test_new_expr() {
    let out = compile_and_run(
        r#"<?php
class Pivot { public function __construct(public string $tag = 'p') {} }
class Other extends Pivot {}
class Model
{
    public function getPivotClass(): string { return Pivot::class; }
    public function pivot() { return (new ($this->getPivotClass())('x')); }
}
$m = new Model();
echo get_class($m->pivot()), ' ', $m->pivot()->tag, ' ', (new ($m->getPivotClass()))->tag, "\n";
$c = true;
echo get_class(new ($c ? Other::class : Pivot::class)('z')), "\n";
echo (new ('Oth' . 'er'))->tag, "\n";
try {
    new ('No' . 'Such');
} catch (Error $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        r#"Pivot x p
Other
p
Error: Class "NoSuch" not found
"#
    );
}
