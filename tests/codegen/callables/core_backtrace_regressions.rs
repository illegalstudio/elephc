//! Purpose:
//! Pins complete native/eval backtrace boundaries, limits, and argument ownership.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_backtrace_regression_`.
//!
//! Key details:
//! - Eval fragments depend on a runtime value so Magician executes the frame reader.
//! - Every frame-capture detector gets its OWN program. The gate is program-wide: one detector
//!   firing gives every frame in that program its hidden argument snapshot, so two detectors in
//!   one fixture would let the working one mask the broken one.
//! - Frames are selected by `function` name, never by index. `call_user_func` and friends may or
//!   may not contribute a frame of their own, and that is not what these tests are pinning; a
//!   fixture that indexed the trace would be asserting the wrong thing either way.

use crate::support::*;

/// Opaque eval traces include eval and suspended AOT frames, while options and limits compose.
#[test]
fn test_core_backtrace_regression_mixed_boundary() {
    let out = compile_and_run(r#"<?php
function outerInventoryTrace(int $n): void {
    $source = 'function innerInventoryTrace(): void {
        $trace = debug_backtrace();
        foreach ($trace as $frame) { echo $frame["function"], ":"; }
        echo $trace[2]["args"][0], ":";
        echo count(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2)), ":";
        echo isset(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS)[2]["args"]) ? "bad" : "noargs";
        ob_start(); debug_print_backtrace(0); $printed = ob_get_clean();
        echo str_contains($printed, "outerInventoryTrace(7)") ? ":printed" : ":bad";
    } innerInventoryTrace();' . ' // ' . $n;
    eval($source);
}
outerInventoryTrace(7);
"#);
    assert_eq!(out, "innerInventoryTrace:eval:outerInventoryTrace:7:2:noargs:printed");
}

/// Nested eval adds boundaries without shadowing func_num_args in its enclosing function.
#[test]
fn test_core_backtrace_regression_nested_eval_keeps_argument_scope() {
    let out = compile_and_run(r#"<?php
$source = 'function nestedInventoryTrace($arg): void {
    eval(\'echo func_num_args(), ":"; foreach (debug_backtrace(2) as $f) echo $f["function"], ":";\');
} nestedInventoryTrace(9);' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "1:eval:nestedInventoryTrace:eval:");
}

/// A live argument ownership guard must never be invoked as a frame reader.
#[test]
fn test_core_backtrace_regression_skips_argument_ownership_guards() {
    let out = compile_and_run(r#"<?php
function tracedGuard($value): void {
    foreach (debug_backtrace() as $frame) echo $frame["function"], ":";
}
tracedGuard(["owned"]);
"#);
    assert_eq!(out, "tracedGuard:");
}

/// An eval-originated trace keeps omitted optionals out of the frame and keeps variadic tails in.
///
/// Both depend on the hidden argument snapshot, which only exists when the frame-capture gate
/// fires. The trace is taken by name rather than by index so a `call_user_func`-shaped frame
/// could not change what is asserted.
#[test]
fn test_core_backtrace_regression_eval_frame_respects_optional_and_variadic_args() {
    let out = compile_and_run(r#"<?php
function tracedOptionalTail($first, $second = 5, ...$rest) {
    foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "tracedOptionalTail") {
            return count($frame["args"]) . ":" . implode(",", $frame["args"]);
        }
    }
    return "missing";
}
$source = 'echo tracedOptionalTail(1), "|", tracedOptionalTail(1, 2, 3, 4);' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "1:1|4:1,2,3,4");
}

/// `eval()` alone arms the frame-capture gate, with no backtrace spelling anywhere in AOT source.
///
/// This is the case `crate::codegen::frame::module_uses_backtrace()` forces on: a program that
/// carries the eval bridge gets backtraces whether or not the AST mentions one, so a frame
/// lowered without its hidden argument snapshot would report stale or missing arguments. The
/// only occurrence of the builtin's name here is INSIDE a longer eval source string, which is
/// not a callable-name literal, and no AOT statement calls it.
#[test]
fn test_core_backtrace_regression_eval_is_the_only_frame_capture_gate() {
    let out = compile_and_run(r#"<?php
function evalOnlyGateFrame($first, $second = 5) {
    $probe = 'foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "evalOnlyGateFrame") { echo count($frame["args"]); }
    }';
    eval($probe . ' // ' . $first);
}
evalOnlyGateFrame(1);
echo "|";
evalOnlyGateFrame(1, 2);
"#);
    assert_eq!(out, "1|2");
}

