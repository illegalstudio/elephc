//! Purpose:
//! End-to-end coverage for values bound to a declared `callable` parameter or returned through
//! a `callable` return type: they keep the PHP value the caller passed.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is the PHP 8.5 output of the same fixture. PHP appends
//!   `, called in FILE on line N` to a userland binding `TypeError`; elephc's entry guards stop
//!   at `given`, like its other parameter guards, so the fixtures print the message up to there.
//! - A declared `callable` is stored as a boxed value (`PhpType::DeclaredCallable`); only the
//!   sites that invoke it (or hand it to a descriptor-typed slot) build a callable descriptor.

use crate::support::*;

/// Verifies strings, arrays, invokable objects and closures keep their PHP type and value
/// inside a `callable` parameter and through a `callable` return, while still invoking.
///
/// Before declared callables kept their value, every one of them arrived as a `Closure`:
/// `gettype()` said `object`, `is_string()` was false and `ctype_alpha("strrev")` deprecated
/// a `Closure` instead of scanning the name.
#[test]
fn test_declared_callable_parameters_keep_php_values() {
    let out = compile_and_run_capture(
        r#"<?php
class Shout { public function __invoke(string $s): string { return strtoupper($s); } }
class Label { public function wrap(string $s): string { return "[$s]"; } public static function tag(string $s): string { return "<$s>"; } }
function describe(callable $f): string {
    return gettype($f) . ":" . (is_string($f) ? "s" : "-") . (is_array($f) ? "a" : "-")
        . (is_object($f) ? "o" : "-") . (ctype_alpha($f) ? "A" : "-") . ":" . $f("hi");
}
echo describe("strrev"), "\n";
echo describe("Label::tag"), "\n";
echo describe([new Label(), "wrap"]), "\n";
echo describe(new Shout()), "\n";
echo describe(fn(string $s): string => "($s)"), "\n";
echo describe(ucfirst(...)), "\n";
function keep(callable $f): callable { return $f; }
var_dump(keep("strlen"), keep(["Label", "tag"]));
echo keep("ucfirst")("word"), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string:s--A:ih\nstring:s---:<hi>\narray:-a--:[hi]\nobject:--o-:HI\nobject:--o-:(hi)\n\
         object:--o-:Hi\nstring(6) \"strlen\"\narray(2) {\n  [0]=>\n  string(5) \"Label\"\n  [1]=>\n  \
         string(3) \"tag\"\n}\nWord\n"
    );
    let deprecation = |type_name: &str| {
        format!(
            "Deprecated: ctype_alpha(): Argument of type {type_name} will be interpreted as string in the future"
        )
    };
    let expected = [deprecation("array"), deprecation("Shout"), deprecation("Closure"), deprecation("Closure")];
    assert_eq!(normalize_diagnostic_text(&out.stderr), expected.join("\n") + "\n");
}

/// Verifies runtime-selected names, branch-selected and instance callable arrays, and spread
/// arguments bind to a declared `callable` and invoke, as do the pipe operator, builtin
/// callbacks and `bindTo()` on the bound value.
///
/// These used to be refused at compile time because a `callable` parameter needed a
/// statically resolved descriptor target.
#[test]
fn test_declared_callable_accepts_runtime_selected_targets() {
    let out = compile_and_run(
        r#"<?php
class BranchCallable { public static function left(): int { return 1; } public static function right(): int { return 2; } }
class InstanceCallable { public function hit(): int { return 3; } }
function consume(callable $callback): int { return $callback(); }
function choose(int $value): int {
    if ($value > 0) { $callback = [BranchCallable::class, 'left']; } else { $callback = [BranchCallable::class, 'right']; }
    return consume($callback);
}
function chooseInstance(int $value): int {
    $receiver = new InstanceCallable();
    $callback = $value > 0 ? [$receiver, 'hit'] : [new InstanceCallable(), 'hit'];
    return consume($callback);
}
function apply(callable $f, string $s): string { return $f($s); }
$name = $argc > 0 ? "strtoupper" : "strtolower";
$callbacks = [[BranchCallable::class, 'right']];
echo choose($argc), chooseInstance($argc), apply($name, "a"), consume(...$callbacks), "\n";
function piped(callable $f, string $s): string { return $s |> $f; }
function mapped(callable $f): string { return implode(",", array_map($f, ["a", "b"])); }
function rebound(callable $f): string { return $f->bindTo(new InstanceCallable())(); }
echo piped("strrev", "abc"), " ", mapped("strtoupper"), " ", rebound(function () { return get_class($this); }), "\n";
"#,
    );
    assert_eq!(out, "13A2\ncba A,B InstanceCallable\n");
}

