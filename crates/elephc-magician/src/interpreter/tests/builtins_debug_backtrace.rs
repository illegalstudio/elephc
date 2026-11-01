//! Purpose:
//! Interpreter tests for `debug_backtrace()` and `debug_print_backtrace()`.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Every expected frame shape here was measured with `php -n` 8.5.6 and is quoted beside the
//!   assertion. The `file` and `line` values are the only part these tests do not pin to PHP:
//!   a fragment executed by the test harness has no source file, so both keys carry the
//!   harness's empty call site. Their PRESENCE and position are still pinned.
//! - The autoload test is the reason this builtin exists at all: Symfony's
//!   `ClassExistenceResource::throwOnRequiredClass` returns silently when the frame above it
//!   names a class probe and has no `class` key, and throws when the trace is empty.

use super::super::*;
use super::support::*;

/// Verifies eval `debug_backtrace()` reports method, static-method, and plain-function frames
/// with PHP's keys, in PHP's order, innermost first.
///
/// `php -n` 8.5.6 prints, for the same program:
///   `file,line,function,class,object,type,args|inner|BtK|->|obj|A;`
///   `file,line,function,class,object,type,args|m|BtK|->|obj|A;`
///   `file,line,function,args|btOuter|-|-|-|A;`
/// and for the static chain:
///   `file,line,function,class,type,args|deep|BtK|::|-|B;`
///   `file,line,function,class,type,args|s|BtK|::|-|B;`
#[test]
fn execute_program_reports_eval_call_frames_for_debug_backtrace() {
    let program = parse_fragment(
        br#"function shape($f) {
    $s = implode(",", array_keys($f));
    $s = $s . "|" . $f['function'];
    $s = $s . "|" . (array_key_exists('class', $f) ? $f['class'] : '-');
    $s = $s . "|" . (array_key_exists('type', $f) ? $f['type'] : '-');
    $s = $s . "|" . (array_key_exists('object', $f) ? 'obj' : '-');
    $s = $s . "|" . (array_key_exists('args', $f) ? implode(",", $f['args']) : '-');
    return $s;
}
class BtK {
    public function m($x) { return $this->inner($x); }
    public function inner($x) { return debug_backtrace(); }
    public static function s($y) { return self::deep($y); }
    public static function deep($y) { return debug_backtrace(); }
}
function btOuter($a) { return (new BtK())->m($a); }
foreach (btOuter("A") as $f) { echo shape($f); echo ";"; }
echo "\n";
foreach (BtK::s("B") as $f) { echo shape($f); echo ";"; }
return count(btOuter("A"));"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "file,line,function,class,object,type,args|inner|BtK|->|obj|A;",
            "file,line,function,class,object,type,args|m|BtK|->|obj|A;",
            "file,line,function,args|btOuter|-|-|-|A;",
            "\n",
            "file,line,function,class,type,args|deep|BtK|::|-|B;",
            "file,line,function,class,type,args|s|BtK|::|-|B;",
        )
    );
    assert_eq!(values.get(result), FakeValue::Int(3));
}

/// Verifies eval `debug_backtrace()` honours `DEBUG_BACKTRACE_IGNORE_ARGS`, a cleared
/// `DEBUG_BACKTRACE_PROVIDE_OBJECT`, and `$limit`.
///
/// `php -n` 8.5.6 prints `2:l1:l2:file,line,function` for the limited trace, and the key lists
/// `file,line,function,args`, `file,line,function,class,object,type,args` and
/// `file,line,function,class,type,args` for the three option probes.
#[test]
fn execute_program_applies_debug_backtrace_options_and_limit() {
    let program = parse_fragment(
        br#"function l1() { return debug_backtrace(2, 2); }
function l2() { return l1(); }
function l3() { return l2(); }
$lim = l3();
echo count($lim); echo ":"; echo $lim[0]['function']; echo ":"; echo $lim[1]['function'];
echo ":"; echo implode(",", array_keys($lim[0])); echo "\n";
function zeroOpt() { $t = debug_backtrace(0); return implode(",", array_keys($t[0])) . "/" . implode(",", array_keys($t[1])); }
class ZeroC { public function go() { return zeroOpt(); } }
echo (new ZeroC())->go(); echo "\n";
function keepObj() { $t = debug_backtrace(); return implode(",", array_keys($t[1])); }
class KeepC { public function go() { return keepObj(); } }
echo (new KeepC())->go();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "2:l1:l2:file,line,function\n",
            "file,line,function,args/file,line,function,class,type,args\n",
            "file,line,function,class,object,type,args",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a class probe that runs an autoloader appears above it in the trace with PHP's
/// shape: the probe's own name, its arguments, and NO `class` key.
///
/// This is the frame `ClassExistenceResource::throwOnRequiredClass` looks for; without it the
/// same code raises `ReflectionException` instead of returning. `php -n` 8.5.6 reports
/// `count=2`, frame 1 `function=class_exists`, no `class` key, `args=["MissingProbeOne"]`.
/// PHP omits `file`/`line` on the engine-invoked autoloader frame; the interpreter carries the
/// caller's, which is why this test reads named keys rather than the key list.
#[test]
fn execute_program_shows_the_class_probe_frame_to_its_autoloader() {
    let program = parse_fragment(
        br#"function probeAutoload($n) {
    $t = debug_backtrace();
    echo count($t); echo ":";
    echo $t[1]['function']; echo ":";
    echo array_key_exists('class', $t[1]) ? 'hasclass' : 'noclass'; echo ":";
    echo $t[1]['args'][0]; echo ":";
    echo $t[0]['function']; echo ":";
    echo $t[0]['args'][0]; echo ";";
}
spl_autoload_register('probeAutoload');
class_exists('MissingProbeOne');
spl_autoload_call('MissingProbeTwo');
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "2:class_exists:noclass:MissingProbeOne:probeAutoload:MissingProbeOne;",
            "2:spl_autoload_call:noclass:MissingProbeTwo:probeAutoload:MissingProbeTwo;",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `debug_print_backtrace()` prints one `#N FILE(LINE): call()` line per frame,
/// innermost first, with no trailing `{main}` line.
///
/// `php -n` 8.5.6 prints, for the same shape:
///   `#0 /path/file.php(31): pFn()`
///   `#1 /path/file.php(32): pOuter()`
/// A harness fragment has no source file, so the file is empty and the line is `0` here; the
/// `#N`, the parentheses, the separator and the call spelling are the pinned part.
#[test]
fn execute_program_prints_eval_call_frames_for_debug_print_backtrace() {
    let program = parse_fragment(
        br#"class PrintC { public function go() { pFn(); } }
function pFn() { debug_print_backtrace(); }
function pOuter() { (new PrintC())->go(); }
pOuter();
return function_exists('debug_print_backtrace');"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "#0 (0): pFn()\n",
            "#1 (0): PrintC->go()\n",
            "#2 (0): pOuter()\n",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies both backtrace functions are visible to `function_exists()` and reachable through a
/// dynamic callable, so they are not half-registered.
///
/// `php -n` 8.5.6 prints `yes:yes:1:dynOuter` for the same program.
#[test]
fn execute_program_dispatches_debug_backtrace_through_a_dynamic_callable() {
    let program = parse_fragment(
        br#"function dynOuter() { return call_user_func('debug_backtrace', 2); }
$t = dynOuter();
echo function_exists('debug_backtrace') ? 'yes' : 'no'; echo ":";
echo function_exists('debug_print_backtrace') ? 'yes' : 'no'; echo ":";
echo count($t); echo ":"; echo $t[0]['function'];
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "yes:yes:1:dynOuter");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
