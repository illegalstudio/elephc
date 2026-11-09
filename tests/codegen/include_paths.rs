//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of include paths, including include with dunder dir concat, include with const ref, and include with define ref.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use crate::support::*;

/// Verifies a variable include path executes the selected file in the caller's scope.
#[test]
fn test_runtime_dynamic_include_variable_path_shares_scope() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { $prefix = 'runtime'; include $path; } load('piece.php');",
            ),
            ("piece.php", "<?php echo $prefix . '-include';"),
        ],
        "main.php",
    );
    assert_eq!(out, "runtime-include");
}

/// Verifies a by-reference nested `foreach` write reaches the outer array when the whole
/// fragment executes through the eval bridge (a runtime-only-known include path forces the
/// interpreter to run the included file, rather than the AOT compiler lowering it directly).
///
/// This is the shape `examples/symfony-app/vendor/symfony/event-dispatcher/EventDispatcher.php`'s
/// `optimizeListeners()` uses: `foreach ($this->listeners as &$byPriority) { foreach
/// ($byPriority as $priority => &$listeners) { ... } }`. `php -n` 8.5.6 prints `X,Y/Z`.
#[test]
fn test_runtime_dynamic_include_nested_by_reference_foreach_writes_through_outer_reference() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { include $path; } load('piece.php');",
            ),
            (
                "piece.php",
                "<?php\n$d = [[\"x\", \"y\"], [\"z\"]];\nforeach ($d as &$row) {\n    foreach ($row as &$cell) {\n        $cell = strtoupper($cell);\n    }\n    unset($cell);\n}\nunset($row);\necho implode(\",\", $d[0]), \"/\", implode(\",\", $d[1]);\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "X,Y/Z");
}

/// Verifies a `use (&$x)` closure capture binds the CELL `$x` names at closure-creation time,
/// not the NAME, when the whole fragment executes through the eval bridge (a runtime-only-known
/// include path forces the interpreter to run the included file, rather than the AOT compiler
/// lowering it directly) -- a faithful replica of
/// `Symfony\Component\EventDispatcher\EventDispatcher::optimizeListeners()`: a private method
/// nests two by-reference `foreach` loops, binds a reference to an appended slot of an object
/// property array, and stores a `static` closure that captures that reference (plus the loop's
/// element alias) and replaces itself in that slot on first call.
///
/// `php -n` 8.5.6 prints `high:x|W(x)|` then `high:y|W(y)|W(y)` then `p4done`. Before the fix,
/// the closure's by-reference captures resolved their caller-side write-back target by
/// (raw-scope-pointer, NAME) instead of holding the cell captured at closure-creation time; here
/// the named scope is `optimizeListeners()`'s own activation, which has already returned and
/// been dropped by the time any stored closure is first invoked, so write-back dereferenced
/// freed Rust stack memory. elephc SIGSEGV'd (exit 139) with no output at all.
#[test]
fn test_runtime_dynamic_include_self_replacing_closure_survives_dropped_defining_method_scope() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { include $path; } load('piece.php');",
            ),
            (
                "piece.php",
                r#"<?php
class Wrapped
{
    public function __invoke(string $tag): string
    {
        return "W(" . $tag . ")";
    }
}

class Dispatcher
{
    private array $listeners = [];
    private array $optimized = [];

    public function add(string $event, int $priority, callable $listener): void
    {
        $this->listeners[$event][$priority][] = $listener;
        unset($this->optimized[$event]);
    }

    private function optimizeListeners(string $eventName): array
    {
        krsort($this->listeners[$eventName]);
        $this->optimized[$eventName] = [];

        foreach ($this->listeners[$eventName] as &$listeners) {
            foreach ($listeners as &$listener) {
                $closure = &$this->optimized[$eventName][];
                if (\is_array($listener) && isset($listener[0]) && $listener[0] instanceof \Closure && 2 >= \count($listener)) {
                    $closure = static function (...$args) use (&$listener, &$closure) {
                        if ($listener[0] instanceof \Closure) {
                            $listener[0] = $listener[0]();
                            $listener[1] ??= '__invoke';
                        }
                        ($closure = $listener(...))(...$args);
                    };
                } else {
                    $closure = $listener instanceof Wrapped ? $listener : $listener(...);
                }
            }
        }

        return $this->optimized[$eventName];
    }

    public function call(string $eventName, string $tag): string
    {
        $out = [];
        foreach ($this->optimized[$eventName] ?? $this->optimizeListeners($eventName) as $listener) {
            $out[] = $listener($tag);
        }
        return implode("|", $out);
    }
}

$d = new Dispatcher();
$d->add("evt", 10, static fn (string $t): string => "high:" . $t);
$d->add("evt", 5, new Wrapped());
$d->add("evt", 1, [static fn (): object => new Wrapped(), "__invoke"]);

echo $d->call("evt", "x"), "\n";
echo $d->call("evt", "y"), "\n";
echo "p4done\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "high:x|W(x)|\nhigh:y|W(y)|W(y)\np4done\n");
}