/// Verifies a builtin whose name exists only at run time, never as a string literal at a
/// `callable` argument, raises a catchable `Error` naming it when a declared `callable` invokes
/// it, while runtime-selected user functions still run.
///
/// This is a documented elephc limit, not PHP output (PHP prints `A`): a declared `callable`
/// links only the builtins named as literals, so a binary does not carry a wrapper for every
/// builtin. The call used to stop with an uncatchable `Call to undefined function <dynamic>()`.
#[test]
fn test_declared_callable_runtime_only_builtin_raises_a_named_error() {
    let out = compile_and_run(
        r#"<?php
function shout(string $s): string { return strtoupper($s) . "!"; }
function apply(callable $f, string $s): string { return $f($s); }
$builtin = strrev("reppuotrts");
$user = strrev("tuohs");
echo is_callable($builtin) ? "callable" : "not callable", "\n";
echo apply($user, "a"), "\n";
try { echo apply($builtin, "a"), "\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        "callable\nA!\nError: Cannot call strtoupper() through a declared callable: a builtin named \
only at run time is not linked; pass its name as a string literal or as a first-class callable\nafter\n"
    );
}

/// Verifies `Closure::fromCallable()` of a declared `callable` resolves runtime names exactly as
/// invoking the value does: literal builtins and runtime-selected user functions convert, and a
/// builtin named only at run time raises the same named `Error` from a function and from a
/// class scope.
///
/// Like the invocation, this is a documented elephc limit, not PHP output (PHP converts
/// `strtoupper`). The conversion used to accept every builtin, so a single `fromCallable()` of a
/// declared `callable` linked a wrapper for each one.
#[test]
fn test_declared_callable_closure_factory_matches_invocation_names() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function shout(string $s): string { return strtoupper($s) . "!"; }
function make(callable $f): Closure { return Closure::fromCallable($f); }
class Holder {
    private Closure $f;
    public function __construct(callable $f) { $this->f = Closure::fromCallable($f); }
    public function run(string $s): string { return ($this->f)($s); }
}
$builtin = strrev("reppuotrts");
echo make("strrev")("abc"), " ", make(strrev("tuohs"))("a"), " ", (new Holder("strrev"))->run("xy"), "\n";
try { make($builtin); echo "bad\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { new Holder($builtin); echo "bad\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "after\n";
"#,
    );
    let message = "Error: Cannot call strtoupper() through a declared callable: a builtin named only \
at run time is not linked; pass its name as a string literal or as a first-class callable\n";
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("cba A! yx\n{message}{message}after\n"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies the restricted builtin set applies only to a value typed exactly `callable`: a
/// `?callable` union and a declared `callable` handed to `array_map()` or `call_user_func()` go
/// through the generic dynamic call and call a runtime-selected builtin as PHP does.
#[test]
fn test_declared_callable_unions_and_builtin_callbacks_call_runtime_builtins() {
    let out = compile_and_run(
        r#"<?php
function nullable(?callable $f, string $s): string { return $f === null ? "-" : $f($s); }
function mapped(callable $f): string { return implode(",", array_map($f, ["a", "b"])); }
function via(callable $f, string $s): string { return call_user_func($f, $s); }
$builtin = strrev("reppuotrts");
echo nullable($builtin, "a"), " ", mapped($builtin), " ", via($builtin, "c"), "\n";
"#,
    );
    assert_eq!(out, "A A,B C\n");
}

/// Verifies the named `Error` for a runtime-only builtin also fires when the invoking function
/// owns an eval context: that context resolves every builtin, so the error check runs before
/// its fallback (a name the native table refuses, such as an eval-only function, still reaches
/// the fallback).
///
/// The fixture's `eval()` declares a function, so the function keeps a live eval context slot;
/// a fully static fragment such as `$unused = 1;` would not, and the call used to resolve
/// `strtoupper` through the eval frame.
#[test]
fn test_declared_callable_runtime_only_builtin_error_with_eval_fallback() {
    let out = compile_and_run(
        r#"<?php
function apply(callable $f, string $s): string {
    if (!function_exists("evalOnly")) {
        eval('function evalOnly(string $s): string { return "eval:" . $s; }');
    }
    return $f($s);
}
$builtin = strrev("reppuotrts");
try { echo apply($builtin, "a"), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo apply("strlen", "abc"), "\n";
echo "after\n";
"#,
    );
    assert_eq!(
        out,
        "Cannot call strtoupper() through a declared callable: a builtin named only at run time \
is not linked; pass its name as a string literal or as a first-class callable\n3\nafter\n"
    );
}

/// Verifies a declared `callable` in a program with no user function and no static method, so
/// with no runtime candidate at all, still raises the named `Error` for a runtime-only builtin
/// instead of linking a wrapper for every builtin.
///
/// The empty candidate set used to be widened to every builtin, so the call returned `AB`.
#[test]
fn test_declared_callable_without_candidates_keeps_the_named_error() {
    let out = compile_and_run(
        r#"<?php
$apply = function (callable $f): string { return $f("ab"); };
try { echo $apply(strrev("reppuotrts")), "\n"; } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "Error: Cannot call strtoupper() through a declared callable: a builtin named only at run \
time is not linked; pass its name as a string literal or as a first-class callable\n"
    );
}

/// Verifies a builtin named as a literal at a callable union parameter (`?callable`,
/// `callable|false`) or a callable variadic element (`?callable ...$fs`, `callable ...$fs`)
/// still invokes when it reaches the callee as a string.
///
/// `ucfirst`, `lcfirst` and `ucwords` have no runtime-selected wrapper, so only a recorded
/// literal name links one; each form names a different builtin so none rides on another's
/// record. Only an exact `callable` parameter used to record it, so these calls stopped with a
/// runtime error.
#[test]
fn test_declared_callable_unions_keep_literal_builtin_names_invokable() {
    let out = compile_and_run(
        r#"<?php
function nullable(?callable $f): string { return $f === null ? "" : $f("word"); }
function orFalse(callable|false $f): string { return $f === false ? "" : $f("word"); }
function many(?callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f === null ? "-" : $f("WORD"); } return $out; }
function every(callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f("word"); } return $out; }
echo nullable("ucfirst"), " ", orFalse("strrev"), " ", many("lcfirst", null), " ", every("ucwords"), "\n";
"#,
    );
    assert_eq!(out, "Word drow wORD- Word\n");
}

/// Verifies a literal builtin name reaching a declared callable through a named variadic entry
/// or a static spread is recorded like a positional literal, so it still invokes.
///
/// The call planner wraps a keyed variadic entry as a named argument, keeps a spread that only
/// fills the variadic whole, and rewrites one into a regular parameter as an access into the
/// literal; none of those nodes was a bare string literal, so the name was never linked.
#[test]
fn test_declared_callable_named_and_spread_literal_builtin_names_invoke() {
    let out = compile_and_run(
        r#"<?php
function every(callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f("word"); } return $out; }
function apply(callable $f): string { return $f("word"); }
function many(?callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f === null ? "-" : $f("WORD"); } return $out; }
echo every(foo: "ucwords"), " ", apply(...["ucfirst"]), " ", many(...["lcfirst", null]), "\n";
"#,
    );
    assert_eq!(out, "Word Word wORD-\n");
}

