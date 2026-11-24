//! Purpose:
//! Pins tokenizer losslessness, the PHP symbol catalog and differential Zend parity.
//!
//! Called from:
//! - The tokenizer crate's CI test job, with PHP 8.5 as the reference interpreter.
//!
//! Key details:
//! - Differential results compare symbolic names, raw bytes, line numbers and positions.
//! - TOKEN_PARSE fixtures include PHP syntax beyond elephc's compiled subset.

use elephc_builtin_contract::token_const_id as id;
use elephc_chopper::scanner::newline_count;
use elephc_chopper::syntax::php_tokenizer::*;
use std::io::Write;
use std::process::{Command, Stdio};
use serde_json::{json, Value};

/// Returns the wall-clock budget for the linearity smoke tests.
///
/// These tests detect accidental quadratic scans, not micro-regressions. The budget is
/// deliberately generous and configurable so a loaded CI runner cannot turn them into flakes;
/// reproducible timings live in `scripts/bench/token_view.py`.
fn linearity_budget() -> std::time::Duration {
    let seconds = std::env::var("ELEPHC_LINEARITY_BUDGET_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30);
    std::time::Duration::from_secs(seconds)
}

/// Every tokenizer constant exists and the historical alias resolves canonically.
#[test]
fn tokenizer_catalog_has_all_154_php85_constants() {
    use elephc_builtin_contract::{constants, PhpModule};
    assert_eq!(constants().iter().filter(|c| c.module == PhpModule::Tokenizer).count(), 154);
    assert_eq!(id("T_DOUBLE_COLON"), id("T_PAAMAYIM_NEKUDOTAYIM"));
    assert_eq!(token_name(id("T_DOUBLE_COLON")), Some("T_DOUBLE_COLON"));
    assert_eq!(token_name(0), None);
    assert_eq!(token_name(i64::MAX), None);
}

/// Arbitrary source bytes survive tokenization with no gaps, overlaps or UTF-8 conversion.
#[test]
fn tokenizer_preserves_original_bytes_and_coordinates() {
    for source in fixtures() {
        let tokens = tokenize(&source, 0).unwrap();
        let mut pos = 0;
        let mut line = 1;
        for token in tokens {
            assert_eq!(token.pos, pos, "{}", hex(&source));
            assert_eq!(token.line, line, "{}", hex(&source));
            line += newline_count(&source[token.pos..token.pos + token.len]);
            pos += token.len;
        }
        assert_eq!(pos, source.len(), "{}", hex(&source));
    }
}

/// Invalid UTF-8 bytes inside a name stay an identifier instead of becoming a keyword.
#[test]
fn tokenizer_invalid_byte_name_stays_identifier() {
    let source = b"<?php function \xffor() {}";
    let parsed = tokenize(source, TOKEN_PARSE).expect("byte identifier is valid PHP");
    assert!(
        parsed
            .iter()
            .any(|token| token.id == id("T_STRING")
                && &source[token.pos..token.pos + token.len] == b"\xffor"),
        "the byte name must remain T_STRING"
    );
}

/// TOKEN_PARSE accepts empty dimensions and semi-reserved `goto` names in PHP-valid positions.
#[test]
fn tokenizer_php85_accepts_empty_dimensions_and_goto_names() {
    for source in [
        "<?php echo $a[];",
        "<?php $a[] ??= 1;",
        "<?php foo()[] = 1;",
        "<?php $a[]--;",
        "<?php $a[]++;",
        "<?php $a[]->b[];",
        "<?php X::goto;",
        "<?php foo(goto: 1);",
        "<?php class E { const goto = 1; }",
        "<?php $o->goto;",
        "<?php abstract class C { abstract public $a { get; } }",
        "<?php abstract class C { abstract public $x { foo => 1; } }",
        "<?php abstract class C { abstract public $x; }",
        "<?php class C { abstract public $x; }",
        "<?php abstract class C { abstract public $x = 1; }",
        "<?php if (1) lab: echo 1;",
        "<?php if ($a) goto done; else echo 1; done:",
        "<?php while (1) { goto done; } done:",
        "<?php if ($a): echo 1; ?>text<?php else: echo 2; endif;",
        "<?php if ($a): while ($b): echo 1; ?>x<?php endwhile; else: echo 2; endif;",
        "<?php if ($a): echo C::ENDIF; ?>text<?php else: echo 2; endif;",
        "<?php if ($a): echo C::IF; ?>text<?php else: echo 2; endif;",
        "<?php if ($a): while ($b) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): for (;;) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): foreach ($b as $c) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): declare(ticks=1) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): if ($b) echo 1; ?>x<?php else: echo 2; endif;",
        "<?php if ($a): if ($b) { echo 1; } ?>x<?php else: echo 2; endif;",
        r"<?php $a = &\foo();",
        r"<?php $a = &Ns\foo();",
        r"<?php $a = &namespace\foo();",
        "<?php { echo 1; }",
        "<?php if (1) { { echo 1; } }",
        "<?php foo()[0]++;",
        "<?php ++foo();",
        "<?php --foo();",
        "<?php $x = foo()++;",
        "<?php ++$a[$i = 0];",
        "<?php $a[$i = 0]++;",
        "<?php ++$a ** 2;",
        "<?php ++\"abc\"[0];",
        "<?php \"abc\"[0]++;",
        "<?php (new Foo)[0]++;",
        "<?php (clone $o)->p++;",
        "<?php (1 + 2)[0]++;",
        "<?php (match(1){1=>1})[0]++;",
        "<?php ${1 ? 2 : 3};",
        "<?php ${fn() => 1};",
        "<?php ${match(1) { 1 => 2 }};",
        "<?php ${new Foo()};",
        "<?php ${$a[0]};",
        "<?php ${$$a};",
        "<?php ${${\"a\"}};",
        "<?php ${[1,2]};",
        "<?php $s = \"${class}\";",
        "<?php $s = \"${class[0]}\";",
        "<?php $s = \"${class[1+2]}\";",
        "<?php $s = \"${foo + 1}\";",
        "<?php echo C::{'FOO'};",
        "<?php echo C::{strtoupper('foo')};",
        "<?php echo C::{'FO' . 'O'};",
        "<?php echo $cls::{expr};",
        "<?php echo C::{foo()}::BAR;",
        "<?php (foo())::$bar = 1;",
        "<?php ++(foo())::$bar;",
        "<?php $cls::$prop++;",
        "<?php $cls::$prop = 1;",
        "<?php $cls::$method();",
        "<?php $o->$m(...['a' => 1, 0 => 2]);",
        "<?php $o->$m(...['a' => 1, true => 2]);",
        "<?php f(...['a' => 1, 0 => 2]);",
        "<?php $cls::VALUE;",
        "<?php $o->__halt_compiler; echo 1;",
        "<?php $o?->__halt_compiler;",
        "<?php namespace\\__halt_compiler;",
        "<?php list() = [];",
        "<?php [] = [];",
        "<?php function f(A|(B&C) $x) {}",
        "<?php function f((A&B)|(C&D) $x) {}",
        "<?php function f(A|(B&C)|D $x) {}",
        "<?php [$p, &$q] = $arr;",
        "<?php foreach ([1] as [$p, &$q]) {}",
        "<?php static fn($x) => $x;",
        "<?php static function () {};",
        "<?php #[Attr(1)] const X = 1;",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_ok(), "{source}");
    }
    let source = b"<?php X::goto; foo(goto: 1); class E { const goto = 1; }";
    let tokens = tokenize(source, TOKEN_PARSE).expect("semi-reserved goto names are valid PHP");
    for token in tokens.iter().filter(|token| &source[token.pos..token.pos + token.len] == b"goto") {
        assert_eq!(token.id, id("T_STRING"));
    }
}

/// TOKEN_PARSE accepts PHP 8.5 grammar that elephc's compiler subset rejects or lowers later.
#[test]
fn tokenizer_php85_accepts_grammar_beyond_compiler_subset() {
    for source in [
        "<?php $x ? $a = 1 : 0;",
        "<?php new $this->_class();",
        "<?php new $a->b();",
        "<?php new $a['x']();",
        "<?php new A::$a();",
        "<?php new static::$a();",
        "<?php new $a::$b();",
        "<?php for (include 'a.php'; false; ) {}",
        "<?php for ($i = 0; include 'a.php'; $i++) {}",
        "<?php try {} finally {}",
        "<?php echo 'strlen'('hello'); $f = 'trim';",
        r#"<?php [0 => 'strlen']('hello'); ['k' => 'trim', 0 => 'strlen']('hello');"#,
        r#"<?php $s = `{$$name}`; $t = "{$$name}";"#,
        r#"<?php $s = `{$a[$$b]}`; $t = `${$name}`; $u = `${1+1}`;"#,
        r#"<?php $s = `${"x"}`; $t = `{${$name}}`; $u = `{$a[${"x"}]}`;"#,
        r#"<?php $s = `$$name`; $t = `$a->b`; $u = `$a[0]`; $v = `{$a?->b}`;"#,
        r#"<?php [0 => "strlen"]("x"); [[0 => "strlen"][0]]("x"); (([0 => "strlen"]))("x"); ([0 => "strlen"])("x");"#,
        r#"<?php ("strlen")("x"); "strlen"("x"); [0 => "strlen"](...);"#,
        r#"<?php f(["key" => $value] = $row); ["a" => $x, 0 => $y] = $row;"#,
        r#"<?php interface I { function f(); } function readonly() {}"#,
        r#"<?php foreach ($arr as $k => &$v) {} list("k" => &$v) = $arr;"#,
        r#"<?php do ; while (true); do 5; while ($a);"#,
        r#"<?php function f() { yield &$x; } abstract class C { abstract function m(public int $x); }"#,
        r#"<?php 1 <=> 2 < 3;"#,
        r#"<?php PHP_INT_MAX[0]; INF[0]; M_PI[0]; -PHP_INT_MAX[0];"#,
        r#"<?php PHP_INT_MAX(); INF(); NAN(); true(); false(); null(); PHP_OS(); M_PI(); STDIN(); \PHP_INT_MAX(); (1)(); (true)();"#,
        r#"<?php 1.e3; 1.5e3; .5e3;"#,
        r#"<?php $o?->make()(); $o?->$m()();"#,
        r#"<?php ++$a[]; --$a[]; $a[]++; $a[] = 1;"#,
        r#"<?php function &f($a = $b) {} function &readonly($a = $b) {} class C { function &readonly($a = $b) {} } class D { function match() {} function &readonly() {} }"#,
        r#"<?php if ($a) done: else other:"#,
        "<?php if (1) label:; while (true) other:; for (;;) third:; foreach ([] as $x) fourth:; declare(strict_types=1) fifth:;",
        "<?php class C { public static function __construct(public int $x) {} }",
        "<?php function f($this) {} class C { function m($this) {} }",
        "<?php function f(int $this) {} function g(int &...$this) {} class C { function m(int $this) {} function n(int &...$this) {} }",
        "<?php function($this) {}; static fn($this) => 1; fn(int &...$this) => 1;",
        "<?php ($o->p++)[0]; ($o->p++)->q; $o->p++ + 1;",
        r#"<?php function f(\empty $x) {} function g(Foo\empty $x) {}"#,
        r#"<?php $s = `{$$a}`; $t = `{$$$a}`; $u = `{$$a[0]}`; $v = `{$a[$$b]}`; $w = `{$a[${"k"}]}`;"#,
        r#"<?php $s = `{${$a}}`; $t = `{${$a[0]}}`; $u = `{${$a}["x"]}`; $v = `{$a->{$$b}}`; $w = `{$a?->{$$b}}`;"#,
        r#"<?php $s = `${$a}`; $t = `${$$a}`; $u = `${$a[0]}`; $v = `${$a->b}`; $w = `${"x"}`; $x = `${1+$a}`;"#,
        r#"<?php $s = `\${$a}`; $t = `\{$a}`;"#,
        "<?php new (foo())();",
        "<?php new Runner()(); new $class()(); new static()(); new $a->b()(); new A::$b()();",
        "<?php new ($class)(); new ($class)()(); new (foo())()();",
        "<?php new (foo());",
        "<?php if (list($a, $b) = explode(' ', $x)) {}",
        "<?php while ([$k, $v] = f()) {}",
        "<?php $x = [$a, $b] = [1, 2];",
        "<?php f([$a] = $b);",
        "<?php fn() => [$a] = $b;",
        "<?php @list($a, $b) = f();",
        "<?php foreach ($xs as foo()) {} foreach ($xs as foo()->bar) {} foreach ($xs as $a[0]) {} foreach ($xs as $$a) {} foreach ($xs as $a::$b) {} foreach ($xs as $a?->b) {} foreach ($xs as &$a[0]) {} foreach ($xs as $k => &$v) {}",
        "<?php foreach ($xs as ($targets)[0]) {} foreach ($xs as ($a)->b) {} foreach ($xs as (foo())[0]) {} foreach ($xs as ($a + $b)[0]) {}",
        "<?php foreach ($x as $a[]) {}",
        "<?php foreach ($x as $a->b) {}",
        "<?php foreach ($x as A::$b) {}",
        "<?php foreach ($x as f()) {}",
        "<?php function __construct(final public int $v = 1) {}",
        "<?php class C { public $x { get; set; } }",
        "<?php class C { public $x { final get => 1; } }",
        "<?php class C { public $x { get => 1; final set => 1; } }",
        "<?php class C { public $x { #[A] get => 1; } }",
        "<?php class C { public static $x { get => 1; } }",
        "<?php class C { public $x = 1 { get => 1; } }",
        "<?php class C { public $x { foo => 1; } }",
        "<?php new #[A] class {};",
        "<?php new #[A] readonly class {};",
        "<?php #[A] fn() => 1;",
        "<?php #[A] function() {};",
        "<?php #[A] static fn() => 1;",
        "<?php #[A] const X = 1;",
        "<?php #[A] const X = 1, Y = 2;",
        "<?php class C { #[A] case X; }",
        "<?php function f(#[A] public $x) {}",
        "<?php $d |> fn($x) => $x;",
        "<?php $b |> static fn($x) => $x;",
        "<?php $b |> include 'x';",
        "<?php $x = \\array; \\array(1, 2); namespace\\array;",
        "<?php class C { public (A&B)|D $value; }",
        "<?php class C { public $x; public $x; const A = 1; const A = 2; }",
        "<?php class C { public int $a = 1, $a = 2; }",
        "<?php class C { public function __construct(public $x, public $x) {} }",
        "<?php class C { public $x { get => 1; get => 2; } }",
        "<?php unset($a); if ($a) unset($b); while (false) unset($c);",
        "<?php \\unset($a); namespace\\unset($b);",
        "<?php new A()::X;",
        "<?php new A()::$x;",
        "<?php $a()::b;",
        "<?php $a[0]::b;",
        "<?php $a < $b == $c;",
        "<?php $a == $b < $c;",
        "<?php $a < $b <=> $c;",
        "<?php $a <=> $b < $c;",
        "<?php $obj->value < $a == $b > ($c);",
        "<?php Foo<Bar|Baz>($c);",
        "<?php Foo<Bar&Baz>($c);",
        "<?php namespace App { use Vendor\\Thing; }",
        "<?php namespace App { const X = 1; }",
        "<?php static $a = 1, $b;",
        "<?php 'A'::b;",
        "<?php (A)::b;",
        "<?php f()::$b;",
        "<?php [A::class, 'b']();",
        "<?php X::class();",
        "<?php class E { const class = 7; }",
        "<?php enum E { case class; }",
        "<?php class C { function new() {} }",
        "<?php class C { function class() {} }",
        "<?php static fn() => 1;",
        "<?php static function() {};",
        "<?php [,];",
        "<?php [1,,2];",
        "<?php [1, 2] = $a;",
        "<?php [...$a] = $b;",
        "<?php list(1) = $a;",
        "<?php function f(int $x, ...$y, $z) {}",
        "<?php function f(public $x) {}",
        "<?php enum E { public $x; }",
        "<?php interface I { public $x; }",
        "<?php class C { static readonly $x; }",
        "<?php class C { var $x; }",
        "<?php class C { private(set) $x; }",
        "<?php class C { public private(set) $x; }",
        "<?php switch (1) { default: default: }",
        "<?php switch ($v) { case $a ? B : C: break; }",
        "<?php switch ($v) { case $a ? B::C : D::E: break; }",
        "<?php static::$$a; echo static::$$a; return static::$$a;",
        "<?php yield static::$$a;",
        "<?php $x = static::$$a; fn() => static::$$a;",
        "<?php $o instanceof static ? $$a : 0;",
        "<?php f(static::$$a); if (static::$$a) {} A::$$a;",
        "<?php label: __halt_compiler();",
        "<?php if (1): endif; __halt_compiler();",
        "<?php switch (1) { case 1: echo 1; } __halt_compiler();",
        "<?php break 0;",
        "<?php break $a;",
        "<?php try {}",
        "<?php declare(foo=1+1);",
        "<?php namespace namespace;",
        "<?php for ($i = 0, $j = 0; $i < 1, $j < 2; $i++, $j++) {}",
        "<?php function f((A&B)|C $x) {}",
        "<?php function f(C|(A&B) $x) {}",
        "<?php function f(A &$x) {}",
        "<?php function f(buffer $value) {}",
        "<?php class C { public buffer $value; }",
        "<?php $a = [&$x, &$y];",
        "<?php $a = array(&$x, 1, &$y); list(&$x, &$y) = $a;",
        "<?php $a = array(&$$name); $b = [&${$name}]; $c = array(&${$name});",
        "<?php $a = [&C::match()]; $b = [&$o->list];",
        "<?php self; parent; true; false; null; self(); $x = self; $a[self] = 1;",
        "<?php $a = [&$x][0];",
        "<?php $a = [&foo()];",
        "<?php [\"k\" => &$x];",
        "<?php $a = [0 => &$x[1]]; array(\"k\" => &$y[0]);",
        "<?php $a = [&readonly()]; $b = array(&readonly());",
        "<?php $r = &readonly()->p; $s = &readonly()(); $t = &readonly()::C[0];",
        "<?php $r = &($get)()[$key]; $s = &($a)[0]; $t = &($a)->p; $u = &($a)();",
        "<?php $a = [&($b)[0]]; $b = array(&($g)());",
        r#"<?php $r = &($a)::FOO[0]; $s = &($a)::$c; $t = &($a)::{"m"}(); $u = &($a)?->p;"#,
        "<?php $r = &($a)->p + 2; $a = ['key' => &($a)::FOO->p];",
        "<?php [f() => &($a)[0], C::fn() => &readonly()];",
        "<?php [fn() => 1 => &($a)[0]];",
        "<?php strlen(...); $f = $a(...); $g = $o->m(...); $h = C::m(...);",
        "<?php $cls::MATCH; (foo())::MATCH; $cls::LIST; $o::ENUM;",
        "<?php (foo())(...); $a[0](...); (new Foo)(...);",
        "<?php $o->$method(value: 1);",
        "<?php C::$method(value: 1);",
        "<?php class C { function m(public int $x) {} }",
        "<?php class C { function m(...$x, $y) {} }",
        "<?php class C { function __construct(public ...$x) {} }",
        "<?php class C { function __construct(public readonly &$x) {} }",
        "<?php $f = function(...$x, $y) {}; $g = fn(...$x, $y) => 1;",
        "<?php function f() { return $x; } function &g() { return $x; }",
        "<?php $c = function &() use (&$x) { return $x; };",
        "<?php packed(); class extern {} function packed() {} ifdef(); class ifdef {} extern();",
        "<?php ++$a[]; --$a[]; ++$a[][];",
        "<?php \"abc\"[0] = \"x\"; (1 + 2)[0] = 1; $o?->p = 1; [1] = 2;",
        "<?php interface I { protected $x { get; } }",
        "<?php interface I { public $x { get; } public $x { get; } }",
        "<?php interface I { abstract public $x { get; } final public $y { get; } }",
        "<?php new $a[0]; new $a->b(); new $a::$b; new self::$b; new A::$b;",
        "<?php for (include 'a.php'; false; ) {}",
        "<?php for ($i = 0; include 'a.php'; $i++) {}",
        "<?php new $a::$b();",
        "<?php f(a: 1, 2); f(...$args, 1);",
        "<?php $o->$m(...$args, 1); $o?->$m(a: 1, 2); $o->$m(a: 1, ...$args); C::$m(...$args, 1);",
        "<?php self::$m(1); static::$m(1); parent::$m(1);",
        "<?php $f = function(public int $x) {}; $g = fn(protected(set) int $x) => 1;",
        "<?php $x = new $classes[0]()->p = 1;",
        "<?php $x = new $a->b()->p = 1;",
        "<?php $x = new self::$b()->p = 1; new A::$b()->p = 1;",
        "<?php isset($a); empty($a); unset($a);",
        "<?php empty($a + 1);",
        "<?php isset($a, $b);",
        "<?php isset($a,);",
        "<?php isset($a + 1);",
        "<?php unset($a, $b);",
        "<?php unset($a,);",
        "<?php unset(foo());",
        "<?php eval('1');",
        "<?php \\empty($a);",
        "<?php exit; exit(1); die('x');",
        "<?php class C { use T { T::c insteadof D; } }",
        "<?php use A\\{B, C}; use function A\\{b}; use const A\\{B};",
        "<?php if (1) { ?> html <?php } else { ?> other <?php }",
        "<?php if (1): ?> html <?php else: ?> other <?php endif;",
        "<?php __halt_compiler();",
        "<?php __halt_compiler()\n?>raw",
        "<?php namespace X; __halt_compiler();",
        "<?php (double)$x; (unset)$x; (void)$x;",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_ok(), "{source}");
    }
}