/// Verifies `EXPR(...)` first-class-callable syntax produces a working `Closure` for every
/// runtime value shape when the whole fragment executes through the eval bridge (a
/// runtime-only-known include path forces the interpreter to run the included file, rather than
/// the AOT compiler lowering it directly) -- the exact surface
/// `Symfony\Component\EventDispatcher\EventDispatcher::optimizeListeners()` exercises through
/// `$listener(...)` where `$listener` already holds a `Closure`.
///
/// `php -n` 8.5.6 prints `closurevar:6|arrayvar:m:7|stringvar:eight`. Before the fix, the
/// interpreter unconditionally wrapped every `EXPR(...)` value in an `InvokableObject { object }`
/// closure target regardless of what `EXPR` evaluated to: a Closure-valued variable produced a
/// target whose "object" was itself a closure vessel, so calling it asked the runtime for that
/// vessel's native class -- the placeholder `stdClass`, never `Closure` -- and raised `Error:
/// Object of type stdClass is not callable`; an array or string callable variable had no matching
/// dispatch case at all and failed with `unsupported DynamicCall expression`.
#[test]
fn test_runtime_dynamic_include_first_class_callable_syntax_on_variable_values() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { include $path; } load('piece.php');",
            ),
            (
                "piece.php",
                "<?php\nclass FccValHolder {\n    public function m($t) { return \"m:\" . $t; }\n}\n$h = new FccValHolder();\n\n$arrow = static function ($t) { return $t; };\n$closureVar = $arrow;\n$e = $closureVar(...);\necho \"closurevar:\" . $e(\"6\") . \"|\";\n\n$pair = [$h, \"m\"];\n$arrayCallableVar = $pair;\n$f = $arrayCallableVar(...);\necho \"arrayvar:\" . $f(\"7\") . \"|\";\n\n$name = \"strtolower\";\n$stringNameVar = $name;\n$i = $stringNameVar(...);\necho \"stringvar:\" . $i(\"EIGHT\");\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "closurevar:6|arrayvar:m:7|stringvar:eight");
}

/// Verifies a dynamic require expression returns the included file's explicit value.
#[test]
fn test_runtime_dynamic_require_expression_returns_value() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { return require $path; } echo load('value.php');",
            ),
            ("value.php", "<?php return 'dynamic-value';"),
        ],
        "main.php",
    );
    assert_eq!(out, "dynamic-value");
}

/// Verifies dynamic include_once canonicalizes and executes one selected path only once.
#[test]
fn test_runtime_dynamic_include_once_tracks_loaded_path() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { include_once $path; } load('once.php'); load('./once.php');",
            ),
            ("once.php", "<?php echo 'once';"),
        ],
        "main.php",
    );
    assert_eq!(out, "once");
}

// `require __DIR__ . '/...';` is the most common idiomatic include pattern
// in PHP. After magic-constant substitution, __DIR__ becomes a string literal
// and the resolver's path folder concatenates it with the trailing literal.

