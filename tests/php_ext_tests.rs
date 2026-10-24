//! Purpose:
//! End-to-end tests for hosted PHP extensions: `elephc extension add` builds the
//! `elephc_demo` fixture extension from source, and compiled programs call it.
//!
//! Called from:
//! - `cargo test --test php_ext_tests`.
//!
//! Key details:
//! - Nothing is downloaded: the fixture is built with the system C compiler
//!   against Elephc's own extension headers and linked with the elephc-zend
//!   engine (`libelephc_zend.a`), built on demand in a source checkout.
//! - `demo_program.expected` is the output of real PHP running the same
//!   program with the fixture loaded (`php -d extension=elephc_demo.so`), so a
//!   hosted call that differs from PHP fails here, not just one that crashes.
//! - Each test works in its own temporary project, so the manifest and lock it
//!   writes never touch the repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};



/// The path of the `elephc` binary built for these integration tests.
fn elephc() -> &'static str {
    env!("CARGO_BIN_EXE_elephc")
}

/// The directory holding the PHP extension fixtures (`tests/fixtures/php_ext`).
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/php_ext")
}

/// A fresh project directory with the fixture extension added as a path source.
fn project(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("elephc-extension-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let output = Command::new(elephc())
        .current_dir(&dir)
        .args(["extension", "add", "elephc_demo", "--path"])
        .arg(fixture_dir().join("demo"))
        .output()
        .expect("run elephc extension add");
    assert_success(&output, "extension add");
    dir
}

/// Asserts a process finished successfully, printing its exit code, stdout and stderr in the
/// failure message that names the step.
fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed ({:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Compiles `source` as `main.php` in `dir` and runs the binary.
fn compile_and_run(dir: &Path, source: &str) -> Output {
    let main = dir.join("main.php");
    fs::write(&main, source).unwrap();
    let compile = Command::new(elephc()).current_dir(dir).arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile");
    Command::new(dir.join("main")).current_dir(dir).output().expect("run the compiled program")
}

/// Every value path of the fixture matches real PHP, line for line.
#[test]
fn hosted_calls_match_real_php() {

    let dir = project("oracle");
    let program = fs::read_to_string(fixture_dir().join("demo_program.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("demo_program.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    fs::remove_dir_all(dir).unwrap();
}

/// `add` reports the surface and names the function it cannot call yet;
/// `list` shows the extension as built.
#[test]
fn add_and_list_report_the_surface() {
    let dir = project("list");
    let list = Command::new(elephc()).current_dir(&dir).args(["extension", "list"]).output().unwrap();
    assert_success(&list, "extension list");
    let text = String::from_utf8_lossy(&list.stdout);
    assert!(text.contains("elephc_demo 1.2.3 — 33 functions, 8 classes, 3 constants"), "{text}");
    assert!(text.contains("not callable yet: demo_inspect()"), "{text}");
    assert!(!text.contains("demo_apply()"), "callables are passed: {text}");
    let manifest = fs::read_to_string(dir.join("elephc.toml")).unwrap();
    assert!(!manifest.contains("php-src"), "nothing from php-src is declared: {manifest}");
    assert!(manifest.contains("elephc_demo = { path ="), "{manifest}");
    fs::remove_dir_all(dir).unwrap();
}

/// `[extension.ini]` is applied before the extension starts, as php.ini is.
#[test]
fn ini_directives_reach_the_extension() {
    let dir = project("ini");
    let mut manifest = fs::read_to_string(dir.join("elephc.toml")).unwrap();
    manifest.push_str("\n[extension.ini]\n\"elephc_demo.greeting\" = \"Bonjour\"\n");
    fs::write(dir.join("elephc.toml"), manifest).unwrap();
    let run = compile_and_run(&dir, "<?php echo demo_greet('Ada'), \"\\n\";");
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Bonjour, Ada.\n");
    fs::remove_dir_all(dir).unwrap();
}

/// A warning is reported and the call returns; an E_ERROR ends the program
/// with PHP's exit status; a function taking an object of the program's
/// own classes explains itself.
#[test]
fn diagnostics_follow_php() {
    let dir = project("diagnostics");
    let run = compile_and_run(
        &dir,
        r#"<?php
var_dump(demo_warn("fire"));
var_dump(@demo_warn("hidden"));
demo_warn("pair");
try {
    demo_inspect(new ArrayObject([]));
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
echo "before\n";
demo_fatal();
echo "after\n";
"#,
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(255), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.starts_with("bool(true)\nbool(true)\n"), "{stdout}");
    assert!(stdout.contains("demo_inspect() cannot be called from Elephc yet"), "{stdout}");
    assert!(stdout.contains("before\n") && !stdout.contains("after"), "{stdout}");
    assert!(stderr.contains("Warning: demo_warn(): careful with fire"), "{stderr}");
    // `@` silences an extension's warning exactly like one Elephc raises.
    assert!(!stderr.contains("careful with hidden"), "{stderr}");
    // php_error_docref2's parameters print inside the parentheses, as in PHP.
    assert!(stderr.contains("Warning: demo_warn(left,right): paired"), "{stderr}");
    assert!(stderr.contains("Fatal error: demo_fatal(): the fixture gave up"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// Many calls returning strings, arrays, objects and arrays holding objects
/// leave the heap flat: the engine copies are freed with each call and Elephc
/// owns one copy of each result.
#[test]
fn repeated_calls_do_not_grow_the_heap() {
    let dir = project("heap");
    let main = dir.join("main.php");
    fs::write(
        &main,
        r#"<?php
$total = 0;
for ($i = 0; $i < 300; $i++) {
    $total += strlen(demo_greet("Ada"));
    $total += count(demo_split("a,b,c"));
    $total += demo_object("xy")->length;
    $total += count(demo_echo(["k" => [1, "two", 3.5]]));
    $total += count(demo_nested());
}
echo $total, "\n";
"#,
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).args(["--heap-debug"]).arg(&main).output().unwrap();
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main")).output().unwrap();
    assert_success(&run, "compiled program");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// Objects, properties, factories, iterations (drained and left early),
/// callables (returning and throwing) and optional parameters, repeated, leave
/// both heaps flat: the engine's block count does not move once warmed up, its
/// tables are empty again, and Elephc's heap debug reports no leak.
#[test]
fn classes_iterations_and_callables_do_not_leak() {
    let dir = project("leaks");
    let main = dir.join("main.php");
    fs::write(
        &main,
        r#"<?php
extern function elephc_php_ext_live_calls(): int;
extern function elephc_php_ext_live_entries(int $kind): int;
extern function elephc_zend_live_blocks(): int;
function cycle(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $counter = new DemoCounter($i);
        $counter->add(1)->add(2);
        $sum += $counter->value();
        $counter->label = "x" . $i;
        $sum += strlen($counter->label);
        [$low, $high] = $counter->split();
        $sum += $low->merge($high)->value();
        $sum += strlen(DemoToken::issue("t" . $i)->id());
        foreach (new DemoRange(3) as $key => $value) {
            $sum += $value + strlen($key);
        }
        foreach (new DemoRange(5) as $value) {
            if ($value > 10) {
                break;
            }
        }
        $sum += count(demo_apply(fn ($value, $key) => $value + $key, [1, 2, 3]));
        try {
            demo_apply(function ($value) {
                throw new RuntimeException("stop");
            }, [1]);
        } catch (RuntimeException $e) {
            $sum++;
        }
        $sum += count(demo_slice([1, 2, 3], 1));
        $copy = clone $counter;
        $printed = print_r($copy, true);
        ob_start();
        var_dump($copy);
        $printed .= ob_get_clean() . var_export($copy, true);
        if (str_contains($printed, "__elephc")) {
            echo "wrapper bookkeeping shown\n";
        }
        $sum += $copy->add(1)->value() + strlen($printed);
        $range = new DemoRange(1);
        try {
            $sum += count(iterator_to_array(clone $range));
        } catch (Error $e) {
            $sum++;
        }
        // A constructor the engine refuses: the engine object created first is released.
        try {
            $refused = new DemoCounter("x" . $i);
        } catch (TypeError $e) {
            $sum++;
        }
    }
    return $sum;
}
cycle(20);
$blocks = elephc_zend_live_blocks();
$sum = cycle(200);
echo $sum > 0 ? "ran" : "empty", " ", elephc_zend_live_blocks() - $blocks, " ", elephc_php_ext_live_calls(), " ",
    elephc_php_ext_live_entries(0), " ", elephc_php_ext_live_entries(1), " ", elephc_php_ext_live_entries(2), "\n";
"#,
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).args(["--heap-debug"]).arg(&main).output().unwrap();
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main")).output().unwrap();
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ran 0 0 0 0 0\n", "engine blocks, calls, objects, iterations, callables");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// `var_dump()` and `print_r()` show an extension object's debug table, as PHP
/// does, at the top level and inside an array, and never the wrapper's handle;
/// the class has no `__debugInfo()` method of its own, as in PHP.
#[test]
fn dumps_show_the_engine_object() {
    let dir = project("dumps");
    let run = compile_and_run(
        &dir,
        r#"<?php
$c = new DemoCounter(3);
$c->label = "hi";
var_dump($c);
print_r($c);
echo "\n";
var_dump([$c]);
var_dump(method_exists($c, "__debugInfo"));
"#,
    );
    assert_success(&run, "compiled program");
    let expected = "object(DemoCounter)#1 (2) {\n  [\"label\"]=>\n  string(2) \"hi\"\n  [\"value\"]=>\n  int(3)\n}\n\
DemoCounter Object\n(\n    [label] => hi\n    [value] => 3\n)\n\n\
array(1) {\n  [0]=>\n  object(DemoCounter)#1 (2) {\n    [\"label\"]=>\n    string(2) \"hi\"\n    [\"value\"]=>\n    int(3)\n  }\n}\n\
bool(false)\n";
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    // PHP throws `Call to undefined method DemoCounter::__debugInfo()`; Elephc refuses the
    // call when it compiles it, as it does for any undefined method of a known class.
    let main = dir.join("direct.php");
    fs::write(&main, "<?php\n(new DemoCounter(1))->__debugInfo();\n").unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).arg(&main).output().expect("run elephc");
    let stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(!compile.status.success(), "{stderr}");
    assert!(stderr.contains("Undefined method: DemoCounter::__debugInfo"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// A result Elephc cannot represent (an object of a class other than
/// stdClass, a value containing itself) is refused with a catchable `Error`
/// before any of it is rebuilt.
#[test]
fn unrepresentable_results_are_refused() {
    let dir = project("refused");
    let run = compile_and_run(
        &dir,
        r#"<?php
try {
    demo_error_object();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    demo_cycle();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
echo "done\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a hosted PHP extension returned a value of type DemoParseException, which Elephc cannot represent yet\n\
         a hosted PHP extension returned a recursive object, which Elephc cannot represent yet\n\
         done\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Every error path frees the call it prepared: an extension exception after
/// a by-reference write, a refused result, a refused argument, a named
/// argument refused before the call. The host counts calls not yet freed.
///
/// Elephc's own heap is not asserted here: a function left by `throw` does
/// not release its heap locals on the current backend (a pure-PHP
/// `function f(string $s) { throw new Error("x"); }` leaks `$s`), so every
/// exception costs a few blocks whatever the bridge does.
#[test]
fn error_paths_free_the_call() {
    let dir = project("error-paths");
    let run = compile_and_run(
        &dir,
        r#"<?php
extern function elephc_php_ext_live_calls(): int;
$caught = 0;
for ($i = 0; $i < 50; $i++) {
    $left = 1;
    try {
        demo_consume(5, $left);
    } catch (UnderflowException $e) {
        $caught += 1 + $left;
    }
    try {
        demo_error_object();
    } catch (Error $e) {
        $caught++;
    }
    try {
        demo_cycle();
    } catch (Error $e) {
        $caught++;
    }
    try {
        demo_parse("x");
    } catch (DemoException $e) {
        $caught++;
    }
    try {
        demo_echo(new stdClass());
    } catch (TypeError $e) {
        $caught++;
    }
}
echo $caught, " ", elephc_php_ext_live_calls(), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "250 0\n");
    fs::remove_dir_all(dir).unwrap();
}

/// A throwable a callable threw is rethrown only while it is still the
/// exception the extension call ends with: one the extension cleared does not
/// resurface from a later, unrelated extension exception, and one it replaced
/// with its own exception gives way to that exception.
#[test]
fn a_cleared_or_replaced_callback_throwable_is_not_rethrown() {
    let dir = project("stale-throwable");
    let run = compile_and_run(
        &dir,
        r#"<?php
echo demo_try(function () { throw new LogicException("stale"); }, false), "\n";
try {
    demo_parse("x");
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
try {
    demo_try(function () { throw new LogicException("inner"); }, true);
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), " ", $e->getCode(), "\n";
}
echo demo_try(fn () => 1, false), "\n";
try {
    demo_parse("y");
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "cleared\n\
         DemoParseException: 'x' is not an integer\n\
         DemoException: wrapped 7\n\
         returned\n\
         DemoParseException: 'y' is not an integer\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// A variadic `callable ...$callbacks` parameter registers every closure it is
/// given, as a fixed callable parameter does, so the extension can call each
/// one back; the engine holds none of them once the call returns.
#[test]
fn variadic_callable_parameters_pass_closures() {
    let dir = project("variadic-callables");
    let run = compile_and_run(
        &dir,
        r#"<?php
extern function elephc_php_ext_live_entries(int $kind): int;
$base = 10;
print_r(demo_call_all(fn () => 1, function () use ($base) { return $base + 2; }, fn () => "three"));
echo "\n", count(demo_call_all()), " ", elephc_php_ext_live_entries(2), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Array\n(\n    [0] => 1\n    [1] => 12\n    [2] => three\n)\n\n0 0\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Two by-reference parameters bound to the same caller variable share one
/// reference in the engine, as in PHP: a write through the first is read back
/// through the second, and the write-back leaves the variable at the last value
/// written rather than undoing the first. (Expected values are real PHP's.)
#[test]
fn aliased_by_reference_parameters_share_storage() {
    let dir = project("aliased-references");
    let run = compile_and_run(
        &dir,
        r#"<?php
$x = 5;
echo demo_twin($x, $x), " ", $x, "\n";
$a = 1;
$b = 2;
echo demo_twin($a, $b), " ", $a, " ", $b, "\n";
$list = [1, 2];
echo demo_twin($list[0], $list[0]), " ", $list[0], " ", $list[1], "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "10 11\n2 10 3\n10 11 2\n");
    fs::remove_dir_all(dir).unwrap();
}

/// The extension's `ZEND_NUM_ARGS()` is the number of arguments the caller
/// wrote: an optional parameter the caller omitted is not passed on with its
/// default, and a value equal to the old sentinel text is an ordinary argument.
/// (Expected values are real PHP's.)
#[test]
fn omitted_optional_arguments_are_not_forwarded() {
    let dir = project("omitted-arguments");
    let run = compile_and_run(
        &dir,
        r#"<?php
echo demo_argc(), demo_argc(1), demo_argc(1, null), demo_argc(null, null, null), demo_argc(second: 2), "\n";
echo demo_argc("\x01elephc:omitted\x01"), demo_argc(1, "\x01elephc:omitted\x01"), "\n";
echo demo_add(2), demo_add(2, 3), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "01232\n12\n35\n");
    fs::remove_dir_all(dir).unwrap();
}

/// An object the extension returns again comes back as the wrapper that already holds it, as PHP
/// returns the same instance; a clone is its own instance. (Expected values are real PHP's.)
#[test]
fn returned_objects_keep_their_identity() {
    let dir = project("identity");
    let run = compile_and_run(
        &dir,
        r#"<?php
$a = new DemoCounter(3);
demo_keep($a);
$b = demo_kept()[0];
var_dump($a === $b, demo_kept()[0] === $a);
unset($b);
$d = clone $a;
demo_keep($d);
var_dump(demo_kept()[1] === $d, demo_kept()[1] === $a);
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "bool(true)\nbool(true)\nbool(true)\nbool(false)\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// An array keeps its next append key through the extension, and an unknown named argument
/// is refused by a variadic function instead of being passed positionally. (Expected values
/// are real PHP's.)
#[test]
fn append_cursors_and_unknown_named_arguments_follow_php() {
    let dir = project("cursor-named");
    let run = compile_and_run(
        &dir,
        r#"<?php
$a = ["keep" => 1, 10 => 2];
unset($a[10]);
$b = demo_echo($a);
$b[] = 3;
echo implode(",", array_keys($b)), "\n";
$c = [1, 2, 3];
unset($c[2]);
$d = demo_echo($c);
$d[] = 4;
echo implode(",", array_keys($d)), "\n";
try {
    demo_call_all(typo: fn () => 1);
} catch (ArgumentCountError $e) {
    echo $e->getMessage(), "\n";
}
echo count(demo_call_all(fn () => 1, fn () => 2)), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "keep,11\n0,1,3\ndemo_call_all() does not accept unknown named parameters\n2\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// A class whose `count()` arginfo states no return type (as every extension
/// written before PHP 8.1) still implements `Countable`. (Real PHP prints a
/// deprecation at startup for this fixture; the rest matches it.)
#[test]
fn an_arginfo_without_a_return_type_still_implements_the_interface() {
    let dir = project("old-style-countable");
    let run = compile_and_run(
        &dir,
        r#"<?php
$bag = new DemoBag();
echo count($bag), " ", $bag instanceof Countable ? "countable" : "plain", " ", $bag->count(), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3 countable 3\n");
    fs::remove_dir_all(dir).unwrap();
}

/// An extension exception class that declares methods is a real class in the
/// program: `catch` can name it, the thrown exception is rebuilt as it (not as
/// its parent), and its method runs in the engine against the exception's
/// message and code. (Expected values are real PHP's.)
#[test]
fn extension_exceptions_with_methods_are_declared() {
    let dir = project("exception-methods");
    let run = compile_and_run(
        &dir,
        r#"<?php
try {
    demo_throw_detail("boom", 7);
} catch (DemoDetailException $e) {
    echo get_class($e), " | ", $e->describe(), " | ", $e->getMessage(), " | ", $e->getCode(), "\n";
}
try {
    demo_throw_detail("again");
} catch (DemoException $e) {
    echo get_class($e), " | ", $e->getMessage(), "\n";
}
// A hosted subclass runs both its own method and the inherited one on an engine object of its
// own class.
$child = new DemoDetailChildException("kid", 3);
echo $child->tag(), " | ", $child->describe(), "\n";
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "DemoDetailException | DemoDetailException: boom (7) | boom | 7\nDemoDetailException | again\n\
         child:kid | DemoDetailChildException: kid (3)\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Compiles `source` in `dir`, runs it with `ELEPHC_DEMO_TRACE` set, and returns the lines the
/// extension traced on stderr (`RINIT`, `RSHUTDOWN`, `MSHUTDOWN`) with the program's exit status.
fn run_traced(dir: &Path, source: &str) -> (Vec<String>, Option<i32>, String) {
    let main = dir.join("main.php");
    fs::write(&main, source).unwrap();
    let compile = Command::new(elephc()).current_dir(dir).arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main")).env("ELEPHC_DEMO_TRACE", "1").output().expect("run the program");
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    let traced = stderr.lines().filter_map(|line| line.strip_prefix("elephc_demo: ")).map(str::to_string).collect();
    (traced, run.status.code(), String::from_utf8_lossy(&run.stdout).into_owned())
}

/// The extension's request and module lifecycle ends the way php-cli's does: request shutdown
/// (RSHUTDOWN) and then module shutdown (MSHUTDOWN), after the output is flushed, on a normal end
/// and on `exit()` alike, and never for a program that uses no extension function.
#[test]
fn the_extension_is_shut_down_when_the_program_ends() {
    let dir = project("lifecycle");
    let (traced, status, stdout) = run_traced(&dir, "<?php\necho demo_add(1, 2), \"\\n\";\n");
    assert_eq!((status, stdout.as_str()), (Some(0), "3\n"));
    assert_eq!(traced, ["RINIT", "RSHUTDOWN", "MSHUTDOWN"]);

    let (traced, status, stdout) = run_traced(&dir, "<?php\necho demo_add(1, 2), \"\\n\";\nexit(3);\n");
    assert_eq!((status, stdout.as_str()), (Some(3), "3\n"));
    assert_eq!(traced, ["RINIT", "RSHUTDOWN", "MSHUTDOWN"]);

    let (traced, status, _) = run_traced(&dir, "<?php\nfunction fail() { throw new RuntimeException('x'); }\necho demo_add(1, 2), \"\\n\";\nfail();\n");
    assert_eq!(status, Some(255));
    assert_eq!(traced, ["RINIT", "RSHUTDOWN", "MSHUTDOWN"]);

    let (traced, status, stdout) = run_traced(&dir, "<?php\necho \"no extension call\\n\";\n");
    assert_eq!((status, stdout.as_str()), (Some(0), "no extension call\n"));
    assert!(traced.is_empty(), "{traced:?}");
    fs::remove_dir_all(dir).unwrap();
}

/// A relative `--path` is relative to where the command ran, not to the project root: added from
/// a subdirectory it is recorded relative to the project (the manifest resolves it there), and the
/// extension builds and loads from it.
#[test]
fn a_relative_path_source_is_resolved_from_the_working_directory() {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("elephc-extension-relative-{}-{nanos}", std::process::id()));
    let sub = root.join("tools");
    fs::create_dir_all(&sub).unwrap();
    fs::write(root.join("elephc.toml"), "").unwrap();
    // The fixture, copied next to the project as `ext/demo`.
    let copy = root.join("ext/demo");
    fs::create_dir_all(&copy).unwrap();
    for entry in fs::read_dir(fixture_dir().join("demo")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), copy.join(entry.file_name())).unwrap();
    }
    let add = Command::new(elephc())
        .current_dir(&sub)
        .args(["extension", "add", "elephc_demo", "--path", "../ext/demo"])
        .output()
        .expect("run elephc extension add");
    assert_success(&add, "extension add from a subdirectory");
    let manifest = fs::read_to_string(root.join("elephc.toml")).unwrap();
    assert!(manifest.contains("elephc_demo = { path = \"ext/demo\" }"), "{manifest}");
    let run = compile_and_run(&root, "<?php\necho demo_add(40, 2), \"\\n\";\n");
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n");
    fs::remove_dir_all(root).unwrap();
}

/// One `GET /` against `address`, returning the response body.
fn http_body(address: &str) -> String {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(address).expect("connect to the web server");
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response.split_once("\r\n\r\n").map(|(_, body)| body.to_string()).unwrap_or(response)
}

/// A `--web` worker serves one request after another in one process, but an extension's request
/// state belongs to a single request: the next request runs RSHUTDOWN and RINIT first, so what
/// `demo_keep()` stored (destroyed by the demo's RSHUTDOWN, rebuilt empty by its RINIT) does not
/// cross. The capturing closure it also keeps is dropped by that RSHUTDOWN, which runs while the
/// hosting tables the closure calls back into are still alive.
#[test]
fn extension_request_state_does_not_cross_web_requests() {
    use std::os::unix::process::CommandExt;
    let dir = project("web-requests");
    let main = dir.join("main.php");
    fs::write(
        &main,
        "<?php\n$tag = \"t\" . strlen(\"ab\");\ndemo_keep(\"seen\");\necho count(demo_kept()), \"\\n\";\ndemo_keep(function () use ($tag) { return $tag; });\n",
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).arg("--web").arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile --web");
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let address = format!("127.0.0.1:{port}");
    let mut server = Command::new(dir.join("main"))
        .args(["--listen", &address, "--workers", "1"])
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn the web server");
    let group = server.id() as libc::pid_t;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::net::TcpStream::connect(&address).is_err() {
        assert!(std::time::Instant::now() < deadline, "the web server did not start");
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let bodies: Vec<String> = (0..3).map(|_| http_body(&address).trim().to_string()).collect();
    // SAFETY: signals the server's own process group, which this test created.
    unsafe { libc::killpg(group, libc::SIGKILL) };
    let _ = server.wait();
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(bodies, ["1", "1", "1"], "each request starts with an empty kept table");
}

/// A request startup (RINIT) that fails between `--web` requests ends the worker as php-src's
/// `zend_activate_modules` does (`Warning: request_startup() for ... module failed`, then exit),
/// and the module never joins that request, so no RSHUTDOWN runs on state it failed to set up.
#[test]
fn a_failed_request_startup_ends_the_web_worker() {
    use std::os::unix::process::CommandExt;
    let dir = project("web-rinit-failure");
    let main = dir.join("main.php");
    fs::write(&main, "<?php\ndemo_keep(\"seen\");\necho count(demo_kept()), \"\\n\";\n").unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).arg("--web").arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile --web");
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let address = format!("127.0.0.1:{port}");
    let stdout = fs::File::create(dir.join("server.out")).unwrap();
    let stderr = fs::File::create(dir.join("server.err")).unwrap();
    let mut server = Command::new(dir.join("main"))
        .args(["--listen", &address, "--workers", "1"])
        .env("ELEPHC_DEMO_TRACE", "1")
        .env("ELEPHC_DEMO_FAIL_RINIT_AFTER", "1")
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .expect("spawn the web server");
    let group = server.id() as libc::pid_t;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::net::TcpStream::connect(&address).is_err() {
        assert!(std::time::Instant::now() < deadline, "the web server did not start");
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let first = http_body(&address).trim().to_string();
    // The second request's startup fails: the worker exits before answering.
    {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(&address).expect("connect to the web server");
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let _ = stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let _ = stream.read_to_end(&mut Vec::new());
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    // SAFETY: signals the server's own process group, which this test created.
    unsafe { libc::killpg(group, libc::SIGKILL) };
    let _ = server.wait();
    let output = fs::read_to_string(dir.join("server.out")).unwrap() + &fs::read_to_string(dir.join("server.err")).unwrap();
    let traced: Vec<&str> = output.lines().filter_map(|line| line.strip_prefix("elephc_demo: ")).collect();
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(first, "1");
    assert_eq!(traced, ["RINIT", "RSHUTDOWN", "RINIT failed"], "{output}");
    assert!(output.contains("request_startup() for elephc_demo module failed"), "{output}");
}
