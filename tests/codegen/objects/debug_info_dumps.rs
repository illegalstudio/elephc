//! Purpose:
//! End-to-end tests for `var_dump()` / `print_r()` honouring a `__debugInfo()` that COMPUTES its
//! result at run time (a body the compiler cannot fold into static descriptor rows).
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is reference PHP 8.5 output, byte for byte (stdout only).
//! - The walkers call the method through `_class_debuginfo_ptrs` and dump the returned array
//!   under the usual `object(C)#N (n)` / `C Object` header, with `n` the returned entry count.
//! - `var_export()` never consults `__debugInfo()` and keeps printing the real properties.
//! - A non-array return is PHP's uncatchable fatal `__debuginfo() must return an array` (exit 255,
//!   text on stderr like every elephc runtime fatal); a null return is an empty array plus a
//!   `Deprecated: Returning null from C::__debugInfo()` diagnostic on stderr.
//! - An exception escaping `__debugInfo()` is uncatchable in PHP 8.5: `Warning: Uncaught ...` then the same
//!   `__debuginfo()` fatal, exit 255, no `catch`/`finally` of the caller runs. A `try` inside the method still works.

use crate::support::*;

/// A computed `__debugInfo()` replaces the property dump for var_dump/print_r at top level, inside an array and through a subclass.
#[test]
fn test_debug_info_computed_body_top_level_array_and_subclass() {
    let out = compile_and_run(r#"<?php
class Holder {
    public array $items = [1, 2];
    private string $secret = "hidden";
    public function __debugInfo() {
        return ["shown" => 1, "dyn" => count($this->items)];
    }
}
$h = new Holder();
var_dump($h);
print_r($h);
echo "\n";
$h->items[] = 3;
var_dump([$h]);
class Child extends Holder {}
var_dump(new Child());
"#);
    assert_eq!(
        out,
        concat!(
            "object(Holder)#1 (2) {\n",
            "  [\"shown\"]=>\n",
            "  int(1)\n",
            "  [\"dyn\"]=>\n",
            "  int(2)\n",
            "}\n",
            "Holder Object\n",
            "(\n",
            "    [shown] => 1\n",
            "    [dyn] => 2\n",
            ")\n",
            "\n",
            "array(1) {\n",
            "  [0]=>\n",
            "  object(Holder)#1 (2) {\n",
            "    [\"shown\"]=>\n",
            "    int(1)\n",
            "    [\"dyn\"]=>\n",
            "    int(3)\n",
            "  }\n",
            "}\n",
            "object(Child)#2 (2) {\n",
            "  [\"shown\"]=>\n",
            "  int(1)\n",
            "  [\"dyn\"]=>\n",
            "  int(2)\n",
            "}\n",
        )
    );
}

/// An indexed-array return prints integer keys, and an untyped (boxed Mixed) return is unwrapped the same way.
#[test]
fn test_debug_info_list_and_mixed_return_shapes() {
    let out = compile_and_run(r#"<?php
class ListInfo {
    public function __debugInfo(): array { return [10, "x", 3.5]; }
}
class Mixedy {
    public function __debugInfo() { return $_SERVER["argc"] > 99 ? 1 : ["k" => "v"]; }
}
$l = new ListInfo();
$m = new Mixedy();
var_dump($l);
print_r($l);
echo "\n";
var_dump($m);
print_r($m);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(ListInfo)#1 (3) {\n",
            "  [0]=>\n",
            "  int(10)\n",
            "  [1]=>\n",
            "  string(1) \"x\"\n",
            "  [2]=>\n",
            "  float(3.5)\n",
            "}\n",
            "ListInfo Object\n",
            "(\n",
            "    [0] => 10\n",
            "    [1] => x\n",
            "    [2] => 3.5\n",
            ")\n",
            "\n",
            "object(Mixedy)#2 (1) {\n",
            "  [\"k\"]=>\n",
            "  string(1) \"v\"\n",
            "}\n",
            "Mixedy Object\n",
            "(\n",
            "    [k] => v\n",
            ")\n",
            "\n",
        )
    );
}