/// Verifies `require __DIR__ . '/lib/inner.php'` works after magic-constant
/// substitution. `__DIR__` is lowered to a string literal by the magic-constants
/// pass; the resolver then concatenates it with the trailing literal to produce
/// the resolved include path.
#[test]
fn test_include_with_dunder_dir_concat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/lib/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is a top-level `const`
/// defined before the require. The resolver resolves the const reference to
/// the string `'lib'` before path resolution.
#[test]
fn test_include_with_const_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nconst LIB = 'lib';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is a top-level `define()`
/// evaluated before the require. The resolver tracks defines incrementally as
/// files are inlined, allowing the require to reference a define set earlier.
#[test]
fn test_include_with_define_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\ndefine('LIB', 'lib');\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is set via the fully
/// qualified `\define()` call. The backslash prefix is canonical and the
/// define still feeds the include path resolution.
#[test]
fn test_include_with_fully_qualified_define_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n\\define('LIB', 'lib');\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require __DIR__ . '/' . 'lib' . '/' . 'inner.php'` with multiple
/// chained concatenations. The resolver must correctly evaluate all BinaryOp
/// nodes in the path expression before resolving the include path.
#[test]
fn test_include_with_nested_concat() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/' . 'lib' . '/' . 'inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies a constant defined in an included bootstrap file can be used in a
/// subsequent require within the main file. The resolver tracks constants
/// incrementally as files are inlined in order, so this cross-file forward
/// reference works.
#[test]
fn test_include_with_const_defined_in_included_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'bootstrap.php';\nrequire SUBDIR . '/inner.php';\necho 'after';\n",
            ),
            (
                "bootstrap.php",
                "<?php\nconst SUBDIR = 'lib';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `const BASE = __DIR__ . '/lib'; require BASE . '/inner.php'`
/// works. `__DIR__` is lowered to a string literal by the magic-constants pass
/// before the resolver runs, so the path folder sees a plain
/// `BinaryOp(StringLiteral, Concat, StringLiteral)`.
#[test]
fn test_include_with_dunder_file_dir_const() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nconst BASE = __DIR__ . '/lib';\nrequire BASE . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is declared inside a
/// namespace (`namespace App; const LIB = 'lib';`). The name resolver applies
/// the namespace scope, so the const reference resolves correctly.
#[test]
fn test_include_with_namespaced_const_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nconst LIB = 'lib';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies `require LIB . '/inner.php'` where `LIB` is imported via
/// `use const Config\LIB` from an included config file. The `use const`
/// directive makes the const available in the importing file's scope for
/// path resolution.
#[test]
fn test_include_with_const_import_ref() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nuse const Config\\LIB;\nrequire 'config.php';\nrequire LIB . '/inner.php';\necho 'after';\n",
            ),
            (
                "config.php",
                "<?php\nnamespace Config;\nconst LIB = 'lib';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies that a const declared in one namespace is not accessible from
/// another namespace when used in an include path. The require must fail
/// because `LIB` in namespace `B` refers to a non-existent const in that scope.
#[test]
fn test_include_does_not_use_const_from_other_namespace() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nnamespace A;\nconst LIB = 'lib';\nnamespace B;\nrequire LIB . '/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies that a `define()` call inside a function does not feed a
/// top-level require. Constants defined inside a function have local scope
/// and cannot be referenced by statements outside that function.
#[test]
fn test_define_inside_function_does_not_feed_top_level_include() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nfunction boot() {\n    define('LIB', 'lib');\n}\nrequire LIB . '/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies that a namespaced `Config\define()` call does not feed a require
/// that references `LIB` at the top level. The callable `Config\define` is not
/// the global `define`, so the const is not set and the require fails.
#[test]
fn test_qualified_define_call_does_not_feed_include() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nnamespace App;\nConfig\\define('LIB', 'lib');\nrequire LIB . '/inner.php';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies that a `define()` inside a function can feed a `require` that is
/// also inside the same function. Function-local defines are in scope for
/// statements within that function body.
#[test]
fn test_define_inside_function_can_feed_include_in_same_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nfunction boot() {\n    define('LIB', 'lib');\n    require LIB . '/inner.php';\n}\nboot();\necho 'after';\n",
            ),
            (
                "lib/inner.php",
                "<?php\necho 'inner';\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "innerafter");
}

/// Verifies that when an included file defines a function with an untyped
/// parameter, calling it with a specialized array from `load_items()` produces
/// a specialized variant. The `append_item` function is called with array
/// type info, so its `count($items) > 0` guard is not folded away.
#[test]
fn test_include_function_variant_specializes_untyped_array_param_from_call() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\n$items = load_items();\n$items = append_item($items, \"c\");\necho count($items) . ':' . $items[2];\n",
            ),
            (
                "lib.php",
                "<?php\nfunction load_items() {\n    return [\"a\", \"b\"];\n}\n\nfunction append_item($items, $value) {\n    if (count($items) > 0) {\n        $items[] = $value;\n    }\n    return $items;\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "3:c");
}

/// Verifies that when an included file defines a function with an untyped
/// parameter, calling it with a specialized string from `trim()` produces a
/// specialized variant. The `describe_text` function is called with string type
/// info, enabling inlining and DCE of the strlen branch.
#[test]
fn test_include_function_variant_specializes_untyped_string_param_from_call() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\necho describe_text(trim(\" hello \"));\n",
            ),
            (
                "lib.php",
                "<?php\nfunction describe_text($input) {\n    if (strlen($input) === 0) {\n        return \"empty\";\n    }\n    return \"len=\" . strlen($input);\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "len=5");
}

/// Verifies that when an included file defines a function with an untyped
/// parameter, calling it with an integer argument (which cannot be passed to
/// strlen) produces a compile error. The specialized variant is not available
/// for that type, and no valid fallback exists.
#[test]
fn test_include_function_variant_keeps_error_when_call_does_not_respecialize() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\necho describe_text(123);\n",
            ),
            (
                "lib.php",
                "<?php\nfunction describe_text($input) {\n    return strlen($input);\n}\n",
            ),
        ],
        "main.php",
    ));
}

/// Verifies a runtime include sees a by-value parameter the function WROTE before including.
///
/// Written `mixed` and array parameters are re-bound to a private copy on entry, and that copy
/// lived in a differently named slot the include's scope bridge never looked up: the included
/// file read the pre-write value (`part` instead of `part.php`, `a` instead of `a,added`). This
/// is the shape of Symfony's generated `Container::load()`, where the stale slot then crashed
/// `--web` workers. `php -n` 8.5 prints the expected line.
#[test]
fn test_runtime_include_sees_written_by_value_params() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php
function viaMixed($file) {
    $file .= '.php';
    return require __DIR__ . '/' . $file;
}
function viaArray(array $items, $path) {
    $items[] = 'added';
    return require $path;
}
echo viaMixed('part'), '|', viaArray(['a'], __DIR__ . '/items.php');",
            ),
            ("part.php", "<?php return 'included:' . $file;"),
            ("items.php", "<?php return implode(',', $items);"),
        ],
        "main.php",
    );
    assert_eq!(out, "included:part.php|a,added");
}

/// Verifies a `?int` parameter reached through a dynamic static call keeps its tag.
///
/// A function that can `require` exposes its locals, so `$class::take($n, ...)` hands the
/// runtime callable invoker by-reference argument cells. Coercing such a cell to the `?int`
/// payload/tag pair and then releasing the temporary box restored only the payload register: the
/// tag was whatever the release call left behind, and php's `n=42 none=NULL` printed `n= none=`.
/// In Symfony's generated container the same garbage tag crashed `--web` workers.
#[test]
fn test_dynamic_static_call_keeps_nullable_int_tag_through_ref_cells() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php
namespace App;

class Target {
    public static function take(?int $x, $label) {
        return $label . '=' . var_export($x, true);
    }
}

function run($class, $n, $file) {
    if ($file !== '') {
        require __DIR__ . '/' . $file;
    }
    $out = $class::take($n, 'n');
    $none = null;
    $out .= ' ' . $class::take($none, 'none');
    return $out;
}

$class = $argc > 5 ? 'Nope' : __NAMESPACE__ . '\\\\Target';
$n = $argc > 5 ? 1 : 42;
echo run($class, $n, ''), '|', run($class, $n, 'empty.php');",
            ),
            ("empty.php", "<?php\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "n=42 none=NULL|n=42 none=NULL");
}

/// Verifies an include evaluated first inside a condition or expression is compiled, with php's
/// values: the file's `return` on a first `require_once`, `true` on a repeat, and caller scope.
///
/// Composer's `autoload_runtime.php` opens with `if (true === (require_once __DIR__.'/autoload.php')
/// || …)`; left as a runtime include, the whole Composer bootstrap ran in the interpreter on every
/// Symfony request. An include that an operand with an effect precedes is compiled too, the
/// operand moved ahead of it: a closed-world binary, which has no interpreter, answered
/// `strtoupper(…) . include …` with php's "Failed to open stream" warning. php reads a bare variable
/// operand only after the include (`$a` shows what the file assigned), an array element before it.
/// Includes that sit where they may not run (the right of `||`) keep their runtime form. Expected
/// output is php 8.5's.
///
/// The output alone cannot tell a compiled include from an interpreted one (both are correct), so
/// the binary also runs with `ELEPHC_EVAL_TRACE=1`: no included file may reach the interpreter.
#[test]
fn test_include_evaluated_first_in_an_expression_is_compiled_with_php_values() {
    let files: &[(&str, &str)] = &[
            (
                "main.php",
                r#"<?php
if (true === (require_once __DIR__ . '/lib.php') || $argc > 99) {
    echo "first:skipped|";
} else {
    echo "first:ran|";
}
echo $fromLib ?? 'unset', "|";
if (true === (require_once __DIR__ . '/lib.php') || $argc > 99) {
    echo "second:skipped|";
}
echo "a" . (require __DIR__ . '/side.php') . "|";
if (!(include __DIR__ . '/side.php')) {
    echo "never|";
}
if ($argc > 0 || (require __DIR__ . '/side.php')) {
    echo "short-circuit|";
}
echo strtoupper("x") . (include __DIR__ . '/side.php'), "|";
$a = "old";
$b = ["k" => "old"];
echo $a . (include __DIR__ . '/write.php'), "|";
echo $b["k"] . strtolower("Y") . (include __DIR__ . '/dim.php'), "|", $b["k"];
"#,
            ),
            ("lib.php", "<?php echo '[lib]'; $fromLib = 'set-by-lib'; return 5;"),
            ("side.php", "<?php echo '[side]'; return 'S';"),
            ("write.php", "<?php $a = 'new'; return '!';"),
            ("dim.php", "<?php $b['k'] = 'set'; return '!';"),
    ];
    let dir = make_cli_test_dir("elephc_hoisted_include");
    for (path, content) in files {
        std::fs::write(dir.join(path), content).unwrap();
    }
    let main_path = dir.join("main.php");
    let compile = elephc_cli_command(&dir).arg(&main_path).output().expect("failed to run elephc CLI");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = std::process::Command::new(main_path.with_extension(""))
        .current_dir(&dir)
        .env("ELEPHC_EVAL_TRACE", "1")
        .output()
        .expect("failed to run the compiled program");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[lib]first:ran|set-by-lib|second:skipped|[side]aS|[side]short-circuit|[side]XS|new!|oldy!|set"
    );
    let trace = String::from_utf8_lossy(&run.stderr);
    let interpreted = |file: &str| {
        trace
            .lines()
            .filter(|line| line.contains("kind=include") && line.contains(file))
            .count()
    };
    assert_eq!(interpreted("lib.php"), 0, "lib.php reached the interpreter:\n{trace}");
    assert_eq!(interpreted("side.php"), 0, "side.php reached the interpreter:\n{trace}");
    assert_eq!(interpreted("write.php"), 0, "write.php reached the interpreter:\n{trace}");
    assert_eq!(interpreted("dim.php"), 0, "dim.php reached the interpreter:\n{trace}");
}

