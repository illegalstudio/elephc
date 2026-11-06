//! Purpose:
//! End-to-end tests for the classes PHP refuses to serialize or unserialize: the builtin set
//! php-src flags `ZEND_ACC_NOT_SERIALIZABLE` that elephc registers, subclasses of those, and
//! Closure values.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the serialize codegen test module.
//!
//! Key details:
//! - Expected output was measured on PHP 8.5.10 (Phar cases with `phar.readonly=0`), except the
//!   user classes named after internals elephc does not model, which PHP rejects at declaration.
//! - A refused class must throw even when the program never names it, since the wire string alone
//!   decides which class `unserialize()` meets.
//! - Only elephc's own declaration of a builtin is refused: a program that does not load the curl
//!   or PDO prelude may declare a class with one of their names, and that class round-trips.
//! - `unserialize()` refuses from its allocation-free preflight, so a caught refusal leaves nothing
//!   behind and runs no hook of the payload, which `--heap-debug` pins.

use crate::support::*;

/// `unserialize()` refuses a Reflection class the program never references, the exact shape of
/// issue #1316: the refusal cannot depend on the class surviving declaration pruning.
#[test]
fn test_unserialize_refuses_reflection_class_the_program_never_names() {
    let out = compile_and_run(
        r#"<?php
try {
    var_dump(unserialize('O:15:"ReflectionClass":0:{}'));
} catch (Throwable $e) {
    echo get_class($e), " ", $e->getMessage(), "\n";
}
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        "Exception Unserialization of 'ReflectionClass' is not allowed\nafter\n"
    );
}

/// `unserialize()` refuses every refused class it meets, whatever the wire spelling, nesting,
/// or ancestry, and names the class the way it is declared. The refusal happens before the
/// object's body is decoded, so a nested object's `__wakeup()` never runs, and an
/// `allowed_classes` policy that blocks the class still yields an incomplete object.
#[test]
fn test_unserialize_refuses_classes_php_marks_not_serializable() {
    let out = compile_and_run(
        r#"<?php
class Mine extends ReflectionClass {}
class Woken {
    public function __wakeup(): void { echo "woken\n"; }
}
function attempt(string $label, string $wire): void {
    try {
        $value = unserialize($wire);
        echo $label, ": ", gettype($value), "\n";
    } catch (Exception $e) {
        echo $label, ": ", get_class($e), " ", $e->getMessage(), "\n";
    }
}
attempt("reflection", 'O:15:"ReflectionClass":0:{}');
attempt("spelling", 'O:18:"reflectionfunction":0:{}');
attempt("nested", 'a:1:{i:0;O:15:"ReflectionClass":0:{}}');
attempt("before-body", 'O:15:"ReflectionClass":1:{s:1:"w";O:5:"Woken":0:{}}');
attempt("subclass", 'O:4:"Mine":0:{}');
attempt("file-info", 'O:11:"SplFileInfo":0:{}');
attempt("file-object", 'O:13:"SplFileObject":0:{}');
attempt("generator", 'O:9:"Generator":0:{}');
attempt("closure", 'O:7:"Closure":0:{}');
attempt("woken", 'O:5:"Woken":0:{}');
$blocked = unserialize('O:4:"Mine":0:{}', ['allowed_classes' => false]);
echo "blocked: ", get_class($blocked), "\n";
echo "reflection still works: ", (new ReflectionClass('Woken'))->getName(), "\n";
"#,
    );
    assert_eq!(
        out,
        "reflection: Exception Unserialization of 'ReflectionClass' is not allowed\n\
         spelling: Exception Unserialization of 'ReflectionFunction' is not allowed\n\
         nested: Exception Unserialization of 'ReflectionClass' is not allowed\n\
         before-body: Exception Unserialization of 'ReflectionClass' is not allowed\n\
         subclass: Exception Unserialization of 'Mine' is not allowed\n\
         file-info: Exception Unserialization of 'SplFileInfo' is not allowed\n\
         file-object: Exception Unserialization of 'SplFileObject' is not allowed\n\
         generator: Exception Unserialization of 'Generator' is not allowed\n\
         closure: Exception Unserialization of 'Closure' is not allowed\n\
         woken\n\
         woken: object\n\
         blocked: __PHP_Incomplete_Class\n\
         reflection still works: Woken\n"
    );
}

