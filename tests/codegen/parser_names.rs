//! Purpose:
//! Integration or regression tests for PHP 8 qualified names whose segments are reserved
//! keywords (`A\Default\X`, `\Default\X`, `namespace A\List;`), as tokenized by PHP's
//! T_NAME_QUALIFIED / T_NAME_FULLY_QUALIFIED rules.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A keyword is a name segment only when a `\` touches it inside the name; a lone
//!   keyword stays reserved (see `error_tests::syntax` negative cases).

use crate::support::*;

/// `use` imports (simple, grouped, function and const) accept keyword segments,
/// as in laravel/prompts `use Laravel\Prompts\Themes\Default\Concerns\InteractsWithStrings;`.
#[test]
fn test_use_imports_accept_keyword_segments() {
    let out = compile_and_run(r#"<?php
namespace P\Default\Concerns {
    trait InteractsWithStrings { public function tag(): string { return 'trait'; } }
    class Box { public function name(): string { return 'box'; } }
}
namespace P\List {
    class Item { public function name(): string { return 'item'; } }
    function make(): string { return 'make'; }
}
namespace P\Function { const LIMIT = 42; }
namespace App {
    use P\Default\Concerns\InteractsWithStrings;
    use P\{Default\Concerns\Box, List\Item as ListItem};
    use P\Default\{Concerns\Box as Box2};
    use function P\List\make;
    use const P\Function\LIMIT;
    class Prompt { use InteractsWithStrings; }
    echo (new Prompt())->tag(), "\n";
    echo (new Box())->name(), ' ', (new ListItem())->name(), ' ', (new Box2())->name(), "\n";
    echo make(), ' ', LIMIT, ' ', \P\List\make(), ' ', \P\Function\LIMIT, "\n";
}
"#);
    assert_eq!(out, "trait\nbox item box\nmake 42 make 42\n");
}

/// Keyword segments work in `new`, static calls, `instanceof`, type hints,
/// `extends`/`implements`, `catch`, attributes, and namespaced function calls.
#[test]
fn test_expression_names_accept_keyword_segments() {
    let out = compile_and_run(r#"<?php
namespace Q\Class {
    interface Shape { public function area(): int; }
    class Base { public static function make(): string { return 'static-call'; } }
    #[\Attribute] class Mark {}
}
namespace Q\Match\Enum {
    class Square extends \Q\Class\Base implements \Q\Class\Shape {
        public function area(): int { return 16; }
    }
    class Oops extends \Exception {}
    function twice(int $n): int { return $n * 2; }
}
namespace Q\Static { class Missing extends \Exception {} }
namespace Q {
    #[Class\Mark]
    function measure(Class\Shape $s): int { return $s->area(); }
    $sq = new Match\Enum\Square();
    echo measure($sq), ' ', Match\Enum\Square::make(), "\n";
    echo $sq instanceof Class\Shape ? 'yes' : 'no', "\n";
    echo Match\Enum\twice(21), ' ', \Q\Match\Enum\twice(5), "\n";
    try { throw new Match\Enum\Oops('boom'); }
    catch (Match\Enum\Oops | \Q\Static\Missing $e) { echo 'caught ', $e->getMessage(), "\n"; }
}
"#);
    assert_eq!(out, "16 static-call\nyes\n42 10\ncaught boom\n");
}

/// Namespace declarations accept keyword segments (and a bare keyword, which PHP 8 allows
/// in `namespace` declarations), and `\Default\X` fully qualified names resolve.
#[test]
fn test_namespace_declarations_accept_keyword_segments() {
    let out = compile_and_run(r#"<?php
namespace R\Static {
    class Holder { const LABEL = 'holder'; }
    function hello(): string { return __NAMESPACE__; }
    echo hello(), ' ', \R\Static\Holder::LABEL, ' ', Holder::class, "\n";
}
namespace Default\Fn {
    class X { public static function id(): string { return static::class; } }
}
namespace List {
    function where(): string { return __FUNCTION__; }
}
namespace {
    echo \Default\Fn\X::id(), ' ', Default\Fn\X::id(), "\n";
    $x = new \Default\Fn\X();
    echo get_class($x), ' ', $x instanceof Default\Fn\X ? 'ok' : 'no', "\n";
    echo \List\where(), "\n";
}
"#);
    assert_eq!(out, "R\\Static holder R\\Static\\Holder\nDefault\\Fn\\X Default\\Fn\\X\nDefault\\Fn\\X ok\nList\\where\n");
}

/// Fully qualified literal constants (`\true`) and predefined-constant tokens keep working
/// next to keyword-segment names.
#[test]
fn test_keyword_segments_keep_fully_qualified_constants() {
    let out = compile_and_run(r#"<?php
namespace A\Default { const MODE = 'mode'; }
namespace {
    echo \true ? 'T' : 'F', \null === null ? 'N' : '-', \PHP_INT_MAX > 0 ? 'M' : '-', "\n";
    echo \A\Default\MODE, ' ', A\Default\MODE, "\n";
}
"#);
    assert_eq!(out, "TNM\nmode mode\n");
}

/// `\static::class`, `\self::class` and `\parent::class` resolve like their unqualified
/// forms, as in PHP 8; `\static` keeps late static binding.
#[test]
fn test_fully_qualified_special_class_fetch_matches_unqualified() {
    let out = compile_and_run(r#"<?php
namespace N {
    class P {}
    class A extends P {
        static function f() { return \static::class; }
        static function both() { return [static::class, \static::class, \self::class, \parent::class]; }
    }
    class B extends A {}
    echo A::f(), ' ', B::f(), "\n";
    echo implode(',', B::both()), "\n";
}
namespace {
    class G { static function f() { return \static::class; } }
    echo G::f(), "\n";
}
"#);
    assert_eq!(out, "N\\A N\\B\nN\\B,N\\B,N\\A,N\\P\nG\n");
}