/// Verifies literal names spread through several static spreads, mixed with named arguments or
/// behind a projection with a runtime key still invoke, and that the checker never binds a
/// spread element to the wrong parameter.
///
/// Spread elements used to be matched to parameters at check time: a second spread restarted at
/// the first open parameter (a false `Undefined function` compile error for `"no_such_function"`
/// bound to a `string`), a spread split by a named argument or holding a closure was never
/// recorded, and a keyed literal resolved an entry a runtime key replaces. Each form names a
/// different builtin so none rides on another's record.
#[test]
fn test_declared_callable_spread_literals_follow_php_binding() {
    let out = compile_and_run(
        r#"<?php
function pair(string $a, callable $b): string { return $b($a); }
function pair2(callable $a, string $b): string { return $a($b); }
function both(callable $a, callable ...$rest): string { $out = $a("Ab"); foreach ($rest as $f) { $out .= $f("Ab"); } return $out; }
function both2(callable $a, callable ...$rest): string { $out = $a("Ab"); foreach ($rest as $f) { $out .= $f("Ab"); } return $out; }
function apply(callable $f): string { return (string) $f("word"); }
function run(string $key): string { return apply(["cb" => "missing_fn", $key => "bin2hex"]["cb"]); }
function every(callable ...$fs): int { return 1; }
echo pair(...["ab"], ...["strtolower"]), " ", pair2(...["ucfirst"], ...["no_such_function"]), "\n";
echo both(...["lcfirst", "strrev"], extra: "ucwords"), " ", both2(...["strtoupper", function (string $s): string { return $s; }], extra: "trim"), "\n";
echo run("cb"), "\n";
try { echo every(...["strlen", "no_such_function"]); } catch (TypeError $e) { echo "TypeError\n"; }
"#,
    );
    assert_eq!(out, "ab No_such_function\nabbAAb ABAbAb\n776f7264\nTypeError\n");
}

