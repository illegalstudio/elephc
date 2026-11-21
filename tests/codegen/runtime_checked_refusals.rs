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