/// A `require` whose path folds to a stream-wrapper URL (`phar://…`) stays a RUNTIME include:
/// php opens it through the wrapper when the statement executes, and a `require` that never
/// executes never fails. `phpstan/phpstan`'s Composer bootstrap requires
/// `'phar://' . __DIR__ . '/phpstan.phar/vendor/autoload.php'` inside a branch a web request never
/// takes, and the build used to fail with "Required file not found". Expected output is php's.
#[test]
fn test_stream_wrapper_require_in_an_untaken_branch_does_not_stop_the_build() {
    let out = compile_and_run(
        r#"<?php
function load_tool() { return require 'phar://' . __DIR__ . '/tool.phar/vendor/autoload.php'; }
if ($argc > 5) {
    require_once 'phar:///nonexistent/tool.phar/bootstrap.php';
    load_tool();
}
echo "ok\n";
"#,
    );
    assert_eq!(out, "ok\n");
}

/// A static property assigned from an include in VALUE position, inside `??`, type-checks and
/// runs: `self::$data ?? self::$data = require __DIR__ . '/data.php'` is how
/// `symfony/polyfill-intl-icu` loads its currency tables. The include contributes an
/// `IncludeOnceMark` to the assignment's prelude, and the checker panicked with "non-assignment
/// statement routed to assignment checker". Expected output is php 8.5's.
#[test]
fn test_static_property_assigned_from_a_value_include_inside_coalesce() {
    let out = compile_and_run_files(
        &[
            ("data.php", "<?php\nreturn ['a' => 'x', 'b' => 'y'];\n"),
            (
                "main.php",
                r#"<?php
class C {
    private static $data;
    public static function get(string $k) {
        $d = self::$data ?? self::$data = require __DIR__ . '/data.php';
        return $d[$k];
    }
}
echo C::get("a"), C::get("b"), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "xy\n");
}

/// An `include` of a URL whose scheme a userspace stream wrapper registered reads the source
/// through the wrapper object -- `stream_open()`, `stream_read()` until `stream_eof()` -- and runs
/// it, from compiled code and from interpreted code alike.
///
/// Serialized-closure libraries restore a closure exactly this way: their wrapper's
/// `stream_open()` turns `scheme://function (...) {...}` into `<?php return function ...;` and the
/// include hands the closure back. The runtime include read only the disk, so the wrapper URL was
/// "No such file or directory", `include` answered false, and the following `->bindTo()` failed.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_include_reads_a_url_through_a_registered_userspace_wrapper() {
    let out = compile_and_run(
        r#"<?php
class MemStream
{
    const PROTO = 'mem';
    protected static $registered = false;
    public $context;
    protected $content;
    protected $length;
    protected $pointer = 0;
    public function stream_open($path, $mode, $options, &$opened_path)
    {
        $this->content = "<?php\nreturn " . substr($path, strlen(static::PROTO . '://')) . ';';
        $this->length = strlen($this->content);
        return true;
    }
    public function stream_read($count)
    {
        $value = substr($this->content, $this->pointer, $count);
        $this->pointer += $count;
        return $value;
    }
    public function stream_eof() { return $this->pointer >= $this->length; }
    public function stream_set_option($option, $arg1, $arg2) { return false; }
    public function stream_stat() { return []; }
    public static function register()
    {
        if (! static::$registered) {
            static::$registered = stream_wrapper_register(static::PROTO, __CLASS__);
        }
    }
}
class Holder
{
    public $closure;
    public function restore($code, $arg)
    {
        MemStream::register();
        $this->closure = include MemStream::PROTO . '://' . $code;
        $this->closure = $this->closure->bindTo(null, null);
        return ($this->closure)($arg);
    }
}
$h = new Holder();
echo $h->restore('function ($x) { return $x * 2; }', 21), "\n";
eval('class EvalHolder { public function restore($code, $arg) {
    MemStream::register();
    $closure = include MemStream::PROTO . "://" . $code;
    $closure = $closure->bindTo(null, null);
    return $closure($arg);
} }');
$e = new EvalHolder();
echo $e->restore('function ($x) { return $x . "!"; }', "hi"), "\n";
"#,
    );
    assert_eq!(out, "42\nhi!\n");
}