/// TOKEN_PARSE rejects elephc dialect syntax and grammar-invalid constructs PHP rejects.
#[test]
fn tokenizer_php85_rejects_dialect_and_accept_invalid_inputs() {
    for source in [
        "<?php $x = unset($a);",
        "<?php echo unset($a);",
        "<?php echo (1) label:;",
        "<?php $o->p++[0];",
        "<?php $r = &($a);",
        "<?php $r = &($a)::FOO;",
        "<?php $a = [&($a)::class];",
        "<?php $r = &($a)[0] = 1;",
        "<?php $r = &($a)->p += 1;",
        "<?php $a = &readonly[0];",
        "<?php $r = &readonly() = 1;",
        "<?php $r = &readonly()::FOO;",
        "<?php [fn() => &($a)[0]];",
        "<?php array(fn&(): (A&B)|C => &readonly());",
        "<?php $o->p++->q;",
        "<?php $o->p++();",
        "<?php list(...$a $b) = $row;",
        "<?php [...$a $b] = $row;",
        "<?php foo() label:;",
        "<?php switch (1) label:;",
        "<?php try {} catch (Exception $e) label:;",
        "<?php @unset($a);",
        "<?php unset($a) or die('x');",
        "<?php extern 'System' { fn puts(s: ptr<u8>) -> i32; }",
        "<?php packed class P { public int $x; }",
        "<?php function f(array<int> $x) {}",
        "<?php final class B<T> {}",
        "<?php int $index = 1;",
        "<?php PDORow $typedLazy = $lazy;",
        "<?php $x = new Box<int>();",
        "<?php class C extends B<int> {}",
        "<?php $box->map<string>($f);",
        "<?php function f((A) $x) {}",
        "<?php function f((A&B) $x) {}",
        "<?php function f((A|B)|C $x) {}",
        "<?php function f(callable(int): string $cb) {}",
        "<?php function f(buffer<int> $value) {}",
        "<?php $a[&$b];",
        "<?php f($a...);",
        "<?php function f() { return &$x; }",
        "<?php function &f() { return &$x; }",
        "<?php $f = fn() => &$x;",
        "<?php $f = fn() => &$x;",
        "<?php [\"key\" => &1];",
        "<?php match(1) { 1 => &$x };",
        "<?php foreach ($xs as 1) {}",
        "<?php foreach ($xs as $a + $b) {}",
        "<?php foreach ($xs as $a ?? $b) {}",
        "<?php foreach ($xs as $k => 1) {}",
        "<?php foreach ($xs as ($a)) {}",
        "<?php foreach ($xs as array($a)) {}",
        "<?php $a = [0 => &$x + 1];",
        "<?php $a = ['k' => &$x + 1];",
        "<?php array(0 => &$x + 1);",
        "<?php new $a[0] = 1;",
        "<?php new $a->b = 1;",
        "<?php new A::$b = 1;",
        "<?php new self::$b = 1;",
        "<?php new static->p = 1;",
        "<?php function f(...) {}",
        "<?php function f(public (set) int $x) {}",
        "<?php function f(final(set) int $x) {}",
        "<?php ptr_cast<int>($p);",
        "<?php buffer_new<int>(8);",
        "<?php new #[A] Foo();",
        "<?php new $a[0]::B();",
        "<?php new $a->b::C();",
        "<?php new $a::B();",
        "<?php try {} finally ;",
        "<?php static $$a;",
        "<?php function f($$a) {}",
        "<?php class A { public $$a; }",
        "<?php try {} catch (E $$e) {}",
        "<?php $f = function() use ($$a) {};",
        "<?php $a = (void)foo();",
        "<?php function f($a = &$b) {}",
        "<?php $f = function ($a = &$b) {}; $g = fn($a = &$b) => 1; $h = static function ($a = &$b) {};",
        "<?php class A { foo: }",
        "<?php switch ($a): ?>\n    <?php case 1: ?>x<?php endswitch;",
        "<?php function empty() {}",
        "<?php class isset {}",
        "<?php interface list {}",
        "<?php trait empty {}",
        "<?php enum isset {}",
        "<?php const empty = 1;",
        "<?php function echo() {}",
        "<?php class match {}",
        "<?php $f = fn(...) => 1;",
        "<?php function &f(...) {}",
        "<?php f([]...);",
        "<?php 1 + 2 = 3;",
        "<?php ($a) = 1;",
        "<?php ($a)++;",
        "<?php ++$a = 1;",
        "<?php ++1;",
        "<?php 1++;",
        "<?php $a ** 2++;",
        "<?php ${1+};",
        "<?php ${};",
        "<?php ${1; 2};",
        "<?php ${new Box<int>()};",
        "<?php ${$box->map<string>($f)};",
        "<?php $x = \"${1+}\";",
        "<?php $x = <<<EOT\n${1+}\nEOT;",
        "<?php $s = \"${class + 1}\";",
        "<?php $s = \"${class[}\";",
        "<?php $s = \"${class[]}\";",
        "<?php $s = \"${class[0][1]}\";",
        "<?php $s = \"${class->p}\";",
        "<?php echo C::{1+};",
        "<?php echo C::{};",
        "<?php (foo())::BAR = 1;",
        "<?php (foo())::BAR++;",
        "<?php $cls::VALUE = 1;",
        "<?php ++$cls::VALUE;",
        "<?php X::__halt_compiler;",
        "<?php class C { const __halt_compiler = 1; }",
        "<?php function __halt_compiler() {}",
        "<?php foo(__halt_compiler: 1);",
        "<?php static $a = 1, $b = ;",
        "<?php goto done ?>text<?php echo ; done:",
        "<?php $a <=> $b == $c;",
        "<?php abstract class C { abstract public $x { get => ; } }",
        "<?php abstract class C { abstract public $x { get; set => ; } }",
        "<?php abstract class C { abstract public $x { get } }",
        "<?php class C { public $x { get => 1; final } }",
        "<?php class C { public $x { get => 1; #[A] } }",
        "<?php class C { public $x { final } }",
        "<?php class C { public $x { #[A] } }",
        "<?php $a = &new C;",
        "<?php $a = &1;",
        "<?php $a::class = 1;",
        "<?php $a == $b == $c;",
        "<?php $a < $b < $c;",
        "<?php $a <=> $b <=> $c;",
        "<?php $a <= $b >= $c;",
        "<?php isset();",
        "<?php empty();",
        "<?php unset();",
        "<?php empty($a, $b);",
        "<?php empty(value: $a);",
        "<?php empty(...$a);",
        "<?php empty($a,);",
        "<?php empty(...);",
        "<?php eval('1', '2');",
        "<?php eval(code: '1');",
        "<?php eval(...$a);",
        "<?php eval();",
        "<?php isset(value: $a);",
        "<?php isset(...$a);",
        "<?php isset(...);",
        "<?php unset(value: $a);",
        "<?php unset(...$a);",
        "<?php unset($a + 1);",
        "<?php unset(...);",
        "<?php exit->foo;",
        "<?php class C { $x; }",
        "<?php class C { int $x; }",
        "<?php class C { public (set) $x; }",
        "<?php function f(A&B|C $x) {}",
        "<?php function f(C|A&B $x) {}",
        "<?php function f(A&(B|C) $x) {}",
        "<?php try {} catch (A&B $e) {}",
        "<?php try {} catch (A $e);",
        "<?php try ; catch (A $e) {}",
        "<?php use A\\{};",
        "<?php use function A\\{function b};",
        "<?php function f() { use A; }",
        "<?php function f() { const A = 1; }",
        "<?php if (1) { const A = 1; }",
        "<?php class C { use T { c insteadof D; } }",
        "<?php if (1) ?> html <?php else ?> other <?php ;",
        "<?php if ($a): if ($b) ?>text<?php else echo 1; endif;",
        "<?php if ($a): if ($b) echo 1; ?>text<?php else echo 2; endif;",
        "<?php if (1) __halt_compiler();",
        "<?php function f() { __halt_compiler(); }",
        "<?php function f() { $s = \"{$a}\"; __halt_compiler(); }",
        "<?php new A::B();",
        "<?php new (foo())[0];",
        "<?php new (foo())->bar();",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err(), "{source}");
    }
}

/// TOKEN_PARSE reports a catchable parser error while plain scanning accepts incomplete PHP.
#[test]
fn tokenizer_parse_validates_and_reclassifies_reserved_identifiers() {
    assert!(tokenize(b"<?php function broken(", 0).is_ok());
    assert!(tokenize(b"<?php function broken(", TOKEN_PARSE).is_err());
    let source = b"<?php class C { public const PUBLIC = 1; public function match() {} }";
    let raw = tokenize(source, 0).unwrap();
    let parsed = tokenize(source, TOKEN_PARSE).unwrap();
    assert!(raw.iter().zip(&parsed).any(|(a,b)| a.id != b.id && b.id == id("T_STRING")));
    assert_eq!(raw.iter().map(|t| (t.pos,t.len,t.line)).collect::<Vec<_>>(), parsed.iter().map(|t| (t.pos,t.len,t.line)).collect::<Vec<_>>());
}

/// TOKEN_PARSE diagnostic wording and lines match the PHP 8.5 oracle.
#[test]
fn tokenizer_php85_parse_error_messages_match_php() {
    compare_messages_with_php85(&parse_message_corpus());
}