/// Verifies spread literals reach a declared callable when a named argument makes call planning
/// split the spread: the overflow of a spread past a non-callable parameter into a callable
/// variadic, and a spread filling an optional callable parameter (planned as
/// `count($spread) > 0 ? $spread[0] : default`). Ordinary text in a spread that names a builtin
/// needing a prelude (`gmp_abs`) must not break the build.
///
/// Those spreads were checked only through the regular slot they fill, so a string bound past a
/// `string` parameter or behind the default ternary was never recorded and stopped at run time.
#[test]
fn test_declared_callable_spread_literals_through_named_call_plans() {
    let out = compile_and_run(
        r#"<?php
function take(string $label, callable ...$fs): string { $out = $label; foreach ($fs as $f) { $out .= $f("Ab"); } return $out; }
function apply2(?callable $f = null, int $n = 1): string { return $f === null ? "none" : (string) $f("ab"); }
function text(callable $f, string $text): string { return $f($text); }
$empty = [];
echo take(...["x", "ucwords"], extra: "lcfirst"), " ", apply2(...["bin2hex"], n: 2), " ", apply2(...$empty, n: 2), " ", text(...["strrev", "gmp_abs"]), "\n";
"#,
    );
    assert_eq!(out, "xAbab 6162 none sba_pmg\n");
}

/// Verifies a literal builtin name bound to a `callable` parameter of a closure, an arrow
/// function, an immediately invoked closure or a pipe target invokes, and that a `callable`
/// return selected by `match` keeps every arm's builtin name.
///
/// Those calls bind without coercion, so the literal was never recorded for the runtime dispatch
/// and `$apply("strlen")` stopped with the runtime-only builtin error; a `match` return recorded
/// no arm at all. Each form names a different builtin so none rides on another's record.
#[test]
fn test_declared_callable_closure_params_and_match_returns_record_literal_names() {
    let out = compile_and_run(
        r#"<?php
$apply = function (callable $f) { return $f("ab"); };
$arrow = fn (callable $f) => $f("ab");
$piped = function (callable $f) { return $f("ab"); };
function pick(int $n): callable { return match ($n) { 0 => "strrev", default => "lcfirst" }; }
echo $apply("strlen"), " ", $arrow("ucfirst"), " ", (function (callable $f) { return $f("ab"); })("bin2hex"), " ", "strtoupper" |> $piped, " ", pick(0)("ab"), " ", pick(1)("AB"), "\n";
"#,
    );
    assert_eq!(out, "2 Ab 6162 AB ba aB\n");
}

/// Verifies a literal builtin name bound through an assignment or the `@` operator, which both
/// evaluate to the literal, is recorded like the bare literal, also inside a ternary arm.
///
/// Only a bare or named literal used to be recognized, so `apply($name = "strlen")` stopped with
/// the runtime-only builtin error.
#[test]
fn test_declared_callable_literal_behind_assignment_or_silence_invokes() {
    let out = compile_and_run(
        r#"<?php
function app(callable $f): string { return (string) $f("abc"); }
echo app($name = "strlen"), " ", app(@"strrev"), " ", $name, "\n";
echo app($argc > 0 ? ($picked = "ucfirst") : "strrev"), " ", $picked, "\n";
"#,
    );
    assert_eq!(out, "3 cba strlen\nAbc ucfirst\n");
}

/// Verifies a string-offset write passed to a `callable` parameter binds the byte it evaluates
/// to, not the name it writes: the name is neither checked at compile time nor recorded, and
/// the byte fails the entry check like any other non-callable string.
#[test]
fn test_declared_callable_string_offset_write_binds_its_byte() {
    let out = compile_and_run(
        r#"<?php
function app(callable $f): string { return (string) $f("abc"); }
function pick(): callable|string { $s = "x"; return ($s[0] = "no_such_function"); }
$s = "x";
try { echo app($s[0] = "no_such_function"), "\n"; } catch (TypeError $e) { echo "TypeError\n"; }
echo pick(), " ", $s, "\n";
"#,
    );
    assert_eq!(out, "TypeError\nn n\n");
}

/// Verifies a builtin name assigned to a property, to an element of an array local or `array`
/// parameter, or to an element of a property declared `array` at the call is the bound
/// callable, as the assignment evaluates to it, so the builtin is linked and runs.
#[test]
fn test_declared_callable_property_and_array_element_writes_bind_the_name() {
    let out = compile_and_run(
        r#"<?php
class Box { public $cb; public array $items = []; }
function app(callable $f): string { return (string) $f("abc"); }
function pick(): callable { $a = []; return ($a[0] = "ucwords"); }
function pass(array $items): string { return app($items[0] = "ucwords"); }
$box = new Box();
$a = [""];
echo app($box->cb = "ucfirst"), " ", app($a[0] = "strrev"), " ", pick()("a b"), " ", $box->cb, "\n";
echo pass([""]), " ", app($box->items[0] = "lcfirst"), "\n";
"#,
    );
    assert_eq!(out, "Abc cba A B ucfirst\nAbc abc\n");
}