/// A literal `eval()` whose code holds an include in VALUE position compiles and runs.
///
/// The literal fragment is parsed and planned for AOT long after the resolver expanded the
/// program's includes, so its `return include $p;` reached the optimizer unexpanded, and the
/// optimizer treats that node as unreachable: the compiler panicked with "ExprKind::IncludeValue
/// must be expanded by the resolver". Such a fragment now runs through the bridge.
///
/// Oracle: `php 8.5.10`.
#[test]
fn test_value_include_inside_a_literal_eval_body_compiles() {
    let out = compile_and_run_files(
        &[
            ("seven.php", "<?php\nreturn 7;\n"),
            (
                "main.php",
                r#"<?php
eval('function load_value($p) { return include $p; }');
echo load_value(__DIR__ . '/seven.php'), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "7\n");
}

/// Issue #476 review follow-up: an include inside a CLOSURE in a `for` clause is DEFERRED, so
/// the clause's include restriction must not reach it.
///
/// `include`/`require` cannot run as the clause itself — the resolver expands a value-include
/// by rewriting the surrounding statement LIST, which a clause is not — and the parser rejects
/// those two shapes with a named diagnostic. A closure body is different: its include runs when
/// the closure is CALLED, after the loop. The first version of the guard reused the resolver's
/// `has_includes`, which descends into closure bodies, and rejected a program PHP accepts.
///
/// The ordering in the expected output is the proof: the loop body and `compiled` print first,
/// and the included file's own output appears only at the call. Measured on PHP 8.5.10.
#[test]
fn test_for_clause_allows_a_deferred_include_inside_a_closure() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nfor ($f = function () { include 'inner.php'; }, $i = 0; $i < 1; $i++) {\n\
                 echo 'body ', $i, \"\\n\";\n}\necho \"compiled\\n\";\n$f();\necho \"|called\\n\";\n",
            ),
            ("inner.php", "<?php\necho 'inc';\n"),
        ],
        "main.php",
    );
    assert_eq!(out, "body 0\ncompiled\ninc|called\n");
}