/// The handles an injected prelude provides refuse `unserialize()` too, whether the program
/// uses their prelude (`PDO` and its driver subclass) or not (`GdImage`), instead of hydrating
/// a zombie object around a dead bridge handle; the live connection keeps working.
#[test]
fn test_unserialize_refuses_prelude_handles() {
    let out = compile_and_run(
        r#"<?php
$db = new PDO("sqlite::memory:");
foreach (['O:3:"PDO":0:{}', 'O:10:"Pdo\\Sqlite":0:{}', 'O:12:"PDOStatement":0:{}', 'O:7:"GdImage":0:{}'] as $wire) {
    try {
        unserialize($wire);
        echo "hydrated\n";
    } catch (Exception $e) {
        echo get_class($e), " ", $e->getMessage(), "\n";
    }
}
echo $db->query("SELECT 7")->fetchColumn(), "\n";
"#,
    );
    assert_eq!(
        out,
        "Exception Unserialization of 'PDO' is not allowed\n\
         Exception Unserialization of 'Pdo\\Sqlite' is not allowed\n\
         Exception Unserialization of 'PDOStatement' is not allowed\n\
         Exception Unserialization of 'GdImage' is not allowed\n\
         7\n"
    );
}

/// Namespaced user classes sharing an internal short name remain ordinary serializable classes.
#[test]
fn test_serialize_keeps_user_classes_named_like_unmodelled_internals() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
class WeakMap { public $a = 1; }
class ReflectionType { public $b = 'x'; }
echo serialize(new WeakMap()), "\n";
echo serialize(new ReflectionType()), "\n";
$copy = unserialize(serialize(new WeakMap()));
echo get_class($copy), " ", $copy->a, "\n";
"#,
    );
    assert_eq!(
        out,
        "O:12:\"Demo\\WeakMap\":1:{s:1:\"a\";i:1;}\n\
         O:19:\"Demo\\ReflectionType\":1:{s:1:\"b\";s:1:\"x\";}\n\
         Demo\\WeakMap 1\n"
    );
}