/// Verifies a GMP builtin named as a literal argument or `callable` return value (also as an arm
/// of a ternary or a literal concatenation), or passed to a fresh invokable object or a
/// first-class callable of a named function, invokes as the call `gmp_abs(...)` would, and that
/// a closure variable, whose target is unknown before types, needs `gmp_abs(...)`.
///
/// The literal used to be recorded without injecting the GMP prelude, so its wrapper had no
/// helper to forward to and the call stopped with the runtime-only builtin error.
#[test]
fn test_declared_callable_gmp_literal_injects_its_prelude() {
    let out = compile_and_run(
        r#"<?php
function apply(callable $f): mixed { return $f(-3); }
function pick(): callable { return "gmp_neg"; }
function choose(int $n): callable { return $n > 0 ? "gmp_sqrt" : "gmp_sign"; }
class Box { public function __invoke(callable $fn) { return $fn(-7); } }
echo apply("gmp_abs"), " ", pick()(5), " ", choose(1)(16), " ", (new Box())("gmp_neg"), "\n";
"#,
    );
    assert_eq!(out, "3 -5 4 7\n");
    // Each named-target form alone, so none rides on another's prelude injection.
    for call in [
        r#""gmp_abs" |> apply(...)"#,
        r#"(apply(...))("gmp_abs")"#,
        r#"apply("gmp_" . "abs")"#,
        r#"apply($argc > 0 ? "gmp_abs" : "gmp_neg")"#,
    ] {
        let out = compile_and_run(&format!(
            "<?php\nfunction apply(callable $f): mixed {{ return $f(-3); }}\necho {call}, \"\\n\";\n"
        ));
        assert_eq!(out, "3\n", "{call}");
    }
    // A call through a closure variable names no target before types, so its string may be
    // ordinary text and injects nothing; the documented recovery is `gmp_abs(...)`.
    let out = compile_and_run(
        r#"<?php
$apply = function (callable $f) { return $f(-3); };
try { echo $apply("gmp_abs"), "\n"; } catch (Error $e) { echo get_class($e), "\n"; }
"#,
    );
    assert_eq!(out, "Error\n");
    let out = compile_and_run(
        r#"<?php
$apply = function (callable $f) { return $f(-3); };
echo $apply(gmp_abs(...)), "\n";
"#,
    );
    assert_eq!(out, "3\n");
}

/// Verifies an XML builtin whose lowering calls prelude helpers invokes when named as a literal
/// argument, and that the same names spread as ordinary text build without the XML prelude.
///
/// The literal used to build a wrapper calling an XML prelude helper that was never injected,
/// which failed compilation (`call to unknown function`), even when the name was only text.
#[test]
fn test_declared_callable_xml_literal_injects_its_prelude() {
    let out = compile_and_run(
        r#"<?php
function setH(callable $f, XMLParser $p): bool { return $f($p, null, null); }
$p = xml_parser_create();
var_dump(setH("xml_set_element_handler", $p));
"#,
    );
    assert_eq!(out, "bool(true)\n");
    let out = compile_and_run(
        r#"<?php
function text(callable $f, string $text): string { return $f($text); }
echo text(...["strrev", "xml_parse_into_struct"]), " ", text(...["strrev", "xml_set_element_handler"]), "\n";
"#,
    );
    assert_eq!(out, "tcurts_otni_esrap_lmx reldnah_tnemele_tes_lmx\n");
}

/// Verifies a literal builtin name passed as a named entry of a callable union variadic links the
/// libraries its wrapper needs.
#[test]
fn test_declared_callable_named_variadic_literal_links_its_library() {
    let out = compile_and_run(
        r#"<?php
function many(?callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f("word"); } return $out; }
echo many(foo: "iconv_strlen"), "\n";
"#,
    );
    assert_eq!(out, "4\n");
}

/// Verifies a literal builtin name kept by a callable union parameter or a `callable` return
/// links the libraries its wrapper needs, as `iconv_strlen(...)` does.
///
/// Recording only the name left `elephc_iconv` out of the link plan, so the program failed to
/// link unless something else used iconv.
#[test]
fn test_declared_callable_literal_builtin_names_link_their_libraries() {
    let out = compile_and_run(
        r#"<?php
function apply(?callable $f): mixed { return $f("word"); }
function pick(): callable { return "iconv_strlen"; }
echo apply("iconv_strlen"), " ", pick()("words"), "\n";
"#,
    );
    assert_eq!(out, "4 5\n");
    let out = compile_and_run(
        r#"<?php
function apply(callable $f): mixed { return $f("word"); }
echo apply($argc > 0 ? "iconv_strlen" : "strlen"), "\n";
"#,
    );
    assert_eq!(out, "4\n");
}