/// An include-variant declaration is not "never called" just because its generated symbol is not
/// the name the call site wrote.
///
/// A function declared in an included file is registered under
/// `__elephc_include_variant_{hash}_{local}`, while the call site names it as the source wrote
/// it. `widen_dynamic_only_passthrough_returns` compares those two, so a variant whose body
/// returns one of its parameters looked like a function no caller had ever touched — its return
/// was widened to `mixed` while the caller still expected the specialized type, and
/// `count($items)` printed a raw pointer instead of `3`.
///
/// `append_item` is the shape that matters: an untyped parameter returned straight back, with a
/// real call site that passes it an array. `load_items` sits beside it as the control — it takes
/// no parameters at all, so the rule must not reach it for a different reason.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_include_variant_returning_its_parameter_keeps_the_call_site_type() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire 'lib.php';\n$items = load_items();\n$items = append_item($items, \"c\");\necho count($items) . ':' . $items[2];\n",
            ),
            (
                "lib.php",
                "<?php\nfunction load_items() {\n    return [\"a\", \"b\"];\n}\n\nfunction append_item($items, $value) {\n    if (count($items) > 0) {\n        $items[] = $value;\n    }\n    return $items;\n}\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "3:c");
}

/// An included file whose one line runs past column 65535 must compile (#1292).
///
/// Included spans pack their source identity into the end column's high bits, which left 16 bits
/// for the column, and a wider column aborted the compiler. Including the file twice gives two
/// source identities past that column, so both copies take the interned encoding. The output
/// only proves that the program compiles and runs correctly; that the two copies stay distinct
/// map keys is pinned by the unit tests in `src/span.rs`. Expected output is the host PHP 8.5.10
/// output for the same fixture.
#[test]
fn test_included_line_wider_than_16_bit_columns_compiles() {
    let row = format!(
        "<?php{}$rows[] = [strlen($word), str_repeat($word, 2)];\n",
        " ".repeat(70_000)
    );
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$rows = [];\n$word = 'ab';\ninclude __DIR__ . '/row.php';\n$word = 'xyz';\ninclude __DIR__ . '/row.php';\nforeach ($rows as $r) { echo $r[0], ' ', $r[1], \"\\n\"; }\n",
            ),
            ("row.php", row.as_str()),
        ],
        "main.php",
    );
    assert_eq!(out, "2 abab\n3 xyzxyz\n");
}