/// Returns the curated message-parity corpus.
fn parse_message_corpus() -> Vec<Vec<u8>> {
    let mut sources: Vec<Vec<u8>> = [
        "<?php 0b;", "<?php 0b2;", "<?php 0x;", "<?php 0xg;", "<?php 09;",
        "<?php 1e;", "<?php 1e+;", "<?php 1..2;", "<?php .e1;", "<?php 1_;",
        "<?php 1__2;", "<?php 0o8;", "<?php 1.5.6;", "<?php 1f;", "<?php 1_000_;",
        "<?php 0o78;", "<?php 078;", "<?php 0x1g;", "<?php 0b12;",
        "<?php function broken(", "<?php function f(", "<?php function f($x",
        "<?php class C {", "<?php class C { public", "<?php if (", "<?php if (1",
        "<?php if (1) {", "<?php while (", "<?php for (;;", "<?php foreach ($a as",
        "<?php switch (1) { case", "<?php try {", "<?php catch (", "<?php echo",
        "<?php return", "<?php new", "<?php $a =", "<?php $a->", "<?php $a[",
        "<?php list(", "<?php array(", "<?php [1,", "<?php match (1) {",
        "<?php fn(", "<?php use", "<?php namespace", "<?php declare(",
        "<?php const", "<?php enum", "<?php interface", "<?php trait", "<?php #[",
        "<?php \"unterminated", "<?php 'unterminated", "<?php /* unterminated",
        "<?php `unterminated", "<?php goto;", "<?php goto 1;", "<?php goto $a;",
        "<?php goto label", "<?php goto label; label: echo 1;",
        "<?php if ($a) goto done; else echo 1; done:",
        "<?php while (1) { goto done; } done:",
        "<?php if ($a): echo 1; ?>text<?php else: echo 2; endif;",
        "<?php if ($a): while ($b): echo 1; ?>x<?php endwhile; else: echo 2; endif;",
        "<?php if ($a): echo C::ENDIF; ?>text<?php else: echo 2; endif;",
        "<?php if ($a): echo C::IF; ?>text<?php else: echo 2; endif;",
        "<?php if ($a): while ($b) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): for (;;) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): foreach ($b as $c) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): declare(ticks=1) ?>text<?php else: echo 2; endif;",
        "<?php if ($a): if ($b) echo 1; ?>x<?php else: echo 2; endif;",
        "<?php if ($a): if ($b) { echo 1; } ?>x<?php else: echo 2; endif;",
        r"<?php $a = &\foo();",
        r"<?php $a = &Ns\foo();",
        r"<?php $a = &namespace\foo();",
        "<?php $r = &($a);",
        "<?php $r = &($a)",
        "<?php $r = &($a) ?>",
        "<?php $r = &(1 +);",
        "<?php $r = &(1 +) + 1;",
        "<?php [&(1 +) + 1];",
        "<?php [&(1 +), ${1+}];",
        "<?php $r = &(1 +)::{1+};",
        "<?php $r = &([1 +, ${1+}]);",
        "<?php $r = &readonly([1 +, ${1+}])::C;",
        "<?php [fn() => &($a)[0]];",
        "<?php [fn() => &$a];",
        "<?php array(fn&(): (A&B)|C => &readonly());",
        "<?php [f() => &($a)[0], C::fn() => &readonly()];",
        "<?php [fn() => 1 => &($a)[0]];",
        "<?php foreach ($a as fn() => &$b) {}",
        "<?php foreach ($a as fn(): int => &$b) {}",
        "<?php foreach ($a as $k => fn() => &$b) {}",
        "<?php foreach ($a as fn&(): int => &$b) {}",
        "<?php foreach ($a as fn(): (A&B)|C => &readonly()) {}",
        "<?php foreach ($a as fn(): int => &($b)[0]) {}",
        "<?php foreach ($a as static fn(): int => &$b) {}",
        "<?php foreach ($a as FN() => &$b) {}",
        "<?php foreach ($a as STATIC fN() => &$b) {}",
        "<?php foreach ($a as STATIC\nFUNCTION() {}) {}",
        "<?php foreach ($a as FUNCTION() {}) {}",
        "<?php foreach ($a as static) {}",
        "<?php foreach ($a as ECHO) {}",
        "<?php foreach ($a as __HALT_COMPILER) {}",
        "<?php foreach ($a as MATCH) {}",
        "<?php foreach ($a as (match (1) {default => $x})[0]) {}",
        "<?php $x = static & $y;",
        "<?php $x = STATIC",
        "<?php $x = static ?>",
        "<?php $x = ECHO;",
        "<?php 1 DiE;",
        "<?php 1 EXIT;",
        "<?php 1 __dIr__;",
        "<?php 1 YiElD FrOm $x;",
        "<?php 1 yield\nfrom\n$x;",
        "<?php 1 yield\n/* comment */\nfrom $x;",
        "<?php 1 PuBlIc(SeT);",
        "<?php 1 NotAKeyword;",
        "<?php $x = LIST;",
        "<?php $x = LIST ?>",
        "<?php $x = LIST",
        r"<?php $x = \LIST;",
        "<?php foo(list);",
        "<?php foo(1, LIST);",
        "<?php foo(list, 1);",
        r"<?php foo(list\);",
        r"<?php foo(list \ Bar);",
        r"<?php foo(list\ Bar);",
        r"<?php foo(list \Bar);",
        r"<?php foo(list Foo\Bar);",
        r"<?php foo(list namespace\Bar);",
        "<?php foo(list ?>",
        r"<?php foo(list\Bar);",
        "<?php new Foo(list);",
        "<?php exit(list);",
        "<?php die(list);",
        "<?php #[A(list)] function f() {}",
        "<?php unset(LIST);",
        "<?php unset(list, $a);",
        "<?php unset($a, list);",
        r"<?php unset($a, list\);",
        r"<?php unset(list /* comment */ \Bar);",
        "<?php isset(list, $a);",
        r"<?php isset(list \Bar);",
        "<?php empty(list);",
        "<?php eval(list);",
        "<?php print list;",
        "<?php foo(list: 1);",
        r"<?php foo(\LIST);",
        "<?php unset($a, $b);",
        "<?php foreach ($a as $k => [fn() => &$v]) {}",
        "<?php foreach ($a as $k => &$v) {}",
        "<?php foreach ($a as (fn() => 1)[0]) {}",
        "<?php foreach ($a as static::fn()) {}",
        "<?php $r = &($a)::FOO;",
        "<?php $r = &($a)::class;",
        r#"<?php $r = &($a)::{"p"};"#,
        "<?php $r = &($a)::FOO + 1;",
        "<?php $r = &($a)::FOO->p::BAR;",
        "<?php $a = [&($a)::FOO];",
        "<?php $a = ['key' => &($a)::class];",
        "<?php array(&($a)[0] = 1);",
        "<?php $r = &($a)[0] = 1;",
        "<?php $r = &($a)->p += 1;",
        "<?php $r = &($a)::$c ??= 1;",
        "<?php $r = &($a)[0]++;",
        "<?php $a = &readonly[0];",
        "<?php $r = &readonly::$a;",
        "<?php $r = &readonly() = 1;",
        "<?php $r = &readonly()++;",
        "<?php $r = &readonly()[0] = 1;",
        "<?php $r = &readonly()->p = 1;",
        "<?php $r = &readonly()::$p = 1;",
        "<?php $r = &readonly()::FOO;",
        "<?php [&readonly()::FOO];",
        "<?php ['k' => &readonly()::FOO];",
        "<?php [&readonly() = 1];",
        "<?php [&readonly()++];",
        "<?php $r = &readonly(1 +)::FOO;",
        "<?php $a = [&readonly];",
        "<?php $r = &readonly",
        "<?php $r = &readonly ?>",
        "<?php echo ;\n$r = &($a)::FOO;",
        "<?php $a[&$b];",
        "<?php function f(callable(int): string $cb) {}",
        "<?php function f(buffer<int> $value) {}",
        "<?php f($a...);",
        "<?php f([]...);",
        "<?php function f() { return &$x; }",
        "<?php function &f() { return &$x; }",
        "<?php 1 + 2 = 3;", "<?php ($a) = 1;", "<?php array &$arr;",
        "<?php new (foo())[0];", "<?php function f(A&B|C $x) {}",
        "<?php $a == $b == $c;", "<?php $a <=> $b == $c;",
        "<?php $a < $b <=> $c;", "<?php $a < $b == $c;",
        "<?php $obj->value < $a == $b > ($c);",
        "<?php Foo<Bar|Baz>($c);",
        "<?php Foo<Bar&Baz>($c);",
        "<?php ++1;", "<?php 1++;", "<?php ++$a = 1;",
        "<?php ++$a[0] = 1;", "<?php (foo())++;", "<?php $a ** 2++;",
        "<?php ++$a ** 2;", "<?php ++$a[$i = 0];",
        "<?php ++\"abc\"[0];", "<?php \"abc\"[0]++;",
        "<?php (new Foo)[0]++;", "<?php (clone $o)->p++;",
        "<?php (1 + 2)[0]++;", "<?php (match(1){1=>1})[0]++;",
        "<?php ++$a->b();", "<?php $a->b()++;", "<?php ++$a();",
        "<?php $a()++;", "<?php ++$a?->b;", "<?php ++$a::$b;",
        "<?php ++Foo::BAR[0];", "<?php Foo::BAR[0]++;",
        "<?php ${1+};", "<?php ${};", "<?php ${1; 2};", "<?php ${1 2};",
        "<?php $x = <<<EOT\n${1+}\nEOT;",
        "<?php ${1 ? 2 : 3};", "<?php ${fn() => 1};",
        "<?php ${match(1) { 1 => 2 }};", "<?php ${new Foo()};",
        "<?php ${include \"x\"};", "<?php ${$a[0]};", "<?php ${$$a};",
        "<?php ${${\"a\"}};", "<?php ${[1,2]};",
        r#"<?php $s = `\${}`;"#,
        r#"<?php $s = `\$foo`;"#,
        r#"<?php $s = `a\$b${x}`;"#,
        "<?php $s = \"${class}\";", "<?php $s = \"${class[0]}\";",
        "<?php $s = \"${class[1+2]}\";", "<?php $s = \"${foo + 1}\";",
        "<?php echo C::{'FOO'};", "<?php echo C::{strtoupper('foo')};",
        "<?php echo C::{'FO' . 'O'};", "<?php echo $cls::{expr};",
        "<?php echo C::{foo()}::BAR;",
        "<?php (foo())::$bar = 1;",
        "<?php ++(foo())::$bar;",
        "<?php $cls::$prop++;",
        "<?php $cls::$prop = 1;",
        "<?php $cls::$method();",
        "<?php $o->$m(...['a' => 1, 0 => 2]);",
        "<?php $o->$m(...['a' => 1, true => 2]);",
        "<?php f(...['a' => 1, 0 => 2]);",
        "<?php $cls::VALUE;",
        "<?php $o->__halt_compiler; echo 1;",
        "<?php $o?->__halt_compiler;",
        "<?php namespace\\__halt_compiler;",
        "<?php $s = \"${class + 1}\";", "<?php $s = \"${class[}\";",
        "<?php $s = \"${class[]}\";", "<?php $s = \"${class[0][1]}\";",
        "<?php $s = \"${class->p}\";",
        "<?php echo C::{1+};",
        "<?php echo C::{};",
        "<?php (foo())::BAR = 1;",
        "<?php (foo())::BAR++;",
        "<?php $cls::VALUE = 1;",
        "<?php $x = \"${1+}\";",
        "<?php static $a = 1, $b;", "<?php static $a = 1, $b = ;",
        "<?php namespace App { use Vendor\\Thing; }",
        "<?php namespace App { const X = 1; }",
        "<?php goto done ?>text<?php echo ; done:",
        "<?php abstract class C { abstract public $x { get => ; } }",
        "<?php abstract class C { abstract public $x { get } }",
        "<?php class C { public $x { get => 1; final } }",
        "<?php class C { public $x { get => 1; #[A] } }",
        "<?php class C { public $x { final } }",
        "<?php class C { public $x { #[A] } }",
        "<?php isset();", "<?php empty();",
        "<?php unset();", "<?php exit->foo;", "<?php class C { $x; }",
        "<?php class C { public (set) $x; }", "<?php try {} catch (A $e);",
        "<?php use A\\{};", "<?php if (1) ?> html <?php else ?> other <?php ;",
        "<?php if (1) __halt_compiler();",
        "<?php new A::B();",
        "<?php\nfunction broken(", "<?php\nclass C {", "<?php\nif (1) {",
        "<?php\n$a[", "<?php\n0b;", "<?php\n1..2;", "<?php\ngoto;",
        "<?php\nreturn",
        "<?php use ;", "<?php echo 1 \"b\";", "<?php echo 'a' 'b';",
        "<?php 1 <<<EOT\nx\nEOT;",
        "<?php echo $a->{\"b\" \"c\"};", "<?php echo $a->{1 2};",
        "<?php \"{$ b}\";", "<?php \"{$ }\";", "<?php \"{$ a }\";",
        "<?php \"$a[1-2]\";", "<?php \"$a[\";", "<?php \"{$a[\";",
        "<?php \"{$a->\";", "<?php \"{$a[}\";", "<?php \"{$a[0}\";",
        "<?php \"{$0}\";", "<?php \"{$-1}\";",
        "<?php 1[0]; 1.5[0]; 1->b; 1::b; 1();",
        "<?php $a = array(&1); $b = array(1, &2); list(&1) = $a;",
        "<?php $a = array(&$x + 1); $b = [&$y + 1];",
        "<?php $a = array(&$x = 1); $b = [&$y ? 1 : 2];",
        "<?php $a = [&$x and $y]; $b = [&$x or $y];",
        "<?php 1 == 2 <=> 3;", "<?php if (1) { $a = 1 }",
        "<?php const A = 1, B;", "<?php class C extends {}",
        "<?php new;", "<?php echo $a->;", "<?php if ($a) }",
        "<?php function var() {}", "<?php function callable() {}", "<?php function die() {}",
        "<?php function f() use", "<?php ?A&B $x;",
        "<?php namespace A", "<?php declare(strict_types=1)",
        "<?php echo 1 2;", "<?php return 1 2;", "<?php $a = 1 2;",
        "<?php foo() 1;", "<?php use A\\B", "<?php const X = 1",
        "<?php global $a", "<?php static $a", "<?php unset($a)",
        "<?php break", "<?php continue", "<?php do {} while (1)",
        "<?php goto x", "<?php echo 1, 2", "<?php $a",
        "<?php throw $e", "<?php match($a) {}", "<?php fn() => 1",
        "<?php abstract function f();", "<?php final function f();",
        "<?php readonly function f();", "<?php C::;", "<?php fn;",
        "<?php fn();", "<?php match;", "<?php match();", "<?php $a?->;",
        "<?php echo $a::;", "<?php new class;",
        "<?php function f(int $$a) {}", "<?php class C { public int $$a; }",
        "<?php [foo($a, &$b)];",
        "<?php $a[]++ = 1;", "<?php ++$a[]++;",
        "<?php ++$a[] = 1;", "<?php ++$a[] += 1;", "<?php ++$a[]->p = 1;",
        "<?php function &f($a = &$b) {}", "<?php function f($a = &$b) {}",
        "<?php function readonly($a = &$b) {}", "<?php function &readonly($a = &$b) {}",
        "<?php class C { function &readonly($a = &$b) {} }",
        "<?php $f = fn &($a = &$b) => 1;",
        "<?php class A { function list($a = &$b) {} }",
        "<?php function f() { __halt_compiler(); }",
        "<?php function f() { $s = \"{$a}\"; __halt_compiler(); }",
        "<?php\nfunction f() {\n__halt_compiler();\n}",
        "<?php function f() { $x = match ($a) { __halt_compiler(); } }",
        "<?php $x = match ($a) { __halt_compiler(); }",
        "<?php $x = match ($a) { 1 => 1, __halt_compiler() };",
        "<?php $x = match ($a) { 1 => 1 __halt_compiler() };",
        "<?php function f() { $x = match ($a) { 1 => 1, __halt_compiler() }; }",
        "<?php function f() { $x = match ($a) { 1 => 1 __halt_compiler() }; }",
        "<?php $x = match ($a) { 1, __halt_compiler() => 2 };",
        "<?php $x = match ($a) { 1 __halt_compiler() => 2 };",
        "<?php $x = match ($a) { default __halt_compiler() => 1 };",
        "<?php $x = match ($a) { 1 => 1, 2 => 2, __halt_compiler() };",
        "<?php $x = match ($a) { 1 => (1), __halt_compiler() };",
        "<?php $x = match ($a) { 1 => match ($b) { 1 => 1 }, __halt_compiler() };",
        "<?php $x = match ($a) { 1 => __halt_compiler() };",
        "<?php $x = match ($a) { 1 => foo(__halt_compiler()) };",
        "<?php $x = match ($a) { 1 => 1 + __halt_compiler() };",
        "<?php function f() { $x = match ($a) { 1 => { __halt_compiler(); } }; }",
        "<?php $f = fn() => { __halt_compiler(); };",
        "<?php $x = match ($a) { 1 => Foo::class __halt_compiler() };",
        "<?php $x = match ($a) { Foo::class __halt_compiler() => 1 };",
        "<?php $x = match ($a) { 1 => exit __halt_compiler() };",
        "<?php $x = match ($a) { 1 => die __halt_compiler() };",
        "<?php $x = match ($a) { 1 => yield __halt_compiler() };",
        "<?php $x = match ($a) { exit __halt_compiler() => 1 };",
        "<?php $x = match ($a) { 1 => yield from __halt_compiler() };",
        "<?php $x = match ($a) { 1 => new Foo __halt_compiler() };",
        "<?php $x = match ($a) { 1 => new class __halt_compiler() };",
        "<?php $x = match ($a) { 1 => new class extends Foo __halt_compiler() };",
        "<?php $x = match ($a) { 1 => new class($a) __halt_compiler() };",
        "<?php $x = match ($a) { 1 => new __halt_compiler() };",
        "<?php $x = match ($a) { 1 => empty __halt_compiler() };",
        "<?php $x = match ($a) { 1 => fn __halt_compiler() };",
        "<?php $x = match ($a) { 1 => class __halt_compiler() };",
        "<?php switch ($a) { ?>html<?php __halt_compiler(); }",
        "<?php switch ($a): ?>html<?php __halt_compiler(); endswitch;",
        "<?php function f() { switch ($a) { ?>html<?php __halt_compiler(); } }",
        "<?php $x = match ($a) { 1 => $b++ __halt_compiler() };",
        "<?php $x = match ($a) { 1 => $b-- __halt_compiler() };",
        "<?php $x = match ($a) { 1 => $b->p++ __halt_compiler() };",
        "<?php $x = match ($a) { 1 => new static __halt_compiler() };",
        "<?php $x = match ($a) { 1 => $b instanceof static __halt_compiler() };",
        "<?php $x = match ($a) { 1 => static __halt_compiler() };",
        "<?php function f() { for ($i = 0; __halt_compiler(); ) {} }",
        "<?php function f() { for (__halt_compiler(); ; ) {} }",
        "<?php function f() { for ($i = 0; $i < 1; __halt_compiler()) {} }",
        "<?php for ($i = 0; __halt_compiler(); ) {}",
        "<?php function f() { $x = match ($a) { 1 => 2 } __halt_compiler(); }",
        "<?php function f() { $x = function () { return 1; } __halt_compiler(); }",
        "<?php function f() { $x = function () use ($a) { return 1; } __halt_compiler(); }",
        "<?php function f() { $x = static function () { return 1; } __halt_compiler(); }",
        "<?php function f() { $x = new class {} __halt_compiler(); }",
        "<?php function f() { new class {} __halt_compiler(); }",
        "<?php function f() { $x = C::{1} __halt_compiler(); }",
        "<?php function f() { $x = $a ? function () { return 1; } __halt_compiler() : 1; }",
        "<?php switch (match ($x) { 1 => 2 }) { __halt_compiler(); }",
        "<?php $x = match ($a) { fn() => 1 __halt_compiler() => 2 };",
        "<?php $x = match ($a) { 1 => fn() => 1, __halt_compiler() };",
        "<?php $x = match ($a) { C::match __halt_compiler() => 1 };",
        "<?php $x = match ($a) { C::new __halt_compiler() => 1 };",
        "<?php $x = match ($a) { C::static __halt_compiler() => 1 };",
        "<?php $x = match ($a) { 1 => C::static __halt_compiler() };",
        "<?php switch (\"{$x}\") { __halt_compiler(); }",
        "<?php switch (\"${x}\") { __halt_compiler(); }",
        "<?php function f() { for ($i = \"{$x}\"; __halt_compiler(); ) {} }",
        "<?php function f() { for ($i = \"${x}\"; __halt_compiler(); ) {} }",
        "<?php function f() { $x = new class(\"{$a}\") {} __halt_compiler(); }",
        "<?php function f() { $x = function ($a = \"{$b}\") { return 1; } __halt_compiler(); }",
        "<?php $x = match ($a) { fn($b = \"{$c}\") => 1 __halt_compiler() => 2 };",
        "<?php function f() { __halt_compiler(1); }",
        "<?php function f() { __halt_compiler($a); }",
        "<?php function f() { __halt_compiler(1.5); }",
        "<?php function f() { __halt_compiler('a'); }",
        "<?php function f() { __halt_compiler(foo); }",
        "<?php function f() { __halt_compiler(Foo\\Bar); }",
        "<?php function f() { __halt_compiler(\\Foo); }",
        "<?php function f() { __halt_compiler(namespace\\Foo); }",
        "<?php function f() { __halt_compiler(echo); }",
        "<?php function f() { __halt_compiler(static); }",
        "<?php function f() { __halt_compiler(\"a$b\"); }",
        "<?php __halt_compiler(Foo\\Bar);",
        "<?php __halt_compiler(\\Foo);",
        "<?php __halt_compiler(namespace\\Foo);",
        "<?php __halt_compiler(echo);",
        "<?php __halt_compiler(\"a$b\");",
        "<?php if (1): __halt_compiler(1); endif;",
        "<?php switch ($a) { case 1: __halt_compiler(1); }",
        "<?php namespace N { __halt_compiler(1); }",
        "<?php\nnamespace N {\n__halt_compiler(1);\n}",
        "<?php __halt_compiler(1);",
        "<?php function f() { for ($i = 0; __halt_compiler(1); ) {} }",
        "<?php switch ($a) { __halt_compiler(1); }",
        "<?php function f($x) { switch ($x) { case 1: break; } if ($x) { __halt_compiler(); } }",
        "<?php switch (1) {} if (1) { __halt_compiler(); }",
        "<?php function f() { switch (1) {} { __halt_compiler(); } }",
        "<?php class A {} if (1) { __halt_compiler(); }",
        "<?php interface I {} { __halt_compiler(); }",
        "<?php enum E {} while (1) { __halt_compiler(); }",
        "<?php class A {} if (1) {} else { __halt_compiler(); }",
        "<?php __halt_compiler((int));",
        "<?php function f() { __halt_compiler((int)); }",
        "<?php __halt_compiler(array);",
        "<?php function f() { __halt_compiler(array); }",
        "<?php\nfunction f() {\n__halt_compiler(array);\n}",
        "<?php\nnamespace N {\n__halt_compiler(array);\n}",
        "<?php if (1): __halt_compiler(array); endif;",
        "<?php switch ($a) { case 1: __halt_compiler(array); }",
        "<?php function f() { for ($i = 0; __halt_compiler(array); ) {} }",
        "<?php switch ($a) { __halt_compiler(array); }",
        "<?php function f() { __halt_compiler; }",
        "<?php echo ; function f() { __halt_compiler(array); }",
        "<?php function f() { foo(__halt_compiler(array)); }",
        "<?php __halt_compiler(<<<EOT\nx\nEOT\n);",
        "<?php function f() { __halt_compiler(<<<EOT\nx\nEOT\n); }",
        "<?php __halt_compiler((integer));",
        "<?php function f() { __halt_compiler(( Array )); }",
        "<?php function f() { __halt_compiler (boolean); }",
        "<?php __halt_compiler((double));",
        "<?php __halt_compiler(binary);",
        "<?php __halt_compiler(<<<'EOT'\nx\nEOT\n);",
        "<?php function f() { __halt_compiler(<<<'EOT'\nx\nEOT\n); }",
        "<?php __halt_compiler(<<<\"EOT\"\nx\nEOT\n);",
        "<?php __halt_compiler(<<<EOT\r\nx\r\nEOT\r\n);",
        "<?php __halt_compiler(<<<'EOT'\r\nx\r\nEOT\r\n);",
        "<?php function f() { __halt_compiler <<<EOT\nx\nEOT;\n }",
        "<?php __halt_compiler <<<EOT\nx\nEOT;\n",
        "<?php function f() { __halt_compiler ?>x",
        "<?php function f() { __halt_compiler( ?>x",
        "<?php __halt_compiler ?>x",
        "<?php __halt_compiler( ?>x",
        "<?php class A { function __construct(private(set) $$a) {} }",
        "<?php class A { function __construct(public(set) $$a) {} }",
        "<?php class A { function __construct(protected(set) $$a) {} }",
        "<?php class A { function __construct(final $$a) {} }",
        "<?php class A { function __construct(public final $$a) {} }",
        "<?php function f() { C::{__halt_compiler()}; }",
        "<?php function f() { $o->{__halt_compiler()}; }",
        "<?php function f() { $o?->{__halt_compiler()}; }",
        "<?php function f() { switch ($a) { __halt_compiler(); } }",
        "<?php switch ($a): __halt_compiler(); endswitch;",
        "<?php class C { function m() { __halt_compiler(); } }",
        "<?php namespace N { __halt_compiler(); }",
        "<?php\nnamespace N {\n__halt_compiler();\n}",
        "<?php\nnamespace N {\n    echo 1;\n    __halt_compiler();\n}",
        "<?php function f() { if (1) __halt_compiler(); }",
        "<?php if (1): __halt_compiler(); endif;",
        "<?php class C { public $x { get { __halt_compiler(); } } }",
        "<?php class C { use T { __halt_compiler(); } }",
        "<?php echo \"${__halt_compiler()}\";",
        "<?= __halt_compiler();",
        "<?php class A { public static ?int $$a; }",
        "<?php class A { public private(set) int $$a; }",
        "<?php class A { private(set) public int $$a; }",
        "<?php class A { protected(set) readonly int $$a; }",
        "<?php class A { public final int $$a; }",
        "<?php abstract class A { public abstract int $$a { get; } }",
        "<?php class A { public $$a; }",
        "<?php class C { function match(...) {} }",
        "<?php class C { function &match(...) {} }",
        "<?php function f(...$a,\n...) {}",
        "<?php echo ;\n${1+};",
        "<?php function f(empty $x): isset {}",
    ]
    .into_iter()
    .map(|source| source.as_bytes().to_vec())
    .collect();
    sources.push(b"<?php \"$a[\\1]\";".to_vec());
    sources.push(b"<?php \"$a[\x00]\";".to_vec());
    sources.push(b"<?php \"$a[1\x00]\";".to_vec());
    sources
}

