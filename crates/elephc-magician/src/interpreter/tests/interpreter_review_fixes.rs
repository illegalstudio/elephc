//! Purpose:
//! Interpreter regression tests for the review findings fixed together: generator keys, throw and
//! loop ownership, `??` warnings, anonymous class names, `assert()` ownership, `filter_var()`
//! options and the default SPL loader.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::interpreter_review_fixes`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.10's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed plus the warnings it raised.
fn run(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    (values.output.clone(), values.warnings.clone())
}

/// Verifies an explicit integer generator key moves the next automatic key past it.
///
/// php prints `5 6 x 7 -9 8 `: the key after `5` is `6`, a string key changes nothing, and a
/// negative key is below the largest used one.
#[test]
fn generator_auto_keys_continue_after_the_largest_explicit_integer_key() {
    let (output, _) = run(
        br#"function g() { yield 5 => 'a'; yield 'b'; yield 'x' => 'c'; yield 'd'; yield -9 => 'e'; yield 'f'; }
foreach (g() as $k => $v) echo $k, ' ';"#,
    );
    assert_eq!(output, "5 6 x 7 -9 8 ");
}

/// Verifies `Generator::throw()` raises at the suspended yield, caught or not.
///
/// php prints `1;caught:boom;99;100|` for the caught case and `out:late;` when the generator is
/// already past its last yield and the exception leaves `throw()` itself.
#[test]
fn generator_throw_lands_on_the_suspended_yield() {
    let (output, _) = run(
        br#"function t() { try { $x = yield 1; echo "got:", $x, ";"; } catch (RuntimeException $e) { echo "caught:", $e->getMessage(), ";"; yield 99; } yield 100; }
$g = t();
echo $g->current(), ";";
echo $g->throw(new RuntimeException('boom')), ";";
$g->next();
echo $g->current(), "|";
function u() { yield 1; yield 2; }
$h = u();
try { $h->throw(new LogicException('late')); } catch (LogicException $e) { echo "out:", $e->getMessage(), ";"; }
echo $h->valid() ? 'valid' : 'closed';"#,
    );
    assert_eq!(output, "1;caught:boom;99;100|out:late;closed");
}

/// Verifies nested lowered `foreach` loops in a generator walk every inner subject.
///
/// php prints `12345`; the inner slot is reused by each outer iteration.
#[test]
fn nested_generator_foreach_reuses_its_slot_without_losing_subjects() {
    let (output, _) = run(
        br#"function nest($groups) { foreach ($groups as $group) { foreach ($group as $item) { yield $item; } } }
foreach (nest([[1,2],[3],[4,5]]) as $v) echo $v;"#,
    );
    assert_eq!(output, "12345");
}

/// Verifies the left operand of `??` stays quiet but a callee body still warns.
///
/// php raises `Undefined variable $undef` from inside `f()` even though the call sits under `??`.
#[test]
fn null_coalesce_does_not_silence_a_callee_bodys_warnings() {
    let (output, warnings) = run(
        br#"function f() { return $undef; }
$r = f() ?? 'd';
echo $r, ($missing ?? 'q');"#,
    );
    assert_eq!(output, "dq");
    assert_eq!(warnings, vec!["Undefined variable $undef".to_string()]);
}

/// Verifies an anonymous class is named as php's `get_debug_type()` does.
///
/// php prints `class@anonymous,Base@anonymous,Iface@anonymous`.
#[test]
fn get_debug_type_hides_the_internal_anonymous_class_suffix() {
    let (output, _) = run(
        br#"class Base {}
interface Iface {}
echo get_debug_type(new class {}), ',', get_debug_type(new class extends Base {}), ',',
    get_debug_type(new class implements Iface {});"#,
    );
    assert_eq!(output, "class@anonymous,Base@anonymous,Iface@anonymous");
}

/// Verifies `UnhandledMatchError` names an interpreter-declared object by its own class.
#[test]
fn unhandled_match_error_names_the_declared_class() {
    let (output, _) = run(
        br#"class Foo {}
try { echo match (new Foo) { 1 => 'a' }; } catch (\UnhandledMatchError $e) { echo $e->getMessage(); }"#,
    );
    assert_eq!(output, "Unhandled match case of type Foo");
}

/// Verifies a throwable passed to `assert()` as a variable survives the catch that releases it.
#[test]
fn assert_throws_an_owned_throwable_so_the_variable_stays_valid() {
    let program = parse_fragment(
        br#"$e = new LogicException('x');
try { assert(false, $e); } catch (LogicException $c) {}
echo $e->getMessage();"#,
    )
    .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute fragment");
    assert_eq!(values.output, "x");
    assert!(values.over_releases().is_empty());
}

/// Verifies `filter_var()` honours php's options array and the IP range flags.
///
/// php prints `false,3,false,8.8.8.8,12,NULL` for these six calls.
#[test]
fn filter_var_reads_options_defaults_ranges_and_ip_flags() {
    let (output, _) = run(
        br#"echo var_export(filter_var('7', FILTER_VALIDATE_INT, ['options' => ['min_range' => 10]]), true), ',';
echo var_export(filter_var('x', FILTER_VALIDATE_INT, ['options' => ['default' => 3]]), true), ',';
echo var_export(filter_var('10.0.0.1', FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE), true), ',';
echo var_export(filter_var('8.8.8.8', FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE), true), ',';
echo var_export(filter_var('12', FILTER_VALIDATE_INT, ['options' => ['min_range' => 10, 'max_range' => 20]]), true), ',';
echo var_export(filter_var('x', FILTER_VALIDATE_INT, ['flags' => FILTER_NULL_ON_FAILURE]), true);"#,
    );
    assert_eq!(output, "false,3,false,'8.8.8.8',12,NULL");
}

/// Verifies a filter the interpreter does not implement refuses instead of answering `false`.
#[test]
fn filter_var_refuses_an_unimplemented_filter() {
    let program = parse_fragment(b"return filter_var('a@b.co', FILTER_VALIDATE_EMAIL);")
        .expect("parse fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(
        execute_program(&program, &mut scope, &mut values).err(),
        Some(EvalStatus::UnsupportedConstruct),
    );
}

/// Verifies a string with a real character of the byte-marker block keeps its UTF-8 bytes.
///
/// php prints `4f3b08181` for `strlen` then `bin2hex` of `"\u{F0041}"`.
#[test]
fn a_marker_block_character_in_a_string_is_not_read_as_a_raw_byte() {
    let (output, _) = run(b"echo strlen(\"\\u{F0041}\"), bin2hex(\"\\u{F0041}\");");
    assert_eq!(output, "4f3b08181");
}

/// Verifies `spl_autoload()` is the default loader and never runs the registered callbacks.
///
/// php prints `done` only: nothing is registered by name, and no `lib/foo.php` exists.
#[test]
fn spl_autoload_does_not_call_the_registered_loaders() {
    let (output, _) = run(
        br#"spl_autoload_register(function ($c) { echo "load $c;"; });
spl_autoload('Foo2');
echo 'done';"#,
    );
    assert_eq!(output, "done");
}