/// A diagnostic reported past column 65535 of an included line names the real column (#1292).
///
/// The column survives the interned span encoding: an undefined call that starts at column
/// 70006 is reported there, not at a truncated or wrapped value.
#[test]
fn test_included_line_diagnostic_past_16_bit_columns_reports_the_real_column() {
    let dir = make_cli_test_dir("elephc_wide_column_diagnostic");
    std::fs::write(
        dir.join("row.php"),
        format!("<?php{}undefined_wide_fn();\n", " ".repeat(70_000)),
    )
    .expect("failed to write the wide included line");
    std::fs::write(dir.join("main.php"), "<?php\ninclude __DIR__ . '/row.php';\n")
        .expect("failed to write the including file");

    let output = elephc_cli_command(&dir)
        .arg("main.php")
        .output()
        .expect("failed to run elephc");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the undefined call must be refused: {stderr}");
    assert!(
        stderr.contains(":70006]: Undefined function: undefined_wide_fn"),
        "the diagnostic must name column 70006: {stderr}"
    );
}

/// A checker error inside an included file names that file (issue #1445).
///
/// The checker reports only a span, and `error[3:12]` alone read as a position in the entry
/// file. The span's source identity now resolves to the included file's path.
#[test]
fn test_checker_error_in_an_included_file_names_the_file() {
    let dir = make_cli_test_dir("elephc_included_file_diagnostic");
    fs::write(
        dir.join("lib.php"),
        "<?php\nfunction lib_run(): int {\n    return undefined_lib_fn();\n}\n",
    )
    .expect("failed to write the included file");
    fs::write(dir.join("main.php"), "<?php\nrequire __DIR__ . '/lib.php';\necho lib_run();\n")
        .expect("failed to write the including file");

    let output = elephc_cli_command(&dir)
        .arg("--check")
        .arg("main.php")
        .output()
        .expect("failed to run elephc");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert!(!output.status.success(), "the undefined call must be refused: {stderr}");
    assert!(
        stderr.contains("lib.php:3:12]: Undefined function: undefined_lib_fn"),
        "the diagnostic must name the included file: {stderr}"
    );
}