/// A literal `call_user_func("debug_backtrace")` is the program's only frame-capture trigger.
#[test]
fn test_core_backtrace_regression_call_user_func_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaCallUserFunc($first, $second = 5) {
    foreach (call_user_func("debug_backtrace") as $frame) {
        if ($frame["function"] === "tracedViaCallUserFunc") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaCallUserFunc(1), ":", tracedViaCallUserFunc(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A literal `call_user_func_array("debug_backtrace", [])` is the program's only trigger.
#[test]
fn test_core_backtrace_regression_call_user_func_array_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaCallUserFuncArray($first, $second = 5) {
    foreach (call_user_func_array("debug_backtrace", []) as $frame) {
        if ($frame["function"] === "tracedViaCallUserFuncArray") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaCallUserFuncArray(1), ":", tracedViaCallUserFuncArray(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A first-class callable `debug_backtrace(...)` is the program's only trigger.
#[test]
fn test_core_backtrace_regression_first_class_callable_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaFirstClassCallable($first, $second = 5) {
    $reader = debug_backtrace(...);
    foreach ($reader() as $frame) {
        if ($frame["function"] === "tracedViaFirstClassCallable") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaFirstClassCallable(1), ":", tracedViaFirstClassCallable(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A callable held in a string variable is the program's only trigger.
#[test]
fn test_core_backtrace_regression_string_variable_callable_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaStringVariable($first, $second = 5) {
    $reader = "debug_backtrace";
    foreach ($reader() as $frame) {
        if ($frame["function"] === "tracedViaStringVariable") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaStringVariable(1), ":", tracedViaStringVariable(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A leading namespace separator in the callable string still names the Core builtin.
///
/// PHP strips exactly one leading `\` from a callable string, so `"\debug_backtrace"` resolves to
/// the same builtin and has to arm the same gate. This is its own program precisely because the
/// unprefixed spelling would otherwise arm it first.
#[test]
fn test_core_backtrace_regression_leading_backslash_callable_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaLeadingBackslash($first, $second = 5) {
    $reader = "\debug_backtrace";
    foreach ($reader() as $frame) {
        if ($frame["function"] === "tracedViaLeadingBackslash") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaLeadingBackslash(1), ":", tracedViaLeadingBackslash(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// `debug_print_backtrace()` through `call_user_func` prints the caller's real argument list.
///
/// The expected text is written out, not rebuilt from `func_get_args()`: deriving it would make
/// the fixture agree with whatever the frame happens to hold, which is the very thing under test.
#[test]
fn test_core_backtrace_regression_call_user_func_print_shows_frame_args() {
    let out = compile_and_run(r#"<?php
function printedViaCallUserFunc($first, $second = 7) {
    ob_start(); call_user_func("debug_print_backtrace"); $printed = ob_get_clean();
    return str_contains($printed, "printedViaCallUserFunc(1, 2)") ? "ok" : $printed;
}
echo printedViaCallUserFunc(1, 2);
"#);
    assert_eq!(out, "ok");
}

/// `debug_print_backtrace()` through a string-variable callable prints the omitted-optional list.
#[test]
fn test_core_backtrace_regression_string_variable_print_shows_frame_args() {
    let out = compile_and_run(r#"<?php
function printedViaStringVariable($first, $second = 7) {
    $printer = "debug_print_backtrace";
    ob_start(); $printer(); $printed = ob_get_clean();
    return str_contains($printed, "printedViaStringVariable(1)") ? "ok" : $printed;
}
echo printedViaStringVariable(1);
"#);
    assert_eq!(out, "ok");
}

/// A subclass of an abstract class that leaves a builtin interface method unimplemented compiles
/// once `debug_backtrace()` arms the hidden collector on every body.
///
/// The abstract class records a copy of `IteratorAggregate::getIterator()`'s contract as its own,
/// and that copy never carries the collector; phpoption's `Option`/`Some` is the real shape.
#[test]
fn test_core_backtrace_regression_override_of_builtin_interface_contract_copy() {
    let out = compile_and_run(r#"<?php
abstract class Opt implements IteratorAggregate {
    public static function of($value): Opt { return $value === null ? new Nothing() : new Just($value); }
    public function all(): array { return iterator_to_array($this->getIterator()); }
}
final class Just extends Opt {
    public function __construct(private $value) {}
    public function getIterator(): ArrayIterator { return new ArrayIterator([$this->value]); }
}
final class Nothing extends Opt {
    public function getIterator(): EmptyIterator { return new EmptyIterator(); }
}
function take(Opt $o): array { return iterator_to_array($o); }
echo json_encode(take(Opt::of("x"))), json_encode(take(Opt::of(null))), json_encode(Opt::of(7)->all());
foreach (Opt::of("y") as $k => $v) { echo " $k=$v"; }
echo " ", count(debug_backtrace());
"#);
    assert_eq!(out, r#"["x"][][7] 0=y 0"#);
}

/// The probe every shared-reader test runs: `debug_backtrace()` with and without arguments and
/// objects, `debug_print_backtrace()` with defaults, variadic surplus and a limit, and an
/// exception's `getTraceAsString()`. The expected text below was produced by php 8.5 itself.
const BACKTRACE_READER_PROBE: &str = r#"<?php
interface Walker { public function go($a, ...$more); }
class Box implements Walker {
    public function go($a, ...$more) { return $this->inner($a, [1, 2], new stdClass, 2.5, null, false, "x"); }
    private function inner(...$all) { return static::land(...$all); }
    public static function land($a, $b = 9, $c = 8) { return debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT); }
}
function describe(array $trace): void {
    foreach ($trace as $i => $f) {
        echo $i, ":", $f['function'], ":", $f['class'] ?? '-', ":", $f['type'] ?? '-', ":", $f['line'];
        echo ":", isset($f['object']) ? get_class($f['object']) : '-', ":", implode(',', array_keys($f)), ":";
        foreach ($f['args'] ?? ['none'] as $a) {
            echo is_object($a) ? get_class($a) : (is_array($a) ? 'array' : var_export($a, true)), ";";
        }
        echo "\n";
    }
}
describe((new Box)->go(1, 'a', 'b'));
echo "--\n";
function plain($p, $q = 3) { return debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS); }
describe(plain(1));
describe(plain(1, 2));
echo "--\n";
function printer($p, $q = 3, ...$rest) { debug_print_backtrace(); debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 1); }
ob_start();
printer(1);
printer(1, 2, 3, 4);
echo str_replace(__FILE__, 'F', ob_get_clean());
echo "--\n";
function thrower($x, $y = 5) { throw new RuntimeException("boom"); }
function mid() { thrower("s", 3); }
try { mid(); } catch (Exception $e) {
    echo str_replace(__FILE__, 'F', $e->getTraceAsString()), "\n";
    echo count($e->getTrace()), ":", $e->getLine(), ":", ($e->getFile() === __FILE__ ? 'F' : 'other'), "\n";
}
"#;

/// What php prints for [`BACKTRACE_READER_PROBE`].
const BACKTRACE_READER_PROBE_EXPECTED: &str = r#"0:land:Box::::5:-:file,line,function,class,type,args:1;array;stdClass;2.5;NULL;false;'x';
1:inner:Box:->:4:Box:file,line,function,class,object,type,args:1;array;stdClass;2.5;NULL;false;'x';
2:go:Box:->:18:Box:file,line,function,class,object,type,args:1;'a';'b';
--
0:plain:-:-:21:-:file,line,function:'none';
0:plain:-:-:22:-:file,line,function:'none';
--
#0 F(26): printer(1)
#0 F(26): printer()
#0 F(27): printer(1, 2, 3, 4)
#0 F(27): printer()
--
#0 F(31): thrower('s', 3)
#1 F(32): mid()
#2 {main}
2:30:F
"#;

/// Asserts a heap-debug run printed `expected` and ended with a clean leak summary.
fn assert_backtrace_run_clean(out: &ProgramOutput, expected: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The frame readers share one renderer, one entry builder and one tail appender, and their
/// output is byte-identical to php's: methods, statics, variadic surplus, defaulted parameters
/// trimmed to the caller's count, `IGNORE_ARGS`, `PROVIDE_OBJECT`, print limits and exception
/// traces; the heap is clean afterwards.
#[test]
fn test_core_backtrace_shared_frame_readers_match_php_and_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(BACKTRACE_READER_PROBE);
    assert_backtrace_run_clean(&out, BACKTRACE_READER_PROBE_EXPECTED);
}

/// Instructions in the frame reader of `function_label_part`, from its `backtrace_frame_reader`
/// label to its `backtrace_call_site` label.
fn backtrace_reader_instructions(asm: &str, function_label_part: &str) -> usize {
    let mut counting = false;
    let mut count = 0;
    for line in asm.lines() {
        if line.starts_with("L_")
            && line.contains(function_label_part)
            && line.contains("backtrace_frame_reader")
        {
            counting = true;
            count = 0;
        } else if counting && line.starts_with("L_") && line.contains("backtrace_call_site") {
            return count;
        } else if counting && line.starts_with("    ") && !line.trim_start().starts_with('.') {
            count += 1;
        }
    }
    panic!("no frame reader for {function_label_part}");
}

/// A frame reader holds only what depends on its own function: parameter loads. Printing, the
/// entry builder, the surplus-argument loop and the trim live once in the runtime, so a reader
/// stays small, does not grow with the name of its class, and grows only by a bounded amount per
/// parameter. Before the move a reader was 260 instructions at no parameters.
#[test]
fn test_core_backtrace_frame_reader_size_is_bounded_and_independent_of_names() {
    let dir = make_cli_test_dir("elephc_backtrace_reader_size");
    let (asm, _runtime, _libs) =
        compile_source_to_asm_with_options(BACKTRACE_READER_PROBE, &dir, 8_388_608, false, false);
    for shared in ["__rt_backtrace_print_frame", "__rt_backtrace_frame_hash"] {
        assert!(asm.contains(shared), "readers must call {shared}");
    }
    for inlined in [
        "debug_print_backtrace_first_arg",
        "debug_backtrace_args_not_trimmed",
    ] {
        assert!(!asm.contains(inlined), "a reader still inlines {inlined}");
    }
    let mid = backtrace_reader_instructions(&asm, "_mid_");
    let plain = backtrace_reader_instructions(&asm, "_plain_");
    let thrower = backtrace_reader_instructions(&asm, "_thrower_");
    let method = backtrace_reader_instructions(&asm, "_Box__land_");
    assert!(mid <= 100, "a parameterless reader grew to {mid} instructions");
    assert!(plain <= mid + 2 * 40, "two parameters cost {plain} vs {mid}");
    assert!(thrower <= mid + 2 * 40, "two parameters cost {thrower} vs {mid}");
    assert!(method <= mid + 3 * 40 + 30, "a three-parameter method costs {method} vs {mid}");
    let _ = fs::remove_dir_all(dir);
}

/// A program with `classes` classes, each with a typed receiver call, plus an eval so the bridge
/// traces are in play.
fn null_receiver_probe(classes: usize) -> String {
    let mut source = String::from("<?php\n");
    for n in 0..classes {
        source.push_str(&format!(
            "class Recv{n} {{ public function ping(): string {{ return 'pong{n}'; }} }}\nfunction call_{n}(?Recv{n} $p) {{ return $p->ping(); }}\n"
        ));
    }
    source.push_str("function make(): ?Recv0 { return $GLOBALS['argc'] > 5 ? new Recv0 : null; }\n");
    source.push_str("echo call_0(new Recv0), \"\\n\";\n");
    source.push_str("try { echo call_0(make()), \"\\n\"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\n");
    source.push_str("$p = make();\ntry { echo $p->ping(), \"\\n\"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\n");
    source.push_str("$code = 'return 1+1;';\necho eval($code), \"\\n\";\n");
    source
}

/// The null-receiver path of a typed method call stays catchable and says what php says.
#[test]
fn test_typed_method_call_on_null_is_a_catchable_error_with_the_eval_bridge_on() {
    let out = compile_and_run(&null_receiver_probe(3));
    assert_eq!(
        out,
        "pong0\nError: Call to a member function ping() on null\nError: Call to a member function ping() on null\n2\n"
    );
}

/// The cold null-receiver path of every typed call is a record address and a call into one
/// shared trace helper: a handful of instructions per site, the same with 2 classes or 40, and
/// the helper is defined exactly once. It was about 24 instructions per site (two bridge calls
/// with six argument loads each) on 22 500 sites of the Symfony app.
#[test]
fn test_null_receiver_path_is_a_shared_helper_call_independent_of_class_count() {
    let mut worst = Vec::new();
    for classes in [2usize, 40] {
        let dir = make_cli_test_dir("elephc_null_receiver_path");
        let (asm, _runtime, _libs) = compile_source_to_asm_with_options(
            &null_receiver_probe(classes),
            &dir,
            8_388_608,
            false,
            false,
        );
        assert_eq!(
            asm.matches("\n_eir_shared_null_method_trace:").count(),
            1,
            "the trace helper must be defined exactly once"
        );
        let mut counting = false;
        let mut count = 0usize;
        let mut paths = 0usize;
        let mut longest = 0usize;
        for line in asm.lines() {
            if line.starts_with("L_") && line.contains("static_method_receiver_null") {
                counting = true;
                count = 0;
            } else if counting && line.starts_with("L_") && line.contains("static_method_receiver_checked") {
                paths += 1;
                longest = longest.max(count);
                counting = false;
            } else if counting && line.starts_with("    ") && !line.trim_start().starts_with('.') {
                count += 1;
            }
        }
        assert!(paths >= classes, "expected a guard per typed call, saw {paths}");
        assert!(longest <= 12, "a null-receiver path grew to {longest} instructions");
        worst.push(longest);
        let _ = fs::remove_dir_all(dir);
    }
    assert_eq!(worst[0], worst[1], "the path must not depend on the class count");
}

/// Late static binding through `static::`, `self::`, `parent::` and `A::` forwarding keeps php's
/// answers (expected text produced by php 8.5), next to the receiver guards the null-receiver
/// work touched.
#[test]
fn test_static_self_parent_and_forwarding_calls_match_php() {
    let out = compile_and_run(r#"<?php
class A {
    public static function create(): static { return new static(); }
    public static function who(): string { return static::class . "/" . self::class . "/" . get_called_class(); }
    public static function viaSelf(): string { return self::who(); }
    public static function viaStatic(): string { return static::who(); }
    public function inst(): string { return static::who() . "|" . $this->name(); }
    public function name(): string { return "A"; }
}
class B extends A {
    public function name(): string { return "B"; }
    public static function viaParent(): string { return parent::who(); }
    public static function forward(): string { return A::who(); }
    public function instParent(): string { return parent::name() . parent::inst(); }
}
class C extends B {}
echo get_class(A::create()), get_class(B::create()), get_class(C::create()), "\n";
echo A::who(), "\n", B::who(), "\n", C::who(), "\n";
echo B::viaSelf(), "\n", C::viaSelf(), "\n", C::viaStatic(), "\n", B::viaParent(), "\n", C::viaParent(), "\n";
echo C::forward(), "\n";
echo (new C)->inst(), "\n", (new C)->instParent(), "\n";
$o = new C;
echo $o::who(), " ", $o->inst(), "\n";
"#);
    assert_eq!(out, r#"ABC
A/A/A
B/A/B
C/A/C
B/A/B
C/A/C
C/A/C
B/A/B
C/A/C
A/A/A
C/A/C|B
AC/A/C|B
C/A/C C/A/C|B
"#);
}

/// A compiled closure's trace frame carries php's `{closure:SCOPE:LINE}` name and the enclosing
/// class, and a `static function` closure is reported with `::` even inside an instance method.
///
/// Oracle: `php closure_trace.php` prints
/// `i#0:show:-:-|i#1:{closure:Tracer::instance():9}:Tracer:->|i#2:instance:Tracer:->|` then the
/// static variant. The frame used to leak the compiler's `__eir_closure_*` symbol and report no
/// class at all, which the dev error page rendered verbatim.
#[test]
fn test_compiled_closure_trace_frames_match_php_names_and_call_types() {
    let out = compile_and_run(r##"<?php
function show(string $tag): void {
    foreach (debug_backtrace() as $i => $f) {
        echo $tag, "#", $i, ":", $f["function"] ?? "-", ":", $f["class"] ?? "-", ":", $f["type"] ?? "-", "|";
    }
    echo "\n";
}
class Tracer {
    public function instance(): void { $f = function () { show("i"); }; $f(); }
    public function viaStatic(): void { $f = static function () { show("s"); }; $f(); }
}
$t = new Tracer();
$t->instance();
$t->viaStatic();
"##);
    assert_eq!(
        out,
        "i#0:show:-:-|i#1:{closure:Tracer::instance():9}:Tracer:->|i#2:instance:Tracer:->|\n\
         s#0:show:-:-|s#1:{closure:Tracer::viaStatic():10}:Tracer:::|s#2:viaStatic:Tracer:->|\n"
    );
}

/// A statically inlined `require_once` republishes PHP's own include frame.
///
/// The resolver expands a resolved include body inline, so without the synthetic activation the
/// frame vanishes from both `debug_backtrace()` and Throwable traces. php reports
/// `require_once` with the include site's file/line and the resolved path as its single argument.
#[test]
fn test_core_backtrace_regression_include_once_frame() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function show(): void {
    foreach (debug_backtrace() as $f) {
        echo $f["function"] ?? "-", "|", basename($f["file"] ?? "?"), ":", $f["line"] ?? -1, "|";
        echo isset($f["args"]) ? count($f["args"]) : -1, "|";
    }
    echo "\n";
}
function boom(): void { throw new RuntimeException("x"); }
require_once __DIR__ . "/inc.php";
"#,
            ),
            (
                "inc.php",
                r#"<?php
show();
try { boom(); } catch (RuntimeException $e) {
    foreach ($e->getTrace() as $f) { echo $f["function"] ?? "-", "|"; }
    echo "\n";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "show|inc.php:2|0|require_once|main.php:10|1|\nboom|require_once|\n");
}

/// `(array) $exception` exposes the Traceable `trace` slot as the array it is.
///
/// PHP declares the slot `array`; elephc declares it `Mixed` (stores may publish a boxed cell)
/// while the throwable capture writes a RAW array pointer. Describing the descriptor row as a
/// boxed Mixed cell made the cast report an integer and made VarDumper's `castObject()` fault on
/// a non-empty trace.
#[test]
fn test_core_backtrace_regression_exception_trace_property_casts_to_array() {
    let out = compile_and_run(
        r#"<?php
function make(): Exception { return new Exception("x"); }
$e = make();
$a = (array) $e;
$t = $a["\0Exception\0trace"];
echo gettype($t), ":", count($t), ":", gettype($e->getTrace()), "\n";
"#,
    );
    assert_eq!(out, "array:1:array\n");
}

/// A runtime include reports the INCLUDED file's resolved `__DIR__`/`__FILE__`.
///
/// PHP canonicalizes the include target: `__DIR__.'/../b/two.php'` answers `<root>/b`, not
/// `<root>/a/../b`. Composer's autoloader and Symfony's runtime-generated container include
/// through paths containing `..`; leaving the literal spelling made service factories compute
/// `dirname(__DIR__, 4)` one directory off (the dev error page's frame paths gained an extra
/// `symfony-app/` segment).
#[test]
fn test_core_backtrace_regression_runtime_include_dir_is_canonical() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$p = $argc > 0 ? __DIR__.'/a/one.php' : '';
require $p;
"#,
            ),
            (
                "a/one.php",
                r#"<?php
echo "one: ", basename(__DIR__), "|";
$q = $argc > 0 ? __DIR__.'/../b/two.php' : '';
require $q;
"#,
            ),
            ("b/two.php", r#"<?php echo "two: ", basename(__DIR__), "|", basename(__FILE__), "\n";"#),
        ],
        "main.php",
    );
    assert_eq!(out, "one: a|two: b|two.php\n");
}
