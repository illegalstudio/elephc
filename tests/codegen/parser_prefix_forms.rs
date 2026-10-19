//! Purpose:
//! End-to-end coverage for prefix and member-name forms that real-world vendor
//! code uses: unary `+`, reserved words as method/constant names after a dynamic `::` or
//! `->`, and `new (expr)` / `new static::$map[...]` class-name references.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.

use crate::support::*;

/// Verifies unary `+` lowers like php-src's `expr * 1`: numeric strings and bools convert,
/// and precedence matches unary minus (`+$n ** 2` is `+($n ** 2)`).
#[test]
fn test_unary_plus_semantics() {
    let out = compile_and_run(
        r#"<?php
function ev($v) { return $v; }
function plus($v) { return +ev($v); }
var_dump(plus(5), plus(-2.5), plus("12"), plus("1.5e1"), plus(true), plus(null));
$n = 3;
var_dump(+$n, -+$n, +-$n, 2 * +$n, +$n ** 2);
var_dump(random_int(+$n, +$n));
const P = +4;
var_dump(P);
"#,
    );
    assert_eq!(
        out,
        r#"int(5)
float(-2.5)
int(12)
float(15)
int(1)
int(0)
int(3)
int(-3)
int(-3)
int(6)
int(9)
int(3)
int(4)
"#
    );
}

/// Verifies reserved words are method and constant names after a dynamic `::` receiver
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
echo $o->list(), ' ', $o->print(), ' ', $o::new(), "\n";
echo $factory::DEFAULT, ' ', $o::LIST, ' ', Factory::DEFAULT, "\n";
"#,
    );
    assert_eq!(
        out,
        r#"new! new! new!
bool(true)
bool(false)
list! print! new!
def lst def
"#
    );
}

/// Verifies `new (expr)` and `new static::$classes[static::class]($m)` build the class the
/// expression names at runtime, and an unknown name throws `Error`.
#[test]
fn test_new_expr() {
    let out = compile_and_run(
        r#"<?php
class Pivot { public function __construct(public string $tag = 'p') {} }
class Other extends Pivot {}
class Collection { public function __construct(public array $items = []) {} }
class Model
{
    protected static array $classes = [];
    public string $class = Other::class;
    public function viaProp() { return new $this->class('k'); }
    public function getPivotClass(): string { return Pivot::class; }
    public function pivot() { return (new ($this->getPivotClass())('x')); }
    public static function boot(): void { static::$classes[static::class] = Collection::class; }
    public function collect(array $m) { return new static::$classes[static::class]($m); }
}
$m = new Model();
echo get_class($m->pivot()), ' ', $m->pivot()->tag, ' ', (new ($m->getPivotClass()))->tag, "\n";
echo $m->viaProp()->tag, "\n";
Model::boot();
echo count($m->collect([1, 2, 3])->items), "\n";
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
k
3
Other
p
Error: Class "NoSuch" not found
"#
    );
}