/// A lowering refusal inside an included file names that file, not the entry file (#1445).
///
/// The pipeline tags every lowering refusal with the entry filename, which printed the included
/// file's line and column as `error[main.php:6:5]`. `--emit-ir` lowers the program without
/// assembling it, which is enough to reach the refusal.
#[test]
fn test_lowering_error_in_an_included_file_names_the_file() {
    let dir = make_cli_test_dir("elephc_included_file_lowering_diagnostic");
    fs::write(
        dir.join("lib.php"),
        "<?php\nclass ValueOnlyHolder { public array $items = [1]; }\nfunction valueOnlySource(ValueOnlyHolder $holder): array { return $holder->items; }\nfunction bindValueOnly(): void {\n    $holder = new ValueOnlyHolder();\n    $alias = &valueOnlySource($holder);\n    echo count($alias);\n}\n",
    )
    .expect("failed to write the included file");
    fs::write(dir.join("main.php"), "<?php\nrequire __DIR__ . '/lib.php';\nbindValueOnly();\n")
        .expect("failed to write the including file");

    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg("main.php")
        .output()
        .expect("failed to run elephc");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert!(!output.status.success(), "the reference assignment must be refused: {stderr}");
    assert!(
        stderr.contains("lib.php:6:5]: Unsupported reference assignment"),
        "the refusal must name the included file: {stderr}"
    );
    assert!(!stderr.contains("main.php:6:5]"), "the refusal must not name the entry file: {stderr}");
}

/// Verifies an anonymous class declared in an included file. The resolver parses the file once to
/// discover its declarations and again to inline its body; both parses must give the class one
/// name, or the `new` in the inlined body refers to a class discovery never declared.
#[test]
fn test_anonymous_class_in_included_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nrequire __DIR__ . '/lib/sink.php';\n$sink->write('main');\n",
            ),
            (
                "lib/sink.php",
                "<?php\ninterface Sink { public function write(string $s): void; }\n$sink = new class () implements Sink { public function write(string $s): void { echo '[', $s, ']'; } };\n$sink->write('included');\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[included][main]");
}

/// Verifies a file with an anonymous class required twice: each require runs the body and
/// constructs an object, where the two parses' identical declarations collided as duplicates.
#[test]
fn test_anonymous_class_in_file_required_twice() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$objects = [];\nrequire __DIR__ . '/lib/make.php';\nrequire __DIR__ . '/lib/make.php';\necho count($objects), ':', $objects[0]->n() + $objects[1]->n();\n",
            ),
            (
                "lib/make.php",
                "<?php\n$objects[] = new class () { public function n(): int { return 7; } };\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "2:14");
}

/// Verifies an anonymous class in a file required under two spellings of its path. Discovery
/// merges the two by canonical path, so both must name the class alike, or the branch that
/// spells the path differently constructs a class that was never declared.
#[test]
fn test_anonymous_class_in_file_required_under_two_path_spellings() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\nif ($argc > 1) {\n    require __DIR__ . '/lib/sink.php';\n} else {\n    require __DIR__ . '/lib/../lib/sink.php';\n}\n$sink->write('main');\n",
            ),
            (
                "lib/sink.php",
                "<?php\n$sink = new class () { public function write(string $s): void { echo '[', $s, ']'; } };\n$sink->write('included');\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "[included][main]");
}

/// An `include_once` inside a closure declares the included interface when the closure runs, and
/// only then. Both whole-program scans stopped at statements: the activation event inside the
/// closure was missed, so the interface got a second, top-level event and the call raised
/// "Cannot redeclare interface", and the file was not registered as a source, which failed EIR
/// validation with `UnresolvedSource`. Symfony's compiled container keeps its service factories
/// exactly this way.
#[test]
fn test_include_once_inside_a_closure_declares_its_interface_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php require __DIR__ . '/lib/Box.php'; echo interface_exists('BagI', false) ? 'early|' : 'late|'; $b = new Box; echo ($b->privates['bag'])(), '|', ($b->privates['bag'])(), \"\\n\";",
            ),
            (
                "lib/Box.php",
                "<?php class Box { public array $privates = []; public function __construct() { $this->privates['bag'] = static function () { include_once \\dirname(__DIR__) . '/lib/BagI.php'; return interface_exists('BagI', false) ? 'yes' : 'no'; }; } }",
            ),
            ("lib/BagI.php", "<?php interface BagI { public function all(): array; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "late|yes|yes\n");
}
