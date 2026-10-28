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



/// The path of the `elephc` binary built for these integration tests, read at run time
/// because CI runs an extracted nextest archive where the build-time path does not exist.
fn elephc() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| env!("CARGO_BIN_EXE_elephc").to_string())
}

/// The directory holding the PHP extension fixtures (`tests/fixtures/php_ext`), read at run
/// time for the same reason.
fn fixture_dir() -> PathBuf {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
    Path::new(&root).join("tests/fixtures/php_ext")
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

/// A hosted object's declared properties keep their PHP contract (#2103), line for line with real
/// PHP: private and protected ones are out of reach of the global scope but not of the class's
/// own methods nor (protected) of a program subclass, a readonly one is set once and only from
/// its class's lineage, and a typed one takes only values of its type, converted in weak mode.
#[test]
fn property_contracts_match_real_php() {
    let dir = project("properties");
    let program = fs::read_to_string(fixture_dir().join("property_contracts.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("property_contracts.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    fs::remove_dir_all(dir).unwrap();
}

/// A program hosting an extension compiles with `eval()`, and eval code reaches a hosted
/// object's properties in its own class scope: a protected one from a subclass method, never
/// from the global scope. Line for line with real PHP.
#[test]
fn eval_reaches_hosted_properties_in_its_scope() {
    let dir = project("eval-properties");
    let program = fs::read_to_string(fixture_dir().join("eval_property_contracts.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("eval_property_contracts.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    fs::remove_dir_all(dir).unwrap();
}

/// A program subclass of a hosted class behaves as one of the internal class: it may write an
/// ancestor's private name (a dynamic property of its own), declare `__get`, `__set`, `__isset`,
/// `__unset` and `__clone` with any signature PHP accepts (a clone still gets its own engine object
/// before the program's `__clone()` runs), and an accessor override elsewhere does not change the
/// scope a dynamically dispatched access runs in. Line for line with real PHP.
#[test]
fn program_subclasses_extend_hosted_classes_as_php_does() {
    let dir = project("subclasses");
    let program = fs::read_to_string(fixture_dir().join("hosted_subclasses.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("hosted_subclasses.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    fs::remove_dir_all(dir).unwrap();
}

/// Code declaring `strict_types=1` assigns a hosted typed property as strict code does: only an
/// int converts (to a float property), anything else is a TypeError. Line for line with real PHP.
#[test]
fn strict_code_assigns_hosted_typed_properties_strictly() {
    let dir = project("strict-properties");
    let program = fs::read_to_string(fixture_dir().join("strict_property_contracts.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("strict_property_contracts.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    fs::remove_dir_all(dir).unwrap();
}

/// The host's internals are not PHP symbols: a program naming them at run time (a string
/// callable, a method by name, `new` of a class name) gets what PHP gives for a missing one.
#[test]
fn host_internals_are_not_reachable_by_name() {
    // One program, one case per run (`argv[1]`): a hosted build is slow enough that a compile per
    // case brought the test to the suite's per-test limit.
    let dir = project("internals");
    let run = compile_and_run(
        &dir,
        "<?php
$case = $argv[1] ?? '';
$c = new DemoCounter(3);
if ($case === '') {
    $n = $argc > 5 ? 'x' : 'elephc_php_ext_object_release';
    var_dump(is_callable($n));
    var_dump($c->value());
} elseif ($case === 'function') {
    $n = $argc > 5 ? 'x' : 'elephc_php_ext_object_release';
    $n(1);
    echo 'reached';
} elseif ($case === 'method') {
    $m = $argc > 5 ? 'x' : '__elephcObject';
    var_dump($c->$m());
} else {
    $k = $argc > 5 ? 'x' : '__ElephcPhpExtOwner';
    var_dump(new $k);
}
",
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "bool(false)
int(3)
");
    // Elephc ends the program on these misses where PHP throws a catchable Error; what matters
    // here is that none of them reaches the internal symbol.
    for (case, refused) in [
        ("function", "undefined function"),
        ("method", "callable array"),
        ("class", "Class \"__ElephcPhpExtOwner\" not found"),
    ] {
        let run = Command::new(dir.join("main")).arg(case).current_dir(&dir).output().expect("run the compiled program");
        let all = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
        assert!(!run.status.success() && !all.contains("reached") && !all.contains("int(") && !all.contains("object("), "{case}: {all}");
        assert!(all.contains(refused), "{case}: {all}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// `var_dump()` object ids with the digits dropped: the wrapper identity table holds an id of its
/// own, so a hosted object's `#N` can run ahead of PHP's (see `dumps_show_the_engine_object`).
fn without_object_ids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '#' {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
    }
    out
}

/// A hosted object's properties as PHP's introspection shows them, from the engine object:
/// `get_object_vars()` in the caller's scope, `(array)` under mangled keys, `var_dump()` and
/// `print_r()` in declaration order with their visibility, `property_exists()` and
/// `get_class_vars()`; and a `__debugInfo()` array's mangled keys print unmangled, as PHP does
/// for any class. Line for line with real PHP (object ids aside).
#[test]
fn hosted_property_views_match_real_php() {
    let dir = project("property-views");
    let program = fs::read_to_string(fixture_dir().join("hosted_property_views.php")).unwrap();
    let expected = fs::read_to_string(fixture_dir().join("hosted_property_views.expected")).unwrap();
    let run = compile_and_run(&dir, &program);
    assert_success(&run, "compiled program");
    assert_eq!(without_object_ids(&String::from_utf8_lossy(&run.stdout)), without_object_ids(&expected));
    fs::remove_dir_all(dir).unwrap();
}

/// Introspection sees a hosted class as PHP's internal class: none of the wrapper's own members
/// (`__elephc…` methods, its `__get`/`__set`/`__isset`/`__unset`/`__clone`, the marker
/// interface) is listed or reported, and Reflection lists methods in declaration order.
#[test]
fn introspection_does_not_see_the_wrapper() {
    let dir = project("introspection");
    let run = compile_and_run(
        &dir,
        r#"<?php
$c = new DemoCounter(3);
echo implode(',', get_class_methods($c)), "
";
var_dump(method_exists($c, '__get'), method_exists('DemoCounter', '__elephcObject'), method_exists($c, 'add'));
var_dump(is_callable([$c, '__get']), is_callable([$c, 'add']));
var_dump(class_implements($c), interface_exists('__ElephcPhpExtObject'));
$r = new ReflectionClass('DemoCounter');
foreach ($r->getMethods() as $m) { echo $m->getName(), ','; }
echo "
";
var_dump($r->hasMethod('__clone'), $r->getInterfaceNames());
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "__construct,add,value,merge,fromString,split
bool(false)
bool(false)
bool(true)
bool(false)
bool(true)
array(0) {
}
bool(false)
__construct,add,value,merge,fromString,split,
bool(false)
array(0) {
}
"
    );
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
    assert!(text.contains("elephc_demo 1.2.3 — 36 functions, 9 classes, 3 constants"), "{text}");
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
        // Wrappers inside arrays, passed in and returned by a callable.
        $nested = demo_echo(["item" => $counter, "list" => [1, $counter]]);
        $sum += $nested["list"][1]->value();
        $sum += count(demo_apply(fn ($value) => [$value], [$counter]));
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
        // The engine iteration behind getIterator() cannot be cloned, as PHP's InternalIterator.
        $iterator = (new DemoRange(2))->getIterator();
        try {
            $iteratorCopy = clone $iterator;
        } catch (Error $e) {
            $sum += str_contains($e->getMessage(), "InternalIterator") ? 1 : 0;
        }
        foreach ($iterator as $value) {
            $sum += $value;
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
    elephc_php_ext_live_entries(0), " ", elephc_php_ext_live_entries(1), " ", elephc_php_ext_live_entries(2), " ",
    count(array_filter(__ElephcPhpExtObjects::$wrappers)), " ",
    count(__ElephcPhpExtObjects::$wrappers) <= 64 ? "bounded" : "grows", "\n";
"#,
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).args(["--heap-debug"]).arg(&main).output().unwrap();
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main")).output().unwrap();
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "ran 0 0 0 0 0 0 bounded\n",
        "engine blocks, calls, objects, iterations, callables, live wrapper entries, wrapper table size"
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    fs::remove_dir_all(dir).unwrap();
}

/// The engine object behind a wrapper is released when the wrapper is freed, whatever PHP
/// destructor runs (#2101): a subclass overriding `__destruct()` without calling anything, and a
/// subclass constructor that throws after `parent::__construct()` (PHP never destructs that
/// object), both leave the engine's held objects and blocks at their warmed-up baseline, with the
/// extension's `free_obj` run once per object. A wrapper its destructor resurrects keeps its
/// engine object until it is really gone.
#[test]
fn engine_objects_are_released_whatever_the_destructor_does() {
    let dir = project("owner");
    let main = dir.join("main.php");
    fs::write(
        &main,
        r#"<?php
extern function elephc_php_ext_live_entries(int $kind): int;
extern function elephc_zend_live_blocks(): int;
class Silent extends DemoCounter {
    public function __destruct() {}
}
class Failing extends DemoCounter {
    public function __construct(int $n) {
        parent::__construct($n);
        throw new RuntimeException("refused");
    }
}
class Back extends DemoCounter {
    public static ?Back $kept = null;
    public function __destruct() { self::$kept = $this; }
}
function silent(int $n): void {
    for ($i = 0; $i < $n; $i++) {
        $counter = new Silent($i);
        $counter->add(1);
        unset($counter);
    }
}
function failing(int $n): int {
    $caught = 0;
    for ($i = 0; $i < $n; $i++) {
        try {
            new Failing($i);
        } catch (RuntimeException $e) {
            $caught++;
        }
    }
    return $caught;
}
silent(5);
failing(5);
$freed = demo_counters_freed();
$blocks = elephc_zend_live_blocks();
$held = elephc_php_ext_live_entries(0);
silent(100);
echo demo_counters_freed() - $freed, " ";
$caught = failing(100);
echo demo_counters_freed() - $freed, " ", $caught, " ", elephc_zend_live_blocks() - $blocks, " ",
    elephc_php_ext_live_entries(0) - $held, "\n";
$freed = demo_counters_freed();
$back = new Back(7);
$back->add(1);
unset($back);
echo demo_counters_freed() - $freed, " ", Back::$kept->value(), "\n";
Back::$kept = null;
echo demo_counters_freed() - $freed, "\n";
"#,
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).args(["--heap-debug"]).arg(&main).output().unwrap();
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main")).output().unwrap();
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "100 200 100 0 0\n0 8\n1\n",
        "freed after the silent subclass, after the failing constructors, exceptions caught, engine \
         blocks, held objects; then a resurrected wrapper keeps its object until it is dropped"
    );
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
// A program subclass declaring its own __debugInfo() is dumped through it, as PHP does.
class TwiceCounter extends DemoCounter {
    public function __debugInfo(): array { return ["twice" => $this->value() * 2]; }
}
var_dump(new TwiceCounter(4));
"#,
    );
    assert_success(&run, "compiled program");
    let expected = "object(DemoCounter)#1 (2) {\n  [\"label\"]=>\n  string(2) \"hi\"\n  [\"value\"]=>\n  int(3)\n}\n\
DemoCounter Object\n(\n    [label] => hi\n    [value] => 3\n)\n\n\
array(1) {\n  [0]=>\n  object(DemoCounter)#1 (2) {\n    [\"label\"]=>\n    string(2) \"hi\"\n    [\"value\"]=>\n    int(3)\n  }\n}\n\
bool(false)\n\
object(TwiceCounter)#3 (1) {\n  [\"twice\"]=>\n  int(8)\n}\n";
    // #3, where PHP prints #2: the wrapper identity table's weak reference to `$c` holds an
    // object id (see "Classes" in docs/beyond-php/php-extensions.md).
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

/// Wrappers inside arrays reach the extension as the objects they hold, both as arguments and as
/// what a callable returns, and come back as the same wrappers; an array holding another object
/// is still refused, without affecting the next call.
#[test]
fn objects_inside_arrays_cross_the_boundary() {
    let dir = project("nested-objects");
    let run = compile_and_run(
        &dir,
        r#"<?php
$c = new DemoCounter(2);
$r = demo_echo(["item" => $c, "deep" => [7, $c]]);
var_dump($r["item"] === $c, $r["deep"][1] === $c, $r["deep"][0]);
$m = demo_apply(fn ($value) => ["w" => $value], [$c]);
var_dump($m[0]["w"] === $c);
try {
    demo_echo([new stdClass(), $c]);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo demo_echo([$c])[0]->value(), "\n";
// An array holding an object keeps its next append key both ways, as in PHP.
$a = ["keep" => $c, 10 => 2];
unset($a[10]);
$b = demo_echo($a);
$b[] = 3;
echo implode(",", array_keys($b)), "\n";
$l = [$c, 1, 2];
unset($l[2]);
$l = demo_echo($l);
$l[] = 9;
echo implode(",", array_keys($l)), "\n";
$n = demo_apply(function ($v) use ($c) { $x = [5 => $c, 6 => $v]; unset($x[6]); return $x; }, [1]);
$n[0][] = 0;
echo implode(",", array_keys($n[0])), "\n";
// An occupied PHP_INT_MAX key exhausts the next key: the array still crosses, appends do not.
$x = demo_echo([PHP_INT_MAX => $c]);
var_dump($x[PHP_INT_MAX] === $c);
try {
    $x[] = 1;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
// A value that is not callable reaches the extension's own argument parsing, as in PHP.
function notCallable(int $argc): mixed { return $argc > 5 ? 'strlen' : 42; }
try {
    demo_apply(notCallable($argc), [1]);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_success(&run, "compiled program");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "bool(true)\nbool(true)\nint(7)\nbool(true)\nobjects cannot be passed to a hosted PHP extension yet\n2\n\
         keep,11\n0,1,3\n5,7\n\
         bool(true)\nCannot add element to the array as the next element is already occupied\n\
         demo_apply(): Argument #1 ($callback) must be a valid callback, no array or string given\n"
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
    run_traced_with(dir, source, &[])
}

/// `run_traced` with extra environment variables for the run.
fn run_traced_with(dir: &Path, source: &str, env: &[(&str, &str)]) -> (Vec<String>, Option<i32>, String) {
    let main = dir.join("main.php");
    fs::write(&main, source).unwrap();
    let compile = Command::new(elephc()).current_dir(dir).arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile");
    let run = Command::new(dir.join("main"))
        .env("ELEPHC_DEMO_TRACE", "1")
        .envs(env.iter().copied())
        .output()
        .expect("run the program");
    let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
    let traced = stderr.lines().filter_map(|line| line.strip_prefix("elephc_demo: ")).map(str::to_string).collect();
    (traced, run.status.code(), String::from_utf8_lossy(&run.stdout).into_owned())
}

/// The extension's request and module lifecycle ends the way php-cli's does: request shutdown
/// (RSHUTDOWN), then post-deactivation once the request's resources and INI changes are undone
/// (the greeting a script changed is back to its configured value), then module shutdown
/// (MSHUTDOWN), after the output is flushed, on a normal end and on `exit()` alike, and never for a
/// program that uses no extension function.
#[test]
fn the_extension_is_shut_down_when_the_program_ends() {
    let dir = project("lifecycle");
    let (traced, status, stdout) = run_traced(&dir, "<?php\necho demo_add(1, 2), \"\\n\";\n");
    assert_eq!((status, stdout.as_str()), (Some(0), "3\n"));
    assert_eq!(traced, ["RINIT", "RSHUTDOWN Hello", "POST_DEACTIVATE Hello", "MSHUTDOWN"]);

    let (traced, status, stdout) = run_traced(&dir, "<?php\necho demo_add(1, 2), \"\\n\";\nexit(3);\n");
    assert_eq!((status, stdout.as_str()), (Some(3), "3\n"));
    assert_eq!(traced, ["RINIT", "RSHUTDOWN Hello", "POST_DEACTIVATE Hello", "MSHUTDOWN"]);

    let (traced, status, _) = run_traced(&dir, "<?php\nfunction fail() { throw new RuntimeException('x'); }\necho demo_add(1, 2), \"\\n\";\nfail();\n");
    assert_eq!(status, Some(255));
    assert_eq!(traced, ["RINIT", "RSHUTDOWN Hello", "POST_DEACTIVATE Hello", "MSHUTDOWN"]);

    let (traced, status, stdout) =
        run_traced(&dir, "<?php\nvar_dump(demo_set_greeting('Hi'));\necho demo_greet('a'), \"\\n\";\n");
    assert_eq!((status, stdout.as_str()), (Some(0), "bool(true)\nHi, a.\n"));
    // The request's change holds through its RSHUTDOWN and is undone before post-deactivation.
    assert_eq!(traced, ["RINIT", "RSHUTDOWN Hi", "POST_DEACTIVATE Hello", "MSHUTDOWN"]);

    let (traced, status, stdout) = run_traced(&dir, "<?php\necho \"no extension call\\n\";\n");
    assert_eq!((status, stdout.as_str()), (Some(0), "no extension call\n"));
    assert!(traced.is_empty(), "{traced:?}");
    fs::remove_dir_all(dir).unwrap();
}

/// An engine object still alive when the request ends (held in a cycle) is freed after the
/// request's resources close, as php-src's `shutdown_executor` runs `zend_close_rsrc_list` before
/// freeing the object store: a resource destructor still finds the objects it refers to.
#[test]
fn live_engine_objects_are_freed_after_resources_close() {
    let dir = project("teardown-order");
    let source = "<?php\n$cycle = new stdClass;\n$cycle->self = $cycle;\n$cycle->counter = new DemoCounter(9);\ndemo_open_handle('h');\necho \"end\\n\";\n";
    let (traced, status, stdout) = run_traced(&dir, source);
    assert_eq!((status, stdout.as_str()), (Some(0), "end\n"));
    assert_eq!(
        traced,
        ["RINIT", "RSHUTDOWN Hello", "CLOSE h alive=1", "FREE counter 9", "POST_DEACTIVATE Hello", "MSHUTDOWN"]
    );
    fs::remove_dir_all(dir).unwrap();
}

/// An engine error thrown from a guarded property write inside a method is caught by the caller:
/// on linux-x86_64 the rethrow reached the unwinder by `jmp`, and the destructors it ran called
/// into the engine on a misaligned stack (a segfault with no output).
#[test]
fn a_readonly_write_inside_a_method_throws_a_catchable_error() {
    let dir = project("readonly-in-method");
    let run = compile_and_run(
        &dir,
        "<?php
class Kid extends DemoRecord {
    public function bad(): void { $this->number = 9; }
}
try { (new Kid(3))->bad(); } catch (Throwable $e) { echo $e->getMessage(), \"\\n\"; }
echo \"done\\n\";
",
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Cannot modify readonly property DemoRecord::$number\ndone\n");
    fs::remove_dir_all(dir).unwrap();
}

/// A program subclass cannot reach the wrapper's bookkeeping: `$this->__elephcOwner`, by literal
/// or run-time name, is an ordinary dynamic property on the object (PHP's deprecation included),
/// and clearing it leaves the engine object alive.
#[test]
fn subclasses_cannot_reach_the_wrapper_bookkeeping() {
    let dir = project("bookkeeping");
    let run = compile_and_run(
        &dir,
        "<?php
class C extends DemoCounter {
    public function clearLiteral(): void { $this->__elephcOwner = null; }
    public function clearDynamic(string $n): void { $this->$n = null; }
    public function readDynamic(string $n): mixed { return $this->$n ?? 'unset'; }
}
$c = new C(4);
$c->clearLiteral();
echo $c->value(), \"\\n\";
$d = new C(5);
$d->clearDynamic('__elephc' . 'Owner');
echo $d->value(), \"\\n\";
var_dump($d->readDynamic('__elephc' . 'Handle') === 'unset');
",
    );
    assert_success(&run, "compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "4\n5\nbool(true)\n");
    assert_eq!(
        String::from_utf8_lossy(&run.stderr).matches("Creation of dynamic property C::$__elephcOwner is deprecated").count(),
        2
    );
    fs::remove_dir_all(dir).unwrap();
}

/// A request's resources close after RSHUTDOWN and before post-deactivation, newest first, while
/// the request state their destructor reads is still there (#2104), and a fatal error in
/// RSHUTDOWN cancels none of the deactivation: resources still close, the INI change is still
/// undone, post-deactivation and MSHUTDOWN still run, and the process ends with status 255, as
/// php-cli's `php_request_shutdown` runs each phase in its own `zend_try`.
#[test]
fn request_deactivation_closes_resources_and_survives_a_fatal_rshutdown() {
    let dir = project("deactivation");
    let source = "<?php\ndemo_open_handle('h1');\ndemo_open_handle('h2');\nvar_dump(demo_set_greeting('Hi'));\necho demo_banner(), \"\\n\";\n";
    let expected = ["RINIT", "RSHUTDOWN Hi", "CLOSE h2 alive=1", "CLOSE h1 alive=1", "POST_DEACTIVATE Hello", "MSHUTDOWN"];
    let (traced, status, stdout) = run_traced(&dir, source);
    assert_eq!((status, stdout.as_str()), (Some(0), "bool(true)\npersistent banner\n"));
    assert_eq!(traced, expected);
    let (traced, status, stdout) = run_traced_with(&dir, source, &[("ELEPHC_DEMO_FAIL_RSHUTDOWN", "1")]);
    assert_eq!((status, stdout.as_str()), (Some(255), "bool(true)\npersistent banner\n"));
    assert_eq!(traced, expected);
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

/// A `--web` server spawned in its own process group, killed with its workers when dropped, so a
/// failed assertion does not leave it running (and holding the test harness's output pipe).
struct ServerGroup(std::process::Child);

impl ServerGroup {
    /// Waits until `address` accepts connections; a loaded machine can take a while to start it.
    fn wait_until_listening(&self, address: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::net::TcpStream::connect(address).is_err() {
            assert!(std::time::Instant::now() < deadline, "the web server did not start");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

impl Drop for ServerGroup {
    fn drop(&mut self) {
        // SAFETY: signals the server's own process group, which this test created.
        unsafe { libc::killpg(self.0.id() as libc::pid_t, libc::SIGKILL) };
        let _ = self.0.wait();
    }
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

/// A runtime INI change lasts one request (#2105) and each request is torn down in php-src's order
/// (#2104): every request of a `--web` worker sees the configured greeting even though the
/// previous one changed it, and between requests the worker runs RSHUTDOWN, then
/// post-deactivation with the greeting already restored, then the next RINIT.
#[test]
fn a_runtime_ini_change_does_not_cross_web_requests() {
    use std::os::unix::process::CommandExt;
    let dir = project("web-ini");
    let main = dir.join("main.php");
    fs::write(&main, "<?php\necho demo_greet('a');\ndemo_set_greeting('Hi');\necho ' ', demo_greet('b');\n").unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).arg("--web").arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile --web");
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let address = format!("127.0.0.1:{port}");
    let stderr = fs::File::create(dir.join("server.err")).unwrap();
    let server = ServerGroup(
        Command::new(dir.join("main"))
            .args(["--listen", &address, "--workers", "1"])
            .env("ELEPHC_DEMO_TRACE", "1")
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(stderr)
            .spawn()
            .expect("spawn the web server"),
    );
    server.wait_until_listening(&address);
    let bodies: Vec<String> = (0..3).map(|_| http_body(&address).trim().to_string()).collect();
    drop(server);
    let output = fs::read_to_string(dir.join("server.err")).unwrap();
    let traced: Vec<&str> = output.lines().filter_map(|line| line.strip_prefix("elephc_demo: ")).collect();
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(bodies, ["Hello, a. Hi, b.", "Hello, a. Hi, b.", "Hello, a. Hi, b."]);
    assert_eq!(
        &traced[..7],
        ["RINIT", "RSHUTDOWN Hi", "POST_DEACTIVATE Hello", "RINIT", "RSHUTDOWN Hi", "POST_DEACTIVATE Hello", "RINIT"],
        "{output}"
    );
}

/// Between `--web` requests the worker deactivates each request as php-src does: the request's
/// resources close before post-deactivation, a persistent allocation MINIT made survives every
/// request heap sweep, and an engine object a leaked wrapper still holds is freed with the request
/// (its free handler runs, as php-src's object store frees every live object) rather than left
/// pointing into the swept heap.
#[test]
fn web_requests_free_their_resources_and_leaked_objects() {
    use std::os::unix::process::CommandExt;
    let dir = project("web-deactivation");
    let main = dir.join("main.php");
    fs::write(
        &main,
        "<?php\ndemo_open_handle('w');\necho demo_banner(), ' ', demo_counters_freed();\n$o = new stdClass;\n$o->self = $o;\n$o->counter = new DemoCounter(1);\n",
    )
    .unwrap();
    let compile = Command::new(elephc()).current_dir(&dir).arg("--web").arg(&main).output().expect("run elephc");
    assert_success(&compile, "compile --web");
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let address = format!("127.0.0.1:{port}");
    let stderr = fs::File::create(dir.join("server.err")).unwrap();
    let server = ServerGroup(
        Command::new(dir.join("main"))
            .args(["--listen", &address, "--workers", "1"])
            .env("ELEPHC_DEMO_TRACE", "1")
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(stderr)
            .spawn()
            .expect("spawn the web server"),
    );
    server.wait_until_listening(&address);
    let bodies: Vec<String> = (0..3).map(|_| http_body(&address).trim().to_string()).collect();
    drop(server);
    let output = fs::read_to_string(dir.join("server.err")).unwrap();
    let traced: Vec<&str> = output.lines().filter_map(|line| line.strip_prefix("elephc_demo: ")).collect();
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(bodies, ["persistent banner 0", "persistent banner 1", "persistent banner 2"], "{output}");
    // The counter the leaked cycle holds is freed after the request's resource closes.
    assert_eq!(
        &traced[..11],
        [
            "RINIT", "RSHUTDOWN Hello", "CLOSE w alive=1", "FREE counter 1", "POST_DEACTIVATE Hello",
            "RINIT", "RSHUTDOWN Hello", "CLOSE w alive=1", "FREE counter 1", "POST_DEACTIVATE Hello", "RINIT",
        ],
        "{output}"
    );
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
    let server = ServerGroup(
        Command::new(dir.join("main"))
            .args(["--listen", &address, "--workers", "1"])
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn the web server"),
    );
    server.wait_until_listening(&address);
    let bodies: Vec<String> = (0..3).map(|_| http_body(&address).trim().to_string()).collect();
    drop(server);
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
    let server = ServerGroup(
        Command::new(dir.join("main"))
            .args(["--listen", &address, "--workers", "1"])
            .env("ELEPHC_DEMO_TRACE", "1")
            .env("ELEPHC_DEMO_FAIL_RINIT_AFTER", "1")
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .expect("spawn the web server"),
    );
    server.wait_until_listening(&address);
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
    drop(server);
    let output = fs::read_to_string(dir.join("server.out")).unwrap() + &fs::read_to_string(dir.join("server.err")).unwrap();
    let traced: Vec<&str> = output.lines().filter_map(|line| line.strip_prefix("elephc_demo: ")).collect();
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(first, "1");
    assert_eq!(traced, ["RINIT", "RSHUTDOWN Hello", "POST_DEACTIVATE Hello", "RINIT failed"], "{output}");
    assert!(output.contains("request_startup() for elephc_demo module failed"), "{output}");
}
