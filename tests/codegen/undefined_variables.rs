//! Purpose:
//! Integration tests for PHP's undefined-variable semantics: reading a variable that was never
//! assigned WARNS and answers `null`, and the program keeps running.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc used to REFUSE these programs at compile time, and — when a call site happened to
//!   resolve the enclosing function's signature early — miscompile them into a read of
//!   uninitialized stack memory that segfaulted. php-src `Zend/zend_execute.c:280`
//!   (`zval_undefined_cv`) raises `E_WARNING` and returns `&EG(uninitialized_zval)`: the read
//!   IS null and execution continues. Every expectation below was measured on `php -n` 8.5.6.
//! - The warning travels on the DIAGNOSTIC stream, which is php's stdout, so these assert
//!   `out.diagnostics` rather than `out.stderr`.

use crate::support::*;

/// Verifies a bare read of a never-assigned variable warns and yields null instead of refusing.
///
/// MEASURED on `php -n` 8.5.6 — `<?php echo $x;` prints the warning and nothing else, exit 0.
/// elephc refused the program with `error[1:12]: Undefined variable: $x`.
#[test]
fn test_undefined_variable_read_warns_and_yields_null() {
    let out = compile_and_run_capture("<?php echo $x;\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $x\n");
}

/// Verifies the read yields null in a VALUE position rather than crashing.
///
/// This is the three-line repro of the segfault: the enclosing function's signature was resolved
/// early by the call site, which silently dropped the body's undefined-variable error, so lowering
/// gave `$nope` a `Mixed` slot no store ever wrote and the concatenation dereferenced whatever the
/// stack held. `php -n` 8.5.6 prints the warning and then `a`.
#[test]
fn test_undefined_variable_in_a_concat_inside_a_function_does_not_crash() {
    let out = compile_and_run_capture(
        r#"<?php
function f() { return "a" . $nope; }
echo f(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "a\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $nope\n");
}

/// Verifies an uncalled function is treated exactly like a called one.
///
/// The two used to disagree: a function reached by a direct call had its body error dropped and
/// compiled, while the same body left uncalled was REFUSED — an artifact of which pass resolved
/// the signature, not of any rule. Two identical bodies in one file, one called and one not, is
/// what pins that they now agree.
#[test]
fn test_called_and_uncalled_functions_agree_on_an_undefined_read() {
    let out = compile_and_run_capture(
        r#"<?php
function called() { return "c" . $missing_a; }
function uncalled() { return "u" . $missing_b; }
echo called(), "\n";
echo "end\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "c\nend\n");
    // Only the body that RAN warns: the warning is raised by the read, not by compiling it.
    assert_eq!(out.diagnostics, "Warning: Undefined variable $missing_a\n");
}

/// Verifies arithmetic on an undefined variable treats it as null, which php coerces to 0.
///
/// MEASURED: `var_dump($nope + 1)` warns then prints `int(1)`.
#[test]
fn test_undefined_variable_in_arithmetic_is_null() {
    let out = compile_and_run_capture("<?php var_dump($nope + 1);\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(1)\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $nope\n");
}

/// Verifies the self-reading compound assignment php allows: `$x = $x + 1` warns, then `$x` is 1.
#[test]
fn test_self_read_assignment_warns_once_and_starts_from_null() {
    let out = compile_and_run_capture(
        r#"<?php
$x = $x + 1;
var_dump($x);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(1)\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $x\n");
}

/// Verifies `@` suppresses the warning, as it does every other suppressible diagnostic.
///
/// MEASURED: `<?php echo @$nope; echo "end\n";` prints `end` and NOTHING else.
#[test]
fn test_suppression_operator_silences_the_undefined_read() {
    let out = compile_and_run_capture(
        r#"<?php
echo @$nope;
echo "end\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "end\n");
    assert_eq!(out.diagnostics, "");
}

/// Verifies a read that PRECEDES the only assignment still warns, and the later read does not.
///
/// This is the flow-sensitive half of the contract, and the reason the warning is decided from
/// the lowering's definitely-initialized set rather than from a set of source spans: the same
/// name is undefined at line 2 and defined at line 4.
#[test]
fn test_a_read_before_the_assignment_warns_and_the_one_after_does_not() {
    let out = compile_and_run_capture(
        r#"<?php
echo "[", $later, "]\n";
$later = "set";
echo "[", $later, "]\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[]\n[set]\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $later\n");
}

/// Verifies an assignment that only happens on a branch never taken leaves the variable undefined.
///
/// MEASURED on `php -n` 8.5.6: the warning names the ECHO's line, not the assignment's, because
/// the branch did not run. A merge that treated a conditional assignment as definite would print
/// nothing and swallow it.
///
/// The condition is a VARIABLE rather than the literal `false` on purpose. A literal is folded
/// away, and the store then never reaches the lowering at all — which would pass this test for
/// the wrong reason, by having nothing to merge rather than by merging correctly.
///
/// The shape PHP's own manual would use here, `false && $x = 1;`, is not written because elephc's
/// parser refuses a bare boolean expression at statement position — a separate gap, recorded
/// rather than worked around silently.
#[test]
fn test_an_untaken_branch_assignment_does_not_define_the_variable() {
    let out = compile_and_run_capture(
        r#"<?php
$c = false;
if ($c) { $x = 1; }
echo "[", $x, "]\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[]\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $x\n");
}

/// Verifies a parameter, a `use()` capture and a `foreach` binding are NOT treated as undefined.
///
/// The warning is driven by the lowering's definitely-initialized set, so every name a frame
/// receives rather than stores has to be marked initialized by its own setup. Getting that wrong
/// replaces the crash with a flood of warnings on correct programs, which is why the quiet case
/// is pinned as hard as the noisy one.
#[test]
fn test_parameters_captures_and_loop_bindings_are_not_undefined() {
    let out = compile_and_run_capture(
        r#"<?php
function takes(string $p): string { return $p; }
$captured = "c";
$closure = function () use ($captured): string { return $captured; };
$total = "";
foreach (["x", "y"] as $index => $value) { $total .= $index . $value; }
echo takes("p"), "|", $closure(), "|", $total, "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "p|c|0x1y\n");
    assert_eq!(out.diagnostics, "");
}

/// Verifies the warning names its source line, like every other php diagnostic.
///
/// The location comes from the per-instruction publisher, so a diagnostic raised from a lowering
/// that forgets to publish one would still print a well-formed line — with the WRONG number, or
/// none. Only an assertion on the located form catches that.
#[test]
fn test_the_undefined_read_warning_names_its_line() {
    let out = compile_and_run_capture(
        r#"<?php
echo "first\n";
echo $absent;
echo "last\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "first\nlast\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Undefined variable $absent in test.php on line 3\n"
    );
}

/// Verifies only the SPINE of a null probe is tolerated: its index subexpression is not.
///
/// `isset()`, `empty()`, `unset()` and `??` exist to name storage that may never have been
/// declared, so the chain root raises nothing. An index is an ordinary expression that happens to
/// sit inside one, and PHP reads it — MEASURED on `php -n` 8.5.6, which warns about `$b` and then
/// prints `bool(false)`.
///
/// The receiver is DEFINED here on purpose. With an undefined receiver the index is not evaluated
/// at all, because the nullable-receiver probe short-circuits before reaching it — a divergence
/// from PHP that this change made reachable rather than caused, and one that belongs to the
/// probe lowering rather than to undefined variables. Testing the property through a defined
/// receiver keeps this test about the property.
///
/// `contains` and not an equality: for a null offset PHP ALSO raises `Using null as an array
/// offset is deprecated`, which elephc does not implement. Asserting the exact set would tie this
/// test to that second, unrelated gap.
#[test]
fn test_a_null_probe_tolerates_its_spine_but_not_its_index() {
    let out = compile_and_run_capture(
        r#"<?php
$a = ["k" => 1];
var_dump(isset($a[$b]));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
    assert!(
        out.diagnostics.contains("Warning: Undefined variable $b"),
        "an index inside a probe is an ordinary read and must warn, got {:?}",
        out.diagnostics
    );
}

/// Verifies the three null probes stay SILENT for a plain undefined variable.
///
/// `isset()`, `empty()` and `??` exist to ask whether storage was ever named, so PHP answers
/// without raising anything — MEASURED on `php -n` 8.5.6, which prints the three results and no
/// diagnostic at all. Making an undefined read warn turned every one of them into a warning
/// while still answering correctly, so the results alone would not have caught it: the empty
/// diagnostic stream is the assertion that matters here.
#[test]
fn test_the_null_probes_say_nothing_about_an_undefined_variable() {
    let out = compile_and_run_capture("<?php var_dump(isset($x), empty($x), $x ?? \"fallback\");\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(true)\nstring(8) \"fallback\"\n");
    assert_eq!(out.diagnostics, "");
}

/// Verifies only the PROBED side of `??` is silent: its default expression is an ordinary read.
///
/// `$x ?? $y` warns about `$y` and not `$x` — MEASURED on `php -n` 8.5.6. The two sides of one
/// operator land on opposite sides of the rule, which is what makes this the tightest witness
/// that the silence is scoped to the probe rather than applied to the whole expression.
#[test]
fn test_the_default_side_of_a_coalesce_is_an_ordinary_read() {
    let out = compile_and_run_capture("<?php var_dump($x ?? $y);\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $y\n");
}

/// Verifies an `eval()` closes the question for reads that come AFTER it, and only those.
///
/// The fragment can bind any name, so PHP's own answer for `eval('$c = 1;'); echo $c;` is `1`
/// with no diagnostic. A read written BEFORE the `eval()` is still undefined at that point and
/// PHP warns about it — MEASURED on `php -n` 8.5.6 for this exact program, which prints one
/// warning and then `1`.
///
/// The flags are set as lowering REACHES the `eval()`, which is what makes position decide.
/// Both are consulted: the fragment here compiles through the barrier-free AOT path, which
/// records only `eval_executed`, and gating on the dynamic barrier alone left this program
/// printing the warning twice and never the value.
#[test]
fn test_an_eval_defines_names_for_the_reads_after_it_only() {
    let out = compile_and_run_capture("<?php echo $created; eval('$created = 1;'); echo $created;\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $created\n");
}

/// Verifies every kind of function BODY reaches the undefined-variable warning.
///
/// Each of these body kinds was once its own hole: enum method bodies skipped name checking
/// entirely, and the registry-first builtin dispatch skipped argument inference. Those holes
/// were pinned by tests that expected a compile-time REFUSAL, which is no longer what elephc
/// does — so the property they guarded moves here, where it is stated as PHP states it: the
/// read warns and answers null, whatever body it sits in. All four measured on `php -n` 8.5.6.
#[test]
fn test_every_body_kind_reaches_the_undefined_read() {
    for (source, expected) in [
        (
            "<?php enum E { case A; public function f() { return $missing; } } var_dump(E::A->f());",
            "$missing",
        ),
        (
            "<?php class C { public static function f() { return $nope; } } var_dump(C::f());",
            "$nope",
        ),
        (
            "<?php trait T { public function f() { return $nope; } } class C { use T; } var_dump((new C)->f());",
            "$nope",
        ),
        ("<?php $f = function () { return $nope; }; var_dump($f());", "$nope"),
    ] {
        let out = compile_and_run_capture(source);
        assert!(out.success, "program failed: {} ({source})", out.stderr);
        assert_eq!(out.stdout, "NULL\n", "for {source}");
        assert_eq!(
            out.diagnostics,
            format!("Warning: Undefined variable {expected}\n"),
            "for {source}",
        );
    }
}

/// Verifies a method cannot see an ordinary top-level local, and says so PHP's way.
///
/// PHP's scope boundary is absolute: a method body reaches a top-level local only through
/// `global`. The boundary used to be observable as a checker error and is now observable as the
/// warning — MEASURED on `php -n` 8.5.6, which prints it and then `NULL`. Losing the diagnostic
/// entirely would mean the method had silently resolved `$value` to the outer 5.
#[test]
fn test_a_method_does_not_see_a_top_level_local() {
    let out = compile_and_run_capture(
        "<?php $value = 5; class C { public function f() { return $value; } } var_dump((new C)->f());\n",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $value\n");
}

/// Verifies an undefined read passed to a registry builtin warns and reaches it as null.
///
/// `ord($undeclared)` answers `int(0)` after the warning — MEASURED on `php -n` 8.5.6. The
/// registry-first dispatch used to skip inference for builtins with no check hook, which
/// dropped the diagnostic altogether; the pin for that lived in the error tests until the
/// diagnostic stopped being an error.
///
/// `contains` and not an equality: PHP also raises two `ord(): Passing null ...` deprecations
/// that elephc does not implement, and asserting the exact set would tie this test to that
/// separate gap.
#[test]
fn test_an_undefined_read_reaches_a_builtin_argument_as_null() {
    let out = compile_and_run_capture("<?php var_dump(ord($undeclared));\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "int(0)\n");
    assert!(
        out.diagnostics.contains("Warning: Undefined variable $undeclared"),
        "a builtin argument is an ordinary read and must warn, got {:?}",
        out.diagnostics
    );
}

/// Verifies capturing a name that was never assigned warns WHERE THE CLOSURE IS CREATED.
///
/// PHP evaluates a `use` list at closure creation, so the warning lands there and the capture
/// holds null — measured on `php -n` 8.5.6, including for a closure that is never called. A
/// by-REFERENCE capture is the opposite case in the same syntax: binding a name creates it, so
/// PHP raises nothing at all and the outer variable is NULL afterwards.
///
/// elephc refused all of these with `Undefined variable in use(): $x`, which also cascaded into
/// a bogus second error (`Cannot call $f — not a callable`).
#[test]
fn test_a_capture_of_an_undefined_name_warns_at_creation() {
    let by_value = compile_and_run_capture(
        "<?php $f = function () use ($x) { return $x; }; echo \"created\\n\"; var_dump($f());\n",
    );
    assert!(by_value.success, "program failed: {}", by_value.stderr);
    assert_eq!(by_value.stdout, "created\nNULL\n");
    assert_eq!(by_value.diagnostics, "Warning: Undefined variable $x\n");

    let never_called = compile_and_run_capture(
        "<?php $f = function () use ($x) { return $x; }; echo \"ok\\n\";\n",
    );
    assert!(never_called.success, "program failed: {}", never_called.stderr);
    assert_eq!(never_called.stdout, "ok\n");
    assert_eq!(never_called.diagnostics, "Warning: Undefined variable $x\n");

    let by_ref = compile_and_run_capture(
        "<?php $f = function () use (&$x) { return $x; }; var_dump($f()); var_dump($x);\n",
    );
    assert!(by_ref.success, "program failed: {}", by_ref.stderr);
    assert_eq!(by_ref.stdout, "NULL\nNULL\n");
    assert_eq!(by_ref.diagnostics, "");
}

/// Verifies an arrow function's implicit capture follows the same rule as an explicit one.
///
/// `fn () => $nope` captures `$nope` by value, so PHP warns once at creation and the body sees
/// null — MEASURED on `php -n` 8.5.6, where the concatenating form prints `string(1) "a"`.
#[test]
fn test_an_arrow_function_captures_an_undefined_name_as_null() {
    let out = compile_and_run_capture("<?php $f = fn () => \"a\" . $nope; var_dump($f());\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "string(1) \"a\"\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $nope\n");
}

/// Verifies a closure returning an undefined read answers null, not the raw null sentinel.
///
/// The single-statement `return $x;` shape has its own return-type inference in EIR lowering,
/// which resolves the name against the closure's captures and parameters and otherwise falls
/// back to a SYNTACTIC default that answers `int` for any variable. A name that is neither
/// captured nor a parameter has no other way to hold anything in PHP, so that default stamped
/// the closure `-> I64` and `var_dump($f())` printed `int(9223372036854775806)` — the in-band
/// null sentinel read back as an integer. PHP prints `NULL`.
#[test]
fn test_a_closure_returning_an_undefined_read_answers_null() {
    let out = compile_and_run_capture("<?php $f = function () { return $nope; }; var_dump($f());\n");
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(out.diagnostics, "Warning: Undefined variable $nope\n");
}

/// Verifies issue #382's shapes: a `use` list is evaluated BEFORE the assignment it sits in.
///
/// `$f = function () use ($f) { ... };` captures the `$f` that exists when the closure is
/// built, which is none — PHP warns and captures null, then assigns the closure to `$f`
/// afterwards. The by-reference spelling of a DIFFERENT name creates that name silently, and
/// `$h` is NULL in the outer scope after it. Both measured on `php -n` 8.5.6.
///
/// These were three error tests pinning `Undefined variable in use()`. The trap they guard is
/// real and stays pinned here; what changed is that PHP does not refuse it.
#[test]
fn test_a_use_list_is_evaluated_before_its_own_assignment() {
    let self_capture =
        compile_and_run_capture("<?php $f = function() use($f) { return $f; }; var_dump($f());\n");
    assert!(self_capture.success, "program failed: {}", self_capture.stderr);
    assert_eq!(self_capture.stdout, "NULL\n");
    assert_eq!(self_capture.diagnostics, "Warning: Undefined variable $f\n");

    let by_ref_other = compile_and_run_capture(
        "<?php $g = function() use(&$h) { return $h; }; var_dump($g()); var_dump($h);\n",
    );
    assert!(by_ref_other.success, "program failed: {}", by_ref_other.stderr);
    assert_eq!(by_ref_other.stdout, "NULL\nNULL\n");
    assert_eq!(by_ref_other.diagnostics, "");

    let echoed = compile_and_run_capture(
        "<?php $fn = function() use ($undefined) { echo $undefined; }; $fn(); echo \"done\\n\";\n",
    );
    assert!(echoed.success, "program failed: {}", echoed.stderr);
    assert_eq!(echoed.stdout, "done\n");
    assert_eq!(echoed.diagnostics, "Warning: Undefined variable $undefined\n");
}

/// Verifies an undefined variable used as an ARRAY ELEMENT yields a null element.
///
/// The literal's storage element type is chosen syntactically, and an unrecognised expression
/// fell back to `Int`. An undefined read is neither: it answers null, so the literal was stamped
/// `array<int>` and the whole program was REFUSED with `unsupported EIR backend feature:
/// array_push for PHP type Void`. `[null]` and `[$x]` where `$x = null` both compiled, which is
/// what made the refusal look like a null problem rather than an undefined-name one.
#[test]
fn test_an_undefined_variable_as_an_array_element_is_a_null_element() {
    let out = compile_and_run_capture(
        r#"<?php
$d = [$undefinedVar];
var_dump($d);
$e = [1, $alsoMissing, "s"];
var_dump(count($e), $e[1]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(1) {\n  [0]=>\n  NULL\n}\nint(3)\nNULL\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: Undefined variable $undefinedVar\nWarning: Undefined variable $alsoMissing\n"
    );
}

/// Verifies a read inside a multi-line interpolated string names the line it is WRITTEN on.
///
/// Every token an interpolated string produced used to carry the span of the string's first
/// character, because `interpolate` built each one from the literal's own span. That is harmless
/// for a one-line double-quoted string and wrong for anything spanning several: a read on the
/// fourth line of a heredoc reported the line of the `<<<LABEL` that opened it, and the same for
/// a double-quoted literal containing real newlines. Measured on `php -n` 8.5.6, which names
/// lines 4 and 9 here.
///
/// The defect belongs to the LEXER, so it reaches every diagnostic an interpolation can raise —
/// this one, an array-to-string conversion, a NaN coercion. The undefined read is the shape used
/// to pin it because it is the cheapest one to trigger.
#[test]
fn test_an_undefined_read_inside_a_multi_line_string_names_its_own_line() {
    let out = compile_and_run_capture(
        r#"<?php
$s = <<<EOT
first line
{$missing}
third line
EOT;
echo strlen($s), "\n";
$t = "one
two {$absent}";
echo strlen($t), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "22\n8\n");
    assert_eq!(
        out.located_diagnostics,
        "Warning: Undefined variable $missing in test.php on line 4\n\
         Warning: Undefined variable $absent in test.php on line 9\n"
    );
}