/// An object that returns itself from `__debugInfo()` renders `*RECURSION*` for the inner occurrence instead of looping.
#[test]
fn test_debug_info_self_reference_renders_recursion() {
    let out = compile_and_run(r#"<?php
class Selfy {
    public $name = "s";
    public function __debugInfo() { return ["me" => $this, "n" => $this->name . "!"]; }
}
$s = new Selfy();
var_dump($s);
print_r($s);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(Selfy)#1 (2) {\n",
            "  [\"me\"]=>\n",
            "  *RECURSION*\n",
            "  [\"n\"]=>\n",
            "  string(2) \"s!\"\n",
            "}\n",
            "Selfy Object\n",
            "(\n",
            "    [me] => Selfy Object\n",
            " *RECURSION*\n",
            "    [n] => s!\n",
            ")\n",
            "\n",
        )
    );
}

/// Objects with a computed `__debugInfo()` nest inside arrays and other debug-info dumps, and `print_r(..., true)` captures the same text.
#[test]
fn test_debug_info_nested_objects_and_return_mode() {
    let out = compile_and_run(r#"<?php
class Plain { public $a = 1; }
class Nest {
    public $kids = [];
    public function __debugInfo(): array {
        return ["count" => count($this->kids), "kids" => $this->kids, "tag" => strtoupper("t")];
    }
}
$n = new Nest();
$n->kids = [new Plain(), new Nest()];
var_dump($n);
print_r($n);
echo "\n";
$text = print_r($n, true);
echo strlen($text), "\n";
var_dump(new Plain());
"#);
    assert_eq!(
        out,
        concat!(
            "object(Nest)#1 (3) {\n",
            "  [\"count\"]=>\n",
            "  int(2)\n",
            "  [\"kids\"]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    object(Plain)#2 (1) {\n",
            "      [\"a\"]=>\n",
            "      int(1)\n",
            "    }\n",
            "    [1]=>\n",
            "    object(Nest)#3 (3) {\n",
            "      [\"count\"]=>\n",
            "      int(0)\n",
            "      [\"kids\"]=>\n",
            "      array(0) {\n",
            "      }\n",
            "      [\"tag\"]=>\n",
            "      string(1) \"T\"\n",
            "    }\n",
            "  }\n",
            "  [\"tag\"]=>\n",
            "  string(1) \"T\"\n",
            "}\n",
            "Nest Object\n",
            "(\n",
            "    [count] => 2\n",
            "    [kids] => Array\n",
            "        (\n",
            "            [0] => Plain Object\n",
            "                (\n",
            "                    [a] => 1\n",
            "                )\n",
            "\n",
            "            [1] => Nest Object\n",
            "                (\n",
            "                    [count] => 0\n",
            "                    [kids] => Array\n",
            "                        (\n",
            "                        )\n",
            "\n",
            "                    [tag] => T\n",
            "                )\n",
            "\n",
            "        )\n",
            "\n",
            "    [tag] => T\n",
            ")\n",
            "\n",
            "408\n",
            "object(Plain)#4 (1) {\n",
            "  [\"a\"]=>\n",
            "  int(1)\n",
            "}\n",
        )
    );
}

/// `__debugInfo()` runs exactly once per dump, and `var_export()` keeps printing the real properties.
#[test]
fn test_debug_info_runs_once_per_dump_and_var_export_ignores_it() {
    let out = compile_and_run(r#"<?php
class Counted {
    public $items = [1, 2, 3];
    public static $calls = 0;
    public function __debugInfo(): array {
        self::$calls++;
        return ["n" => count($this->items)];
    }
}
$c = new Counted();
var_dump($c);
print_r($c);
echo "\n";
echo Counted::$calls, "\n";
var_export($c);
echo "\n";
echo Counted::$calls, "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(Counted)#1 (1) {\n",
            "  [\"n\"]=>\n",
            "  int(3)\n",
            "}\n",
            "Counted Object\n",
            "(\n",
            "    [n] => 3\n",
            ")\n",
            "\n",
            "2\n",
            "\\Counted::__set_state(array(\n",
            "   'items' => \n",
            "  array (\n",
            "    0 => 1,\n",
            "    1 => 2,\n",
            "    2 => 3,\n",
            "  ),\n",
            "))\n",
            "2\n",
        )
    );
}

