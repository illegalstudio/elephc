//! Purpose:
//! Constructs the checker used to refuse although php only decides them when the code runs.
//! Each one compiled in the Symfony Demo's vendor tree, refused its file, and cost a whole
//! compiler re-run (`docblock::erasure::rerun_erasing`).
//!
//! Called from:
//! - `cargo test --test codegen_tests runtime_checked_refusals`.
//!
//! Key details:
//! - Expected outputs are what `php -n` prints for the same source.
//! - The erasure counters (`ELEPHC_GENERICS_STATS`) pin that no re-run happens.

use crate::support::*;

/// The last `[elephc-generics]` totals line of a compile's stderr.
fn stats_line(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| line.starts_with("[elephc-generics] templates="))
        .last()
        .unwrap_or_else(|| panic!("no generics stats line in:\n{stderr}"))
        .to_string()
}

/// A nullable local returned from a non-null declaration is php's run-time `TypeError`, for a
/// scalar and for a union (`symfony/asset-mapper`, `symfony/cache` `doSave()`).
#[test]
fn test_nullable_local_returned_from_a_non_null_type_is_a_runtime_type_error() {
    let out = compile_and_run(
        "<?php
class M {
    public function f(string $c, bool $nul): string { $c = $nul ? null : $c . \"!\"; return $c; }
}
echo (new M)->f(\"a\", false), \"\\n\";
try { echo (new M)->f(\"a\", true), \"\\n\"; } catch (TypeError $e) { echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }
function g(?array $x): array|bool { $y = $x; return $y; }
var_dump(g([1]));
try { var_dump(g(null)); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }
",
    );
    assert_eq!(
        out,
        "a!\nTypeError: M::f(): Return value must be of type string, null returned\narray(1) {\n  [0]=>\n  int(1)\n}\ng(): Return value must be of type array|bool, null returned\n"
    );
}

/// A `mixed` value stored into a typed static property is checked when the store runs
/// (`symfony/cache` `LockRegistry::$signalingException`).
#[test]
fn test_static_property_takes_a_mixed_value_and_checks_it_at_runtime() {
    let out = compile_and_run(
        "<?php
class L {
    private static \\Exception $e;
    public static function lazy() {
        self::$e ??= unserialize('O:9:\"Exception\":1:{s:16:\"' . \"\\0Exception\\0trace\" . '\";a:0:{}}');
        return self::$e;
    }
    public static function set(mixed $v): void { self::$e = $v; }
}
var_dump(get_class(L::lazy()));
function make(int $n): mixed { return $n > 0 ? 5 : new RuntimeException('r'); }
L::set(make(0));
echo \"object ok\\n\";
try { L::set(make(1)); } catch (TypeError $e) { echo get_class($e), \"\\n\"; }
",
    );
    assert_eq!(out, "string(9) \"Exception\"\nobject ok\nTypeError\n");
}

/// A local a closure writes through `use (&$x)` is not the `null` it was initialized with
/// (`symfony/polyfill-deepclone`).
#[test]
fn test_reference_captured_null_local_is_not_typed_null_after_the_closure_ran() {
    let out = compile_and_run(
        "<?php
function found(string $name): string {
    $found = null;
    $walk = function (string $v) use (&$found, &$walk) { $found = $v; };
    $walk($name);
    $walk = null;
    if (null === $found) { throw new ValueError('unknown'); }
    return (new ReflectionFunction($found))->getName();
}
echo found('strlen'), \"\\n\";
function never(bool $b): string {
    $found = null;
    $set = function () use (&$found, $b) { if ($b) { $found = 'strlen'; } };
    $set();
    return (new ReflectionFunction($found))->getName();
}
echo never(true), \"\\n\";
",
    );
    assert_eq!(out, "strlen\nstrlen\n");
}

/// The `else` arm reads the value the loop bound, not what the `then` arm stored over it, and
/// the join after an `if`/`else` is exactly the arms' exits (`symfony/var-dumper`
/// `ExceptionCaster::extractSource()`).
#[test]
fn test_else_arm_and_join_ignore_the_entry_view_of_a_retyped_local() {
    let out = compile_and_run(
        "<?php
class S { public array $attr = []; function __construct(public string $n) {} }
function looped(?string $file): int {
    $out = 0;
    foreach ([\"a\", \"b\"] as $i => $c) {
        if ($i === 0) {
            $c = new S('d');
        } else {
            $c = new S($c);
            if (null !== $file) {
                $c->attr['file'] = $file;
            }
        }
        $c->attr['lang'] = 'x';
        $out += count($c->attr);
    }
    return $out;
}
echo looped('f'), \"\\n\";
function straight(int $i, ?string $file): int {
    $c = \"a\" . $i;
    if ($i === 0) {
        $c = new S('d');
    } else {
        $c = new S($c);
        if (null !== $file) {
            $c->attr['file'] = $file;
        }
    }
    $c->attr['lang'] = 'x';
    return count($c->attr);
}
echo straight(0, 'f'), straight(1, 'f'), \"\\n\";
",
    );
    assert_eq!(out, "3\n12\n");
}

/// An override of `__toString()` through an interface that only extends `Stringable` is not an
/// `func_args` ABI mismatch, even when another `Stringable` uses `func_num_args()`
/// (`symfony/mailer` transports).
#[test]
fn test_tostring_override_through_an_interface_extending_stringable() {
    let out = compile_and_run(
        "<?php
interface TI extends \\Stringable {}
abstract class AT implements TI {}
final class NT extends AT { public function __toString(): string { return 'null://'; } }
final class Other implements \\Stringable {
    public function __toString(): string { return 'other' . func_num_args(); }
}
echo new NT, \" \", new Other, \"\\n\";
echo (new NT)->__toString(), (new Other)->__toString(1, 2), \"\\n\";
",
    );
    assert_eq!(out, "null:// other0\nnull://other2\n");
}

/// `list<T>` in a callable signature of a PHPDoc template is the array php passes, so the
/// call through the callable is not refused and nothing is erased (`doctrine/dbal`
/// `TableEditor::renameColumnInConstraints()`).
#[test]
fn test_doc_list_type_in_a_callable_signature_is_an_array() {
    let source = "<?php
final class Name { public function __construct(public string $v) {} }
final class Editor {
    /**
     * @param array<Name> $collection
     * @param callable(Name): list<Name> $getNames
     * @param callable(Name, list<Name>): Name $modify
     *
     * @template T of Name
     */
    private function rename(array $collection, callable $getNames, callable $modify): array {
        $out = [];
        foreach ($collection as $c) {
            $new = [];
            foreach ($getNames($c) as $n) { $new[] = $n; }
            $out[] = $modify($c, $new);
        }
        return $out;
    }
    public function run(): string {
        $r = $this->rename([new Name('a'), new Name('b')],
            static fn (Name $c): array => [$c, new Name('x')],
            static fn (Name $c, array $names): Name => new Name($c->v . count($names)));
        return implode(',', array_map(static fn (Name $n) => $n->v, $r));
    }
}
echo (new Editor)->run(), \"\\n\";
";
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &[("main.php", source)],
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(stdout, "a2,b2\n", "{stderr}");
    assert!(stats_line(&stderr).contains(" erasure_reruns=0 "), "{stderr}");
}

/// A refusal no annotation can have caused, in a file that does carry PHPDoc templates, does not
/// re-run the whole compiler: erasing the file cannot change the verdict, so the file goes to
/// the runtime exactly as before the PHPDoc adoption existed.
#[test]
fn test_refusal_independent_of_adoption_skips_the_erasure_rerun() {
    let autoloader = "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\n";
    let main = format!("{autoloader}echo (new Gated)->id(\"ok\"), \"\\n\";\n");
    let files = [
        ("main.php", main.as_str()),
        (
            "src/Gated.php",
            "<?php\nclass Gated {\n    /**\n     * @template T\n     * @param T $v\n     * @return T\n     */\n    public function id($v) { return $v; }\n    public function broken(): int { return [1, 2]; }\n}\n",
        ),
    ];
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &files,
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(stdout, "ok\n", "{stderr}");
    let line = stats_line(&stderr);
    assert!(line.contains(" erasure_reruns=0 "), "{line}");
    assert!(!line.contains(" erasure_rerun_skipped=0 "), "{line}");
}

/// A non-numeric string literal at an `int` parameter is php's run-time `TypeError`, not a compile
/// refusal; a numeric one is coerced (`f("5")` is 5, silently).
#[test]
fn test_non_numeric_string_at_an_int_parameter_throws_a_catchable_type_error() {
    let out = compile_and_run(
        "<?php function f(int $i) { return $i; }
try { f(\"x\"); } catch (\\Throwable $t) { echo get_class($t), \"|\", substr($t->getMessage(), 0, 55), \"\\n\"; }
try { f(\"42abc\"); } catch (TypeError $t) { echo \"leading numeric\\n\"; }
var_dump(f(\"5\"));
",
    );
    assert_eq!(
        out,
        "TypeError|f(): Argument #1 ($i) must be of type int, string given\nleading numeric\nint(5)\n"
    );
}

/// A promoted constructor parameter takes the same path: `new Box("bad")` compiles and php's
/// `TypeError` names `Box::__construct()`.
#[test]
fn test_non_numeric_string_at_a_promoted_int_constructor_parameter_throws_a_type_error() {
    let out = compile_and_run(
        "<?php class Box { public function __construct(public int $value) {} }
try { new Box(\"bad\"); } catch (TypeError $t) { echo get_class($t), \"|\", strstr($t->getMessage(), \", called\", true), \"\\n\"; }
echo (new Box(\"7\"))->value, \"\\n\";
",
    );
    assert_eq!(
        out,
        "TypeError|Box::__construct(): Argument #1 ($value) must be of type int, string given\n7\n"
    );
}

/// The same refusal uncaught prints php's fatal block.
#[test]
fn test_uncaught_non_numeric_string_at_an_int_parameter_is_a_fatal_type_error() {
    let reached = compile_and_run_expect_failure(
        "<?php function f(int $i) { return $i; }
f(\"x\");
",
    );
    assert!(
        reached.contains(
            "Fatal error: Uncaught TypeError: f(): Argument #1 ($i) must be of type int, string given, called in "
        ),
        "{reached}"
    );
    assert!(reached.contains("Stack trace:"), "{reached}");
    assert!(reached.contains("thrown in "), "{reached}");
}

/// An array at a builtin's `string` parameter throws php's `TypeError` when the call runs.
#[test]
fn test_array_at_a_builtin_string_parameter_throws_a_catchable_type_error() {
    let out = compile_and_run(
        "<?php
try { strlen([]); } catch (\\Throwable $t) { echo get_class($t), \"|\", $t->getMessage(), \"\\n\"; }
try { echo strtoupper(string: [1]); } catch (TypeError $t) { echo $t->getMessage(), \"\\n\"; }
echo strlen(\"abc\"), \"\\n\";
",
    );
    assert_eq!(
        out,
        "TypeError|strlen(): Argument #1 ($string) must be of type string, array given\nstrtoupper(): Argument #1 ($string) must be of type string, array given\n3\n"
    );
}

/// Uncaught, the builtin's `TypeError` carries no `called in` clause, like php.
#[test]
fn test_uncaught_array_at_a_builtin_string_parameter_is_a_fatal_type_error() {
    let reached = compile_and_run_expect_failure(
        "<?php
echo strlen([]);
",
    );
    assert!(
        reached.contains(
            "Fatal error: Uncaught TypeError: strlen(): Argument #1 ($string) must be of type string, array given in "
        ),
        "{reached}"
    );
    assert!(reached.contains("#0 {main}"), "{reached}");
    assert!(reached.contains("thrown in "), "{reached}");
}

/// A checked-arithmetic key that overflows to float keeps php's warning text and line for a
/// coalesce, an `isset` and an `@`-silenced read (00886abb2e lowers the key to a typed read).
#[test]
fn test_overflowing_checked_key_warns_like_php_with_its_own_line() {
    let out = compile_and_run_capture_merged(
        "<?php
$a = [1];
$i = PHP_INT_MAX;
var_dump($a[$i + 1] ?? 'none');
var_dump(isset($a[$i + 1]));
echo @$a[$i + 1];
echo \"end\\n\";
",
    )
    .stdout;
    let warning = "Warning: The float 9.223372036854776E+18 is not representable as an int, cast occurred in ";
    assert_eq!(out.matches(warning).count(), 2, "{out}");
    assert!(out.contains(" on line 4\nstring(4) \"none\"\n"), "{out}");
    assert!(out.contains(" on line 5\nbool(false)\n"), "{out}");
    assert!(out.ends_with("end\n"), "{out}");
}

/// A `mixed` value handed to the `object` parameter of a builtin reflection method is php's
/// run-time question, not a compile refusal (`symfony/dependency-injection` `ServicesResetter`,
/// doctrine/orm `RawValuePropertyAccessor`, `EntityUserProvider`). php names no call site for an
/// internal method.
#[test]
fn test_mixed_value_at_a_builtin_reflection_object_parameter_is_checked_at_runtime() {
    let out = compile_and_run(
        "<?php
class Pt { public $v = 1; }
function g(mixed $s): bool { return (new ReflectionClass('Pt'))->isUninitializedLazyObject($s); }
function h(mixed $s): bool { return (new ReflectionClass('Pt'))->isInstance($s); }
function k(mixed $s): void { (new ReflectionProperty('Pt', 'v'))->setRawValueWithoutLazyInitialization($s, 5); }
echo var_export(g(new Pt), true), var_export(h(new Pt), true), var_export(h(new stdClass), true), \"\\n\";
$p = new Pt; k($p); echo $p->v, \"\\n\";
try { h(5); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }
",
    );
    assert_eq!(
        out,
        "falsetruefalse\n5\nReflectionClass::isInstance(): Argument #1 ($object) must be of type object, int given\n"
    );
}

/// A variable a closure creates through `use (&$error)` holds what the closure stored once it
/// ran, so `throw $error` and `new RuntimeException($error)` after it are checked when they run
/// (Doctrine's `Instantiator`, symfony/finder `SplFileInfo::getContents()`, symfony/runtime
/// `FrankenPhpWorkerRunner`).
#[test]
fn test_variable_created_by_a_reference_capture_is_not_null_after_the_closure_ran() {
    let out = compile_and_run(
        "<?php
final class C {
    public function run(string $s): void {
        set_error_handler(static function (int $code, string $message) use (&$error): bool {
            $error = new RuntimeException($message);
            return true;
        });
        try { trigger_error($s, E_USER_WARNING); } finally { restore_error_handler(); }
        if ($error) { throw $error; }
    }
    public function msg(string $s): string {
        set_error_handler(static function ($type, $msg) use (&$error) { $error = $msg; });
        try { trigger_error($s, E_USER_WARNING); } finally { restore_error_handler(); }
        if ($s === 'x') { throw new RuntimeException($error); }
        return $error;
    }
}
$c = new C;
try { $c->run('first'); } catch (RuntimeException $e) { echo $e->getMessage(), \"\\n\"; }
try { $c->msg('x'); } catch (RuntimeException $e) { echo $e->getMessage(), \"\\n\"; }
echo $c->msg('y'), \"\\n\";
",
    );
    assert_eq!(out, "first\nx\ny\n");
}

/// An `object|string` parameter whose object arm an `if` rewrites to the class name still has
/// the declared union at the property array key; php fails only if an object reaches the key
/// (symfony/dependency-injection `ContainerBuilder::addObjectResource()`).
#[test]
fn test_object_or_string_parameter_keys_a_property_array_after_the_object_arm_is_rewritten() {
    let out = compile_and_run(
        "<?php
final class B {
    private array $r = [];
    public function add(object|string $object): static {
        if (is_object($object)) { $object = $object::class; }
        $class = $this->r[$object] ??= new \\ReflectionClass($object);
        echo $class->getName(), \"\\n\";
        return $this;
    }
    public function put(object|string $object): int {
        if (is_object($object)) { $object = $object::class; }
        $this->r[$object] = 1;
        return count($this->r);
    }
}
$b = new B;
$b->add(new stdClass)->add('ArrayObject');
echo $b->put(new stdClass), $b->put('Foo'), \"\\n\";
",
    );
    assert_eq!(out, "stdClass\nArrayObject\n23\n");
}

/// An object whose static type is an interface may implement a member of a union of unrelated
/// interfaces; php binds it when the call runs and throws `TypeError` if it implements none
/// (symfony/framework-bundle `ConfigDumpReferenceCommand`).
#[test]
fn test_interface_object_at_a_union_of_other_interfaces_is_checked_at_runtime() {
    let out = compile_and_run(
        "<?php
interface Ext { public function alias(): string; }
interface Conf { public function tree(): string; }
interface ConfExt { public function conf(): string; }
class A implements Ext, ConfExt { public function alias(): string { return 'a'; } public function conf(): string { return 'conf-a'; } }
class B implements Ext { public function alias(): string { return 'b'; } }
function find(string $n): Ext { return $n === 'a' ? new A : new B; }
function url(Conf|ConfExt $e): ?string { return $e instanceof Conf ? $e->tree() : $e->conf(); }
echo url(find('a')), \"\\n\";
try { echo url(find('b')), \"\\n\"; } catch (TypeError $t) {
    echo get_class($t), ' ', str_contains($t->getMessage(), 'url(): Argument #1 ($e) must be of type Conf|ConfExt, ') ? 'named' : $t->getMessage(), \"\\n\";
}
",
    );
    assert_eq!(out, "conf-a\nTypeError named\n");
}

/// A closure literal's `self` return type, passed to a PHPDoc-templated factory from a static
/// method, means the class the call is written in (doctrine/dbal `IBMDB2\Exception\*::new()`).
#[test]
fn test_closure_self_return_type_at_a_phpdoc_template_call_binds_the_enclosing_class() {
    let out = compile_and_run(
        "<?php
declare(strict_types=1);
abstract class AbsE extends Exception {
    public function __construct(string $message, private readonly ?string $sqlState = null, int $code = 0, ?Throwable $previous = null) {
        parent::__construct($message, $code, $previous);
    }
    public function getSQLState(): ?string { return $this->sqlState; }
}
final class Factory {
    /**
     * @param callable(int): T $constructor
     *
     * @return T
     *
     * @template T of AbsE
     */
    public static function create(string $message, callable $constructor): AbsE {
        $code = 0;
        if (strpos($message, 'SQL30082N') !== false) { $code = -30082; }
        return $constructor($code);
    }
}
final class ConnErr extends AbsE {
    public static function new(string $connection): self {
        $message = 'conn SQL30082N failed';
        $sqlState = '08001';
        return Factory::create($message, static function (int $code) use ($message, $sqlState): self {
            return new self($message, $sqlState, $code);
        });
    }
}
$e = ConnErr::new('x');
echo $e->getMessage(), ' ', $e->getCode(), ' ', $e->getSQLState(), \"\\n\";
",
    );
    assert_eq!(out, "conn SQL30082N failed -30082 08001\n");
}