/// Compares TOKEN_PARSE messages and lines for every source against the PHP 8.5 oracle.
fn compare_messages_with_php85(sources: &[Vec<u8>]) {
    let available = Command::new("php")
        .args(["-r", "echo PHP_VERSION_ID >= 80500 && PHP_VERSION_ID < 80600 && extension_loaded('tokenizer') ? 'yes' : 'no';"])
        .output();
    if !available.is_ok_and(|output| output.stdout == b"yes") {
        assert!(
            std::env::var_os("ELEPHC_TOKENIZER_REQUIRE_PHP85").is_none(),
            "CI requires PHP 8.5 with tokenizer enabled"
        );
        return;
    }
    let input = serde_json::to_vec(&sources.iter().map(|source| hex(source)).collect::<Vec<_>>()).unwrap();
    let script = r#"
        $out = [];
        foreach (json_decode(stream_get_contents(STDIN), true) as $hex) {
            $code = hex2bin($hex);
            try {
                PhpToken::tokenize($code, TOKEN_PARSE);
                $out[] = null;
            } catch (Throwable $e) {
                $out[] = [$e->getMessage(), $e->getLine()];
            }
        }
        echo json_encode($out, JSON_THROW_ON_ERROR);
    "#;
    let mut child = Command::new("php")
        .args(["-d", "short_open_tag=0", "-d", "error_reporting=0", "-r", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "PHP message oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut mismatches = Vec::new();
    for (source, expected) in sources.iter().zip(expected) {
        let got = match tokenize_with_warnings(source, TOKEN_PARSE) {
            Ok(_) => Value::Null,
            Err(error) => json!([error.message, error.line]),
        };
        if got != expected {
            mismatches.push(format!(
                "source={}\n  got {got}\n  expected {expected}",
                String::from_utf8_lossy(source)
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}

/// Echo statement validation must not reject a reserved word used as a constant name.
#[test]
fn tokenizer_parse_distinguishes_echo_statements_from_constant_names() {
    let source = b"<?php class C { const ECHO = 1; } C::ECHO; echo C::ECHO;";
    let parsed = tokenize(source, TOKEN_PARSE).expect("reserved constant names are valid PHP");
    let names: Vec<_> = parsed.iter().filter(|t| &source[t.pos..t.pos + t.len] == b"ECHO").collect();
    assert_eq!(names.len(), 3);
    assert!(names.iter().all(|t| t.id == id("T_STRING")));
    assert!(parsed.iter().any(|t| t.id == id("T_ECHO")));
    for invalid in ["<?php echo;", "<?php echo /*comment*/ ;", "<?php echo 1,;", "<?=?>", "<?= ; ?>"] {
        assert!(tokenize(invalid.as_bytes(), TOKEN_PARSE).is_err(), "{invalid}");
    }
}

/// Zend excludes extends/implements prefixes from enum declaration lookahead.
#[test]
fn tokenizer_enum_lookahead_preserves_php85_prefix_exclusions() {
    for name in ["extendsFoo", "implementsFoo", "ExTeNdS_Foo", "ImPlEmEnTs_Foo", "extendsé"] {
        let source = format!("<?php enum {name} {{ case A; }}");
        let tokens = tokenize(source.as_bytes(), 0).unwrap();
        assert_eq!(tokens.iter().find(|t| t.pos == 6).unwrap().id, id("T_STRING"), "{source}");
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err(), "{source}");
    }
    for name in ["FooExtends", "FooImplements", "XextendsFoo", "XimplementsFoo"] {
        let source = format!("<?php enum {name} {{ case A; }}");
        for flags in [0, TOKEN_PARSE] {
            let tokens = tokenize(source.as_bytes(), flags).unwrap();
            assert_eq!(tokens.iter().find(|t| t.pos == 6).unwrap().id, id("T_ENUM"), "{source}, flags={flags}");
        }
    }
}

/// Compares the same corpus against PHP 8.5's Zend tokenizer in both flag modes.
#[test]
fn tokenizer_php85_differential_corpus() {
    compare_with_php85(&fixtures());
}

/// Compares every php-src ext/tokenizer test file against the PHP 8.5 oracle.
#[test]
fn tokenizer_php_src_ext_tokenizer_files_match_php() {
    compare_with_php85(&php_src_fixtures());
}

/// Loads the transcribed `--FILE--` sources from php-src's ext/tokenizer/tests suite.
///
/// The corpus is PHP License 3.01 (see `tests/tokenizer_php_src/LICENSE`); the JSON holds
/// only sources, so this differential test never reads a `.phpt` file.
fn php_src_fixtures() -> Vec<Vec<u8>> {
    const FILES: &str = include_str!("data/php_src_tokenizer_files.json");
    serde_json::from_str::<Vec<String>>(FILES)
        .expect("vendored php-src tokenizer fixture must be a JSON string list")
        .into_iter()
        .map(|source| (0..source.len()).step_by(2)
            .map(|index| u8::from_str_radix(&source[index..index + 2], 16).expect("hex fixture byte"))
            .collect())
        .collect()
}

/// Compares tokenization and TOKEN_PARSE outcomes for every source against PHP 8.5.
fn compare_with_php85(sources: &[Vec<u8>]) {
    let available = Command::new("php").args(["-r", "echo PHP_VERSION_ID >= 80500 && PHP_VERSION_ID < 80600 && extension_loaded('tokenizer') ? 'yes' : 'no';"]).output();
    if !available.is_ok_and(|output| output.stdout == b"yes") {
        assert!(std::env::var_os("ELEPHC_TOKENIZER_REQUIRE_PHP85").is_none(), "CI requires PHP 8.5 with tokenizer enabled");
        return;
    }
    let input = serde_json::to_vec(&sources.iter().map(|s| hex(s)).collect::<Vec<_>>()).unwrap();
    let script = r#"
        $out = [];
        foreach (json_decode(stream_get_contents(STDIN), true) as $hex) {
            $code = hex2bin($hex);
            foreach ([0, TOKEN_PARSE] as $flags) {
                try {
                    $row = []; $pos = 0; $line = 1;
                    foreach (PhpToken::tokenize($code, $flags) as $t) {
                        $row[] = [$t->id < 256 ? null : token_name($t->id), bin2hex($t->text), $t->line, $t->pos];
                    }
                    $out[] = $row;
                } catch (Throwable $e) { $out[] = 'ParseError'; }
            }
        }
        echo json_encode($out, JSON_THROW_ON_ERROR);
    "#;
    let mut child = Command::new("php").args(["-d", "short_open_tag=0", "-d", "error_reporting=0", "-r", script])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "PHP oracle failed: {}", String::from_utf8_lossy(&output.stderr));
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut case = 0;
    let mut mismatches = Vec::new();
    for source in sources {
        for flags in [0, TOKEN_PARSE] {
            let got = match tokenize(source, flags) {
                Ok(tokens) => Value::Array(tokens.iter().map(|t| json!([
                    if t.id < 256 { None } else { token_name(t.id) },
                    hex(&source[t.pos..t.pos + t.len]), t.line, t.pos,
                ])).collect()),
                Err(_) => json!("ParseError"),
            };
            if got != expected[case] {
                let detail = match (&got, &expected[case]) {
                    (Value::Array(a), Value::Array(b)) => {
                        let index = a.iter().zip(b).position(|(a,b)| a != b).unwrap_or(a.len().min(b.len()));
                        format!("token {index}: got {:?}; expected {:?}; counts {} vs {}", a.get(index), b.get(index), a.len(), b.len())
                    }
                    _ => format!("got {}; expected {}", if got.is_array() { "tokens" } else { "ParseError" }, if expected[case].is_array() { "tokens" } else { "ParseError" }),
                };
                mismatches.push(format!("flags={flags}, source={}\n{detail}", String::from_utf8_lossy(source)));
            }
            case += 1;
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}
/// Covers trivia, PHP 8.5 syntax, parser feedback, interpolation and malformed input.
fn fixtures() -> Vec<Vec<u8>> {
    let mut sources = vec![
        "", "plain HTML", "<?php", "<?phpx echo 1;", "<?PHP\r\necho 1; ?>\r\nhtml",
        "before<?=1?>after", "<?xml version='1.0'?>", "<? echo 1; ?>", "#!/usr/bin/php\n<?php echo 1;",
        "<?php \t\r\n\r\necho 1; // comment\n# comment ?>html",
        "<?php /** doc */ /**not_doc*/ /**/ /****/ /* comment */ echo 1;",
        "<?php class C { public const PUBLIC = 1; public function match() {} }",
        "<?php class C { const ECHO = 1; } C::ECHO; echo C::ECHO;",
        "<?php class C { const ECHO = 1; } C::ECHO /*comment*/ ; ?>",
        "<?php $x->class; $x?->match; $x-> /*comment*/ function; C::class; C::match();",
        "<?php namespace A\\B; use Foo\\Bar as B; \\strlen('x'); namespace\\x();",
        "<?php (int\n)$x; (i n t)$x; ( int )$x; (int\r\n);",
        "<?php $x->foo\\bar; $x?->namespace\\name;",
        r#"<?php $s = "\u{D800}"; $s = "\\u{110000}"; $s = '\u{110000}';"#,
        "<?php $s = <<<TXT\n\\u{110000}\nTXT;",
        "<?php $s = <<<'TXT'\n\\u{110000}\nTXT;",
        r#"<?php $s = "\u{FFFFFFFFFFFFFFFFFFFFFF}";"#,
        r#"<?php $s = "\u{110000}";"#, r#"<?php $s = "\u{}";"#,
        r#"<?php $s = "\u{unterminated";"#,
        "<?php class C { public function class() {} public function clone() {} } C::class; clone($x);",
        "<?php (real)$x;", "<?php (unset)$x;", "<?php (void)$x;", "<?=?>",
        "<?php echo 1,;", "<?php print;", "<?php throw;", "<?php yield from;",
        "<?php __halt_compiler( /*c*/ ) /*c*/ ; raw", "<?php __halt_compiler // comment\n ( ) ; raw",
        "<?php function & /*comment*/ f(& /*comment*/ $x, &...$rest) { return $x; } $a & $b; $a & 1;",
        "<?php function f(){yield /*comment*/ from []; yield\nfrom []; yield fromx;}",
        "<?php enum A: string { case B = 'b'; } function enum() {} enum(); enum extends;",
        "<?php enum extendsFoo { case A; } enum implementsFoo { case B; }",
        "<?php enum /*comment*/ ExTeNdS_Foo { case A; } enum ImPlEmEnTs_Foo { case B; }",
        "<?php enum extendsé { case A; }", "<?php enum extends;", "<?php enum implements;",
        "<?php enum FooExtends { case A; } enum FooImplements { case B; }",
        "<?php enum XextendsFoo { case A; } enum XimplementsFoo { case B; }",
        "<?php function enum() {} enum();",
        "<?php $x = (int)1; (integer)$x; (bool)$x; (boolean)$x; (binary)$x; (double)$x; (real)$x; (float)$x; (array)$x; (object)$x; (unset)$x; (void)$x;",
        "<?php 0; 00; 0777; 0o77; 0xF; 0b11; 1_000; 1.5; .2; 2.; 1e3; 1E-2; 9223372036854775808; 0xffffffffffffffff;",
        "<?php echo __CLASS__, __TRAIT__, __METHOD__, __FUNCTION__, __PROPERTY__, __LINE__, __FILE__, __DIR__, __NAMESPACE__;",
        "<?php $s = 'single \\' quote'; $s = b'bytes'; $s = B\"bytes\"; $s = \"plain\";",
        "<?php $s = \"hello $name {$obj->class} ${name} $a[0] $a[foo] $a[$i]\";",
        "<?php $s = \"{$a['key']} {$a->{foo()}} ${foo[1]}\";",
        "<?php $s = `echo $name`;",
        r#"<?php $s = `\${}`;"#,
        r#"<?php $s = `\$foo`;"#,
        r#"<?php $s = `a\$b${x}`;"#,
        "<?php \"${123}\"; ${a}; \"${a[0]}\";",
        "<?php $s = <<<TXT\nhello $name\nTXT;\necho $s;",
        "<?php $s = <<<'TXT'\r\nhello $name\r\nTXT;\r\necho $s;",
        "<?php $s = <<<TXT\n    hello {$a->b}\n    TXT;",
        "<?php #[Attribute] class A { public private(set) string $x { get => $this->x; } }",
        "<?php $x = 1 |> strval(...); clone($x, []);",
        "<?php __halt_compiler();RAW<?php data", "<?php __halt_compiler /*c*/ ( ) ;\nRAW",
        "<?php __halt_compiler a b c RAW", "<?php function broken(", "<?php echo ;",
        "<?php 'unfinished", "<?php \"unfinished", "<?php /*unfinished", "<?php /** unfinished",
        "<?php 09; 0x; 0b; 1e;", "<?php }", "<?php $a = [1,;", "<?php \"{$a[\"b\"]}\";",
    ].into_iter().map(|s| s.as_bytes().to_vec()).collect::<Vec<_>>();
    sources.push(b"<?php // a\r\nb;".to_vec());
    sources.push(b"<?php # a\rb;".to_vec());
    sources.push(b"<?php $s = <<<TXT\nabc\\\nTXT;\necho 'ok';".to_vec());
    sources.push(b"<?php $s = <<<TXT\rabc\rTXT;\r".to_vec());
    sources.push(b"<?php <<<TXT".to_vec());
    sources.push(b"<?php <<<TXT foo\nTXT;".to_vec());
    sources.push(b"<?php <<<0TXT\nbody\n0TXT;".to_vec());
    sources.push(b"<?php <<<TXT\nbody\nTXT".to_vec());
    sources.push(b"<?php b<<<TXT\nbody\nTXT;".to_vec());
    sources.push(b"<?php b<<<'TXT'\nbody\nTXT;".to_vec());
    sources.push(b"<?php $s = \"$a[-1]\";".to_vec());
    sources.push(b"<?php $s = \"$a[1.5]\";".to_vec());
    sources.push(b"<?php $s = \"$a[\"b\"]\";".to_vec());
    sources.push(b"<?php $s = \"$a['x']\";".to_vec());
    sources.push(b"<?php $s = \"$a[]\";".to_vec());
    sources.push(b"<?php $s = \"$a?->b\";".to_vec());
    sources.push(b"<?php $s = \"$a[foo->bar]\";".to_vec());
    sources.push(b"<?php $s = \"$a[0x1]\";".to_vec());
    sources.push(b"<?php $s = \"{$f(#[A] fn() => 1)}\";".to_vec());
    sources.push(b"<?php public (set) $x;".to_vec());
    sources.push(b"<?php private(set\t) $x;".to_vec());
    sources.push(b"<?php foo(&$a);".to_vec());
    sources.push(b"<?php $s = `echo {$a +}`;".to_vec());
    sources.push(b"<?php goto a; a: echo 1;".to_vec());
    sources.push(b"<?php $$y = 1;".to_vec());
    sources.push(b"<?php echo ${$b};".to_vec());
    sources.push(b"<?php static $s = 1, $t;".to_vec());
    sources.push(b"<?php A::{'B'}::c();".to_vec());
    sources.push(b"<?php echo $a[]; $a[] ??= 1; foo()[] = 1; $a[]--; $a[]++; $a[]->b[];".to_vec());
    sources.push(b"<?php X::goto; foo(goto: 1); class E { const goto = 1; } $o->goto;".to_vec());
    sources.push(b"<?php abstract class C { abstract public $a { get; } } if (1) lab: echo 1;".to_vec());
    sources.push(b"<?php { echo 1; } foo()[0]++; ++foo(); --foo(); $x = foo()++;".to_vec());
    sources.push(b"<?php list() = []; [] = []; function f(A|(B&C) $x) {} [$p, &$q] = $arr;".to_vec());
    sources.push(b"<?php static fn($x) => $x; static function () {}; #[Attr(1)] const X = 1;".to_vec());
    sources.push(b"<?php new ($cls)();".to_vec());
    sources.push(b"<?php new ($cls)()();".to_vec());
    sources.push(b"<?php $a?->b::C;".to_vec());
    sources.push(b"<?php $a::$b::$c;".to_vec());
    sources.push(b"<?php A::B::C;".to_vec());
    sources.push(b"<?php $a->b::C;".to_vec());
    sources.push(b"<?php function f((A&B)|null $x) {}".to_vec());
    sources.push(b"<?php class C { public abstract $y { get; } }".to_vec());
    sources.push(b"<?php $f(...);".to_vec());
    sources.push(b"<?php echo \"\xff\"; echo ;".to_vec());
    sources.push(b"<?php \x0b\x0c\t echo 1;".to_vec());
    sources.push(b"<?php \0\x01\x7f; $\xff = '\xfe\0'; ?>\xff".to_vec());
    sources.push(b"<?php function \xffor() {}".to_vec());
    sources.push("<?php $città = 'è'; echo $città;".as_bytes().to_vec());

    // Four-model review corpus: PHP 8.5 grammar TOKEN_PARSE accepts even when its compiler
    // rejects the construct later, plus the elephc-dialect and accept-invalid negatives.
    for source in [
        "<?php $x ? $a = 1 : 0;",
        "<?php $x ? $a = 1 : $b = 2;",
        "<?php new $this->_class();",
        "<?php new $a->b();",
        "<?php new $a['x']();",
        "<?php new $a[0]();",
        "<?php new A::$a();",
        "<?php new static::$a();",
        "<?php new $a::B();",
        "<?php new (foo())[0];",
        "<?php if (list($a, $b) = explode(' ', $x)) {}",
        "<?php while ([$k, $v] = f()) {}",
        "<?php $x = [$a, $b] = [1, 2];",
        "<?php f([$a] = $b);",
        "<?php fn() => [$a] = $b;",
        "<?php @list($a, $b) = f();",
        "<?php @[$a, $b] = f();",
        "<?php foreach ($xs as foo()) {} foreach ($xs as foo()->bar) {} foreach ($xs as $a[0]) {} foreach ($xs as $$a) {} foreach ($xs as $a::$b) {} foreach ($xs as $a?->b) {} foreach ($xs as &$a[0]) {} foreach ($xs as $k => &$v) {}",
        "<?php foreach ($xs as ($targets)[0]) {} foreach ($xs as ($a)->b) {} foreach ($xs as (foo())[0]) {} foreach ($xs as ($a + $b)[0]) {}",
        "<?php foreach ($x as $a[]) {}",
        "<?php foreach ($x as $a->b) {}",
        "<?php foreach ($x as $a[0]) {}",
        "<?php foreach ($x as A::$b) {}",
        "<?php foreach ($x as f()) {}",
        "<?php function __construct(final public int $v = 1) {}",
        "<?php interface I { public $x { get; set; } }",
        "<?php class C { public $x { get; set; } }",
        "<?php class C { public $x { final get => 1; } }",
        "<?php class C { public $x { get => 1; final set => 1; } }",
        "<?php class C { public $x { #[A] get => 1; } }",
        "<?php class C { public static $x { get => 1; } }",
        "<?php class C { public $x = 1 { get => 1; } }",
        "<?php class C { public $x { foo => 1; } }",
        "<?php new #[A] class {};",
        "<?php new #[A] readonly class {};",
        "<?php #[A] fn() => 1;",
        "<?php #[A] function() {};",
        "<?php #[A] static fn() => 1;",
        "<?php #[A] const X = 1;",
        "<?php #[A] const X = 1, Y = 2;",
        "<?php class C { #[A] case X; }",
        "<?php function f(#[A] public $x) {}",
        "<?php $d |> fn($x) => $x;",
        "<?php $b |> static fn($x) => $x;",
        "<?php $b |> include 'x';",
        "<?php $x = \\array; \\array(1, 2); namespace\\array;",
        "<?php class C { public (A&B)|D $value; }",
        "<?php class C { public $x; public $x; const A = 1; const A = 2; }",
        "<?php class C { public int $a = 1, $a = 2; }",
        "<?php class C { public function __construct(public $x, public $x) {} }",
        "<?php class C { public $x { get => 1; get => 2; } }",
        "<?php unset($a); if ($a) unset($b); while (false) unset($c);",
        "<?php \\unset($a); namespace\\unset($b);",
        "<?php new A()::X;",
        "<?php new A()::$x;",
        "<?php $a()::b;",
        "<?php $a[0]::b;",
        "<?php \"A\"::b;",
        "<?php (A)::b;",
        "<?php f()::$b;",
        "<?php [A::class, \"b\"]();",
        "<?php X::class();",
        "<?php class E { const class = 7; }",
        "<?php enum E { case class; }",
        "<?php class C { function new() {} }",
        "<?php class C { function class() {} }",
        "<?php static fn() => 1;",
        "<?php static function() {};",
        "<?php [,];",
        "<?php [1,,2];",
        "<?php [1, 2,,3];",
        "<?php [1, 2] = $a;",
        "<?php [...$a] = $b;",
        "<?php list(1) = $a;",
        "<?php function f(int $x, ...$y, $z) {}",
        "<?php function f(public $x) {}",
        "<?php enum E { public $x; }",
        "<?php interface I { public $x; }",
        "<?php class C { static readonly $x; }",
        "<?php class C { var $x; }",
        "<?php class C { private(set) $x; }",
        "<?php class C { public private(set) $x; }",
        "<?php switch (1) { default: default: }",
        "<?php break 0;",
        "<?php break $a;",
        "<?php try {}",
        "<?php declare(foo=1+1);",
        "<?php namespace namespace;",
        "<?php for ($i = 0, $j = 0; $i < 1, $j < 2; $i++, $j++) {}",
        "<?php function f(A|(B&C) $x) {}",
        "<?php function f((A&B)|C $x) {}",
        "<?php function f(A&(B|C) $x) {}",
        "<?php function f(C|(A&B) $x) {}",
        "<?php function f(A &$x) {}",
        "<?php function f(buffer $value) {}",
        "<?php class C { public buffer $value; }",
        "<?php $a = [&$x, &$y];",
        "<?php $a = [&$x][0];",
        "<?php $a = [&foo()];",
        "<?php [\"k\" => &$x];",
        "<?php $a = [0 => &$x[1]]; array(\"k\" => &$y[0]);",
        "<?php $a = [&readonly()]; $b = array(&readonly());",
        "<?php $r = &readonly()->p; $s = &readonly()(); $t = &readonly()::C[0];",
        "<?php $r = &($get)()[$key]; $s = &($a)[0]; $t = &($a)->p; $u = &($a)();",
        "<?php $a = [&($b)[0]]; $b = array(&($g)());",
        r#"<?php $r = &($a)::FOO[0]; $s = &($a)::$c; $t = &($a)::{"m"}(); $u = &($a)?->p;"#,
        "<?php $r = &($a)->p + 2; $a = ['key' => &($a)::FOO->p];",
        "<?php $o->$method(value: 1);",
        "<?php C::$method(value: 1);",
        "<?php $a[&$b];",
        "<?php function f(callable(int): string $cb) {}",
        "<?php function f(buffer<int> $value) {}",
        "<?php [$a, &$b] = $arr;",
        "<?php foreach ($arr as [$k, &$v]) {}",
        "<?php $a = array(&$x, &$y);",
        "<?php array(\"k\" => &$x); [1 => &$y];",
        "<?php $s = `{$a[\"hello world\"]}`; $t = `{$a['x']}`;",
        "<?php $s = `{$f(`whoami`)}`;",
        "<?php buffer_new < 1;",
        "<?php class C { function empty() {} const isset = 1; public $list; }",
        "<?php new #[A] class {}; new readonly class {}; new #[A] readonly class {};",
        "<?php $x = <<<A\n    {$a[\n1]}\n    A;",
        "<?php switch ($a): ?>\n<?php case 1: ?>x<?php endswitch;",
        "<?php switch ($a) { ; case 1: }",
        "<?php function f($x = $$a) {} (void)$x; (unset)$x;",
        "<?php function f($x = [1, $$a]) {} function g($x = foo($$a)) {}",
        "<?php function h($x = foo(1, 2) . $$a) {} function i($x = bar(1, 2) . [$$a]) {}",
        "<?php function j($x = foo(1, 2) + [1, $$a]) {}",
        "<?php switch ($x) { case $a ? 1 : 2: break; }",
        "<?php packed(); class extern {} function packed() {} ifdef(); class ifdef {} extern();",
        "<?php ++$a[]; --$a[]; ++$a[][];",
        "<?php \"abc\"[0] = \"x\"; (1 + 2)[0] = 1; $o?->p = 1; [1] = 2;",
        "<?php interface I { protected $x { get; } }",
        "<?php interface I { public $x { get; } public $x { get; } }",
        "<?php interface I { abstract public $x { get; } final public $y { get; } }",
        "<?php for (include 'a.php'; false; ) {}",
        "<?php for ($i = 0; include 'a.php'; $i++) {}",
        "<?php try {} finally {}",
        "<?php echo 'strlen'('hello'); $f = 'trim';",
        r#"<?php [0 => 'strlen']('hello'); ['k' => 'trim', 0 => 'strlen']('hello');"#,
        r#"<?php $s = `{$$name}`; $t = "{$$name}";"#,
        r#"<?php $s = `{$a[$$b]}`; $t = `${$name}`; $u = `${1+1}`;"#,
        r#"<?php $s = `${"x"}`; $t = `{${$name}}`; $u = `{$a[${"x"}]}`;"#,
        r#"<?php $s = `$$name`; $t = `$a->b`; $u = `$a[0]`; $v = `{$a?->b}`;"#,
        r#"<?php [0 => "strlen"]("x"); [[0 => "strlen"][0]]("x"); (([0 => "strlen"]))("x"); ([0 => "strlen"])("x");"#,
        r#"<?php ("strlen")("x"); "strlen"("x"); [0 => "strlen"](...);"#,
        r#"<?php f(["key" => $value] = $row); ["a" => $x, 0 => $y] = $row;"#,
        r#"<?php interface I { function f(); } function readonly() {}"#,
        r#"<?php foreach ($arr as $k => &$v) {} list("k" => &$v) = $arr;"#,
        r#"<?php do ; while (true); do 5; while ($a);"#,
        r#"<?php function f() { yield &$x; } abstract class C { abstract function m(public int $x); }"#,
        r#"<?php 1 <=> 2 < 3;"#,
        r#"<?php PHP_INT_MAX[0]; INF[0]; M_PI[0]; -PHP_INT_MAX[0];"#,
        r#"<?php 1.e3; 1.5e3; .5e3;"#,
        r#"<?php $o?->make()(); $o?->$m()();"#,
        r#"<?php ++$a[]; --$a[]; $a[]++; $a[] = 1;"#,
        r#"<?php function &f($a = $b) {} function &readonly($a = $b) {} class C { function &readonly($a = $b) {} } class D { function match() {} function &readonly() {} }"#,
        r#"<?php if ($a) done: else other:"#,
        "<?php if (1) label:; while (true) other:; for (;;) third:; foreach ([] as $x) fourth:; declare(strict_types=1) fifth:;",
        "<?php class C { public static function __construct(public int $x) {} }",
        "<?php function f($this) {} class C { function m($this) {} }",
        "<?php function f(int $this) {} function g(int &...$this) {} class C { function m(int $this) {} function n(int &...$this) {} }",
        "<?php function($this) {}; static fn($this) => 1; fn(int &...$this) => 1;",
        "<?php ($o->p++)[0]; ($o->p++)->q; $o->p++ + 1;",
        r#"<?php function f(\empty $x) {} function g(Foo\empty $x) {}"#,
        r#"<?php $s = `{$$a}`; $t = `{$$$a}`; $u = `{$$a[0]}`; $v = `{$a[$$b]}`; $w = `{$a[${"k"}]}`;"#,
        r#"<?php $s = `{${$a}}`; $t = `{${$a[0]}}`; $u = `{${$a}["x"]}`; $v = `{$a->{$$b}}`; $w = `{$a?->{$$b}}`;"#,
        r#"<?php $s = `${$a}`; $t = `${$$a}`; $u = `${$a[0]}`; $v = `${$a->b}`; $w = `${"x"}`; $x = `${1+$a}`;"#,
        r#"<?php $s = `\${$a}`; $t = `\{$a}`;"#,
        "<?php try {} finally ;",
        "<?php new $a[0]; new $a->b(); new $a::$b; new self::$b; new A::$b;",
        "<?php new $a::$b();",
        "<?php f(a: 1, 2); f(...$args, 1);",
        "<?php $f = function(public int $x) {}; $g = fn(protected(set) int $x) => 1;",
        "<?php function f(public (set) int $x) {}",
        "<?php function f(final(set) int $x) {}",
        "<?php ptr_cast<int>($p);",
        "<?php buffer_new<int>(8);",
        "<?php new #[A] Foo();",
        "<?php new $a[0]::B();",
        "<?php new $a->b::C();",
        "<?php new $a::B();",
        "<?php $a = (void)foo();",
        "<?php switch ($a): ?>\n    <?php case 1: ?>x<?php endswitch;",
        "<?php class A { foo: }",
        "<?php function empty() {}",
        "<?php class isset {}",
        "<?php interface list {}",
        "<?php trait empty {}",
        "<?php enum isset {}",
        "<?php const empty = 1;",
        "<?php function echo() {}",
        "<?php class match {}",
        "<?php function f(public (set) int $x) {}",
        "<?php class C { function m(final(set) int $x) {} }",
        "<?php $x = new $classes[0]()->p = 1;",
        "<?php $x = new $a->b()->p = 1;",
        "<?php $x = new self::$b()->p = 1; new A::$b()->p = 1;",
        "<?php new $a[0] = 1;",
        "<?php new $a->b = 1;",
        "<?php new A::$b = 1;",
        "<?php new self::$b = 1;",
        "<?php new static->p = 1;",
        "<?php function f(...) {}",
        "<?php ptr_cast<int>($p);",
        "<?php $f = fn(...) => 1;",
        "<?php function &f(...) {}",
        "<?php function f(...) {}",
        "<?php $f = fn(...) => 1;",
        "<?php function &f(...) {}",
        "<?php $a = [\"k\" => &$x, \"j\" => &$y];",
        "<?php array(\"k\" => &$x); [1 => &$y];",
        "<?php $s = `{$a[\"hello world\"]}`; $t = `{$a['x']}`;",
        "<?php $s = `{$f(`whoami`)}`;",
        "<?php buffer_new < 1;",
        "<?php class C { function empty() {} const isset = 1; public $list; }",
        "<?php new #[A] class {}; new readonly class {}; new #[A] readonly class {};",
        "<?php $x = <<<A\n    {$a[\n1]}\n    A;",
        "<?php switch ($a): ?>\n<?php case 1: ?>x<?php endswitch;",
        "<?php switch ($a) { ; case 1: }",
        "<?php function f($x = $$a) {} (void)$x; (unset)$x;",
        "<?php switch ($x) { case $a ? 1 : 2: break; }",
        "<?php static $$a;",
        "<?php function f($$a) {}",
        "<?php class A { public $$a; }",
        "<?php try {} catch (E $$e) {}",
        "<?php $f = function() use ($$a) {};",
        "<?php $a = (void)foo();",
        "<?php function f($a = &$b) {}",
        "<?php class A { foo: }",
        "<?php switch ($a): ?>\n    <?php case 1: ?>x<?php endswitch;",
        "<?php new #[A] Foo();",
        "<?php new $a[0]::B();",
        "<?php new $a->b::C();",
        "<?php new $a::B();",
        "<?php function empty() {}",
        "<?php class isset {}",
        "<?php interface list {}",
        "<?php trait empty {}",
        "<?php enum isset {}",
        "<?php const empty = 1;",
        "<?php function echo() {}",
        "<?php class match {}",
        "<?php buffer_new<int>(8);",
        "<?php f(&$a);",
        "<?php $a[&$b] = 1;",
        "<?php $a[&$b][0];",
        "<?php strlen(...); $a[0](...); (new Foo)(...);",
        "<?php function f() { return $x; } function &g() { return $x; }",
        "<?php function f() { return &$x; }",
        "<?php function &f() { return &$x; }",
        "<?php $f = fn() => &$x;",
        "<?php [\"key\" => &1];",
        "<?php match(1) { 1 => &$x };",
        "<?php foreach ($xs as 1) {}",
        "<?php foreach ($xs as $a + $b) {}",
        "<?php foreach ($xs as $a ?? $b) {}",
        "<?php foreach ($xs as $k => 1) {}",
        "<?php foreach ($xs as ($a)) {}",
        "<?php foreach ($xs as array($a)) {}",
        "<?php $a = [0 => &$x + 1];",
        "<?php $a = ['k' => &$x + 1];",
        "<?php array(0 => &$x + 1);",
        "<?php new $a[0] = 1;",
        "<?php new $a->b = 1;",
        "<?php new A::$b = 1;",
        "<?php new self::$b = 1;",
        "<?php new static->p = 1;",
        "<?php $cls::MATCH; (foo())::MATCH; $cls::LIST; $o::ENUM;",
        "<?php class C { function m(public int $x) {} }",
        "<?php class C { function m(...$x, $y) {} }",
        "<?php class C { function __construct(public ...$x) {} }",
        "<?php class C { function __construct(public readonly &$x) {} }",
        "<?php $f = function(...$x, $y) {}; $g = fn(...$x, $y) => 1;",
        "<?php f($a...);",
        "<?php f([]...);",
        "<?php isset($a); empty($a); unset($a);",
        "<?php exit; exit(1); die('x');",
        "<?php class C { use T { T::c insteadof D; } }",
        "<?php use A\\{B, C}; use function A\\{b}; use const A\\{B};",
        "<?php if (1) { ?> html <?php } else { ?> other <?php }",
        "<?php if (1): ?> html <?php else: ?> other <?php endif;",
        "<?php __halt_compiler();",
        "<?php namespace X; __halt_compiler();",
        "<?php (double)$x; (unset)$x; (void)$x;",
        "<?php $x = unset($a);",
        "<?php echo unset($a);",
        "<?php echo (1) label:;",
        "<?php $o->p++[0];",
        "<?php $r = &($a);",
        "<?php $o->p++->q;",
        "<?php $o->p++();",
        "<?php list(...$a $b) = $row;",
        "<?php [...$a $b] = $row;",
        "<?php foo() label:;",
        "<?php switch (1) label:;",
        "<?php try {} catch (Exception $e) label:;",
        "<?php @unset($a);",
        "<?php unset($a) or die('x');",
        // PHP rejects these in TOKEN_PARSE; the engine must reject them too.
        "<?php 1 + 2 = 3;",
        "<?php ($a) = 1;",
        "<?php ($a)++;",
        "<?php ++$a = 1;",
        "<?php $a = &new C;",
        "<?php $a = &1;",
        "<?php $a::class = 1;",
        "<?php $a == $b == $c;",
        "<?php $a < $b < $c;",
        "<?php $a <=> $b <=> $c;",
        "<?php $a <= $b >= $c;",
        "<?php isset();",
        "<?php empty();",
        "<?php unset();",
        "<?php exit->foo;",
        "<?php class C { $x; }",
        "<?php class C { int $x; }",
        "<?php class C { public (set) $x; }",
        "<?php function f(A&B|C $x) {}",
        "<?php function f(C|A&B $x) {}",
        "<?php try {} catch (A&B $e) {}",
        "<?php try {} catch (A $e);",
        "<?php try ; catch (A $e) {}",
        "<?php use A\\{};",
        "<?php use function A\\{function b};",
        "<?php function f() { use A; }",
        "<?php function f() { const A = 1; }",
        "<?php if (1) { const A = 1; }",
        "<?php class C { use T { c insteadof D; } }",
        "<?php if (1) ?> html <?php else ?> other <?php ;",
        "<?php if (1) __halt_compiler();",
        "<?php function f() { __halt_compiler(); }",
        "<?php extern \"System\" { fn puts(s: ptr<u8>) -> i32; }",
        "<?php packed class P { public int $x; }",
        "<?php function f(array<int> $x) {}",
        "<?php final class B<T> {}",
        "<?php int $index = 1;",
        "<?php PDORow $typedLazy = $lazy;",
        "<?php $x = new Box<int>();",
        "<?php class C extends B<int> {}",
        "<?php $box->map<string>($f);",
    ] {
        sources.push(source.as_bytes().to_vec());
    }
    // Lexer boundary: Zend stops the T_NUM_STRING offset at the numeric prefix.
    for source in [
        "<?php \"$a[1e3]\";",
        "<?php \"$a[1abc]\";",
        "<?php \"$a[0b2]\";",
        "<?php \"$a[0xg]\";",
        "<?php \"$a[1_]\";",
        "<?php \"$a[1.5e3]\";",
        "<?php \"$a[1_000]\";",
        "<?php \"$a[0x1A]\";",
        "<?php \"$a[0b10]\";",
        "<?php \"$a[0o17]\";",
        "<?php \"$a[12abc]\";",
    ] {
        sources.push(source.as_bytes().to_vec());
    }
    // ST_VAR_OFFSET fallback: an unexpected byte emits an empty T_ENCAPSED_AND_WHITESPACE
    // and resumes the normal string body; control bytes are T_BAD_CHARACTER.
    sources.push(b"<?php \"$a[\\1]\";".to_vec());
    sources.push(b"<?php \"$a[1\n]\";".to_vec());
    sources.push(b"<?php \"$a[\\0]\";".to_vec());
    sources.push(b"<?php \"$a[\x00]\";".to_vec());
    sources.push(b"<?php \"$a[\x01]\";".to_vec());
    sources.push(b"<?php \"$a[1\x00]\";".to_vec());
    sources.push(b"<?php \"$a[\\1]x\";".to_vec());
    sources.push(b"<?php \"$a[ ]\";".to_vec());
    // Heredoc indentation inspects only the first `indent.len()` bytes of each body line.
    for source in [
        b"<?php $s = <<<EOT\n  abc\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n  \tabc\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n\t  abc\n\tEOT;".as_slice(),
        b"<?php $s = <<<EOT\n\t\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n abc\n\tEOT;".as_slice(),
        b"<?php $s = <<<EOT\n abc\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n\n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n   \n  EOT;".as_slice(),
        b"<?php $s = <<<EOT\n \n  EOT;".as_slice(),
    ] {
        sources.push(source.to_vec());
    }
    sources
}

/// Encodes source bytes for an ASCII-only cross-process oracle protocol.
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

/// php-src tokenizer cases: every half-reserved name and trait adaptation reclassifies.
#[test]
fn tokenizer_php_src_reserved_name_feedback() {
    let source = b"<?php\nX::continue;\nX::$continue;\n$x->$continue;\nX::continue();\n$x->continue();\nX::class;\n$x->__halt_compiler();\nclass X {\n    const CONTINUE = 1;\n    const ARRAY = [1, 2];\n}\nclass C {\n    use A {\n        namespace as bar;\n    }\n}\n";
    let tokens = tokenize(source, TOKEN_PARSE).expect("php-src semi-reserved source is valid PHP");
    for name in [b"continue".as_slice(), b"class", b"__halt_compiler", b"CONTINUE", b"ARRAY", b"namespace", b"bar"] {
        let token = tokens.iter().find(|token| &source[token.pos..token.pos + token.len] == name)
            .unwrap_or_else(|| panic!("missing token {}", String::from_utf8_lossy(name)));
        assert_eq!(token.id, id("T_STRING"), "{}", String::from_utf8_lossy(name));
    }
}

/// Keyword names parsed inside blanked `${...}` and `::{...}` regions still reclassify.
#[test]
fn tokenizer_php_src_reserved_name_feedback_in_blanked_expressions() {
    let source = b"<?php\necho C::{D::MATCH};\n$s = \"${D::MATCH}\";\n";
    let tokens = tokenize(source, TOKEN_PARSE).expect("keyword member names are valid PHP");
    let names = tokens
        .iter()
        .filter(|token| &source[token.pos..token.pos + token.len] == b"MATCH")
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 2);
    assert!(
        names.iter().all(|token| token.id == id("T_STRING")),
        "{:?}",
        names.iter().map(|token| token.id).collect::<Vec<_>>()
    );

    // Many names on one minified line resolve through the same forward pass.
    let minified = b"<?php C::MATCH;C::MATCH;C::MATCH;C::MATCH;C::MATCH;";
    let tokens = tokenize(minified, TOKEN_PARSE).expect("minified member names are valid PHP");
    let names = tokens
        .iter()
        .filter(|token| &minified[token.pos..token.pos + token.len] == b"MATCH")
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 5);
    assert!(names.iter().all(|token| token.id == id("T_STRING")));

    // A member/named `__halt_compiler` must not switch the scanner into halt mode.
    let source = b"<?php $o->__halt_compiler; echo 1;";
    let tokens = tokenize(source, TOKEN_PARSE).expect("`->__halt_compiler` is a valid member name");
    assert!(tokens.iter().any(|token| {
        &source[token.pos..token.pos + token.len] == b"echo" && token.id == id("T_ECHO")
    }));
}

/// php-src tokenizer heredoc cases keep PHP's exact TOKEN_PARSE diagnostics and lines.
#[test]
fn tokenizer_php_src_heredoc_parse_errors() {
    for (source, message, line) in [
        (b"<?php <<<INNER_END\na".as_slice(), "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"", 2),
        (b"<?php <<<INNER_END\nab".as_slice(), "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"", 2),
        (b"<?php\n \t<<<'DOC'\n \tXXX\n \tDOC;".as_slice(), "Invalid indentation - tabs and spaces cannot be mixed", 3),
        (b"<?php <<<INNER_END\nabc\n   INNER_END;".as_slice(), "Invalid body indentation level (expecting an indentation level of at least 3)", 2),
        (b"<?php <<<INNER_END\nab\nab\n\t".as_slice(), "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"", 4),
    ] {
        let error = tokenize(source, TOKEN_PARSE).expect_err("malformed heredoc must fail with TOKEN_PARSE");
        assert_eq!(error.message, message, "{}", String::from_utf8_lossy(source));
        assert_eq!(error.line, line, "{}", String::from_utf8_lossy(source));
    }
    assert!(tokenize(b"<?php <<<INNER_END\n\n   INNER_END;", TOKEN_PARSE).is_ok());
}

/// An unclosed delimiter highlights its opener but parsing stopped at EOF, so warnings
/// reached before that stop must still be reported.
#[test]
fn tokenizer_unclosed_delimiter_keeps_earlier_warnings() {
    let error = tokenize_with_warnings(b"<?php function f() { (integer)$x;", TOKEN_PARSE)
        .expect_err("unclosed block");
    assert_eq!(error.message, "Unclosed '{'");
    assert_eq!(error.warnings.len(), 1, "{:?}", error.warnings);
    assert_eq!(
        error.warnings[0].message,
        "Non-canonical cast (integer) is deprecated, use the (int) cast instead"
    );
    assert_eq!(error.warnings[0].severity, 8_192);
}

/// php-src tokenizer invalid escapes report PHP's parser-facing messages.
#[test]
fn tokenizer_php_src_unicode_escape_errors() {
    for (source, message) in [
        (r#"<?php var_dump("\u{xyz}");"#, "Invalid UTF-8 codepoint escape sequence"),
        (r#"<?php var_dump("\u{ffffff}");"#, "Invalid UTF-8 codepoint escape sequence: Codepoint too large"),
        (r#"<?php var_dump("\u{110000}");"#, "Invalid UTF-8 codepoint escape sequence: Codepoint too large"),
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err("invalid escape must fail");
        assert_eq!(error.message, message);
    }
}

/// Non-canonical casts surface PHP's deprecations only under TOKEN_PARSE.
#[test]
fn tokenizer_php_src_non_canonical_cast_warnings() {
    let output = tokenize_with_warnings(b"<?php (double) $x; (integer) $x; (boolean) $x; (binary) $x;", TOKEN_PARSE)
        .expect("non-canonical casts are valid syntax");
    assert_eq!(output.warnings.len(), 4);
    assert_eq!(output.warnings[0].message, "Non-canonical cast (double) is deprecated, use the (float) cast instead");
    assert_eq!(output.warnings[1].message, "Non-canonical cast (integer) is deprecated, use the (int) cast instead");
    assert_eq!(output.warnings[2].message, "Non-canonical cast (boolean) is deprecated, use the (bool) cast instead");
    assert_eq!(output.warnings[3].message, "Non-canonical cast (binary) is deprecated, use the (string) cast instead");
    // Zend raises each non-canonical cast through zend_error(E_DEPRECATED, ...).
    assert!(output.warnings.iter().all(|warning| warning.severity == 8_192));
    assert!(tokenize_with_warnings(b"<?php (double) $x;", 0).expect("plain scanning succeeds").warnings.is_empty());
}

/// Nested shell strings are rewritten recursively, so the depth must stay bounded.
///
/// PHP parses nested backticks at any depth; the validation view stops rewriting past
/// `MAX_SHELL_NESTING` and reports a catchable `ParseError` instead of exhausting the stack.
#[test]
fn tokenizer_deep_shell_nesting_is_bounded() {
    let nested = |depth: usize| {
        let mut source = String::from("<?php $s = ");
        for _ in 0..depth {
            source.push_str("`{$f(");
        }
        source.push_str("`whoami`");
        for _ in 0..depth {
            source.push_str(")}`");
        }
        source.push(';');
        source
    };
    assert!(tokenize(nested(40).as_bytes(), TOKEN_PARSE).is_ok());
    // Pathological input must return instead of aborting the process.
    let _ = tokenize(nested(2_000).as_bytes(), TOKEN_PARSE);
}

/// PHP warns about octal escapes above one byte, with or without TOKEN_PARSE.
#[test]
fn tokenizer_php85_octal_escape_overflow_warning() {
    for flags in [0, TOKEN_PARSE] {
        let output = tokenize_with_warnings(b"<?php\n$s = \"\\400\";", flags)
            .expect("octal escapes are valid syntax");
        assert_eq!(output.warnings.len(), 1, "flags={flags}");
        assert_eq!(
            output.warnings[0].message,
            "Octal escape sequence overflow \\400 is greater than \\377"
        );
        assert_eq!(output.warnings[0].severity, 2);
        assert_eq!(output.warnings[0].line, 2);
    }
    // `\377` fits one byte and stays quiet.
    let output = tokenize_with_warnings(b"<?php $s = \"\\377\";", TOKEN_PARSE).expect("valid");
    assert!(output.warnings.is_empty());
}

/// Warning lines come from one forward pass, so every deprecation reports its own line.
///
/// Recomputing the line from the start of the source for each warning made a file full of
/// deprecated casts quadratic; this pins the resulting line numbers, including a CRLF break.
#[test]
fn tokenizer_warning_lines_follow_the_source() {
    let output = tokenize_with_warnings(
        b"<?php\n(double) $a;\n\n(integer) $b;\r\n(boolean) $c;",
        TOKEN_PARSE,
    )
    .expect("non-canonical casts are valid syntax");
    let lines: Vec<usize> = output.warnings.iter().map(|warning| warning.line).collect();
    assert_eq!(lines, vec![2, 4, 5]);
}

/// A malformed `declare()` list never reports the encoding warning: Zend has not compiled
/// the statement when parsing fails.
#[test]
fn tokenizer_declare_encoding_warning_requires_a_complete_list() {
    for source in [
        b"<?php declare(encoding='ISO-8859-1';".as_slice(),
        b"<?php declare(encoding=);",
        b"<?php declare(encoding='ISO-8859-1',);",
        b"<?php declare(encoding='ISO-8859-1', ticks=);",
        b"<?php declare(encoding='ISO-8859-1', ticks);",
    ] {
        let error = tokenize_with_warnings(source, TOKEN_PARSE).expect_err("invalid declare list");
        assert!(error.message.contains("syntax error"), "{}", String::from_utf8_lossy(source));
        assert!(
            error.warnings.is_empty(),
            "{:?} for {}",
            error.warnings,
            String::from_utf8_lossy(source)
        );
    }
}

/// Zend reports the encoding warning at the closing `)`, which decides the printed line.
#[test]
fn tokenizer_declare_encoding_warning_uses_closing_line() {
    let output = tokenize_with_warnings(
        b"<?php\ndeclare(\n encoding='ISO-8859-1'\n);\necho 1;",
        TOKEN_PARSE,
    )
    .expect("complete declare list");
    assert_eq!(output.warnings.len(), 1);
    assert_eq!(output.warnings[0].line, 4);
}

/// Warnings the validator raised before a parse failure survive the `ParseError` result.
#[test]
fn tokenizer_declare_encoding_warning_before_parse_error() {
    let error = tokenize_with_warnings(
        b"<?php\n\ndeclare(encoding='ISO-8859-1'); echo ;",
        TOKEN_PARSE,
    )
    .expect_err("echo ; is a syntax error");
    assert!(error.message.contains("syntax error"), "{}", error.message);
    assert_eq!(error.warnings.len(), 1, "{:?}", error.warnings);
    assert_eq!(error.warnings[0].severity, 2);
    assert_eq!(error.warnings[0].line, 3);
}

/// `declare(encoding=...)` warns exactly as Zend does when multibyte support is off.
#[test]
fn tokenizer_declare_encoding_warning() {
    let output = tokenize_with_warnings(b"<?php declare(encoding='ISO-8859-1');", TOKEN_PARSE)
        .expect("declare(encoding) is valid syntax");
    assert_eq!(output.warnings.len(), 1);
    assert_eq!(
        output.warnings[0].message,
        "declare(encoding=...) ignored because Zend multibyte feature is turned off by settings"
    );
    // php_error_docref(NULL, E_WARNING, ...), NOT a deprecation: handlers and reporting masks
    // must see E_WARNING here.
    assert_eq!(output.warnings[0].severity, 2);
    assert!(tokenize_with_warnings(b"<?php declare(strict_types=1);", TOKEN_PARSE)
        .expect("strict_types is valid syntax")
        .warnings
        .is_empty());
}

/// Nested `__halt_compiler()` matches PHP's CompileError text, line and namespace unclosed brace.
#[test]
fn tokenizer_halt_compiler_scope_matches_php_diagnostics() {
    let compile = "__HALT_COMPILER() can only be used from the outermost scope";
    for source in [
        "<?php function f() { __halt_compiler(); }",
        "<?php class C { function m() { __halt_compiler(); } }",
        "<?php if (1) { __halt_compiler(); }",
        "<?php if (1): __halt_compiler(); endif;",
        "<?php function f() { label: __halt_compiler(); }",
        "<?php namespace N { if (1): __halt_compiler(); endif; }",
        "<?php class C { public $x { get { __halt_compiler(); } } }",
        "<?php $f = function () use ($a) { __halt_compiler(); };",
        "<?php function f() { $s = \"{$a}\"; __halt_compiler(); }",
        "<?php function f() { class C { function m() { __halt_compiler(); } } }",
        "<?php function f() { class C { public $x { get { __halt_compiler(); } } } }",
        "<?php function f() { switch ($a) { case 1: __halt_compiler(); } }",
        "<?php switch ($a): case 1: __halt_compiler(); endswitch;",
        "<?php if (Foo::class) { __halt_compiler(); }",
        "<?php function f() { if (Foo::class) { __halt_compiler(); } }",
        "<?php function f() { if (1) {} __halt_compiler(); }",
        "<?php function f() { function g() {} __halt_compiler(); }",
        "<?php function f() { class A {} __halt_compiler(); }",
        "<?php function f() { label: function g() {} __halt_compiler(); }",
        "<?php function f() { __halt_compiler( ); }",
        "<?php function f() { __halt_compiler(/*c*/); }",
        "<?php function f() { switch ($a) { case 1: echo 1; } __halt_compiler(); }",
        "<?php function f($x) { switch ($x) { case 1: break; } if ($x) { __halt_compiler(); } }",
        "<?php switch (1) {} if (1) { __halt_compiler(); }",
        "<?php function f() { switch (1) {} { __halt_compiler(); } }",
        "<?php class A {} if (1) { __halt_compiler(); }",
        "<?php interface I {} { __halt_compiler(); }",
        "<?php enum E {} while (1) { __halt_compiler(); }",
        "<?php class A {} if (1) {} else { __halt_compiler(); }",
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err(source);
        assert_eq!(error.message, compile, "{source}");
        assert_eq!(error.line, 1, "{source}");
        assert!(error.is_compile_error(), "{source}");
    }
    let nested = tokenize(b"<?php\nfunction f() {\n__halt_compiler();\n}", TOKEN_PARSE)
        .expect_err("halt line follows the function brace");
    assert_eq!(nested.message, compile);
    assert_eq!(nested.line, 3);
    assert!(nested.is_compile_error());
    let quoted_brace = tokenize(
        b"<?php\nfunction f() {\n    echo \"}\";\n    __halt_compiler();\n}",
        TOKEN_PARSE,
    )
    .expect_err("a quoted brace does not close the function");
    assert_eq!(quoted_brace.message, compile);
    assert_eq!(quoted_brace.line, 4);
    assert!(quoted_brace.is_compile_error());
    let earlier = tokenize(
        b"<?php\necho ;\nfunction f() {\n__halt_compiler();\n}",
        TOKEN_PARSE,
    )
    .expect_err("an earlier syntax error stays ahead of the halt");
    assert_eq!(earlier.message, "syntax error, unexpected token \";\"");
    assert_eq!(earlier.line, 2);
    assert!(!earlier.is_compile_error());
    let inside = tokenize(
        b"<?php\nfunction f() {\necho ;\n__halt_compiler();\n}",
        TOKEN_PARSE,
    )
    .expect_err("a syntax error inside the function stays ahead of the halt");
    assert_eq!(inside.message, "syntax error, unexpected token \";\"");
    assert_eq!(inside.line, 3);
    assert!(!inside.is_compile_error());

    let same_line = tokenize(b"<?php namespace N { __halt_compiler(); }", TOKEN_PARSE)
        .expect_err("braced namespace halts before the closing brace");
    assert_eq!(same_line.message, "Unclosed '{'");
    assert_eq!(same_line.line, 1);
    assert!(!same_line.is_compile_error());

    let later_line = tokenize(b"<?php\nnamespace N {\n__halt_compiler();\n}", TOKEN_PARSE)
        .expect_err("the unclosed brace names its own line when the halt is later");
    assert_eq!(later_line.message, "Unclosed '{' on line 2");
    assert_eq!(later_line.line, 3);
    assert!(!later_line.is_compile_error());

    for source in [
        "<?php if (1) __halt_compiler();",
        "<?php function f() { if (1) __halt_compiler(); }",
        "<?php class C { __halt_compiler(); }",
        "<?php class C { public $x = __halt_compiler(); }",
        "<?php class C { use T { __halt_compiler(); } }",
        "<?php fn() => __halt_compiler();",
        "<?= __halt_compiler();",
        "<?php function f() { class C { __halt_compiler(); } }",
        "<?php function f() { class C { use T { __halt_compiler(); } } }",
        "<?php function f() { class C { public $x { __halt_compiler(); } } }",
        "<?php function f() { $x = match ($a) { __halt_compiler(); } }",
        "<?php function f() { C::{__halt_compiler()}; }",
        "<?php function f() { $o->{__halt_compiler()}; }",
        "<?php function f() { $o?->{__halt_compiler()}; }",
        "<?php function f() { switch ($a) { __halt_compiler(); } }",
        "<?php switch ($a): __halt_compiler(); endswitch;",
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err(source);
        assert!(!error.is_compile_error(), "{source}: {}", error.message);
        assert!(error.message.contains("__halt_compiler") || error.message.contains("__HALT_COMPILER"), "{source}: {}", error.message);
    }
    for (source, message) in [
        (
            "<?php function f() { $x = match ($a) { __halt_compiler(); } }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php function f() { C::{__halt_compiler()}; }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $o->{__halt_compiler()}; }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $o?->{__halt_compiler()}; }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { switch ($a) { __halt_compiler(); } }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"case\" or \"default\" or \"}\"",
        ),
        (
            "<?php switch ($a): __halt_compiler(); endswitch;",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"endswitch\" or \"case\" or \"default\"",
        ),
        (
            "<?php $x = match ($a) { 1 => 1, __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => 1 __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php function f() { $x = match ($a) { 1 => 1, __halt_compiler() }; }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1, __halt_compiler() => 2 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php $x = match ($a) { 1 __halt_compiler() => 2 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php $x = match ($a) { 1 => __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php $x = match ($a) { 1 => foo(__halt_compiler()) };",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = match ($a) { 1 => { __halt_compiler(); } }; }",
            "syntax error, unexpected token \"{\"",
        ),
        (
            "<?php $x = match ($a) { 1 => Foo::class __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => exit __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => yield __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => new class __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"{\"",
        ),
        (
            "<?php $x = match ($a) { 1 => new __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"class\"",
        ),
        (
            "<?php $x = match ($a) { 1 => empty __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"(\"",
        ),
        (
            "<?php $x = match ($a) { 1 => class __halt_compiler() };",
            "syntax error, unexpected token \"class\"",
        ),
        (
            "<?php switch ($a) { ?>html<?php __halt_compiler(); }",
            "syntax error, unexpected T_INLINE_HTML \"html\", expecting \"case\" or \"default\" or \"}\"",
        ),
        (
            "<?php switch ($a): ?>html<?php __halt_compiler(); endswitch;",
            "syntax error, unexpected T_INLINE_HTML \"html\", expecting \"endswitch\" or \"case\" or \"default\"",
        ),
        (
            "<?php $x = match ($a) { 1 => $b++ __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => new static __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => $b instanceof static __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { 1 => static __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"::\"",
        ),
        (
            "<?php function f() { for ($i = 0; __halt_compiler(); ) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php function f() { for (__halt_compiler(); ; ) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php function f() { for ($i = 0; $i < 1; __halt_compiler()) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \")\"",
        ),
        (
            "<?php for ($i = 0; __halt_compiler(); ) {}",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php function f() { $x = match ($a) { 1 => 2 } __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = function () { return 1; } __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = function () use ($a) { return 1; } __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = static function () { return 1; } __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = new class {} __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { new class {} __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = C::{1} __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = $a ? function () { return 1; } __halt_compiler() : 1; }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php switch (match ($x) { 1 => 2 }) { __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"case\" or \"default\" or \"}\"",
        ),
        (
            "<?php $x = match ($a) { fn() => 1 __halt_compiler() => 2 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php $x = match ($a) { 1 => fn() => 1, __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php $x = match ($a) { C::match __halt_compiler() => 1 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php $x = match ($a) { C::new __halt_compiler() => 1 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php $x = match ($a) { C::static __halt_compiler() => 1 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php $x = match ($a) { 1 => C::static __halt_compiler() };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"}\"",
        ),
        (
            "<?php switch (\"{$x}\") { __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"case\" or \"default\" or \"}\"",
        ),
        (
            "<?php switch (\"${x}\") { __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"case\" or \"default\" or \"}\"",
        ),
        (
            "<?php function f() { for ($i = \"{$x}\"; __halt_compiler(); ) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php function f() { for ($i = \"${x}\"; __halt_compiler(); ) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php function f() { $x = new class(\"{$a}\") {} __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php function f() { $x = function ($a = \"{$b}\") { return 1; } __halt_compiler(); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php $x = match ($a) { fn($b = \"{$c}\") => 1 __halt_compiler() => 2 };",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"=>\"",
        ),
        (
            "<?php function f() { __halt_compiler(1); }",
            "syntax error, unexpected integer \"1\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler($a); }",
            "syntax error, unexpected variable \"$a\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(1.5); }",
            "syntax error, unexpected floating-point number \"1.5\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler('a'); }",
            "syntax error, unexpected single-quoted string \"a\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(foo); }",
            "syntax error, unexpected identifier \"foo\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(Foo\\Bar); }",
            "syntax error, unexpected namespaced name \"Foo\\Bar\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(\\Foo); }",
            "syntax error, unexpected fully qualified name \"\\Foo\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(namespace\\Foo); }",
            "syntax error, unexpected namespace-relative name \"namespace\\Foo\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(echo); }",
            "syntax error, unexpected token \"echo\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(static); }",
            "syntax error, unexpected token \"static\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(\"a$b\"); }",
            "syntax error, unexpected double-quote mark, expecting \")\"",
        ),
        (
            "<?php __halt_compiler(Foo\\Bar);",
            "syntax error, unexpected namespaced name \"Foo\\Bar\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler(\\Foo);",
            "syntax error, unexpected fully qualified name \"\\Foo\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler(namespace\\Foo);",
            "syntax error, unexpected namespace-relative name \"namespace\\Foo\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler(echo);",
            "syntax error, unexpected token \"echo\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler(\"a$b\");",
            "syntax error, unexpected double-quote mark, expecting \")\"",
        ),
        (
            "<?php __halt_compiler((int));",
            "syntax error, unexpected token \"(int)\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler((int)); }",
            "syntax error, unexpected token \"(int)\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler((integer));",
            "syntax error, unexpected token \"(int)\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(( Array )); }",
            "syntax error, unexpected token \"(array)\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler (boolean); }",
            "syntax error, unexpected token \"(bool)\", expecting \"(\"",
        ),
        (
            "<?php __halt_compiler((double));",
            "syntax error, unexpected token \"(float)\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler(binary);",
            "syntax error, unexpected token \"(string)\", expecting \"(\"",
        ),
        (
            "<?php function f() { __halt_compiler ?>x",
            "syntax error, unexpected token \";\", expecting \"(\"",
        ),
        (
            "<?php function f() { __halt_compiler( ?>x",
            "syntax error, unexpected token \";\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler ?>x",
            "syntax error, unexpected token \";\", expecting \"(\"",
        ),
        (
            "<?php __halt_compiler( ?>x",
            "syntax error, unexpected token \";\", expecting \")\"",
        ),
        (
            "<?php __halt_compiler(array);",
            "syntax error, unexpected token \"(array)\", expecting \"(\"",
        ),
        (
            "<?php function f() { __halt_compiler(array); }",
            "syntax error, unexpected token \"(array)\", expecting \"(\"",
        ),
        (
            "<?php\nfunction f() {\n__halt_compiler(array);\n}",
            "syntax error, unexpected token \"(array)\", expecting \"(\"",
        ),
        (
            "<?php\nnamespace N {\n__halt_compiler(array);\n}",
            "syntax error, unexpected token \"(array)\", expecting \"(\"",
        ),
        (
            "<?php if (1): __halt_compiler(array); endif;",
            "syntax error, unexpected token \"(array)\", expecting \"(\"",
        ),
        (
            "<?php switch ($a) { case 1: __halt_compiler(array); }",
            "syntax error, unexpected token \"(array)\", expecting \"(\"",
        ),
        (
            "<?php function f() { __halt_compiler; }",
            "syntax error, unexpected token \";\", expecting \"(\"",
        ),
        (
            "<?php echo ; function f() { __halt_compiler(array); }",
            "syntax error, unexpected token \";\"",
        ),
        (
            "<?php function f() { foo(__halt_compiler(array)); }",
            "syntax error, unexpected token \"__halt_compiler\"",
        ),
        (
            "<?php __halt_compiler(<<<EOT\nx\nEOT\n);",
            "syntax error, unexpected heredoc start \"<<<EOT\", expecting \")\"",
        ),
        (
            "<?php function f() { __halt_compiler(<<<EOT\nx\nEOT\n); }",
            "syntax error, unexpected heredoc start \"<<<EOT\", expecting \")\"",
        ),
        (
            "<?php if (1): __halt_compiler(1); endif;",
            "syntax error, unexpected integer \"1\", expecting \")\"",
        ),
        (
            "<?php switch ($a) { case 1: __halt_compiler(1); }",
            "syntax error, unexpected integer \"1\", expecting \")\"",
        ),
        (
            "<?php namespace N { __halt_compiler(1); }",
            "syntax error, unexpected integer \"1\", expecting \")\"",
        ),
        (
            "<?php\nnamespace N {\n__halt_compiler(1);\n}",
            "syntax error, unexpected integer \"1\", expecting \")\"",
        ),
        (
            "<?php function f() { for ($i = 0; __halt_compiler(1); ) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php function f() { for ($i = 0; __halt_compiler(array); ) {} }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \";\"",
        ),
        (
            "<?php switch ($a) { __halt_compiler(1); }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"case\" or \"default\" or \"}\"",
        ),
        (
            "<?php switch ($a) { __halt_compiler(array); }",
            "syntax error, unexpected token \"__halt_compiler\", expecting \"case\" or \"default\" or \"}\"",
        ),
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err(source);
        assert_eq!(error.message, message, "{source}");
        assert!(!error.is_compile_error(), "{source}");
    }
    assert!(tokenize(b"<?php __halt_compiler();", TOKEN_PARSE).is_ok());
    assert!(tokenize(b"<?php label: __halt_compiler();", TOKEN_PARSE).is_ok());
    assert!(tokenize(b"<?php namespace N;\n__halt_compiler();", TOKEN_PARSE).is_ok());
    let typed = tokenize(b"<?php class A { public static ?int $$a; }", TOKEN_PARSE)
        .expect_err("a typed property cannot be a variable variable");
    assert_eq!(typed.message, "syntax error, unexpected token \"$\", expecting variable");
    for source in [
        "<?php class A { public private(set) int $$a; }",
        "<?php class A { private(set) public int $$a; }",
        "<?php class A { protected(set) readonly int $$a; }",
        "<?php class A { public final int $$a; }",
        "<?php abstract class A { public abstract int $$a { get; } }",
        "<?php class A { function __construct(private(set) $$a) {} }",
        "<?php class A { function __construct(public(set) $$a) {} }",
        "<?php class A { function __construct(protected(set) $$a) {} }",
        "<?php class A { function __construct(final $$a) {} }",
        "<?php class A { function __construct(public final $$a) {} }",
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err(source);
        assert_eq!(
            error.message,
            "syntax error, unexpected token \"$\", expecting variable",
            "{source}"
        );
    }
    let heredoc = tokenize(b"<?php __halt_compiler(<<<EOT\nx\nEOT\n);", TOKEN_PARSE)
        .expect_err("a heredoc argument is reported on its body line");
    assert_eq!(
        heredoc.message,
        "syntax error, unexpected heredoc start \"<<<EOT\", expecting \")\""
    );
    assert_eq!(heredoc.line, 2);
    let heredoc_in_function = tokenize(
        b"<?php function f() { __halt_compiler(<<<EOT\nx\nEOT\n); }",
        TOKEN_PARSE,
    )
    .expect_err("a heredoc argument inside a function is reported on its body line");
    assert_eq!(
        heredoc_in_function.message,
        "syntax error, unexpected heredoc start \"<<<EOT\", expecting \")\""
    );
    assert_eq!(heredoc_in_function.line, 2);
    for (source, message, line) in [
        (
            "<?php __halt_compiler(<<<'EOT'\nx\nEOT\n);",
            "syntax error, unexpected heredoc start \"<<<'EOT\", expecting \")\"",
            2,
        ),
        (
            "<?php function f() { __halt_compiler(<<<'EOT'\nx\nEOT\n); }",
            "syntax error, unexpected heredoc start \"<<<'EOT\", expecting \")\"",
            2,
        ),
        (
            "<?php __halt_compiler(<<<\"EOT\"\nx\nEOT\n);",
            "syntax error, unexpected heredoc start \"<<<\"EOT\", expecting \")\"",
            2,
        ),
        (
            "<?php __halt_compiler(<<<EOT\r\nx\r\nEOT\r\n);",
            "syntax error, unexpected heredoc start \"<<<EOT\r\", expecting \")\"",
            2,
        ),
        (
            "<?php __halt_compiler(<<<'EOT'\r\nx\r\nEOT\r\n);",
            "syntax error, unexpected heredoc start \"<<<'EOT'\r\", expecting \")\"",
            2,
        ),
        (
            "<?php function f() { __halt_compiler <<<EOT\nx\nEOT;\n }",
            "syntax error, unexpected heredoc start \"<<<EOT\", expecting \"(\"",
            2,
        ),
        (
            "<?php __halt_compiler <<<EOT\nx\nEOT;\n",
            "syntax error, unexpected heredoc start \"<<<EOT\", expecting \"(\"",
            2,
        ),
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err(source);
        assert_eq!(error.message, message, "{source:?}");
        assert_eq!(error.line, line, "{source:?}");
        assert!(!error.is_compile_error(), "{source:?}");
    }
    for source in [
        "<?php function &f($a = &$b) {}",
        "<?php $f = fn &($a = &$b) => 1;",
        "<?php class A { function list($a = &$b) {} }",
    ] {
        let error = tokenize(source.as_bytes(), TOKEN_PARSE).expect_err(source);
        assert_eq!(error.message, "syntax error, unexpected token \"&\"", "{source}");
    }
}

/// php-src halt-compiler headers keep PHP's TOKEN_PARSE diagnostics.
#[test]
fn tokenizer_php_src_halt_compiler_parse_errors() {
    for (source, message) in [
        (b"<?php __halt_compiler".as_slice(), "syntax error, unexpected end of file, expecting \"(\""),
        (b"<?php __halt_compiler(".as_slice(), "Unclosed '('"),
        (b"<?php __halt_compiler a b c RAW".as_slice(), "syntax error, unexpected identifier \"a\", expecting \"(\""),
        (b"<?php __halt_compiler( )".as_slice(), "syntax error, unexpected end of file, expecting \";\""),
        (b"<?php __halt_compiler(1);".as_slice(), "syntax error, unexpected integer \"1\", expecting \")\""),
    ] {
        let error = tokenize(source, TOKEN_PARSE).expect_err("malformed halt header must fail");
        assert_eq!(error.message, message, "{}", String::from_utf8_lossy(source));
    }
    assert!(tokenize(b"<?php __halt_compiler();RAW", TOKEN_PARSE).is_ok());
    assert!(tokenize(b"<?php __halt_compiler", 0).is_ok());
}


/// PHP reserves `list` in declaration and expression positions; semi-reserved uses stay valid.
#[test]
fn tokenizer_php_src_reserved_list_positions() {
    for source in [
        "<?php function list() {}",
        "<?php class list {}",
        "<?php use A as list;",
        "<?php echo list;",
        "<?php print list;",
        "<?php $x = list;",
        "<?php $x instanceof list;",
        "<?php new list;",
        "<?php new list();",
        "<?php return list;",
        "<?php yield list;",
        "<?php fn() => list;",
        "<?php public list $x;",
        "<?php #[list] function f() {}",
        "<?php function f(): list {}",
        "<?php function f(list $x) {}",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err(), "{source}");
    }
    for source in [
        "<?php class C { function list() {} }",
        "<?php class C { const LIST = 1; }",
        "<?php class C { use A { list as x; } }",
        "<?php foo(list: 1); function foo($x) {}",
        "<?php list($a) = [1];",
        "<?php X::list;",
        "<?php $o->list;",
    ] {
        assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_ok(), "{source}");
    }
}

/// Long interpolation and reference arrays must stay linear under TOKEN_PARSE.
#[test]
fn tokenizer_long_interpolation_and_reference_arrays_stay_linear() {
    let mut source = String::from("<?php $s = \"");
    for _ in 0..20_000 {
        source.push_str("$a[0]");
    }
    source.push_str("\"; $v = [");
    for _ in 0..20_000 {
        source.push_str("&$x, ");
    }
    source.push_str("];");
    let started = std::time::Instant::now();
    tokenize(source.as_bytes(), TOKEN_PARSE).expect("valid interpolation and references");
    assert!(
        started.elapsed() < linearity_budget(),
        "linear scan took {:?}",
        started.elapsed()
    );
}

/// Deeply nested reference targets must be scanned linearly even before the nesting cap rejects
/// the source; the old per-marker target walk revisited every enclosing body quadratically.
#[test]
fn tokenizer_nested_reference_targets_stay_linear() {
    for target in ["array(&$a[", "array(&($a)["] {
        let mut source = String::from("<?php $v = ");
        for _ in 0..20_000 {
            source.push_str(target);
        }
        source.push_str("$b");
        for _ in 0..20_000 {
            source.push_str("])");
        }
        source.push(';');
        let started = std::time::Instant::now();
        // The parser rejects this at its nesting cap; the precheck must still finish linearly.
        let _ = tokenize(source.as_bytes(), TOKEN_PARSE);
        assert!(
            started.elapsed() < linearity_budget(),
            "nested {target:?} reference target scan took {:?}",
            started.elapsed()
        );
    }
}

/// Comments between a control header and its statement must not rescan the comment run for
/// every trivia token; only a T_STRING candidate can be a label.
#[test]
fn tokenizer_comment_after_control_header_stays_linear() {
    let mut source = String::from("<?php if (1) ");
    for _ in 0..100_000 {
        source.push_str("/*x*/");
    }
    source.push_str(" echo 1;");
    let started = std::time::Instant::now();
    tokenize(source.as_bytes(), TOKEN_PARSE).expect("valid comment run after a control header");
    assert!(
        started.elapsed() < linearity_budget(),
        "comment-after-control-header scan took {:?}",
        started.elapsed()
    );
}

/// Repeated computed variables must not walk the same statement prefix for every `$$`.
#[test]
fn tokenizer_computed_variable_runs_stay_linear() {
    let mut source = String::from("<?php echo $$a");
    for _ in 0..20_000 {
        source.push_str(" + $$a");
    }
    source.push(';');
    let started = std::time::Instant::now();
    tokenize(source.as_bytes(), TOKEN_PARSE).expect("valid computed-variable expression");
    assert!(
        started.elapsed() < linearity_budget(),
        "computed-variable scan took {:?}",
        started.elapsed()
    );
}

/// Long runs of comments must not make the parser view rescan the remaining trivia.
#[test]
fn tokenizer_comment_runs_stay_linear() {
    let mut source = String::from("<?php echo 1; ");
    for _ in 0..20_000 {
        source.push_str("/*x*/");
    }
    source.push_str("echo 2;");
    let started = std::time::Instant::now();
    tokenize(source.as_bytes(), TOKEN_PARSE).expect("valid comment run");
    assert!(
        started.elapsed() < linearity_budget(),
        "comment-run scan took {:?}",
        started.elapsed()
    );
}

/// Deeply nested heredocs must not rescan every inner body from every outer pair.
#[test]
fn tokenizer_nested_heredocs_stay_linear() {
    let depth = 20_000;
    let mut source = String::from("<?php $a = [];\n$s = <<<H0\n");
    for level in 1..=depth {
        source.push_str(&format!("{{$a[<<<H{level}\n"));
    }
    source.push_str("x\n");
    for level in (1..=depth).rev() {
        source.push_str(&format!("H{level}\n]}}\n"));
    }
    source.push_str("H0;\n");
    let started = std::time::Instant::now();
    // The parser rejects the nesting after lexing; heredoc validation itself must stay linear.
    let _ = elephc_chopper::scanner::scan(source.as_bytes());
    assert!(
        started.elapsed() < linearity_budget(),
        "nested heredoc validation took {:?}",
        started.elapsed()
    );
}

/// Deeply nested `${...}` chains must not rescan overlapping bodies quadratically.
#[test]
fn tokenizer_deep_dollar_brace_nesting_stays_bounded() {
    let mut source = String::from("<?php ");
    for _ in 0..20_000 {
        source.push_str("${");
    }
    source.push_str("$x");
    for _ in 0..20_000 {
        source.push('}');
    }
    source.push(';');
    let started = std::time::Instant::now();
    tokenize(source.as_bytes(), TOKEN_PARSE).expect("valid variable-variable expression");
    assert!(
        started.elapsed() < linearity_budget(),
        "nested interpolation took {:?}",
        started.elapsed()
    );
}

/// Ordinary braces must not consume the interpolation-view depth budget.
#[test]
fn tokenizer_deep_braces_still_validate_interpolation() {
    let mut source = String::from("<?php ");
    source.push_str(&"{".repeat(256));
    source.push_str("${1+};");
    source.push_str(&"}".repeat(256));
    let error = tokenize_with_warnings(source.as_bytes(), TOKEN_PARSE)
        .expect_err("invalid interpolation inside deep braces");
    assert_eq!(error.message, "syntax error, unexpected token \"}\"");
}

/// Deeply nested computed members must not rescan overlapping bodies quadratically.
#[test]
fn tokenizer_deep_computed_members_stay_bounded() {
    let mut source = String::from("<?php ");
    for _ in 0..20_000 {
        source.push_str("C::{");
    }
    source.push('1');
    for _ in 0..20_000 {
        source.push('}');
    }
    source.push(';');
    let started = std::time::Instant::now();
    let _ = tokenize_with_warnings(source.as_bytes(), TOKEN_PARSE);
    assert!(
        started.elapsed() < linearity_budget(),
        "nested computed members took {:?}",
        started.elapsed()
    );
}

/// Many warnings before a parse error must assign lines in one forward pass.
#[test]
fn tokenizer_many_warnings_before_an_error_stay_linear() {
    let mut source = String::from("<?php ");
    for _ in 0..5_000 {
        source.push_str("declare(encoding='UTF-8'); ");
    }
    source.push_str("echo ;");
    let started = std::time::Instant::now();
    let error = tokenize_with_warnings(source.as_bytes(), TOKEN_PARSE)
        .expect_err("echo ; is a syntax error");
    assert_eq!(error.warnings.len(), 5_000);
    assert!(
        started.elapsed() < linearity_budget(),
        "warning lines took {:?}",
        started.elapsed()
    );
}

/// Deeply nested double-quoted interpolation must return a catchable error, not overflow.
#[test]
fn tokenizer_deep_double_quoted_interpolation_is_bounded() {
    let mut expression = String::new();
    for _ in 0..20_000 {
        expression.push_str("\"{$a[");
    }
    expression.push_str("\"x\"");
    for _ in 0..20_000 {
        expression.push_str("]}\"");
    }
    let source = format!("<?php $s = {expression};");
    let started = std::time::Instant::now();
    let result = tokenize_with_warnings(source.as_bytes(), TOKEN_PARSE);
    assert!(result.is_err(), "deep interpolation must be a catchable syntax error");
    assert!(
        started.elapsed() < linearity_budget(),
        "deep interpolation took {:?}",
        started.elapsed()
    );
}

/// Comments inside a heredoc interpolation may contain braces without ending the range.
#[test]
fn tokenizer_heredoc_interpolation_skips_brace_comments() {
    let source = "<?php $s = <<<TXT\n    {$o->m(\n/* } */\n1)}\n    TXT;";
    tokenize(source.as_bytes(), TOKEN_PARSE).expect("comment braces are not interpolation delimiters");
}

/// Interleaves public compiler and tokenizer calls in one process and checks their grammar and state.
#[test]
fn tokenizer_validation_preserves_compiler_ast_and_anonymous_class_counter() {
    use elephc_chopper::syntax::parser::{self, ast::StmtKind};

    let source = r#"<?php extern "m" function sin(float $x): float; function f(int $x): int { return $x + 1; }"#;
    let before = parser::parse_test_source(source).expect("compiler grammar accepts extern");
    assert!(matches!(before[0].kind, StmtKind::ExternFunctionDecl { .. }));
    let counter = parser::anonymous_class_counter();

    assert!(tokenize(source.as_bytes(), TOKEN_PARSE).is_err());
    tokenize(b"<?php $value = new class {}; ", TOKEN_PARSE).expect("PHP anonymous class");
    tokenize(b"<?php new (foo())();", TOKEN_PARSE).expect("Zend accepts this dynamic new");
    assert_eq!(parser::anonymous_class_counter(), counter);

    let after = parser::parse_test_source(source).expect("compiler mode remains active after validation");
    assert_eq!(format!("{before:?}"), format!("{after:?}"));
    assert_eq!(format!("{after:?}"), format!("{:?}", after.clone()));
}

/// Checks the shared AST for grouped constant attributes and preserves compiler parameter rules.
#[test]
fn tokenizer_validation_preserves_compiler_declaration_rules() {
    use elephc_chopper::syntax::parser::{self, ast::StmtKind};

    let program = parser::parse_test_source("<?php #[A] const X = 1, Y = 2;").unwrap();
    assert_eq!(program.len(), 2);
    for constant in &program {
        assert!(matches!(constant.kind, StmtKind::ConstDecl { .. }));
        assert_eq!(constant.attributes.len(), 1);
    }

    for source in [
        "<?php function f(int $this) {}",
        "<?php class C { function m(int &...$this) {} }",
        "<?php $f = function($this) {};",
        "<?php $f = static fn(int &...$this) => 1;",
    ] {
        tokenize(source.as_bytes(), TOKEN_PARSE).expect("PHP accepts the parameter grammar");
        assert!(parser::parse_test_source(source).is_err(), "compiler accepted {source}");
    }
}

/// Padded cast spellings like `( int )` are valid PHP and must use the same parser path.
#[test]
fn tokenizer_padded_casts_are_valid() {
    for source in [
        b"<?php echo ( int )$x;".as_slice(),
        b"<?php $a = ( integer )$b;",
        b"<?php $a = ( string )$b;",
        b"<?php ( void )$a;",
        b"<?php $a = ( array )$b;",
    ] {
        tokenize(source, TOKEN_PARSE)
            .unwrap_or_else(|error| panic!("{}: {}", String::from_utf8_lossy(source), error.message));
    }
}

/// A delimiter mismatch stops before the later tokens and drops their warnings.
#[test]
fn tokenizer_mismatched_delimiter_drops_later_warnings() {
    let error = tokenize_with_warnings(b"<?php echo \"${a[0}\"; (integer)$x;", TOKEN_PARSE)
        .expect_err("mismatched delimiter");
    assert_eq!(error.message, "Unclosed '[' does not match '}'");
    assert!(
        error.warnings.iter().all(|warning| !warning.message.contains("Non-canonical cast")),
        "{:?}",
        error.warnings
    );
}

/// Qualified names are valid reference targets through indexes and members.
#[test]
fn tokenizer_namespaced_reference_targets_are_valid() {
    for source in [
        b"<?php $a = &\\Foo\\Bar[0];".as_slice(),
        b"<?php $a = &\\Foo\\Bar->x;",
        b"<?php $a = &\\Foo\\Bar?->x;",
        b"<?php $a = &namespace\\Foo\\Bar[0];",
        b"<?php $a = &\\Foo\\Bar[0]->x;",
    ] {
        tokenize(source, TOKEN_PARSE)
            .unwrap_or_else(|error| panic!("{}: {}", String::from_utf8_lossy(source), error.message));
    }
    assert!(tokenize(b"<?php $a = &\\Foo\\Bar{0};", TOKEN_PARSE).is_err());
}

/// A reference target cannot be followed by an assignment or update operator.
#[test]
fn tokenizer_reference_write_continuations_are_errors() {
    for (source, message) in [
        (b"<?php $a = &$b = 1;".as_slice(), "syntax error, unexpected token \"=\""),
        (b"<?php $a = &$b += 1;", "syntax error, unexpected token \"+=\""),
        (b"<?php $a = &$b++;", "syntax error, unexpected token \"++\""),
        (b"<?php $a = &$b[0] = 1;", "syntax error, unexpected token \"=\""),
        (b"<?php $a = &$b->c = 1;", "syntax error, unexpected token \"=\""),
        (b"<?php $a = &foo() = 1;", "syntax error, unexpected token \"=\""),
        (
            b"<?php foreach ($a as &$b = 1) {}",
            "syntax error, unexpected token \"=\", expecting \"->\" or \"?->\" or \"[\"",
        ),
        (
            b"<?php foreach ($a as $k => &$v = 1) {}",
            "syntax error, unexpected token \"=\", expecting \"->\" or \"?->\" or \"[\"",
        ),
        (
            b"<?php $a = [1 => &$v = 1];",
            "syntax error, unexpected token \"=\", expecting \"->\" or \"?->\" or \"[\"",
        ),
        (
            b"<?php $a = [1 => &$v++];",
            "syntax error, unexpected token \"++\", expecting \"->\" or \"?->\" or \"[\"",
        ),
        (
            b"<?php $f = function () use (&$x = 1) {};",
            "syntax error, unexpected token \"=\", expecting \")\"",
        ),
    ] {
        let error = tokenize(source, TOKEN_PARSE).expect_err("write continuation must fail");
        assert_eq!(error.message, message, "{}", String::from_utf8_lossy(source));
    }
    for source in [
        b"<?php foreach ($a as $k => &$v) {}".as_slice(),
        b"<?php $a = [1 => &$v];",
        b"<?php $f = function () use (&$x) {};",
    ] {
        tokenize(source, TOKEN_PARSE)
            .unwrap_or_else(|error| panic!("{}: {}", String::from_utf8_lossy(source), error.message));
    }
}

/// Static member first-class callables are valid PHP in both validation views.
#[test]
fn tokenizer_static_member_first_class_callables_are_valid() {
    for source in [
        b"<?php C::class(...);".as_slice(),
        b"<?php C::$method(...);",
        b"<?php C::method(...);",
        b"<?php $c::class(...);",
    ] {
        tokenize(source, TOKEN_PARSE)
            .unwrap_or_else(|error| panic!("{}: {}", String::from_utf8_lossy(source), error.message));
    }
}

/// A later reference write error must not hide an earlier `${...}` syntax error.
#[test]
fn tokenizer_earlier_dollar_brace_error_wins_over_later_reference() {
    let error = tokenize(b"<?php ${1+}; $a = &$b++;", TOKEN_PARSE)
        .expect_err("malformed ${...} must fail");
    assert_eq!(error.message, "syntax error, unexpected token \"}\"");
    assert_eq!(error.pos, 10);

    let error = tokenize(b"<?php $a = &$b++; ${1+};", TOKEN_PARSE)
        .expect_err("reference write continuation must fail");
    assert_eq!(error.message, "syntax error, unexpected token \"++\"");
}