/// `Phar` and `PharData` refuse both directions like PHP, including a user subclass and an
/// archive nested in an array, even though elephc registers them without their PHP parents.
#[test]
fn test_serialize_refuses_phar_archives() {
    let out = compile_and_run(
        r#"<?php
class Archive extends PharData {}
function attempt(string $label, mixed $value): void {
    try {
        echo $label, ": ", serialize($value), "\n";
    } catch (Exception $e) {
        echo get_class($e), " ", $e->getMessage(), "\n";
    }
}
attempt("phar", new Phar("deny.phar"));
attempt("phardata", new PharData("deny.tar"));
attempt("subclass", new Archive("deny2.tar"));
attempt("nested", [new PharData("deny.tar")]);
foreach (['O:4:"Phar":0:{}', 'O:8:"PharData":0:{}', 'O:7:"Archive":0:{}'] as $wire) {
    try {
        unserialize($wire);
        echo "hydrated\n";
    } catch (Exception $e) {
        echo get_class($e), " ", $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "phar: Exception Serialization of 'Phar' is not allowed\n\
         phardata: Exception Serialization of 'PharData' is not allowed\n\
         subclass: Exception Serialization of 'Archive' is not allowed\n\
         nested: Exception Serialization of 'PharData' is not allowed\n\
         Exception Unserialization of 'Phar' is not allowed\n\
         Exception Unserialization of 'PharData' is not allowed\n\
         Exception Unserialization of 'Archive' is not allowed\n"
    );
}

/// `serialize()` of a Closure compiles and throws PHP's catchable Exception wherever the
/// closure sits: typed as a callable, behind `mixed`, nested in an array, or held by an untyped
/// or `Closure`-typed object property (whose slot holds the descriptor itself, not a Mixed box).
/// Ordinary values still serialize and the closure stays callable afterwards.
#[test]
fn test_serialize_refuses_closures() {
    let out = compile_and_run(
        r#"<?php
class Job {
    public function __construct(public Closure $run) {}
}
class Holder {
    public $callback;
}
function attempt(string $label, mixed $value): void {
    try {
        echo $label, ": ", serialize($value), "\n";
    } catch (Exception $e) {
        echo get_class($e), " ", $e->getMessage(), "\n";
    }
}
$add = function (int $x): int { return $x + 1; };
try {
    echo serialize($add), "\n";
} catch (Exception $e) {
    echo "typed: ", get_class($e), " ", $e->getMessage(), "\n";
}
attempt("mixed", $add);
attempt("first-class", strlen(...));
attempt("nested", ['k' => [1, fn() => 2]]);
$holder = new Holder();
$holder->callback = $add;
attempt("property", $holder);
attempt("typed-property", new Job($add));
attempt("plain", [1, 'two']);
echo $add(1), "\n";
"#,
    );
    assert_eq!(
        out,
        "typed: Exception Serialization of 'Closure' is not allowed\n\
         mixed: Exception Serialization of 'Closure' is not allowed\n\
         first-class: Exception Serialization of 'Closure' is not allowed\n\
         nested: Exception Serialization of 'Closure' is not allowed\n\
         property: Exception Serialization of 'Closure' is not allowed\n\
         typed-property: Exception Serialization of 'Closure' is not allowed\n\
         plain: a:2:{i:0;i:1;i:1;s:3:\"two\";}\n\
         2\n"
    );
}

/// A program that does not use ext/curl or PDO may declare its own `CurlHandle`, `CURLFile` or
/// `Pdo\Sqlite`: no prelude declares them, so they are ordinary user classes that serialize and
/// round-trip (in any wire spelling) instead of being refused like the builtins they are named
/// after. PHP without those extensions behaves the same; the expected wire was measured on PHP
/// 8.5.10 with the classes renamed, since that build loads both extensions.
#[test]
fn test_serialize_keeps_user_classes_named_like_prelude_classes() {
    let out = compile_and_run(
        r#"<?php
namespace Pdo {
    class Sqlite { public $db = 'memory'; }
    function make(): Sqlite { return new Sqlite(); }
}
namespace {
    class CurlHandle { public $url = 'x'; }
    class CURLFile { public $name = 'f.txt'; }
    foreach ([new CurlHandle(), new CURLFile(), Pdo\make()] as $object) {
        $wire = serialize($object);
        echo $wire, "\n";
        echo get_class(unserialize($wire)), "\n";
    }
    var_dump(unserialize('O:10:"curlhandle":0:{}') instanceof CurlHandle);
}
"#,
    );
    assert_eq!(
        out,
        "O:10:\"CurlHandle\":1:{s:3:\"url\";s:1:\"x\";}\nCurlHandle\n\
         O:8:\"CURLFile\":1:{s:4:\"name\";s:5:\"f.txt\";}\nCURLFile\n\
         O:10:\"Pdo\\Sqlite\":1:{s:2:\"db\";s:6:\"memory\";}\nPdo\\Sqlite\n\
         bool(true)\n"
    );
}

/// A `Closure`-typed property that was never assigned holds no Closure, so `serialize()` leaves it
/// out like any uninitialized typed property instead of refusing the object; once assigned, the
/// Closure is refused. Measured on PHP 8.5.10.
#[test]
fn test_serialize_skips_an_uninitialized_closure_property() {
    let out = compile_and_run(
        r#"<?php
class Job {
    public Closure $callback;
    public int $n = 1;
}
$job = new Job();
echo serialize($job), "\n";
$job->callback = fn() => 1;
try {
    echo serialize($job), "\n";
} catch (Exception $e) {
    echo get_class($e), " ", $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "O:3:\"Job\":1:{s:1:\"n\";i:1;}\n\
         Exception Serialization of 'Closure' is not allowed\n"
    );
}

/// Catching `unserialize()` refusals in a loop leaves a clean heap: the refused class, whether the
/// program declares it (a user subclass, a builtin) or not (`Generator` here), is refused by the
/// allocation-free preflight before the decoder allocates its object, its box, or an enclosing
/// array, and before a sibling's `__wakeup()` could run (PHP defers those hooks until the payload
/// decoded, so it never runs them either).
#[test]
fn test_caught_unserialize_refusals_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Mine extends ReflectionClass {}
class Woken {
    public function __wakeup(): void { echo "woken\n"; }
}
$refused = 0;
for ($i = 0; $i < 40; $i++) {
    foreach ([
        'O:4:"Mine":0:{}',
        'O:15:"ReflectionClass":0:{}',
        'O:9:"Generator":0:{}',
        'a:2:{i:0;O:8:"stdClass":0:{}i:1;O:4:"Mine":0:{}}',
        'a:2:{i:0;O:5:"Woken":0:{}i:1;O:15:"ReflectionClass":0:{}}',
        'O:5:"Woken":1:{s:1:"x";O:4:"Mine":0:{}}',
    ] as $wire) {
        try {
            unserialize($wire);
        } catch (Exception $e) {
            $refused++;
            $last = $e->getMessage();
        }
    }
}
echo $refused, " ", $last, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "240 Unserialization of 'Mine' is not allowed\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