/// Verifies a value that is not callable raises PHP's binding `TypeError` when it reaches a
/// declared `callable`, including through a `mixed` forwarder, and that `?callable` admits null.
#[test]
fn test_declared_callable_rejects_non_callables_on_entry() {
    let out = compile_and_run(
        r#"<?php
function apply(callable $f): string { return "called"; }
function maybe(?callable $f): string { return $f === null ? "null" : "set"; }
function forward(mixed $value): string { return apply($value); }
foreach ([[1, 2], [1, "join"], "missing_function", 42] as $value) {
    try { echo apply($value), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { echo forward(null), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo maybe(null), " ", maybe("strlen"), "\n";
"#,
    );
    assert_eq!(
        out,
        "apply(): Argument #1 ($f) must be of type callable, array given\n\
         apply(): Argument #1 ($f) must be of type callable, array given\n\
         apply(): Argument #1 ($f) must be of type callable, string given\n\
         apply(): Argument #1 ($f) must be of type callable, int given\n\
         apply(): Argument #1 ($f) must be of type callable, null given\n\
         null set\n"
    );
}

/// Verifies `bindTo()` / `call()` on a declared `callable` act on a Closure only.
///
/// An invokable object runs its own `bindTo()` / `call()`, a string raises PHP's
/// `Call to a member function bindTo() on string`, and a Closure is still rebound. These
/// used to unbox any value as a Closure descriptor and rebind it.
#[test]
fn test_declared_callable_closure_methods_need_a_closure() {
    let out = compile_and_run(
        r#"<?php
class C { public function __invoke() {} public function bindTo(): string { return "user"; } public function call(): string { return "usercall"; } }
function bt(callable $c): mixed { return $c->bindTo(null); }
function cl(callable $c): mixed { return $c->call(new stdClass()); }
echo bt(new C()), " ", cl(new C()), "\n";
try { bt("strlen"); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$b = bt(function () { return isset($this) ? "this" : "none"; });
echo $b(), "\n";
"#,
    );
    assert_eq!(
        out,
        "user usercall\nError: Call to a member function bindTo() on string\nnone\n"
    );
}

/// Verifies by-reference and variadic `callable` parameters (including a by-reference variadic
/// `callable &...$xs`) check their values on entry, and that a union with a coercible scalar
/// (`int|callable`) still takes its members.
#[test]
fn test_declared_callable_reference_and_variadic_parameters_check_values() {
    let out = compile_and_run(
        r#"<?php
function r(callable &$c): void { echo "ok\n"; }
$x = $argc > 0 ? "nope" : [1];
try { r($x); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$y = $argc > 0 ? "strlen" : [1];
r($y);
function v(callable ...$xs): int { return count($xs); }
$nope = $argc > 0 ? "nope" : "strlen";
try { v("strlen", $nope); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$pair = $argc > 0 ? [1, 2] : [];
try { v($pair); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo v("strlen", fn() => 1), "\n";
function u(int|callable $v): string { return is_int($v) ? "int" : gettype($v); }
echo u(5), " ", u(fn() => 1), "\n";
function rv(callable &...$xs): void { echo "ran ", count($xs), "\n"; }
$a = $argc > 0 ? "nope" : "strlen";
try { rv($a); echo "after\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$b = "strlen"; $c = "strtoupper"; rv($b, $c);
"#,
    );
    assert_eq!(
        out,
        "r(): Argument #1 ($c) must be of type callable, string given\nok\n\
         v(): Argument #2 must be of type callable, string given\n\
         v(): Argument #1 must be of type callable, array given\n2\nint object\n\
         rv(): Argument #1 must be of type callable, string given\nran 2\n"
    );
}

/// Verifies a `callable` return type checks the returned value, that a nullable variadic
/// `?callable ...` admits null while refusing other non-callables, and that a scalar union
/// (`int|callable`) reached through a `mixed` forwarder still refuses a non-callable array.
///
/// These used to pass unchecked: a `callable` return accepted any string, the variadic entry
/// guard refused null for `?callable`, and the scalar union skipped the entry check.
#[test]
fn test_declared_callable_returns_nullable_variadics_and_scalar_unions_check_values() {
    let out = compile_and_run(
        r#"<?php
function bad(int $argc): callable { $n = $argc > 0 ? "missing_function" : "strlen"; return $n; }
try { bad($argc); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function good(): callable { return "strlen"; }
echo good()("abc"), "\n";
function take(?callable ...$fs): int { return count($fs); }
echo take(null, "strlen"), "\n";
function pick(int $n): mixed { return $n > 0 ? 5 : null; }
$bad = pick($argc);
try { take(null, $bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
function either(callable|false ...$fs): int { return count($fs); }
echo either(false, "strlen"), "\n";
function scalar(int|callable $v): string { return gettype($v); }
function forward(mixed $v): string { return scalar($v); }
echo forward(7), " ", forward("strlen"), "\n";
try { echo forward([]), "\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "bad(): Return value must be of type callable, string returned\n3\n2\n\
         take(): Argument #2 must be of type ?callable, int given\n2\ninteger string\n\
         scalar(): Argument #1 ($v) must be of type callable|int, array given\n"
    );
}

/// Verifies a native method reached from eval through a Closure descriptor resolves a callback
/// name that only eval declared when it binds to a declared `callable` parameter.
///
/// The method invoker built for an eval-created descriptor used to skip the eval-callable
/// conversion, so the eval-only function name was refused as not callable.
#[test]
fn test_declared_callable_from_eval_method_descriptor_sees_eval_functions() {
    let out = compile_and_run(
        r#"<?php
class Target { public static function run(callable $f): string { return $f(); } }
function invoke(Closure $c, string $name): string { return $c($name); }
eval('function evalOnlyCallback(): string { return "E"; } echo invoke(Closure::fromCallable(["Target", "run"]), "evalOnlyCallback"), "\n";');
"#,
    );
    assert_eq!(out, "E\n");
}

/// Verifies a call whose arguments are all fresh objects, and an immediately invoked closure,
/// keep the `TypeError` handler of a declared `callable` check, whose catch used to be pruned.
///
/// Exception-flow DCE let a fresh `new` argument waive every entry check, though it proves
/// nothing about `is_callable`, and modelled only `int` return checks, so a `callable` return
/// of a function given `new Box()` or of an immediate closure escaped its `try`.
#[test]
fn test_declared_callable_type_errors_survive_fresh_objects_and_immediate_closures() {
    let out = compile_and_run(
        r#"<?php
class Box {}
function bad(Box $o): callable { $name = "missing_function"; return $name; }
try { bad(new Box()); } catch (TypeError $e) { echo "A ", $e->getMessage(), "\n"; }
try {
    (function (): callable { $n = "missing_function"; return $n; })();
} catch (TypeError $e) { echo "B caught\n"; }
try { (fn(): callable => "missing_function")(); } catch (TypeError $e) { echo "B2 caught\n"; }
function take(callable $c): void {}
try { take(new Box()); } catch (TypeError $e) { echo "C ", $e->getMessage(), "\n"; }
class Holder { public static function take(callable ...$cs): void {} }
try { Holder::take(new Box()); } catch (TypeError $e) { echo "D ", $e->getMessage(), "\n"; }
echo "done\n";
"#,
    );
    assert_eq!(
        out,
        "A bad(): Return value must be of type callable, string returned\nB caught\nB2 caught\n\
         C take(): Argument #1 ($c) must be of type callable, Box given\n\
         D Holder::take(): Argument #1 must be of type callable, Box given\ndone\n"
    );
}

/// Verifies a native method reached from eval through a Closure descriptor resolves an
/// eval-only callback name bound to `int|callable`, `?callable` and `callable ...`, keeps an int
/// for `int|callable`, and raises PHP's `TypeError` for a value no member accepts.
///
/// Only an exact `callable` parameter used to convert eval-only callbacks; the union, nullable
/// and variadic forms rejected them because the native `is_callable` cannot see eval functions.
#[test]
fn test_declared_callable_unions_and_variadics_from_eval_see_eval_functions() {
    let out = compile_and_run(
        r#"<?php
class Target {
    public static function run(int|callable $f): string { return is_int($f) ? "int" : $f(); }
    public static function maybe(?callable $f): string { return $f === null ? "null" : $f(); }
    public static function many(callable ...$fs): string { $out = ""; foreach ($fs as $f) { $out .= $f(); } return $out; }
}
function invoke(Closure $c, mixed $arg): string { return $c($arg); }
eval('function evalOnlyCallback(): string { return "E"; }
echo invoke(Closure::fromCallable(["Target", "run"]), "evalOnlyCallback"), " ",
    invoke(Closure::fromCallable(["Target", "run"]), 7), " ",
    invoke(Closure::fromCallable(["Target", "maybe"]), "evalOnlyCallback"), " ",
    invoke(Closure::fromCallable(["Target", "maybe"]), null), " ",
    invoke(Closure::fromCallable(["Target", "many"]), "evalOnlyCallback"), "\n";
try { invoke(Closure::fromCallable(["Target", "run"]), "nope"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }');
"#,
    );
    assert_eq!(
        out,
        "E int E null E\nTarget::run(): Argument #1 ($f) must be of type callable|int, string given\n"
    );
}

/// Verifies a generator raises its declared `callable` `TypeError` at the call (before the
/// Generator exists), checks a by-reference `callable` only once at the call, and that named
/// arguments to a `callable ...$xs` variadic report PHP's positional argument numbers.
///
/// Generators used to check callable parameters only when the body first ran, so the call's
/// `catch` never saw the error and a later change to a by-reference argument was rechecked;
/// named arguments promoted the variadic to an iterable container that skipped the check, and
/// a named entry used to be reported by its name instead of its argument number.
#[test]
fn test_declared_callable_generators_and_named_variadics_check_at_the_call() {
    let out = compile_and_run(
        r#"<?php
function g(callable &...$xs): Generator { echo "body\n"; yield 1; }
function g3(callable $c): Generator { echo "body3\n"; yield 1; }
function g2(callable &$c): Generator { echo "body2\n"; yield 1; }
$a = $argc > 0 ? "nope" : "strlen";
try { $gen = g($a); echo "created\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $gen = g3($a); echo "created3\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$b = "strlen";
$gen2 = g2($b);
$b = "nope";
foreach ($gen2 as $_) {}
echo "done\n";
function v(callable ...$xs): string { return "ran"; }
$nope = $argc > 0 ? "nope" : "strlen";
try { v(foo: $nope); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { v(foo: "strlen", bar: $nope); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { v("strlen", "strrev", z: $nope); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "g(): Argument #1 must be of type callable, string given\n\
         g3(): Argument #1 ($c) must be of type callable, string given\n\
         body2\n\
         done\n\
         v(): Argument #1 must be of type callable, string given\n\
         v(): Argument #1 must be of type callable, string given\n\
         v(): Argument #3 must be of type callable, string given\n"
    );
}

/// Verifies a `callable` passed from eval into an AOT constructor, an `__invoke` method and an
/// ordinary method keeps its PHP value, and that a builtin named only inside the evaluated code
/// (`sort`, by reference) or a function declared there still invokes.
///
/// A constructor (and a method reached without a bridge scope) used to run without the eval
/// frame, so invoking a builtin the compiled program never named was a fatal error; the eval
/// bridges also replaced the value with a Closure, which `var_dump` then showed.
#[test]
fn test_declared_callable_values_from_eval_keep_their_value_and_invoke() {
    let out = compile_and_run(
        r#"<?php
class T {
    public function __construct(callable &$callback) {
        $items = [3, "1", 2];
        $ok = $callback($items);
        echo ($ok ? "T" : "F"), ":", implode(",", $items), ":", $callback, "\n";
    }
}
class I {
    public function __invoke(callable &$callback): string { $items = [2, 1]; $callback($items); return implode(",", $items); }
}
class S {
    public function show(callable $c): void { var_dump($c); }
}
$callback = "sort";
eval('new T($callback);');
eval('$i = new I(); echo $i($callback), "\n";');
echo $callback, "\n";
eval('(new S())->show("strlen");');
eval('function evalShown(): int { return 1; } (new S())->show("evalShown");');
"#,
    );
    assert_eq!(
        out,
        "T:1,2,3:sort\n\
         1,2\n\
         sort\n\
         string(6) \"strlen\"\n\
         string(9) \"evalShown\"\n"
    );
}

/// Verifies a by-reference `callable` return (`function &f(): callable`) checks the value read
/// through the cell it hands the caller, from a local and from a property, like a by-value
/// return: a callable passes and stays invokable, anything else throws PHP's `TypeError`.
///
/// The reference return path skipped the declared `callable` check, so a non-callable string
/// reached the caller.
#[test]
fn test_declared_callable_reference_returns_check_the_value() {
    let out = compile_and_run(
        r#"<?php
class Holder { public $cb = "backwards"; }
function size(string $s): int { return strlen($s); }
function backwards(string $s): string { return strrev($s); }
function &local(mixed &$v): callable { return $v; }
function &prop(Holder $h): callable { return $h->cb; }
$ok = "size";
$r = &local($ok);
echo $r("abc"), "\n";
$h = new Holder();
$p = &prop($h);
echo $p("abc"), "\n";
$bad = "nope";
try { local($bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$h->cb = 5;
try { prop($h); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "3\ncba\nlocal(): Return value must be of type callable, string returned\n\
prop(): Return value must be of type callable, int returned\n"
    );
}

/// A failing declared `callable` return check must release an owning temporary it was handed.
///
/// The property read (`return $h->cb`) owns one reference; the check copied it and threw, so a
/// caught `TypeError` left every previous heap string stored in `$h->cb` alive, by value and by
/// reference.
#[test]
fn test_declared_callable_failed_return_check_releases_the_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Holder { public $cb; }
function &byref(Holder $h): callable { return $h->cb; }
function byval(Holder $h): callable { return $h->cb; }
$h = new Holder();
$caught = 0;
for ($i = 0; $i < 3; $i++) {
    $h->cb = str_repeat("x", 64) . $i;
    try { $r = &byref($h); } catch (TypeError $e) { $caught++; }
    try { byval($h); } catch (TypeError $e) { $caught++; }
}
$h->cb = null;
echo $caught, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "6\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