/// An empty returned array prints a zero-entry block, and a subclass override wins over the parent's `__debugInfo()`.
#[test]
fn test_debug_info_empty_array_and_inherited_override() {
    let out = compile_and_run(r#"<?php
class Base {
    public $v = 7;
    public function __debugInfo(): array { return ["base" => $this->v * 2]; }
}
class Quiet extends Base {
    public function __debugInfo(): array { return array_slice([1, 2], 0, 0); }
}
class Loud extends Base {
    public function __debugInfo(): array { return ["loud" => parent::__debugInfo()["base"] + 1]; }
}
$q = new Quiet();
$l = new Loud();
var_dump($q);
print_r($q);
echo "\n";
var_dump($l);
print_r($l);
echo "\n";
"#);
    assert_eq!(
        out,
        concat!(
            "object(Quiet)#1 (0) {\n",
            "}\n",
            "Quiet Object\n",
            "(\n",
            ")\n",
            "\n",
            "object(Loud)#2 (1) {\n",
            "  [\"loud\"]=>\n",
            "  int(15)\n",
            "}\n",
            "Loud Object\n",
            "(\n",
            "    [loud] => 15\n",
            ")\n",
            "\n",
        )
    );
}

/// Asserts the PHP 8.5 shape of a dump that died inside `__debugInfo()`: exit non-zero, neither the
/// `catch` nor the `finally` around the dump ran, and stdout ends with the `__debuginfo()` fatal.
fn assert_debug_info_fatal(out: &ProgramOutput) {
    assert!(!out.success, "{}", out.stderr);
    // PHP names the dump call: `... must return an array in <file> on line <n>`.
    let fatal = out.stdout.rfind("\nFatal error: __debuginfo() must return an array in ");
    assert!(fatal.is_some(), "{}", out.stdout);
    let tail = &out.stdout[fatal.unwrap()..];
    assert!(tail.contains(" on line ") && tail.ends_with('\n') && tail.matches('\n').count() == 2, "{}", out.stdout);
    for forbidden in ["caught", "outer finally", "unreachable"] {
        assert!(!out.stdout.lines().any(|line| line == forbidden), "{}", out.stdout);
    }
}

/// A non-array `__debugInfo()` return is PHP's uncatchable fatal `__debuginfo() must return an array`: the
/// `catch (\TypeError)` never runs, the process exits non-zero and `var_dump` has written nothing for the object.
#[test]
fn test_debug_info_non_array_return_is_uncatchable_fatal_var_dump() {
    let out = compile_and_run_capture(r#"<?php
class Bad {
    public function __debugInfo() { return $_SERVER["argc"] > 0 ? 5 : []; }
}
echo "start\n";
try {
    var_dump(new Bad());
} catch (\TypeError $e) {
    echo "caught\n";
} finally {
    echo "outer finally\n";
}
echo "unreachable\n";
"#);
    assert!(out.stdout.starts_with("start\n\nFatal error: __debuginfo() must return an array in "), "{}", out.stdout);
    assert!(out.stdout.ends_with(" on line 7\n"), "{}", out.stdout);
    assert_debug_info_fatal(&out);
}

/// The same fatal through `print_r`: PHP builds the text in a buffer, so nothing of the call reaches stdout.
#[test]
fn test_debug_info_non_array_return_is_uncatchable_fatal_print_r() {
    let out = compile_and_run_capture(r#"<?php
class Bad {
    public function __debugInfo() { return $_SERVER["argc"] > 0 ? "text" : []; }
}
class Outer { public $a = 1; public $inner; }
$outer = new Outer();
$outer->inner = new Bad();
echo "start\n";
try {
    print_r($outer);
} catch (\Throwable $e) {
    echo "caught\n";
}
echo "unreachable\n";
"#);
    assert!(out.stdout.starts_with("start\n\nFatal error: __debuginfo() must return an array in "), "{}", out.stdout);
    assert!(out.stdout.ends_with(" on line 10\n"), "{}", out.stdout);
    assert_debug_info_fatal(&out);
}

/// An exception thrown inside `__debugInfo()` is not catchable by the dump's caller in PHP 8.5: the engine prints
/// `Warning: Uncaught <Class>: <message>` and then the `__debuginfo()` fatal, exit 255. Nothing but the `start` line
/// reaches stdout before the report (PHP prints the same; its `[internal function]` and `var_dump()` trace frames are the
/// ones elephc cannot list, since internal calls have no activation record).
#[test]
fn test_debug_info_exception_is_uncatchable_var_dump_top_level() {
    let out = compile_and_run_capture(r#"<?php
class Flaky {
    public function __debugInfo(): array { throw new RuntimeException("boom"); }
}
echo "start\n";
try {
    var_dump(new Flaky());
} catch (RuntimeException $e) {
    echo "caught\n";
} finally {
    echo "outer finally\n";
}
echo "unreachable\n";
"#);
    assert!(out.stdout.starts_with("start\n\nWarning: Uncaught RuntimeException: boom"), "{}", out.stdout);
    // The warning carries PHP's whole report: the trace (the `__debugInfo()` frame, then `{main}`)
    // and the closing `thrown in` line, before the `__debuginfo()` fatal.
    assert!(out.stdout.contains("\nStack trace:\n#0 "), "{}", out.stdout);
    assert!(out.stdout.contains("Flaky->__debugInfo()\n#1 {main}\n  thrown in "), "{}", out.stdout);
    assert_debug_info_fatal(&out);
}

/// Nested in an outer object: `var_dump` has already written the outer header and the property key plus the nested
/// value's indent (PHP's partial output), then the warning and the fatal follow on the same stream.
#[test]
fn test_debug_info_exception_is_uncatchable_var_dump_nested() {
    let out = compile_and_run_capture(r#"<?php
class Inner {
    public function __debugInfo(): array { throw new RuntimeException("boom"); }
}
class Outer { public $inner; }
$outer = new Outer();
$outer->inner = new Inner();
try {
    var_dump($outer);
} catch (Throwable $e) {
    echo "caught\n";
} finally {
    echo "outer finally\n";
}
echo "unreachable\n";
"#);
    assert!(
        out.stdout.starts_with("object(Outer)#1 (1) {\n  [\"inner\"]=>\n  \nWarning: Uncaught RuntimeException: boom"),
        "{}",
        out.stdout
    );
    assert_debug_info_fatal(&out);
}

/// Asserts stdout is exactly `head` followed by PHP's report: the warning with its ` in <file>:3` location, the trace
/// (the `__debugInfo()` frame, then `{main}`), the `thrown in` line and the fatal naming the dump call on line 9
/// (PHP 8.5.10 prints the same bytes, apart from the `[internal function]` and `print_r()` frames elephc does not list).
fn assert_debug_info_exception_report(out: &ProgramOutput, head: &str) {
    assert_debug_info_fatal(out);
    let warning = format!("{head}\nWarning: Uncaught RuntimeException: boom in ");
    assert!(out.stdout.starts_with(&warning), "{}", out.stdout);
    let rest = &out.stdout[warning.len()..];
    assert!(rest.contains(":3\nStack trace:\n#0 "), "{}", out.stdout);
    assert!(rest.contains("Inner->__debugInfo()\n#1 {main}\n  thrown in "), "{}", out.stdout);
    assert!(rest.contains(" on line 3\n\nFatal error: __debuginfo() must return an array in "), "{}", out.stdout);
    assert!(rest.ends_with(" on line 9\n"), "{}", out.stdout);
}

/// `print_r` renders into a buffer in PHP, so a fatal inside the walk prints none of the partial text: nested in an
/// object, inside an array, in return mode, and under `ob_start` (the output buffer keeps only what was echoed).
#[test]
fn test_debug_info_exception_is_uncatchable_print_r_prints_no_partial_text() {
    let classes = r#"<?php
class Inner {
    public function __debugInfo(): array { throw new RuntimeException("boom"); }
}
class Outer { public $a = 1; public $inner; }
$outer = new Outer();
$outer->inner = new Inner();
echo "start\n";
"#;
    let tail = r#"
catch (Throwable $e) { echo "caught\n"; } finally { echo "outer finally\n"; }
echo "unreachable\n";
"#;
    let nested = compile_and_run_capture(&format!("{classes}try {{ print_r($outer); }}{tail}"));
    assert_debug_info_exception_report(&nested, "start\n");

    let in_array = compile_and_run_capture(&format!("{classes}try {{ print_r([1, [\"k\" => $outer], 3]); }}{tail}"));
    assert_debug_info_exception_report(&in_array, "start\n");

    let returned = compile_and_run_capture(&format!("{classes}try {{ $t = print_r($outer, true); echo \"got\\n\"; }}{tail}"));
    assert_debug_info_exception_report(&returned, "start\n");

    let buffered = compile_and_run_capture(&format!(
        "{classes}try {{ ob_start(); echo \"buffered;\"; print_r($outer); echo \"never\"; ob_end_flush(); }}{tail}"
    ));
    assert_debug_info_exception_report(&buffered, "start\nbuffered;");

    let runtime_flag = compile_and_run_capture(&format!(
        "{classes}$flag = $_SERVER[\"argc\"] > 5; try {{ print_r($outer, $flag); }}{tail}"
    ));
    assert_debug_info_exception_report(&runtime_flag, "start\n");
}

/// The deferred `print_r` buffer must survive a `__debugInfo()` that echoes or calls `print_r(.., true)` itself: the
/// echo lands before the whole print_r text (PHP builds the text first) and the nested capture does not clobber the
/// outer one.
#[test]
fn test_debug_info_print_r_buffer_survives_nested_user_code() {
    let out = compile_and_run_capture(r#"<?php
class Chatty {
    public $v = 7;
    public function __debugInfo(): array {
        echo "[echo]";
        return ["inner" => print_r([1, 2], true), "v" => $this->v];
    }
}
$o = new stdClass();
$o->a = new Chatty();
$o->b = 5;
print_r($o);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "[echo]stdClass Object\n",
            "(\n",
            "    [a] => Chatty Object\n",
            "        (\n",
            "            [inner] => Array\n",
            "(\n",
            "    [0] => 1\n",
            "    [1] => 2\n",
            ")\n",
            "\n",
            "            [v] => 7\n",
            "        )\n",
            "\n",
            "    [b] => 5\n",
            ")\n",
        )
    );
}

/// Output beyond the 64 KiB capture buffer is streamed out instead of truncated, in a program that defers `print_r`.
#[test]
fn test_debug_info_print_r_deferred_buffer_streams_large_output() {
    let out = compile_and_run_capture(r#"<?php
class H { public function __debugInfo() { return ["n" => 1]; } }
$h = new H();
$w = [];
for ($i = 0; $i < 20; $i++) {
    $w[] = str_repeat(chr(65 + $i), 6000);
}
print_r($w);
echo "end\n";
"#);
    assert!(out.success, "{}", out.stderr);
    let mut expected = String::from("Array\n(\n");
    for i in 0..20u8 {
        expected.push_str(&format!("    [{i}] => {}\n", char::from(65 + i).to_string().repeat(6000)));
    }
    expected.push_str(")\nend\n");
    assert!(expected.len() > 120_000);
    assert_eq!(out.stdout, expected);
}

/// A `try`/`catch` INSIDE `__debugInfo()` still handles its own exception normally; only an escaping one is fatal.
#[test]
fn test_debug_info_exception_caught_inside_the_method_is_not_fatal() {
    let out = compile_and_run_capture(r#"<?php
class Careful {
    public function __debugInfo(): array {
        try {
            throw new RuntimeException("inner");
        } catch (RuntimeException $e) {
            return ["handled" => $e->getMessage()];
        }
    }
}
var_dump(new Careful());
print_r(new Careful());
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "object(Careful)#1 (1) {\n",
            "  [\"handled\"]=>\n",
            "  string(5) \"inner\"\n",
            "}\n",
            "Careful Object\n",
            "(\n",
            "    [handled] => inner\n",
            ")\n",
            "\n",
        )
    );
}

/// A null return is an empty array plus PHP 8.5's `Returning null from C::__debugInfo() is deprecated` diagnostic
/// (stderr, like every elephc runtime diagnostic), once per dump.
#[test]
fn test_debug_info_null_return_emits_deprecation() {
    let out = compile_and_run_capture(r#"<?php
class Nully {
    public function __debugInfo() { return $_SERVER["argc"] > 0 ? null : []; }
}
var_dump(new Nully());
print_r(new Nully());
echo "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "object(Nully)#1 (0) {\n",
            "}\n",
            "Nully Object\n",
            "(\n",
            ")\n",
            "\n",
        )
    );
    let line = "Deprecated: Returning null from Nully::__debugInfo() is deprecated, return an empty array instead";
    assert_eq!(out.stderr.matches(line).count(), 2, "{}", out.stderr);
}

/// An untyped `__debugInfo()` the checker infers `void` still runs, as PHP calls it. One that only
/// throws reports `Warning: Uncaught` and the fatal; one that falls off its end is a null return.
/// Both used to be skipped, so the dump showed the declared properties instead.
#[test]
fn test_debug_info_returning_nothing_still_runs() {
    let thrower = compile_and_run_capture(r#"<?php
class Inner {
    public function __debugInfo() { throw new RuntimeException("boom"); }
}
class Outer { public $a = 1; public $inner; }
$outer = new Outer();
$outer->inner = new Inner();
echo "start\n";
try { print_r($outer); } catch (Throwable $e) { echo "caught\n"; }
"#);
    assert_debug_info_exception_report(&thrower, "start\n");

    let silent = compile_and_run_capture(r#"<?php
class Quiet {
    public $a = 1;
    public function __debugInfo() { echo "called\n"; }
}
var_dump(new Quiet());
"#);
    assert!(silent.success, "{}", silent.stderr);
    assert_eq!(silent.stdout, "called\nobject(Quiet)#1 (0) {\n}\n");
    assert!(silent.stderr.contains("Deprecated: Returning null from Quiet::__debugInfo() is deprecated"), "{}", silent.stderr);
}

/// A nested `print_r()` of another `__debugInfo()` object inside `__debugInfo()` must not leak its dump state
/// into the enclosing `var_dump`: when the outer method then returns a non-array or throws, PHP 8.5 still prints
/// the nested value's indent before the report and names the outer `var_dump` call (line 11), not the inner
/// `print_r` (line 6).
#[test]
fn test_debug_info_nested_dump_keeps_outer_var_dump_indent() {
    // The throwing variant keeps `: array` (the checker refuses the non-array return under it).
    let program = |ret: &str, tail: &str| {
        format!(
            r#"<?php
class B {{
    public function __debugInfo(): array {{ return ["b" => $_SERVER["argc"]]; }}
}}
class A {{
    public function __debugInfo(){ret} {{ $t = print_r(new B(), true); {tail} }}
}}
class Holder {{ public $a; }}
$h = new Holder();
$h->a = new A();
var_dump($h);
echo "unreachable\n";
"#
        )
    };
    let head = "object(Holder)#1 (1) {\n  [\"a\"]=>\n  \n";
    let invalid = compile_and_run_capture(&program("", r#"return $_SERVER["argc"] > 0 ? 5 : [];"#));
    assert_debug_info_fatal(&invalid);
    assert!(
        invalid.stdout.starts_with(&format!("{head}Fatal error: __debuginfo() must return an array in ")),
        "{}",
        invalid.stdout
    );
    assert!(invalid.stdout.ends_with(" on line 11\n"), "{}", invalid.stdout);

    let thrown = compile_and_run_capture(&program(": array", r#"throw new RuntimeException("boom");"#));
    assert_debug_info_fatal(&thrown);
    assert!(
        thrown.stdout.starts_with(&format!("{head}Warning: Uncaught RuntimeException: boom in ")),
        "{}",
        thrown.stdout
    );
    assert!(thrown.stdout.ends_with(" on line 11\n"), "{}", thrown.stdout);
}
